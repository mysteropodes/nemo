//! Typed refusals of the raster transactions: every refusal leaves revision, history depths, snapshot id and the
//! canonical bytes exactly as they were (no partial publication).
use super::super::support::{
    drawing, head, layers_with_widget, legacy_record, owner, prepare, raster_item, run, state, target,
};
use crate::history::object_history::NativeObjectHistory;
use crate::request_receipts::DispatchErrorCode as E;
use serde_json::{json, Value};

/// L0 holds item `r0`, the locked layer L1 holds item `rl`, L2 holds the legacy path record `v0`, W is a widget layer.
fn world() -> NativeObjectHistory {
    owner(&drawing(json!({
        "layers":layers_with_widget(),
        "objects":[legacy_record("L2","v0")],
        "items":{"raster":[
            {"target":target("L0","r0"),"item":raster_item("img-1",1,2,3,4)},
            {"target":target("L1","rl"),"item":raster_item("img-1",5,6,7,8)}]}
    })))
}

fn t(layer: &str, stroke: &str) -> Value {
    target(layer, stroke)
}

fn create_at(target: Value, item: Value) -> Value {
    json!({"command":"object.raster.create","target":target,"item":item})
}

#[test]
fn every_refusal_has_its_typed_code_and_publishes_nothing() {
    let ok_item = || raster_item("img-1", 1, 2, 3, 4);
    let mut frame8 = t("L0", "");
    frame8["frameScope"]["frame"] = json!(8);
    let mut reference = t("L0", "");
    reference["frameScope"]["kind"] = json!("reference");
    let mut other_context = t("L0", "");
    other_context["contextId"] = json!("other");
    let mut frame99 = t("L0", "r0");
    frame99["frameScope"]["frame"] = json!(99);
    let cases: Vec<(&str, Value, E)> = vec![
        (
            "create on the locked layer",
            create_at(t("L1", ""), ok_item()),
            E::Unavailable,
        ),
        (
            "create on an unknown layer",
            create_at(t("L9", ""), ok_item()),
            E::NotFound,
        ),
        (
            "create on a widget layer",
            create_at(t("W", ""), ok_item()),
            E::InvalidRequest,
        ),
        (
            "create past the last frame",
            create_at(frame8, ok_item()),
            E::InvalidRequest,
        ),
        (
            "create in a reference scope",
            create_at(reference, ok_item()),
            E::Unavailable,
        ),
        (
            "create in a foreign context",
            create_at(other_context, ok_item()),
            E::InvalidRequest,
        ),
        (
            "create with a caller chosen stroke id",
            create_at(t("L0", "mine"), ok_item()),
            E::InvalidRequest,
        ),
        (
            "create over a missing asset",
            create_at(t("L0", ""), raster_item("nope", 1, 2, 3, 4)),
            E::NotFound,
        ),
        (
            "create over a font asset",
            create_at(t("L0", ""), raster_item("font-1", 1, 2, 3, 4)),
            E::InvalidRequest,
        ),
        (
            "create with a member of another command",
            json!({"command":"object.raster.create","target":t("L0",""),"item":ok_item(),"x":1}),
            E::InvalidRequest,
        ),
        (
            "create without the item",
            json!({"command":"object.raster.create","target":t("L0","")}),
            E::InvalidRequest,
        ),
        (
            "create with a zero width item",
            create_at(t("L0", ""), raster_item("img-1", 1, 2, 0, 4)),
            E::InvalidRequest,
        ),
        (
            "create with an unknown item member",
            create_at(
                t("L0", ""),
                json!({"assetId":"img-1","x":1,"y":2,"width":3,"height":4,"groupId":"g"}),
            ),
            E::InvalidRequest,
        ),
        (
            "create with a null opacity",
            create_at(
                t("L0", ""),
                json!({"assetId":"img-1","x":1,"y":2,"width":3,"height":4,"opacity":null}),
            ),
            E::InvalidRequest,
        ),
        (
            "move an unknown stroke",
            json!({"command":"object.raster.move","stableTarget":t("L0","nope"),"x":1,"y":2}),
            E::NotFound,
        ),
        (
            "move on the locked layer",
            json!({"command":"object.raster.move","stableTarget":t("L1","rl"),"x":1,"y":2}),
            E::Unavailable,
        ),
        (
            "move a path record",
            json!({"command":"object.raster.move","stableTarget":t("L2","v0"),"x":1,"y":2}),
            E::Unavailable,
        ),
        (
            "move past the last frame",
            json!({"command":"object.raster.move","stableTarget":frame99,"x":1,"y":2}),
            E::InvalidRequest,
        ),
        (
            "move without y",
            json!({"command":"object.raster.move","stableTarget":t("L0","r0"),"x":1}),
            E::InvalidRequest,
        ),
        (
            "move with an opacity",
            json!({"command":"object.raster.move","stableTarget":t("L0","r0"),"x":1,"y":2,"opacity":1}),
            E::InvalidRequest,
        ),
        (
            "resize to zero",
            json!({"command":"object.raster.resize","stableTarget":t("L0","r0"),"width":0,"height":5}),
            E::InvalidRequest,
        ),
        (
            "resize to negative",
            json!({"command":"object.raster.resize","stableTarget":t("L0","r0"),"width":5,"height":-1}),
            E::InvalidRequest,
        ),
        (
            "opacity above one",
            json!({"command":"object.raster.opacity","stableTarget":t("L0","r0"),"opacity":1.5}),
            E::InvalidRequest,
        ),
        (
            "opacity below zero",
            json!({"command":"object.raster.opacity","stableTarget":t("L0","r0"),"opacity":-0.1}),
            E::InvalidRequest,
        ),
        (
            "rotate with an x",
            json!({"command":"object.raster.rotate","stableTarget":t("L0","r0"),"rotation":3,"x":1}),
            E::InvalidRequest,
        ),
        (
            "rotate a null",
            json!({"command":"object.raster.rotate","stableTarget":t("L0","r0"),"rotation":null}),
            E::InvalidRequest,
        ),
        (
            "delete an unknown stroke",
            json!({"command":"object.raster.delete","stableTarget":t("L0","nope")}),
            E::NotFound,
        ),
        (
            "delete on the locked layer",
            json!({"command":"object.raster.delete","stableTarget":t("L1","rl")}),
            E::Unavailable,
        ),
        (
            "delete with a target member",
            json!({"command":"object.raster.delete","stableTarget":t("L0","r0"),"target":t("L0","")}),
            E::InvalidRequest,
        ),
        (
            "a text command on a raster target",
            json!({"command":"object.text.delete","stableTarget":t("L0","r0")}),
            E::Unavailable,
        ),
        (
            "an unknown raster command",
            json!({"command":"object.raster.flip","stableTarget":t("L0","r0")}),
            E::InvalidRequest,
        ),
        (
            "a foreign command family",
            json!({"command":"object.vector.create","target":t("L0","")}),
            E::InvalidRequest,
        ),
    ];
    assert_eq!(cases.len(), 32);
    for (label, payload, code) in cases {
        let mut o = world();
        let before = state(&o);
        assert_eq!(run(&mut o, &payload), Err(code), "{label}");
        assert_eq!(
            state(&o),
            before,
            "{label}: nothing may be partially published"
        );
    }
}

#[test]
fn raw_payload_shapes_that_a_value_would_launder_are_refused_atomically() {
    let o = world();
    let before = state(&o);
    let valid = json!({"command":"object.raster.delete","stableTarget":t("L0","r0")}).to_string();
    let duplicate = format!(
        "{{{},\"command\":\"object.raster.delete\"}}",
        &valid[1..valid.len() - 1]
    );
    let oversize = format!("{valid}{}", " ".repeat(4097));
    for raw in [
        "null".to_string(),
        "[]".into(),
        "7".into(),
        "\"object.raster.delete\"".into(),
        "{}".into(),
        duplicate,
        oversize,
        format!("{valid} trailing"),
    ] {
        assert_eq!(
            o.prepare_item_edit_json(raw.as_bytes()).unwrap_err(),
            E::InvalidRequest,
            "{}",
            &raw[..raw.len().min(60)]
        );
        assert_eq!(state(&o), before);
    }
}

#[test]
fn a_prepared_candidate_pins_its_origin_and_a_stale_or_foreign_commit_publishes_nothing() {
    let mut o = world();
    let payload = json!({"command":"object.raster.move","stableTarget":t("L0","r0"),"x":9,"y":9});
    let stale = prepare(&o, &payload).unwrap();
    let winner = prepare(
        &o,
        &json!({"command":"object.raster.opacity","stableTarget":t("L0","r0"),"opacity":0.5}),
    )
    .unwrap();
    assert_eq!(o.commit_item_edit(winner), Ok(1));
    let after = state(&o);
    assert_eq!(o.commit_item_edit(stale), Err(E::StaleRevision));
    assert_eq!(state(&o), after);
    // Even after an undo restores the very same bytes, an older token stays refused.
    let old = prepare(&o, &payload).unwrap();
    o.undo(1).unwrap();
    let restored = state(&o);
    assert_eq!(o.commit_item_edit(old), Err(E::StaleRevision));
    assert_eq!(state(&o), restored);
    // Another owner: the token is bound to the instance and document it was prepared on.
    let other = world();
    let foreign = prepare(&other, &payload).unwrap();
    let before = state(&o);
    assert_eq!(o.commit_item_edit(foreign), Err(E::WrongDocument));
    assert_eq!(state(&o), before);
}

fn remove_asset_request(o: &NativeObjectHistory, id: &str) -> crate::request_receipts::OpacityRequest {
    let current = head(o);
    serde_json::from_value(json!({"apiVersion":2,"requestId":id,
        "instanceId":o.instance_id(),"documentId":o.document_id(),
        "expectedRevision":o.content_revision(),"operation":"command.document.object.asset.remove",
        "payload":{"documentSnapshotId":current.snapshot_id(),"assetId":"img-1"}})).unwrap()
}

#[test]
fn stored_raster_references_refuse_asset_removal_without_publication_or_history_change() {
    for (layer, frame, hidden, locked) in [("L0", 0, false, false), ("L1", 0, false, true), ("L2", 7, true, false)] {
        let mut item_target = target(layer, "held-raster");
        item_target["frameScope"]["frame"] = json!(frame);
        let mut doc = drawing(json!({"items":{"raster":[
            {"target":item_target,"item":raster_item("img-1",1,2,3,4)}]}}));
        let layer_value = doc["layers"].as_array_mut().unwrap().iter_mut()
            .find(|value| value["layerUid"] == layer).unwrap();
        layer_value["visible"] = json!(!hidden);
        layer_value["locked"] = json!(locked);
        let mut o = owner(&doc);
        let before = state(&o);
        let held = head(&o);
        let refusal = held.prepare_asset_removal("img-1").unwrap_err();
        assert_eq!((refusal.code(), refusal.reason()), (E::BusyConflict, "asset_in_use"));
        assert_eq!(state(&o), before);
        let request = remove_asset_request(&o, "remove-stored-raster");
        let first = o.dispatch_asset_command(&request, None).unwrap();
        assert_eq!(first.error().unwrap().code(), E::BusyConflict);
        assert_eq!(serde_json::to_value(&first).unwrap()["error"]["details"]["reason"], "asset_in_use");
        assert_eq!(o.dispatch_asset_command(&request, None).unwrap(), first);
        assert_eq!(state(&o), before, "layer={layer}, frame={frame}");
        let after = head(&o);
        assert!(std::sync::Arc::ptr_eq(
            &held.project().unwrap().asset_inventory().unwrap().records()[0],
            &after.project().unwrap().asset_inventory().unwrap().records()[0]));
    }
}

#[test]
fn deleting_the_last_raster_allows_removal_and_undo_redo_restore_both_owners() {
    let mut o = owner(&drawing(json!({"items":{"raster":[
        {"target":target("L0","r0"),"item":raster_item("img-1",1,2,3,4)}]}})));
    let original = state(&o);
    let held = head(&o);
    assert_eq!(run(&mut o, &json!({"command":"object.raster.delete","stableTarget":target("L0","r0")})), Ok(1));
    let deleted = state(&o);
    assert_eq!(o.history_depths(), (1, 0));
    assert!(head(&o).prepare_asset_removal("img-1").is_ok());
    let response = o.dispatch_asset_command(&remove_asset_request(&o, "remove-unreferenced"), None).unwrap();
    assert!(response.is_ok());
    assert_eq!(o.content_revision(), 2);
    assert_eq!(o.history_depths(), (2, 0));
    let id = crate::object_document::AssetId::new("img-1".into()).unwrap();
    assert!(head(&o).project().unwrap().asset_inventory().unwrap().get(&id).is_none());
    o.undo(2).unwrap();
    assert_eq!(state(&o).3, deleted.3);
    assert!(std::sync::Arc::ptr_eq(
        &held.project().unwrap().asset_inventory().unwrap().records()[0],
        &head(&o).project().unwrap().asset_inventory().unwrap().records()[0]));
    o.undo(3).unwrap();
    assert_eq!(state(&o).3, original.3);
    assert_eq!(o.history_depths(), (0, 2));
    assert_eq!(head(&o).prepare_asset_removal("img-1").unwrap_err().code(), E::BusyConflict);
    assert_eq!(o.history_depths(), (0, 2));
    o.redo(4).unwrap();
    o.redo(5).unwrap();
    assert_eq!(state(&o).3, deleted.3);
    assert!(head(&o).project().unwrap().asset_inventory().unwrap().get(&id).is_none());
    assert_eq!(o.history_depths(), (2, 0));
}
