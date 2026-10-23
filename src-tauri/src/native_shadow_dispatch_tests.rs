//! Authenticated Shadow MCP wire and revision ACK through the actual desktop owner.
//! In-memory host evidence only; installed input, rendering and filesystem SaveAs differ.
use crate::application_mcp::revision_sync::dispatch_core::property_edit_tests::{
    history, open, receipt, round_trip, saved,
};
use crate::application_mcp::{revision_sync::RevisionControl, ApplicationMcp};
use nemo_mcp::contract::{NativeApplicationRequest, NativeApplicationResponse};
use nemo_mcp::{registry::Endpoint, wire};
use serde_json::{json, Value};
use std::time::Duration;
use tokio::net::TcpListener;

fn document(extra: Value) -> String {
    let mut project = json!({
        "canvasW":64,"canvasH":48,"fps":24,"workspace":{"panelOrder":null},
        "guides":{"h":[-0.25]},"comments":[{"text":"Keep","authorName":"Writer"}],
        "perspectiveMode":"unknown-import","perspectiveDensity":-3.5
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
        "operation":"command.document.object.project.shadow.edit",
        "payload":{"command":"object.project.shadow.edit","action":action}
    }))
    .unwrap()
}
fn read(state: &ApplicationMcp, id: &str, name: &str, selector: Value) -> NativeApplicationRequest {
    let pin = state.native_project_info(id).unwrap();
    serde_json::from_value(json!({
        "apiVersion":2,"requestId":name,"instanceId":state.instance_id(),
        "documentId":id,"operation":"query.document.object.project.shadow.read",
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

/// Keeps the real wire client pending while the actual host waits for correlated ACK.
async fn gated_add(
    state: &ApplicationMcp,
    command: NativeApplicationRequest,
    binding: &Value,
) -> NativeApplicationResponse {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = Endpoint {
        instance_id: state.instance_id().into(),
        port: listener.local_addr().unwrap().port(),
        secret: "isolated-shadow-wire".into(),
        build_id: "shadow-host-test".into(),
    };
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let mut emitted = 0;
    let server = async {
        let (mut stream, _) = listener.accept().await.unwrap();
        let envelope: wire::AuthenticatedWireRequest = wire::read_json(&mut stream).await.unwrap();
        let wire::AuthenticatedWireRequest::Native(message) = envelope else {
            panic!("non-native Shadow envelope")
        };
        assert_eq!(message.secret, endpoint.secret);
        assert_eq!(message.native_request, command);
        state
            .serve_external_for_test(&mut stream, message.native_request, |event| {
                emitted += 1;
                sender
                    .send(serde_json::to_value(event).unwrap())
                    .map_err(|_| "Shadow event receiver lost".into())
            })
            .await
            .unwrap();
    };
    let drive = async {
        let client = wire::call_native(&endpoint, command.clone(), wire::CancellationToken::new());
        tokio::pin!(client);
        let event = tokio::select! {
            event = receiver => event.unwrap(),
            response = &mut client => panic!("terminal reply preceded ACK: {response:?}"),
        };
        assert_eq!(event["instanceId"], command.instance_id);
        assert_eq!(event["documentId"], command.document_id);
        assert_eq!(event["requestId"], command.request_id);
        assert_eq!(event["lifecycleGeneration"], binding["lifecycleGeneration"]);
        assert_eq!(event["fromRevision"], 0);
        assert_eq!(event["toRevision"], 1);
        // Native commits before emitting. Terminal reply waits for ACK; this is not UI proof.
        assert_eq!(
            state.native_project_info(&command.document_id).unwrap()["contentRevision"],
            1
        );
        let committed = saved(state, &command.document_id);
        let mut wrong = event.clone();
        wrong["requestId"] = json!("wrong-shadow-ack");
        assert!(state
            .revisions
            .control(
                &state.native,
                serde_json::from_value(json!({
                    "action":"acknowledge","subscriptionId":binding["subscriptionId"],"event":wrong
                }))
                .unwrap(),
            )
            .is_err());
        let blocked = edit(
            state,
            &command.document_id,
            "pending-shadow-write",
            1,
            json!({"kind":"add","id":"blocked","color":"#123456"}),
        );
        let (refused, events) = round_trip(state, blocked, None).await;
        assert!(!refused.ok);
        assert_eq!(refused.error.unwrap().code, "unavailable");
        assert_eq!(events, 0);
        assert_eq!(saved(state, &command.document_id), committed);
        assert!(
            tokio::time::timeout(Duration::from_millis(25), client.as_mut())
                .await
                .is_err()
        );
        state
            .revisions
            .control(
                &state.native,
                serde_json::from_value(json!({
                    "action":"acknowledge","subscriptionId":binding["subscriptionId"],"event":event
                }))
                .unwrap(),
            )
            .unwrap();
        client.await.unwrap()
    };
    let (_, response) = tokio::time::timeout(Duration::from_secs(5), async {
        tokio::join!(server, drive)
    })
    .await
    .unwrap();
    assert_eq!(emitted, 1);
    response
}

#[tokio::test]
async fn shadow_authenticated_add_select_ack_replay_noop_redo_and_fresh_owner_are_exact() {
    let state = ApplicationMcp::default();
    let palette = json!([
        {"id":"same","color":"#AbC"},{"id":"same","color":"#abc"},{"id":"","color":""}
    ]);
    let id = open(
        &state,
        document(json!({"shadowPalette":palette,"shadowActiveId":"orphan"})),
    );
    let initial = saved(&state, &id);
    let add = edit(
        &state,
        &id,
        "shadow-add",
        0,
        json!({"kind":"add","id":"same","color":"#AbC"}),
    );
    add.validate().unwrap();
    let (refused, events) = round_trip(&state, add.clone(), None).await;
    assert!(!refused.ok);
    assert_eq!(refused.error.unwrap().code, "unavailable");
    assert_eq!(refused.content_revision, 0);
    assert_eq!(events, 0);
    assert_eq!(saved(&state, &id), initial);
    let binding = subscribe(&state);
    let first = gated_add(&state, add.clone(), &binding).await;
    receipt(&first, 1, true);
    let added = saved(&state, &id);
    let old_page = read(
        &state,
        &id,
        "shadow-old-page",
        json!({"kind":"palettePage","cursor":0,"limit":16}),
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
    assert_eq!(result["atRevision"], 1);
    assert_eq!(
        result["result"]["items"],
        json!([
            {"ordinal":1,"id":"same","color":"#AbC"},{"ordinal":2,"id":"same","color":"#abc"},
            {"ordinal":3,"id":"","color":""},{"ordinal":4,"id":"same","color":"#AbC"}
        ])
    );
    let (selected, events) = round_trip(
        &state,
        edit(
            &state,
            &id,
            "shadow-select",
            1,
            json!({"kind":"select","ordinal":3}),
        ),
        Some(&binding),
    )
    .await;
    receipt(&selected, 2, true);
    assert_eq!(events, 1);
    let changed = saved(&state, &id);
    let mut next: Value = serde_json::from_slice(&changed).unwrap();
    let original: Value = serde_json::from_slice(&initial).unwrap();
    assert_eq!(next["project"]["shadowActiveId"], "");
    next["project"]["shadowPalette"] = original["project"]["shadowPalette"].clone();
    next["project"]["shadowActiveId"] = original["project"]["shadowActiveId"].clone();
    assert_eq!(next, original);
    let (replay, events) = round_trip(&state, add, None).await;
    assert_eq!(replay, first);
    assert_eq!(events, 0);
    let (stale, events) = round_trip(&state, old_page, None).await;
    assert!(!stale.ok);
    assert_eq!(stale.error.unwrap().code, "stale_revision");
    assert_eq!(events, 0);
    let stale = edit(
        &state,
        &id,
        "stale-shadow-edit",
        0,
        json!({"kind":"select","ordinal":1}),
    );
    let (refused, events) = round_trip(&state, stale, None).await;
    assert!(!refused.ok);
    assert_eq!(refused.error.unwrap().code, "stale_revision");
    assert_eq!(events, 0);
    assert_eq!(saved(&state, &id), changed);
    assert!(history(&state, &id, "shadow-undo", 2, false).ok);
    assert_eq!(saved(&state, &id), added);
    let (noop, events) = round_trip(
        &state,
        edit(
            &state,
            &id,
            "same-id-noop",
            3,
            json!({"kind":"select","ordinal":2}),
        ),
        None,
    )
    .await;
    receipt(&noop, 3, false);
    assert_eq!(events, 0);
    assert!(history(&state, &id, "shadow-redo", 3, true).ok);
    assert_eq!(saved(&state, &id), changed);
    assert!(history(&state, &id, "shadow-undo-select", 4, false).ok);
    assert!(history(&state, &id, "shadow-undo-add", 5, false).ok);
    assert_eq!(
        saved(&state, &id),
        initial,
        "add-and-select used exactly one checkpoint"
    );
    let fresh = ApplicationMcp::default();
    let next_id = open(&fresh, String::from_utf8(changed.clone()).unwrap());
    assert_eq!(saved(&fresh, &next_id), changed);
}

#[tokio::test]
async fn shadow_wire_missing_empty_reads_and_explicit_default_materialization_survive_history() {
    for fields in [
        json!({}),
        json!({"shadowPalette":[]}),
        json!({"shadowPalette":[],"shadowActiveId":""}),
        json!({"shadowActiveId":"orphan"}),
    ] {
        let state = ApplicationMcp::default();
        let id = open(&state, document(fields.clone()));
        let initial = saved(&state, &id);
        let (settings, events) = round_trip(
            &state,
            read(&state, &id, "absent-settings", json!({"kind":"settings"})),
            None,
        )
        .await;
        assert!(settings.ok, "{settings:?}");
        assert_eq!(events, 0);
        let raw = settings.result.unwrap()["result"].clone();
        assert_eq!(
            raw["shadowPalettePresent"],
            fields.get("shadowPalette").is_some()
        );
        assert_eq!(raw["shadowColorCount"], 0);
        assert_eq!(raw.get("shadowActiveId"), fields.get("shadowActiveId"));
        let (page, events) = round_trip(
            &state,
            read(
                &state,
                &id,
                "absent-page",
                json!({"kind":"palettePage","cursor":0,"limit":16}),
            ),
            None,
        )
        .await;
        assert!(page.ok, "{page:?}");
        assert_eq!(events, 0);
        assert_eq!(page.result.unwrap()["result"]["items"], json!([]));
        assert_eq!(saved(&state, &id), initial);
        let binding = subscribe(&state);
        let (selected, events) = round_trip(
            &state,
            edit(
                &state,
                &id,
                "default-select",
                0,
                json!({"kind":"select","ordinal":6}),
            ),
            Some(&binding),
        )
        .await;
        receipt(&selected, 1, true);
        assert_eq!(events, 1);
        let selected = saved(&state, &id);
        let value: Value = serde_json::from_slice(&selected).unwrap();
        assert_eq!(
            value["project"]["shadowPalette"],
            json!([
                {"id":"sh1","color":"#ff3355"},{"id":"sh2","color":"#ff8800"},
                {"id":"sh3","color":"#ffdd00"},{"id":"sh4","color":"#22cc55"},
                {"id":"sh5","color":"#2288ff"},{"id":"sh6","color":"#aa33ff"}
            ])
        );
        assert_eq!(value["project"]["shadowActiveId"], "sh6");
        assert!(history(&state, &id, "default-undo", 1, false).ok);
        assert_eq!(saved(&state, &id), initial);
        assert!(history(&state, &id, "default-redo", 2, true).ok);
        assert_eq!(saved(&state, &id), selected);
        assert!(history(&state, &id, "restore-absence-before-add", 3, false).ok);
        assert_eq!(saved(&state, &id), initial);
        let (added, events) = round_trip(
            &state,
            edit(
                &state,
                &id,
                "default-add",
                4,
                json!({"kind":"add","id":"sh7","color":"#AbCdEf"}),
            ),
            Some(&binding),
        )
        .await;
        receipt(&added, 5, true);
        assert_eq!(events, 1);
        let changed = saved(&state, &id);
        let mut expected = value;
        expected["project"]["shadowPalette"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"sh7","color":"#AbCdEf"}));
        expected["project"]["shadowActiveId"] = json!("sh7");
        assert_eq!(serde_json::from_slice::<Value>(&changed).unwrap(), expected);
        assert!(history(&state, &id, "default-add-undo", 5, false).ok);
        assert_eq!(saved(&state, &id), initial);
        assert!(history(&state, &id, "default-add-redo", 6, true).ok);
        assert_eq!(saved(&state, &id), changed);
        let fresh = ApplicationMcp::default();
        let next = open(&fresh, String::from_utf8(changed.clone()).unwrap());
        assert_eq!(saved(&fresh, &next), changed);
    }
}
