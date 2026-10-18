//! L02-H delete-selection topologies built through the actual owner and
//! compared with literals hand-derived from `SM.deleteSelStrokes`
//! (src/js/timeline.js @ fbe4b8a3) plus the native admission rules: records,
//! order elements, holders and group tables leave together in one revision.
use super::object_selection_edit_tests::{
    apply, base, bytes, delete, document, elements_at, frozen, layer, owner_of, prepare, shape,
    strokes_at, timed, value,
};
use super::*;
use serde_json::{json, Value};

fn ids(values: &[&str]) -> Vec<String> {
    values.iter().map(|id| id.to_string()).collect()
}
fn pair(stroke: &str, position: u64) -> (String, u64) {
    (stroke.to_string(), position)
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
/// `base` with a group table on L0 and a holder on `b` and on `a`.
fn grouped(version: u32, entries: Value, members: Value) -> Value {
    let mut document = base(version);
    document["layers"][0]["groups"] =
        json!({"contextId":"scene-root","entries":entries,"members":members});
    document["propertyHolders"] = json!([
        {"target":{"contextId":"scene-root","layerUid":"L0","strokeId":"a"},
            "properties":[{"property":"opacity","static":[50]}]},
        {"target":{"contextId":"scene-root","layerUid":"L0","strokeId":"b"},
            "properties":[{"property":"opacity","static":[60]}]},
        {"target":{"contextId":"scene-root","layerUid":"L0"},
            "properties":[{"property":"opacity","static":[70]}]}
    ]);
    document
}
fn holder_strokes(document: &Value) -> Vec<String> {
    document["propertyHolders"]
        .as_array()
        .unwrap()
        .iter()
        .map(|holder| {
            holder["target"]["strokeId"]
                .as_str()
                .unwrap_or("<layer>")
                .to_string()
        })
        .collect()
}

#[test]
fn deleting_the_middle_shape_repacks_the_order_elements_and_undo_restores_every_byte() {
    for version in [1, 2] {
        let mut owner = owner_of(&base(version));
        let before = bytes(&owner);
        assert_eq!(
            apply(
                &mut owner,
                "delete",
                &delete("L0", 0, "animation2d", &["b"])
            ),
            Ok(true)
        );
        let after = value(&owner);
        assert_eq!(strokes_at(&after, "L0", 0), ["a", "c"]);
        assert_eq!(strokes_at(&after, "L0", 3), ["a", "d"], "other frames stay");
        assert_eq!(strokes_at(&after, "L1", 0), ["x"], "other layers stay");
        if version == 2 {
            assert_eq!(
                elements_at(&after, "L0", 0),
                [pair("a", 0), pair("c", 1)],
                "c moved down to the gapless slot 1"
            );
            assert_eq!(elements_at(&after, "L0", 3), [pair("a", 0), pair("d", 1)]);
            assert_eq!(elements_at(&after, "L1", 0), [pair("x", 0)]);
        }
        assert_eq!(
            (owner.content_revision(), owner.history_depths()),
            (1, (1, 0))
        );
        assert_eq!(owner.undo(1).unwrap(), 2);
        assert_eq!(bytes(&owner), before, "v{version} undo");
    }
}

#[test]
fn deleting_several_shapes_in_any_order_keeps_the_survivors_relative_order() {
    let mut owner = owner_of(&base(2));
    apply(
        &mut owner,
        "delete",
        &delete("L0", 0, "animation2d", &["c", "a"]),
    )
    .unwrap();
    let after = value(&owner);
    assert_eq!(strokes_at(&after, "L0", 0), ["b"]);
    assert_eq!(elements_at(&after, "L0", 0), [pair("b", 0)]);
    // `a` survives on frame 3, so its element there is untouched.
    assert_eq!(elements_at(&after, "L0", 3), [pair("a", 0), pair("d", 1)]);
}

#[test]
fn the_last_record_of_a_shape_takes_its_holder_and_a_survivor_keeps_it() {
    let table = (json!([]), json!([]));
    for version in [1, 2] {
        let mut owner = owner_of(&grouped(version, table.0.clone(), table.1.clone()));
        // `a` also lives on frame 3: its holder stays. `b` was only on frame 0: gone.
        apply(
            &mut owner,
            "delete",
            &delete("L0", 0, "animation2d", &["a", "b"]),
        )
        .unwrap();
        assert_eq!(
            holder_strokes(&value(&owner)),
            ["a", "<layer>"],
            "v{version}: the layer-level holder and the surviving element stay"
        );
        // Deleting `a` from its last frame drops that holder too.
        apply(
            &mut owner,
            "delete",
            &delete("L0", 3, "animation2d", &["a"]),
        )
        .unwrap();
        assert_eq!(holder_strokes(&value(&owner)), ["<layer>"], "v{version}");
        assert_eq!(owner.history_depths(), (2, 0));
    }
}

#[test]
fn a_shape_that_survives_in_another_frame_keeps_its_group_membership() {
    let g1 = entry("g1", json!({"name":"G","combineMode":"unite"}), &["a", "b"]);
    let mut owner = owner_of(&grouped(2, json!([g1]), tags(&[("a", "g1"), ("b", "g1")])));
    apply(
        &mut owner,
        "delete",
        &delete("L0", 0, "animation2d", &["a"]),
    )
    .unwrap();
    let after = value(&owner);
    assert_eq!(
        after["layers"][0]["groups"],
        json!({"contextId":"scene-root",
            "entries":[entry("g1", json!({"name":"G","combineMode":"unite"}), &["a", "b"])],
            "members":tags(&[("a","g1"),("b","g1")])}),
        "`a` still exists on frame 3"
    );
}

#[test]
fn the_last_member_leaving_dissolves_a_group_left_with_one_entry_in_the_same_revision() {
    for version in [1, 2] {
        let g1 = entry("g1", json!({"name":"G","combineMode":"unite"}), &["a", "b"]);
        let mut owner = owner_of(&grouped(
            version,
            json!([g1]),
            tags(&[("a", "g1"), ("b", "g1")]),
        ));
        let before = bytes(&owner);
        apply(
            &mut owner,
            "delete",
            &delete("L0", 0, "animation2d", &["b"]),
        )
        .unwrap();
        let after = value(&owner);
        assert_eq!(
            after["layers"][0]["groups"],
            json!({"contextId":"scene-root","entries":[],"members":[]}),
            "v{version}: an authored empty table, not an absent one"
        );
        assert_eq!(
            (owner.content_revision(), owner.history_depths()),
            (1, (1, 0))
        );
        owner.undo(1).unwrap();
        assert_eq!(bytes(&owner), before);
    }
}

#[test]
fn a_nested_group_promotes_its_child_when_the_outer_one_falls_below_two() {
    let inner = entry(
        "gi",
        json!({"parent":"go","combineMode":"none"}),
        &["a", "b"],
    );
    let outer = entry("go", json!({"combineMode":"none"}), &["gi", "c"]);
    let mut owner = owner_of(&grouped(
        2,
        json!([inner, outer]),
        tags(&[("a", "gi"), ("b", "gi"), ("c", "go")]),
    ));
    apply(
        &mut owner,
        "delete",
        &delete("L0", 0, "animation2d", &["c"]),
    )
    .unwrap();
    assert_eq!(
        value(&owner)["layers"][0]["groups"],
        json!({"contextId":"scene-root",
            "entries":[entry("gi", json!({"combineMode":"none"}), &["a", "b"])],
            "members":tags(&[("a","gi"),("b","gi")])}),
        "the inner group lost its parent link with the outer group"
    );
}

#[test]
fn a_hold_is_promoted_in_animation_2d_and_edited_in_its_key_in_motion() {
    // Frame 5 holds key 3; frame 1 holds key 0.
    let mut owner = owner_of(&base(2));
    let before = frozen(&owner);
    assert_eq!(
        apply(
            &mut owner,
            "delete",
            &delete("L0", 5, "animation2d", &["d"])
        ),
        Ok(true)
    );
    let promoted = value(&owner);
    assert_eq!(strokes_at(&promoted, "L0", 5), ["a"]);
    assert_eq!(strokes_at(&promoted, "L0", 3), ["a", "d"]);
    assert_eq!(owner.history_depths(), (1, 0));
    owner.undo(1).unwrap();
    assert_eq!(frozen(&owner).0, before.0);
    assert_eq!(
        apply(&mut owner, "delete", &delete("L0", 5, "motion", &["d"])),
        Ok(true)
    );
    let after = value(&owner);
    assert_eq!(
        strokes_at(&after, "L0", 3),
        ["a"],
        "written into key 3, once"
    );
    assert_eq!(strokes_at(&after, "L0", 5), Vec::<String>::new());
    assert_eq!(owner.history_depths(), (1, 0));
    assert_eq!(
        apply(&mut owner, "delete", &delete("L0", 1, "motion", &["b"])),
        Ok(true)
    );
    assert_eq!(strokes_at(&value(&owner), "L0", 0), ["a", "c"]);
}

#[test]
fn a_motion_hold_before_every_key_stores_nothing_and_says_so() {
    let source = document(
        1,
        vec![timed("L0", "Hero", &[2], &[]), layer("L1", "Side")],
        vec![shape("L0", "a", 2, 0), shape("L0", "b", 2, 20)],
    );
    let mut owner = owner_of(&source);
    let before = frozen(&owner);
    assert_eq!(
        apply(&mut owner, "delete", &delete("L0", 0, "motion", &["a"])),
        Err(DispatchErrorCode::InvalidRequest),
        "the baseline dropped the deletion silently"
    );
    assert_eq!(
        apply(
            &mut owner,
            "delete",
            &delete("L0", 0, "animation2d", &["a"])
        ),
        Err(DispatchErrorCode::Unavailable)
    );
    assert_eq!(frozen(&owner), before);
}

#[test]
fn an_in_between_frame_is_edited_in_place_in_both_modes() {
    let source = document(
        2,
        vec![timed("L0", "Hero", &[0, 4], &[2]), layer("L1", "Side")],
        vec![
            shape("L0", "a", 0, 0),
            shape("L0", "p", 2, 5),
            shape("L0", "q", 2, 25),
            shape("L0", "a", 4, 0),
        ],
    );
    for mode in ["animation2d", "motion"] {
        let mut owner = owner_of(&source);
        assert_eq!(
            apply(&mut owner, "delete", &delete("L0", 2, mode, &["p"])),
            Ok(true),
            "{mode}"
        );
        let after = value(&owner);
        assert_eq!(strokes_at(&after, "L0", 2), ["q"], "{mode}");
        assert_eq!(elements_at(&after, "L0", 2), [pair("q", 0)], "{mode}");
        assert_eq!(strokes_at(&after, "L0", 0), ["a"]);
    }
}

#[test]
fn the_committed_candidate_reports_what_it_removed() {
    let owner = owner_of(&grouped(
        2,
        json!([entry("g1", json!({}), &["a", "b"])]),
        tags(&[("a", "g1"), ("b", "g1")]),
    ));
    let candidate = prepare(&owner, "delete", &delete("L0", 0, "motion", &["b", "c"])).unwrap();
    assert_eq!(candidate.removed(), ids(&["b", "c"]).as_slice());
    assert_eq!(candidate.dissolved_groups(), ids(&["g1"]).as_slice());
    assert_eq!(candidate.content_frame(), 0);
    assert!(!candidate.promoted());
    assert!(candidate.created().is_empty());
}
