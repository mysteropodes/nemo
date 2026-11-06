//! #2257 the group edits of the hold-only blend keys: `blendKeysMove` (a drag or an Alt+Arrow nudge of the selected Blend
//! diamonds) and `blendKeysRemove` (Delete over them), each ONE write: one revision and one history entry however many keys
//! the gesture touches. Literal expected members throughout; the fixture project holds 8 frames.
use super::scene_effect_layer_record_edit::{
    apply, bytes, dispatch, fixture, layer_json, owner, payload, prepare, request, state, EDIT,
};
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::request_receipts::DispatchErrorCode::{self, InvalidRequest, NotFound};
use serde_json::{json, Value};

fn key(frame: u32, mode: &str) -> Value {
    json!({"frame": frame, "mode": mode})
}

/// Layer `b` keyed at 1 (multiply), 3 (screen), 4 (hue) and 6 (darken).
fn keyed(name: &str) -> crate::history::NativeObjectHistory {
    let mut o = owner(name, &fixture(2));
    for (frame, mode) in [(1, "multiply"), (3, "screen"), (4, "hue"), (6, "darken")] {
        apply(
            &mut o,
            "b",
            "blendKeySet",
            json!({"index": frame, "mode": mode}),
        );
    }
    o
}

fn refused(o: &crate::history::NativeObjectHistory, edit: &str, extra: Value) -> DispatchErrorCode {
    o.prepare_layer_compositing_json(payload("b", edit, extra).to_string().as_bytes())
        .unwrap_err()
}

#[test]
fn a_group_moves_with_its_modes_in_one_entry_and_undo_redo_restore_the_exact_bytes() {
    let mut o = keyed("blend-group-move");
    let before = bytes(&o);
    let depth = o.history_depths().0;
    // Frames 3 and 4 slide one frame earlier: 3 lands on the place 4 leaves, which is no collision.
    apply(
        &mut o,
        "b",
        "blendKeysMove",
        json!({"frames": [3, 4], "by": -1}),
    );
    assert_eq!(
        layer_json(&o, "b")["blendKeys"],
        json!([
            key(1, "multiply"),
            key(2, "screen"),
            key(3, "hue"),
            key(6, "darken")
        ]),
        "the modes travel with the keys"
    );
    assert_eq!(o.history_depths().0, depth + 1, "one entry for two keys");
    let after = bytes(&o);
    o.undo(o.content_revision()).unwrap();
    assert_eq!(bytes(&o), before, "undo restores the exact bytes");
    o.redo(o.content_revision()).unwrap();
    assert_eq!(bytes(&o), after, "redo restores the exact bytes");
    let reloaded = decode_storage_project(&after).unwrap();
    assert_eq!(
        encode_storage_project(&reloaded).unwrap(),
        after,
        "save and reload"
    );
    // The group can also jump over a key that stays: 4 and 6 go one frame later (to 5 and 7), 3 stays put.
    apply(
        &mut o,
        "b",
        "blendKeysMove",
        json!({"frames": [3, 6], "by": 1}),
    );
    assert_eq!(
        layer_json(&o, "b")["blendKeys"],
        json!([
            key(1, "multiply"),
            key(2, "screen"),
            key(4, "hue"),
            key(7, "darken")
        ])
    );
}

#[test]
fn a_group_lands_whole_or_not_at_all() {
    let o = keyed("blend-group-refused");
    assert_eq!(
        refused(&o, "blendKeysMove", json!({"frames": [3, 4], "by": 2})),
        InvalidRequest,
        "4 would land on the staying key at 6"
    );
    assert_eq!(
        refused(&o, "blendKeysMove", json!({"frames": [4, 6], "by": 2})),
        InvalidRequest,
        "6 would leave the 8-frame project"
    );
    assert_eq!(
        refused(&o, "blendKeysMove", json!({"frames": [1, 3], "by": -2})),
        InvalidRequest,
        "1 would go below frame 0"
    );
    assert_eq!(
        refused(&o, "blendKeysMove", json!({"frames": [2, 4], "by": 1})),
        NotFound,
        "2 is no key"
    );
    assert_eq!(
        refused(&o, "blendKeysRemove", json!({"frames": [1, 2]})),
        NotFound,
        "2 is no key"
    );
}

#[test]
fn deleting_a_selection_is_one_entry_and_the_last_keys_remove_the_member() {
    let mut o = keyed("blend-group-remove");
    let before = bytes(&o);
    let depth = o.history_depths().0;
    apply(&mut o, "b", "blendKeysRemove", json!({"frames": [1, 4]}));
    assert_eq!(
        layer_json(&o, "b")["blendKeys"],
        json!([key(3, "screen"), key(6, "darken")])
    );
    assert_eq!(o.history_depths().0, depth + 1, "one entry for two keys");
    apply(&mut o, "b", "blendKeysRemove", json!({"frames": [3, 6]}));
    assert!(
        layer_json(&o, "b").get("blendKeys").is_none(),
        "an empty list is never stored"
    );
    assert!(
        layer_json(&o, "b").get("blendMode").is_none(),
        "removing keys never writes the static blend"
    );
    assert_eq!(o.history_depths().0, depth + 2);
    o.undo(o.content_revision()).unwrap();
    o.undo(o.content_revision()).unwrap();
    assert_eq!(bytes(&o), before);
}

#[test]
fn malformed_group_requests_are_typed_refusals_and_publish_nothing() {
    let o = keyed("blend-group-malformed");
    let before = state(&o);
    for (edit, extra) in [
        ("blendKeysMove", json!({"frames": [3], "by": 0})),
        ("blendKeysMove", json!({"frames": [], "by": 1})),
        ("blendKeysMove", json!({"frames": [4, 3], "by": 1})),
        ("blendKeysMove", json!({"frames": [3, 3], "by": 1})),
        ("blendKeysMove", json!({"frames": [3]})),
        ("blendKeysMove", json!({"by": 1})),
        ("blendKeysMove", json!({"frames": [3], "by": 1, "index": 3})),
        ("blendKeysMove", json!({"frames": [3], "by": 1.5})),
        ("blendKeysRemove", json!({})),
        ("blendKeysRemove", json!({"frames": []})),
        ("blendKeysRemove", json!({"frames": [4, 3]})),
        ("blendKeysRemove", json!({"frames": [3], "by": 1})),
        ("blendKeysRemove", json!({"frames": [3], "mode": "screen"})),
        ("blendKeysRemove", json!({"frames": [-1]})),
        ("blendKeyRemove", json!({"index": 3, "frames": [3]})),
        ("blend", json!({"mode": "screen", "by": 1})),
    ] {
        assert_eq!(
            refused(&o, edit, extra.clone()),
            InvalidRequest,
            "{edit} {extra}"
        );
    }
    let too_many: Vec<u32> = (0..65).collect();
    assert_eq!(
        refused(&o, "blendKeysRemove", json!({"frames": too_many})),
        InvalidRequest,
        "more than 64 keys in one write"
    );
    assert_eq!(state(&o), before);
}

#[test]
fn a_stale_group_candidate_publishes_nothing_and_the_dispatcher_counts_one_entry() {
    let mut o = keyed("blend-group-dispatch");
    let stale = prepare(&o, "b", "blendKeysRemove", json!({"frames": [1]}));
    apply(&mut o, "c", "blend", json!({"mode": "multiply"}));
    let before = state(&o);
    assert_eq!(
        o.commit_layer_compositing(stale).unwrap_err(),
        crate::request_receipts::DispatchErrorCode::StaleRevision
    );
    assert_eq!(state(&o), before);
    let r = request(
        &o,
        "bkg-1",
        EDIT,
        payload("b", "blendKeysMove", json!({"frames": [3, 4], "by": -1})),
    );
    let receipt = dispatch(&mut o, &r);
    assert_eq!(receipt["ok"], true);
    assert_eq!(
        receipt["result"],
        json!({"applied": true, "historyEntriesAdded": 1})
    );
    let r = request(
        &o,
        "bkg-2",
        EDIT,
        payload("b", "blendKeysRemove", json!({"frames": [2, 3]})),
    );
    let receipt = dispatch(&mut o, &r);
    assert_eq!(
        receipt["result"],
        json!({"applied": true, "historyEntriesAdded": 1})
    );
    assert_eq!(
        layer_json(&o, "b")["blendKeys"],
        json!([key(1, "multiply"), key(6, "darken")])
    );
}
