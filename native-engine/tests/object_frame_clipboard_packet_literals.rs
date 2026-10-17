//! Hand-typed acceptance cases for the frame clipboard packet. Expected values are typed here (or
//! pinned from the baseline oracle in the sibling file); none is computed by the code under test.
use super::tests::{
    copy_row, document, pins, record, record_in, snapshot, timing_json, Build, Row,
};
use super::{
    copy_frame_cells, CellKind, ClipboardPacketError, PacketCell, PacketLimits, SelectedCell,
};
use crate::object_document::FrameScopeKind;
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};

pub(crate) fn cell(layer_uid: &str, frame: u32) -> SelectedCell<'_> {
    SelectedCell { layer_uid, frame }
}

pub(crate) fn copy(
    snapshot: &ObjectSnapshot,
    selection: &[SelectedCell<'_>],
) -> Result<super::FrameClipboardPacket, ClipboardPacketError> {
    copy_frame_cells(snapshot, &pins(snapshot), selection, &PacketLimits::DEFAULT)
}

/// Frozen drawing: A has keys 1 and 5 (5 is blank), sample 3, manual sample 6, an ignored record
/// X at the plain frame 2 and a Reference-scope record at the blank key 5; B has keys 0, 2, 6.
pub(crate) fn frozen() -> ObjectSnapshot {
    let doc = json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":8,
        "layers":[
            {"layerUid":"A","timing":timing_json(&[1,5], json!([{"frame":3},{"frame":6,"manualEdit":true}]))},
            {"layerUid":"B","timing":timing_json(&[0,2,6], json!([]))}],
        "objects":[record("A","A",1,11), record("A","X",2,12), record("A","C",3,13),
            record("A","D",6,16), record("B","B0",0,20), record("B","B2",2,22),
            record_in("reference", "A", "R5", 5, 55)]});
    snapshot(&doc)
}

type Summary = (CellKind, Option<u32>, bool, Vec<String>, bool, Vec<String>);

fn ids(records: &[crate::object_document::ObjectRecord]) -> Vec<String> {
    records
        .iter()
        .map(|r| r.target().stroke_id().to_owned())
        .collect()
}

fn summary(cell: &PacketCell) -> Summary {
    (
        cell.kind(),
        cell.source_frame(),
        cell.manual_edit(),
        ids(cell.records()),
        cell.paste_is_keyframe(),
        ids(cell.paste_records()),
    )
}

fn s(list: &[&str]) -> Vec<String> {
    list.iter().map(|item| (*item).to_owned()).collect()
}

#[test]
fn frozen_matrix_keeps_key_blank_held_unexposed_and_tween_apart() {
    let snapshot = frozen();
    let selection: Vec<_> = (0..8).map(|frame| cell("A", frame)).collect();
    let packet = copy(&snapshot, &selection).unwrap();
    let want: Vec<Summary> = vec![
        (CellKind::Unexposed, None, false, s(&[]), false, s(&[])),
        (CellKind::Key, Some(1), false, s(&["A"]), true, s(&["A"])),
        // the record X stored at the plain frame 2 is not exposed and is never copied
        (CellKind::Held, Some(1), false, s(&["A"]), false, s(&[])),
        (
            CellKind::Interpolated,
            Some(3),
            false,
            s(&["C"]),
            true,
            s(&["C"]),
        ),
        // a sample is not a hold key: frame 4 shows key 1 again
        (CellKind::Held, Some(1), false, s(&["A"]), false, s(&[])),
        // the Reference-scope record at frame 5 does not make the blank key a content key
        (CellKind::BlankKey, Some(5), false, s(&[]), true, s(&[])),
        (
            CellKind::Interpolated,
            Some(6),
            true,
            s(&["D"]),
            true,
            s(&["D"]),
        ),
        (CellKind::Held, Some(5), false, s(&[]), false, s(&[])),
    ];
    let got: Vec<Summary> = packet.cells().iter().map(summary).collect();
    assert_eq!(got, want);
    // unexposed, blank key and a held blank share "no records" but are three different cells
    let (unexposed, blank, held_blank) =
        (&packet.cells()[0], &packet.cells()[5], &packet.cells()[7]);
    assert_ne!(unexposed, blank);
    assert_ne!(blank, held_blank);
    assert_ne!(unexposed, held_blank);
    assert!(
        unexposed.records().is_empty()
            && blank.records().is_empty()
            && held_blank.records().is_empty()
    );
}

#[test]
fn array_length_is_not_the_oracle_two_exposures_of_equal_length_differ() {
    // Same layer, same eight frames, same eight selected cells; only the sparse keys differ.
    let make = |keys: &[u32]| {
        let doc = json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":8,
            "layers":[{"layerUid":"A","timing":timing_json(keys, json!([]))}],
            "objects":[record("A","A",0,10)]});
        snapshot(&doc)
    };
    let (held, keyed) = (make(&[0]), make(&[0, 1, 2, 3, 4, 5, 6, 7]));
    let selection: Vec<_> = (0..8).map(|frame| cell("A", frame)).collect();
    let (a, b) = (
        copy(&held, &selection).unwrap(),
        copy(&keyed, &selection).unwrap(),
    );
    assert_eq!(a.cells().len(), b.cells().len());
    let kinds =
        |p: &super::FrameClipboardPacket| p.cells().iter().map(|c| c.kind()).collect::<Vec<_>>();
    assert_eq!(kinds(&a)[..2], [CellKind::Key, CellKind::Held]);
    assert_eq!(kinds(&b)[..2], [CellKind::Key, CellKind::BlankKey]);
    assert_eq!(
        a.cells()
            .iter()
            .filter(|c| c.kind() == CellKind::Held)
            .count(),
        7
    );
    assert_eq!(
        b.cells()
            .iter()
            .filter(|c| c.kind() == CellKind::BlankKey)
            .count(),
        7
    );
}

#[test]
fn selection_order_and_relative_offsets_follow_copy_frames() {
    let snapshot = frozen();
    let selection = [cell("B", 6), cell("A", 3), cell("B", 2), cell("A", 1)];
    let packet = copy(&snapshot, &selection).unwrap();
    let got: Vec<(&str, u32, usize, u32, CellKind)> = packet
        .cells()
        .iter()
        .map(|c| {
            (
                c.layer_uid(),
                c.frame(),
                c.rel_layer(),
                c.rel_frame(),
                c.kind(),
            )
        })
        .collect();
    assert_eq!(
        got,
        vec![
            ("B", 6, 1, 5, CellKind::BlankKey),
            ("A", 3, 0, 2, CellKind::Interpolated),
            ("B", 2, 1, 1, CellKind::Key),
            ("A", 1, 0, 0, CellKind::Key),
        ]
    );
    assert_eq!((packet.origin(), packet.extent()), ((0, 1), (2, 6)));
    assert_eq!(packet.cells()[2].records()[0].target().stroke_id(), "B2");
    // the same cells selected in the reverse order are the same cells in the reverse order
    let reversed = [selection[3], selection[2], selection[1], selection[0]];
    let back = copy(&snapshot, &reversed).unwrap();
    let mut forward: Vec<_> = packet.cells().to_vec();
    forward.reverse();
    assert_eq!(back.cells(), forward.as_slice());
}

#[test]
fn packet_owns_its_bytes_and_outlives_the_snapshot_and_later_edits() {
    fn assert_owned<T: 'static>(_: &T) {}
    let snapshot = frozen();
    let sel = [cell("A", 1), cell("A", 2), cell("B", 2)];
    let packet = copy(&snapshot, &sel).unwrap();
    assert_owned(&packet);
    for (copied, stored) in packet.cells()[0]
        .records()
        .iter()
        .zip(snapshot.document().objects())
    {
        assert_eq!(copied, stored);
        assert!(!std::ptr::eq(copied, stored));
    }
    // an edited copy of the document gets its own snapshot; the packet does not follow it
    let mut edited = snapshot.document().clone();
    match &mut edited.objects[0].fill {
        crate::object_document::Presence::Present(Some(fill)) => {
            fill.r = serde_json::Number::from_f64(0.9).unwrap()
        }
        _ => panic!("fixture record has a solid fill"),
    }
    let moved = ObjectSnapshot::new("clip", edited).unwrap();
    let again = copy(&moved, &sel).unwrap();
    let red = |p: &super::FrameClipboardPacket| {
        p.cells()[0].records()[0]
            .solid_fill()
            .unwrap()
            .r
            .as_f64()
            .unwrap()
    };
    assert_eq!((red(&packet), red(&again)), (0.25, 0.9));
    assert_ne!(packet.cells(), again.cells());
    let kept = packet.clone();
    drop(snapshot);
    drop(moved);
    assert_eq!(kept, packet);
    assert_eq!(red(&kept), 0.25);
    assert_eq!(kept.cells()[2].records()[0].target().stroke_id(), "B2");
    assert_eq!(kept.cells()[1].kind(), CellKind::Held);
}

#[test]
fn reference_scope_records_are_never_copied() {
    let snapshot = frozen();
    // the stored document does contain the Reference record at A frame 5
    assert!(snapshot.document().objects().iter().any(|o| {
        o.target().scope_kind() == &FrameScopeKind::Reference && o.target().frame() == 5
    }));
    let packet = copy(&snapshot, &[cell("A", 5), cell("A", 7)]).unwrap();
    assert!(packet.cells().iter().all(|c| c.records().is_empty()));
    // a key that has both an authored and a Reference record copies only the authored one
    let both = snapshot_of(
        json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":3,
        "layers":[{"layerUid":"A","timing":timing_json(&[1], json!([]))}],
        "objects":[record_in("reference","A","RA",1,1), record("A","A",1,11), record_in("reference","A","RB",1,2)]}),
    );
    let packet = copy(&both, &[cell("A", 1), cell("A", 2)]).unwrap();
    for c in packet.cells() {
        assert_eq!(c.records().len(), 1);
        assert_eq!(c.records()[0].target().stroke_id(), "A");
        assert_eq!(
            c.records()[0].target().scope_kind(),
            &FrameScopeKind::Authored
        );
    }
}

#[test]
fn copy_is_deterministic_and_does_not_touch_the_snapshot() {
    let snapshot = frozen();
    let before = crate::object_codec::encode_storage_project(snapshot.document()).unwrap();
    let sel: Vec<_> = (0..8).map(|frame| cell("A", frame)).collect();
    let (a, b) = (
        copy(&snapshot, &sel).unwrap(),
        copy(&snapshot, &sel).unwrap(),
    );
    assert_eq!(a, b);
    assert_eq!(
        crate::object_codec::encode_storage_project(snapshot.document()).unwrap(),
        before
    );
    assert_eq!(snapshot.content_revision(), 0);
}

#[test]
fn oracle_helper_rows_round_trip_through_the_row_type() {
    // keeps the shared Row/Build helpers honest: one anchor row, hand-checked against the fixture text
    let all = super::tests::rows();
    let row: &Row = &all[0];
    assert_eq!((row.id, row.total, row.selection.len()), (0, 6, 6));
    let snapshot = snapshot(&document(row, &Build::PLAIN));
    let packet = copy_row(&snapshot, row).unwrap();
    let kinds: Vec<CellKind> = packet.cells().iter().map(|c| c.kind()).collect();
    assert_eq!(
        kinds,
        [
            CellKind::Key,
            CellKind::Held,
            CellKind::Held,
            CellKind::BlankKey,
            CellKind::Held,
            CellKind::Key
        ]
    );
}

pub(crate) fn snapshot_of(value: Value) -> ObjectSnapshot {
    snapshot(&value)
}
