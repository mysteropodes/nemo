//! Lane B post-merge review: delete, duplicate and reorder follow the baseline (`deleteLayer`, `duplicateLayer`, `reorderLayersAtGap` of
//! `src/js/timeline.js` / `app.js` at 1065e9d8), which never gated a lock or a transform parent; the folder chevron saves its state
//! without an undo step. Through the actual owner, history and dispatcher on the L02-F three-layer document with evaluable strokes.
use super::object_layer_kind_create::{created, drawn, kind, layer_of, send as send_kind};
use crate::history::object_history::object_layer_lifecycle_tests::{
    bytes, delete, duplicate, owner_of, reorder, selection, send, value,
};
use crate::history::object_history::NativeObjectHistory;
use crate::object_codec::decode_storage_project;
use serde_json::{json, Value};

fn names(owner: &NativeObjectHistory) -> Vec<String> {
    value(owner)["layers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["layerUid"].as_str().unwrap().to_string())
        .collect()
}
fn reopened(owner: &NativeObjectHistory) -> Vec<u8> {
    let saved = bytes(owner);
    let again =
        NativeObjectHistory::new("reopened", decode_storage_project(&saved).unwrap()).unwrap();
    assert_eq!(bytes(&again), saved, "save and reopen");
    saved
}

#[test]
fn deleting_a_null_with_children_releases_them_and_undo_restores_the_whole_state() {
    let mut owner = owner_of(&drawn());
    let null = created(
        &mut owner,
        "n",
        &kind("null", "Null 4", json!({"children":["L0","L2"]})),
    );
    let nid = null["layerUid"].as_str().unwrap().to_string();
    assert_eq!(layer_of(&owner, "L0")["parentLayerUid"], nid.as_str());
    let (before, depths) = (bytes(&owner), owner.history_depths());
    let reply = send(&mut owner, "d", "delete", &delete(&nid, &[]));
    assert_eq!(
        reply["result"],
        json!({"applied":true,"historyEntriesAdded":1}),
        "{reply}"
    );
    assert_eq!(names(&owner), ["L0", "L1", "L2"]);
    for child in ["L0", "L2"] {
        assert_eq!(
            layer_of(&owner, child)["parentLayerUid"],
            Value::Null,
            "{child} is released"
        );
    }
    assert_eq!(owner.history_depths(), (depths.0 + 1, 0));
    let kept = reopened(&owner);
    let at = owner.content_revision();
    owner.undo(at).unwrap();
    assert_eq!(bytes(&owner), before, "undo");
    assert_eq!(layer_of(&owner, "L0")["parentLayerUid"], nid.as_str());
    let at = owner.content_revision();
    owner.redo(at).unwrap();
    assert_eq!(bytes(&owner), kept, "redo");
}

#[test]
fn a_folder_with_children_and_a_parented_layer_delete_duplicate_and_reorder() {
    let mut owner = owner_of(&drawn());
    let folder = created(
        &mut owner,
        "f",
        &kind("folder", "Folder 4", json!({"children":["L1","L2"]})),
    );
    let fid = folder["layerUid"].as_str().unwrap().to_string();
    // a folder child (a parented layer) duplicates and reorders
    let reply = send(&mut owner, "dup", "duplicate", &duplicate("L1"));
    assert_eq!(reply["result"]["applied"], true, "{reply}");
    assert_eq!(reply["result"]["historyEntriesAdded"], 1, "{reply}");
    assert_eq!(names(&owner).len(), 5);
    let reply = send(
        &mut owner,
        "mv",
        "reorder",
        &reorder(&["L2"], 0.0, selection("L2", &["L2"], None)),
    );
    assert_eq!(reply["result"]["applied"], true, "{reply}");
    assert_eq!(names(&owner)[0], "L2");
    assert_eq!(
        layer_of(&owner, "L2")["parentLayerUid"],
        fid.as_str(),
        "reordering keeps the folder link"
    );
    // a parented layer is deleted
    let reply = send(&mut owner, "del-child", "delete", &delete("L2", &["L2"]));
    assert_eq!(reply["result"]["applied"], true, "{reply}");
    assert!(!names(&owner).contains(&"L2".to_string()));
    // the folder with its remaining children is deleted: they are released, never left pointing at it
    let reply = send(&mut owner, "del-folder", "delete", &delete(&fid, &[&fid]));
    assert_eq!(reply["result"]["applied"], true, "{reply}");
    assert!(!names(&owner).contains(&fid));
    assert_eq!(layer_of(&owner, "L1")["parentLayerUid"], Value::Null);
    reopened(&owner);
    assert_eq!(owner.history_depths(), (5, 0));
}

#[test]
fn a_locked_layer_deletes_duplicates_and_reorders_like_any_other() {
    let mut document = drawn();
    document["layers"][1]["locked"] = json!(true);
    let mut owner = owner_of(&document);
    let reply = send(&mut owner, "dup", "duplicate", &duplicate("L1"));
    assert_eq!(reply["result"]["applied"], true, "{reply}");
    let reply = send(
        &mut owner,
        "mv",
        "reorder",
        &reorder(&["L1"], 0.0, selection("L1", &["L1"], None)),
    );
    assert_eq!(reply["result"]["applied"], true, "{reply}");
    assert_eq!(names(&owner)[0], "L1");
    let reply = send(&mut owner, "del", "delete", &delete("L1", &["L1"]));
    assert_eq!(reply["result"]["applied"], true, "{reply}");
    assert!(!names(&owner).contains(&"L1".to_string()));
    assert_eq!(owner.history_depths(), (3, 0));
    reopened(&owner);
}

#[test]
fn the_folder_chevron_is_saved_with_a_revision_but_adds_no_undo_entry_and_keeps_redo() {
    let mut owner = owner_of(&drawn());
    let folder = created(
        &mut owner,
        "f",
        &kind("folder", "Folder 4", json!({"children":["L1"]})),
    );
    let fid = folder["layerUid"].as_str().unwrap().to_string();
    send_kind(
        &mut owner,
        "ren",
        "rename",
        &json!({"command":"object.layer.rename","layerUid":"L0","name":"Ink"}),
    );
    let at = owner.content_revision();
    owner.undo(at).unwrap();
    let depths = owner.history_depths();
    assert_eq!(depths.1, 1, "one step can be redone");
    let revision = owner.content_revision();
    let fold = |owner: &mut NativeObjectHistory, id: &str, collapsed: bool| {
        send_kind(
            owner,
            id,
            "compositing.edit",
            &json!({"command":"object.layer.compositing.edit","layerUid":fid,"edit":"folderCollapsed","collapsed":collapsed}),
        )
    };
    let reply = fold(&mut owner, "fold", true);
    assert_eq!(
        reply["result"],
        json!({"applied":true,"historyEntriesAdded":0}),
        "{reply}"
    );
    assert_eq!(layer_of(&owner, &fid)["folderCollapsed"], true);
    assert_eq!(
        owner.content_revision(),
        revision + 1,
        "the fold is a saved revision"
    );
    assert_eq!(
        owner.history_depths(),
        depths,
        "no undo entry, redo untouched"
    );
    let saved = reopened(&owner);
    assert_eq!(
        value(&decode_back(&saved))["layers"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
    let reply = fold(&mut owner, "unfold", false);
    assert_eq!(reply["result"]["historyEntriesAdded"], 0, "{reply}");
    assert_eq!(layer_of(&owner, &fid)["folderCollapsed"], false);
    let reply = fold(&mut owner, "again", false);
    assert_eq!(
        reply["result"],
        json!({"applied":false,"historyEntriesAdded":0}),
        "{reply}"
    );
    assert_eq!(owner.history_depths(), depths);
}

fn decode_back(saved: &[u8]) -> NativeObjectHistory {
    NativeObjectHistory::new("back", decode_storage_project(saved).unwrap()).unwrap()
}
