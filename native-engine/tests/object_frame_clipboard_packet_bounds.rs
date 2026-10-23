//! Bounds, pins and typed refusals of the clipboard packet, hand-typed. Every refusal is raised
//! before a packet exists, so nothing partial can escape; the byte and cell caps are tested at the
//! exact boundary (limit passes, limit minus one refuses).
use super::literals::{cell, copy, frozen, snapshot_of};
use super::tests::{pins, record, timing_json, uid};
use super::{
    copy_frame_cells, ClipboardPacketError, PacketLimits, PacketPins, SelectedCell,
    MAX_PACKET_BYTES, MAX_PACKET_CELLS,
};
use crate::revision::ObjectSnapshot;
use serde_json::json;

/// `JSON.stringify` length of the standard test record, produced by the Node generator.
const RECORD_BYTES: usize = 587;

fn refuse_pins(
    snapshot: &ObjectSnapshot,
    wanted: &PacketPins<'_>,
    selection: &[SelectedCell<'_>],
) -> ClipboardPacketError {
    copy_frame_cells(snapshot, wanted, selection, &PacketLimits::DEFAULT).unwrap_err()
}

fn refuse(snapshot: &ObjectSnapshot, selection: &[SelectedCell<'_>]) -> ClipboardPacketError {
    copy(snapshot, selection).unwrap_err()
}

#[test]
fn pins_are_checked_in_order_and_each_mismatch_is_its_own_refusal() {
    let snapshot = frozen();
    let other = frozen();
    let sel = [cell("A", 1)];
    let good = pins(&snapshot);
    let run = |wanted: PacketPins<'_>| refuse_pins(&snapshot, &wanted, &sel);
    assert_eq!(
        run(PacketPins {
            instance_id: "elsewhere",
            ..good
        }),
        ClipboardPacketError::WrongInstance
    );
    assert_eq!(run(pins(&other)), ClipboardPacketError::WrongDocument);
    assert_eq!(
        run(PacketPins {
            content_revision: good.content_revision + 1,
            ..good
        }),
        ClipboardPacketError::WrongRevision
    );
    assert_eq!(
        run(PacketPins {
            document_snapshot_id: "native-object:other:0",
            ..good
        }),
        ClipboardPacketError::WrongSnapshot
    );
    assert!(copy_frame_cells(&snapshot, &good, &sel, &PacketLimits::DEFAULT).is_ok());
    let packet = copy(&snapshot, &sel).unwrap();
    assert_eq!(
        (
            packet.instance_id(),
            packet.document_id(),
            packet.content_revision(),
            packet.document_snapshot_id(),
            packet.context_id()
        ),
        (
            "clip",
            snapshot.document_id(),
            0,
            snapshot.snapshot_id(),
            "scene-root"
        )
    );
    assert_ne!(packet.document_id(), other.document_id());
}

#[test]
fn malformed_selections_and_documents_are_refused_typed() {
    let snapshot = frozen();
    assert_eq!(refuse(&snapshot, &[]), ClipboardPacketError::EmptySelection);
    assert_eq!(
        refuse(&snapshot, &[cell("nope", 1)]),
        ClipboardPacketError::UnknownLayer
    );
    assert_eq!(
        refuse(&snapshot, &[cell("A", 1), cell("A", 8)]),
        ClipboardPacketError::FrameOutOfRange
    );
    assert_eq!(
        refuse(&snapshot, &[cell("A", 1), cell("B", 1), cell("A", 1)]),
        ClipboardPacketError::DuplicateCell
    );
    // a layer without a timing model is not a blank key
    let untimed = snapshot_of(
        json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":3,
        "layers":[{"layerUid":"A"}],"objects":[]}),
    );
    assert_eq!(
        refuse(&untimed, &[cell("A", 0)]),
        ClipboardPacketError::TimingUnavailable
    );
    // An ordered packet carries the exact admitted DTO, not an invented record order.
    let ordered = snapshot_of(
        json!({"format":"nemo.native-object-document","formatVersion":2,"totalFrames":3,
        "layers":[{"layerUid":"A","timing":timing_json(&[1], json!([]))}],
        "objects":[record("A","A",1,11)],
        "compositionOrder":{"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
            "migration":"explicit-plan-required","referencePolicy":"unresolved","contexts":[{"contextId":"scene-root",
            "layers":[{"layerUid":"A","naturalPosition":0,"order":{}}],
            "scopes":[{"frameScope":{"kind":"authored","frame":1},
                "elements":[{"layerUid":"A","strokeId":"A","naturalPosition":0,"order":{}}]}]}]}}),
    );
    let packet = copy(&ordered, &[cell("A", 1)]).unwrap();
    assert_eq!(
        serde_json::to_value(packet.cells()[0].order_elements().value().unwrap()).unwrap(),
        json!([{"layerUid":"A","strokeId":"A","naturalPosition":0,"order":{}}])
    );
    let exact = packet.encoded_bytes();
    assert!(copy_frame_cells(
        &ordered,
        &pins(&ordered),
        &[cell("A", 1)],
        &PacketLimits {
            max_cells: 1,
            max_bytes: exact
        }
    )
    .is_ok());
    assert_eq!(
        copy_frame_cells(
            &ordered,
            &pins(&ordered),
            &[cell("A", 1)],
            &PacketLimits {
                max_cells: 1,
                max_bytes: exact - 1
            }
        )
        .unwrap_err(),
        ClipboardPacketError::TooManyBytes
    );
}

/// L0: key 0 with three records of identical encoded size; L1: a blank key.
fn sized() -> ObjectSnapshot {
    snapshot_of(
        json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":6,
        "layers":[{"layerUid":"L0","timing":timing_json(&[0], json!([]))},
                  {"layerUid":"L1","timing":timing_json(&[0], json!([]))}],
        "objects":[record("L0","s0_0_0",0,10), record("L0","s0_0_1",0,10), record("L0","s0_0_2",0,10)]}),
    )
}

fn copy_limited(
    snapshot: &ObjectSnapshot,
    selection: &[SelectedCell<'_>],
    max_bytes: usize,
) -> Result<super::FrameClipboardPacket, ClipboardPacketError> {
    let limits = PacketLimits {
        max_cells: 64,
        max_bytes,
    };
    copy_frame_cells(snapshot, &pins(snapshot), selection, &limits)
}

#[test]
fn byte_cap_is_exact_charges_held_cells_again_and_blank_cells_are_free() {
    let snapshot = sized();
    let one_record = serde_json::to_vec(&snapshot.document().objects()[0])
        .unwrap()
        .len();
    assert_eq!(one_record, RECORD_BYTES);
    let run = |sel: &[SelectedCell<'_>], max_bytes: usize| copy_limited(&snapshot, sel, max_bytes);
    let key_only = [cell("L0", 0)];
    assert_eq!(
        run(&key_only, 3 * RECORD_BYTES).unwrap().encoded_bytes(),
        3 * RECORD_BYTES
    );
    assert_eq!(
        run(&key_only, 3 * RECORD_BYTES - 1).unwrap_err(),
        ClipboardPacketError::TooManyBytes
    );
    // a held cell clones the shown key's records again and is charged again
    let with_held = [cell("L0", 0), cell("L0", 1), cell("L0", 2)];
    assert_eq!(
        run(&with_held, 9 * RECORD_BYTES).unwrap().encoded_bytes(),
        9 * RECORD_BYTES
    );
    assert_eq!(
        run(&with_held, 9 * RECORD_BYTES - 1).unwrap_err(),
        ClipboardPacketError::TooManyBytes
    );
    // a blank key, and any cell without records, costs nothing
    assert_eq!(
        run(&[cell("L1", 0), cell("L1", 5)], 0)
            .unwrap()
            .encoded_bytes(),
        0
    );
    assert_eq!(
        run(&key_only, 0).unwrap_err(),
        ClipboardPacketError::TooManyBytes
    );
    // the hard ceiling clamps a larger caller limit
    assert_eq!(MAX_PACKET_BYTES, 4 * 1024 * 1024);
}

#[test]
fn cell_cap_is_exact_and_the_hard_ceiling_clamps_the_caller_limit() {
    let snapshot = sized();
    let sel = [cell("L0", 0), cell("L0", 1), cell("L0", 2)];
    let run = |max_cells| {
        copy_frame_cells(
            &snapshot,
            &pins(&snapshot),
            &sel,
            &PacketLimits {
                max_cells,
                max_bytes: usize::MAX,
            },
        )
    };
    assert_eq!(run(3).unwrap().cells().len(), 3);
    assert_eq!(run(2).unwrap_err(), ClipboardPacketError::TooManyCells);
    let long = snapshot_of(
        json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":4100,
        "layers":[{"layerUid":"L1","timing":timing_json(&[0], json!([]))}],"objects":[]}),
    );
    let uids = vec![uid(1); MAX_PACKET_CELLS + 1];
    let many: Vec<SelectedCell<'_>> = (0..=MAX_PACKET_CELLS as u32)
        .map(|f| cell(&uids[f as usize], f))
        .collect();
    let unbounded = PacketLimits {
        max_cells: usize::MAX,
        max_bytes: usize::MAX,
    };
    let ok = copy_frame_cells(&long, &pins(&long), &many[..MAX_PACKET_CELLS], &unbounded).unwrap();
    assert_eq!((ok.cells().len(), ok.encoded_bytes()), (4096, 0));
    assert_eq!(
        copy_frame_cells(&long, &pins(&long), &many, &unbounded).unwrap_err(),
        ClipboardPacketError::TooManyCells
    );
}

#[test]
fn byte_ceiling_clamps_an_unbounded_caller_limit() {
    // 7600 records of about 587 encoded bytes each exceed the 4 MiB ceiling on one key cell.
    let objects: Vec<_> = (0..7600)
        .map(|i| record("L0", &format!("stroke-{i}"), 0, 10))
        .collect();
    let big = snapshot_of(
        json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":2,
        "layers":[{"layerUid":"L0","timing":timing_json(&[0], json!([]))}],"objects":objects}),
    );
    let unbounded = PacketLimits {
        max_cells: usize::MAX,
        max_bytes: usize::MAX,
    };
    let got = copy_frame_cells(&big, &pins(&big), &[cell("L0", 0)], &unbounded);
    assert_eq!(got.unwrap_err(), ClipboardPacketError::TooManyBytes);
    // the same cell with a caller limit below the ceiling refuses at the same point; a few
    // records fit under both
    let few = sized();
    let ok = copy_frame_cells(&few, &pins(&few), &[cell("L0", 0)], &unbounded).unwrap();
    assert_eq!(ok.encoded_bytes(), 3 * RECORD_BYTES);
}

#[test]
fn rich_multi_element_order_obeys_exact_array_byte_budget_without_changing_source() {
    let records = vec![record("A", "first", 1, 11), record("A", "second", 1, 22)];
    let elements = json!([
        {"layerUid":"A","strokeId":"second","naturalPosition":0,"order":{
            "static":[-7],"track":{"keys":[{"frame":0,"v":[3],"hold":true},
                {"frame":1,"v":[5],"hIn":[0,1],"hOut":[1,0]}]},
            "expression":{"enabled":false,"code":"1 + 2"}}},
        {"layerUid":"A","strokeId":"first","naturalPosition":1,"order":{
            "static":[9],"track":{"keys":[{"frame":0,"v":[-2]},
                {"frame":2,"v":[8],"curvePoints":null}]}}}
    ]);
    let expected_bytes = records
        .iter()
        .map(|value| serde_json::to_vec(value).unwrap().len())
        .sum::<usize>()
        + serde_json::to_vec(&elements).unwrap().len();
    let snapshot = snapshot_of(json!({
        "format":"nemo.native-object-document","formatVersion":2,"totalFrames":3,
        "layers":[{"layerUid":"A","timing":timing_json(&[1], json!([]))}],
        "objects":records,
        "compositionOrder":{"format":"nemo.native-object-order-plan","formatVersion":1,
            "documentFormatVersion":2,"migration":"explicit-plan-required","referencePolicy":"unresolved",
            "contexts":[{"contextId":"scene-root","layers":[{"layerUid":"A","naturalPosition":0,"order":{}}],
                "scopes":[{"frameScope":{"kind":"authored","frame":1},"elements":elements}]}]}
    }));
    let before = serde_json::to_vec(snapshot.document()).unwrap();
    let selection = [cell("A", 1)];
    let limits = PacketLimits {
        max_cells: 1,
        max_bytes: expected_bytes,
    };
    let packet = copy_frame_cells(&snapshot, &pins(&snapshot), &selection, &limits).unwrap();
    assert_eq!(packet.encoded_bytes(), expected_bytes);
    assert_eq!(
        serde_json::to_value(packet.cells()[0].order_elements().value().unwrap()).unwrap(),
        elements
    );
    assert_eq!(
        copy_frame_cells(
            &snapshot,
            &pins(&snapshot),
            &selection,
            &PacketLimits {
                max_bytes: expected_bytes - 1,
                ..limits
            }
        )
        .unwrap_err(),
        ClipboardPacketError::TooManyBytes
    );
    assert_eq!(
        serde_json::to_vec(snapshot.document()).unwrap(),
        before,
        "both success and partial-budget refusal leave the retained source untouched"
    );
}
