//! PRIVATE UNRUN: actual sole host/history and RevisionSync; not installed proof.
use crate::application_mcp::{
    revision_sync::{Delivery, RevisionControl},
    ApplicationMcp,
};
use crate::native_object_bootstrap::bootstrap_raw;
use nemo_mcp::contract::NativeApplicationRequest;
use serde_json::{json, Value};
const OP: &str = "command.document.object.layer.policy.edit";
fn install(state: &ApplicationMcp) -> String {
    let r = bootstrap_raw(
        state,
        &json!({"apiVersion":2,"requestId":"policy-install","instanceId":state.instance_id(),
        "newProject":{"canvasW":64,"canvasH":48,"fps":24.0}})
        .to_string(),
    )
    .unwrap();
    serde_json::to_value(r).unwrap()["documentId"]
        .as_str()
        .unwrap()
        .to_owned()
}
fn request(
    state: &ApplicationMcp,
    id: &str,
    rid: &str,
    revision: u64,
    flag: &str,
    value: bool,
) -> NativeApplicationRequest {
    let info = state.native_project_info(id).unwrap();
    serde_json::from_value(json!({"apiVersion":2,"requestId":rid,"instanceId":state.instance_id(),"documentId":id,"expectedRevision":revision,
        "operation":OP,"payload":{"command":"object.layer.policy.edit","layerUid":info["layers"][0]["layerUid"],"flag":flag,"value":value}})).unwrap()
}
fn invoke(state: &ApplicationMcp, r: NativeApplicationRequest, external: bool) -> Delivery {
    r.validate().unwrap();
    state
        .revisions
        .dispatch_admitted(state.instance_id(), &state.native, r, external, false)
        .unwrap()
}
fn saved(state: &ApplicationMcp, id: &str) -> Vec<u8> {
    let info = state.native_project_info(id).unwrap();
    state
        .native_project_file_snapshot(
            id,
            info["contentRevision"].as_u64().unwrap(),
            info["documentSnapshotId"].as_str().unwrap(),
        )
        .unwrap()
        .unwrap()
}
#[test]
fn policy_public_commands_reach_original_host_with_exact_replay_noop_and_authored_projection() {
    let state = ApplicationMcp::default();
    let id = install(&state);
    let initial: Value = serde_json::from_slice(&saved(&state, &id)).unwrap();
    assert!(state.native_project_info(&id).unwrap()["layers"][0]
        .get("shy")
        .is_none());
    let noop = request(&state, &id, "absent-shy", 0, "shy", false);
    let reply = invoke(&state, noop, false);
    assert!(reply.response.ok);
    assert_eq!(reply.response.content_revision, 0);
    assert_eq!(
        reply.response.result.unwrap(),
        json!({"applied":false,"historyEntriesAdded":0})
    );
    assert!(reply.notification.is_none());
    for (index, flag) in ["locked", "shy"].into_iter().enumerate() {
        let r = request(&state, &id, flag, index as u64, flag, true);
        let first = invoke(&state, r.clone(), false);
        assert!(first.response.ok);
        assert_eq!(first.response.content_revision, index as u64 + 1);
        assert_eq!(
            first.response.result,
            Some(json!({"applied":true,"historyEntriesAdded":1}))
        );
        assert!(first.notification.is_none());
        let before = saved(&state, &id);
        let retry = invoke(&state, r, false);
        assert_eq!(retry.response, first.response);
        assert!(retry.notification.is_none());
        assert_eq!(saved(&state, &id), before);
        assert_eq!(
            state.native_project_info(&id).unwrap()["layers"][0][flag],
            true
        );
    }
    let after: Value = serde_json::from_slice(&saved(&state, &id)).unwrap();
    let mut expected = initial;
    expected["drawing"]["layers"][0]["locked"] = json!(true);
    expected["drawing"]["layers"][0]["shy"] = json!(true);
    assert_eq!(after, expected);
    let r = request(&state, &id, "same-shy", 2, "shy", true);
    let before = saved(&state, &id);
    let noop = invoke(&state, r, false);
    assert!(noop.response.ok);
    assert_eq!(noop.response.content_revision, 2);
    assert_eq!(
        noop.response.result,
        Some(json!({"applied":false,"historyEntriesAdded":0}))
    );
    assert!(noop.notification.is_none());
    assert_eq!(saved(&state, &id), before);
}
#[test]
fn policy_stale_cancel_wrong_owner_and_missing_target_do_not_mutate_or_publish() {
    let state = ApplicationMcp::default();
    let id = install(&state);
    let before = saved(&state, &id);
    let mut stale = request(&state, &id, "stale", 1, "locked", true);
    let r = invoke(&state, stale.clone(), false);
    assert!(!r.response.ok);
    assert_eq!(r.response.error.unwrap().code, "stale_revision");
    assert!(r.notification.is_none());
    stale.request_id = "cancel".into();
    stale.expected_revision = Some(0);
    stale.cancelled_before_dispatch = true;
    let r = invoke(&state, stale, false);
    assert!(!r.response.ok);
    assert_eq!(r.response.error.unwrap().code, "cancelled_before_dispatch");
    assert!(r.notification.is_none());
    for (rid, code, instance) in [
        ("foreign-instance", "wrong_instance", true),
        ("foreign-document", "wrong_document", false),
    ] {
        let mut r = request(&state, &id, rid, 0, "shy", true);
        if instance {
            r.instance_id = "foreign".into();
        } else {
            r.document_id = "foreign".into();
        }
        let r = invoke(&state, r, false);
        assert!(!r.response.ok);
        assert_eq!(r.response.error.unwrap().code, code);
        assert!(r.notification.is_none());
    }
    let mut missing = request(&state, &id, "missing", 0, "locked", true);
    missing.payload["layerUid"] = json!("missing");
    let r = invoke(&state, missing, false);
    assert!(!r.response.ok);
    assert_eq!(r.response.error.unwrap().code, "not_found");
    assert!(r.notification.is_none());
    assert_eq!(saved(&state, &id), before);
}
#[test]
fn policy_external_command_requires_subscriber_and_pending_ack_fences_later_writes_not_project_info_reads(
) {
    let state = ApplicationMcp::default();
    let id = install(&state);
    let command = request(&state, &id, "lock", 0, "locked", true);
    let before = saved(&state, &id);
    let refused = invoke(&state, command.clone(), true);
    assert!(!refused.response.ok);
    assert!(refused.notification.is_none());
    assert_eq!(saved(&state, &id), before);
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
    let committed = invoke(&state, command.clone(), true);
    assert!(committed.response.ok);
    assert!(committed.notification.is_some());
    assert_eq!(committed.response.content_revision, 1);
    // Read the existing protected840 project-info provider while ACK is pending.
    // This is not public D35 metadata query acceptance and does not ACK a notice.
    let info = state.native_project_info(&id).unwrap();
    assert_eq!(info["contentRevision"], 1);
    assert_eq!(info["documentSnapshotId"], format!("native-object:{id}:1"));
    assert_eq!(info["layers"][0]["locked"], true);
    assert_eq!(state.native_project_info(&id).unwrap(), info);
    let blocked = invoke(
        &state,
        request(&state, &id, "shy-blocked", 1, "shy", true),
        true,
    );
    assert!(!blocked.response.ok);
    assert!(blocked.notification.is_none());
    assert_eq!(state.native_project_info(&id).unwrap(), info);
    let retry = invoke(&state, command, true);
    assert!(!retry.response.ok);
    assert!(retry.notification.is_none());
    assert_eq!(state.native_project_info(&id).unwrap(), info);
}
#[test]
fn policy_payload_cannot_escape_command_revision_or_query_contract_admission() {
    let state = ApplicationMcp::default();
    let id = install(&state);
    let source = saved(&state, &id);
    let r = request(&state, &id, "shape", 0, "locked", true);
    for flag in [json!("solo"), json!(null), json!({"locked":null})] {
        let mut bad = r.clone();
        bad.payload["flag"] = flag;
        assert!(bad.validate().is_err());
    }
    let mut missing = r.clone();
    missing.expected_revision = None;
    assert!(missing.validate().is_err());
    let fixture: Value = serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    ))
    .unwrap();
    let mut read: NativeApplicationRequest =
        serde_json::from_value(fixture["read"]["request"].clone()).unwrap();
    assert_eq!(read.operation, "query.document.object");
    assert!(
        read.validate().is_ok(),
        "registered fixture query is admitted"
    );
    read.payload = r.payload;
    assert!(
        read.validate().is_err(),
        "policy body is not an object query selector"
    );
    assert_eq!(saved(&state, &id), source);
}

#[test]
fn policy_bounded_raw_client_reaches_same_owner_and_rejects_original_duplicate_members() {
    let state = ApplicationMcp::default();
    let id = install(&state);
    let request = request(&state, &id, "raw-policy", 0, "shy", true);
    let raw = serde_json::to_string(&request).unwrap();
    let duplicate = raw.replacen(
        "\"flag\":\"shy\"",
        "\"flag\":\"shy\",\"flag\":\"locked\"",
        1,
    );
    assert_ne!(duplicate, raw);
    let before = saved(&state, &id);
    assert!(crate::application_mcp::object_client::admit_raw(&duplicate).is_err());
    assert_eq!(saved(&state, &id), before);
    let reply = crate::application_mcp::object_client::dispatch_raw(&state, &raw).unwrap();
    assert!(reply.ok);
    assert_eq!(reply.content_revision, 1);
    assert_eq!(
        reply.result,
        Some(json!({"applied":true,"historyEntriesAdded":1}))
    );
    assert_eq!(
        state.native_project_info(&id).unwrap()["layers"][0]["shy"],
        true
    );
    assert_eq!(
        crate::application_mcp::object_client::dispatch_raw(&state, &raw).unwrap(),
        reply
    );
    let after = saved(&state, &id);
    assert_eq!(
        state.native_project_info(&id).unwrap()["contentRevision"],
        1
    );
    assert_ne!(after, before);
}

/// Public authenticated wire, actual host and real ACK release; no owner mock.
async fn policy_external_round_trip(
    state: &ApplicationMcp,
    original: NativeApplicationRequest,
    binding: Option<&Value>,
    expected_locked: Option<bool>,
    expected_shy: Option<bool>,
) -> (nemo_mcp::contract::NativeApplicationResponse, usize) {
    use nemo_mcp::{registry::Endpoint, wire};
    use tokio::net::TcpListener;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = Endpoint {
        instance_id: state.instance_id().into(),
        port: listener.local_addr().unwrap().port(),
        secret: "isolated-layer-policy-wire".into(),
        build_id: "policy-wire-history-test".into(),
    };
    let mut acknowledgments = 0;
    let server = async {
        let (mut stream, _) = listener.accept().await.unwrap();
        let envelope: wire::AuthenticatedWireRequest = wire::read_json(&mut stream).await.unwrap();
        let wire::AuthenticatedWireRequest::Native(message) = envelope else {
            panic!("policy operation used a non-native envelope")
        };
        assert_eq!(message.secret, endpoint.secret);
        assert_eq!(
            serde_json::to_value(&message.native_request).unwrap(),
            serde_json::to_value(&original).unwrap()
        );
        state
            .serve_external_for_test(&mut stream, message.native_request, |event| {
                let binding = binding.expect("unsubscribed command must not publish");
                let observed = serde_json::to_value(&event).unwrap();
                assert_eq!(observed["instanceId"], original.instance_id);
                assert_eq!(observed["documentId"], original.document_id);
                assert_eq!(observed["requestId"], original.request_id);
                assert_eq!(
                    observed["lifecycleGeneration"],
                    binding["lifecycleGeneration"]
                );
                assert_eq!(
                    observed["fromRevision"],
                    original.expected_revision.unwrap()
                );
                assert_eq!(
                    observed["toRevision"],
                    original.expected_revision.unwrap() + 1
                );
                let info = state.native_project_info(&original.document_id).unwrap();
                assert_eq!(info["contentRevision"], observed["toRevision"]);
                assert_eq!(
                    info["layers"][0].get("locked").and_then(Value::as_bool),
                    expected_locked
                );
                assert_eq!(
                    info["layers"][0].get("shy").and_then(Value::as_bool),
                    expected_shy
                );
                state.revisions.control(
                    &state.native,
                    RevisionControl::Acknowledge {
                        subscription_id: binding["subscriptionId"].as_str().unwrap().to_owned(),
                        event,
                    },
                )?;
                acknowledgments += 1;
                Ok(())
            })
            .await
            .unwrap();
    };
    let client = wire::call_native(&endpoint, original.clone(), wire::CancellationToken::new());
    let (_, response) = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        tokio::join!(server, client)
    })
    .await
    .unwrap();
    (response.unwrap(), acknowledgments)
}

// Only policy commands traverse public MCP here. Object undo/redo deliberately
// use the already admitted bounded desktop route; public history is not declared.
#[tokio::test]
async fn policy_public_wire_ack_and_bounded_history_preserve_noop_redo_and_authored_shy_absence() {
    let state = ApplicationMcp::default();
    let id = install(&state);
    let initial = saved(&state, &id);
    let info = state.native_project_info(&id).unwrap();
    // New uses blank_drawing, whose baseline authors locked:false, unlike bare.
    assert_eq!(info["layers"][0].get("locked"), Some(&json!(false)));
    assert!(info["layers"][0].get("shy").is_none());
    let lock = request(&state, &id, "wire-lock", 0, "locked", true);
    let (refused, events) =
        policy_external_round_trip(&state, lock.clone(), None, Some(false), None).await;
    assert!(!refused.ok);
    assert_eq!(refused.content_revision, 0);
    assert_eq!(refused.error.unwrap().code, "unavailable");
    assert_eq!(events, 0);
    assert_eq!(saved(&state, &id), initial);
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
    let (first, events) =
        policy_external_round_trip(&state, lock.clone(), Some(&binding), Some(true), None).await;
    assert!(first.ok);
    assert_eq!(first.content_revision, 1);
    assert_eq!(
        first.result,
        Some(json!({"applied":true,"historyEntriesAdded":1}))
    );
    assert_eq!(events, 1);
    let locked = saved(&state, &id);
    assert_ne!(locked, initial);
    let history = |rid: &str, revision: u64, redo: bool| {
        let raw = json!({"apiVersion":2,"requestId":rid,
            "instanceId":state.instance_id(),"documentId":id,"expectedRevision":revision,
            "operation":if redo {"command.document.object.redo"} else {"command.document.object.undo"},
            "payload":{"command":if redo {"object.redo"} else {"object.undo"}}}).to_string();
        crate::application_mcp::object_client::dispatch_raw(&state, &raw).unwrap()
    };
    let undo = history("bounded-undo-lock", 1, false);
    assert!(undo.ok);
    assert_eq!(undo.content_revision, 2);
    assert_eq!(saved(&state, &id), initial);
    let (noop, events) = policy_external_round_trip(
        &state,
        request(&state, &id, "wire-noop", 2, "locked", false),
        Some(&binding),
        Some(false),
        None,
    )
    .await;
    assert!(noop.ok);
    assert_eq!(noop.content_revision, 2);
    assert_eq!(
        noop.result,
        Some(json!({"applied":false,"historyEntriesAdded":0}))
    );
    assert_eq!(events, 0);
    assert_eq!(saved(&state, &id), initial);
    let redo = history("bounded-redo-lock", 2, true);
    assert!(redo.ok);
    assert_eq!(redo.content_revision, 3);
    assert_eq!(saved(&state, &id), locked);
    let (shy, events) = policy_external_round_trip(
        &state,
        request(&state, &id, "wire-shy", 3, "shy", true),
        Some(&binding),
        Some(true),
        Some(true),
    )
    .await;
    assert!(shy.ok);
    assert_eq!(shy.content_revision, 4);
    assert_eq!(events, 1);
    let both = saved(&state, &id);
    assert_ne!(both, locked);
    for (rid, revision, redo, expected, bytes) in [
        ("bounded-undo-shy", 4, false, None, &locked),
        ("bounded-redo-shy", 5, true, Some(true), &both),
    ] {
        let reply = history(rid, revision, redo);
        assert!(reply.ok);
        assert_eq!(reply.content_revision, revision + 1);
        assert_eq!(&saved(&state, &id), bytes);
        let info = state.native_project_info(&id).unwrap();
        assert_eq!(info["layers"][0].get("locked"), Some(&json!(true)));
        assert_eq!(
            info["layers"][0].get("shy").and_then(Value::as_bool),
            expected
        );
    }
    let (retained, events) =
        policy_external_round_trip(&state, lock, Some(&binding), Some(true), Some(true)).await;
    assert_eq!(retained, first);
    assert_eq!(events, 0);
    assert_eq!(
        state.native_project_info(&id).unwrap()["contentRevision"],
        6
    );
    assert_eq!(saved(&state, &id), both);
}
