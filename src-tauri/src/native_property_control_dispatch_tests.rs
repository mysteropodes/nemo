//! D18-B: expression-control declaration edits through the real subscribed native owner and public wire envelope;
//! undo/redo, replay, widget-axis refusal and reopen. Not installed UI proof.
use crate::application_mcp::revision_sync::dispatch_core::property_edit_tests::{
    history, open, receipt, saved,
};
use crate::application_mcp::ApplicationMcp;
use nemo_mcp::contract::{NativeApplicationRequest, NativeApplicationResponse};
use serde_json::{json, Value};

fn install(state: &ApplicationMcp) -> String {
    let document = json!({"format":"nemo.native-project-document","formatVersion":1,
        "project":{"canvasW":64,"canvasH":48,"fps":24},"viewportReference":null,"assets":[],
        "drawing":{"format":"nemo.native-object-document","formatVersion":1,"totalFrames":16,
        "layers":[{"layerUid":"L0"}],"objects":[]}});
    open(state, document.to_string())
}
fn request(
    state: &ApplicationMcp,
    id: &str,
    rid: &str,
    revision: u64,
    key: &str,
    action: Value,
) -> NativeApplicationRequest {
    serde_json::from_value(
        json!({"apiVersion":2,"requestId":rid,"instanceId":state.instance_id(),"documentId":id,
        "expectedRevision":revision,"operation":"command.document.object.property.edit",
        "payload":{"command":"object.property.edit",
        "target":{"contextId":"scene-root","layerUid":"L0"},"property":key,"action":action}}),
    )
    .unwrap()
}
fn dispatch(
    state: &ApplicationMcp,
    request: NativeApplicationRequest,
) -> NativeApplicationResponse {
    request.validate().unwrap();
    state.dispatch_native(request).unwrap()
}
fn holder(state: &ApplicationMcp, id: &str) -> Value {
    let document: Value = serde_json::from_slice(&saved(state, id)).unwrap();
    document["drawing"]["propertyHolders"][0].clone()
}
fn add(kind: &str, name: &str) -> Value {
    json!({"kind":"addControl","controlType":kind,"name":name})
}

#[test]
fn control_declaration_edits_use_the_native_owner_history_replay_and_reopen() {
    let state = ApplicationMcp::default();
    let id = install(&state);
    let initial = saved(&state, &id);
    let first = request(&state, &id, "add-a", 0, "xc_a", add("number", "  Speed "));
    let added = dispatch(&state, first.clone());
    receipt(&added, 1, true);
    assert_eq!(dispatch(&state, first), added, "retained receipt replays");
    receipt(
        &dispatch(
            &state,
            request(&state, &id, "add-b", 1, "xc_b", add("point", "Where")),
        ),
        2,
        true,
    );
    assert_eq!(
        holder(&state, &id)["controls"],
        json!([
            {"key":"xc_a","name":"Speed","type":"number","default":[0]},
            {"key":"xc_b","name":"Where","type":"point","default":[0,0]}
        ])
    );
    // A refused edit (name clash) publishes nothing and keeps the document byte for byte.
    let before = saved(&state, &id);
    let clash = dispatch(
        &state,
        request(&state, &id, "clash", 2, "xc_c", add("angle", " speed")),
    );
    assert!(!clash.ok);
    assert_eq!(clash.error.unwrap().code, "invalid_request");
    assert_eq!(saved(&state, &id), before);
    receipt(
        &dispatch(
            &state,
            request(
                &state,
                &id,
                "move",
                2,
                "xc_a",
                json!({"kind":"moveControl","direction":1}),
            ),
        ),
        3,
        true,
    );
    receipt(
        &dispatch(
            &state,
            request(
                &state,
                &id,
                "end",
                3,
                "xc_a",
                json!({"kind":"moveControl","direction":1}),
            ),
        ),
        3,
        false,
    );
    receipt(
        &dispatch(
            &state,
            request(
                &state,
                &id,
                "rename",
                3,
                "xc_b",
                json!({"kind":"renameControl","name":"Place"}),
            ),
        ),
        4,
        true,
    );
    receipt(
        &dispatch(
            &state,
            request(
                &state,
                &id,
                "remove",
                4,
                "xc_a",
                json!({"kind":"removeControl"}),
            ),
        ),
        5,
        true,
    );
    let changed = saved(&state, &id);
    assert_eq!(
        holder(&state, &id)["controls"],
        json!([{"key":"xc_b","name":"Place","type":"point","default":[0,0]}])
    );
    for (step, rid) in ["u1", "u2", "u3", "u4", "u5"].iter().enumerate() {
        assert!(history(&state, &id, rid, 5 + step as u64, false).ok);
    }
    assert_eq!(saved(&state, &id), initial, "five edits undo to the opening bytes");
    let reopened = ApplicationMcp::default();
    let next = open(&reopened, String::from_utf8(changed.clone()).unwrap());
    assert_eq!(saved(&reopened, &next), changed);
}
