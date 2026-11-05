//! #2371 D36: folder membership (`layer.folder.reparent`) through the actual owner, history and dispatcher: in and out of a folder as
//! single entries, the world pose kept (or a typed refusal), the drag-out drop that also reorders, and the typed refusals.
use super::object_layer_kind_create::{
    created, drawn, kind, layer_of, position_of, refused, send,
};
use crate::history::object_history::object_layer_lifecycle_tests::{
    base, bytes, owner_of, selection, value,
};
use crate::history::object_history::NativeObjectHistory;
use crate::object_codec::decode_storage_project;
use serde_json::{json, Value};

fn reparent(uids: &[&str], folder: Option<&str>) -> Value {
    json!({"command":"object.layer.folder.reparent","layerUids":uids,"folderUid":folder,"frame":0})
}

#[test]
fn layers_move_into_a_folder_and_out_again_as_single_entries_and_the_folder_opens() {
    let mut owner = owner_of(&base(2));
    let folder = created(
        &mut owner,
        "f",
        &kind("folder", "Folder 4", json!({"pos":[0, 0]})),
    );
    let fid = folder["layerUid"].as_str().unwrap().to_owned();
    send(
        &mut owner,
        "collapse",
        "compositing.edit",
        &json!({"command":"object.layer.compositing.edit","layerUid":fid,"edit":"folderCollapsed","collapsed":true}),
    );
    assert_eq!(layer_of(&owner, &fid)["folderCollapsed"], true);
    let before = bytes(&owner);
    let depths = owner.history_depths();
    let reply = send(
        &mut owner,
        "in",
        "folder.reparent",
        &reparent(&["L0", "L2", &fid], Some(&fid)),
    );
    assert_eq!(
        reply["result"],
        json!({"applied":true,"historyEntriesAdded":1}),
        "{reply}"
    );
    assert_eq!(layer_of(&owner, "L0")["parentLayerUid"], fid.as_str());
    assert_eq!(layer_of(&owner, "L2")["parentLayerUid"], fid.as_str());
    assert_eq!(
        layer_of(&owner, &fid)["parentLayerUid"],
        Value::Null,
        "a folder never nests in itself"
    );
    assert_eq!(
        layer_of(&owner, &fid)["folderCollapsed"],
        false,
        "the drop is revealed"
    );
    assert_eq!(owner.history_depths().0, depths.0 + 1);
    let inside = bytes(&owner);
    // moving what is already inside changes nothing
    let reply = send(
        &mut owner,
        "again",
        "folder.reparent",
        &reparent(&["L0"], Some(&fid)),
    );
    assert_eq!(
        reply["result"],
        json!({"applied":false,"historyEntriesAdded":0}),
        "{reply}"
    );
    // out: only the layers whose parent is a folder move
    let reply = send(
        &mut owner,
        "out",
        "folder.reparent",
        &reparent(&["L0", "L1"], None),
    );
    assert_eq!(
        reply["result"],
        json!({"applied":true,"historyEntriesAdded":1}),
        "{reply}"
    );
    assert_eq!(layer_of(&owner, "L0")["parentLayerUid"], Value::Null);
    assert_eq!(
        layer_of(&owner, "L2")["parentLayerUid"],
        fid.as_str(),
        "an unnamed member stays"
    );
    owner.undo(owner.content_revision()).unwrap();
    assert_eq!(bytes(&owner), inside, "undo of the move out");
    owner.undo(owner.content_revision()).unwrap();
    assert_eq!(bytes(&owner), before, "undo of the move in");
    let reopened =
        NativeObjectHistory::new("reopened", decode_storage_project(&inside).unwrap()).unwrap();
    assert_eq!(bytes(&reopened), inside, "save and reopen");
}

#[test]
fn a_member_keeps_its_world_pose_across_a_translated_folder() {
    let mut document = base(2);
    document["propertyHolders"].as_array_mut().unwrap().push(
        json!({"target":{"contextId":"scene-root","layerUid":"F"},"properties":[{"property":"position","static":[30,0]}]}),
    );
    document["layers"].as_array_mut().unwrap().push(
        json!({"layerUid":"F","name":"Folder","visible":true,"locked":false,"color":"#ffb020","parentLayerUid":null,
            "isNullLayer":true,"nullPos":[0,0],"isFolderLayer":true,"folderCollapsed":false,"effects":[],
            "timing":{"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing","contextId":"scene-root","keys":[{"frame":0}],"samples":[]}}),
    );
    for context in document["compositionOrder"]["contexts"]
        .as_array_mut()
        .unwrap()
    {
        context["layers"]
            .as_array_mut()
            .unwrap()
            .push(json!({"layerUid":"F","naturalPosition":3,"order":{}}));
    }
    let mut owner = owner_of(&document);
    send(
        &mut owner,
        "in",
        "folder.reparent",
        &reparent(&["L2"], Some("F")),
    );
    // the folder moves its members 30 right: L2 compensates so it does not jump (2 - 30)
    assert_eq!(position_of(&owner, "L2"), json!([-28, 0]));
    send(
        &mut owner,
        "out",
        "folder.reparent",
        &reparent(&["L2"], None),
    );
    assert_eq!(position_of(&owner, "L2"), json!([2, 0]));
}

#[test]
fn a_rotated_folder_gives_its_pivot_so_a_member_keeps_its_world_pose_instead_of_refusing() {
    let mut document = drawn();
    document["layers"].as_array_mut().unwrap().push(
        json!({"layerUid":"F","name":"Folder","visible":true,"locked":false,"color":"#ffb020","parentLayerUid":null,
            "isNullLayer":true,"nullPos":[0,0],"isFolderLayer":true,"folderCollapsed":false,"effects":[],
            "timing":{"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing","contextId":"scene-root","keys":[{"frame":0}],"samples":[]}}),
    );
    document["propertyHolders"].as_array_mut().unwrap().push(
        json!({"target":{"contextId":"scene-root","layerUid":"F"},"properties":[{"property":"rotation","static":[45]}]}),
    );
    for context in document["compositionOrder"]["contexts"]
        .as_array_mut()
        .unwrap()
    {
        context["layers"]
            .as_array_mut()
            .unwrap()
            .push(json!({"layerUid":"F","naturalPosition":3,"order":{}}));
    }
    let mut owner = owner_of(&document);
    let before = bytes(&owner);
    let reply = send(
        &mut owner,
        "rotated",
        "folder.reparent",
        &reparent(&["L2"], Some("F")),
    );
    assert_eq!(
        reply["result"],
        json!({"applied":true,"historyEntriesAdded":1}),
        "{reply}"
    );
    assert_eq!(layer_of(&owner, "L2")["parentLayerUid"], "F");
    // the member undoes the folder's 45 degrees, so its own rotation is -45 (the translation absorbs the rest)
    let holder = value(&owner)["propertyHolders"]
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h["target"]["layerUid"] == "L2" && h["target"].get("strokeId").is_none())
        .unwrap()
        .clone();
    let rotation = holder["properties"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["property"] == "rotation")
        .unwrap()["static"][0]
        .as_f64()
        .unwrap();
    assert!((rotation + 45.).abs() < 1e-9, "{holder}");
    assert_ne!(position_of(&owner, "L2"), json!([2, 0]), "{holder}");
    send(
        &mut owner,
        "out",
        "folder.reparent",
        &reparent(&["L2"], None),
    );
    let back = position_of(&owner, "L2");
    assert!(
        (back[0].as_f64().unwrap() - 2.).abs() < 1e-9 && back[1].as_f64().unwrap().abs() < 1e-9,
        "back out: its own pose, {back}"
    );
    owner.undo(owner.content_revision()).unwrap();
    owner.undo(owner.content_revision()).unwrap();
    assert_eq!(bytes(&owner), before, "both gestures undone");
}

#[test]
fn leaving_a_folder_by_a_drop_also_lands_the_block_at_the_gap_in_the_same_entry() {
    let mut owner = owner_of(&base(2));
    let folder = created(
        &mut owner,
        "f",
        &kind(
            "folder",
            "Folder 4",
            json!({"children":["L0"],"pos":[0, 0]}),
        ),
    );
    let fid = folder["layerUid"].as_str().unwrap().to_owned();
    assert_eq!(layer_of(&owner, "L0")["parentLayerUid"], fid.as_str());
    let depths = owner.history_depths();
    let mut payload = reparent(&["L0"], None);
    payload["reorder"] = json!({"gap":3,"selection":selection("L0", &["L0"], None)});
    let reply = send(&mut owner, "drop", "folder.reparent", &payload);
    assert_eq!(
        reply["result"],
        json!({"applied":true,"historyEntriesAdded":1}),
        "{reply}"
    );
    let order: Vec<String> = value(&owner)["layers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["layerUid"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        order,
        ["L1", "L2", "L0", fid.as_str()].map(String::from),
        "L0 lands at gap 3 of the four layers"
    );
    assert_eq!(layer_of(&owner, "L0")["parentLayerUid"], Value::Null);
    assert_eq!(
        owner.history_depths().0,
        depths.0 + 1,
        "one gesture, one entry"
    );
    owner.undo(owner.content_revision()).unwrap();
    assert_eq!(layer_of(&owner, "L0")["parentLayerUid"], fid.as_str());
    assert_eq!(
        value(&owner)["layers"][0]["layerUid"],
        "L0",
        "the order is back too"
    );
}

#[test]
fn folder_membership_refusals_are_typed_and_change_nothing() {
    let mut owner = owner_of(&base(2));
    let folder = created(
        &mut owner,
        "f",
        &kind("folder", "Folder 4", json!({"pos":[0, 0]})),
    );
    let fid = folder["layerUid"].as_str().unwrap().to_owned();
    refused(
        &mut owner,
        "ghost-layer",
        "folder.reparent",
        &reparent(&["nope"], Some(&fid)),
        "not_found",
    );
    refused(
        &mut owner,
        "ghost-folder",
        "folder.reparent",
        &reparent(&["L0"], Some("nope")),
        "not_found",
    );
    refused(
        &mut owner,
        "not-a-folder",
        "folder.reparent",
        &reparent(&["L0"], Some("L1")),
        "invalid_request",
    );
    refused(
        &mut owner,
        "empty",
        "folder.reparent",
        &reparent(&[], Some(&fid)),
        "invalid_request",
    );
    refused(
        &mut owner,
        "dup",
        "folder.reparent",
        &reparent(&["L0", "L0"], Some(&fid)),
        "invalid_request",
    );
    refused(
        &mut owner,
        "empty-folder",
        "folder.reparent",
        &reparent(&["L0"], Some("")),
        "invalid_request",
    );
    let mut into = reparent(&["L0"], Some(&fid));
    into["reorder"] = json!({"gap":0,"selection":selection("L0", &["L0"], None)});
    refused(
        &mut owner,
        "reorder-into",
        "folder.reparent",
        &into,
        "invalid_request",
    );
    // a cycle: the folder sits under L0, so L0 cannot go into it
    send(
        &mut owner,
        "park",
        "parent.set",
        &json!({"command":"object.layer.parent.set","contextId":"scene-root","layerUid":fid,"parentLayerUid":"L0","frame":0}),
    );
    refused(
        &mut owner,
        "cycle",
        "folder.reparent",
        &reparent(&["L0"], Some(&fid)),
        "invalid_request",
    );
    // a keyed position cannot absorb the change: typed unavailability, never a jump
    let mut keyed = base(2);
    keyed["propertyHolders"][0]["properties"] = json!([{"property":"position","track":{"keys":[
        {"frame":0,"v":[0,0]},{"frame":4,"v":[8,0]}]}}]);
    keyed["layers"][0]["parentLayerUid"] = json!("L1");
    keyed["propertyHolders"].as_array_mut().unwrap().push(
        json!({"target":{"contextId":"scene-root","layerUid":"L1"},"properties":[{"property":"position","static":[4,0]}]}),
    );
    let mut owner = owner_of(&keyed);
    let reply = send(
        &mut owner,
        "keyed",
        "kind.create",
        &kind("null", "n", json!({"children":["L0"],"pos":[0, 0]})),
    );
    assert_eq!(reply["error"]["code"], "unavailable", "{reply}");
    assert_eq!(owner.history_depths(), (0, 0));
}
