//! Original workspace record over the authenticated desktop owner and real ACK route.
//! These in-memory tests do not establish UI gestures, picker or installed behavior.
use crate::application_mcp::revision_sync::dispatch_core::property_edit_tests::{
    history, open, receipt, round_trip, saved,
};
use crate::application_mcp::{revision_sync::RevisionControl, ApplicationMcp};
use nemo_mcp::contract::NativeApplicationRequest;
use serde_json::{json, Value};

fn document(workspace: Option<Value>) -> String {
    let mut project = json!({"canvasW":64,"canvasH":48,"fps":24,
        "comments":[{"id":"same","text":"Keep","authorName":""},{"id":"same"},{}],
        "customBrushPresets":{"10":{"label":""},"2":{}},
        "perspectiveMode":"unknown","perspectiveDensity":-3.5,
        "shadowPalette":[{"id":"same","color":"#AbC"},{"id":"same","color":"#abc"}]});
    if let Some(workspace) = workspace {
        project["workspace"] = workspace;
    }
    json!({"format":"nemo.native-project-document","formatVersion":1,
        "project":project,"viewportReference":null,"assets":[],
        "drawing":{"format":"nemo.native-object-document","formatVersion":1,
            "totalFrames":40,"layers":[{"layerUid":"L0"}],"objects":[]}})
    .to_string()
}
fn capture(width: &str) -> Value {
    json!({"layerPanelWidth":width,"toolsPanelWidth":null,"propsPanelWidth":"",
        "propsPanelCollapsed":false,"timelineHeight":"-3.5px",
        "panelOrder":["unknown","unknown",""],"motionColumns":null})
}
fn edit(
    state: &ApplicationMcp,
    id: &str,
    name: &str,
    revision: u64,
    record: Value,
) -> NativeApplicationRequest {
    serde_json::from_value(json!({"apiVersion":2,"requestId":name,"instanceId":state.instance_id(),
        "documentId":id,"expectedRevision":revision,
        "operation":"command.document.object.project.workspace.edit",
        "payload":{"command":"object.project.workspace.edit","action":{"kind":"capture","workspace":record}}})).unwrap()
}
fn read(state: &ApplicationMcp, id: &str, name: &str) -> NativeApplicationRequest {
    let pin = state.native_project_info(id).unwrap();
    serde_json::from_value(json!({"apiVersion":2,"requestId":name,"instanceId":state.instance_id(),
        "documentId":id,"operation":"query.document.object.project.workspace.read",
        "payload":{"atRevision":pin["contentRevision"],"documentSnapshotId":pin["documentSnapshotId"],
            "contextId":"scene-root","selector":{"kind":"record"}}})).unwrap()
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
            serde_json::from_value(json!({
        "action":"subscribe","binding":binding}))
            .unwrap(),
        )
        .unwrap();
    binding
}
fn assert_only_workspace(before: &[u8], after: &[u8], expected: &Value) {
    let original: Value = serde_json::from_slice(before).unwrap();
    let mut next: Value = serde_json::from_slice(after).unwrap();
    assert_eq!(&next["project"]["workspace"], expected);
    if let Some(old) = original["project"].get("workspace") {
        next["project"]["workspace"] = old.clone();
    } else {
        next["project"].as_object_mut().unwrap().remove("workspace");
    }
    assert_eq!(next, original);
}

#[tokio::test]
async fn workspace_capture_wire_ack_replay_noop_redo_and_fresh_owner_preserve_other_fields() {
    let state = ApplicationMcp::default();
    let original_record = capture("120px");
    let id = open(&state, document(Some(original_record.clone())));
    let initial = saved(&state, &id);
    let next_record = capture("240.5px");
    let command = edit(&state, &id, "workspace-capture", 0, next_record.clone());
    command.validate().unwrap();
    let (refused, events) = round_trip(&state, command.clone(), None).await;
    assert_eq!(refused.error.unwrap().code, "unavailable");
    assert_eq!(refused.content_revision, 0);
    assert_eq!(events, 0);
    assert_eq!(saved(&state, &id), initial);
    let binding = subscribe(&state);
    // The existing helper authenticates TCP, checks commit-before-event, rejects a bad
    // ACK and a second pending write, then settles the correlated ACK exactly once.
    let (first, events) = round_trip(&state, command.clone(), Some(&binding)).await;
    receipt(&first, 1, true);
    assert_eq!(events, 1);
    assert_eq!(first.request_id, command.request_id);
    assert_eq!(first.instance_id, command.instance_id);
    assert_eq!(first.document_id, command.document_id);
    let changed = saved(&state, &id);
    assert_only_workspace(&initial, &changed, &next_record);
    let pinned = read(&state, &id, "workspace-pinned-read");
    pinned.validate().unwrap();
    let (response, events) = round_trip(&state, pinned.clone(), None).await;
    assert!(response.ok, "{response:?}");
    assert_eq!(events, 0);
    let result = response.result.unwrap();
    assert_eq!(
        result["documentSnapshotId"],
        pinned.payload["documentSnapshotId"]
    );
    assert_eq!(result["atRevision"], 1);
    assert_eq!(
        result["result"],
        json!({"workspacePresent":true,"workspace":next_record})
    );
    let (replay, events) = round_trip(&state, command, None).await;
    assert_eq!(replay, first);
    assert_eq!(events, 0);
    assert!(history(&state, &id, "workspace-undo", 1, false).ok);
    assert_eq!(saved(&state, &id), initial);
    let (noop, events) = round_trip(
        &state,
        edit(&state, &id, "workspace-same", 2, original_record),
        None,
    )
    .await;
    receipt(&noop, 2, false);
    assert_eq!(events, 0);
    assert!(history(&state, &id, "workspace-redo", 2, true).ok);
    assert_eq!(saved(&state, &id), changed);
    let (stale, events) = round_trip(&state, pinned, None).await;
    assert_eq!(stale.error.unwrap().code, "stale_revision");
    assert_eq!(events, 0);
    let fresh = ApplicationMcp::default();
    let reopened = open(&fresh, String::from_utf8(changed.clone()).unwrap());
    assert_eq!(saved(&fresh, &reopened), changed);
}

#[tokio::test]
async fn workspace_absent_partial_projection_and_refusals_stay_pure_until_explicit_capture() {
    for workspace in [
        None,
        Some(json!({})),
        Some(json!({"layerPanelWidth":null,
        "toolsPanelWidth":"","panelOrder":[],"motionColumns":""})),
    ] {
        let state = ApplicationMcp::default();
        let id = open(&state, document(workspace.clone()));
        let initial = saved(&state, &id);
        let pinned = read(&state, &id, "workspace-raw");
        let (raw, events) = round_trip(&state, pinned.clone(), None).await;
        assert!(raw.ok, "{raw:?}");
        assert_eq!(events, 0);
        let raw = raw.result.unwrap()["result"].clone();
        assert_eq!(raw["workspacePresent"], workspace.is_some());
        assert_eq!(raw.get("workspace"), workspace.as_ref());
        assert_eq!(saved(&state, &id), initial);
        let binding = subscribe(&state);
        for (name, field, code) in [
            ("document", "documentId", "wrong_document"),
            ("stale", "expectedRevision", "stale_revision"),
        ] {
            let mut request =
                serde_json::to_value(edit(&state, &id, name, 0, capture("1px"))).unwrap();
            request[field] = if field == "expectedRevision" {
                json!(1)
            } else {
                json!("foreign")
            };
            let (response, events) =
                round_trip(&state, serde_json::from_value(request).unwrap(), None).await;
            assert_eq!(response.error.unwrap().code, code);
            assert_eq!(events, 0);
            assert_eq!(saved(&state, &id), initial);
        }
        // This negative owner-refusal control uses the existing host path directly.
        // The strict MCP client rejects mismatched instance correlation as transport failure.
        let mut foreign_instance = edit(&state, &id, "foreign-instance", 0, capture("1px"));
        foreign_instance.instance_id = "foreign".into();
        let refused = state.dispatch_native(foreign_instance).unwrap();
        assert!(!refused.ok);
        assert_eq!(refused.error.unwrap().code, "wrong_instance");
        assert_eq!(saved(&state, &id), initial);
        let mut cancelled = edit(&state, &id, "workspace-cancelled", 0, capture("1px"));
        cancelled.cancelled_before_dispatch = true;
        let (refused, events) = round_trip(&state, cancelled, None).await;
        assert_eq!(refused.error.unwrap().code, "cancelled_before_dispatch");
        assert_eq!(events, 0);
        assert_eq!(saved(&state, &id), initial);
        let mut record = capture("0px");
        record.as_object_mut().unwrap().remove("motionColumns");
        let (captured, events) = round_trip(
            &state,
            edit(&state, &id, "workspace-explicit", 0, record.clone()),
            Some(&binding),
        )
        .await;
        receipt(&captured, 1, true);
        assert_eq!(events, 1);
        let changed = saved(&state, &id);
        assert_only_workspace(&initial, &changed, &record);
        assert!(history(&state, &id, "workspace-restore", 1, false).ok);
        assert_eq!(saved(&state, &id), initial);
        assert!(history(&state, &id, "workspace-reapply", 2, true).ok);
        assert_eq!(saved(&state, &id), changed);
        let fresh = ApplicationMcp::default();
        let next = open(&fresh, String::from_utf8(changed.clone()).unwrap());
        assert_eq!(saved(&fresh, &next), changed);
    }
}
