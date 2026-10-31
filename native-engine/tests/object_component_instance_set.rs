//! Tests of the Component Instance panel edit (#2256 Q5). The expected values are the baseline writers of `timeline.js` at
//! 1065e9d8: `setSymbolPlayMode` stores the label, `setSymbolSingleFrame` stores the pick and, on an outer keyframe, the same
//! pick as that key's `componentFrame` (deleting `blankOverride`), `setSymbolSpeed` and `setSymbolPlacedAt` store their numbers.
use super::fixture::{decode, document, layer, wire};
use super::*;
use serde_json::{json, Value};

fn instance(extra: Value) -> Value {
    let mut value = json!({"contextId":"scene-root","layerUid":"ly_inst","symbolId":"sym_x",
        "symPlayMode":"once","symSpeed":1,"symPlacedAt":0,"symSingleFrame":0,"symTrimIn":2,"symTrimOut":20,
        "symMatrix":[1,0,0,1,0,0]});
    for (key, member) in extra.as_object().expect("object") {
        value[key] = member.clone();
    }
    value
}

fn world(instance: Value) -> ObjectDocument {
    let timing = json!({"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing",
        "contextId":"scene-root","keys":[{"frame":0},{"frame":6}],"samples":[]});
    let layers = [
        layer("ly_inst", "Inst", &[]).with(json!({"timing":timing})),
        layer("ly_plain", "Plain", &[(0, "x1")]),
    ];
    let components = json!({"format":"nemo.native-component-set","formatVersion":1,
        "components":[{"symbolId":"sym_x","totalFrames":24,"layers":[{"layerUid":"xi"}],"objects":[]}],
        "instances":[instance]});
    decode(&document(
        &layers,
        json!({"componentTables":{"components":components}}),
    ))
}

fn stored(document: &ObjectDocument) -> Value {
    wire(document)["componentTables"]["components"]["instances"][0].clone()
}

fn edit_with(f: impl FnOnce(&mut InstanceEdit)) -> InstanceEdit {
    let mut edit = InstanceEdit::default();
    f(&mut edit);
    edit
}

fn apply_edit(document: &ObjectDocument, edit: &InstanceEdit) -> SetupPlan {
    plan_instance_set(document, "ly_inst", edit).expect("edit is admissible")
}

#[test]
fn each_member_is_applied_and_every_other_member_keeps_its_stored_bytes() {
    let cases: Vec<(InstanceEdit, &str, Value)> = vec![
        (
            edit_with(|e| e.mode = Some("pingpong".into())),
            "symPlayMode",
            json!("pingpong"),
        ),
        (
            edit_with(|e| e.single_frame = Some(9)),
            "symSingleFrame",
            json!(9),
        ),
        (
            edit_with(|e| e.placed_at = Some(-3)),
            "symPlacedAt",
            json!(-3),
        ),
        (edit_with(|e| e.speed = Some(2.5)), "symSpeed", json!(2.5)),
        (edit_with(|e| e.speed = Some(3.0)), "symSpeed", json!(3)),
    ];
    let before = world(instance(json!({})));
    for (edit, key, expected) in cases {
        let plan = apply_edit(&before, &edit);
        assert!(plan.changed, "{key}");
        assert_eq!(plan.symbol_id.as_deref(), Some("sym_x"));
        assert_eq!(plan.layer_uid.as_deref(), Some("ly_inst"));
        let after = stored(&plan.document);
        assert_eq!(after[key], expected, "{key}");
        let (mut rest, mut original) = (after.clone(), instance(json!({})));
        rest.as_object_mut().unwrap().remove(key);
        original.as_object_mut().unwrap().remove(key);
        assert_eq!(rest, original, "{key}: no other member moves");
        assert_eq!(
            decode(&wire(&plan.document)),
            plan.document,
            "{key}: round trip"
        );
    }
}

#[test]
fn a_member_the_record_never_authored_is_added_and_the_rest_stays_absent() {
    let bare = json!({"contextId":"scene-root","layerUid":"ly_inst","symbolId":"sym_x"});
    let before = world(bare.clone());
    let plan = apply_edit(&before, &edit_with(|e| e.mode = Some("loop".into())));
    assert!(plan.changed);
    assert_eq!(
        stored(&plan.document),
        json!({"contextId":"scene-root","layerUid":"ly_inst","symbolId":"sym_x","symPlayMode":"loop"})
    );
    // An explicit null is a different record from an absent member: naming the value writes it over the null.
    let nulled = world(instance(json!({"symPlayMode":null})));
    let nulled = apply_edit(&nulled, &edit_with(|e| e.mode = Some("loop".into())));
    assert_eq!(stored(&nulled.document)["symPlayMode"], "loop");
}

#[test]
fn an_edit_that_changes_nothing_publishes_nothing() {
    let before = world(instance(
        json!({"symSpeed":2,"symPlacedAt":4,"symSingleFrame":7}),
    ));
    let same = edit_with(|e| {
        e.mode = Some("once".into());
        e.single_frame = Some(7);
        e.placed_at = Some(4);
        e.speed = Some(2.0);
    });
    let plan = apply_edit(&before, &same);
    assert!(!plan.changed);
    assert_eq!(plan.document, before);
    assert!(!apply_edit(&before, &InstanceEdit::default()).changed);
}

#[test]
fn a_pick_on_an_outer_keyframe_is_also_that_keys_component_frame_and_clears_its_blank_flag() {
    let key = json!({"frame":6,"blankOverride":true});
    let before = world(instance(json!({"frameKeys":[key]})));
    let pick = edit_with(|e| {
        e.single_frame = Some(11);
        e.at_frame = Some(6);
    });
    let after = stored(&apply_edit(&before, &pick).document);
    assert_eq!(after["symSingleFrame"], 11);
    assert_eq!(after["frameKeys"], json!([{"frame":6,"componentFrame":11}]));
    // A key that does not exist yet is created in frame order; the pick on the same value twice changes nothing.
    let bare = world(instance(
        json!({"frameKeys":[{"frame":0,"componentFrame":3}]}),
    ));
    let created = apply_edit(&bare, &pick);
    assert_eq!(
        stored(&created.document)["frameKeys"],
        json!([{"frame":0,"componentFrame":3},{"frame":6,"componentFrame":11}])
    );
    assert!(!apply_edit(&created.document, &pick).changed);
    // The first key of a record that never had any.
    let none = world(instance(json!({})));
    assert_eq!(
        stored(&apply_edit(&none, &pick).document)["frameKeys"],
        json!([{"frame":6,"componentFrame":11}])
    );
}

#[test]
fn a_pick_off_an_outer_keyframe_writes_only_the_layer_level_single_frame() {
    // Frame 3 of the instance layer is held, not a keyframe: the baseline `f.isKeyframe` guard skips the key write.
    let before = world(instance(
        json!({"frameKeys":[{"frame":6,"componentFrame":2}]}),
    ));
    let pick = edit_with(|e| {
        e.single_frame = Some(11);
        e.at_frame = Some(3);
    });
    let after = stored(&apply_edit(&before, &pick).document);
    assert_eq!(after["symSingleFrame"], 11);
    assert_eq!(after["frameKeys"], json!([{"frame":6,"componentFrame":2}]));
}

#[test]
fn refusals_are_typed_and_nothing_is_edited() {
    let before = world(instance(json!({})));
    let mode = edit_with(|e| e.mode = Some("loop".into()));
    assert_eq!(
        plan_instance_set(&before, "ly_plain", &mode).unwrap_err(),
        SetupError::NotAnInstance("ly_plain".into())
    );
    assert_eq!(
        plan_instance_set(&before, "xi", &mode).unwrap_err(),
        SetupError::UnknownLayer("xi".into()),
        "a layer of the component is not a scene layer: the entered component is not addressable"
    );
    assert_eq!(
        plan_instance_set(&before, "nope", &mode).unwrap_err(),
        SetupError::UnknownLayer("nope".into())
    );
    let bad: Vec<InstanceEdit> = vec![
        edit_with(|e| e.mode = Some("bounce".into())),
        edit_with(|e| e.mode = Some("Loop".into())),
        edit_with(|e| e.speed = Some(0.0)),
        edit_with(|e| e.speed = Some(-1.0)),
        edit_with(|e| e.speed = Some(f64::NAN)),
        edit_with(|e| e.speed = Some(f64::INFINITY)),
        edit_with(|e| e.at_frame = Some(6)),
        edit_with(|e| {
            e.single_frame = Some(1);
            e.at_frame = Some(24);
        }),
    ];
    for edit in bad {
        assert_eq!(
            plan_instance_set(&before, "ly_inst", &edit).unwrap_err(),
            SetupError::PlaybackInvalid,
            "{edit:?}"
        );
    }
    let v1 = decode(
        &json!({"format":"nemo.native-object-document","formatVersion":1,
        "totalFrames":20,"layers":[{"layerUid":"L"}],"objects":[]}),
    );
    assert_eq!(
        plan_instance_set(&v1, "L", &mode).unwrap_err(),
        SetupError::V1Document
    );
}

#[test]
fn a_document_without_component_tables_has_no_instance() {
    let layers = [layer("ly_inst", "Inst", &[])];
    let bare = decode(&document(&layers, json!({})));
    assert_eq!(
        plan_instance_set(&bare, "ly_inst", &edit_with(|e| e.speed = Some(2.0))).unwrap_err(),
        SetupError::NotAnInstance("ly_inst".into())
    );
}
