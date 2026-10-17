//! L02-H clipboard topologies built through the actual owner: copy into a
//! detached payload, paste with the 12 px same-place rule and the group remap,
//! cut as one snapshot-sealed pair, and the payload as strict wire data. Literals
//! are hand-derived from `copySelection` / `pasteSelection` / `cutSelection`
//! (src/js/tools.js @ fbe4b8a3).
use super::object_selection_edit_tests::{
    apply, base, bytes, corners, delete, dispatch, document, elements_at, frozen, layer, owner_of,
    record_at, selection, shape, strokes_at, timed, value,
};
use super::*;
use crate::commands::PasteDestination;
use crate::object_document::object_group_kernels::object_clipboard_plan::ClipPayload;
use crate::object_document::object_group_kernels::object_selection_edit_frames::EditMode;
use serde_json::{json, Value};

fn pair(stroke: &str, position: u64) -> (String, u64) {
    (stroke.to_string(), position)
}
pub(super) fn copy(
    owner: &NativeObjectHistory,
    layer: &str,
    frame: u32,
    ids: &[&str],
) -> Result<Option<ClipPayload>, DispatchErrorCode> {
    let payload = selection("object.clipboard.copy", layer, frame, "motion", ids);
    owner.copy_selection_json(payload.to_string().as_bytes())
}
pub(super) fn target(layer: &str, frame: u32, mode: EditMode) -> PasteDestination {
    PasteDestination {
        context_id: "scene-root".into(),
        layer_uid: layer.into(),
        frame,
        mode,
    }
}
pub(super) fn paste(
    owner: &mut NativeObjectHistory,
    clip: &ClipPayload,
    destination: &PasteDestination,
) -> Result<bool, DispatchErrorCode> {
    let candidate = owner.prepare_clipboard_paste(clip, destination)?;
    let changed = candidate.changed();
    owner.commit_selection_edit(candidate)?;
    Ok(changed)
}
fn entry(gid: &str, extra: Value, order: &[&str]) -> Value {
    let mut entry = json!({"gid":gid,"order":order});
    for (key, value) in extra.as_object().unwrap() {
        entry[key] = value.clone();
    }
    entry
}
fn tags(pairs: &[(&str, &str)]) -> Value {
    Value::Array(
        pairs
            .iter()
            .map(|(stroke, gid)| json!({"strokeId":stroke,"groupId":gid}))
            .collect(),
    )
}
/// L0 holds `a b` (group `g1`, named "G", unite) and L1 holds `x y` (its own group `g1`).
pub(super) fn two_tables(version: u32) -> Value {
    let mut source = document(
        version,
        vec![timed("L0", "Hero", &[0], &[]), layer("L1", "Side")],
        vec![
            shape("L0", "a", 0, 0),
            shape("L0", "b", 0, 20),
            shape("L1", "x", 0, 40),
            shape("L1", "y", 0, 60),
        ],
    );
    source["layers"][0]["groups"] = json!({"contextId":"scene-root",
        "entries":[entry("g1", json!({"name":"G","combineMode":"unite"}), &["a", "b"])],
        "members":tags(&[("a","g1"),("b","g1")])});
    source["layers"][1]["groups"] = json!({"contextId":"scene-root",
        "entries":[entry("g1", json!({"combineMode":"none"}), &["x", "y"])],
        "members":tags(&[("x","g1"),("y","g1")])});
    source
}

#[test]
fn a_copy_is_a_detached_self_contained_payload_and_changes_nothing() {
    let owner = owner_of(&base(2));
    let before = frozen(&owner);
    let clip = copy(&owner, "L0", 0, &["c", "a"]).unwrap().unwrap();
    assert_eq!(frozen(&owner), before, "copy is a pure read");
    assert_eq!((clip.layer_uid.as_str(), clip.frame), ("L0", 0));
    let document = value(&owner);
    let wire = serde_json::to_value(&clip).unwrap();
    assert_eq!(
        wire,
        json!({"contextId":"scene-root","layerUid":"L0","frame":0,
            "records":[record_at(&document, "L0", 0, "c"), record_at(&document, "L0", 0, "a")]}),
        "selection order, the exact records, no table member when the layer had none"
    );
    let back: ClipPayload = serde_json::from_value(wire).unwrap();
    assert_eq!(
        back, clip,
        "the payload is strict wire data and round-trips"
    );
    assert_eq!(
        copy(&owner, "L0", 0, &[]),
        Ok(None),
        "an empty selection copies nothing"
    );
    assert_eq!(
        copy(&owner, "L0", 0, &["d"]),
        Err(DispatchErrorCode::NotFound)
    );
    assert_eq!(
        copy(&owner, "nope", 0, &["a"]),
        Err(DispatchErrorCode::NotFound)
    );
    assert_eq!(
        copy(&owner, "L0", 0, &["a", "a"]),
        Err(DispatchErrorCode::InvalidRequest)
    );
    // A hold shows its key: frame 4 holds key 3, so the clip carries the key's record.
    let held = copy(&owner, "L0", 4, &["d"]).unwrap().unwrap();
    assert_eq!((held.frame, held.records[0].target.frame()), (4, 3));
}

#[test]
fn a_paste_over_its_source_is_nudged_and_elsewhere_lands_exactly_in_place() {
    for version in [1, 2] {
        let mut owner = owner_of(&base(version));
        let before = value(&owner);
        let clip = copy(&owner, "L0", 0, &["a", "c"]).unwrap().unwrap();
        // Same layer, same frame: 12 px.
        assert_eq!(
            paste(&mut owner, &clip, &target("L0", 0, EditMode::Motion)),
            Ok(true)
        );
        let nudged = value(&owner);
        assert_eq!(
            strokes_at(&nudged, "L0", 0),
            ["a", "b", "c", "native-stroke:1:0", "native-stroke:1:1"]
        );
        assert_eq!(
            corners(&record_at(&nudged, "L0", 0, "native-stroke:1:0")),
            [12.0, 12.0, 22.0, 17.0]
        );
        // Another layer: exactly in place, on top of what the layer already shows.
        assert_eq!(
            paste(&mut owner, &clip, &target("L1", 0, EditMode::Motion)),
            Ok(true)
        );
        // The same layer on another frame is also "elsewhere".
        assert_eq!(
            paste(&mut owner, &clip, &target("L0", 3, EditMode::Animation2d)),
            Ok(true)
        );
        let after = value(&owner);
        assert_eq!(
            strokes_at(&after, "L1", 0),
            ["x", "native-stroke:2:0", "native-stroke:2:1"]
        );
        assert_eq!(
            corners(&record_at(&after, "L1", 0, "native-stroke:2:0")),
            [0.0, 0.0, 10.0, 5.0]
        );
        assert_eq!(
            corners(&record_at(&after, "L1", 0, "native-stroke:2:1")),
            [40.0, 0.0, 50.0, 5.0]
        );
        assert_eq!(
            strokes_at(&after, "L0", 3),
            ["a", "d", "native-stroke:3:0", "native-stroke:3:1"]
        );
        for original in ["a", "b", "c"] {
            assert_eq!(
                record_at(&after, "L0", 0, original),
                record_at(&before, "L0", 0, original),
                "{original} keeps its identity"
            );
        }
        if version == 2 {
            assert_eq!(
                elements_at(&after, "L1", 0),
                [
                    pair("x", 0),
                    pair("native-stroke:2:0", 1),
                    pair("native-stroke:2:1", 2)
                ]
            );
            assert_eq!(
                elements_at(&after, "L0", 3),
                [
                    pair("a", 0),
                    pair("d", 1),
                    pair("native-stroke:3:0", 2),
                    pair("native-stroke:3:1", 3)
                ]
            );
        }
        assert_eq!(owner.history_depths(), (3, 0));
    }
}

#[test]
fn a_pasted_group_becomes_its_own_group_in_the_destination_table() {
    for version in [1, 2] {
        let mut owner = owner_of(&two_tables(version));
        let clip = copy(&owner, "L0", 0, &["a", "b"]).unwrap().unwrap();
        assert!(
            clip.groups.value().is_some(),
            "the source table travels with the copy"
        );
        // The source layer may change before the paste: the clip needs nothing from it.
        apply(
            &mut owner,
            "delete",
            &delete("L0", 0, "motion", &["a", "b"]),
        )
        .unwrap();
        assert_eq!(
            paste(&mut owner, &clip, &target("L1", 0, EditMode::Motion)),
            Ok(true)
        );
        let after = value(&owner);
        assert_eq!(
            after["layers"][1]["groups"],
            json!({"contextId":"scene-root",
                "entries":[
                    entry("g1", json!({"combineMode":"none"}), &["x", "y"]),
                    entry("native-group:2:0", json!({"name":"G","combineMode":"unite"}),
                        &["native-stroke:2:0", "native-stroke:2:1"])],
                "members":tags(&[("x","g1"),("y","g1"),
                    ("native-stroke:2:0","native-group:2:0"),("native-stroke:2:1","native-group:2:0")])}),
            "v{version}: a fresh gid beside the destination's own `g1`"
        );
        // The deleted source group dissolved in its own transaction.
        assert_eq!(
            after["layers"][0]["groups"],
            json!({"contextId":"scene-root","entries":[],"members":[]})
        );
    }
}

#[test]
fn cut_is_a_clip_plus_a_delete_sealed_to_one_snapshot() {
    let mut owner = owner_of(&base(2));
    let payload = selection("object.clipboard.cut", "L0", 0, "motion", &["a", "b"]);
    let (clip, candidate) = owner
        .prepare_clipboard_cut_json(payload.to_string().as_bytes())
        .unwrap();
    let clip = clip.unwrap();
    assert_eq!(
        candidate.removed(),
        ["a".to_string(), "b".to_string()].as_slice()
    );
    assert_eq!(
        owner.history_depths(),
        (0, 0),
        "preparing publishes nothing"
    );
    // Another edit lands first: the cut candidate is stale, nothing is lost.
    apply(&mut owner, "delete", &delete("L0", 0, "motion", &["c"])).unwrap();
    let kept = frozen(&owner);
    assert_eq!(
        owner.commit_selection_edit(candidate),
        Err(DispatchErrorCode::StaleRevision)
    );
    assert_eq!(frozen(&owner), kept);
    let payload = selection("object.clipboard.cut", "L0", 0, "motion", &["a", "b"]);
    let (clip_again, candidate) = owner
        .prepare_clipboard_cut_json(payload.to_string().as_bytes())
        .unwrap();
    assert_eq!(clip_again.unwrap().records.len(), 2);
    owner.commit_selection_edit(candidate).unwrap();
    let after = value(&owner);
    assert_eq!(strokes_at(&after, "L0", 0), Vec::<String>::new());
    assert_eq!(elements_at(&after, "L0", 3), [pair("a", 0), pair("d", 1)]);
    assert_eq!(owner.history_depths(), (2, 0));
    // The clip from the first (discarded) cut still pastes: it is self-contained.
    assert_eq!(
        paste(&mut owner, &clip, &target("L1", 0, EditMode::Motion)),
        Ok(true)
    );
    assert_eq!(
        strokes_at(&value(&owner), "L1", 0),
        ["x", "native-stroke:3:0", "native-stroke:3:1"]
    );
}

#[test]
fn cutting_nothing_and_pasting_nothing_publish_nothing() {
    let mut owner = owner_of(&base(2));
    let payload = selection("object.clipboard.cut", "L0", 0, "motion", &[]);
    let (clip, candidate) = owner
        .prepare_clipboard_cut_json(payload.to_string().as_bytes())
        .unwrap();
    assert!(clip.is_none() && !candidate.changed());
    apply(&mut owner, "delete", &delete("L0", 0, "motion", &["c"])).unwrap();
    owner.undo(1).unwrap();
    let before = frozen(&owner);
    assert_eq!(
        owner.commit_selection_edit(candidate),
        Err(DispatchErrorCode::StaleRevision)
    );
    let (_, noop) = owner
        .prepare_clipboard_cut_json(payload.to_string().as_bytes())
        .unwrap();
    assert_eq!(owner.commit_selection_edit(noop), Ok(before.2));
    let empty = ClipPayload {
        context_id: "scene-root".into(),
        layer_uid: "L0".into(),
        frame: 0,
        records: Vec::new(),
        groups: crate::object_document::Presence::Missing,
    };
    assert_eq!(
        paste(&mut owner, &empty, &target("L1", 0, EditMode::Motion)),
        Ok(false)
    );
    assert_eq!(
        frozen(&owner),
        before,
        "bytes, both stacks and the revision are untouched"
    );
}

#[test]
fn a_small_clip_pastes_over_the_staged_wire_and_replays_once() {
    let mut owner = owner_of(&base(2));
    let clip = copy(&owner, "L0", 0, &["b"]).unwrap().unwrap();
    let payload = json!({"command":"object.clipboard.paste","contextId":"scene-root","layerUid":"L1",
        "frame":0,"mode":"motion","clip":serde_json::to_value(&clip).unwrap()});
    let req = super::object_selection_edit_tests::request(&owner, "paste-1", "paste", &payload);
    let first = dispatch(&mut owner, &req);
    assert_eq!(
        first["result"],
        json!({"applied":true,"historyEntriesAdded":1})
    );
    let after = bytes(&owner);
    assert_eq!(dispatch(&mut owner, &req), first);
    assert_eq!((owner.content_revision(), bytes(&owner)), (1, after));
    assert_eq!(
        strokes_at(&value(&owner), "L1", 0),
        ["x", "native-stroke:1:0"]
    );
}
