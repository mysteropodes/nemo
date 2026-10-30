//! Oracle rows for the frame-clipboard transactions: oracle.txt, produced by running the verbatim baseline editor
//! (fbe4b8a3: copyFrames, cutFrames, pasteFrames, duplicateSelectedFrames, heldKeyframeAt, the plain-layer tail of
//! getEffectiveStrokes) over dense layers whose key and in-between cells carry strokes. Each row is rebuilt as a native
//! document, turned into the real operation, applied through the history owner and compared back to the baseline.
//! Support only: the tests are in object_frame_clipboard_paste_history.rs and its siblings; running a row
//! through the history owner is object_frame_clipboard_paste_apply.rs.
use super::build::{
    displayed, exposure_string, record_json, render_layer, stroke_of, uid, untouched, Build, Cell,
    Stroke,
};
use crate::commands::{CellRef, FrameClipboardPacket, PasteCell};
use crate::object_document::{ObjectDocument, ObjectRecord};
use crate::revision::ObjectSnapshot;

const ORACLE: &str = include_str!("fixtures/object-frame-clipboard-paste/oracle.txt");

pub(crate) const OP_DUPLICATE: &str = "command.document.object.frames.clipboard.duplicate";
pub(crate) const OP_PASTE: &str = "command.document.object.frames.clipboard.paste";
/// The two forms the clipboard operations run on: plain v1 and v1 with a Reference record on every layer.
pub(crate) const FORMS: [Build; 2] = [Build::V1, Build::V1Ref];

pub(crate) struct Entry {
    pub(crate) rl: u32,
    pub(crate) rf: u32,
    pub(crate) cell: Cell,
}
pub(crate) struct Row {
    pub(crate) id: usize,
    pub(crate) op: String,
    pub(crate) locked: Vec<bool>,
    pub(crate) args: String,
    pub(crate) chg: bool,
    pub(crate) total: u32,
    pub(crate) input: Vec<Vec<Cell>>,
    pub(crate) output: Vec<Vec<Cell>>,
    pub(crate) out_tok: Vec<String>,
    pub(crate) exposure: Vec<String>,
    pub(crate) displayed: Vec<String>,
    pub(crate) clip: Vec<Entry>,
}

fn parse_cell(token: &str) -> Cell {
    let strokes = match token[1..].strip_prefix('=') {
        None => Vec::new(),
        Some(list) => list
            .split(',')
            .map(|entry| {
                let part: Vec<&str> = entry.split('.').collect();
                Stroke {
                    id: part[0].to_owned(),
                    z: part[1].parse().unwrap(),
                    g: part[2].parse().unwrap(),
                }
            })
            .collect(),
    };
    Cell {
        flag: token.chars().next().unwrap(),
        strokes,
    }
}
pub(crate) fn parse_layers(text: &str) -> Vec<Vec<Cell>> {
    text.split('|')
        .map(|layer| layer.split(';').map(parse_cell).collect())
        .collect()
}
fn parse_clip(text: &str) -> Vec<Entry> {
    if text == "-" {
        return Vec::new();
    }
    text.split('+')
        .map(|entry| {
            let part: Vec<&str> = entry.splitn(3, ':').collect();
            Entry {
                rl: part[0].parse().unwrap(),
                rf: part[1].parse().unwrap(),
                cell: parse_cell(part[2]),
            }
        })
        .collect()
}
fn pairs(text: &str) -> Vec<(usize, u32)> {
    text.split(',')
        .map(|cell| {
            let (layer, frame) = cell.split_once(':').unwrap();
            (layer.parse().unwrap(), frame.parse().unwrap())
        })
        .collect()
}

pub(crate) fn rows() -> Vec<Row> {
    ORACLE
        .lines()
        .filter(|line| line.starts_with("row "))
        .map(|line| {
            let part: Vec<&str> = line.split(' ').collect();
            assert_eq!(part.len(), 12, "{line}");
            let output = parse_layers(part[8]);
            Row {
                id: part[1].parse().unwrap(),
                op: part[2].to_owned(),
                locked: part[3].chars().map(|letter| letter == 'L').collect(),
                args: part[4].to_owned(),
                chg: part[5] == "1",
                total: part[6].parse().unwrap(),
                input: parse_layers(part[7]),
                output,
                out_tok: part[8].split('|').map(str::to_owned).collect(),
                exposure: part[9].split('|').map(str::to_owned).collect(),
                displayed: part[10].split('|').map(str::to_owned).collect(),
                clip: parse_clip(part[11]),
            }
        })
        .collect()
}

pub(crate) fn record_of(stroke: &Stroke) -> ObjectRecord {
    serde_json::from_value(record_json(
        "scene-root",
        "authored",
        "source",
        &stroke.id,
        0,
        stroke.z,
        stroke.g,
    ))
    .unwrap()
}

impl Row {
    pub(crate) fn label(&self) -> String {
        format!("{} row {}", self.op, self.id)
    }
    pub(crate) fn any_locked(&self) -> bool {
        self.locked.iter().any(|locked| *locked)
    }
    /// cut: the selection; dupsel: the selection after the `/`.
    pub(crate) fn selection(&self) -> Vec<(usize, u32)> {
        match self.op.as_str() {
            "cut" => pairs(&self.args),
            "dupsel" => pairs(self.args.split_once('/').unwrap().1),
            other => panic!("{other} has no selection"),
        }
    }
    pub(crate) fn cell_refs(&self) -> Vec<CellRef> {
        self.selection()
            .into_iter()
            .map(|(layer, frame)| CellRef {
                layer_uid: uid(layer),
                frame,
            })
            .collect()
    }
    pub(crate) fn paste_base(&self) -> (usize, u32) {
        let (layer, frame) = self.args.split_once(':').unwrap();
        (layer.parse().unwrap(), frame.parse().unwrap())
    }
    pub(crate) fn dup_anchor(&self) -> usize {
        self.args.split_once('/').unwrap().0.parse().unwrap()
    }
    pub(crate) fn min_selected_layer(&self) -> usize {
        self.selection().iter().map(|cell| cell.0).min().unwrap()
    }
    /// The baseline clipboard in paste form: every flag but a plain `.` lands as a key (an in-between included).
    pub(crate) fn paste_cells(&self) -> Vec<PasteCell> {
        self.clip
            .iter()
            .map(|entry| PasteCell {
                items: Default::default(),
                order_elements: crate::object_document::Presence::Missing,
                rel_layer: entry.rl,
                rel_frame: entry.rf,
                keyframe: entry.cell.flag != '.',
                records: entry.cell.strokes.iter().map(record_of).collect(),
            })
            .collect()
    }
}

/// The shared oracle comparison; the controls run it on deliberately wrong outputs.
pub(crate) fn compare(
    row: &Row,
    input: &ObjectDocument,
    out: &ObjectDocument,
) -> Result<(), String> {
    if out.total_frames != row.total {
        return Err(format!("totalFrames {} != {}", out.total_frames, row.total));
    }
    for layer in 0..row.output.len() {
        let (got, want) = (render_layer(out, layer), &row.out_tok[layer]);
        if &got != want {
            return Err(format!("layer {layer} cells\n got {got}\nwant {want}"));
        }
        let exposure = exposure_string(out, layer);
        if exposure != row.exposure[layer] {
            return Err(format!(
                "layer {layer} exposure {exposure} != {}",
                row.exposure[layer]
            ));
        }
        let shown = displayed(out, layer);
        if shown != row.displayed[layer] {
            return Err(format!(
                "layer {layer} displayed {shown} != {}",
                row.displayed[layer]
            ));
        }
    }
    if untouched(input) != untouched(out) {
        return Err(
            "a record, context, policy or property holder that must not change changed".into(),
        );
    }
    ObjectSnapshot::validate_drawing(out).map_err(|error| format!("storage admission: {error}"))
}

/// The cut packet equals the baseline clipboard in paste form: offsets, key or plain, and the exact strokes.
pub(crate) fn packet_matches(row: &Row, packet: &FrameClipboardPacket) -> Result<(), String> {
    let cells = packet.cells();
    if cells.len() != row.clip.len() {
        return Err(format!(
            "{} packet cells != {}",
            cells.len(),
            row.clip.len()
        ));
    }
    for (index, (cell, entry)) in cells.iter().zip(&row.clip).enumerate() {
        let strokes: Vec<Stroke> = cell.paste_records().iter().map(stroke_of).collect();
        let ok = cell.rel_layer() as u32 == entry.rl
            && cell.rel_frame() == entry.rf
            && cell.paste_is_keyframe() == (entry.cell.flag != '.')
            && strokes == entry.cell.strokes;
        if !ok {
            return Err(format!(
                "packet cell {index} differs from the baseline clipboard"
            ));
        }
    }
    Ok(())
}

/// Hand-authored order deliberately disagrees with record insertion order a,b.
pub(crate) fn ordered_elements(layer: &str) -> serde_json::Value {
    serde_json::json!([
        {"layerUid":layer,"strokeId":"b","naturalPosition":0,"order":{"static":[-4],
            "track":{"keys":[{"frame":0,"v":[-3]}]},"expression":{"enabled":false,"code":"1"}}},
        {"layerUid":layer,"strokeId":"a","naturalPosition":1,"order":{"static":[7]}}
    ])
}
pub(crate) fn ordered_document(form: Build) -> ObjectDocument {
    let mut value =
        super::build::document_json(&parse_layers("K=a.1.10,b.2.20;.;.;.|K=c.3.30;.;.;."), form);
    let scopes = value["compositionOrder"]["contexts"][0]["scopes"]
        .as_array_mut()
        .unwrap();
    let elements = scopes[0]["elements"].as_array_mut().unwrap();
    let other = elements
        .iter()
        .find(|element| element["layerUid"] == "L1")
        .unwrap()
        .clone();
    *elements = ordered_elements("L0").as_array().unwrap().clone();
    elements.push(other);
    crate::object_codec::decode_storage_project(value.to_string().as_bytes()).unwrap()
}
pub(crate) fn scoped_elements(
    document: &ObjectDocument,
    layer: &str,
    frame: u32,
) -> serde_json::Value {
    serde_json::to_value(
        document
            .composition_order
            .value()
            .unwrap()
            .contexts
            .iter()
            .filter(|context| context.context_id == "scene-root")
            .flat_map(|context| &context.scopes)
            .filter(|scope| scope.frame_scope.frame == frame)
            .flat_map(|scope| &scope.elements)
            .filter(|element| element.layer_uid == layer)
            .collect::<Vec<_>>(),
    )
    .unwrap()
}
