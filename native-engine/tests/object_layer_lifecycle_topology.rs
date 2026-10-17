//! L02-F: literal topology of each operation's committed document (v2 order
//! plan with two contexts, and the v1 shape without one), plus the selection
//! each candidate reports. Expected values are derived by hand from the base
//! document in `object_layer_lifecycle.rs`.
use super::object_layer_lifecycle_tests::{
    apply, base, delete, duplicate, owner_of, rename, reorder, selection, topology, value,
};
use serde_json::{json, Value};

const NEW: &str = "native-layer:1:0";

fn expected_base() -> Value {
    topology(&base(2))
}

#[test]
fn delete_a_middle_layer_removes_its_content_and_compacts_every_context() {
    let mut owner = owner_of(&base(2));
    let outcome = apply(&mut owner, "delete", &delete("L1", &["L1"]));
    assert_eq!(outcome, Some((1, vec![1], Some(1))));
    let want = json!({"layers":[["L0","A"],["L2","C"]],
        "objects":[["L0","s0",0],["L2","s2",0]],
        "holders":[["L0",null],["L0","s0"],["L2",null]],
        "order":[{"context":"scene-root","layers":[["L0",0,{}],["L2",1,{}]],
            "scopes":[[["L0","s0"],["L2","s2"]],[]]},
            {"context":"unused-symbol","layers":[["L0",1,{}],["L2",0,{}]],"scopes":[]}]});
    assert_eq!(topology(&value(&owner)), want);
    assert_eq!(owner.content_revision(), 1);
    assert_eq!(owner.history_depths(), (1, 0));
}

#[test]
fn delete_the_active_layer_when_the_selection_is_empty_and_drop_its_holders() {
    let mut owner = owner_of(&base(2));
    let outcome = apply(&mut owner, "delete", &delete("L0", &[]));
    assert_eq!(outcome, Some((0, vec![0], Some(0))));
    let want = json!({"layers":[["L1","B"],["L2","C"]],
        "objects":[["L1","s1",0],["L1","s1",1],["L2","s2",0]],
        "holders":[["L2",null]],
        "order":[{"context":"scene-root",
            "layers":[["L1",0,{"static":[9]}],["L2",1,{}]],
            "scopes":[[["L1","s1"],["L2","s2"]],[["L1","s1"]]]},
            {"context":"unused-symbol",
            "layers":[["L1",1,{"static":[9]}],["L2",0,{}]],"scopes":[]}]});
    assert_eq!(topology(&value(&owner)), want);
}

#[test]
fn duplicate_a_layer_with_holders_copies_content_under_the_minted_identity() {
    let mut owner = owner_of(&base(2));
    let outcome = apply(&mut owner, "duplicate", &duplicate("L0"));
    assert_eq!(outcome, Some((3, vec![3], Some(3))));
    let want = json!({"layers":[["L0","A"],["L1","B"],["L2","C"],[NEW,"A copy"]],
        "objects":[["L0","s0",0],["L1","s1",0],["L1","s1",1],["L2","s2",0],[NEW,"s0",0]],
        "holders":[["L0",null],["L0","s0"],["L2",null],[NEW,null],[NEW,"s0"]],
        "order":[{"context":"scene-root",
            "layers":[["L0",0,{}],["L1",1,{"static":[9]}],["L2",2,{}],[NEW,3,{}]],
            "scopes":[[["L0","s0"],["L1","s1"],["L2","s2"],[NEW,"s0"]],[["L1","s1"]]]},
            {"context":"unused-symbol",
            "layers":[["L0",2,{}],["L1",1,{"static":[9]}],["L2",0,{}],[NEW,3,{}]],"scopes":[]}]});
    let after = value(&owner);
    assert_eq!(topology(&after), want);
    // Every authored field of the source layer travels with the copy.
    let (source, copy) = (&after["layers"][0], &after["layers"][3]);
    for key in ["visible", "locked", "color", "parentLayerUid", "timing"] {
        assert_eq!(source[key], copy[key], "{key}");
    }
    // The element holder keeps its controls and values under the new identity.
    assert_eq!(
        after["propertyHolders"][4]["controls"],
        after["propertyHolders"][1]["controls"]
    );
    assert_eq!(
        after["propertyHolders"][4]["properties"],
        after["propertyHolders"][1]["properties"]
    );
    assert_eq!(after["propertyHolders"][4]["target"]["layerUid"], NEW);
}

#[test]
fn duplicate_a_two_frame_layer_copies_both_frames_and_its_authored_order() {
    let mut owner = owner_of(&base(2));
    apply(&mut owner, "duplicate", &duplicate("L1"));
    let want = json!({"layers":[["L0","A"],["L1","B"],["L2","C"],[NEW,"B copy"]],
        "objects":[["L0","s0",0],["L1","s1",0],["L1","s1",1],["L2","s2",0],
            [NEW,"s1",0],[NEW,"s1",1]],
        "holders":[["L0",null],["L0","s0"],["L2",null]],
        "order":[{"context":"scene-root",
            "layers":[["L0",0,{}],["L1",1,{"static":[9]}],["L2",2,{}],[NEW,3,{"static":[9]}]],
            "scopes":[[["L0","s0"],["L1","s1"],["L2","s2"],[NEW,"s1"]],
                [["L1","s1"],[NEW,"s1"]]]},
            {"context":"unused-symbol",
            "layers":[["L0",2,{}],["L1",1,{"static":[9]}],["L2",0,{}],[NEW,3,{"static":[9]}]],
            "scopes":[]}]});
    assert_eq!(topology(&value(&owner)), want);
}

#[test]
fn rename_changes_exactly_one_member_of_one_layer() {
    let mut owner = owner_of(&base(2));
    let before = value(&owner);
    assert_eq!(
        apply(&mut owner, "rename", &rename("L0", "Z \u{65e5}\u{672c}")),
        None
    );
    let mut want = before;
    want["layers"][0]["name"] = json!("Z \u{65e5}\u{672c}");
    assert_eq!(value(&owner), want);
    assert_eq!(owner.history_depths(), (1, 0));
}

#[test]
fn reorder_to_the_bottom_reseats_every_context_and_maps_the_selection() {
    let mut owner = owner_of(&base(2));
    let pick = selection("L0", &["L0", "L2"], Some("L1"));
    let outcome = apply(&mut owner, "reorder", &reorder(&["L2"], 0.0, pick));
    assert_eq!(outcome, Some((1, vec![1, 0], Some(2))));
    let mut want = expected_base();
    want["layers"] = json!([["L2", "C"], ["L0", "A"], ["L1", "B"]]);
    want["order"][0]["layers"] = json!([["L0",1,{}],["L1",2,{"static":[9]}],["L2",0,{}]]);
    want["order"][1]["layers"] = json!([["L0",1,{}],["L1",0,{"static":[9]}],["L2",2,{}]]);
    assert_eq!(topology(&value(&owner)), want);
}

#[test]
fn reorder_into_a_middle_gap_counts_only_the_layers_that_stay() {
    let mut owner = owner_of(&base(2));
    let pick = selection("L1", &[], None);
    let outcome = apply(&mut owner, "reorder", &reorder(&["L0"], 2.0, pick));
    assert_eq!(outcome, Some((0, vec![], None)));
    let mut want = expected_base();
    want["layers"] = json!([["L1", "B"], ["L0", "A"], ["L2", "C"]]);
    want["order"][0]["layers"] = json!([["L0",1,{}],["L1",0,{"static":[9]}],["L2",2,{}]]);
    want["order"][1]["layers"] = json!([["L0",1,{}],["L1",2,{"static":[9]}],["L2",0,{}]]);
    assert_eq!(topology(&value(&owner)), want);
}

#[test]
fn version_one_documents_have_no_order_plan_and_follow_the_same_content_rules() {
    let mut owner = owner_of(&base(1));
    apply(&mut owner, "delete", &delete("L1", &["L1"]));
    let after = topology(&value(&owner));
    assert_eq!(after["layers"], json!([["L0", "A"], ["L2", "C"]]));
    assert_eq!(after["objects"], json!([["L0", "s0", 0], ["L2", "s2", 0]]));
    assert_eq!(after["order"], Value::Null);
    assert!(value(&owner).get("compositionOrder").is_none());
    apply(&mut owner, "duplicate", &duplicate("L0"));
    let copied = topology(&value(&owner));
    assert_eq!(copied["layers"][2], json!(["native-layer:2:0", "A copy"]));
    assert_eq!(copied["holders"][3], json!(["native-layer:2:0", null]));
    assert_eq!(copied["order"], Value::Null);
}
