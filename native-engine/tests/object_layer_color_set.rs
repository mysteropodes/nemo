//! #2371 U11: the layer colour label (`layer.color.set`) through the actual owner, history and dispatcher, on the L02-F
//! three-layer document (stored labels L0 #5fa875, L1 #b49be6, L2 #f0a585). Expectations are hand-derived literals read back
//! through plain `serde_json::Value` projections. Rename shares the label policy, so its parent-versus-lock split is pinned here.
use super::object_layer_lifecycle_tests::{base, bytes, owner_of, rename, send, value};
use super::*;
use crate::object_codec::decode_storage_project;
use serde_json::{json, Value};

fn paint(layer: &str, color: &str, selected: Option<&[&str]>) -> Value {
    let mut payload = json!({"command":"object.layer.color.set","layerUid":layer,"color":color});
    if let Some(selected) = selected {
        payload["selected"] = json!(selected);
    }
    payload
}
fn colors(owner: &NativeObjectHistory) -> Vec<String> {
    value(owner)["layers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|layer| layer["color"].as_str().unwrap().to_string())
        .collect()
}
fn refused(owner: &mut NativeObjectHistory, id: &str, payload: &Value, code: &str) {
    let (before, depths, revision) = (bytes(owner), owner.history_depths(), owner.content_revision());
    let reply = send(owner, id, "color.set", payload);
    assert_eq!(reply["ok"], false, "{id}: {reply}");
    assert_eq!(reply["error"]["code"], code, "{id}: {reply}");
    assert_eq!(bytes(owner), before, "{id} bytes");
    assert_eq!((owner.history_depths(), owner.content_revision()), (depths, revision), "{id}");
}

#[test]
fn a_colour_label_is_one_exact_undoable_entry_and_survives_save_and_reopen() {
    let mut owner = owner_of(&base(2));
    let before = bytes(&owner);
    let reply = send(&mut owner, "c1", "color.set", &paint("L1", "#112233", None));
    assert_eq!(reply["result"], json!({"applied":true,"historyEntriesAdded":1}), "{reply}");
    assert_eq!(colors(&owner), ["#5fa875", "#112233", "#f0a585"]);
    assert_eq!((owner.content_revision(), owner.history_depths()), (1, (1, 0)));
    let kept = bytes(&owner);
    assert_eq!(owner.undo(1).unwrap(), 2);
    assert_eq!(bytes(&owner), before, "undo");
    assert_eq!(owner.redo(2).unwrap(), 3);
    assert_eq!(bytes(&owner), kept, "redo");
    // the stored label again publishes nothing
    let reply = send(&mut owner, "c2", "color.set", &paint("L1", "#112233", None));
    assert_eq!(reply["result"], json!({"applied":false,"historyEntriesAdded":0}), "{reply}");
    assert_eq!(owner.history_depths(), (1, 0));
    let reopened = NativeObjectHistory::new("reopened", decode_storage_project(&kept).unwrap()).unwrap();
    assert_eq!(bytes(&reopened), kept, "save and reopen");
}

#[test]
fn a_selection_is_painted_together_only_when_the_clicked_layer_is_among_it() {
    let mut owner = owner_of(&base(2));
    // clicked L0 inside a two-layer selection: both change, in one entry
    send(&mut owner, "s1", "color.set", &paint("L0", "#abcdef", Some(&["L0", "L2"])));
    assert_eq!(colors(&owner), ["#abcdef", "#b49be6", "#abcdef"]);
    assert_eq!(owner.history_depths(), (1, 0));
    // clicked L1 outside the selection: only L1
    send(&mut owner, "s2", "color.set", &paint("L1", "#010203", Some(&["L0", "L2"])));
    assert_eq!(colors(&owner), ["#abcdef", "#010203", "#abcdef"]);
    // a one-layer selection is just the clicked layer
    send(&mut owner, "s3", "color.set", &paint("L2", "#0a0b0c", Some(&["L2"])));
    assert_eq!(colors(&owner), ["#abcdef", "#010203", "#0a0b0c"]);
    assert_eq!(owner.history_depths(), (3, 0));
    // undo walks the three gestures back one by one
    let at = owner.content_revision();
    owner.undo(at).unwrap();
    assert_eq!(colors(&owner), ["#abcdef", "#010203", "#abcdef"]);
    let at = owner.content_revision();
    owner.undo(at).unwrap();
    assert_eq!(colors(&owner), ["#abcdef", "#b49be6", "#abcdef"]);
}

#[test]
fn colour_refusals_are_typed_and_change_nothing() {
    let mut owner = owner_of(&base(2));
    refused(&mut owner, "ghost", &paint("nope", "#112233", None), "not_found");
    refused(&mut owner, "ghost-selected", &paint("L0", "#112233", Some(&["L0", "nope"])), "not_found");
    for (id, hex) in [("short", "#123"), ("nohash", "112233"), ("word", "#12345g"), ("alpha", "#11223344"), ("empty", "")] {
        refused(&mut owner, id, &paint("L0", hex, None), "invalid_request");
    }
    refused(&mut owner, "empty-uid", &paint("", "#112233", None), "invalid_request");
    refused(&mut owner, "empty-selected", &paint("L0", "#112233", Some(&["L0", ""])), "invalid_request");
    let mut extra = paint("L0", "#112233", None);
    extra["extra"] = json!(1);
    refused(&mut owner, "extra", &extra, "invalid_request");
    let mut wrong = paint("L0", "#112233", None);
    wrong["command"] = json!("object.layer.rename");
    refused(&mut owner, "command", &wrong, "invalid_request");
    // a locked layer is a typed unavailability, also when it sits in the painted selection
    let mut document = base(2);
    document["layers"][1]["locked"] = json!(true);
    let mut locked = owner_of(&document);
    refused(&mut locked, "locked", &paint("L1", "#112233", None), "unavailable");
    refused(&mut locked, "locked-in-selection", &paint("L0", "#112233", Some(&["L0", "L1"])), "unavailable");
    send(&mut locked, "unlocked-ok", "color.set", &paint("L0", "#112233", None));
    assert_eq!(colors(&locked), ["#112233", "#b49be6", "#f0a585"]);
}

#[test]
fn rename_and_colour_are_label_edits_a_parent_does_not_block_but_a_lock_does() {
    let mut document = base(2);
    document["layers"][2]["parentLayerUid"] = json!("L0");
    document["layers"][1]["locked"] = json!(true);
    let mut owner = owner_of(&document);
    let reply = send(&mut owner, "r1", "rename", &rename("L2", "Child"));
    assert_eq!(reply["result"], json!({"applied":true,"historyEntriesAdded":1}), "{reply}");
    assert_eq!(value(&owner)["layers"][2]["name"], "Child");
    assert_eq!(value(&owner)["layers"][2]["parentLayerUid"], "L0", "the parent link is kept");
    send(&mut owner, "r2", "color.set", &paint("L2", "#445566", None));
    assert_eq!(colors(&owner), ["#5fa875", "#b49be6", "#445566"]);
    let (before, depths) = (bytes(&owner), owner.history_depths());
    let reply = send(&mut owner, "r3", "rename", &rename("L1", "Locked"));
    assert_eq!(reply["error"]["code"], "unavailable", "{reply}");
    assert_eq!((bytes(&owner), owner.history_depths()), (before, depths));
}
