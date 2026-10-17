//! PRIVATE UNRUN: baseline matte-menu rows replayed through the sealed transaction, staged dispatch
//! (receipts, cancel, stale, wrong scope), creation of kind layers, locked layers, snapshot reads and the
//! saved-project round trip. Sibling of `scene_effect_layer_record_edit.rs`, which owns the shared builders.
use super::scene_effect_layer_record_edit::{
    apply, bytes, changed_redo, dispatch, fixture, head, layer_json, owner, payload, prepare,
    request, state, CREATE, EDIT,
};
use super::*;
use crate::object_codec::decode_storage_project;
use serde_json::{json, Value};

const ORACLE: &str = include_str!("fixtures/scene-effect-layer-record/oracle.txt");

fn rows(kind: &str) -> Vec<Value> {
    ORACLE
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .filter(|row| row["kind"] == kind)
        .collect()
}

/// The sealed edit a baseline menu step corresponds to; `None` when the baseline step did nothing
/// (the "+" handler found no free layer and only toasted).
fn edit_of_step(step: &Value) -> Option<(&'static str, Value)> {
    let uid = &step["uid"];
    let (menu, pick) = (step["menu"].as_str()?, step["pick"].as_str().unwrap_or(""));
    Some(match (menu, pick) {
        ("legacy", "clear") | ("config", "remove") => ("matteClear", json!({})),
        ("legacy", "source") | ("config", "source") => ("matteSource", json!({"source": uid})),
        ("config", "mode") => ("matteMode", json!({"mode": step["mode"]})),
        ("add", _) => {
            let added = step["added"].as_str()?;
            ("matteMoreAdd", json!({"source": added}))
        }
        ("extra", "source") => (
            "matteMoreSet",
            json!({"index": step["index"], "source": uid}),
        ),
        ("extra", "mode") => (
            "matteMoreSet",
            json!({"index": step["index"], "mode": step["mode"]}),
        ),
        ("extra", "remove") => ("matteMoreRemove", json!({"index": step["index"]})),
        other => panic!("unmapped baseline menu step {other:?}"),
    })
}

#[test]
fn every_baseline_matte_menu_row_replays_through_the_sealed_transaction() {
    let menus = rows("menu");
    assert_eq!(menus.len(), 13);
    for row in menus {
        let source = json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":8,
            "layers":row["layers"],"objects":[]});
        let mut o = owner("compositing", &source);
        let original = bytes(&o);
        let mut committed = 0;
        for step in row["steps"].as_array().unwrap() {
            match edit_of_step(step) {
                Some((edit, extra)) => {
                    let t = prepare(&o, "a", edit, extra);
                    assert!(t.changed(), "{} {step}", row["id"]);
                    o.commit_layer_compositing(t).unwrap();
                    committed += 1;
                }
                None => assert_eq!(
                    step["added"],
                    Value::Null,
                    "{}: nothing left to add",
                    row["id"]
                ),
            }
        }
        let layer = layer_json(&o, "a");
        for key in ["matteMode", "matteSourceLayerUid", "mattesMore"] {
            assert_eq!(
                layer.get(key),
                row["result"].get(key),
                "{} {key}",
                row["id"]
            );
        }
        assert_eq!(o.history_depths(), (committed, 0), "{}", row["id"]);
        let end = bytes(&o);
        for _ in 0..committed {
            o.undo(o.content_revision()).unwrap();
        }
        assert_eq!(bytes(&o), original, "{} undo", row["id"]);
        for _ in 0..committed {
            o.redo(o.content_revision()).unwrap();
        }
        assert_eq!(bytes(&o), end, "{} redo", row["id"]);
    }
}

#[test]
fn staged_receipt_replay_is_exact_and_noops_add_no_history() {
    let mut o = owner("compositing", &fixture(2));
    let r = request(
        &o,
        "blend-on",
        EDIT,
        payload("b", "blend", json!({"mode":"screen"})),
    );
    let first = dispatch(&mut o, &r);
    assert_eq!(first["ok"], true);
    assert_eq!(first["result"]["applied"], true);
    assert_eq!(first["result"]["historyEntriesAdded"], 1);
    let before = state(&o);
    assert_eq!(
        dispatch(&mut o, &r),
        first,
        "replay returns the retained receipt"
    );
    assert_eq!(state(&o), before, "replay executes once");
    let mut conflict = r.clone();
    conflict.payload["mode"] = json!("multiply");
    assert_eq!(
        dispatch(&mut o, &conflict)["error"]["code"],
        "invalid_request"
    );
    assert_eq!(state(&o), before);
    let same = request(
        &o,
        "blend-same",
        EDIT,
        payload("b", "blend", json!({"mode":"screen"})),
    );
    let noop = dispatch(&mut o, &same);
    assert_eq!(noop["ok"], true);
    assert_eq!(noop["result"]["applied"], false);
    assert_eq!(noop["result"]["historyEntriesAdded"], 0);
    assert_eq!(state(&o), before);
    let later = request(
        &o,
        "matte",
        EDIT,
        payload("b", "matteSource", json!({"source":"c"})),
    );
    assert_eq!(dispatch(&mut o, &later)["ok"], true);
    let after = state(&o);
    assert_eq!(
        dispatch(&mut o, &r),
        first,
        "an older receipt still replays"
    );
    assert_eq!(state(&o), after);
}

#[test]
fn cancelled_stale_wrong_scope_and_malformed_requests_publish_nothing_and_keep_redo() {
    let mut o = owner("compositing", &fixture(2));
    changed_redo(&mut o);
    let before = state(&o);
    let mut cancelled = request(
        &o,
        "cancel",
        EDIT,
        payload("b", "blend", json!({"mode":"screen"})),
    );
    cancelled.cancelled_before_dispatch = true;
    assert_eq!(
        dispatch(&mut o, &cancelled)["error"]["code"],
        "cancelled_before_dispatch"
    );
    let mut stale = request(
        &o,
        "stale",
        EDIT,
        payload("b", "blend", json!({"mode":"screen"})),
    );
    stale.expected_revision = Some(0);
    assert_eq!(dispatch(&mut o, &stale)["error"]["code"], "stale_revision");
    let mut missing = request(
        &o,
        "no-revision",
        EDIT,
        payload("b", "blend", json!({"mode":"screen"})),
    );
    missing.expected_revision = None;
    assert_eq!(
        dispatch(&mut o, &missing)["error"]["code"],
        "invalid_request"
    );
    for (id, code, instance) in [
        ("wrong-i", "wrong_instance", true),
        ("wrong-d", "wrong_document", false),
    ] {
        let mut wrong = request(
            &o,
            id,
            EDIT,
            payload("b", "blend", json!({"mode":"screen"})),
        );
        if instance {
            wrong.instance_id = "foreign".into();
        } else {
            wrong.document_id = "foreign".into();
        }
        assert_eq!(dispatch(&mut o, &wrong)["error"]["code"], code);
    }
    for (id, edit, extra, code) in [
        (
            "bad-mode",
            "blend",
            json!({"mode":"nope"}),
            "invalid_request",
        ),
        ("no-layer", "blend", json!({"mode":"screen"}), "not_found"),
        (
            "self-source",
            "matteSource",
            json!({"source":"b"}),
            "invalid_request",
        ),
    ] {
        let layer = if id == "no-layer" { "ghost" } else { "b" };
        let r = request(&o, id, EDIT, payload(layer, edit, extra));
        assert_eq!(dispatch(&mut o, &r)["error"]["code"], code, "{id}");
    }
    let mut wrong_scope = request(
        &o,
        "scope",
        EDIT,
        payload("b", "blend", json!({"mode":"screen"})),
    );
    wrong_scope.payload = json!([]);
    assert_eq!(
        dispatch(&mut o, &wrong_scope)["error"]["code"],
        "invalid_request"
    );
    assert_eq!(
        state(&o),
        before,
        "no refusal published, recorded or cleared redo"
    );
}

#[test]
fn foreign_and_stale_candidates_cannot_publish() {
    let value = fixture(2);
    let first = owner("one", &value);
    let mut second = owner("two", &value);
    let before = state(&second);
    assert_eq!(
        second.commit_layer_compositing(prepare(&first, "b", "blend", json!({"mode":"screen"}))),
        Err(DispatchErrorCode::WrongInstance)
    );
    let twin = owner("two", &value);
    assert_ne!(twin.document_id(), second.document_id());
    assert_eq!(
        second.commit_layer_compositing(prepare(&twin, "b", "blend", json!({"mode":"screen"}))),
        Err(DispatchErrorCode::WrongDocument)
    );
    assert_eq!(state(&second), before);
    let mut o = owner("compositing", &value);
    let changed = prepare(&o, "b", "blend", json!({"mode":"screen"}));
    let noop = prepare(&o, "b", "matteClear", json!({}));
    apply(&mut o, "c", "blend", json!({"mode":"hue"}));
    o.undo(o.content_revision()).unwrap();
    let after = state(&o);
    assert_eq!(
        o.commit_layer_compositing(changed),
        Err(DispatchErrorCode::StaleRevision)
    );
    assert_eq!(
        o.commit_layer_compositing(noop),
        Err(DispatchErrorCode::StaleRevision)
    );
    assert_eq!(
        state(&o),
        after,
        "tokens fail even though undo restored the same bytes"
    );
}

#[test]
fn kind_layers_are_created_in_one_history_entry_and_match_the_baseline_rows() {
    for version in [1, 2] {
        let source = fixture(version);
        let mut o = owner("compositing", &source);
        let original = bytes(&o);
        for (kind, expected_color) in [("effect", "#7EA0FA"), ("folder", "#ffb020")] {
            let create =
                json!({"command":"object.layer.compositing.create","kind":kind,"name":"New"});
            let r = request(&o, &format!("make-{kind}"), CREATE, create);
            let depth = o.history_depths().0;
            let reply = dispatch(&mut o, &r);
            assert_eq!(reply["ok"], true, "{reply}");
            assert_eq!(reply["result"]["historyEntriesAdded"], 1);
            let uid = reply["result"]["layerUid"].as_str().unwrap().to_string();
            assert!(uid.starts_with("native-layer:"));
            assert_eq!(o.history_depths(), (depth + 1, 0));
            let layer = layer_json(&o, &uid);
            assert_eq!(layer["name"], "New");
            assert_eq!(layer["color"], expected_color);
            let row = rows("create")
                .into_iter()
                .find(|row| row["layer"] == kind)
                .unwrap();
            for (key, value) in row["fields"].as_object().unwrap() {
                if key == "isNullLayer" || key == "nullShape" || key == "color" {
                    continue;
                }
                assert_eq!(&layer[key], value, "{kind} {key}");
            }
            assert!(layer.get("isNullLayer").is_none());
            if version == 2 {
                let doc = serde_json::to_value(head(&o).document()).unwrap();
                let listed = doc["compositionOrder"]["contexts"][0]["layers"]
                    .as_array()
                    .unwrap();
                assert!(listed.iter().any(|entry| entry["layerUid"] == uid.as_str()));
            }
        }
        let end = bytes(&o);
        let (undo_depth, _) = o.history_depths();
        assert_eq!(undo_depth, 2);
        for _ in 0..2 {
            o.undo(o.content_revision()).unwrap();
        }
        assert_eq!(bytes(&o), original);
        for _ in 0..2 {
            o.redo(o.content_revision()).unwrap();
        }
        assert_eq!(bytes(&o), end);
        assert_eq!(decode_storage_project(&end).unwrap(), *head(&o).document());
    }
    let mut o = owner("compositing", &fixture(1));
    let before = state(&o);
    let make = |kind: Value, name: Value| json!({"command":"object.layer.compositing.create","kind":kind,"name":name});
    let refused = [
        make(json!("group"), json!("x")),
        make(json!("folder"), json!("x".repeat(257))),
        make(json!("folder"), json!(7)),
        make(json!(3), json!("x")),
        json!({"command":"object.layer.compositing.create","kind":"folder"}),
        json!({"command":"object.layer.create","kind":"folder","name":"x"}),
        json!({"command":"object.layer.compositing.create","kind":"folder","name":"x","extra":1}),
    ];
    for (n, create) in refused.iter().enumerate() {
        // Each case has its own requestId, so it reaches payload preparation instead of the receipt lookup.
        let r = request(&o, &format!("bad-create-{n}"), CREATE, create.clone());
        assert_eq!(
            dispatch(&mut o, &r)["error"]["code"],
            "invalid_request",
            "{create}"
        );
        assert_eq!(state(&o), before, "{create}");
        assert_eq!(
            o.owner
                .prepare_layer_compositing_create_json(create.to_string().as_bytes())
                .err(),
            Some(DispatchErrorCode::InvalidRequest),
            "{create}"
        );
    }
    // The 256-byte bound is inclusive.
    let name = "n".repeat(256);
    let edge = make(json!("folder"), json!(name));
    let r = request(&o, "edge-create", CREATE, edge);
    let reply = dispatch(&mut o, &r);
    assert_eq!(reply["ok"], true, "{reply}");
    let uid = reply["result"]["layerUid"].as_str().unwrap().to_string();
    assert_eq!(layer_json(&o, &uid)["name"], name.as_str());
    assert_eq!(o.history_depths(), (1, 0));
}

#[test]
fn locked_layers_still_take_compositing_edits_as_in_the_baseline() {
    let mut o = owner("compositing", &fixture(1));
    let lock =
        json!({"command":"object.layer.policy.edit","layerUid":"b","flag":"locked","value":true});
    let t = o
        .prepare_layer_policy_json(lock.to_string().as_bytes())
        .unwrap();
    o.commit_layer_policy(t).unwrap();
    apply(&mut o, "b", "blend", json!({"mode":"darken"}));
    assert_eq!(layer_json(&o, "b")["locked"], true);
    assert_eq!(layer_json(&o, "b")["blendMode"], "darken");
}

#[test]
fn consumers_read_the_record_through_the_actual_snapshot_and_old_pins_do_not_move() {
    let mut o = owner("compositing", &fixture(2));
    let zero = head(&o);
    apply(&mut o, "b", "blend", json!({"mode":"colorBurn"}));
    apply(&mut o, "b", "matteSource", json!({"source":"c"}));
    apply(
        &mut o,
        "b",
        "matteMoreAdd",
        json!({"source":"d","mode":"lumaInverted"}),
    );
    apply(
        &mut o,
        "b",
        "effects",
        json!({"effects":[{"type":"blur","enabled":true,"p1":3}]}),
    );
    apply(&mut o, "b", "effectsFromSet", json!({"source":"a"}));
    let snapshot = head(&o);
    let b = &snapshot.document().layers()[1];
    assert_eq!(b.blend_mode(), Some("colorBurn"));
    assert_eq!(
        (b.matte_mode(), b.matte_source_uid()),
        (Some("alpha"), Some("c"))
    );
    assert_eq!(b.mattes_more()[0].uid, "d");
    assert_eq!(b.mattes_more()[0].mode, "lumaInverted");
    assert_eq!(b.effects().unwrap()[0].param(0), Some(3.0));
    assert_eq!(b.effects_from(), Some("a"));
    assert_eq!(b.compositing_reference_uids(), ["c", "d", "a"]);
    let old = &zero.document().layers()[1];
    assert_eq!(
        (old.blend_mode(), old.matte_mode(), old.effects()),
        (None, None, None)
    );
    assert!(old.mattes_more().is_empty());
}

#[test]
fn compositing_undo_and_redo_are_routed_by_the_existing_shared_history_dispatch() {
    let mut o = owner("compositing", &fixture(2));
    let source = bytes(&o);
    let r = request(
        &o,
        "edit",
        EDIT,
        payload("b", "blend", json!({"mode":"exclusion"})),
    );
    assert_eq!(dispatch(&mut o, &r)["ok"], true);
    let changed = bytes(&o);
    for (id, op, want) in [("undo", "undo", source), ("redo", "redo", changed)] {
        let mut r = request(&o, id, EDIT, json!({}));
        r.operation = format!("command.document.object.{op}");
        r.payload = json!({"command":format!("object.{op}")});
        assert_eq!(dispatch(&mut o, &r)["ok"], true);
        assert_eq!(bytes(&o), want);
    }
}

#[cfg(feature = "project_media_codec")]
#[test]
fn authored_compositing_fields_survive_exact_project_save_reopen_and_old_snapshot_pins() {
    let mut o = NativeObjectHistory::new_blank_project("project", 640, 480, 24.).unwrap();
    let zero = head(&o);
    let base = zero.document().layers()[0].layer_uid().to_owned();
    for kind in ["effect", "folder"] {
        let create = json!({"command":"object.layer.compositing.create","kind":kind,"name":kind});
        let r = request(&o, &format!("make-{kind}"), CREATE, create);
        assert_eq!(dispatch(&mut o, &r)["ok"], true);
    }
    let folder = head(&o).document().layers()[2].layer_uid().to_owned();
    apply(&mut o, &base, "blend", json!({"mode":"softLight"}));
    apply(&mut o, &base, "matteSource", json!({"source":folder}));
    apply(
        &mut o,
        &folder,
        "folderCollapsed",
        json!({"collapsed":true}),
    );
    let current = head(&o);
    let saved = o
        .serialize_project_file(o.content_revision(), current.snapshot_id())
        .unwrap();
    let reopened = NativeObjectHistory::from_project_file("reopened", &saved).unwrap();
    assert_eq!(
        reopened
            .serialize_project_file(0, head(&reopened).snapshot_id())
            .unwrap(),
        saved
    );
    assert_eq!(head(&reopened).document(), current.document());
    assert_eq!(zero.document().layers().len(), 1);
    assert_eq!(zero.document().layers()[0].blend_mode(), None);
    let reopened_head = head(&reopened);
    let reopened_base = &reopened_head.document().layers()[0];
    assert_eq!(reopened_base.blend_mode(), Some("softLight"));
    assert_eq!(reopened_base.matte_source_uid(), Some(folder.as_str()));
    assert!(head(&reopened).document().layers()[2].folder_collapsed());
    for _ in 0..5 {
        o.undo(o.content_revision()).unwrap();
    }
    assert_eq!(head(&o).document(), zero.document());
    for _ in 0..5 {
        o.redo(o.content_revision()).unwrap();
    }
    assert_eq!(
        o.serialize_project_file(o.content_revision(), head(&o).snapshot_id())
            .unwrap(),
        saved
    );
}
