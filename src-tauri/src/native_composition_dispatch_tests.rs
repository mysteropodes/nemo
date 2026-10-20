//! Real bundled MCP wire, subscribed desktop owner and retained native history.
//! In-memory host acceptance; installed input and filesystem SaveAs remain separate.
use crate::application_mcp::revision_sync::dispatch_core::property_edit_tests::{
    history, open, receipt, round_trip, saved,
};
use crate::application_mcp::{revision_sync::RevisionControl, ApplicationMcp};
use nemo_mcp::contract::NativeApplicationRequest;
use serde_json::{json, Value};
const EDIT: &str = "command.document.object.project.composition.edit";
const READ: &str = "query.document.object.project.composition.read";
fn edit(
    state: &ApplicationMcp,
    id: &str,
    name: &str,
    revision: u64,
    action: Value,
) -> NativeApplicationRequest {
    serde_json::from_value(
        json!({"apiVersion":2,"requestId":name,"instanceId":state.instance_id(),
        "documentId":id,"operation":EDIT,"expectedRevision":revision,
        "payload":{"command":"object.project.composition.edit","action":action}}),
    )
    .unwrap()
}
fn read(state: &ApplicationMcp, id: &str, name: &str, selector: Value) -> NativeApplicationRequest {
    let pin = state.native_project_info(id).unwrap();
    serde_json::from_value(json!({"apiVersion":2,"requestId":name,"instanceId":state.instance_id(),
        "documentId":id,"operation":READ,"payload":{"atRevision":pin["contentRevision"],
        "documentSnapshotId":pin["documentSnapshotId"],"contextId":"scene-root","selector":selector}})).unwrap()
}
#[tokio::test]
async fn composition_real_mcp_wire_ack_pinned_reads_replay_history_and_fresh_owner_reopen() {
    let state = ApplicationMcp::default();
    let project = json!({"format":"nemo.native-project-document","formatVersion":1,
        "project":{"canvasW":64,"canvasH":48,"fps":24,"workspace":{"panelOrder":null},
            "guides":{"h":[-0.25]},"comments":[{"text":"Unrelated","authorName":"Writer"}]},
        "viewportReference":null,"assets":[],"drawing":{"format":"nemo.native-object-document",
        "formatVersion":1,"totalFrames":40,"layers":[{"layerUid":"L0"}],"objects":[],
        "propertyHolders":[{"target":{"contextId":"scene-root","layerUid":"L0"},
            "properties":[{"property":"opacity","static":[17]}]}]}});
    let id = open(&state, project.to_string());
    let initial = saved(&state, &id);
    let command = edit(
        &state,
        &id,
        "wire-marker",
        0,
        json!({"kind":"addMarker","frame":25.5,"name":"chapter","color":""}),
    );
    command.validate().unwrap();
    let (refused, events) = round_trip(&state, command.clone(), None).await;
    assert!(!refused.ok);
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
    let (first, events) = round_trip(&state, command.clone(), Some(&binding)).await;
    receipt(&first, 1, true);
    assert_eq!(events, 1);
    let marker_only = saved(&state, &id);
    let page = read(
        &state,
        &id,
        "wire-page",
        json!({"kind":"markerPage","cursor":0,"limit":16}),
    );
    page.validate().unwrap();
    let (response, events) = round_trip(&state, page.clone(), None).await;
    assert!(response.ok, "{response:?}");
    assert_eq!(events, 0);
    assert_eq!(response.content_revision, 1);
    let result = response.result.unwrap();
    assert_eq!(
        result["documentSnapshotId"],
        page.payload["documentSnapshotId"]
    );
    assert_eq!(
        result["result"]["items"],
        json!([{"ordinal":1,"frame":25.5,"name":"chapter","color":""}])
    );
    let tempo = edit(
        &state,
        &id,
        "wire-tempo",
        1,
        json!({"kind":"setBpm","bpm":137.5,"bpmShow":true}),
    );
    let (response, events) = round_trip(&state, tempo, Some(&binding)).await;
    receipt(&response, 2, true);
    assert_eq!(events, 1);
    let changed = saved(&state, &id);
    let next: Value = serde_json::from_slice(&changed).unwrap();
    for field in ["workspace", "guides", "comments"] {
        assert_eq!(next["project"][field], project["project"][field]);
    }
    assert!(next["project"].get("bpmOffset").is_none());
    let (settings, events) = round_trip(
        &state,
        read(&state, &id, "wire-settings", json!({"kind":"settings"})),
        None,
    )
    .await;
    assert!(settings.ok, "{settings:?}");
    assert_eq!(events, 0);
    assert_eq!(settings.result.unwrap()["result"]["effectiveBpm"], 137.5);
    let (noop, events) = round_trip(
        &state,
        edit(
            &state,
            &id,
            "wire-noop",
            2,
            json!({"kind":"setBpm","bpm":137.5}),
        ),
        None,
    )
    .await;
    receipt(&noop, 2, false);
    assert_eq!(events, 0);
    let (replay, events) = round_trip(&state, command, None).await;
    assert_eq!(replay, first);
    assert_eq!(events, 0);
    let (stale, events) = round_trip(&state, page, None).await;
    assert!(!stale.ok);
    assert_eq!(stale.error.unwrap().code, "stale_revision");
    assert_eq!(events, 0);
    assert_eq!(saved(&state, &id), changed);
    assert!(history(&state, &id, "wire-undo", 2, false).ok);
    assert_eq!(saved(&state, &id), marker_only);
    assert!(history(&state, &id, "wire-redo", 3, true).ok);
    assert_eq!(saved(&state, &id), changed);
    let fresh = ApplicationMcp::default();
    let next_id = open(&fresh, String::from_utf8(changed.clone()).unwrap());
    assert_eq!(saved(&fresh, &next_id), changed);
}

fn palette_edit(
    state: &ApplicationMcp,
    id: &str,
    name: &str,
    revision: u64,
    action: Value,
) -> NativeApplicationRequest {
    let mut command = edit(state, id, name, revision, action);
    command.operation = "command.document.object.project.palette.edit".into();
    command.payload["command"] = json!("object.project.palette.edit");
    command
}

fn palette_read(
    state: &ApplicationMcp,
    id: &str,
    name: &str,
    selector: Value,
) -> NativeApplicationRequest {
    let mut command = read(state, id, name, selector);
    command.operation = "query.document.object.project.palette.read".into();
    command
}

#[tokio::test]
async fn palette_real_mcp_wire_all_field_intents_ack_replay_noop_redo_and_fresh_owner_reopen() {
    let state = ApplicationMcp::default();
    let project = json!({"format":"nemo.native-project-document","formatVersion":1,
        "project":{"canvasW":64,"canvasH":48,"fps":24,"workspace":{"panelOrder":null},
            "guides":{"h":[-0.25]},"comments":[{"text":"Unrelated","authorName":"Writer"}],
            "palettes":[{"id":"same","name":"First","colors":["#AbC","#abc","#00000000"]},
                {"id":"same","name":"Second","colors":["#DEF"]}],"activePaletteIdx":0},
        "viewportReference":null,"assets":[],"drawing":{"format":"nemo.native-object-document",
        "formatVersion":1,"totalFrames":40,"layers":[{"layerUid":"L0"}],"objects":[],
        "propertyHolders":[{"target":{"contextId":"scene-root","layerUid":"L0"},
            "properties":[{"property":"opacity","static":[17]}]}]}});
    let id = open(&state, project.to_string());
    let initial = saved(&state, &id);
    let command = palette_edit(
        &state,
        &id,
        "palette-rename",
        0,
        json!({"kind":"rename","ordinal":1,"name":"Renamed"}),
    );
    command.validate().unwrap();
    let (refused, events) = round_trip(&state, command.clone(), None).await;
    assert!(!refused.ok);
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
    let (first, events) = round_trip(&state, command.clone(), Some(&binding)).await;
    receipt(&first, 1, true);
    assert_eq!(events, 1);
    let page = palette_read(
        &state,
        &id,
        "palette-list",
        json!({"kind":"palettePage","cursor":0,"limit":16}),
    );
    page.validate().unwrap();
    let (response, events) = round_trip(&state, page.clone(), None).await;
    assert!(response.ok, "{response:?}");
    assert_eq!(events, 0);
    assert_eq!(
        response.result.unwrap()["result"]["items"],
        json!([
            {"ordinal":1,"id":"same","name":"Renamed","colorCount":3},
            {"ordinal":2,"id":"same","name":"Second","colorCount":1}
        ])
    );
    let (response, _) = round_trip(
        &state,
        palette_read(
            &state,
            &id,
            "palette-colors",
            json!({"kind":"colorPage","ordinal":1,"cursor":0,"limit":16}),
        ),
        None,
    )
    .await;
    assert!(response.ok, "{response:?}");
    assert_eq!(
        response.result.unwrap()["result"]["items"],
        json!([
            {"index":1,"color":"#AbC"},{"index":2,"color":"#abc"},{"index":3,"color":"#00000000"}
        ])
    );
    for (revision, action) in [
        (1, json!({"kind":"appendColor","ordinal":1,"color":"#1234"})),
        (2, json!({"kind":"moveColor","ordinal":1,"from":4,"to":1})),
        (3, json!({"kind":"removeColor","ordinal":1,"index":2})),
        (
            4,
            json!({"kind":"duplicate","ordinal":1,"id":"new-copy","name":"Copy"}),
        ),
        (5, json!({"kind":"select","ordinal":3})),
        (6, json!({"kind":"remove","ordinal":2})),
    ] {
        let request = palette_edit(
            &state,
            &id,
            &format!("palette-step-{revision}"),
            revision,
            action,
        );
        request.validate().unwrap();
        let (response, events) = round_trip(&state, request, Some(&binding)).await;
        receipt(&response, revision + 1, true);
        assert_eq!(events, 1);
    }
    let before_create = saved(&state, &id);
    let (response, events) = round_trip(
        &state,
        palette_edit(
            &state,
            &id,
            "palette-create",
            7,
            json!({"kind":"create","id":"new","name":"Created","color":"data:authored-color"}),
        ),
        Some(&binding),
    )
    .await;
    receipt(&response, 8, true);
    assert_eq!(events, 1);
    let changed = saved(&state, &id);
    let next: Value = serde_json::from_slice(&changed).unwrap();
    assert_eq!(next["project"]["activePaletteIdx"], 2.0);
    assert_eq!(
        next["project"]["palettes"],
        json!([
            {"id":"same","name":"Renamed","colors":["#1234","#abc","#00000000"]},
            {"id":"same","name":"Second","colors":["#DEF"]},
            {"id":"new","name":"Created","colors":["data:authored-color"]}
        ])
    );
    for field in ["workspace", "guides", "comments"] {
        assert_eq!(next["project"][field], project["project"][field]);
    }
    let initial_value: Value = serde_json::from_slice(&initial).unwrap();
    assert_eq!(next["drawing"], initial_value["drawing"]);
    let (replay, events) = round_trip(&state, command, None).await;
    assert_eq!(replay, first);
    assert_eq!(events, 0);
    let (stale, events) = round_trip(&state, page, None).await;
    assert!(!stale.ok);
    assert_eq!(stale.error.unwrap().code, "stale_revision");
    assert_eq!(events, 0);
    assert_eq!(saved(&state, &id), changed);
    assert!(history(&state, &id, "palette-undo", 8, false).ok);
    assert_eq!(saved(&state, &id), before_create);
    let (noop, events) = round_trip(
        &state,
        palette_edit(
            &state,
            &id,
            "palette-noop",
            9,
            json!({"kind":"select","ordinal":2}),
        ),
        None,
    )
    .await;
    receipt(&noop, 9, false);
    assert_eq!(events, 0);
    assert!(history(&state, &id, "palette-redo", 9, true).ok);
    assert_eq!(saved(&state, &id), changed);
    let fresh = ApplicationMcp::default();
    let next_id = open(&fresh, String::from_utf8(changed.clone()).unwrap());
    assert_eq!(saved(&fresh, &next_id), changed);
}
