//! Hand-written literal controls for duplicate, rename, recolor and the
//! lock / parent policy port (L02-E). The expected after-documents are built from
//! the input's JSON with plain `serde_json::Value` edits, never through the kernel.

use super::fixture::{
    admitted, build, build_value, json_of, layer_uids, parse, plain, text_of, Spec,
};
use super::{
    plan_duplicate, plan_recolor, plan_rename, LifecycleError,
    UnavailableLayerEditPolicy as Nobody, MAX_REVISION,
};
use crate::object_document::ObjectDocument;
use serde_json::{json, Value};

/// Version 1 fixture (no order plan) with extra raw layers appended.
fn with_layers(count: usize, extra: Vec<Value>) -> ObjectDocument {
    let mut value = build_value(&plain(count), 1);
    value["layers"]
        .as_array_mut()
        .expect("layers")
        .extend(extra);
    parse(value)
}

#[test]
fn duplicate_appends_the_copy_with_content_and_order_entries() {
    let before = build(&plain(3), 2);
    let bytes = text_of(&before);
    let plan = plan_duplicate(&before, "L1", 7, &Nobody).expect("duplicate");
    assert!(admitted(&plan.document));
    let (was, now) = (json_of(&before), json_of(&plan.document));
    let fresh = "native-layer:7:0";
    let mut layers = was["layers"].as_array().expect("layers").clone();
    let mut copy = layers[1].clone();
    copy["layerUid"] = json!(fresh);
    copy["name"] = json!("L1 copy");
    layers.push(copy);
    assert_eq!(now["layers"], Value::Array(layers));
    let mut objects = was["objects"].as_array().expect("objects").clone();
    for object in was["objects"].as_array().expect("objects") {
        if object["target"]["layerUid"] == "L1" {
            let mut copy = object.clone();
            copy["target"]["layerUid"] = json!(fresh);
            objects.push(copy);
        }
    }
    assert_eq!(
        objects.len(),
        4 + 2,
        "L1 owns a frame 0 and a frame 1 object"
    );
    assert_eq!(now["objects"], Value::Array(objects));
    assert_eq!(
        now["propertyHolders"], was["propertyHolders"],
        "L1 has no holder"
    );
    let mut order = was["compositionOrder"].clone();
    let stack = json!({"layerUid":fresh,"naturalPosition":3,"order":{"static":[9]}});
    let element = json!({"layerUid":fresh,"strokeId":"s1","naturalPosition":0,"order":{}});
    for context in 0..2 {
        order["contexts"][context]["layers"]
            .as_array_mut()
            .expect("layers")
            .push(stack.clone());
    }
    for scope in 0..2 {
        let elements = order["contexts"][0]["scopes"][scope]["elements"].as_array_mut();
        elements.expect("elements").push(element.clone());
    }
    assert_eq!(now["compositionOrder"], order);
    assert_eq!(now["totalFrames"], was["totalFrames"]);
    let outcome = plan.selection.as_ref().expect("selection");
    assert_eq!(
        (outcome.active, outcome.selected.clone(), outcome.anchor),
        (3, vec![3], Some(3))
    );
    assert_eq!(plan.created.as_deref(), Some(fresh));
    assert!(plan.changed && plan.removed.is_empty() && plan.touched.is_empty());
    assert_eq!(text_of(&before), bytes, "input untouched");
}

#[test]
fn duplicate_retargets_layer_and_element_holders_with_their_controls() {
    let before = build(&plain(3), 2);
    let plan = plan_duplicate(&before, "L0", 5, &Nobody).expect("duplicate");
    let (was, now) = (json_of(&before), json_of(&plan.document));
    let mut holders = was["propertyHolders"].as_array().expect("holders").clone();
    assert_eq!(
        holders.len(),
        3,
        "L0 layer and element holder, L2 layer holder"
    );
    for holder in was["propertyHolders"].as_array().expect("holders") {
        if holder["target"]["layerUid"] == "L0" {
            let mut copy = holder.clone();
            copy["target"]["layerUid"] = json!("native-layer:5:0");
            holders.push(copy);
        }
    }
    assert_eq!(holders.len(), 5);
    assert_eq!(now["propertyHolders"], Value::Array(holders));
    let copied = &now["propertyHolders"][4];
    assert_eq!(copied["target"]["strokeId"], "s0");
    assert_eq!(copied["controls"][0]["key"], "xc_a");
    assert_eq!(
        copied["properties"][1],
        json!({"property":"xc_a","static":[7]})
    );
}

#[test]
fn duplicate_takes_the_first_free_id_and_the_first_free_name() {
    let taken = |uid: &str| json!({"layerUid":uid,"name":uid});
    let document = with_layers(
        1,
        vec![taken("native-layer:7:0"), taken("native-layer:7:1")],
    );
    let plan = plan_duplicate(&document, "L0", 7, &Nobody).expect("duplicate");
    assert_eq!(plan.created.as_deref(), Some("native-layer:7:2"));
    assert!(plan.document.composition_order.value().is_none());
    let specs: Vec<Spec> = ["L0", "L0 copy", "L0 copy 2"]
        .iter()
        .map(|name| Spec {
            name: name.to_string(),
            color: "#5fa875".to_string(),
            visible: true,
        })
        .collect();
    let named = build(&specs, 1);
    let copy = plan_duplicate(&named, "L0", 2, &Nobody).expect("duplicate");
    let name = copy.document.layers[3].name.value().cloned();
    assert_eq!(name.as_deref(), Some("L0 copy 3"));
}

#[test]
fn duplicate_keeps_absent_null_and_explicit_fields_distinct() {
    let ghost = json!({"layerUid":"G","name":"g"});
    let quiet = json!({"layerUid":"H","name":"h","parentLayerUid":null,"visible":false});
    let document = with_layers(1, vec![ghost, quiet]);
    let first = plan_duplicate(&document, "G", 3, &Nobody).expect("duplicate");
    assert_eq!(
        json_of(&first.document)["layers"][3],
        json!({"layerUid":"native-layer:3:0","name":"g copy"})
    );
    let second = plan_duplicate(&document, "H", 3, &Nobody).expect("duplicate");
    let want = json!({"layerUid":"native-layer:3:0","name":"h copy","parentLayerUid":null,"visible":false});
    assert_eq!(json_of(&second.document)["layers"][3], want);
}

#[test]
fn duplicate_refusals_name_the_cause_and_leave_the_input_alone() {
    let document = with_layers(2, vec![json!({"layerUid":"G"})]);
    let bytes = text_of(&document);
    let cases = [
        ("G", 3, LifecycleError::SourceUnnamed("G".to_string())),
        ("zz", 3, LifecycleError::UnknownLayer("zz".to_string())),
        ("L0", MAX_REVISION + 1, LifecycleError::RevisionOutOfRange),
    ];
    for (source, revision, want) in cases {
        assert_eq!(
            plan_duplicate(&document, source, revision, &Nobody).expect_err("refusal"),
            want
        );
    }
    assert_eq!(text_of(&document), bytes);
    let edge = plan_duplicate(&document, "L0", MAX_REVISION, &Nobody).expect("largest revision");
    assert_eq!(
        edge.created.as_deref(),
        Some("native-layer:9007199254740991:0")
    );
    let mut value = build_value(&plain(3), 2);
    let entries = value["compositionOrder"]["contexts"][1]["layers"]
        .as_array_mut()
        .expect("layers");
    entries.retain(|entry| entry["layerUid"] != "L1");
    let broken = plan_duplicate(&parse(value), "L1", 4, &Nobody).expect_err("refusal");
    let want = LifecycleError::OrderPlanIncomplete {
        context_id: "unused-symbol".to_string(),
        layer_uid: "L1".to_string(),
    };
    assert_eq!(broken, want);
}

#[test]
fn rename_changes_only_the_name_and_a_noop_keeps_the_bytes() {
    let document = with_layers(2, vec![json!({"layerUid":"G"})]);
    let bytes = text_of(&document);
    let named = plan_rename(&document, "G", "x", &Nobody).expect("rename");
    assert_eq!(
        json_of(&named.document)["layers"][2],
        json!({"layerUid":"G","name":"x"})
    );
    assert_eq!(named.touched, ["G"]);
    let mut layers = json_of(&document)["layers"].clone();
    layers[1]["name"] = json!("");
    let emptied = plan_rename(&document, "L1", "", &Nobody).expect("rename");
    assert_eq!(
        json_of(&emptied.document)["layers"],
        layers,
        "no trim, empty is stored; parent null, lock and timing intact"
    );
    let shared = plan_rename(&document, "L1", "L0", &Nobody).expect("rename");
    assert_eq!(
        layer_uids(&shared.document),
        layer_uids(&document),
        "names need not be unique"
    );
    let same = plan_rename(&document, "L1", "L1", &Nobody).expect("rename");
    assert!(!same.changed && same.touched.is_empty());
    assert_eq!(text_of(&same.document), bytes);
    let missing = plan_rename(&document, "zz", "q", &Nobody);
    assert_eq!(
        missing.expect_err("refusal"),
        LifecycleError::UnknownLayer("zz".to_string())
    );
    assert_eq!(text_of(&document), bytes);
}

#[test]
fn recolor_paints_the_selection_only_when_the_clicked_layer_is_in_it() {
    let document = build(&plain(4), 2);
    let bytes = text_of(&document);
    let colors = |plan: &super::LifecyclePlan| -> Vec<String> {
        plan.document
            .layers
            .iter()
            .map(|l| l.color.value().cloned().expect("color"))
            .collect()
    };
    let both = ["L1".to_string(), "L2".to_string()];
    let painted = plan_recolor(&document, "L1", &both, "#a1b2c3", &Nobody).expect("recolor");
    assert_eq!(
        colors(&painted),
        ["#5fa875", "#a1b2c3", "#a1b2c3", "#e88bae"]
    );
    assert_eq!(painted.touched, ["L1", "L2"]);
    let outside = plan_recolor(&document, "L0", &both, "#a1b2c3", &Nobody).expect("recolor");
    assert_eq!(
        colors(&outside),
        ["#a1b2c3", "#b49be6", "#f0a585", "#e88bae"]
    );
    let single = ["L2".to_string()];
    let alone = plan_recolor(&document, "L2", &single, "#a1b2c3", &Nobody).expect("recolor");
    assert_eq!(
        alone.touched,
        ["L2"],
        "a one-layer selection is just the clicked layer"
    );
    let already = plan_recolor(&document, "L0", &[], "#5fa875", &Nobody).expect("recolor");
    assert!(!already.changed && already.touched.is_empty());
    assert_eq!(text_of(&already.document), bytes);
    let upper = plan_recolor(&document, "L0", &[], "#5FA875", &Nobody).expect("recolor");
    assert!(
        upper.changed,
        "the stored string is compared, not the color value"
    );
    let wire = json_of(&painted.document);
    assert_eq!(wire["layers"][1]["parentLayerUid"], Value::Null);
    assert_eq!(
        wire["layers"][1]["timing"],
        json_of(&document)["layers"][1]["timing"]
    );
    assert_eq!(text_of(&document), bytes);
}

#[test]
fn recolor_refuses_malformed_colors_and_unknown_layers() {
    let document = with_layers(2, vec![json!({"layerUid":"G"})]);
    let bytes = text_of(&document);
    for bad in ["", "#12345", "123456", "#12345g", "#1234567", "red", "#ééé"] {
        let got = plan_recolor(&document, "L0", &[], bad, &Nobody);
        assert_eq!(
            got.expect_err("refusal"),
            LifecycleError::InvalidColor(bad.to_string()),
            "{bad}"
        );
    }
    let unknown = LifecycleError::UnknownLayer("zz".to_string());
    assert_eq!(
        plan_recolor(&document, "zz", &[], "#112233", &Nobody).expect_err("refusal"),
        unknown
    );
    let pair = ["L0".to_string(), "zz".to_string()];
    assert_eq!(
        plan_recolor(&document, "L0", &pair, "#112233", &Nobody).expect_err("refusal"),
        unknown
    );
    let ghost = plan_recolor(&document, "G", &[], "#112233", &Nobody).expect("recolor");
    assert_eq!(
        json_of(&ghost.document)["layers"][2],
        json!({"layerUid":"G","color":"#112233"})
    );
    assert_eq!(text_of(&document), bytes);
}
