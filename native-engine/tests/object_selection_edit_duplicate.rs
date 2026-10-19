//! L02-H duplicate-selection topologies built through the actual owner and
//! compared with literals hand-derived from `duplicateSelection` /
//! `_materializeClones` / `_remapCombineGroups` (src/js/tools.js @ fbe4b8a3):
//! fresh identities named after the published revision, a 12 px nudge, the
//! originals untouched, combine groups recreated children first.
use super::object_selection_edit_tests::{
    apply, base, bytes, corners, delete, document, duplicate, elements_at, frozen, layer, owner_of,
    prepare, record_at, shape, strokes_at, timed, value,
};
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
fn table(entries: Value, members: Value) -> Value {
    json!({"contextId":"scene-root","entries":entries,"members":members})
}
/// L0 "Hero" holds `a b c e` on frame 0 with the given group table.
fn with_table(version: u32, groups: Value) -> Value {
    let mut layer0 = timed("L0", "Hero", &[0], &[]);
    layer0["groups"] = groups;
    document(
        version,
        vec![layer0, layer("L1", "Side")],
        vec![
            shape("L0", "a", 0, 0),
            shape("L0", "b", 0, 20),
            shape("L0", "c", 0, 40),
            shape("L0", "e", 0, 60),
        ],
    )
}

#[test]
fn duplicates_land_on_top_nudged_twelve_pixels_with_fresh_identities() {
    for version in [1, 2] {
        let mut owner = owner_of(&base(version));
        let before = value(&owner);
        assert_eq!(
            apply(
                &mut owner,
                "duplicate",
                &duplicate("L0", 0, "animation2d", &["a", "c"])
            ),
            Ok(true)
        );
        let after = value(&owner);
        // Revision 1 publishes the edit, so the identities are named after it.
        assert_eq!(
            strokes_at(&after, "L0", 0),
            ["a", "b", "c", "native-stroke:1:0", "native-stroke:1:1"]
        );
        assert_eq!(
            corners(&record_at(&after, "L0", 0, "native-stroke:1:0")),
            [12.0, 12.0, 22.0, 17.0],
            "a at x=0 moves by (12, 12)"
        );
        assert_eq!(
            corners(&record_at(&after, "L0", 0, "native-stroke:1:1")),
            [52.0, 12.0, 62.0, 17.0],
            "c at x=40 moves by (12, 12)"
        );
        for original in ["a", "b", "c"] {
            assert_eq!(
                record_at(&after, "L0", 0, original),
                record_at(&before, "L0", 0, original),
                "{original} keeps its identity and its exact record"
            );
        }
        assert_eq!(
            record_at(&after, "L0", 0, "native-stroke:1:0")["fill"],
            record_at(&before, "L0", 0, "a")["fill"]
        );
        if version == 2 {
            assert_eq!(
                elements_at(&after, "L0", 0),
                [
                    pair("a", 0),
                    pair("b", 1),
                    pair("c", 2),
                    pair("native-stroke:1:0", 3),
                    pair("native-stroke:1:1", 4)
                ],
                "the copies join the top of the stack"
            );
            assert_eq!(elements_at(&after, "L0", 3), [pair("a", 0), pair("d", 1)]);
        }
        assert_eq!(
            (owner.content_revision(), owner.history_depths()),
            (1, (1, 0))
        );
    }
}

#[test]
fn a_second_duplicate_skips_identities_already_in_the_document() {
    let mut source = base(1);
    // The next published revision is 1, so this id is the first one it would mint.
    source["objects"]
        .as_array_mut()
        .unwrap()
        .push(shape("L1", "native-stroke:1:0", 0, 90));
    let mut owner = owner_of(&source);
    apply(
        &mut owner,
        "duplicate",
        &duplicate("L0", 0, "animation2d", &["b"]),
    )
    .unwrap();
    assert_eq!(
        strokes_at(&value(&owner), "L0", 0),
        ["a", "b", "c", "native-stroke:1:1"]
    );
}

#[test]
fn a_combine_group_is_recreated_and_a_group_left_with_one_operand_is_dropped() {
    let g1 = entry("g1", json!({"name":"G","combineMode":"unite"}), &["a", "b"]);
    let g2 = entry("g2", json!({"combineMode":"none"}), &["c", "e"]);
    let groups = table(
        json!([g1.clone(), g2.clone()]),
        tags(&[("a", "g1"), ("b", "g1"), ("c", "g2"), ("e", "g2")]),
    );
    for version in [1, 2] {
        let mut owner = owner_of(&with_table(version, groups.clone()));
        let before = bytes(&owner);
        apply(
            &mut owner,
            "duplicate",
            &duplicate("L0", 0, "animation2d", &["a", "b", "c"]),
        )
        .unwrap();
        // Clones n0,n1,n2 take native-stroke:1:0..2; g1 -> native-group:1:0; g2 takes
        // native-group:1:1 and is dropped (only `c` came along), so n2 is a plain shape.
        assert_eq!(
            value(&owner)["layers"][0]["groups"],
            table(
                json!([
                    g1,
                    g2,
                    entry(
                        "native-group:1:0",
                        json!({"name":"G","combineMode":"unite"}),
                        &["native-stroke:1:0", "native-stroke:1:1"]
                    )
                ]),
                tags(&[
                    ("a", "g1"),
                    ("b", "g1"),
                    ("c", "g2"),
                    ("e", "g2"),
                    ("native-stroke:1:0", "native-group:1:0"),
                    ("native-stroke:1:1", "native-group:1:0")
                ])
            ),
            "v{version}"
        );
        owner.undo(1).unwrap();
        assert_eq!(
            bytes(&owner),
            before,
            "v{version}: one undo removes copies and groups"
        );
    }
}

#[test]
fn a_nested_group_is_recreated_children_first_with_its_parent_link_repointed() {
    let inner = entry(
        "gi",
        json!({"parent":"go","combineMode":"none"}),
        &["a", "b"],
    );
    let outer = entry("go", json!({"combineMode":"none"}), &["gi", "c"]);
    let groups = table(
        json!([inner.clone(), outer.clone()]),
        tags(&[("a", "gi"), ("b", "gi"), ("c", "go")]),
    );
    let mut owner = owner_of(&with_table(2, groups));
    apply(
        &mut owner,
        "duplicate",
        &duplicate("L0", 0, "animation2d", &["a", "b", "c"]),
    )
    .unwrap();
    let candidate_table = value(&owner)["layers"][0]["groups"].clone();
    assert_eq!(
        candidate_table["entries"],
        json!([
            inner,
            outer,
            entry(
                "native-group:1:0",
                json!({"parent":"native-group:1:1","combineMode":"none"}),
                &["native-stroke:1:0", "native-stroke:1:1"]
            ),
            entry(
                "native-group:1:1",
                json!({"combineMode":"none"}),
                &["native-group:1:0", "native-stroke:1:2"]
            )
        ])
    );
    assert_eq!(
        candidate_table["members"].as_array().unwrap().len(),
        6,
        "three originals and three tagged copies"
    );
}

#[test]
fn a_reference_to_a_dropped_group_is_settled_so_the_table_stays_admissible() {
    let inner = entry(
        "gi",
        json!({"parent":"go","combineMode":"none"}),
        &["a", "b"],
    );
    let outer = entry("go", json!({"combineMode":"none"}), &["gi", "c"]);
    let groups = table(
        json!([inner, outer]),
        tags(&[("a", "gi"), ("b", "gi"), ("c", "go")]),
    );
    let mut owner = owner_of(&with_table(2, groups.clone()));
    // gi' keeps one operand and is dropped, go' lists [gi', c'] and so still has two
    // entries in the baseline, with a dangling reference. Native settling prunes it.
    let candidate = prepare(
        &owner,
        "duplicate",
        &duplicate("L0", 0, "animation2d", &["a", "c"]),
    )
    .unwrap();
    assert_eq!(
        candidate.created(),
        ids(&["native-stroke:1:0", "native-stroke:1:1"]).as_slice()
    );
    assert!(candidate.created_groups().is_empty());
    owner.commit_selection_edit(candidate).unwrap();
    let after = value(&owner);
    assert_eq!(
        after["layers"][0]["groups"], groups,
        "no new group survives"
    );
    assert_eq!(
        strokes_at(&after, "L0", 0).len(),
        6,
        "the shapes themselves were copied"
    );
}

#[test]
fn a_hold_is_promoted_in_animation_2d_and_duplicated_into_its_key_in_motion() {
    let mut owner = owner_of(&base(2));
    let before = frozen(&owner);
    assert_eq!(
        apply(
            &mut owner,
            "duplicate",
            &duplicate("L0", 4, "animation2d", &["d"])
        ),
        Ok(true)
    );
    let promoted = value(&owner);
    assert_eq!(
        strokes_at(&promoted, "L0", 4),
        ["a", "d", "native-stroke:1:0"]
    );
    assert_eq!(strokes_at(&promoted, "L0", 3), ["a", "d"]);
    assert_eq!(
        elements_at(&promoted, "L0", 4),
        [pair("a", 0), pair("d", 1), pair("native-stroke:1:0", 2)]
    );
    owner.undo(1).unwrap();
    assert_eq!(frozen(&owner).0, before.0);
    // A fresh owner keeps the original Motion identity/revision oracle.
    let mut owner = owner_of(&base(2));
    let candidate = prepare(&owner, "duplicate", &duplicate("L0", 4, "motion", &["d"])).unwrap();
    assert_eq!(candidate.content_frame(), 3);
    owner.commit_selection_edit(candidate).unwrap();
    let after = value(&owner);
    assert_eq!(strokes_at(&after, "L0", 3), ["a", "d", "native-stroke:1:0"]);
    assert_eq!(
        elements_at(&after, "L0", 3),
        [pair("a", 0), pair("d", 1), pair("native-stroke:1:0", 2)]
    );
    assert_eq!(strokes_at(&after, "L0", 4), Vec::<String>::new());
}

#[test]
fn a_pressure_stroke_moves_its_centerline_and_leaves_the_shape_box_as_authored() {
    let brush = json!({"schemaVersion":1,"family":"editable-vector-path",
        "target":{"contextId":"scene-root","frameScope":{"kind":"authored","frame":0},
            "layerUid":"L0","strokeId":"v"},
        "geometry":{"kind":"cubic-path","closed":true,"coordinateSpace":"document-world",
            "handleSpace":"relative","segments":[
                {"point":{"x":0,"y":0},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}},
                {"point":{"x":40,"y":20},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}}]},
        "fill":{"kind":"solid","r":0.25,"g":0.5,"b":0.75,"a":1},
        "vector":{"stroke":null,"hasRealStroke":false,"strokeWidth":6,"strokeCap":"round",
            "strokeJoin":"round","opacity":1},
        "vectorExt":{"pressure":{"centerSegments":[
            {"point":{"x":0,"y":0},"handleIn":{"x":0,"y":0},"handleOut":{"x":5,"y":5},"width":0.5},
            {"point":{"x":40,"y":20},"handleIn":{"x":-5,"y":-5},"handleOut":{"x":0,"y":0},"width":4}],
            "widthProfile":[{"t":0,"width":0.5},{"t":1,"width":4}],
            "strokeProfile":"bulge","profileBase":0},
            "paramShape":{"kind":"rect","tl":0,"box":{"x1":0,"y1":0,"x2":40,"y2":20}},
            "boxAngle":-15}});
    let source = document(1, vec![timed("L0", "Hero", &[0], &[])], vec![brush]);
    let mut owner = owner_of(&source);
    apply(
        &mut owner,
        "duplicate",
        &duplicate("L0", 0, "motion", &["v"]),
    )
    .unwrap();
    let after = value(&owner);
    let copy = record_at(&after, "L0", 0, "native-stroke:1:0");
    assert_eq!(corners(&copy), [12.0, 12.0, 52.0, 32.0]);
    let center = &copy["vectorExt"]["pressure"]["centerSegments"];
    assert_eq!(center[0]["point"], json!({"x":12.0,"y":12.0}));
    assert_eq!(center[1]["point"], json!({"x":52.0,"y":32.0}));
    assert_eq!(
        center[1]["handleIn"],
        json!({"x":-5,"y":-5}),
        "handles are relative"
    );
    assert_eq!(center[0]["width"], json!(0.5));
    let original = record_at(&after, "L0", 0, "v");
    assert_eq!(
        copy["vectorExt"]["paramShape"],
        original["vectorExt"]["paramShape"]
    );
    assert_eq!(
        copy["vectorExt"]["boxAngle"],
        original["vectorExt"]["boxAngle"]
    );
    assert_eq!(
        copy["vectorExt"]["pressure"]["widthProfile"],
        original["vectorExt"]["pressure"]["widthProfile"]
    );
}

#[test]
fn a_duplicate_survives_deleting_its_source_and_the_selection_is_the_copies() {
    let mut owner = owner_of(&base(2));
    let candidate = prepare(
        &owner,
        "duplicate",
        &duplicate("L0", 0, "motion", &["b", "a"]),
    )
    .unwrap();
    assert_eq!(
        candidate.created(),
        ids(&["native-stroke:1:0", "native-stroke:1:1"]).as_slice(),
        "copies come out in selection order"
    );
    owner.commit_selection_edit(candidate).unwrap();
    apply(&mut owner, "delete", &delete("L0", 0, "motion", &["b"])).unwrap();
    assert_eq!(
        strokes_at(&value(&owner), "L0", 0),
        ["a", "c", "native-stroke:1:0", "native-stroke:1:1"]
    );
}
