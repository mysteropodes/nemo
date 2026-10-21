//! Original #2226: duplicator output stays a read-only Drawing surface.
use crate::history::NativeObjectHistory;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::request_receipts::DispatchErrorCode;
use serde_json::{json, Value};

fn fixture(locked: Option<bool>) -> Value {
    let cases: Value = serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    ))
    .unwrap();
    let mut records = Vec::new();
    for uid in ["host", "ordinary"] {
        let mut record = cases["records"][0].clone();
        record["target"] = json!({"contextId":"scene-root","layerUid":uid,
            "strokeId":"ink","frameScope":{"kind":"authored","frame":0}});
        records.push(record);
    }
    let mut drawing = json!({"format":"nemo.native-object-document","formatVersion":2,
        "totalFrames":8,"layers":[{"layerUid":"host"},{"layerUid":"ordinary"}],
        "objects":records,"propertyHolders":[{"target":{"contextId":"scene-root",
            "layerUid":"host"},"properties":[{"property":"opacity","static":[50]}]}],
        "compositionOrder":{"format":"nemo.native-object-order-plan","formatVersion":1,
            "documentFormatVersion":2,"migration":"explicit-plan-required","referencePolicy":"unresolved",
            "contexts":[{"contextId":"scene-root","layers":[
                {"layerUid":"host","naturalPosition":0,"order":{}},
                {"layerUid":"ordinary","naturalPosition":1,"order":{}}],
                "scopes":[{"frameScope":{"kind":"authored","frame":0},"elements":[
                    {"layerUid":"host","strokeId":"ink","naturalPosition":0,"order":{}},
                    {"layerUid":"ordinary","strokeId":"ink","naturalPosition":0,"order":{}}]}]}]},
        "componentTables":{"duplicators":{"format":"nemo.native-duplicator-set","formatVersion":1,
            "duplicators":[{"contextId":"scene-root","layerUid":"host",
                "duplicator":{"mode":"grid","rows":1,"cols":3,"spacingX":10}}],"effectorLayers":[]}}});
    if let Some(value) = locked {
        drawing["layers"][0]["locked"] = json!(value);
    }
    drawing
}

fn owner(source: &Value) -> NativeObjectHistory {
    NativeObjectHistory::new(
        "duplicator-lock",
        decode_storage_project(source.to_string().as_bytes()).unwrap(),
    )
    .unwrap()
}

fn state(owner: &NativeObjectHistory) -> (u64, (usize, usize), String, Vec<u8>) {
    let snapshot = owner.acquire_snapshot(owner.content_revision()).unwrap();
    (
        owner.content_revision(),
        owner.history_depths(),
        snapshot.snapshot_id().to_string(),
        encode_storage_project(snapshot.document()).unwrap(),
    )
}

fn policy(uid: &str, value: bool) -> Vec<u8> {
    json!({"command":"object.layer.policy.edit","layerUid":uid,"flag":"locked","value":value})
        .to_string()
        .into_bytes()
}

#[test]
fn duplicator_policy_unlock_is_refused_without_authored_or_history_mutation() {
    for locked in [None, Some(false), Some(true)] {
        let source = fixture(locked);
        let owner = owner(&source);
        let before = state(&owner);
        assert_eq!(
            serde_json::from_slice::<Value>(&before.3).unwrap(),
            source
        );
        assert_eq!(
            owner.prepare_layer_policy_json(&policy("host", false)).unwrap_err(),
            DispatchErrorCode::Unavailable
        );
        assert_eq!(state(&owner), before);
    }
}

#[test]
fn duplicator_fill_and_selection_are_refused_even_when_the_stored_flag_is_absent_or_false() {
    for locked in [None, Some(false), Some(true)] {
        let owner = owner(&fixture(locked));
        let before = state(&owner);
        let fill = json!({"command":"object.fill.set","stableTarget":{"contextId":"scene-root",
            "layerUid":"host","strokeId":"ink","frameScope":{"kind":"authored","frame":0}},
            "fill":{"kind":"solid","r":0.8,"g":0.1,"b":0.3,"a":1}});
        assert_eq!(
            owner.prepare_fill_json(fill.to_string().as_bytes()).unwrap_err(),
            DispatchErrorCode::Unavailable
        );
        let selection = json!({"command":"object.selection.delete","contextId":"scene-root",
            "layerUid":"host","frame":0,"mode":"motion","strokeIds":["ink"]});
        assert_eq!(
            owner.prepare_selection_delete_json(selection.to_string().as_bytes()).unwrap_err(),
            DispatchErrorCode::Unavailable
        );
        assert_eq!(state(&owner), before);
    }
}

#[test]
fn forced_drawing_lock_preserves_motion_edits_with_one_history_entry_and_exact_undo_redo() {
    let source = fixture(Some(false));
    let mut owner = owner(&source);
    let before = state(&owner);
    let request = json!({"command":"object.property.edit","target":{"contextId":"scene-root",
        "layerUid":"host"},"property":"opacity","action":{"kind":"setValue","frame":0,"values":[60]}});
    let prepared = owner.prepare_property_edit_json(request.to_string().as_bytes()).unwrap();
    assert_eq!(state(&owner), before);
    assert_eq!(owner.commit_property_edit(prepared), Ok(1));
    assert_eq!(owner.history_depths(), (1, 0));
    let changed = state(&owner);
    let value: Value = serde_json::from_slice(&changed.3).unwrap();
    assert_eq!(value["layers"], source["layers"]);
    assert_eq!(value["objects"], source["objects"]);
    assert_eq!(value["componentTables"], source["componentTables"]);
    assert_eq!(value["propertyHolders"][0]["properties"][0]["static"], json!([60]));
    assert_eq!(owner.undo(1), Ok(2));
    assert_eq!(state(&owner).3, before.3);
    assert_eq!(owner.redo(2), Ok(3));
    assert_eq!(state(&owner).3, changed.3);
}

#[test]
fn an_ordinary_layer_retains_its_original_lock_and_unlock_history() {
    let mut owner = owner(&fixture(Some(false)));
    let before = state(&owner);
    let lock = owner.prepare_layer_policy_json(&policy("ordinary", true)).unwrap();
    assert_eq!(owner.commit_layer_policy(lock), Ok(1));
    let unlock = owner.prepare_layer_policy_json(&policy("ordinary", false)).unwrap();
    assert_eq!(owner.commit_layer_policy(unlock), Ok(2));
    assert_eq!(owner.history_depths(), (2, 0));
    assert_eq!(owner.undo(2), Ok(3));
    assert_eq!(owner.undo(3), Ok(4));
    assert_eq!(state(&owner).3, before.3);
}
