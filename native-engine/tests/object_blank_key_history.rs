//! F7 through the real public dispatcher, revision/history and retained receipts.
use super::*;
use crate::history::NativeObjectHistory;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::object_document::ObjectDocument;
use crate::request_receipts::OpacityRequest;
use serde_json::{json, Value};

#[path = "object_frame_edit_document_build.rs"]
#[allow(dead_code)]
mod build;
#[path = "object_blank_key_history_controls.rs"]
mod controls;
#[path = "object_blank_key_edit_build.rs"]
#[allow(dead_code)]
mod fixture;
#[path = "object_exposure_edit_key_rows.rs"]
#[allow(dead_code)]
mod rows;

const OP: &str = "command.document.object.layer.key.blank";
fn owner(document: ObjectDocument) -> NativeObjectHistory {
    NativeObjectHistory::new("blank-history", document).unwrap()
}
fn head(owner: &NativeObjectHistory) -> ObjectDocument {
    owner
        .acquire_snapshot(owner.content_revision())
        .unwrap()
        .document()
        .clone()
}
fn bytes(owner: &NativeObjectHistory) -> Vec<u8> {
    encode_storage_project(&head(owner)).unwrap()
}
fn state(owner: &NativeObjectHistory) -> (u64, (usize, usize), Vec<u8>) {
    (
        owner.content_revision(),
        owner.history_depths(),
        bytes(owner),
    )
}
fn request(owner: &NativeObjectHistory, id: &str, op: &str, payload: Value) -> Value {
    json!({"apiVersion":2,"requestId":id,"instanceId":owner.instance_id(),
        "documentId":owner.document_id(),"expectedRevision":owner.content_revision(),
        "operation":op,"payload":payload})
}
fn blank(owner: &NativeObjectHistory, id: &str, layer: &str, cell: u32) -> Value {
    request(
        owner,
        id,
        OP,
        json!({"command":"object.layer.key.blank","layerUid":layer,"cell":cell}),
    )
}
fn dispatch(owner: &mut NativeObjectHistory, value: &Value) -> Value {
    let typed: OpacityRequest = serde_json::from_value(value.clone()).unwrap();
    serde_json::to_value(owner.try_dispatch_object_command(&typed).unwrap().unwrap()).unwrap()
}
fn success(reply: &Value) {
    assert_eq!(reply["ok"], true, "{reply}");
    assert_eq!(
        reply["result"],
        json!({"applied":true,"historyEntriesAdded":1})
    );
    assert!(serde_json::to_vec(reply).unwrap().len() <= 4096);
}

#[test]
fn public_blank_key_in_four_forms_has_one_checkpoint_undo_redo_and_storage_reopen() {
    for form in build::Build::ALL {
        let input = fixture::document("K=a.1.10;M=m.2.20;.;K=b.3.30|K=z.4.40;.;.;.", form);
        let mut o = owner(input.clone());
        let before = bytes(&o);
        let retained = o.acquire_snapshot(0).unwrap();
        let q = blank(&o, "blank", "L0", 1);
        let reply = dispatch(&mut o, &q);
        success(&reply);
        assert_eq!(reply["contentRevision"], 1);
        assert_eq!(o.history_depths(), (1, 0));
        assert_eq!(
            fixture::value(&head(&o)),
            fixture::expected_blank(&input, 0, 1, "K=a.1.10;K;.;K=b.3.30")
        );
        let after = bytes(&o);
        let after_state = state(&o);
        assert_eq!(dispatch(&mut o, &q), reply);
        assert_eq!(state(&o), after_state);
        let undo = request(&o, "undo", "command.document.object.undo", json!({}));
        assert_eq!(dispatch(&mut o, &undo)["ok"], true);
        assert_eq!(bytes(&o), before);
        assert_eq!(o.history_depths(), (0, 1));
        let redo = request(&o, "redo", "command.document.object.redo", json!({}));
        assert_eq!(dispatch(&mut o, &redo)["ok"], true);
        assert_eq!(bytes(&o), after);
        assert_eq!(o.history_depths(), (1, 0));
        let reopened = owner(decode_storage_project(&after).unwrap());
        assert_eq!(bytes(&reopened), after);
        assert_eq!(encode_storage_project(retained.document()).unwrap(), before);
    }
}

#[test]
fn already_blank_f7_success_always_checkpoints_and_exact_replay_does_not_checkpoint_again() {
    let mut o = owner(fixture::document(
        "K;.;.;K|K=x.1.10;.;.;.",
        build::Build::V2Extras,
    ));
    let original = bytes(&o);
    let q1 = blank(&o, "blank-one", "L0", 0);
    let r1 = dispatch(&mut o, &q1);
    success(&r1);
    assert_eq!(o.history_depths(), (1, 0));
    let q2 = blank(&o, "blank-two", "L0", 0);
    success(&dispatch(&mut o, &q2));
    assert_eq!(o.content_revision(), 2);
    assert_eq!(o.history_depths(), (2, 0));
    assert_eq!(bytes(&o), original);
    let before = state(&o);
    assert_eq!(dispatch(&mut o, &q1), r1);
    assert_eq!(state(&o), before);
    let undo = request(&o, "undo-empty", "command.document.object.undo", json!({}));
    assert_eq!(dispatch(&mut o, &undo)["ok"], true);
    assert_eq!(o.history_depths(), (1, 1));
    let next = blank(&o, "blank-after-undo", "L0", 0);
    success(&dispatch(&mut o, &next));
    assert_eq!(o.history_depths(), (2, 0));
    assert_eq!(bytes(&o), original);
}

#[test]
fn component_without_timing_public_f7_undo_restores_exact_absence_and_replay_is_exact() {
    for locked in [false, true] {
        let mut value = fixture::value(&fixture::component(locked));
        value["layers"][1].as_object_mut().unwrap().remove("timing");
        value["componentTables"]["components"]["instances"][0]
            .as_object_mut()
            .unwrap()
            .remove("frameKeys");
        let input = fixture::admit(value);
        let mut o = owner(input.clone());
        let before = bytes(&o);
        let q = blank(&o, "component-blank", "L1", 2);
        let reply = dispatch(&mut o, &q);
        success(&reply);
        let out = head(&o);
        assert_eq!(o.history_depths(), (1, 0));
        assert_eq!(
            fixture::value(&out)["componentTables"]["components"]["instances"][0]["frameKeys"],
            json!([{"frame":2,"blankOverride":true}])
        );
        #[cfg(feature = "evaluation")]
        {
            use crate::evaluation::object_scene_style_kernels::object_scene_producer_component_select::playback_input;
            use crate::evaluation::object_component_kernels::object_component_playback::{evaluate_component_playback,PlaybackOutcome};
            let instance = out
                .component_set()
                .unwrap()
                .instance("scene-root", "L1")
                .unwrap();
            for frame in 0..5 {
                let playback = playback_input(instance, &out.layers[1], frame).unwrap();
                assert_eq!(
                    matches!(
                        evaluate_component_playback(&playback, 3.0, i64::from(frame), None),
                        PlaybackOutcome::Blank
                    ),
                    frame >= 2
                );
            }
        }
        let after = state(&o);
        assert_eq!(dispatch(&mut o, &q), reply);
        assert_eq!(state(&o), after);
        let undo = request(
            &o,
            "component-undo",
            "command.document.object.undo",
            json!({}),
        );
        assert_eq!(dispatch(&mut o, &undo)["ok"], true);
        assert_eq!(bytes(&o), before);
        assert!(head(&o).layers[1].timing.missing());
    }
}

#[test]
fn mixed_raster_text_blank_history_restores_real_item_and_order_bytes_on_undo() {
    let input = fixture::mixed_items();
    let mut o = owner(input);
    let before = bytes(&o);
    let q = blank(&o, "mixed-blank", "L0", 0);
    success(&dispatch(&mut o, &q));
    let after = bytes(&o);
    assert_ne!(after, before);
    assert_eq!(o.history_depths(), (1, 0));
    let undo = request(&o, "mixed-undo", "command.document.object.undo", json!({}));
    assert_eq!(dispatch(&mut o, &undo)["ok"], true);
    assert_eq!(bytes(&o), before);
    let redo = request(&o, "mixed-redo", "command.document.object.redo", json!({}));
    assert_eq!(dispatch(&mut o, &redo)["ok"], true);
    assert_eq!(bytes(&o), after);
    assert_eq!(
        bytes(&owner(decode_storage_project(&after).unwrap())),
        after
    );
}
