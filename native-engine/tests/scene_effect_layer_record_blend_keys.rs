//! #2257 the hold-only blend keys (`blendKeys`, baseline `ld.blendKeys` / `layerBlendModeAt`): the stored record, its
//! admission, the three edits (`blendKeySet`, `blendKeyRemove`, `blendKeysClear`) through the sealed owner with one undo
//! entry each, the saved-project round trip, and the typed refusals. Literal expected members throughout.
use super::scene_effect_layer_record_edit::{
    apply, bytes, dispatch, fixture, head, layer_json, owner, payload, prepare, request, state, EDIT,
};
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::request_receipts::DispatchErrorCode::{InvalidRequest, NotFound, StaleRevision};
use serde_json::{json, Value};

fn key(frame: u32, mode: &str) -> Value {
    json!({"frame": frame, "mode": mode})
}

#[test]
fn a_key_is_put_at_its_frame_in_order_and_replaced_in_place_each_edit_one_undo_entry() {
    let mut o = owner("blend-keys", &fixture(2));
    let original = bytes(&o);
    assert_eq!(o.history_depths(), (0, 0));
    apply(&mut o, "b", "blendKeySet", json!({"index": 4, "mode": "multiply"}));
    assert_eq!(layer_json(&o, "b")["blendKeys"], json!([key(4, "multiply")]));
    apply(&mut o, "b", "blendKeySet", json!({"index": 1, "mode": "screen"}));
    apply(&mut o, "b", "blendKeySet", json!({"index": 6, "mode": "hue"}));
    assert_eq!(
        layer_json(&o, "b")["blendKeys"],
        json!([key(1, "screen"), key(4, "multiply"), key(6, "hue")]),
        "kept in frame order"
    );
    apply(&mut o, "b", "blendKeySet", json!({"index": 4, "mode": "overlay"}));
    assert_eq!(
        layer_json(&o, "b")["blendKeys"],
        json!([key(1, "screen"), key(4, "overlay"), key(6, "hue")]),
        "a key at the same frame is replaced, never doubled"
    );
    assert_eq!(o.history_depths(), (4, 0), "one entry per edit");
    let after = bytes(&o);
    for _ in 0..4 {
        o.undo(o.content_revision()).unwrap();
    }
    assert_eq!(bytes(&o), original, "undo restores the exact bytes");
    assert!(layer_json(&o, "b").get("blendKeys").is_none());
    for _ in 0..4 {
        o.redo(o.content_revision()).unwrap();
    }
    assert_eq!(bytes(&o), after, "redo restores the exact bytes");
}

#[test]
fn setting_the_same_key_again_publishes_nothing() {
    let mut o = owner("blend-keys-noop", &fixture(2));
    apply(&mut o, "b", "blendKeySet", json!({"index": 2, "mode": "darken"}));
    let before = state(&o);
    assert!(!prepare(&o, "b", "blendKeySet", json!({"index": 2, "mode": "darken"})).changed());
    apply(&mut o, "b", "blendKeySet", json!({"index": 2, "mode": "darken"}));
    assert_eq!(state(&o), before, "no revision, no history entry, same bytes");
}

#[test]
fn removing_a_key_keeps_the_rest_and_the_last_one_removes_the_member() {
    let mut o = owner("blend-keys-remove", &fixture(2));
    apply(&mut o, "c", "blendKeySet", json!({"index": 0, "mode": "multiply"}));
    apply(&mut o, "c", "blendKeySet", json!({"index": 5, "mode": "screen"}));
    apply(&mut o, "c", "blendKeyRemove", json!({"index": 0}));
    assert_eq!(layer_json(&o, "c")["blendKeys"], json!([key(5, "screen")]));
    apply(&mut o, "c", "blendKeyRemove", json!({"index": 5}));
    assert!(layer_json(&o, "c").get("blendKeys").is_none(), "an empty list is never stored");
    assert!(layer_json(&o, "c").get("blendMode").is_none(), "removing a key never writes the static blend");
    let raw = payload("c", "blendKeyRemove", json!({"index": 5})).to_string();
    assert_eq!(o.prepare_layer_compositing_json(raw.as_bytes()).unwrap_err(), NotFound, "no key at that frame");
}

#[test]
fn the_stopwatch_off_drops_every_key_and_can_fold_a_mode_into_the_static_blend() {
    let mut o = owner("blend-keys-clear", &fixture(2));
    apply(&mut o, "d", "blend", json!({"mode": "screen"}));
    apply(&mut o, "d", "blendKeySet", json!({"index": 3, "mode": "multiply"}));
    assert_eq!(layer_json(&o, "d")["blendMode"], "screen", "the static blend stays while keys exist");
    let depth = o.history_depths().0;
    apply(&mut o, "d", "blendKeysClear", json!({}));
    assert!(layer_json(&o, "d").get("blendKeys").is_none());
    assert_eq!(layer_json(&o, "d")["blendMode"], "screen", "a plain clear leaves the static blend");
    apply(&mut o, "d", "blendKeySet", json!({"index": 3, "mode": "multiply"}));
    apply(&mut o, "d", "blendKeysClear", json!({"mode": "multiply"}));
    assert_eq!(layer_json(&o, "d")["blendMode"], "multiply", "the only key's mode becomes the static blend in the same write");
    assert!(layer_json(&o, "d").get("blendKeys").is_none());
    apply(&mut o, "d", "blendKeySet", json!({"index": 3, "mode": "hue"}));
    apply(&mut o, "d", "blendKeysClear", json!({"mode": "normal"}));
    assert!(layer_json(&o, "d").get("blendMode").is_none(), "Normal clears the member");
    assert_eq!(o.history_depths().0, depth + 5, "clear, then set + clear, then set + clear: one entry each");
    let cleared = state(&o);
    assert!(!prepare(&o, "d", "blendKeysClear", json!({})).changed(), "nothing to drop");
    apply(&mut o, "d", "blendKeysClear", json!({}));
    assert_eq!(state(&o), cleared);
}

#[test]
fn keys_survive_the_saved_project_round_trip_in_both_storage_versions() {
    for version in [1, 2] {
        let mut o = owner("blend-keys-save", &fixture(version));
        apply(&mut o, "a", "blendKeySet", json!({"index": 7, "mode": "colorBurn"}));
        apply(&mut o, "a", "blendKeySet", json!({"index": 0, "mode": "luminosity"}));
        let saved = bytes(&o);
        let reloaded = decode_storage_project(&saved).unwrap();
        assert_eq!(encode_storage_project(&reloaded).unwrap(), saved, "v{version}");
        let layers = serde_json::to_value(&reloaded).unwrap();
        assert_eq!(layers["layers"][0]["blendKeys"], json!([key(0, "luminosity"), key(7, "colorBurn")]));
    }
}

#[test]
fn admission_refuses_an_empty_unordered_or_unnamed_list_and_an_unknown_member() {
    let with = |keys: Value| {
        let mut source = fixture(2);
        source["layers"][1]["blendKeys"] = keys;
        decode_storage_project(source.to_string().as_bytes()).map(|_| ()).map_err(|e| e.to_string())
    };
    assert!(with(json!([key(0, "multiply"), key(3, "screen")])).is_ok());
    assert!(with(json!([])).is_err(), "an empty list must be omitted");
    assert!(with(json!([key(3, "multiply"), key(3, "screen")])).is_err(), "a frame is keyed once");
    assert!(with(json!([key(5, "multiply"), key(3, "screen")])).is_err(), "frame order");
    assert!(with(json!([key(1, "")])).is_err(), "a key names its mode");
    assert!(with(json!([{"frame": 1, "mode": "screen", "ease": 0}])).is_err(), "no interpolation member");
    assert!(with(json!([{"frame": -1, "mode": "screen"}])).is_err());
    assert!(with(json!([{"frame": 1.5, "mode": "screen"}])).is_err());
}

#[test]
fn typed_refusals_for_a_bad_frame_mode_layer_or_stale_pin() {
    let mut o = owner("blend-keys-refusals", &fixture(2));
    let refused = |o: &crate::history::NativeObjectHistory, layer: &str, edit: &str, extra: Value| {
        o.prepare_layer_compositing_json(payload(layer, edit, extra).to_string().as_bytes()).unwrap_err()
    };
    assert_eq!(refused(&o, "b", "blendKeySet", json!({"index": 8, "mode": "multiply"})), InvalidRequest, "frame 8 is outside the 8-frame project");
    assert_eq!(refused(&o, "b", "blendKeySet", json!({"index": 1, "mode": "luma"})), InvalidRequest, "a matte mode is no blend mode");
    assert_eq!(refused(&o, "b", "blendKeySet", json!({"mode": "multiply"})), InvalidRequest);
    assert_eq!(refused(&o, "b", "blendKeySet", json!({"index": 1})), InvalidRequest);
    assert_eq!(refused(&o, "b", "blendKeyRemove", json!({"index": 1, "mode": "screen"})), InvalidRequest);
    assert_eq!(refused(&o, "b", "blendKeysClear", json!({"index": 1})), InvalidRequest);
    assert_eq!(refused(&o, "nope", "blendKeySet", json!({"index": 1, "mode": "screen"})), NotFound);
    // A candidate prepared before another edit lands is stale and publishes nothing.
    let stale = prepare(&o, "b", "blendKeySet", json!({"index": 2, "mode": "screen"}));
    apply(&mut o, "c", "blend", json!({"mode": "multiply"}));
    let before = state(&o);
    assert_eq!(o.commit_layer_compositing(stale).unwrap_err(), StaleRevision);
    assert_eq!(state(&o), before);
}

#[test]
fn the_staged_dispatcher_receipt_counts_one_history_entry() {
    let mut o = owner("blend-keys-dispatch", &fixture(2));
    let r = request(&o, "bk-1", EDIT, payload("b", "blendKeySet", json!({"index": 3, "mode": "overlay"})));
    let receipt = dispatch(&mut o, &r);
    assert_eq!(receipt["ok"], true);
    assert_eq!(receipt["result"], json!({"applied": true, "historyEntriesAdded": 1}));
    assert_eq!(layer_json(&o, "b")["blendKeys"], json!([key(3, "overlay")]));
    assert_eq!(head(&o).document().layers()[1].blend_mode_at(3), Some("overlay"));
}

#[test]
fn the_held_mode_is_the_last_key_at_or_before_the_frame_and_the_first_before_any() {
    let mut o = owner("blend-keys-at", &fixture(2));
    let layer = |o: &crate::history::NativeObjectHistory| head(o).document().layers()[1].clone();
    assert_eq!(layer(&o).blend_mode_at(0), None, "no keys, no static blend: Normal");
    apply(&mut o, "b", "blend", json!({"mode": "darken"}));
    assert_eq!(layer(&o).blend_mode_at(5), Some("darken"), "no keys: the static blend");
    apply(&mut o, "b", "blendKeySet", json!({"index": 2, "mode": "multiply"}));
    apply(&mut o, "b", "blendKeySet", json!({"index": 5, "mode": "screen"}));
    let expected = [(0, "multiply"), (1, "multiply"), (2, "multiply"), (4, "multiply"), (5, "screen"), (7, "screen"), (90, "screen")];
    for (frame, mode) in expected {
        assert_eq!(layer(&o).blend_mode_at(frame), Some(mode), "frame {frame}: hold, never interpolate; the static blend is ignored");
    }
}
