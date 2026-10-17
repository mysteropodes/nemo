//! The range records through the real history owner: every oracle row (the verbatim dense baseline editor
//! at fbe4b8a3 over layers with full range records) is rebuilt as a native document in two forms, sent as
//! ONE frame-structure request, compared back to the baseline, undone to the original bytes, redone and
//! reopened from storage. Selection and lock policy come from the real transaction, not from the oracle's
//! roles. Hand-computed cases cover a chain of edits, a no-op after an undo, request replay and the key
//! transactions, which must leave every range alone.
use super::*;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::object_document::ObjectDocument;
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};

#[path = "object_frame_edit_document_build.rs"]
#[allow(dead_code)]
mod build;
#[cfg(feature = "evaluation")]
#[path = "object_frame_edit_range_records_exposure.rs"]
mod exposure;
#[path = "object_frame_edit_range_records_rows.rs"]
#[allow(dead_code)]
mod rows;

use build::{document_json, Build, Cell, Stroke};
use rows::{range_json, shift_rows, Outcome, ShiftRow};

fn owner(document: ObjectDocument) -> NativeObjectHistory {
    NativeObjectHistory::new("range-records", document).unwrap()
}
fn head(o: &NativeObjectHistory) -> ObjectSnapshot {
    o.acquire_snapshot(o.content_revision()).unwrap()
}
fn bytes(o: &NativeObjectHistory) -> Vec<u8> {
    encode_storage_project(head(o).document()).unwrap()
}
fn request(o: &NativeObjectHistory, id: &str, operation: &str, payload: Value) -> Value {
    json!({"apiVersion":2,"requestId":id,"instanceId":o.instance_id(),
        "documentId":o.document_id(),"expectedRevision":o.content_revision(),
        "operation":operation,"payload":payload})
}
fn dispatch(o: &mut NativeObjectHistory, request: &Value) -> Value {
    let typed = serde_json::from_value(request.clone()).unwrap();
    serde_json::to_value(o.try_dispatch_object_command(&typed).unwrap().unwrap()).unwrap()
}
fn send(o: &mut NativeObjectHistory, id: &str, operation: &str, payload: Value) -> Value {
    let submitted = request(o, id, operation, payload);
    dispatch(o, &submitted)
}
fn applied(reply: &Value) -> Value {
    json!({"applied":reply["result"]["applied"],"historyEntriesAdded":reply["result"]["historyEntriesAdded"]})
}
fn ranges(o: &NativeObjectHistory) -> Vec<Value> {
    let snapshot = head(o);
    (0..snapshot.document().layers.len())
        .map(|index| range_json(snapshot.document(), index))
        .collect()
}

/// Every layer holds one stroke on a key at frame 0 and holds it to the end, so the optional
/// duplicate-key source always shows something.
fn cells(layer: usize, total: usize) -> Vec<Cell> {
    (0..total)
        .map(|frame| Cell {
            flag: if frame == 0 { 'K' } else { '.' },
            strokes: if frame == 0 {
                vec![Stroke {
                    id: format!("s{layer}"),
                    z: 1,
                    g: layer as i64 + 1,
                }]
            } else {
                Vec::new()
            },
        })
        .collect()
}
fn document(total: usize, specs: &[(bool, Value)], build: Build) -> ObjectDocument {
    let layers: Vec<Vec<Cell>> = (0..specs.len()).map(|layer| cells(layer, total)).collect();
    let mut value = document_json(&layers, build);
    for (index, (locked, range)) in specs.iter().enumerate() {
        if *locked {
            value["layers"][index]["locked"] = json!(true);
        }
        if !range.is_null() {
            value["layers"][index]["range"] = range.clone();
        }
    }
    decode_storage_project(value.to_string().as_bytes()).unwrap()
}
fn row_document(row: &ShiftRow, build: Build) -> ObjectDocument {
    let specs: Vec<(bool, Value)> = row
        .layers
        .iter()
        .map(|layer| (layer.locked, layer.range.clone()))
        .collect();
    document(row.total as usize, &specs, build)
}

fn operation(row: &ShiftRow) -> &'static str {
    match row.op.as_str() {
        "insert" => "command.document.object.frames.insert",
        "extend" => "command.document.object.frames.extend",
        "dupkey" => "command.document.object.frames.duplicate",
        _ => "command.document.object.frames.remove",
    }
}
fn payload(row: &ShiftRow) -> Value {
    match row.op.as_str() {
        "insert" => {
            let mut payload = json!({"command":"object.frames.insert","cell":row.cursor});
            if let Some(selection) = &row.sel {
                payload["layerUids"] = json!(selection);
            }
            payload
        }
        "extend" => json!({"command":"object.frames.extend","cell":row.cursor,"count":row.count}),
        "dupkey" => json!({"command":"object.frames.duplicate","layerUid":"L0","cell":row.cursor}),
        "remove" => json!({"command":"object.frames.remove","from":row.cursor,"to":row.cursor}),
        _ => json!({"command":"object.frames.remove","from":row.cursor,
            "to":row.cursor + row.count - 1}),
    }
}
/// A one-frame span is the native single-frame removal (see the join file's recorded decision).
fn expected(row: &ShiftRow) -> Option<&Outcome> {
    if row.op == "span" && row.count == 1 {
        row.as_remove.as_ref().or(row.out.as_ref())
    } else {
        row.out.as_ref()
    }
}

#[test]
fn every_oracle_row_matches_through_dispatch_undo_redo_and_reopen() {
    let (mut changed, mut unchanged) = (0, 0);
    for row in shift_rows() {
        for form in [Build::V1, Build::V2] {
            let input = row_document(&row, form);
            let mut o = owner(input);
            let before = bytes(&o);
            let who = format!(
                "{} total {} cursor {} {form:?}",
                row.op, row.total, row.cursor
            );
            let reply = send(&mut o, "row", operation(&row), payload(&row));
            assert_eq!(reply["ok"], true, "{who}: {reply}");
            let Some(want) = expected(&row) else {
                assert_eq!(
                    applied(&reply),
                    json!({"applied":false,"historyEntriesAdded":0}),
                    "{who}"
                );
                assert_eq!(
                    (o.content_revision(), o.history_depths()),
                    (0, (0, 0)),
                    "{who}"
                );
                assert_eq!(
                    bytes(&o),
                    before,
                    "{who}: a refused baseline row is a native no-op"
                );
                unchanged += 1;
                continue;
            };
            assert_eq!(
                applied(&reply),
                json!({"applied":true,"historyEntriesAdded":1}),
                "{who}"
            );
            assert_eq!(head(&o).document().total_frames, want.total, "{who}");
            assert_eq!(ranges(&o), want.ranges, "{who}");
            assert_eq!(
                (o.content_revision(), o.history_depths()),
                (1, (1, 0)),
                "{who}"
            );
            let after = head(&o).document().clone();
            let reopened = decode_storage_project(&bytes(&o)).unwrap();
            assert_eq!(reopened, after, "{who}: save and reopen");
            let undone = o.undo(o.content_revision()).unwrap();
            assert_eq!(
                bytes(&o),
                before,
                "{who}: one Undo restores the complete bytes"
            );
            assert_eq!(o.history_depths(), (0, 1), "{who}");
            o.redo(undone).unwrap();
            assert_eq!(
                *head(&o).document(),
                after,
                "{who}: Redo reproduces the edit"
            );
            changed += 1;
        }
    }
    assert_eq!(
        (changed, unchanged),
        (668, 34),
        "334 + 17 rows in two forms"
    );
}

fn chain_ranges() -> [(bool, Value); 2] {
    [
        (
            false,
            json!({"inPoint":1,"outPoint":3,"markers":[{"frame":2,"name":"a"}],
                "timeLink":{"uid":"L1"}}),
        ),
        (false, json!({"markers":[{"frame":3,"color":"#fff"}]})),
    ]
}

#[test]
fn a_chain_of_edits_shifts_literally_and_undoes_back_through_every_byte_state() {
    let mut o = owner(document(4, &chain_ranges(), Build::V1));
    let start = ranges(&o);
    let mut states = vec![bytes(&o)];
    let steps = [
        (
            "command.document.object.frames.insert",
            json!({"command":"object.frames.insert","cell":0}),
        ),
        (
            "command.document.object.frames.remove",
            json!({"command":"object.frames.remove","from":1,"to":1}),
        ),
        (
            "command.document.object.frames.extend",
            json!({"command":"object.frames.extend","cell":0,"count":2}),
        ),
    ];
    let expected_after = [
        vec![
            json!({"inPoint":2,"outPoint":4,"markers":[{"frame":3,"name":"a"}],"timeLink":{"uid":"L1"}}),
            json!({"markers":[{"frame":4,"color":"#fff"}]}),
        ],
        start.clone(),
        start.clone(),
    ];
    for (index, (name, body)) in steps.into_iter().enumerate() {
        let reply = send(&mut o, &format!("c{index}"), name, body);
        assert_eq!(reply["ok"], true, "{reply}");
        assert_eq!(ranges(&o), expected_after[index], "step {index}");
        states.push(bytes(&o));
    }
    assert_eq!(head(&o).document().total_frames, 6);
    assert_eq!(o.history_depths(), (3, 0));
    for earlier in (0..3).rev() {
        o.undo(o.content_revision()).unwrap();
        assert_eq!(bytes(&o), states[earlier], "undo back to state {earlier}");
    }
    assert_eq!(ranges(&o), start);
}

#[test]
fn a_no_op_after_an_undo_keeps_redo_and_the_bytes() {
    let mut o = owner(document(4, &chain_ranges(), Build::V1));
    let insert = json!({"command":"object.frames.insert","cell":0});
    send(&mut o, "a", "command.document.object.frames.insert", insert);
    o.undo(o.content_revision()).unwrap();
    let state = (o.content_revision(), o.history_depths(), bytes(&o));
    for (name, body) in [
        (
            "command.document.object.frames.remove",
            json!({"command":"object.frames.remove","from":0,"to":3}),
        ),
        (
            "command.document.object.frames.extend",
            json!({"command":"object.frames.extend","cell":0,"count":0}),
        ),
    ] {
        let reply = send(&mut o, name, name, body);
        assert_eq!(
            applied(&reply),
            json!({"applied":false,"historyEntriesAdded":0}),
            "{name}"
        );
        assert_eq!(
            (o.content_revision(), o.history_depths(), bytes(&o)),
            state,
            "{name}"
        );
    }
    assert_eq!(o.history_depths(), (0, 1), "redo survived both no-ops");
}

#[test]
fn exact_request_replay_never_shifts_a_range_twice() {
    let mut o = owner(document(4, &chain_ranges(), Build::V1));
    let submitted = request(
        &o,
        "once",
        "command.document.object.frames.insert",
        json!({"command":"object.frames.insert","cell":0}),
    );
    let first = dispatch(&mut o, &submitted);
    let shifted = ranges(&o);
    let again = dispatch(&mut o, &submitted);
    assert_eq!(again, first, "the retained receipt is returned");
    assert_eq!((o.content_revision(), o.history_depths()), (1, (1, 0)));
    assert_eq!(ranges(&o), shifted, "a replay does not execute twice");
}

#[test]
fn the_key_transactions_leave_every_layer_range_alone() {
    let mut o = owner(document(4, &chain_ranges(), Build::V1));
    let start = ranges(&o);
    let before = bytes(&o);
    let promote = json!({"command":"object.layer.key.promote","cells":[2]});
    let reply = send(
        &mut o,
        "f6",
        "command.document.object.layer.key.promote",
        promote,
    );
    assert_eq!(
        applied(&reply),
        json!({"applied":true,"historyEntriesAdded":1})
    );
    assert_ne!(bytes(&o), before, "the key really was inserted");
    assert_eq!(ranges(&o), start, "F6 moves no range, as the baseline");
    o.undo(o.content_revision()).unwrap();
    assert_eq!(bytes(&o), before);
}
