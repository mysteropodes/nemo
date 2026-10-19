//! Real retained-dispatch/history/storage oracles for the original atomic bar action. Prospective.
use super::*;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::object_document::ObjectDocument;
use serde_json::{json, Value};
#[path = "object_frame_edit_document_build.rs"]
#[allow(dead_code)]
mod build;
#[path = "object_layer_shift_history_controls.rs"]
mod controls;
#[path = "object_layer_shift_build.rs"]
#[allow(dead_code)]
mod fixture;
#[path = "object_exposure_edit_key_rows.rs"]
#[allow(dead_code)]
mod rows;
fn owner(d: ObjectDocument) -> NativeObjectHistory {
    NativeObjectHistory::new("bar-history", d).unwrap()
}
fn head(o: &NativeObjectHistory) -> ObjectDocument {
    o.acquire_snapshot(o.content_revision())
        .unwrap()
        .document()
        .clone()
}
fn bytes(o: &NativeObjectHistory) -> Vec<u8> {
    encode_storage_project(&head(o)).unwrap()
}
fn state(o: &NativeObjectHistory) -> (u64, (usize, usize), Vec<u8>) {
    (o.content_revision(), o.history_depths(), bytes(o))
}
fn request(o: &NativeObjectHistory, id: &str, payload: Value) -> Value {
    json!({"apiVersion":2,"requestId":id,"instanceId":o.instance_id(),"documentId":o.document_id(),
        "expectedRevision":o.content_revision(),"operation":OP,"payload":payload})
}
fn dispatch(o: &mut NativeObjectHistory, q: &Value) -> Value {
    let typed: OpacityRequest = serde_json::from_value(q.clone()).unwrap();
    serde_json::to_value(o.try_dispatch_object_command(&typed).unwrap().unwrap()).unwrap()
}
fn success(reply: &Value, request: &Value, revision: u64) {
    assert_eq!(reply["ok"], true, "{reply}");
    assert_eq!(reply["contentRevision"], revision);
    assert_eq!(
        reply["result"],
        json!({"applied":true,"historyEntriesAdded":1})
    );
    for key in ["requestId", "instanceId", "documentId"] {
        assert_eq!(reply[key], request[key]);
    }
    assert!(serde_json::to_vec(reply).unwrap().len() <= 4096);
}
fn history(o: &mut NativeObjectHistory, id: &str, kind: &str) {
    let mut q = request(o, id, json!({"command":format!("object.{kind}")}));
    q["operation"] = json!(format!("command.document.object.{kind}"));
    assert_eq!(dispatch(o, &q)["ok"], true);
}
#[test]
fn actual_bar_request_atomically_publishes_literal_content_motion_range_and_active_arc_once() {
    let input = fixture::document();
    let mut o = owner(input.clone());
    let before = bytes(&o);
    let retained = o.acquire_snapshot(0).unwrap();
    let q = request(&o, "bar-body", fixture::gesture("both", &[2]));
    let reply = dispatch(&mut o, &q);
    success(&reply, &q, 1);
    assert_eq!(o.history_depths(), (1, 0));
    assert_eq!(
        serde_json::to_value(head(&o)).unwrap(),
        fixture::expected_plus_two(&input)
    );
    let after = bytes(&o);
    let committed = state(&o);
    assert_eq!(dispatch(&mut o, &q), reply);
    assert_eq!(state(&o), committed);
    history(&mut o, "undo-body", "undo");
    assert_eq!(bytes(&o), before);
    assert_eq!(o.history_depths(), (0, 1));
    let undone = state(&o);
    assert_eq!(dispatch(&mut o, &q), reply);
    assert_eq!(state(&o), undone);
    history(&mut o, "redo-body", "redo");
    assert_eq!(bytes(&o), after);
    assert_eq!(o.history_depths(), (1, 0));
    assert_eq!(
        bytes(&owner(decode_storage_project(&after).unwrap())),
        after
    );
    assert_eq!(encode_storage_project(retained.document()).unwrap(), before);
}
#[test]
fn unmoved_and_plain_click_both_checkpoint_and_clear_redo_even_when_storage_is_identical() {
    for plain in [false, true] {
        let mut o = owner(fixture::document());
        let before = bytes(&o);
        let mut payload = fixture::gesture("both", &[]);
        payload["plainClick"] = json!(plain);
        let q = request(&o, "click", payload.clone());
        success(&dispatch(&mut o, &q), &q, 1);
        assert_eq!(bytes(&o), before);
        assert_eq!(o.history_depths(), (1, 0));
        history(&mut o, "undo-click", "undo");
        assert_eq!(o.history_depths(), (0, 1));
        let q = request(&o, "click-after-undo", payload);
        let revision = o.content_revision() + 1;
        success(&dispatch(&mut o, &q), &q, revision);
        assert_eq!(bytes(&o), before);
        assert_eq!(o.history_depths(), (1, 0));
    }
}
#[test]
fn explicit_selected_keys_and_alt_are_real_shared_command_inputs_not_unconditional_whole_track_edits(
) {
    for alt in [false, true] {
        let input = fixture::document();
        let mut o = owner(input.clone());
        let mut payload = fixture::gesture("both", &[2]);
        payload["selectedKeys"] = fixture::selected();
        payload["alt"] = json!(alt);
        let q = request(&o, "selected", payload);
        success(&dispatch(&mut o, &q), &q, 1);
        let mut expected = fixture::expected_plus_two(&input);
        if alt {
            expected["propertyHolders"][0]["properties"][0]["track"]["keys"][0]["frame"] = json!(1);
        }
        assert_eq!(serde_json::to_value(head(&o)).unwrap(), expected);
    }
}
