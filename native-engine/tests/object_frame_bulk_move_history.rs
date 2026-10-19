//! Ordinary Move through the actual shared dispatcher, history, codec and retained receipts.
use super::*;
use crate::history::NativeObjectHistory;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::object_document::{ObjectDocument, Presence};
use crate::request_receipts::OpacityRequest;
use serde_json::{json, Value};
#[path = "object_frame_edit_document_build.rs"]
#[allow(dead_code)]
mod build;
#[path = "object_frame_bulk_move_history_controls.rs"]
mod controls;
#[path = "object_blank_key_edit_build.rs"]
#[allow(dead_code)]
mod f7;
#[path = "object_frame_bulk_move_build.rs"]
#[allow(dead_code)]
mod fixture;
#[path = "object_exposure_edit_key_rows.rs"]
#[allow(dead_code)]
mod rows;
const OP: &str = "command.document.object.frames.move";
fn owner(document: ObjectDocument) -> NativeObjectHistory {
    NativeObjectHistory::new("bulk-history", document).unwrap()
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
fn moving(
    o: &NativeObjectHistory,
    id: &str,
    cells: &[(&str, u32)],
    layer: &str,
    frame: u32,
) -> Value {
    request(o, id, fixture::moving(cells, layer, frame))
}
fn dispatch(o: &mut NativeObjectHistory, q: &Value) -> Value {
    let typed: OpacityRequest = serde_json::from_value(q.clone()).unwrap();
    serde_json::to_value(o.try_dispatch_object_command(&typed).unwrap().unwrap()).unwrap()
}
fn result(applied: bool, selection: Value) -> Value {
    json!({"applied":applied,"historyEntriesAdded":u64::from(applied),"selection":selection})
}
fn success(reply: &Value, selection: Value) {
    assert_eq!(reply["ok"], true, "{reply}");
    assert_eq!(reply["result"], result(true, selection));
    assert!(serde_json::to_vec(reply).unwrap().len() <= 4096);
}
fn history(o: &mut NativeObjectHistory, id: &str, kind: &str) {
    let mut q = request(o, id, json!({"command":format!("object.{kind}")}));
    q["operation"] = json!(format!("command.document.object.{kind}"));
    assert_eq!(dispatch(o, &q)["ok"], true);
}
#[test]
fn ordinary_rectangle_move_four_forms_checkpoints_once_and_restores_complete_storage_bytes() {
    for form in build::Build::ALL {
        let input = fixture::document(
            "K=a.1.10;M=b.2.20;.;N=c.3.30;I=d.4.40;K=e.5.50|K=x.6.60;K;K;K;K;K=z.7.70",
            form,
        );
        let mut o = owner(input.clone());
        let before = bytes(&o);
        let retained = o.acquire_snapshot(0).unwrap();
        let q = moving(
            &o,
            "rectangle",
            &[("L0", 0), ("L0", 1), ("L0", 2), ("L0", 3), ("L0", 4)],
            "L1",
            0,
        );
        let reply = dispatch(&mut o, &q);
        success(
            &reply,
            json!((0..5)
                .map(|frame| json!({"layerUid":"L1","frame":frame}))
                .collect::<Vec<_>>()),
        );
        assert_eq!(o.content_revision(), 1);
        assert_eq!(o.history_depths(), (1, 0));
        fixture::assert_rows(
            &input,
            &head(&o),
            ".;.;.;.;.;K=e.5.50|K=a.1.10;M=b.2.20;.;N=c.3.30;I=d.4.40;K=z.7.70",
            form,
        );
        let after = bytes(&o);
        let done = state(&o);
        assert_eq!(dispatch(&mut o, &q), reply);
        assert_eq!(state(&o), done);
        history(&mut o, "undo-rectangle", "undo");
        assert_eq!(bytes(&o), before);
        assert_eq!(o.history_depths(), (0, 1));
        history(&mut o, "redo-rectangle", "redo");
        assert_eq!(bytes(&o), after);
        assert_eq!(o.history_depths(), (1, 0));
        assert_eq!(
            bytes(&owner(decode_storage_project(&after).unwrap())),
            after
        );
        assert_eq!(encode_storage_project(retained.document()).unwrap(), before);
    }
}
#[test]
fn actual_move_selection_reports_ripple_clamp_and_crop_instead_of_requested_destination() {
    let mut o = owner(fixture::document(
        "K=a.1.10;.;K=b.2.20;.;K=c.3.30;.",
        build::Build::V2,
    ));
    let q = moving(&o, "ripple", &[("L0", 2)], "L0", 4);
    success(
        &dispatch(&mut o, &q),
        json!([{"layerUid":"L0","frame":3},{"layerUid":"L0","frame":5}]),
    );
    let input = fixture::document("K=z.1.10;K=a.2.20;K=b.3.30;.", build::Build::V1Ref);
    let mut o = owner(input.clone());
    let q = moving(&o, "crop", &[("L0", 1), ("L0", 2)], "L0", 3);
    success(&dispatch(&mut o, &q), json!([{"layerUid":"L0","frame":3}]));
    fixture::assert_rows(
        &input,
        &head(&o),
        "K=z.1.10;.;.;K=a.2.20",
        build::Build::V1Ref,
    );
}
#[test]
fn lock_filtered_move_can_commit_an_empty_actual_selection_when_only_eligible_source_is_cropped() {
    let mut input = fixture::document(
        "K=a.1.10;.;.;.|K=b.2.20;.;.;.|K=c.3.30;.;.;.",
        build::Build::V2,
    );
    input.layers[1].locked = Presence::Present(true);
    let mut o = owner(input.clone());
    let q = moving(&o, "locked", &[("L0", 0), ("L1", 0)], "L1", 1);
    let before = state(&o);
    assert_eq!(dispatch(&mut o, &q)["result"], result(false, Value::Null));
    assert_eq!(state(&o), before);
    let q = moving(
        &o,
        "crop-unlocked",
        &[("L0", 0), ("L1", 0), ("L2", 0)],
        "L1",
        1,
    );
    success(&dispatch(&mut o, &q), json!([]));
    assert_eq!(o.history_depths(), (1, 0));
    fixture::assert_rows(
        &input,
        &head(&o),
        "K=a.1.10;.;.;.|K=b.2.20;.;.;.|.;.;.;.",
        build::Build::V2,
    );
}
#[test]
fn post_ripple_zero_checkpoint_clears_redo_but_same_anchor_and_empty_move_preserve_it() {
    let mut o = owner(fixture::document(
        "K=a.1.10;.;K=b.2.20;.;K=c.3.30",
        build::Build::V1,
    ));
    let original = bytes(&o);
    let q = moving(&o, "zero-clamp", &[("L0", 2)], "L0", 3);
    success(
        &dispatch(&mut o, &q),
        json!([{"layerUid":"L0","frame":2},{"layerUid":"L0","frame":4}]),
    );
    assert_eq!(bytes(&o), original);
    assert_eq!(o.history_depths(), (1, 0));
    history(&mut o, "undo-zero", "undo");
    for (id, cells, frame) in [("same", vec![("L0", 2)], 2), ("empty", vec![], 0)] {
        let before = state(&o);
        let q = moving(&o, id, &cells, "L0", frame);
        let reply = dispatch(&mut o, &q);
        assert_eq!(reply["ok"], true);
        assert_eq!(reply["result"], result(false, Value::Null));
        assert_eq!(state(&o), before);
    }
    let q = moving(&o, "zero-after-undo", &[("L0", 2)], "L0", 3);
    assert_eq!(dispatch(&mut o, &q)["ok"], true);
    assert_eq!(o.history_depths(), (1, 0));
    assert_eq!(bytes(&o), original);
}
#[cfg(feature = "project_media_codec")]
#[test]
fn real_nonempty_raster_text_vector_move_keeps_exact_order_and_undo_redo_reopen() {
    let input = f7::mixed_items();
    let mut o = owner(input.clone());
    let before = bytes(&o);
    let q = moving(&o, "items", &[("L0", 0)], "L1", 1);
    success(&dispatch(&mut o, &q), json!([{"layerUid":"L1","frame":1}]));
    let after = bytes(&o);
    assert_eq!(
        fixture::canonical(fixture::value(&head(&o))),
        fixture::canonical(fixture::expected_mixed_move(&input))
    );
    history(&mut o, "undo-items", "undo");
    assert_eq!(bytes(&o), before);
    history(&mut o, "redo-items", "redo");
    assert_eq!(bytes(&o), after);
    assert_eq!(
        bytes(&owner(decode_storage_project(&after).unwrap())),
        after
    );
}
