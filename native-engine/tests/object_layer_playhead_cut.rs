//! Literal cut-at-playhead plans through real v2 storage/range admission.
//! Neither a second range algorithm nor a lifecycle policy mock lives here.
use super::{plan_cut, CutError};
use crate::object_document::RangeEvaluator;
use serde_json::{json, Value};

#[path = "object_component_convert_fixture.rs"]
#[allow(dead_code)]
mod build;
use build::{decode, document, layer, record, wire};

const CREATED: &str = "native-layer:7:0";
fn fixture() -> Value {
    let source = layer("a", "Ink", &[(0,"ink"),(6,"ink"),(16,"ink")]).with(json!({
        "visible":false,"locked":true,"color":"#5FA875","parentLayerUid":"z",
        "shy":true,"keyLock":"layer","range":{"inPoint":2,"outPoint":18,
            "markers":[{"frame":30,"name":" lead α ","color":"#abc"},{"frame":12},{"frame":12,"name":""}]},
        "timing":{"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing",
            "contextId":"scene-root","keys":[{"frame":0},{"frame":8},{"frame":16}],
            "samples":[{"frame":6,"manualEdit":true}]}}));
    let holders = json!([
        {"target":{"contextId":"scene-root","layerUid":"a"},
         "controls":[{"key":"xc_gain","name":"Gain","type":"number","default":[2.5]}],
         "properties":[{"property":"position","static":[1.25,2.5],"track":{"keys":[
            {"frame":-0.5,"v":[3,4],"hold":true,"curvePoints":null},
            {"frame":15.5,"v":[9,10],"curvePoints":[],"hIn":[0,0],"hOut":[0,0]}]},
            "expression":{"enabled":false,"code":"value"}},{"property":"xc_gain","static":[6.25]}]},
        {"target":{"contextId":"scene-root","layerUid":"a","strokeId":"ink"},
         "properties":[{"property":"opacity","static":[77.5],"track":{"keys":[{"frame":12.5,"v":[55]}]}}]},
        {"target":{"contextId":"scene-root","layerUid":"z"},"properties":[]}
    ]);
    document(
        &[
            layer("z", "Before", &[(0, "z")]),
            source,
            layer("c", "Ink (2)", &[(0, "c")]),
        ],
        json!({"propertyHolders":holders}),
    )
}
fn layer_value<'a>(doc: &'a Value, uid: &str) -> &'a Value {
    doc["layers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|layer| layer["layerUid"] == uid)
        .unwrap()
}
fn retarget(mut value: Value, uid: &str) -> Value {
    value["target"]["layerUid"] = json!(uid);
    value
}

#[test]
fn split_copies_authored_frames_motion_and_baseline_metadata_and_inserts_adjacent_without_name_uniquify(
) {
    let input = fixture();
    let source = decode(&input);
    let original = wire(&source);
    let plan = plan_cut(&source, 7, "a", 10).unwrap();
    assert!(plan.changed);
    assert_eq!(plan.created.as_deref(), Some(CREATED));
    let selection = plan.selection.as_ref().unwrap();
    assert_eq!(
        (
            selection.active,
            selection.selected.as_slice(),
            selection.anchor
        ),
        (2, &[2][..], Some(2))
    );
    let after = wire(&plan.document);
    assert_eq!(
        after["layers"]
            .as_array()
            .unwrap()
            .iter()
            .map(|layer| layer["layerUid"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["z", "a", CREATED, "c"]
    );
    let mut expected_source = input["layers"][1].clone();
    expected_source["range"]["outPoint"] = json!(9);
    let mut expected_copy = input["layers"][1].clone();
    expected_copy["layerUid"] = json!(CREATED);
    expected_copy["name"] = json!("Ink (2)");
    // Ordinary createUserLayer starts unlocked; only the separately unsupported
    // component branch copies the source lock in the original baseline.
    expected_copy["locked"] = json!(false);
    expected_copy["range"]["inPoint"] = json!(10);
    expected_copy["range"]["outPoint"] = json!(18);
    assert_eq!(layer_value(&after, "a"), &expected_source);
    assert_eq!(layer_value(&after, CREATED), &expected_copy);
    assert_eq!(layer_value(&after, "a")["locked"], true);
    assert_eq!(layer_value(&after, CREATED)["locked"], false);
    assert_eq!(layer_value(&after, "z"), layer_value(&input, "z"));
    assert_eq!(layer_value(&after, "c"), layer_value(&input, "c"));
    let original_records = input["objects"].as_array().unwrap();
    assert_eq!(
        &after["objects"].as_array().unwrap()[..original_records.len()],
        original_records.as_slice(),
        "all original source and unrelated records survive in encounter order"
    );
    let expected_records: Vec<_> = original_records
        .iter()
        .filter(|record| record["target"]["layerUid"] == "a")
        .cloned()
        .map(|record| retarget(record, CREATED))
        .collect();
    let actual_records: Vec<_> = after["objects"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|record| record["target"]["layerUid"] == CREATED)
        .cloned()
        .collect();
    assert_eq!(
        actual_records, expected_records,
        "both halves retain frames before AND after the cut"
    );
    assert_eq!(
        actual_records
            .iter()
            .map(|record| record["target"]["frameScope"]["frame"].as_u64().unwrap())
            .collect::<Vec<_>>(),
        [0, 6, 16]
    );
    let original_holders = input["propertyHolders"].as_array().unwrap();
    let mut expected_holders = original_holders.clone();
    expected_holders.push(retarget(original_holders[0].clone(), CREATED));
    expected_holders.push(retarget(original_holders[1].clone(), CREATED));
    assert_eq!(after["propertyHolders"], json!(expected_holders));
    assert_eq!(
        wire(&source),
        original,
        "preparation only returns a detached candidate"
    );
    assert!(crate::object_codec::decode_storage_project(
        &crate::object_codec::encode_storage_project(&plan.document).unwrap()
    )
    .is_ok());
}

#[test]
fn actual_link_resolution_materializes_both_halves_drops_live_links_and_preserves_markers() {
    let mut value = fixture();
    value["layers"][0]["range"] = json!({"inPoint":4,"outPoint":20});
    value["layers"][1]["range"] = json!({"inPoint":-5,"outPoint":23,
        "timeLink":{"uid":"z"},"markers":[{"frame":12,"name":"keep"}]});
    value["propertyHolders"][0]["properties"]
        .as_array_mut()
        .unwrap()
        .extend([
            json!({"property":"timeLinkInOffset","static":[1]}),
            json!({"property":"timeLinkOutOffset","static":[-2]}),
        ]);
    let doc = decode(&value);
    let resolved = RangeEvaluator::for_document(&doc, "scene-root", 10)
        .unwrap()
        .resolve("a", true)
        .unwrap();
    assert_eq!((resolved.in_point, resolved.out_point), (5.0, 18.0));
    let after = wire(&plan_cut(&doc, 7, "a", 10).unwrap().document);
    assert_eq!(
        layer_value(&after, "a")["range"],
        json!({"inPoint":-5,"outPoint":9,"markers":[{"frame":12,"name":"keep"}]})
    );
    assert_eq!(
        layer_value(&after, CREATED)["range"],
        json!({"inPoint":10,"outPoint":18,"markers":[{"frame":12,"name":"keep"}]})
    );
    assert!(layer_value(&after, "a")["range"].get("timeLink").is_none());
    assert!(layer_value(&after, CREATED)["range"]
        .get("timeLink")
        .is_none());
}

#[test]
fn range_missing_null_and_explicit_empty_markers_keep_their_original_presence() {
    for range in [
        None,
        Some(json!({"inPoint":null,"outPoint":null,"timeLink":null})),
        Some(json!({"markers":[]})),
    ] {
        let mut value = fixture();
        if let Some(range) = range {
            value["layers"][1]["range"] = range;
        } else {
            value["layers"][1].as_object_mut().unwrap().remove("range");
        }
        let doc = decode(&value);
        let after = wire(&plan_cut(&doc, 7, "a", 10).unwrap().document);
        let mut expected_original = value["layers"][1]
            .get("range")
            .cloned()
            .unwrap_or(json!({}));
        expected_original["outPoint"] = json!(9);
        let mut expected_copy = expected_original.clone();
        expected_copy["inPoint"] = json!(10);
        expected_copy["outPoint"] = json!(23);
        expected_copy.as_object_mut().unwrap().remove("timeLink");
        assert_eq!(layer_value(&after, "a")["range"], expected_original);
        assert_eq!(layer_value(&after, CREATED)["range"], expected_copy);
    }
}

#[test]
fn outside_resolved_range_is_exact_unchanged_even_for_an_unsupported_source() {
    let mut value = fixture();
    value["componentTables"] = json!({"duplicators":{"format":"nemo.native-duplicator-set","formatVersion":1,
        "duplicators":[{"contextId":"scene-root","layerUid":"a","duplicator":{}}],"effectorLayers":[]}});
    let doc = decode(&value);
    let before = wire(&doc);
    assert_eq!(
        plan_cut(&doc, 7, "a", 10).unwrap_err(),
        CutError::Unavailable
    );
    assert_eq!(
        wire(&doc),
        before,
        "unsupported in-range cut cannot manufacture a blank second half"
    );
    for frame in [0, 2, 19, 23] {
        let plan = plan_cut(&doc, 7, "a", frame).unwrap();
        assert!(!plan.changed);
        assert!(plan.created.is_none());
        assert!(plan.selection.is_none());
        assert_eq!(wire(&plan.document), wire(&doc));
    }
}

#[test]
fn fractional_linked_effective_out_refuses_inside_cut_without_changing_input() {
    let mut value = fixture();
    value["layers"][0]["range"] = json!({"inPoint":0,"outPoint":20});
    value["layers"][1]["range"]["timeLink"] = json!({"uid":"z","mode":"out"});
    value["propertyHolders"][0]["properties"]
        .as_array_mut()
        .unwrap()
        .push(json!({"property":"timeLinkOutOffset","static":[-1.5]}));
    let doc = decode(&value);
    let before = wire(&doc);
    assert_eq!(
        plan_cut(&doc, 7, "a", 10).unwrap_err(),
        CutError::Unavailable
    );
    assert_eq!(wire(&doc), before);
    assert!(
        !plan_cut(&doc, 7, "a", 20).unwrap().changed,
        "outside fractional edge stays noop"
    );
}

#[test]
fn identity_suffix_is_first_free_and_invalid_source_or_frame_errors_are_typed() {
    let mut value = fixture();
    value["layers"][2]["layerUid"] = json!(CREATED);
    for record in value["objects"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .filter(|record| record["target"]["layerUid"] == "c")
    {
        record["target"]["layerUid"] = json!(CREATED);
    }
    value["compositionOrder"]["contexts"][0]["layers"][2]["layerUid"] = json!(CREATED);
    for scope in value["compositionOrder"]["contexts"][0]["scopes"]
        .as_array_mut()
        .unwrap()
    {
        for element in scope["elements"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .filter(|element| element["layerUid"] == "c")
        {
            element["layerUid"] = json!(CREATED);
        }
    }
    let doc = decode(&value);
    assert_eq!(
        plan_cut(&doc, 7, "a", 10).unwrap().created.as_deref(),
        Some("native-layer:7:1")
    );
    assert_eq!(
        plan_cut(&doc, 7, "unknown", 10).unwrap_err(),
        CutError::MissingLayer
    );
    assert_eq!(
        plan_cut(&doc, 7, "a", 24).unwrap_err(),
        CutError::InvalidRequest
    );
    assert_eq!(
        plan_cut(&doc, 9_007_199_254_740_992, "a", 10).unwrap_err(),
        CutError::InvalidRequest
    );
}

#[test]
fn another_component_context_with_the_same_layer_uid_and_its_holder_is_not_duplicated() {
    let mut value = fixture();
    let mut inner = record("a", "component-ink", 0);
    inner["target"]["contextId"] = json!("sym-existing");
    value["componentTables"] = json!({"components":{"format":"nemo.native-component-set","formatVersion":1,
        "components":[{"symbolId":"sym-existing","totalFrames":24,"layers":[{"layerUid":"a"}],"objects":[inner]}],"instances":[]}});
    let holder = json!({"target":{"contextId":"sym-existing","layerUid":"a","strokeId":"component-ink"},"properties":[{"property":"opacity","static":[51]}]});
    value["propertyHolders"]
        .as_array_mut()
        .unwrap()
        .push(holder.clone());
    let doc = decode(&value);
    let after = wire(&plan_cut(&doc, 7, "a", 10).unwrap().document);
    assert_eq!(after["componentTables"], value["componentTables"]);
    let matching: Vec<_> = after["propertyHolders"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|entry| entry["target"]["contextId"] == "sym-existing")
        .collect();
    assert_eq!(matching, vec![&holder]);
    assert_eq!(
        after["propertyHolders"].as_array().unwrap().len(),
        value["propertyHolders"].as_array().unwrap().len() + 2
    );
}

#[test]
fn absent_and_explicit_empty_property_holder_tables_are_not_manufactured_or_collapsed() {
    for present in [false, true] {
        let mut value = fixture();
        if present {
            value["propertyHolders"] = json!([]);
        } else {
            value.as_object_mut().unwrap().remove("propertyHolders");
        }
        let doc = decode(&value);
        let after = wire(&plan_cut(&doc, 7, "a", 10).unwrap().document);
        assert_eq!(after.get("propertyHolders"), value.get("propertyHolders"));
    }
}

#[test]
fn fractional_linked_in_with_integer_out_is_losslessly_split_after_exact_inside_check() {
    let mut value = fixture();
    value["layers"][0]["range"] = json!({"inPoint":4,"outPoint":20});
    value["layers"][1]["range"]["inPoint"] = json!(-5);
    value["layers"][1]["range"]["timeLink"] = json!({"uid":"z","mode":"in"});
    value["propertyHolders"][0]["properties"]
        .as_array_mut()
        .unwrap()
        .push(json!({"property":"timeLinkInOffset","static":[1.5]}));
    let doc = decode(&value);
    let range = RangeEvaluator::for_document(&doc, "scene-root", 10)
        .unwrap()
        .resolve("a", true)
        .unwrap();
    assert_eq!((range.in_point, range.out_point), (5.5, 18.0));
    assert!(!plan_cut(&doc, 7, "a", 5).unwrap().changed);
    for frame in [6, 10] {
        let after = wire(&plan_cut(&doc, 7, "a", frame).unwrap().document);
        let mut expected_original = value["layers"][1]["range"].clone();
        expected_original["outPoint"] = json!(frame - 1);
        expected_original
            .as_object_mut()
            .unwrap()
            .remove("timeLink");
        let mut expected_copy = expected_original.clone();
        expected_copy["inPoint"] = json!(frame);
        expected_copy["outPoint"] = json!(18);
        assert_eq!(layer_value(&after, "a")["range"], expected_original);
        assert_eq!(layer_value(&after, CREATED)["range"], expected_copy);
        assert_eq!(
            layer_value(&after, "a")["range"]["inPoint"],
            -5,
            "raw input edge is never rounded or rewritten"
        );
    }
}
