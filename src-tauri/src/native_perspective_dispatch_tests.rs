//! Original Perspective gestures over authenticated MCP with native history and ACK.
//! In-memory host tests; assisted drawing, rendering and installed acceptance differ.
use crate::application_mcp::revision_sync::dispatch_core::property_edit_tests::{
    history, open, receipt, round_trip, saved,
};
use crate::application_mcp::{revision_sync::RevisionControl, ApplicationMcp};
use nemo_mcp::contract::{NativeApplicationRequest, NativeApplicationResponse};
use serde_json::{json, Value};

fn document(extra: Value) -> String {
    let mut project = json!({
        "canvasW":100,"canvasH":80,"fps":24,"workspace":{"panelOrder":null},
        "guides":{"h":[-0.25]},"comments":[{"text":"Keep","authorName":"Writer"}],
        "shadowPalette":[{"id":"same","color":"#AbC"},{"id":"same","color":"#abc"}]
    });
    project
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    json!({
        "format":"nemo.native-project-document","formatVersion":1,
        "project":project,"viewportReference":null,"assets":[],
        "drawing":{"format":"nemo.native-object-document","formatVersion":1,
            "totalFrames":40,"layers":[{"layerUid":"L0"}],"objects":[],
            "propertyHolders":[{"target":{"contextId":"scene-root","layerUid":"L0"},
                "properties":[{"property":"opacity","static":[17]}]}]}
    })
    .to_string()
}
fn edit(
    state: &ApplicationMcp,
    id: &str,
    name: &str,
    revision: u64,
    action: Value,
) -> NativeApplicationRequest {
    serde_json::from_value(json!({
        "apiVersion":2,"requestId":name,"instanceId":state.instance_id(),
        "documentId":id,"expectedRevision":revision,
        "operation":"command.document.object.project.perspective.edit",
        "payload":{"command":"object.project.perspective.edit","action":action}
    }))
    .unwrap()
}
fn read(state: &ApplicationMcp, id: &str, name: &str, selector: Value) -> NativeApplicationRequest {
    let pin = state.native_project_info(id).unwrap();
    serde_json::from_value(json!({
        "apiVersion":2,"requestId":name,"instanceId":state.instance_id(),
        "documentId":id,"operation":"query.document.object.project.perspective.read",
        "payload":{"atRevision":pin["contentRevision"],
            "documentSnapshotId":pin["documentSnapshotId"],"contextId":"scene-root",
            "selector":selector}
    }))
    .unwrap()
}
fn subscribe(state: &ApplicationMcp) -> Value {
    let binding = state
        .revisions
        .control(&state.native, RevisionControl::Binding)
        .unwrap();
    state
        .revisions
        .control(
            &state.native,
            serde_json::from_value(json!({"action":"subscribe","binding":binding})).unwrap(),
        )
        .unwrap();
    binding
}
fn project(state: &ApplicationMcp, id: &str) -> Value {
    serde_json::from_slice::<Value>(&saved(state, id)).unwrap()["project"].clone()
}
async fn change(
    state: &ApplicationMcp,
    id: &str,
    binding: &Value,
    revision: u64,
    action: Value,
) -> NativeApplicationResponse {
    let request = edit(
        state,
        id,
        &format!("perspective-step-{revision}"),
        revision,
        action,
    );
    request.validate().unwrap();
    // Existing helper checks native commit-before-event, bad ACK and pending-write refusal.
    let (response, events) = round_trip(state, request, Some(binding)).await;
    receipt(&response, revision + 1, true);
    assert_eq!(events, 1);
    response
}

#[tokio::test]
async fn perspective_six_original_gestures_use_authenticated_ack_history_replay_and_fresh_owner() {
    let state = ApplicationMcp::default();
    let id = open(
        &state,
        document(json!({"perspectiveMode":"imported-unknown","perspectiveDensity":-3.5})),
    );
    let initial = saved(&state, &id);
    let enable = edit(
        &state,
        &id,
        "perspective-enable",
        0,
        json!({"kind":"setEnabled","enabled":true}),
    );
    enable.validate().unwrap();
    let (refused, events) = round_trip(&state, enable.clone(), None).await;
    assert!(!refused.ok);
    assert_eq!(refused.error.unwrap().code, "unavailable");
    assert_eq!(refused.content_revision, 0);
    assert_eq!(events, 0);
    assert_eq!(saved(&state, &id), initial);
    let binding = subscribe(&state);
    let (first, events) = round_trip(&state, enable.clone(), Some(&binding)).await;
    receipt(&first, 1, true);
    assert_eq!(events, 1);
    assert_eq!(project(&state, &id)["perspectiveMode"], "imported-unknown");
    assert_eq!(
        project(&state, &id)["perspectiveVPs"],
        json!([
            {"x":-15.0,"y":40.0,"locked":false},{"x":115.0,"y":40.0,"locked":false}
        ])
    );
    let old_page = read(
        &state,
        &id,
        "perspective-old-pin",
        json!({"kind":"vpPage","cursor":0,"limit":16}),
    );
    old_page.validate().unwrap();
    let (page, events) = round_trip(&state, old_page.clone(), None).await;
    assert!(page.ok, "{page:?}");
    assert_eq!(events, 0);
    assert_eq!(page.content_revision, 1);
    let result = page.result.unwrap();
    assert_eq!(
        result["documentSnapshotId"],
        old_page.payload["documentSnapshotId"]
    );
    assert_eq!(result["result"]["vpSource"], "stored");
    assert_eq!(result["result"]["effectiveVPCount"], 2);
    change(
        &state,
        &id,
        &binding,
        1,
        json!({"kind":"setMode","mode":"3pt"}),
    )
    .await;
    let defaults = json!([
        {"x":-10.0,"y":28.0,"locked":false},{"x":110.0,"y":28.0,"locked":false},
        {"x":50.0,"y":144.0,"locked":false}
    ]);
    assert_eq!(project(&state, &id)["perspectiveVPs"], defaults);
    change(
        &state,
        &id,
        &binding,
        2,
        json!({"kind":"lockAll","locked":true}),
    )
    .await;
    let locked = saved(&state, &id);
    for (name, action) in [
        (
            "locked-move",
            json!({"kind":"move","ordinal":1,"x":99.0,"y":99.0}),
        ),
        (
            "locked-translate",
            json!({"kind":"translate","dx":5.5,"dy":-8.25}),
        ),
    ] {
        let (noop, events) = round_trip(&state, edit(&state, &id, name, 3, action), None).await;
        receipt(&noop, 3, false);
        assert_eq!(events, 0);
        assert_eq!(saved(&state, &id), locked);
    }
    change(
        &state,
        &id,
        &binding,
        3,
        json!({"kind":"lockAll","locked":false}),
    )
    .await;
    change(
        &state,
        &id,
        &binding,
        4,
        json!({"kind":"move","ordinal":1,"x":-2.25,"y":3.5}),
    )
    .await;
    change(
        &state,
        &id,
        &binding,
        5,
        json!({"kind":"translate","dx":-3.25,"dy":4.5}),
    )
    .await;
    assert_eq!(
        project(&state, &id)["perspectiveVPs"],
        json!([
            {"x":-5.5,"y":8.0,"locked":false},{"x":106.75,"y":32.5,"locked":false},
            {"x":46.75,"y":148.5,"locked":false}
        ])
    );
    let before_reset = saved(&state, &id);
    change(&state, &id, &binding, 6, json!({"kind":"reset"})).await;
    let changed = saved(&state, &id);
    let mut next: Value = serde_json::from_slice(&changed).unwrap();
    assert_eq!(next["project"]["perspectiveVPs"], defaults);
    assert_eq!(next["project"]["perspectiveDensity"], -3.5);
    assert_eq!(next["project"]["perspectiveMode"], "3pt");
    next["project"]
        .as_object_mut()
        .unwrap()
        .remove("perspectiveVPs");
    next["project"]
        .as_object_mut()
        .unwrap()
        .remove("perspectiveEnabled");
    next["project"]["perspectiveMode"] = json!("imported-unknown");
    assert_eq!(next, serde_json::from_slice::<Value>(&initial).unwrap());
    let (replay, events) = round_trip(&state, enable, None).await;
    assert_eq!(replay, first);
    assert_eq!(events, 0);
    let (stale, events) = round_trip(&state, old_page, None).await;
    assert!(!stale.ok);
    assert_eq!(stale.error.unwrap().code, "stale_revision");
    assert_eq!(events, 0);
    let stale = edit(&state, &id, "stale-perspective", 0, json!({"kind":"reset"}));
    let (refused, events) = round_trip(&state, stale, None).await;
    assert!(!refused.ok);
    assert_eq!(refused.error.unwrap().code, "stale_revision");
    assert_eq!(events, 0);
    assert_eq!(saved(&state, &id), changed);
    assert!(history(&state, &id, "perspective-undo-reset", 7, false).ok);
    assert_eq!(saved(&state, &id), before_reset);
    let (noop, events) = round_trip(
        &state,
        edit(
            &state,
            &id,
            "zero-translate",
            8,
            json!({"kind":"translate","dx":0,"dy":0}),
        ),
        None,
    )
    .await;
    receipt(&noop, 8, false);
    assert_eq!(events, 0);
    assert!(history(&state, &id, "perspective-redo-reset", 8, true).ok);
    assert_eq!(saved(&state, &id), changed);
    for revision in 9..16 {
        assert!(history(&state, &id, &format!("undo-{revision}"), revision, false).ok);
    }
    assert_eq!(
        saved(&state, &id),
        initial,
        "seven changed gestures made seven checkpoints"
    );
    let fresh = ApplicationMcp::default();
    let next_id = open(&fresh, String::from_utf8(changed.clone()).unwrap());
    assert_eq!(saved(&fresh, &next_id), changed);
}

#[path = "native_perspective_read_dispatch_tests.rs"]
mod projection_tests;
