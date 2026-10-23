//! Original project team comments over real authenticated MCP and native history.
//! Feedback submission, rendering and installed popover behavior are not exercised.
use crate::application_mcp::revision_sync::dispatch_core::property_edit_tests::{
    history, open, receipt, round_trip, saved,
};
use crate::application_mcp::{revision_sync::RevisionControl, ApplicationMcp};
use nemo_mcp::contract::NativeApplicationRequest;
use serde_json::{json, Value};

fn document(comments: Option<Value>) -> String {
    let mut project = json!({"canvasW":64,"canvasH":48,"fps":24,
        "workspace":{"panelOrder":null,"motionColumns":""},
        "customBrushPresets":{"10":{"label":""},"2":{"markerTip":false}},
        "perspectiveMode":"unknown","perspectiveDensity":-3.5,
        "shadowPalette":[{"id":"same","color":"#AbC"},{"id":"same","color":"#abc"}]});
    if let Some(comments) = comments {
        project["comments"] = comments;
    }
    json!({"format":"nemo.native-project-document","formatVersion":1,
        "project":project,"viewportReference":null,"assets":[],
        "drawing":{"format":"nemo.native-object-document","formatVersion":1,
            "totalFrames":40,"layers":[{"layerUid":"L0"}],"objects":[]}})
    .to_string()
}
fn draft() -> Value {
    json!({"id":"same","x":-0.25,"y":0.5,"frame":-1.5,"text":"Note\n日本語",
        "createdAt":-2.25,"resolved":false,"authorId":"\u{0000}raw",
        "authorName":"","authorColor":"#AbC"})
}
fn edit(
    state: &ApplicationMcp,
    id: &str,
    name: &str,
    revision: u64,
    action: Value,
) -> NativeApplicationRequest {
    serde_json::from_value(
        json!({"apiVersion":2,"requestId":name,"instanceId":state.instance_id(),
        "documentId":id,"expectedRevision":revision,
        "operation":"command.document.object.project.comments.edit",
        "payload":{"command":"object.project.comments.edit","action":action}}),
    )
    .unwrap()
}
fn read(state: &ApplicationMcp, id: &str, name: &str, selector: Value) -> NativeApplicationRequest {
    let pin = state.native_project_info(id).unwrap();
    serde_json::from_value(json!({"apiVersion":2,"requestId":name,"instanceId":state.instance_id(),
        "documentId":id,"operation":"query.document.object.project.comments.read",
        "payload":{"atRevision":pin["contentRevision"],"documentSnapshotId":pin["documentSnapshotId"],
            "contextId":"scene-root","selector":selector}})).unwrap()
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
async fn change(state: &ApplicationMcp, id: &str, binding: &Value, revision: u64, action: Value) {
    let request = edit(
        state,
        id,
        &format!("comments-step-{revision}"),
        revision,
        action,
    );
    request.validate().unwrap();
    let (response, events) = round_trip(state, request, Some(binding)).await;
    receipt(&response, revision + 1, true);
    assert_eq!(events, 1);
}
fn comments(state: &ApplicationMcp, id: &str) -> Value {
    serde_json::from_slice::<Value>(&saved(state, id)).unwrap()["project"]["comments"].clone()
}

#[tokio::test]
async fn comments_wire_ack_empty_save_resolve_append_and_ordinal_delete_preserve_duplicate_identity(
) {
    let state = ApplicationMcp::default();
    let original_comments = json!([{"id":"same","authorName":"","authorColor":"raw"},
        {"id":"same","text":"second","resolved":false},{}]);
    let id = open(&state, document(Some(original_comments.clone())));
    let initial = saved(&state, &id);
    let command = edit(
        &state,
        &id,
        "comments-save-existing",
        0,
        json!({"kind":"update","ordinal":2,"text":"","resolved":true}),
    );
    command.validate().unwrap();
    let (refused, events) = round_trip(&state, command.clone(), None).await;
    assert_eq!(refused.error.unwrap().code, "unavailable");
    assert_eq!(refused.content_revision, 0);
    assert_eq!(events, 0);
    assert_eq!(saved(&state, &id), initial);
    let binding = subscribe(&state);
    // This actual helper observes native commit before the event, rejects bad ACK
    // and a pending second write, then settles exactly one correlated event.
    let (first, events) = round_trip(&state, command.clone(), Some(&binding)).await;
    receipt(&first, 1, true);
    assert_eq!(events, 1);
    assert_eq!(first.request_id, command.request_id);
    assert_eq!(first.instance_id, command.instance_id);
    assert_eq!(first.document_id, command.document_id);
    let expected = json!([{"id":"same","authorName":"","authorColor":"raw"},
        {"id":"same","text":"","resolved":true},{}]);
    assert_eq!(comments(&state, &id), expected);
    let pinned = read(
        &state,
        &id,
        "comments-pinned",
        json!({"kind":"commentsPage","cursor":0,"limit":16}),
    );
    pinned.validate().unwrap();
    let (page, events) = round_trip(&state, pinned.clone(), None).await;
    assert!(page.ok, "{page:?}");
    assert_eq!(events, 0);
    let result = page.result.unwrap();
    assert_eq!(
        result["documentSnapshotId"],
        pinned.payload["documentSnapshotId"]
    );
    assert_eq!(result["atRevision"], 1);
    assert_eq!(
        result["result"]["items"],
        json!([
        {"ordinal":1,"comment":expected[0]},{"ordinal":2,"comment":expected[1]},
        {"ordinal":3,"comment":{}}])
    );
    change(
        &state,
        &id,
        &binding,
        1,
        json!({"kind":"append","comment":draft()}),
    )
    .await;
    assert_eq!(comments(&state, &id)[3], draft());
    let appended = saved(&state, &id);
    change(
        &state,
        &id,
        &binding,
        2,
        json!({"kind":"delete","ordinal":2}),
    )
    .await;
    assert_eq!(comments(&state, &id), json!([expected[0], {}, draft()]));
    let changed = saved(&state, &id);
    let original: Value = serde_json::from_slice(&initial).unwrap();
    let mut next: Value = serde_json::from_slice(&changed).unwrap();
    next["project"]["comments"] = original["project"]["comments"].clone();
    assert_eq!(next, original);
    let (replay, events) = round_trip(&state, command, None).await;
    assert_eq!(replay, first);
    assert_eq!(events, 0);
    let (stale, events) = round_trip(&state, pinned, None).await;
    assert_eq!(stale.error.unwrap().code, "stale_revision");
    assert_eq!(events, 0);
    assert!(history(&state, &id, "comments-undo-delete", 3, false).ok);
    assert_eq!(saved(&state, &id), appended);
    let (noop, events) = round_trip(
        &state,
        edit(
            &state,
            &id,
            "comments-same",
            4,
            json!({"kind":"update","ordinal":2,"text":"","resolved":true}),
        ),
        None,
    )
    .await;
    receipt(&noop, 4, false);
    assert_eq!(events, 0);
    assert!(history(&state, &id, "comments-redo-delete", 4, true).ok);
    assert_eq!(saved(&state, &id), changed);
    let fresh = ApplicationMcp::default();
    let reopened = open(&fresh, String::from_utf8(changed.clone()).unwrap());
    assert_eq!(saved(&fresh, &reopened), changed);
}

#[tokio::test]
async fn comments_absent_empty_projection_and_authenticated_refusals_preserve_raw_attribution() {
    for existing in [None, Some(json!([]))] {
        let state = ApplicationMcp::default();
        let id = open(&state, document(existing.clone()));
        let initial = saved(&state, &id);
        let (settings, events) = round_trip(
            &state,
            read(&state, &id, "comments-settings", json!({"kind":"settings"})),
            None,
        )
        .await;
        assert!(settings.ok, "{settings:?}");
        assert_eq!(events, 0);
        assert_eq!(
            settings.result.unwrap()["result"],
            json!({"commentsPresent":existing.is_some(),"commentCount":0})
        );
        let (page, events) = round_trip(
            &state,
            read(
                &state,
                &id,
                "comments-empty-page",
                json!({"kind":"commentsPage","cursor":0,"limit":16}),
            ),
            None,
        )
        .await;
        assert!(page.ok, "{page:?}");
        assert_eq!(events, 0);
        assert_eq!(page.result.unwrap()["result"]["items"], json!([]));
        let (missing, events) = round_trip(
            &state,
            read(
                &state,
                &id,
                "missing-comment",
                json!({"kind":"record","ordinal":1}),
            ),
            None,
        )
        .await;
        assert_eq!(missing.error.unwrap().code, "not_found");
        assert_eq!(events, 0);
        assert_eq!(saved(&state, &id), initial);
        let binding = subscribe(&state);
        let action = json!({"kind":"append","comment":draft()});
        for (name, field, code) in [
            ("document", "documentId", "wrong_document"),
            ("stale", "expectedRevision", "stale_revision"),
        ] {
            let mut request =
                serde_json::to_value(edit(&state, &id, name, 0, action.clone())).unwrap();
            request[field] = if field == "expectedRevision" {
                json!(1)
            } else {
                json!("foreign")
            };
            let (refused, events) =
                round_trip(&state, serde_json::from_value(request).unwrap(), None).await;
            assert_eq!(refused.error.unwrap().code, code);
            assert_eq!(events, 0);
            assert_eq!(saved(&state, &id), initial);
        }
        // This negative owner-refusal control uses the existing host path directly.
        // The strict MCP client rejects mismatched instance correlation as transport failure.
        let mut foreign_instance = edit(&state, &id, "foreign-instance", 0, action.clone());
        foreign_instance.instance_id = "foreign".into();
        let refused = state.dispatch_native(foreign_instance).unwrap();
        assert!(!refused.ok);
        assert_eq!(refused.error.unwrap().code, "wrong_instance");
        assert_eq!(saved(&state, &id), initial);
        let mut cancelled = edit(&state, &id, "comments-cancelled", 0, action.clone());
        cancelled.cancelled_before_dispatch = true;
        let (refused, events) = round_trip(&state, cancelled, None).await;
        assert_eq!(refused.error.unwrap().code, "cancelled_before_dispatch");
        assert_eq!(events, 0);
        assert_eq!(saved(&state, &id), initial);
        change(&state, &id, &binding, 0, action).await;
        let (record, events) = round_trip(
            &state,
            read(
                &state,
                &id,
                "comment-record",
                json!({"kind":"record","ordinal":1}),
            ),
            None,
        )
        .await;
        assert!(record.ok, "{record:?}");
        assert_eq!(events, 0);
        assert_eq!(
            record.result.unwrap()["result"],
            json!({"kind":"projectCommentRecord","ordinal":1,"comment":draft()})
        );
        change(
            &state,
            &id,
            &binding,
            1,
            json!({"kind":"update","ordinal":1,"text":" \n","resolved":true}),
        )
        .await;
        let mut expected = draft();
        expected["text"] = json!(" \n");
        expected["resolved"] = json!(true);
        assert_eq!(comments(&state, &id), json!([expected]));
        let updated = saved(&state, &id);
        change(
            &state,
            &id,
            &binding,
            2,
            json!({"kind":"delete","ordinal":1}),
        )
        .await;
        assert_eq!(comments(&state, &id), json!([]));
        for revision in 3..6 {
            assert!(
                history(
                    &state,
                    &id,
                    &format!("comments-restore-{revision}"),
                    revision,
                    false
                )
                .ok
            );
        }
        assert_eq!(saved(&state, &id), initial);
        let fresh = ApplicationMcp::default();
        let reopened = open(&fresh, String::from_utf8(updated.clone()).unwrap());
        assert_eq!(saved(&fresh, &reopened), updated);
    }
}
