//! PRIVATE UNRUN: real codec/sole-owner oracles for admitted Drawing locks.
use crate::history::NativeObjectHistory;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::request_receipts::{DispatchErrorCode, OpacityRequest};
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};
fn target(layer: &str, element: bool) -> Value {
    let mut value = json!({"contextId":"scene-root","layerUid":layer});
    if element {
        value["strokeId"] = json!("ink");
    }
    value
}
fn fixture(version: u32, locked: Option<bool>) -> Value {
    let base: Value = serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    ))
    .unwrap();
    let mut objects = vec![];
    for layer in ["L0", "L1"] {
        let mut record = base["records"][0].clone();
        record["target"] = json!({"contextId":"scene-root","layerUid":layer,"strokeId":"ink",
            "frameScope":{"kind":"authored","frame":0}});
        objects.push(record);
    }
    let mut value = json!({"format":"nemo.native-object-document","formatVersion":version,"totalFrames":8,
        "layers":[{"layerUid":"L0","folderId":"g","shy":true},{"layerUid":"L1"}],
        "objects":objects,"layerFolders":{"g":{"name":"Preserve group","collapsed":false}},
        "propertyHolders":[
            {"target":target("L0",false),"properties":[{"property":"opacity","track":{"keys":[
                {"frame":0,"v":[10],"curvePoints":[]},{"frame":4,"v":[90],"curvePoints":[]}]}}]},
            {"target":target("L1",false),"properties":[{"property":"opacity","track":{"keys":[
                {"frame":0,"v":[10],"curvePoints":[]},{"frame":4,"v":[90],"curvePoints":[]}]}}]},
            {"target":target("L0",true),"properties":[{"property":"opacity","static":[50]}]}]});
    if let Some(locked) = locked {
        value["layers"][0]["locked"] = json!(locked);
    }
    if version == 2 {
        value["compositionOrder"] = json!({"format":"nemo.native-object-order-plan","formatVersion":1,
            "documentFormatVersion":2,"migration":"explicit-plan-required","referencePolicy":"unresolved",
            "contexts":[{"contextId":"scene-root","layers":[
                {"layerUid":"L0","naturalPosition":0,"order":{}},{"layerUid":"L1","naturalPosition":1,"order":{}}],
                "scopes":[{"frameScope":{"kind":"authored","frame":0},"elements":[
                    {"layerUid":"L0","strokeId":"ink","naturalPosition":0,"order":{}},
                    {"layerUid":"L1","strokeId":"ink","naturalPosition":0,"order":{}}]}]}]});
    }
    value
}
fn document(value: &Value) -> crate::object_document::ObjectDocument {
    decode_storage_project(value.to_string().as_bytes()).unwrap()
}
fn owner(value: &Value) -> NativeObjectHistory {
    NativeObjectHistory::new("layer-lock", document(value)).unwrap()
}
fn head(owner: &NativeObjectHistory) -> ObjectSnapshot {
    owner.acquire_snapshot(owner.content_revision()).unwrap()
}
fn bytes(owner: &NativeObjectHistory) -> Vec<u8> {
    encode_storage_project(head(owner).document()).unwrap()
}
fn state(owner: &NativeObjectHistory) -> (u64, (usize, usize), String, Vec<u8>) {
    (
        owner.content_revision(),
        owner.history_depths(),
        head(owner).snapshot_id().into(),
        bytes(owner),
    )
}
fn fill(layer: &str) -> Value {
    json!({"command":"object.fill.set","stableTarget":{"contextId":"scene-root","layerUid":layer,
        "strokeId":"ink","frameScope":{"kind":"authored","frame":0}},
        "fill":{"kind":"solid","r":0.8,"g":0.1,"b":0.3,"a":1}})
}
fn property(element: bool, action: Value) -> Value {
    json!({"command":"object.property.edit","target":target("L0",element),"property":"opacity","action":action})
}
fn track(action: Value) -> Value {
    // Mixed unlocked/locked layers remain valid generic Motion selections.
    // Each actual track is edited once through the existing transaction owner.
    json!({"command":"object.property.track.edit","selections":[
        {"target":target("L1",false),"property":"opacity","frame":0},
        {"target":target("L0",false),"property":"opacity","frame":0},
        {"target":target("L1",false),"property":"opacity","frame":4},
        {"target":target("L0",false),"property":"opacity","frame":4}],"action":action})
}
fn edit_fill(owner: &mut NativeObjectHistory, layer: &str) -> Result<u64, DispatchErrorCode> {
    let token = owner.prepare_fill_json(fill(layer).to_string().as_bytes())?;
    owner.commit_fill(token)
}
#[test]
fn lock_absence_false_true_are_strict_and_roundtrip_both_native_versions() {
    for version in [1, 2] {
        for lock in [None, Some(false), Some(true)] {
            let source = fixture(version, lock);
            let doc = document(&source);
            assert_eq!(doc.layers()[0].locked(), lock.unwrap_or(false));
            assert_eq!(
                serde_json::from_slice::<Value>(&encode_storage_project(&doc).unwrap()).unwrap(),
                source
            );
            let mut parented = source.clone();
            parented["layers"][0]["parentLayerUid"] = json!("L1");
            if version == 2 {
                assert_eq!(
                    serde_json::from_slice::<Value>(
                        &encode_storage_project(&document(&parented)).unwrap()
                    )
                    .unwrap(),
                    parented
                );
            } else {
                assert!(decode_storage_project(parented.to_string().as_bytes()).is_err());
            }
            for parent in ["", "L0", "absent"] {
                let mut invalid = source.clone();
                invalid["layers"][0]["parentLayerUid"] = json!(parent);
                assert!(decode_storage_project(invalid.to_string().as_bytes()).is_err());
            }
        }
        for family in ["symbolId", "isFolderLayer", "isNullLayer", "isGuideLayer"] {
            let mut unsupported = fixture(version, Some(true));
            unsupported["layers"][0][family] = if family == "symbolId" {
                json!("component")
            } else {
                json!(true)
            };
            assert!(decode_storage_project(unsupported.to_string().as_bytes()).is_err());
        }
        for bad in [Value::Null, json!(0), json!("true"), json!({}), json!([])] {
            let mut source = fixture(version, None);
            source["layers"][0]["locked"] = bad;
            assert!(decode_storage_project(source.to_string().as_bytes()).is_err());
        }
        let raw = fixture(version, Some(true))
            .to_string()
            .replace("\"locked\":true", "\"locked\":true,\"locked\":false");
        assert!(decode_storage_project(raw.as_bytes()).is_err());
    }
}
#[test]
fn locked_fill_and_semantic_noop_refuse_without_touching_bytes_revision_or_redo() {
    for version in [1, 2] {
        let source = fixture(version, Some(true));
        let mut o = owner(&source);
        edit_fill(&mut o, "L1").unwrap();
        o.undo(1).unwrap();
        let before = state(&o);
        assert_eq!(o.history_depths(), (0, 1));
        for paint in [
            fill("L0")["fill"].clone(),
            source["objects"][0]["fill"].clone(),
        ] {
            let mut request = fill("L0");
            request["fill"] = paint;
            assert_eq!(
                o.prepare_fill_json(request.to_string().as_bytes())
                    .unwrap_err(),
                DispatchErrorCode::Unavailable
            );
            assert_eq!(
                crate::commands::prepare_object_fill_json(
                    &head(&o),
                    request.to_string().as_bytes()
                )
                .unwrap_err(),
                DispatchErrorCode::Unavailable
            );
            assert_eq!(state(&o), before);
        }
        assert_eq!(o.redo(2), Ok(3));
    }
}
#[test]
fn locked_layer_and_element_motion_value_key_remove_and_animated_actions_remain_permitted() {
    for version in [1, 2] {
        for element in [false, true] {
            for (kind, action, changed) in [
                (
                    "value",
                    json!({"kind":"setValue","frame":2,"values":[30]}),
                    true,
                ),
                (
                    "key",
                    json!({"kind":"setKey","frame":2,"values":[30],"curvePoints":[]}),
                    true,
                ),
                ("remove", json!({"kind":"removeKey","frame":0}), !element),
                (
                    "animated",
                    json!({"kind":"setAnimated","frame":0,"animated":false}),
                    !element,
                ),
            ] {
                let source = fixture(version, Some(true));
                let mut o = owner(&source);
                edit_fill(&mut o, "L1").unwrap();
                o.undo(1).unwrap();
                let old = head(&o);
                let initial = bytes(&o);
                assert_eq!(o.history_depths(), (0, 1));
                let request = property(element, action);
                let before_prepare = state(&o);
                let prepared = o.prepare_property_edit_json(request.to_string().as_bytes());
                // A history-only build has no evaluator for baking a keyed value.
                // Assert its refusal; retain the positive oracle when evaluation exists.
                if !cfg!(feature = "evaluation") && kind == "animated" && !element {
                    assert_eq!(prepared.unwrap_err(), DispatchErrorCode::Unavailable);
                    assert_eq!(state(&o), before_prepare);
                    assert_eq!(serde_json::to_value(old.document()).unwrap(), source);
                    assert_eq!(o.redo(2), Ok(3));
                    continue;
                }
                let token = prepared.unwrap();
                assert_eq!(
                    o.commit_property_edit(token),
                    Ok(if changed { 3 } else { 2 })
                );
                let current = serde_json::to_value(head(&o).document()).unwrap();
                assert_eq!(current["layers"], source["layers"]);
                assert_eq!(current["layerFolders"], source["layerFolders"]);
                assert_eq!(current["objects"], source["objects"]);
                let record =
                    &current["propertyHolders"][if element { 2 } else { 0 }]["properties"][0];
                match kind {
                    "value" if element => assert_eq!(record["static"], json!([30])),
                    "value" | "key" => {
                        let keys = record["track"]["keys"].as_array().unwrap();
                        let at_two = keys
                            .iter()
                            .find(|k| k["frame"].as_f64() == Some(2.))
                            .unwrap();
                        assert_eq!(at_two["v"], json!([30]));
                    }
                    "remove" if !element => assert_eq!(
                        record["track"]["keys"],
                        json!([{"frame":4,"v":[90],"curvePoints":[]}])
                    ),
                    "animated" if !element => {
                        assert_eq!(record["static"][0].as_f64(), Some(10.));
                        assert_eq!(record["track"]["keys"], json!([]));
                    }
                    _ => assert_eq!(record, &source["propertyHolders"][2]["properties"][0]),
                }
                assert_eq!(serde_json::to_value(old.document()).unwrap(), source);
                if changed {
                    let after = bytes(&o);
                    assert_ne!(after, initial);
                    assert_eq!(o.history_depths(), (1, 0));
                    assert_eq!(o.undo(3), Ok(4));
                    assert_eq!(bytes(&o), initial);
                    assert_eq!(o.redo(4), Ok(5));
                    assert_eq!(bytes(&o), after);
                } else {
                    assert_eq!(bytes(&o), initial);
                    assert_eq!(o.history_depths(), (0, 1));
                }
                let before_refusal = state(&o);
                assert_eq!(
                    o.prepare_fill_json(fill("L0").to_string().as_bytes())
                        .unwrap_err(),
                    DispatchErrorCode::Unavailable
                );
                assert_eq!(state(&o), before_refusal);
            }
        }
    }
}
#[test]
fn mixed_locked_unlocked_motion_tracks_keep_preset_influence_and_subdivision_permitted() {
    for version in [1, 2] {
        for (kind, action) in [
            ("preset", json!({"kind":"preset","preset":"easeOut"})),
            (
                "influence",
                json!({"kind":"influence","side":"out","percent":50}),
            ),
            ("subdivide", json!({"kind":"subdivide"})),
        ] {
            let source = fixture(version, Some(true));
            let mut o = owner(&source);
            let zero = head(&o);
            let request = track(action);
            let before_prepare = state(&o);
            let prepared = o.prepare_property_track_edit_json(request.to_string().as_bytes());
            // Subdivision samples new values; curve-only edits need no evaluator.
            if !cfg!(feature = "evaluation") && kind == "subdivide" {
                assert_eq!(prepared.unwrap_err(), DispatchErrorCode::Unavailable);
                assert_eq!(state(&o), before_prepare);
                assert_eq!(serde_json::to_value(zero.document()).unwrap(), source);
                continue;
            }
            let token = prepared.unwrap();
            assert_eq!(o.commit_property_edit(token), Ok(1));
            assert_eq!(o.history_depths(), (1, 0));
            let current = serde_json::to_value(head(&o).document()).unwrap();
            assert_eq!(current["layers"], source["layers"]);
            assert_eq!(current["layerFolders"], source["layerFolders"]);
            assert_eq!(current["objects"], source["objects"]);
            assert_eq!(current["propertyHolders"][2], source["propertyHolders"][2]);
            for index in [0, 1] {
                let keys = &current["propertyHolders"][index]["properties"][0]["track"]["keys"];
                assert_ne!(
                    keys,
                    &source["propertyHolders"][index]["properties"][0]["track"]["keys"]
                );
                if kind == "subdivide" {
                    assert_eq!(
                        keys.as_array()
                            .unwrap()
                            .iter()
                            .map(|k| k["frame"].as_f64().unwrap())
                            .collect::<Vec<_>>(),
                        vec![0., 2., 4.]
                    );
                    assert_eq!(keys[1]["v"][0].as_f64(), Some(50.));
                } else {
                    assert_eq!(keys.as_array().unwrap().len(), 2);
                    assert_eq!(keys[0]["v"], json!([10]));
                    assert_eq!(keys[1]["v"], json!([90]));
                }
            }
            let after = bytes(&o);
            assert_eq!(o.undo(1), Ok(2));
            assert_eq!(bytes(&o), encode_storage_project(zero.document()).unwrap());
            assert_eq!(o.redo(2), Ok(3));
            assert_eq!(bytes(&o), after);
            assert_eq!(serde_json::to_value(zero.document()).unwrap(), source);
        }
    }
}
#[test]
fn missing_and_false_lock_preserve_normal_fill_property_track_and_undo_behavior() {
    for version in [1, 2] {
        for lock in [None, Some(false)] {
            let source = fixture(version, lock);
            let mut o = owner(&source);
            let zero = head(&o);
            assert_eq!(edit_fill(&mut o, "L0"), Ok(1));
            let request = property(
                false,
                json!({"kind":"setKey","frame":0,"values":[25],"curvePoints":[]}),
            );
            let token = o
                .prepare_property_edit_json(request.to_string().as_bytes())
                .unwrap();
            assert_eq!(o.commit_property_edit(token), Ok(2));
            let request = track(json!({"kind":"preset","preset":"easeOut"}));
            let token = o
                .prepare_property_track_edit_json(request.to_string().as_bytes())
                .unwrap();
            assert_eq!(o.commit_property_edit(token), Ok(3));
            let after = bytes(&o);
            assert_eq!(o.history_depths(), (3, 0));
            assert_eq!(o.undo(3), Ok(4));
            assert_eq!(o.redo(4), Ok(5));
            assert_eq!(bytes(&o), after);
            let roundtrip = decode_storage_project(&after).unwrap();
            assert_eq!(encode_storage_project(&roundtrip).unwrap(), after);
            assert_eq!(serde_json::to_value(zero.document()).unwrap(), source);
            assert_eq!(
                serde_json::to_value(head(&o).document()).unwrap()["layers"],
                source["layers"]
            );
        }
    }
}
#[test]
fn locked_eye_still_changes_and_noop_preserves_redo_with_lock_and_metadata_intact() {
    let source = fixture(2, Some(true));
    let mut o = owner(&source);
    let request = json!({"command":"object.layer.visibility.set","layerUid":"L0","visible":false});
    let token = o
        .prepare_layer_visibility_json(request.to_string().as_bytes())
        .unwrap();
    assert_eq!(o.commit_layer_visibility(token), Ok(1));
    assert!(head(&o).document().layers()[0].locked());
    assert!(!head(&o).document().layers()[0].visible());
    assert_eq!(o.undo(1), Ok(2));
    let noop = json!({"command":"object.layer.visibility.set","layerUid":"L0","visible":true});
    let token = o
        .prepare_layer_visibility_json(noop.to_string().as_bytes())
        .unwrap();
    assert_eq!(o.commit_layer_visibility(token), Ok(2));
    assert_eq!(o.history_depths(), (0, 1));
    assert_eq!(o.redo(2), Ok(3));
    assert!(head(&o).document().layers()[0].locked());
    let current = serde_json::to_value(head(&o).document()).unwrap();
    assert_eq!(current["layerFolders"], source["layerFolders"]);
    assert_eq!(current["layers"][0]["shy"], source["layers"][0]["shy"]);
}
#[test]
fn add_is_allowed_when_all_existing_layers_are_locked_and_new_drawing_is_unlocked() {
    for version in [1, 2] {
        let mut source = fixture(version, Some(true));
        source["layers"][1]["locked"] = json!(true);
        let mut o = owner(&source);
        let request = json!({"command":"object.layer.create","name":"New drawing"});
        let token = o
            .prepare_layer_create_json(request.to_string().as_bytes())
            .unwrap();
        assert_eq!(o.commit_layer_create(token), Ok(1));
        let current = serde_json::to_value(head(&o).document()).unwrap();
        assert_eq!(current["layers"][0], source["layers"][0]);
        assert_eq!(current["layers"][1], source["layers"][1]);
        assert_eq!(current["layers"][2]["locked"], false);
        assert_eq!(current["layerFolders"], source["layerFolders"]);
    }
}
#[test]
fn staged_locked_fill_refusal_replay_cancel_and_stale_preserve_redo() {
    let mut o = owner(&fixture(1, Some(true)));
    edit_fill(&mut o, "L1").unwrap();
    o.undo(1).unwrap();
    let before = state(&o);
    let request: OpacityRequest =
        serde_json::from_value(json!({"apiVersion":2,"requestId":"locked-fill",
        "instanceId":o.instance_id(),"documentId":o.document_id(),"expectedRevision":2,
        "operation":"command.document.object.fill.set",
        "payload":fill("L0")}))
        .unwrap();
    let first = o.try_dispatch_object_command(&request).unwrap().unwrap();
    let value = serde_json::to_value(&first).unwrap();
    assert_eq!(value["ok"], false);
    assert_eq!(value["error"]["code"], "unavailable");
    let replay = o.try_dispatch_object_command(&request).unwrap().unwrap();
    assert_eq!(serde_json::to_value(replay).unwrap(), value);
    assert_eq!(state(&o), before);
    let mut cancelled = request.clone();
    cancelled.request_id = "locked-cancel".into();
    cancelled.cancelled_before_dispatch = true;
    let cancelled_reply = o.try_dispatch_object_command(&cancelled).unwrap().unwrap();
    assert!(!cancelled_reply.ok);
    assert_eq!(
        cancelled_reply.error.unwrap().code(),
        DispatchErrorCode::CancelledBeforeDispatch
    );
    assert_eq!(state(&o), before);
    let mut stale = request;
    stale.request_id = "locked-stale".into();
    stale.expected_revision = Some(0);
    let stale_reply = o.try_dispatch_object_command(&stale).unwrap().unwrap();
    assert!(!stale_reply.ok);
    assert_eq!(
        stale_reply.error.unwrap().code(),
        DispatchErrorCode::StaleRevision
    );
    assert_eq!(state(&o), before);
}
#[test]
fn foreign_unlocked_candidates_cannot_bypass_locked_owner_origin_fences() {
    let unlocked = owner(&fixture(2, Some(false)));
    let mut locked = owner(&fixture(2, Some(true)));
    let before = state(&locked);
    let fill_token = unlocked
        .prepare_fill_json(fill("L0").to_string().as_bytes())
        .unwrap();
    assert_eq!(
        locked.commit_fill(fill_token),
        Err(DispatchErrorCode::WrongDocument)
    );
    assert_eq!(state(&locked), before);
    let request = property(
        false,
        json!({"kind":"setKey","frame":2,"values":[25],"curvePoints":[]}),
    );
    let property_token = unlocked
        .prepare_property_edit_json(request.to_string().as_bytes())
        .unwrap();
    assert_eq!(
        locked.commit_property_edit(property_token),
        Err(DispatchErrorCode::WrongDocument)
    );
    assert_eq!(state(&locked), before);
    let request = track(json!({"kind":"preset","preset":"easeIn"}));
    let track_token = unlocked
        .prepare_property_track_edit_json(request.to_string().as_bytes())
        .unwrap();
    assert_eq!(
        locked.commit_property_edit(track_token),
        Err(DispatchErrorCode::WrongDocument)
    );
    assert_eq!(state(&locked), before);
}

#[cfg(feature = "project_media_codec")]
#[path = "object_layer_lock_project.rs"]
mod project_tests;
