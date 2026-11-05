//! #2371 U11 / D36: Null, Effect, Guide, Folder and Effector creation (`layer.kind.create`) and folder membership
//! (`layer.folder.reparent`) through the actual owner, history and dispatcher, on the L02-F three-layer v2 document
//! (L0 position 0,0; L2 position 2,0; `drawn()` gives L0 and L2 evaluable strokes). Expectations are hand-derived
//! literals read back through plain `serde_json::Value` projections.
use crate::history::object_history::object_layer_lifecycle_tests::{
    base, bytes, owner_of, request, selection, value,
};
use crate::history::object_history::NativeObjectHistory;
use crate::object_codec::decode_storage_project;
use serde_json::{json, Value};

fn send(owner: &mut NativeObjectHistory, id: &str, op: &str, payload: &Value) -> Value {
    let request = request(owner, id, op, payload);
    serde_json::to_value(
        owner
            .try_dispatch_object_command(&serde_json::from_value(request).unwrap())
            .unwrap()
            .unwrap(),
    )
    .unwrap()
}
/// The three layers with evaluable editable-path strokes: L0 spans x 0..10, y 0..10 and L2 spans x 30..40, y 20..30.
fn drawn() -> Value {
    let rect = |layer: &str, stroke: &str, frame: u32, x: f64, y: f64| {
        let corner = |x: f64, y: f64| json!({"point":{"x":x,"y":y},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}});
        json!({"schemaVersion":1,"family":"editable-vector-path",
            "target":{"contextId":"scene-root","frameScope":{"kind":"authored","frame":frame},"layerUid":layer,"strokeId":stroke},
            "geometry":{"kind":"cubic-path","closed":true,"coordinateSpace":"document-world","handleSpace":"relative",
                "segments":[corner(x, y), corner(x + 10.0, y), corner(x + 10.0, y + 10.0), corner(x, y + 10.0)]},
            "fill":{"kind":"solid","r":0.25,"g":0.5,"b":0.75,"a":1},
            "vector":{"stroke":null,"hasRealStroke":false,"strokeWidth":3,"strokeCap":"round","strokeJoin":"round","opacity":1}})
    };
    let mut document = base(2);
    document["objects"] = json!([
        rect("L0", "s0", 0, 0.0, 0.0),
        rect("L1", "s1", 0, 0.0, 0.0),
        rect("L1", "s1", 1, 0.0, 0.0),
        rect("L2", "s2", 0, 30.0, 20.0)
    ]);
    document["propertyHolders"]
        .as_array_mut()
        .unwrap()
        .remove(1);
    document
}
fn kind(kind: &str, name: &str, extra: Value) -> Value {
    let mut payload =
        json!({"command":"object.layer.kind.create","kind":kind,"name":name,"frame":0});
    for (key, member) in extra.as_object().unwrap() {
        payload[key] = member.clone();
    }
    payload
}
fn created(owner: &mut NativeObjectHistory, id: &str, payload: &Value) -> Value {
    let reply = send(owner, id, "kind.create", payload);
    assert_eq!(reply["ok"], true, "{id}: {reply}");
    assert_eq!(reply["result"]["applied"], true, "{id}: {reply}");
    assert_eq!(reply["result"]["historyEntriesAdded"], 1, "{id}: {reply}");
    value(owner)["layers"]
        .as_array()
        .unwrap()
        .last()
        .unwrap()
        .clone()
}
fn refused(owner: &mut NativeObjectHistory, id: &str, op: &str, payload: &Value, code: &str) {
    let (before, depths, revision) = (
        bytes(owner),
        owner.history_depths(),
        owner.content_revision(),
    );
    let reply = send(owner, id, op, payload);
    assert_eq!(reply["ok"], false, "{id}: {reply}");
    assert_eq!(reply["error"]["code"], code, "{id}: {reply}");
    assert_eq!(bytes(owner), before, "{id} bytes");
    assert_eq!(
        (owner.history_depths(), owner.content_revision()),
        (depths, revision),
        "{id}"
    );
}
fn with_duplicator() -> Value {
    let mut document = base(2);
    document["componentTables"] = json!({"duplicators":{"format":"nemo.native-duplicator-set","formatVersion":1,
        "effectorLayers":[],"duplicators":[{"contextId":"scene-root","layerUid":"L0","duplicator":{"mode":"grid"}}]}});
    document
}
fn layer_of(owner: &NativeObjectHistory, uid: &str) -> Value {
    value(owner)["layers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["layerUid"] == uid)
        .unwrap()
        .clone()
}
fn position_of(owner: &NativeObjectHistory, uid: &str) -> Value {
    value(owner)["propertyHolders"]
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h["target"]["layerUid"] == uid && h["target"].get("strokeId").is_none())
        .and_then(|h| {
            h["properties"]
                .as_array()
                .unwrap()
                .iter()
                .find(|p| p["property"] == "position")
        })
        .map_or(Value::Null, |p| p["static"].clone())
}

#[test]
fn a_null_is_one_undoable_entry_with_its_anchor_and_survives_save_and_reopen() {
    let mut owner = owner_of(&base(2));
    let before = bytes(&owner);
    let layer = created(
        &mut owner,
        "n1",
        &kind("null", "Null 4", json!({"pos":[160.5,90]})),
    );
    assert_eq!(layer["name"], "Null 4");
    assert_eq!(layer["isNullLayer"], true);
    assert_eq!(layer["nullPos"], json!([160.5, 90.0]));
    assert_eq!(layer["color"], "#ff2d78");
    assert_eq!(layer["parentLayerUid"], Value::Null);
    assert_eq!(
        (owner.content_revision(), owner.history_depths()),
        (1, (1, 0))
    );
    // the new layer sits last in the stack of every order context
    for context in value(&owner)["compositionOrder"]["contexts"]
        .as_array()
        .unwrap()
    {
        assert_eq!(context["layers"].as_array().unwrap().len(), 4, "{context}");
    }
    let kept = bytes(&owner);
    assert_eq!(owner.undo(1).unwrap(), 2);
    assert_eq!(bytes(&owner), before, "undo");
    assert_eq!(owner.redo(2).unwrap(), 3);
    assert_eq!(bytes(&owner), kept, "redo");
    let reopened =
        NativeObjectHistory::new("reopened", decode_storage_project(&kept).unwrap()).unwrap();
    assert_eq!(bytes(&reopened), kept, "save and reopen");
}

#[test]
fn a_null_adopts_the_selection_around_the_centre_of_its_bounds_in_one_entry() {
    let mut owner = owner_of(&drawn());
    let layer = created(
        &mut owner,
        "n2",
        &kind("null", "Null 4", json!({"children":["L0","L2"]})),
    );
    // the union of x 0..10, y 0..10 and x 30..40, y 20..30 is x 0..40, y 0..30: its centre is (20, 15)
    assert_eq!(layer["nullPos"], json!([20.0, 15.0]));
    let uid = layer["layerUid"].as_str().unwrap();
    assert_eq!(layer_of(&owner, "L0")["parentLayerUid"], uid);
    assert_eq!(layer_of(&owner, "L2")["parentLayerUid"], uid);
    assert_eq!(
        layer_of(&owner, "L1")["parentLayerUid"],
        Value::Null,
        "an unselected layer stays free"
    );
    assert_eq!(
        position_of(&owner, "L2"),
        json!([2, 0]),
        "a free layer keeps its own position"
    );
    assert_eq!(owner.history_depths(), (1, 0), "one gesture, one entry");
    owner.undo(1).unwrap();
    assert_eq!(layer_of(&owner, "L0")["parentLayerUid"], Value::Null);
    assert_eq!(value(&owner)["layers"].as_array().unwrap().len(), 3);
}

#[test]
fn an_adopted_layer_keeps_its_world_pose_when_it_leaves_a_translated_parent() {
    let mut document = base(2);
    document["layers"][2]["parentLayerUid"] = json!("L1");
    document["propertyHolders"].as_array_mut().unwrap().push(
        json!({"target":{"contextId":"scene-root","layerUid":"L1"},"properties":[{"property":"position","static":[10,5]}]}),
    );
    let mut owner = owner_of(&document);
    let layer = created(
        &mut owner,
        "n3",
        &kind("folder", "Folder 4", json!({"children":["L2"],"pos":[0,0]})),
    );
    assert_eq!(layer_of(&owner, "L2")["parentLayerUid"], layer["layerUid"]);
    // it was drawn 10,5 further by L1; now it is free of L1 and carries that offset itself: 2+10, 0+5
    assert_eq!(position_of(&owner, "L2"), json!([12, 5]));
    owner.undo(1).unwrap();
    assert_eq!(layer_of(&owner, "L2")["parentLayerUid"], "L1");
    assert_eq!(position_of(&owner, "L2"), json!([2, 0]));
}

#[test]
fn a_folder_effect_and_guide_layer_carry_the_baseline_defaults() {
    let mut owner = owner_of(&base(2));
    let folder = created(
        &mut owner,
        "f1",
        &kind("folder", "Folder 4", json!({"pos":[320,180]})),
    );
    assert_eq!(
        (
            folder["isFolderLayer"].clone(),
            folder["folderCollapsed"].clone(),
            folder["effects"].clone()
        ),
        (json!(true), json!(false), json!([]))
    );
    assert_eq!(
        (
            folder["isNullLayer"].clone(),
            folder["nullPos"].clone(),
            folder["color"].clone()
        ),
        (json!(true), json!([320.0, 180.0]), json!("#ffb020"))
    );
    let effect = created(&mut owner, "e1", &kind("effect", "Effet 5", json!({})));
    assert_eq!(
        (effect["isEffectLayer"].clone(), effect["effects"].clone()),
        (json!(true), json!([]))
    );
    assert!(effect.get("isNullLayer").is_none() && effect.get("isGuideLayer").is_none());
    let guide = created(
        &mut owner,
        "g1",
        &kind("guide", "Guide 6", json!({"pos":[320,180]})),
    );
    assert_eq!(guide["isGuideLayer"], true);
    assert_eq!(guide["guidePos"], json!([320.0, 180.0]));
    assert_eq!(
        (guide["guideOrientation"].clone(), guide["color"].clone()),
        (json!("horizontal"), json!("#00baff"))
    );
    assert_eq!(owner.history_depths(), (3, 0));
    let kept = bytes(&owner);
    let reopened =
        NativeObjectHistory::new("reopened", decode_storage_project(&kept).unwrap()).unwrap();
    assert_eq!(bytes(&reopened), kept, "save and reopen");
}
#[test]
fn an_effector_needs_a_duplicator_host_and_starts_with_the_seeded_offset() {
    let mut owner = owner_of(&with_duplicator());
    refused(
        &mut owner,
        "no-host",
        "kind.create",
        &kind("effector", "Effector 4", json!({"target":"L1"})),
        "invalid_request",
    );
    refused(
        &mut owner,
        "ghost",
        "kind.create",
        &kind("effector", "Effector 4", json!({"target":"nope"})),
        "not_found",
    );
    refused(
        &mut owner,
        "no-target",
        "kind.create",
        &kind("effector", "Effector 4", json!({})),
        "invalid_request",
    );
    let layer = created(
        &mut owner,
        "x1",
        &kind(
            "effector",
            "Effector 4",
            json!({"target":"L0","pos":[100,50]}),
        ),
    );
    assert_eq!(layer["color"], "#59d38a");
    let uid = layer["layerUid"].as_str().unwrap();
    let record = &value(&owner)["componentTables"]["duplicators"]["effectorLayers"][0];
    assert_eq!(record["layerUid"], uid);
    assert_eq!(
        record["effector"],
        json!({"targetLayerUid":"L0","falloff":"radial","pos":[100.0, 50.0]})
    );
    assert_eq!(
        position_of_key(&owner, uid, "effOffPosition"),
        json!([120, 0])
    );
    let kept = bytes(&owner);
    owner.undo(1).unwrap();
    assert_eq!(
        value(&owner)["componentTables"]["duplicators"]["effectorLayers"],
        json!([])
    );
    owner.redo(2).unwrap();
    assert_eq!(bytes(&owner), kept, "redo");
    let reopened =
        NativeObjectHistory::new("reopened", decode_storage_project(&kept).unwrap()).unwrap();
    assert_eq!(bytes(&reopened), kept, "save and reopen");
}
fn position_of_key(owner: &NativeObjectHistory, uid: &str, key: &str) -> Value {
    value(owner)["propertyHolders"]
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h["target"]["layerUid"] == uid)
        .and_then(|h| {
            h["properties"]
                .as_array()
                .unwrap()
                .iter()
                .find(|p| p["property"] == key)
        })
        .map_or(Value::Null, |p| p["static"].clone())
}

#[test]
fn kind_creation_refusals_are_typed_and_change_nothing() {
    let mut owner = owner_of(&with_duplicator());
    refused(
        &mut owner,
        "bad-kind",
        "kind.create",
        &kind("widget", "x", json!({})),
        "invalid_request",
    );
    refused(
        &mut owner,
        "children-on-guide",
        "kind.create",
        &kind("guide", "g", json!({"children":["L0"]})),
        "invalid_request",
    );
    refused(
        &mut owner,
        "target-on-null",
        "kind.create",
        &kind("null", "n", json!({"target":"L0"})),
        "invalid_request",
    );
    refused(
        &mut owner,
        "dup-children",
        "kind.create",
        &kind("null", "n", json!({"children":["L0","L0"]})),
        "invalid_request",
    );
    refused(
        &mut owner,
        "empty-child",
        "kind.create",
        &kind("null", "n", json!({"children":[""]})),
        "invalid_request",
    );
    refused(
        &mut owner,
        "ghost-child",
        "kind.create",
        &kind("null", "n", json!({"children":["nope"]})),
        "not_found",
    );
    refused(
        &mut owner,
        "far",
        "kind.create",
        &kind("null", "n", json!({"pos":[1e12, 0]})),
        "invalid_request",
    );
    refused(
        &mut owner,
        "pos-on-effect",
        "kind.create",
        &kind("effect", "e", json!({"pos":[1, 2]})),
        "invalid_request",
    );
    let mut extra = kind("null", "n", json!({}));
    extra["extra"] = json!(1);
    refused(
        &mut owner,
        "extra",
        "kind.create",
        &extra,
        "invalid_request",
    );
    let mut wrong = kind("null", "n", json!({}));
    wrong["command"] = json!("object.layer.create");
    refused(
        &mut owner,
        "command",
        "kind.create",
        &wrong,
        "invalid_request",
    );
    let mut missing = kind("null", "n", json!({}));
    missing.as_object_mut().unwrap().remove("frame");
    refused(
        &mut owner,
        "no-frame",
        "kind.create",
        &missing,
        "invalid_request",
    );
    // a stale expected revision is the dispatcher's own refusal
    let mut stale = request(
        &owner,
        "stale",
        "kind.create",
        &kind("null", "n", json!({"pos":[0, 0]})),
    );
    stale["expectedRevision"] = json!(99);
    let reply = serde_json::to_value(
        owner
            .try_dispatch_object_command(&serde_json::from_value(stale).unwrap())
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(reply["error"]["code"], "stale_revision", "{reply}");
}

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
fn a_rotated_folder_that_cannot_give_its_pivot_refuses_instead_of_jumping() {
    let mut document = base(2);
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
    refused(
        &mut owner,
        "rotated",
        "folder.reparent",
        &reparent(&["L2"], Some("F")),
        "unavailable",
    );
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
