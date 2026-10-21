//! Custom preset catalog metadata through authenticated MCP and native history.
//! Vector brush resolution/drawing and installed consumers remain separate.
use crate::application_mcp::revision_sync::dispatch_core::property_edit_tests::{
    history, open, receipt, round_trip, saved,
};
use crate::application_mcp::{revision_sync::RevisionControl, ApplicationMcp};
use nemo_mcp::contract::NativeApplicationRequest;
use serde_json::{json, Value};

fn document(catalog: Option<&str>) -> String {
    let mut project = json!({"canvasW":64,"canvasH":48,"fps":24,
        "workspace":{"panelOrder":null,"motionColumns":""},
        "comments":[{"id":"same","authorName":"Writer"},{"id":"same"},{}],
        "perspectiveMode":"unknown","perspectiveDensity":-3.5,
        "shadowPalette":[{"id":"same","color":"#AbC"},{"id":"same","color":"#abc"}]});
    if catalog.is_some() {
        project["customBrushPresets"] = json!("__RAW_CATALOG__");
    }
    let text = json!({"format":"nemo.native-project-document","formatVersion":1,
        "project":project,"viewportReference":null,"assets":[],
        "drawing":{"format":"nemo.native-object-document","formatVersion":1,
            "totalFrames":40,"layers":[{"layerUid":"L0"}],"objects":[]}})
    .to_string();
    catalog.map_or(text.clone(), |raw| text.replace("\"__RAW_CATALOG__\"", raw))
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
        "operation":"command.document.object.project.customBrushPreset.edit",
        "payload":{"command":"object.project.customBrushPreset.edit","action":action}}),
    )
    .unwrap()
}
fn read(state: &ApplicationMcp, id: &str, name: &str, selector: Value) -> NativeApplicationRequest {
    let pin = state.native_project_info(id).unwrap();
    serde_json::from_value(json!({"apiVersion":2,"requestId":name,"instanceId":state.instance_id(),
        "documentId":id,"operation":"query.document.object.project.customBrushPreset.read",
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
        &format!("catalog-step-{revision}"),
        revision,
        action,
    );
    request.validate().unwrap();
    let (response, events) = round_trip(state, request, Some(binding)).await;
    receipt(&response, revision + 1, true);
    assert_eq!(events, 1);
}

#[tokio::test]
async fn custom_catalog_wire_ack_overwrite_append_delete_order_replay_redo_and_reopen_are_exact() {
    let state = ApplicationMcp::default();
    // Deliberately preserve numeric-like IDs in non-lexical native order.
    let id = open(&state, document(Some(r#"{"2":{"label":"two"},"10":{}}"#)));
    let initial = saved(&state, &id);
    let preset = json!({"label":"","nibSize":-0.25,"markerTip":false,
        "customStamp":{"segments":[],"closed":false}});
    let command = edit(
        &state,
        &id,
        "catalog-overwrite",
        0,
        json!({"kind":"upsert","id":"2","preset":preset}),
    );
    command.validate().unwrap();
    let (refused, events) = round_trip(&state, command.clone(), None).await;
    assert_eq!(refused.error.unwrap().code, "unavailable");
    assert_eq!(refused.content_revision, 0);
    assert_eq!(events, 0);
    assert_eq!(saved(&state, &id), initial);
    let binding = subscribe(&state);
    // Actual helper authenticates the wire and checks native commit, bad ACK,
    // pending-write refusal and exactly one correlated ACK before returning.
    let (first, events) = round_trip(&state, command.clone(), Some(&binding)).await;
    receipt(&first, 1, true);
    assert_eq!(events, 1);
    assert_eq!(first.request_id, command.request_id);
    assert_eq!(first.instance_id, command.instance_id);
    assert_eq!(first.document_id, command.document_id);
    let old_page = read(
        &state,
        &id,
        "catalog-pinned",
        json!({"kind":"catalogPage","cursor":0,"limit":16}),
    );
    old_page.validate().unwrap();
    let (page, events) = round_trip(&state, old_page.clone(), None).await;
    assert!(page.ok, "{page:?}");
    assert_eq!(events, 0);
    let result = page.result.unwrap();
    assert_eq!(
        result["documentSnapshotId"],
        old_page.payload["documentSnapshotId"]
    );
    assert_eq!(result["atRevision"], 1);
    assert_eq!(
        result["result"]["items"],
        json!([
        {"ordinal":1,"id":"2","label":""},{"ordinal":2,"id":"10"}])
    );
    let (record, events) = round_trip(
        &state,
        read(
            &state,
            &id,
            "catalog-record",
            json!({"kind":"record","id":"2"}),
        ),
        None,
    )
    .await;
    assert!(record.ok, "{record:?}");
    assert_eq!(events, 0);
    assert_eq!(
        record.result.unwrap()["result"],
        json!({"kind":"customBrushPresetRecord","id":"2","preset":preset})
    );
    change(
        &state,
        &id,
        &binding,
        1,
        json!({"kind":"upsert","id":"","preset":{}}),
    )
    .await;
    let appended = saved(&state, &id);
    let (page, _) = round_trip(
        &state,
        read(
            &state,
            &id,
            "appended-order",
            json!({"kind":"catalogPage","cursor":0,"limit":16}),
        ),
        None,
    )
    .await;
    assert_eq!(
        page.result.unwrap()["result"]["items"],
        json!([
        {"ordinal":1,"id":"2","label":""},{"ordinal":2,"id":"10"},{"ordinal":3,"id":""}])
    );
    change(&state, &id, &binding, 2, json!({"kind":"delete","id":"10"})).await;
    let changed = saved(&state, &id);
    let original: Value = serde_json::from_slice(&initial).unwrap();
    let mut next: Value = serde_json::from_slice(&changed).unwrap();
    assert_eq!(
        next["project"]["customBrushPresets"],
        json!({"2":preset,"":{}})
    );
    next["project"]["customBrushPresets"] = original["project"]["customBrushPresets"].clone();
    assert_eq!(next, original);
    let (replay, events) = round_trip(&state, command, None).await;
    assert_eq!(replay, first);
    assert_eq!(events, 0);
    let (stale, events) = round_trip(&state, old_page, None).await;
    assert_eq!(stale.error.unwrap().code, "stale_revision");
    assert_eq!(events, 0);
    assert!(history(&state, &id, "catalog-undo-delete", 3, false).ok);
    assert_eq!(saved(&state, &id), appended);
    for (name, action) in [
        (
            "catalog-noop-delete",
            json!({"kind":"delete","id":"missing"}),
        ),
        (
            "catalog-noop-update",
            json!({"kind":"upsert","id":"2","preset":preset}),
        ),
    ] {
        let (noop, events) = round_trip(&state, edit(&state, &id, name, 4, action), None).await;
        receipt(&noop, 4, false);
        assert_eq!(events, 0);
    }
    assert!(history(&state, &id, "catalog-redo-delete", 4, true).ok);
    assert_eq!(saved(&state, &id), changed);
    let fresh = ApplicationMcp::default();
    let reopened = open(&fresh, String::from_utf8(changed.clone()).unwrap());
    assert_eq!(saved(&fresh, &reopened), changed);
}

#[tokio::test]
async fn custom_catalog_absent_empty_raw_identity_and_authenticated_refusals_do_not_seed_defaults()
{
    for catalog in [None, Some("{}")] {
        let state = ApplicationMcp::default();
        let id = open(&state, document(catalog));
        let initial = saved(&state, &id);
        let (settings, events) = round_trip(
            &state,
            read(&state, &id, "catalog-settings", json!({"kind":"settings"})),
            None,
        )
        .await;
        assert!(settings.ok, "{settings:?}");
        assert_eq!(events, 0);
        assert_eq!(
            settings.result.unwrap()["result"],
            json!({"customBrushPresetsPresent":catalog.is_some(),"presetCount":0})
        );
        let (missing, events) = round_trip(
            &state,
            read(
                &state,
                &id,
                "missing-record",
                json!({"kind":"record","id":"none"}),
            ),
            None,
        )
        .await;
        assert_eq!(missing.error.unwrap().code, "not_found");
        assert_eq!(events, 0);
        let binding = subscribe(&state);
        let (noop, events) = round_trip(
            &state,
            edit(
                &state,
                &id,
                "missing-delete",
                0,
                json!({"kind":"delete","id":"none"}),
            ),
            None,
        )
        .await;
        receipt(&noop, 0, false);
        assert_eq!(events, 0);
        assert_eq!(saved(&state, &id), initial);
        let action = json!({"kind":"upsert","id":"\u{0000}日本語","preset":{"rotationMode":"","savedBrushSize":-2.5}});
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
        let mut cancelled = edit(&state, &id, "catalog-cancelled", 0, action.clone());
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
                "raw-identity",
                json!({"kind":"record","id":"\u{0000}日本語"}),
            ),
            None,
        )
        .await;
        assert!(record.ok, "{record:?}");
        assert_eq!(events, 0);
        assert_eq!(
            record.result.unwrap()["result"]["preset"],
            json!({"rotationMode":"","savedBrushSize":-2.5})
        );
        change(
            &state,
            &id,
            &binding,
            1,
            json!({"kind":"delete","id":"\u{0000}日本語"}),
        )
        .await;
        let empty = saved(&state, &id);
        let value: Value = serde_json::from_slice(&empty).unwrap();
        assert_eq!(value["project"]["customBrushPresets"], json!({}));
        for field in ["brushPreset", "brushSize", "favorites", "toolPresets"] {
            assert!(value["project"].get(field).is_none());
        }
        assert!(history(&state, &id, "catalog-undo-remove", 2, false).ok);
        assert!(history(&state, &id, "catalog-undo-insert", 3, false).ok);
        assert_eq!(saved(&state, &id), initial);
        let fresh = ApplicationMcp::default();
        let next = open(&fresh, String::from_utf8(empty.clone()).unwrap());
        assert_eq!(saved(&fresh, &next), empty);
    }
}
