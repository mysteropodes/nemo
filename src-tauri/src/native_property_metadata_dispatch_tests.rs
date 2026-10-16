//! Metadata reads use actual common capability admission and native owner locks.
use crate::application_mcp::revision_sync::Delivery;
use nemo_mcp::contract::NativeApplicationRequest;
use crate::application_mcp::ApplicationMcp;
use crate::native_object_bootstrap::{bootstrap_raw, replace_blank_without_viewport_for_test};
use serde_json::{json, Value};
fn new_project(state: &ApplicationMcp, replace: Option<Value>) -> String {
    let mut q = json!({"apiVersion":2,"requestId":"metadata-install","instanceId":state.instance_id(),"newProject":{"canvasW":64,"canvasH":48,"fps":24.0}});
    let receipt = if let Some(replace) = replace {
        q["requestId"] = json!("metadata-replace");
        q["replace"] = replace;
        let raw = q.to_string();
        assert_eq!(bootstrap_raw(state, &raw).unwrap_err().code, "invalid_request");
        replace_blank_without_viewport_for_test(state, &raw).unwrap()
    } else {
        bootstrap_raw(state, &q.to_string()).unwrap()
    };
    let receipt = serde_json::to_value(receipt).unwrap();
    receipt["documentId"].as_str().unwrap().into()
}
fn query(state: &ApplicationMcp, id: &str) -> NativeApplicationRequest {
    let info = state.native_project_info(id).unwrap();
    serde_json::from_value(json!({"apiVersion":2,"requestId":"metadata-read","instanceId":state.instance_id(),"documentId":id,
      "operation":"query.document.object.property.metadata","payload":{"atRevision":info["contentRevision"],"documentSnapshotId":info["documentSnapshotId"],"view":{"kind":"targets","offset":0,"limit":8}}})).unwrap()
}
fn invoke(state: &ApplicationMcp, q: NativeApplicationRequest, external: bool) -> Delivery {
    q.validate().unwrap();
    state
        .revisions
        .dispatch_admitted(state.instance_id(), &state.native, q, external, false)
        .unwrap()
}
#[test]
fn metadata_locked_current_read_is_repeatable_without_history_events_or_subscriber() {
    let state = ApplicationMcp::default();
    let id = new_project(&state, None);
    let q = query(&state, &id);
    let before = state.native_project_info(&id).unwrap();
    assert!(
        q.validate().is_ok(),
        "coherent public query must be admitted"
    );
    let a = invoke(&state, q.clone(), false);
    let b = invoke(&state, q.clone(), true);
    assert!(a.response.ok);
    assert_eq!(a.response, b.response);
    assert!(a.notification.is_none() && b.notification.is_none());
    assert_eq!(state.native_project_info(&id).unwrap(), before);
    let r = a.response.result.unwrap();
    assert_eq!(r["instanceId"], q.instance_id);
    assert_eq!(r["documentId"], id);
    assert_eq!(r["atRevision"], q.payload["atRevision"]);
    assert_eq!(r["documentSnapshotId"], q.payload["documentSnapshotId"]);
}
#[test]
fn metadata_replacement_uses_fresh_document_incarnation_not_echoed_lifecycle() {
    let state = ApplicationMcp::default();
    let old = new_project(&state, None);
    let stale = query(&state, &old);
    let info = state.native_project_info(&old).unwrap();
    let next = new_project(
        &state,
        Some(
            json!({"documentId":old,"expectedRevision":info["contentRevision"],"documentSnapshotId":info["documentSnapshotId"],"lifecycleGeneration":info["lifecycleGeneration"]}),
        ),
    );
    assert_ne!(old, next);
    let refused = invoke(&state, stale, true);
    assert!(!refused.response.ok);
    assert!(refused.notification.is_none());
    assert_eq!(refused.response.error.unwrap().code, "wrong_document");
    assert!(invoke(&state, query(&state, &next), true).response.ok);
}
#[test]
fn metadata_cancel_and_stale_snapshot_do_not_mutate_or_publish() {
    let state = ApplicationMcp::default();
    let id = new_project(&state, None);
    let before = state.native_project_info(&id).unwrap();
    let mut cancelled = query(&state, &id);
    cancelled.cancelled_before_dispatch = true;
    let delivery = invoke(&state, cancelled, true);
    assert!(!delivery.response.ok);
    assert_eq!(
        delivery.response.error.unwrap().code,
        "cancelled_before_dispatch"
    );
    assert!(delivery.notification.is_none());
    let mut stale = query(&state, &id);
    stale.payload["documentSnapshotId"] = json!("old-snapshot");
    let delivery = invoke(&state, stale, true);
    assert!(!delivery.response.ok);
    assert_eq!(delivery.response.error.unwrap().code, "stale_revision");
    assert!(delivery.notification.is_none());
    assert_eq!(state.native_project_info(&id).unwrap(), before);
}
#[test]
fn metadata_release_refuses_before_any_old_snapshot_dispatch() {
    use crate::native_application::{admit_release_request, complete_release};
    use crate::native_application_contract::NativeReleaseRequest;
    use crate::native_dispatch::ReleaseAdmission;
    let state = ApplicationMcp::default();
    let id = new_project(&state, None);
    let q = query(&state, &id);
    let request = NativeReleaseRequest {
        api_version: 2,
        request_id: "metadata-release".into(),
        instance_id: state.instance_id().into(),
        document_id: id,
        expected_revision: 0,
        cancelled_before_dispatch: false,
    };
    let ReleaseAdmission::Execute { generation } =
        admit_release_request(&state.native, &request).unwrap()
    else {
        panic!("first release retried")
    };
    complete_release(&state.native, generation, &request, || {
        Ok((Vec::new(), "already_absent"))
    })
    .unwrap();
    let refused = invoke(&state, q, true);
    assert!(!refused.response.ok);
    assert_eq!(refused.response.error.unwrap().code, "unavailable");
    assert!(refused.notification.is_none());
}

#[test]
fn metadata_pending_ui_ack_reads_committed_head_without_clearing_barrier() {
    use crate::application_mcp::revision_sync::RevisionControl;
    let state = ApplicationMcp::default();
    let id = new_project(&state, None);
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
    let command:NativeApplicationRequest=serde_json::from_value(json!({"apiVersion":2,"requestId":"metadata-commit","instanceId":state.instance_id(),"documentId":id,"operation":"command.document.object.layer.create","expectedRevision":0,"payload":{"command":"object.layer.create","name":"Metadata fixture"}})).unwrap();
    let committed = invoke(&state, command, true);
    assert!(committed.response.ok);
    assert!(committed.notification.is_some());
    let before = state.native_project_info(&id).unwrap();
    let read = invoke(&state, query(&state, &id), true);
    assert!(read.response.ok);
    assert_eq!(read.response.content_revision, 1);
    assert!(read.notification.is_none());
    assert_eq!(state.native_project_info(&id).unwrap(), before);
    let blocked:NativeApplicationRequest=serde_json::from_value(json!({"apiVersion":2,"requestId":"metadata-second-command","instanceId":state.instance_id(),"documentId":id,"operation":"command.document.object.layer.create","expectedRevision":1,"payload":{"command":"object.layer.create","name":"Must not commit"}})).unwrap();
    let refused = invoke(&state, blocked, true);
    assert!(!refused.response.ok);
    assert!(refused.notification.is_none());
    assert_eq!(state.native_project_info(&id).unwrap(), before);
}
