//! L02-G literal topologies: nested group / ungroup / combine / rename built
//! through the actual owner and compared with tables hand-derived from the
//! baseline rules of `groupSelection`, `ungroupSelection` and `combineSelection`
//! (src/js/group-bridge.js @ fbe4b8a3), read back as plain JSON.
use super::object_group_ops_tests::{
    apply, base, bytes, combine, document, document_frames, entry, exit_group, group, has_table,
    owner_of, prepare, rename, set_mode, table, table_of, tags, ungroup, value,
};
use super::*;
use serde_json::json;

const STROKES: [&str; 6] = ["s1", "s2", "s3", "s4", "s5", "s6"];

#[test]
fn a_three_level_nest_is_built_peeled_and_undone_with_literal_tables() {
    for version in [1, 2] {
        let mut owner = owner_of(&document(version, &STROKES, None));
        let start = bytes(&owner);
        assert!(!has_table(&owner), "no table until a group exists");
        assert_eq!(
            apply(
                &mut owner,
                "group",
                &group(&["s1", "s2", "s3"], "ga", "inner")
            ),
            Ok(true)
        );
        assert_eq!(
            table_of(&owner),
            (
                json!([entry(
                    "ga",
                    json!({"name":"inner","combineMode":"none"}),
                    &["s1", "s2", "s3"]
                )]),
                tags(&[("s1", "ga"), ("s2", "ga"), ("s3", "ga")])
            )
        );
        // The whole group nests as ONE unit beside the loose leaf.
        assert_eq!(
            apply(
                &mut owner,
                "group",
                &group(&["s4", "s3", "s1", "s2"], "gb", "outer")
            ),
            Ok(true)
        );
        assert_eq!(
            table_of(&owner),
            (
                json!([
                    entry(
                        "ga",
                        json!({"name":"inner","combineMode":"none","parent":"gb"}),
                        &["s1", "s2", "s3"]
                    ),
                    entry(
                        "gb",
                        json!({"name":"outer","combineMode":"none"}),
                        &["ga", "s4"]
                    )
                ]),
                tags(&[("s1", "ga"), ("s2", "ga"), ("s3", "ga"), ("s4", "gb")])
            )
        );
        assert_eq!(
            apply(
                &mut owner,
                "group",
                &group(&["s1", "s2", "s3", "s4", "s5"], "gc", "top")
            ),
            Ok(true)
        );
        let three = table_of(&owner);
        assert_eq!(
            three,
            (
                json!([
                    entry(
                        "ga",
                        json!({"name":"inner","combineMode":"none","parent":"gb"}),
                        &["s1", "s2", "s3"]
                    ),
                    entry(
                        "gb",
                        json!({"name":"outer","combineMode":"none","parent":"gc"}),
                        &["ga", "s4"]
                    ),
                    entry(
                        "gc",
                        json!({"name":"top","combineMode":"none"}),
                        &["gb", "s5"]
                    )
                ]),
                tags(&[
                    ("s1", "ga"),
                    ("s2", "ga"),
                    ("s3", "ga"),
                    ("s4", "gb"),
                    ("s5", "gc")
                ])
            )
        );
        let three_bytes = bytes(&owner);
        // One level per ungroup: the outermost group of the selected shape.
        assert_eq!(apply(&mut owner, "ungroup", &ungroup(&["s1"])), Ok(true));
        assert_eq!(
            table_of(&owner),
            (
                json!([
                    entry(
                        "ga",
                        json!({"name":"inner","combineMode":"none","parent":"gb"}),
                        &["s1", "s2", "s3"]
                    ),
                    entry(
                        "gb",
                        json!({"name":"outer","combineMode":"none"}),
                        &["ga", "s4"]
                    )
                ]),
                tags(&[("s1", "ga"), ("s2", "ga"), ("s3", "ga"), ("s4", "gb")])
            )
        );
        assert_eq!(apply(&mut owner, "ungroup", &ungroup(&["s1"])), Ok(true));
        assert_eq!(
            table_of(&owner),
            (
                json!([entry(
                    "ga",
                    json!({"name":"inner","combineMode":"none"}),
                    &["s1", "s2", "s3"]
                )]),
                tags(&[("s1", "ga"), ("s2", "ga"), ("s3", "ga")])
            )
        );
        assert_eq!(apply(&mut owner, "ungroup", &ungroup(&["s2"])), Ok(true));
        // An authored empty table is not an absent one.
        assert!(has_table(&owner));
        assert_eq!(
            value(&owner)["layers"][0]["groups"],
            json!({"contextId":"scene-root","entries":[],"members":[]})
        );
        assert_eq!(owner.history_depths(), (6, 0));
        // Six undos walk back through the exact bytes, down to the absent table.
        for _ in 0..3 {
            owner.undo(owner.content_revision()).unwrap();
        }
        assert_eq!(
            bytes(&owner),
            three_bytes,
            "v{version}: back at the 3-level nest"
        );
        for _ in 0..3 {
            owner.undo(owner.content_revision()).unwrap();
        }
        assert_eq!(bytes(&owner), start, "v{version}: back at the absent table");
        assert!(!has_table(&owner));
        assert_eq!(owner.history_depths(), (0, 6));
    }
}

#[test]
fn a_single_entry_survivor_dissolves_in_the_same_transaction() {
    // Baseline quirk: nesting `inn` out of `mid` leaves `mid` listing only s3.
    let start = table(
        json!([
            entry("mid", json!({"combineMode":"none"}), &["inn", "s3"]),
            entry(
                "inn",
                json!({"combineMode":"none","parent":"mid"}),
                &["s1", "s2"]
            )
        ]),
        tags(&[("s1", "inn"), ("s2", "inn"), ("s3", "mid")]),
    );
    for version in [1, 2] {
        let mut owner = owner_of(&document(version, &STROKES[..5], Some(start.clone())));
        assert_eq!(
            apply(&mut owner, "group", &group(&["s1", "s2", "s4"], "new", "N")),
            Ok(true)
        );
        assert_eq!(
            table_of(&owner),
            (
                json!([
                    entry(
                        "inn",
                        json!({"combineMode":"none","parent":"new"}),
                        &["s1", "s2"]
                    ),
                    entry(
                        "new",
                        json!({"name":"N","combineMode":"none"}),
                        &["inn", "s4"]
                    )
                ]),
                tags(&[("s1", "inn"), ("s2", "inn"), ("s4", "new")])
            )
        );
        assert_eq!(
            owner.history_depths(),
            (1, 0),
            "dissolving `mid` is not a second entry"
        );
        let prepared = prepare(&owner, "group", &group(&["s1", "s2", "s5"], "again", "M")).unwrap();
        assert_eq!(prepared.created(), Some("again"));
        assert_eq!(prepared.removed(), ["new".to_string()]);
    }
}

#[test]
fn combine_updates_the_group_the_selection_is_else_creates_a_flat_one() {
    let mut owner = owner_of(&document(2, &STROKES, None));
    assert_eq!(
        apply(
            &mut owner,
            "combine",
            &combine(&["s3", "s1"], "k1", "unite")
        ),
        Ok(true)
    );
    assert_eq!(
        table_of(&owner),
        (
            json!([entry("k1", json!({"combineMode":"unite"}), &["s1", "s3"])]),
            tags(&[("s1", "k1"), ("s3", "k1")])
        )
    );
    // Exactly the group's membership: only its mode changes, no new group.
    assert_eq!(
        apply(
            &mut owner,
            "combine",
            &combine(&["s1", "s3"], "unused", "subtract")
        ),
        Ok(true)
    );
    assert_eq!(
        table_of(&owner).0,
        json!([entry(
            "k1",
            json!({"combineMode":"subtract"}),
            &["s1", "s3"]
        )])
    );
    // The same mode again is a no-op and the unused id is never validated against the table.
    let revision = owner.content_revision();
    assert_eq!(
        apply(
            &mut owner,
            "combine",
            &combine(&["s3", "s1"], "k1", "subtract")
        ),
        Ok(false)
    );
    assert_eq!(
        (owner.content_revision(), owner.history_depths()),
        (revision, (2, 0))
    );
    // A rename and a mode set of an existing group keep every other member exactly.
    assert_eq!(apply(&mut owner, "rename", &rename("k1", "pair")), Ok(true));
    assert_eq!(apply(&mut owner, "mode", &set_mode("k1", "none")), Ok(true));
    assert_eq!(
        table_of(&owner).0,
        json!([entry(
            "k1",
            json!({"name":"pair","combineMode":"none"}),
            &["s1", "s3"]
        )])
    );
}

#[test]
fn one_stroke_is_one_item_across_frames_and_x1_stays_in_its_own_layer() {
    let frames: [(u32, &[&str]); 2] = [(0, &["s1", "s2", "s3"]), (1, &["s2", "s4"])];
    let mut owner = owner_of(&document_frames(2, &frames, None));
    assert_eq!(
        apply(&mut owner, "group", &group(&["s4", "s2"], "g", "frames")),
        Ok(true)
    );
    assert_eq!(
        table_of(&owner),
        (
            json!([entry(
                "g",
                json!({"name":"frames","combineMode":"none"}),
                &["s2", "s4"]
            )]),
            tags(&[("s2", "g"), ("s4", "g")])
        )
    );
    // Layer isolation: another layer's stroke is not an item of L0.
    let before = bytes(&owner);
    assert_eq!(
        apply(&mut owner, "group", &group(&["s1", "x1"], "h", "no")),
        Err(DispatchErrorCode::NotFound)
    );
    assert_eq!(bytes(&owner), before);
    let reopened = owner_of(&value(&owner));
    assert_eq!(
        table_of(&reopened),
        table_of(&owner),
        "save and reopen keep the table"
    );
}

#[test]
fn exiting_a_group_takes_one_shape_out_and_dissolves_a_group_left_with_one() {
    for version in [1, 2] {
        let mut owner = owner_of(&base(version));
        // g1 = {s1,s2} falls below two: dissolved in the SAME transaction; s2 is loose.
        assert_eq!(apply(&mut owner, "exit", &exit_group("s1")), Ok(true));
        assert_eq!(
            table_of(&owner),
            (
                json!([entry("g2", json!({"combineMode":"none"}), &["s3", "s4"])]),
                tags(&[("s3", "g2"), ("s4", "g2")])
            )
        );
        assert_eq!(owner.history_depths(), (1, 0));
        // s1 is loose now: leaving a group it is not in is a typed refusal.
        let before = bytes(&owner);
        assert_eq!(
            apply(&mut owner, "exit", &exit_group("s1")),
            Err(DispatchErrorCode::InvalidRequest)
        );
        assert_eq!(bytes(&owner), before);
    }
    // A three-member group keeps the other two; the shape loses its tag.
    let three = table(
        json!([entry("g", json!({"name":"trio"}), &["s1", "s2", "s3"])]),
        tags(&[("s1", "g"), ("s2", "g"), ("s3", "g")]),
    );
    let mut owner = owner_of(&document(2, &STROKES, Some(three)));
    assert_eq!(apply(&mut owner, "exit", &exit_group("s2")), Ok(true));
    assert_eq!(
        table_of(&owner),
        (
            json!([entry("g", json!({"name":"trio"}), &["s1", "s3"])]),
            tags(&[("s1", "g"), ("s3", "g")])
        )
    );
}
