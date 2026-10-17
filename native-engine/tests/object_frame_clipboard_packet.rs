//! The clipboard packet against the verbatim dense baseline editor (fbe4b8a3).
//!
//! Every oracle row is one run of the baseline copyFrames and pasteFrames (src/js/timeline.js, in a
//! Node vm) over 1-3 dense layers: the selection order, the relative offsets, the copied cell, the
//! cell pasteFrames leaves at the target, and the baseline's own classification of each selected
//! cell (heldKeyframeAt from tests/fixtures/lib/reference.cjs). Each row is rebuilt as a sparse
//! native document (keys, samples, authored records), copied through the real kernel from a real
//! ObjectSnapshot, and the packet is rendered back to the same tokens. Mutation controls live in
//! object_frame_clipboard_packet_controls.rs, hand-typed acceptance cases in ..._literals.rs.
use super::rendering::{expected, render, Mutant};
use super::{
    copy_frame_cells, ClipboardPacketError, FrameClipboardPacket, PacketLimits, PacketPins,
    SelectedCell,
};
use crate::object_codec::decode_storage_project;
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};

pub(crate) const ORACLE: &str = include_str!("fixtures/object-frame-clipboard-packet/oracle.txt");

pub(crate) struct Row {
    pub(crate) id: usize,
    pub(crate) total: u32,
    pub(crate) layers: Vec<Vec<String>>,
    pub(crate) selection: Vec<(usize, u32)>,
    pub(crate) clip: Vec<(usize, u32, String)>,
    pub(crate) pasted: Vec<String>,
    pub(crate) classes: Vec<String>,
    pub(crate) bounds: (usize, u32, usize, u32),
}

fn pair(text: &str) -> (usize, u32) {
    let (a, b) = text.split_once(':').unwrap();
    (a.parse().unwrap(), b.parse().unwrap())
}

fn parse_row(line: &str) -> Row {
    let part: Vec<&str> = line.split(' ').collect();
    assert_eq!(part.len(), 10, "{line}");
    let bounds: Vec<&str> = part[9].split(':').collect();
    Row {
        id: part[1].parse().unwrap(),
        total: part[2].parse().unwrap(),
        layers: part[3]
            .split('/')
            .map(|l| l.split(',').map(str::to_owned).collect())
            .collect(),
        selection: part[4].split(',').map(pair).collect(),
        clip: part[5]
            .split(',')
            .map(|entry| {
                let mut it = entry.splitn(3, ':');
                let rl = it.next().unwrap().parse().unwrap();
                let rf = it.next().unwrap().parse().unwrap();
                (rl, rf, it.next().unwrap().to_owned())
            })
            .collect(),
        pasted: part[7].split(',').map(str::to_owned).collect(),
        classes: part[8].split(',').map(str::to_owned).collect(),
        bounds: (
            bounds[0].parse().unwrap(),
            bounds[1].parse().unwrap(),
            bounds[2].parse().unwrap(),
            bounds[3].parse().unwrap(),
        ),
    }
}

pub(crate) fn rows() -> Vec<Row> {
    ORACLE
        .lines()
        .filter(|line| line.starts_with("row "))
        .map(parse_row)
        .collect()
}

// ---- sparse native document built from the dense tokens

/// Switches that bend the document builder; PLAIN is the faithful dense-to-sparse conversion.
#[derive(Clone, Copy)]
pub(crate) struct Build {
    pub(crate) frame_major: bool,
    pub(crate) with_reference: bool,
    pub(crate) drop_samples: bool,
    pub(crate) drop_manual: bool,
    pub(crate) all_manual: bool,
}
impl Build {
    pub(crate) const PLAIN: Build = Build {
        frame_major: false,
        with_reference: false,
        drop_samples: false,
        drop_manual: false,
        all_manual: false,
    };
}

pub(crate) fn uid(layer: usize) -> String {
    format!("L{layer}")
}

pub(crate) fn record_in(scope: &str, layer: &str, id: &str, frame: u32, x: i64) -> Value {
    json!({"schemaVersion":1,"family":"closed-solid-fill-cubic-path",
        "target":{"contextId":"scene-root","frameScope":{"kind":scope,"frame":frame},"layerUid":layer,"strokeId":id},
        "geometry":{"kind":"cubic-path","closed":true,"coordinateSpace":"document-world","handleSpace":"relative",
            "segments":[
                {"point":{"x":x,"y":20},"handleIn":{"x":-2,"y":0},"handleOut":{"x":3,"y":-1}},
                {"point":{"x":40,"y":20},"handleIn":{"x":-3,"y":2},"handleOut":{"x":0,"y":6}},
                {"point":{"x":25,"y":50},"handleIn":{"x":4,"y":-2},"handleOut":{"x":-5,"y":-3}}]},
        "fill":{"kind":"solid","r":0.25,"g":0.5,"b":0.75,"a":1}})
}

pub(crate) fn record(layer: &str, id: &str, frame: u32, x: i64) -> Value {
    record_in("authored", layer, id, frame, x)
}

pub(crate) fn timing_json(keys: &[u32], samples: Value) -> Value {
    json!({"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing","contextId":"scene-root",
        "keys":keys.iter().map(|frame| json!({"frame":frame})).collect::<Vec<_>>(),"samples":samples})
}

fn stroke_count(token: &str) -> usize {
    token[1..].parse().unwrap_or(0)
}

fn layer_timing(tokens: &[String], build: &Build) -> Value {
    let mut keys = Vec::new();
    let mut samples = Vec::new();
    for (frame, token) in tokens.iter().enumerate() {
        let frame = frame as u32;
        match token.as_bytes()[0] {
            b'K' | b'B' => keys.push(frame),
            b'T' | b'M' if !build.drop_samples => {
                let manual = (token.starts_with('M') && !build.drop_manual)
                    || (token.starts_with('T') && build.all_manual);
                samples.push(if manual {
                    json!({"frame":frame,"manualEdit":true})
                } else if frame % 2 == 1 {
                    json!({"frame":frame,"manualEdit":false})
                } else {
                    json!({"frame":frame})
                });
            }
            _ => {}
        }
    }
    timing_json(&keys, Value::Array(samples))
}

pub(crate) fn document(row: &Row, build: &Build) -> Value {
    let mut objects: Vec<(u32, usize, Value)> = Vec::new();
    for (layer, tokens) in row.layers.iter().enumerate() {
        for (frame, token) in tokens.iter().enumerate() {
            let frame = frame as u32;
            if !matches!(token.as_bytes()[0], b'H') {
                for i in 0..stroke_count(token) {
                    let id = format!("s{layer}_{frame}_{i}");
                    let x = (layer * 100) as i64 + i64::from(frame) * 10 + i as i64;
                    objects.push((frame, layer, record(&uid(layer), &id, frame, x)));
                }
                if build.with_reference {
                    let id = format!("ref{layer}_{frame}");
                    objects.push((
                        frame,
                        layer,
                        record_in("reference", &uid(layer), &id, frame, 7),
                    ));
                }
            }
        }
    }
    if build.frame_major {
        objects.sort_by_key(|entry| (entry.0, entry.1));
    }
    let layers: Vec<Value> = row
        .layers
        .iter()
        .enumerate()
        .map(|(layer, tokens)| json!({"layerUid":uid(layer),"timing":layer_timing(tokens, build)}))
        .collect();
    json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":row.total,
        "layers":layers,"objects":objects.into_iter().map(|entry| entry.2).collect::<Vec<_>>()})
}

pub(crate) fn snapshot(value: &Value) -> ObjectSnapshot {
    ObjectSnapshot::new(
        "clip",
        decode_storage_project(value.to_string().as_bytes()).unwrap(),
    )
    .unwrap()
}

pub(crate) fn pins(snapshot: &ObjectSnapshot) -> PacketPins<'_> {
    PacketPins {
        instance_id: snapshot.instance_id(),
        document_id: snapshot.document_id(),
        content_revision: snapshot.content_revision(),
        document_snapshot_id: snapshot.snapshot_id(),
    }
}

pub(crate) fn copy_row(
    snapshot: &ObjectSnapshot,
    row: &Row,
) -> Result<FrameClipboardPacket, ClipboardPacketError> {
    let uids: Vec<String> = row.selection.iter().map(|&(layer, _)| uid(layer)).collect();
    let selection: Vec<SelectedCell<'_>> = row
        .selection
        .iter()
        .zip(&uids)
        .map(|(&(_, frame), layer_uid)| SelectedCell { layer_uid, frame })
        .collect();
    copy_frame_cells(
        snapshot,
        &pins(snapshot),
        &selection,
        &PacketLimits::DEFAULT,
    )
}

// ---- tests

#[test]
fn every_oracle_row_matches_the_packet() {
    let all = rows();
    assert_eq!(all.len(), 425);
    for row in &all {
        let snapshot = snapshot(&document(row, &Build::PLAIN));
        let packet = copy_row(&snapshot, row).unwrap_or_else(|e| panic!("row {}: {e:?}", row.id));
        assert_eq!(
            render(&packet, Mutant::None),
            expected(row),
            "row {}",
            row.id
        );
        assert_eq!(packet.cells().len(), row.selection.len(), "row {}", row.id);
        let (min_layer, min_frame, extent_layers, extent_frames) = row.bounds;
        assert_eq!(packet.origin(), (min_layer, min_frame), "row {}", row.id);
        assert_eq!(
            packet.extent(),
            (extent_layers, extent_frames),
            "row {}",
            row.id
        );
        assert_eq!(
            (
                packet.instance_id(),
                packet.document_id(),
                packet.content_revision()
            ),
            (
                snapshot.instance_id(),
                snapshot.document_id(),
                snapshot.content_revision()
            ),
            "row {}",
            row.id
        );
        assert_eq!(packet.document_snapshot_id(), snapshot.snapshot_id());
        for (cell, &(layer, frame)) in packet.cells().iter().zip(&row.selection) {
            assert_eq!(
                (cell.layer_uid(), cell.frame()),
                (uid(layer).as_str(), frame)
            );
        }
    }
}

#[test]
fn copied_records_equal_the_snapshot_records_and_share_nothing_with_them() {
    for row in &rows() {
        let snapshot = snapshot(&document(row, &Build::PLAIN));
        let packet = copy_row(&snapshot, row).unwrap();
        let stored = snapshot.document().objects();
        let mut total_records = 0usize;
        for cell in packet.cells() {
            for copied in cell.records() {
                let at = stored
                    .iter()
                    .position(|candidate| candidate.target() == copied.target())
                    .expect("copied record exists in the snapshot");
                assert_eq!(&stored[at], copied, "row {}", row.id);
                assert!(!std::ptr::eq(&stored[at], copied), "row {}", row.id);
                assert_eq!(
                    copied.target().scope_kind(),
                    &crate::object_document::FrameScopeKind::Authored
                );
                total_records += 1;
            }
        }
        let bytes: usize = packet
            .cells()
            .iter()
            .flat_map(|cell| cell.records())
            .map(|record| serde_json::to_vec(record).unwrap().len())
            .sum();
        assert_eq!(packet.encoded_bytes(), bytes, "row {}", row.id);
        assert_eq!(
            total_records,
            packet
                .cells()
                .iter()
                .map(|cell| cell.records().len())
                .sum::<usize>(),
            "row {}",
            row.id
        );
    }
}

#[test]
fn record_order_and_reference_records_never_change_the_packet() {
    for row in &rows() {
        let base = snapshot(&document(row, &Build::PLAIN));
        let want = copy_row(&base, row).unwrap();
        for build in [
            Build {
                frame_major: true,
                ..Build::PLAIN
            },
            Build {
                with_reference: true,
                ..Build::PLAIN
            },
            Build {
                frame_major: true,
                with_reference: true,
                ..Build::PLAIN
            },
        ] {
            let other = snapshot(&document(row, &build));
            let got = copy_row(&other, row).unwrap();
            assert_eq!(got.cells(), want.cells(), "row {}", row.id);
            assert_eq!(
                (got.origin(), got.extent(), got.encoded_bytes()),
                (want.origin(), want.extent(), want.encoded_bytes()),
                "row {}",
                row.id
            );
        }
    }
}

#[test]
fn oracle_header_meta_and_row_shapes_are_stable() {
    assert!(ORACLE.starts_with("# L05-H oracle: copyFrames / pasteFrames"));
    assert!(ORACLE.contains("fbe4b8a33e6d5a229f014154df2ba21988a9fabc"));
    assert!(ORACLE.lines().any(|line| line == "meta record-bytes 587"));
    for row in &rows() {
        assert_eq!(row.clip.len(), row.selection.len(), "row {}", row.id);
        assert_eq!(row.pasted.len(), row.selection.len(), "row {}", row.id);
        assert_eq!(row.classes.len(), row.selection.len(), "row {}", row.id);
        assert!(
            row.layers.iter().all(|l| l.len() == row.total as usize),
            "row {}",
            row.id
        );
    }
}

#[test]
fn oracle_reaches_every_cell_kind_and_selection_shape() {
    let all = rows();
    let cells: Vec<&String> = all.iter().flat_map(|row| &row.classes).collect();
    let count = |pred: &dyn Fn(&str) -> bool| cells.iter().filter(|c| pred(c.as_str())).count();
    assert_eq!(cells.len(), 1488);
    assert_eq!(count(&|c| c == "K"), 499);
    assert_eq!(count(&|c| c == "B"), 141);
    assert_eq!(count(&|c| c == "T"), 163);
    assert_eq!(count(&|c| c == "M"), 152);
    assert_eq!(count(&|c| c == "U"), 94);
    assert_eq!(count(&|c| c.starts_with('H') && c.ends_with("=-")), 114);
    assert_eq!(count(&|c| c.starts_with('H') && !c.ends_with("=-")), 325);
    let rows_with = |pred: &dyn Fn(&str) -> bool| {
        all.iter()
            .filter(|row| row.classes.iter().any(|c| pred(c)))
            .count()
    };
    assert_eq!(rows_with(&|c| c == "T" || c == "M"), 223);
    assert_eq!(rows_with(&|c| c == "T"), 135);
    assert_eq!(rows_with(&|c| c == "M"), 128);
    assert_eq!(rows_with(&|c| c.starts_with('H')), 270);
    assert_eq!(rows_with(&|c| c == "B"), 121);
    assert_eq!(rows_with(&|c| c == "U"), 65);
    assert_eq!(
        all.iter()
            .filter(|r| r.bounds.0 > 0 || r.bounds.1 > 0)
            .count(),
        289
    );
    assert_eq!(all.iter().filter(|r| r.bounds.2 > 1).count(), 166);
    assert_eq!(all.iter().filter(|r| r.selection.len() == 1).count(), 102);
    assert_eq!(
        all.iter()
            .filter(|r| r.bounds.2 * r.bounds.3 as usize != r.selection.len())
            .count(),
        162
    );
    assert_eq!(
        all.iter()
            .filter(|r| {
                let mut sorted = r.selection.clone();
                sorted.sort();
                sorted != r.selection
            })
            .count(),
        184
    );
}
