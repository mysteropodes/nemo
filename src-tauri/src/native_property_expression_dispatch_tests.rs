//! #2376 real native owner/history over the authenticated wire: expression authoring edits and
//! the exact-source read. Not installed UI acceptance.
use crate::application_mcp::revision_sync::dispatch_core::property_edit_tests::{
    history, open, receipt, request, saved,
};
use crate::application_mcp::ApplicationMcp;
use nemo_mcp::contract::{NativeApplicationRequest, NativeApplicationResponse};
use serde_json::{json, Value};

const READ: &str = "query.document.object.property.read";
fn install(state: &ApplicationMcp) -> String {
    open(
        state,
        json!({"format":"nemo.native-project-document","formatVersion":1,
        "project":{"canvasW":64,"canvasH":48,"fps":24},"viewportReference":null,"assets":[],
        "drawing":{"format":"nemo.native-object-document","formatVersion":1,"totalFrames":16,
        "layers":[{"layerUid":"L0"}],"objects":[],"propertyHolders":[{
        "target":{"contextId":"scene-root","layerUid":"L0"},"properties":[{
        "property":"opacity","static":[17]}]}]}})
        .to_string(),
    )
}
fn dispatch(state: &ApplicationMcp, q: NativeApplicationRequest) -> NativeApplicationResponse {
    q.validate().unwrap();
    state.dispatch_native(q).unwrap()
}
fn read(state: &ApplicationMcp, id: &str, view: Value) -> NativeApplicationResponse {
    let info = state.native_project_info(id).unwrap();
    dispatch(
        state,
        serde_json::from_value(
            json!({"apiVersion":2,"requestId":"expression-read","instanceId":state.instance_id(),
        "documentId":id,"operation":READ,"payload":{"atRevision":info["contentRevision"],
        "documentSnapshotId":info["documentSnapshotId"],"view":view}}),
        )
        .unwrap(),
    )
}
fn source_view() -> Value {
    json!({"kind":"expressionSource","target":{"contextId":"scene-root","layerUid":"L0"},"property":"opacity"})
}
fn authored(state: &ApplicationMcp, id: &str) -> Value {
    let read = read(state, id, source_view());
    assert!(read.ok, "{read:?}");
    read.result.unwrap()["expression"].clone()
}

#[test]
fn expression_public_actions_use_the_native_owner_history_and_exact_source_read() {
    let state = ApplicationMcp::default();
    let id = install(&state);
    let initial = saved(&state, &id);
    assert_eq!(authored(&state, &id), Value::Null);
    // Invalid syntax and exact Unicode bytes stay authored data in both directions.
    let code = "return ( // café 🧭\r\n値";
    let set = request(
        &state,
        &id,
        "set",
        0,
        json!({"kind":"setExpression","code":code}),
    );
    let first = dispatch(&state, set.clone());
    receipt(&first, 1, true);
    assert_eq!(
        dispatch(&state, set.clone()),
        first,
        "replay answers the original receipt"
    );
    assert_eq!(authored(&state, &id), json!({"enabled":false,"code":code}));
    let project: Value = serde_json::from_slice(&saved(&state, &id)).unwrap();
    assert_eq!(
        project["drawing"]["propertyHolders"][0]["properties"][0]["expression"],
        json!({"enabled":false,"code":code})
    );
    let enable = request(
        &state,
        &id,
        "enable",
        1,
        json!({"kind":"setExpressionEnabled","enabled":true}),
    );
    receipt(&dispatch(&state, enable), 2, true);
    assert_eq!(authored(&state, &id), json!({"enabled":true,"code":code}));
    // Authoring never evaluates: the state read stays honestly "not evaluated" until the existing
    // owner publishes a real diagnostic receipt, so invalid syntax is never reported as success.
    let state_read = read(
        &state,
        &id,
        json!({"kind":"expressionState","target":{"contextId":"scene-root","layerUid":"L0"},"property":"opacity","frame":0}),
    );
    assert!(state_read.ok, "{state_read:?}");
    let result = state_read.result.unwrap();
    assert_eq!(result["expression"], json!({"present":true,"enabled":true}));
    assert_eq!(
        result["evaluation"],
        json!({"state":"unavailable","reason":"not-evaluated"})
    );
    // An empty code is present-but-empty; removal is a separate intent.
    receipt(
        &dispatch(
            &state,
            request(
                &state,
                &id,
                "empty",
                2,
                json!({"kind":"setExpression","code":""}),
            ),
        ),
        3,
        true,
    );
    assert_eq!(authored(&state, &id), json!({"enabled":true,"code":""}));
    receipt(
        &dispatch(
            &state,
            request(&state, &id, "remove", 3, json!({"kind":"removeExpression"})),
        ),
        4,
        true,
    );
    assert_eq!(authored(&state, &id), Value::Null);
    receipt(
        &dispatch(
            &state,
            request(
                &state,
                &id,
                "remove-again",
                4,
                json!({"kind":"removeExpression"}),
            ),
        ),
        4,
        false,
    );
    let removed = saved(&state, &id);
    assert!(history(&state, &id, "undo", 4, false).ok);
    assert_eq!(authored(&state, &id), json!({"enabled":true,"code":""}));
    let redo = history(&state, &id, "redo", 5, true);
    assert!(redo.ok);
    assert_eq!(
        saved(&state, &id),
        removed,
        "the no-op removal preserved redo"
    );
    assert_eq!(removed, initial, "removal returns the exact authored bytes");
    let reopened = ApplicationMcp::default();
    let next = open(&reopened, String::from_utf8(removed.clone()).unwrap());
    assert_eq!(saved(&reopened, &next), removed);
}

#[test]
fn expression_public_refusals_are_typed_and_leave_the_document_untouched() {
    let state = ApplicationMcp::default();
    let id = install(&state);
    receipt(
        &dispatch(
            &state,
            request(
                &state,
                &id,
                "seed",
                0,
                json!({"kind":"setExpression","code":"value"}),
            ),
        ),
        1,
        true,
    );
    let before = saved(&state, &id);
    let mut cases = vec![(
        request(&state, &id, "stale", 0, json!({"kind":"removeExpression"})),
        "stale_revision",
    )];
    let mut missing = request(
        &state,
        &id,
        "wrong-holder",
        1,
        json!({"kind":"setExpression","code":"x"}),
    );
    missing.payload["target"]["layerUid"] = json!("absent");
    cases.push((missing, "not_found"));
    for name in ["order", "timeRemap", "not-a-builtin"] {
        let mut r = request(
            &state,
            &id,
            name,
            1,
            json!({"kind":"setExpression","code":"x"}),
        );
        r.payload["property"] = json!(name);
        cases.push((r, "unavailable"));
    }
    for (r, code) in cases {
        let response = dispatch(&state, r);
        assert!(!response.ok, "{code}");
        assert_eq!(response.error.unwrap().code, code);
        assert_eq!(saved(&state, &id), before);
    }
    // Over the code ceiling the request cannot even be admitted; nothing is truncated.
    let oversize = request(
        &state,
        &id,
        "oversize",
        1,
        json!({"kind":"setExpression","code":"x".repeat(4097)}),
    );
    assert!(oversize.validate().is_err());
    assert_eq!(saved(&state, &id), before);
    assert_eq!(
        authored(&state, &id),
        json!({"enabled":false,"code":"value"})
    );
    // A source that does not fit the bounded read result is refused, never trimmed.
    receipt(
        &dispatch(
            &state,
            request(
                &state,
                &id,
                "big",
                1,
                json!({"kind":"setExpression","code":"y".repeat(3500)}),
            ),
        ),
        2,
        true,
    );
    let big = read(&state, &id, source_view());
    assert!(!big.ok);
    assert_eq!(big.error.unwrap().code, "unavailable");
}
