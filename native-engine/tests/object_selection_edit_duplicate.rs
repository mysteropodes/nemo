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

// The Alt-drag compound clones (ribbon + linked fill, bitmap raster) live beside these in their own file.
#[path = "object_selection_alt_drag.rs"]
mod object_selection_alt_drag;

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

/// The payload of an Alt-drag duplicate: the baseline duplicate request plus where the copies land.
fn dragged(layer: &str, mode: &str, ids: &[&str], offset: Value) -> Value {
    let mut payload = duplicate(layer, 0, mode, ids);
    payload["offset"] = offset;
    payload
}

#[test]
fn an_alt_drag_duplicate_lands_the_copies_at_the_drag_distance_in_one_revision() {
    for version in [1, 2] {
        let mut owner = owner_of(&base(version));
        let (before_value, before) = (value(&owner), bytes(&owner));
        assert_eq!(
            apply(
                &mut owner,
                "duplicate",
                &dragged("L0", "animation2d", &["a", "c"], json!([30, -7.5]))
            ),
            Ok(true)
        );
        let after = value(&owner);
        assert_eq!(
            strokes_at(&after, "L0", 0),
            ["a", "b", "c", "native-stroke:1:0", "native-stroke:1:1"]
        );
        assert_eq!(
            corners(&record_at(&after, "L0", 0, "native-stroke:1:0")),
            [30.0, -7.5, 40.0, -2.5],
            "a at x=0 moves by (30, -7.5), not by the 12 px nudge"
        );
        assert_eq!(
            corners(&record_at(&after, "L0", 0, "native-stroke:1:1")),
            [70.0, -7.5, 80.0, -2.5]
        );
        for original in ["a", "b", "c"] {
            assert_eq!(
                record_at(&after, "L0", 0, original),
                record_at(&before_value, "L0", 0, original),
                "{original} is left untouched"
            );
        }
        assert_eq!(
            (owner.content_revision(), owner.history_depths()),
            (1, (1, 0)),
            "one revision, one history entry"
        );
        let applied = bytes(&owner);
        owner.undo(1).unwrap();
        assert_eq!(bytes(&owner), before, "v{version}: one undo removes the copies");
        owner.redo(2).unwrap();
        assert_eq!(bytes(&owner), applied, "v{version}: redo restores them exactly");
    }
}

/// A dynamic rectangle that also carries a gradient fill (the Alt-drag baseline runs the ordinary move on the clone, which moves the
/// `paramShape.box` frame and both gradient end points with the segments).
fn dynamic_gradient_rect() -> Value {
    let mut record = shape("L0", "r", 0, 0);
    record["family"] = json!("editable-vector-path");
    record["vector"] = json!({"stroke":null,"hasRealStroke":false,"strokeWidth":1,"strokeCap":"round",
        "strokeJoin":"round","opacity":1});
    record["vectorExt"] = json!({
        "paramShape":{"kind":"rect","tl":2,"box":{"x1":0,"y1":0,"x2":10,"y2":5}},
        "fillGradient":{"kind":"linear","from":[0,0],"to":[10,5],
            "stops":[{"offset":0,"color":"#ff0000"},{"offset":1,"color":"#0000ff"}]}});
    record
}

#[test]
fn an_alt_drag_duplicate_moves_the_dynamic_shape_frame_and_the_gradient_ends_with_the_copy() {
    let source = document(1, vec![timed("L0", "Hero", &[0], &[])], vec![dynamic_gradient_rect()]);
    let mut owner = owner_of(&source);
    let before = value(&owner);
    apply(&mut owner, "duplicate", &dragged("L0", "animation2d", &["r"], json!([30, -7.5]))).unwrap();
    let after = value(&owner);
    let copy = record_at(&after, "L0", 0, "native-stroke:1:0");
    assert_eq!(corners(&copy), [30.0, -7.5, 40.0, -2.5]);
    assert_eq!(copy["vectorExt"]["paramShape"]["box"], json!({"x1":30.0,"y1":-7.5,"x2":40.0,"y2":-2.5}));
    assert_eq!(copy["vectorExt"]["paramShape"]["tl"], json!(2), "the radius is not a position");
    assert_eq!(copy["vectorExt"]["fillGradient"]["from"], json!([30.0, -7.5]));
    assert_eq!(copy["vectorExt"]["fillGradient"]["to"], json!([40.0, -2.5]));
    assert_eq!(
        copy["vectorExt"]["fillGradient"]["stops"],
        record_at(&before, "L0", 0, "r")["vectorExt"]["fillGradient"]["stops"]
    );
    assert_eq!(record_at(&after, "L0", 0, "r"), record_at(&before, "L0", 0, "r"), "the original is untouched");
    owner.undo(1).unwrap();
    assert_eq!(value(&owner), before, "one undo removes the copy");
}

#[test]
fn the_cmd_d_nudge_still_leaves_the_frame_and_the_gradient_as_authored() {
    let source = document(1, vec![timed("L0", "Hero", &[0], &[])], vec![dynamic_gradient_rect()]);
    let mut owner = owner_of(&source);
    apply(&mut owner, "duplicate", &duplicate("L0", 0, "animation2d", &["r"])).unwrap();
    let copy = record_at(&value(&owner), "L0", 0, "native-stroke:1:0");
    assert_eq!(corners(&copy), [12.0, 12.0, 22.0, 17.0]);
    assert_eq!(copy["vectorExt"]["paramShape"]["box"], json!({"x1":0,"y1":0,"x2":10,"y2":5}));
    assert_eq!(copy["vectorExt"]["fillGradient"]["from"], json!([0, 0]));
}

/// A duplicate whose selection also holds shapes of other layers: `others` is [{layerUid, strokeIds}].
fn spanning(layer: &str, frame: u32, mode: &str, ids: &[&str], others: Value) -> Value {
    let mut payload = duplicate(layer, frame, mode, ids);
    payload["others"] = others;
    payload
}

#[test]
fn a_selection_that_spans_layers_is_cloned_into_the_destination_layer_in_one_entry() {
    for version in [1, 2] {
        let mut owner = owner_of(&base(version));
        let (before_value, before) = (value(&owner), bytes(&owner));
        // L0 is the destination (the active layer); `x` lives on L1.
        let payload = spanning("L0", 0, "animation2d", &["a"], json!([{"layerUid":"L1","strokeIds":["x"]}]));
        assert_eq!(apply(&mut owner, "duplicate", &payload), Ok(true));
        let after = value(&owner);
        assert_eq!(strokes_at(&after, "L0", 0), ["a", "b", "c", "native-stroke:1:0", "native-stroke:1:1"], "own copy first, then the other layer's");
        assert_eq!(strokes_at(&after, "L1", 0), ["x"], "the other layer is only read");
        assert_eq!(corners(&record_at(&after, "L0", 0, "native-stroke:1:0")), [12.0, 12.0, 22.0, 17.0]);
        assert_eq!(corners(&record_at(&after, "L0", 0, "native-stroke:1:1")), [92.0, 12.0, 102.0, 17.0], "x at 80 + the 12 px nudge");
        assert_eq!(record_at(&after, "L1", 0, "x"), record_at(&before_value, "L1", 0, "x"));
        assert_eq!((owner.content_revision(), owner.history_depths()), (1, (1, 0)), "one revision, one history entry");
        if version == 2 {
            assert_eq!(elements_at(&after, "L0", 0).last(), Some(&pair("native-stroke:1:1", 4)), "the copies join the top of the destination cell");
        }
        let applied = bytes(&owner);
        owner.undo(1).unwrap();
        assert_eq!(bytes(&owner), before, "v{version}: one undo removes every copy");
        owner.redo(2).unwrap();
        assert_eq!(bytes(&owner), applied, "v{version}: redo restores them exactly");
    }
}

#[test]
fn the_destination_may_hold_none_of_the_selection_and_a_drag_distance_applies_to_every_copy() {
    let mut owner = owner_of(&base(2));
    let payload = {
        let mut p = spanning("L0", 0, "animation2d", &[], json!([{"layerUid":"L1","strokeIds":["x"]}]));
        p["offset"] = json!([30, -7.5]);
        p
    };
    assert_eq!(apply(&mut owner, "duplicate", &payload), Ok(true));
    let after = value(&owner);
    assert_eq!(strokes_at(&after, "L0", 0), ["a", "b", "c", "native-stroke:1:0"]);
    assert_eq!(corners(&record_at(&after, "L0", 0, "native-stroke:1:0")), [110.0, -7.5, 120.0, -2.5]);
}

#[test]
fn a_source_layer_is_read_at_the_cell_its_playhead_shows_and_the_destination_held_cell_is_promoted() {
    // L1 shows its key 0 at frame 4 (held); L0 holds key 3 at frame 4: Animation 2D keys the destination, Motion edits the key it inherits.
    let layers = vec![timed("L0", "Hero", &[3], &[]), timed("L1", "Side", &[0], &[])];
    let objects = vec![shape("L0", "d", 3, 60), shape("L1", "x", 0, 80)];
    let others = json!([{"layerUid":"L1","strokeIds":["x"]}]);
    let mut animation = owner_of(&document(2, layers.clone(), objects.clone()));
    assert_eq!(apply(&mut animation, "duplicate", &spanning("L0", 4, "animation2d", &[], others.clone())), Ok(true));
    let after = value(&animation);
    assert_eq!(after["layers"][0]["timing"]["keys"], json!([{"frame":3},{"frame":4}]), "the held destination cell became a key");
    assert_eq!(strokes_at(&after, "L0", 4), ["d", "native-stroke:1:0"], "the key holds a copy of what it showed, then the clone of x");
    let mut motion = owner_of(&document(2, layers, objects));
    assert_eq!(apply(&mut motion, "duplicate", &spanning("L0", 4, "motion", &[], others)), Ok(true));
    let after = value(&motion);
    assert_eq!(after["layers"][0]["timing"]["keys"], json!([{"frame":3}]), "Motion edits the key it inherits");
    assert_eq!(strokes_at(&after, "L0", 3), ["d", "native-stroke:1:0"]);
}

#[test]
fn a_spanning_duplicate_refuses_a_malformed_or_unreachable_selection_and_writes_nothing() {
    let owner = owner_of(&base(2));
    let before = bytes(&owner);
    let bad = [
        ("an empty list", spanning("L0", 0, "animation2d", &["a"], json!([]))),
        ("the destination again", spanning("L0", 0, "animation2d", &["a"], json!([{"layerUid":"L0","strokeIds":["b"]}]))),
        ("a layer twice", spanning("L0", 0, "animation2d", &["a"], json!([{"layerUid":"L1","strokeIds":["x"]},{"layerUid":"L1","strokeIds":["x"]}]))),
        ("no shapes", spanning("L0", 0, "animation2d", &["a"], json!([{"layerUid":"L1","strokeIds":[]}]))),
        ("a stray member", spanning("L0", 0, "animation2d", &["a"], json!([{"layerUid":"L1","strokeIds":["x"],"extra":1}]))),
        ("a repeated shape", spanning("L0", 0, "animation2d", &["a"], json!([{"layerUid":"L1","strokeIds":["x","x"]}]))),
        ("not a list", spanning("L0", 0, "animation2d", &["a"], json!({"layerUid":"L1","strokeIds":["x"]}))),
    ];
    for (name, payload) in bad {
        let refused = prepare(&owner, "duplicate", &payload);
        assert!(refused.is_err(), "{name}");
    }
    let unknown_layer = spanning("L0", 0, "animation2d", &["a"], json!([{"layerUid":"nope","strokeIds":["x"]}]));
    let unknown_stroke = spanning("L0", 0, "animation2d", &["a"], json!([{"layerUid":"L1","strokeIds":["nope"]}]));
    for payload in [unknown_layer, unknown_stroke] {
        assert!(prepare(&owner, "duplicate", &payload).is_err());
    }
    assert_eq!(bytes(&owner), before);
    // A locked destination refuses it whole, as the single-layer duplicate does.
    let mut locked = base(2);
    locked["layers"][0]["locked"] = json!(true);
    let owner = owner_of(&locked);
    assert!(prepare(&owner, "duplicate", &spanning("L0", 0, "animation2d", &["a"], json!([{"layerUid":"L1","strokeIds":["x"]}]))).is_err());
    // A locked source layer is only read.
    let mut source_locked = base(2);
    source_locked["layers"][1]["locked"] = json!(true);
    let mut owner = owner_of(&source_locked);
    assert_eq!(apply(&mut owner, "duplicate", &spanning("L0", 0, "animation2d", &["a"], json!([{"layerUid":"L1","strokeIds":["x"]}]))), Ok(true));
}

#[test]
fn an_explicit_zero_offset_copies_in_place_and_the_default_stays_the_baseline_nudge() {
    let mut owner = owner_of(&base(1));
    apply(
        &mut owner,
        "duplicate",
        &dragged("L0", "animation2d", &["a"], json!([0, 0])),
    )
    .unwrap();
    assert_eq!(
        corners(&record_at(&value(&owner), "L0", 0, "native-stroke:1:0")),
        [0.0, 0.0, 10.0, 5.0]
    );
}

#[test]
fn a_malformed_or_runaway_duplicate_offset_is_refused_before_anything_is_planned() {
    let owner = owner_of(&base(1));
    for offset in [json!([1]), json!([1, 2, 3]), json!(["1", 2]), json!(12), json!([1.0e7, 0]), json!([0, -1.0e7])] {
        let refused = prepare(&owner, "duplicate", &dragged("L0", "animation2d", &["a"], offset.clone()));
        assert_eq!(
            refused.err(),
            Some(crate::request_receipts::DispatchErrorCode::InvalidRequest),
            "{offset}"
        );
    }
    // Only a duplicate carries a distance: a delete with one is an unknown member.
    let mut delete_with_offset = delete("L0", 0, "animation2d", &["a"]);
    delete_with_offset["offset"] = json!([1, 1]);
    assert!(prepare(&owner, "delete", &delete_with_offset).is_err());
}
