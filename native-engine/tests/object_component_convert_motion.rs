//! Real convert owner/history tests. Nested-holder admission is the declared
//! prerequisite; expressions here are retained DATA, never raw-sampled as code.
use super::fixture::{decode, document, layer, record, uids, wire};
use super::plan_convert;
use crate::commands::{PreparedComponentSetup, SetupKind};
use crate::history::NativeObjectHistory;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::request_receipts::DispatchErrorCode;
use serde_json::{json, Value};

const SYMBOL: &str = "native-component:1:0";
const INSTANCE: &str = "native-layer:1:0";
const INNER: &str = "native-component:1:0:layer-1";

fn holder(context: &str, layer: &str, stroke: Option<&str>, properties: Value) -> Value {
    let mut target = json!({"contextId":context,"layerUid":layer});
    if let Some(stroke) = stroke {
        target["strokeId"] = json!(stroke);
    }
    json!({"target":target,"properties":properties})
}
fn controls() -> Value {
    json!([{"key":"xc_gain","name":"Gain α","type":"number","default":[2.5]},
        {"key":"xc_point","name":"Point","type":"point","default":[12,34]}])
}
fn properties(enabled: bool) -> Value {
    json!([
        {"property":"position","static":[1.25,-0.0],
         "track":{"keys":[{"frame":-0.5,"v":[2,3],"hold":true,"curvePoints":null},
            {"frame":4.25,"v":[8,9],"hIn":[0,0],"hOut":[0,0],"curvePoints":[]}]},
         "expression":{"enabled":enabled,"code":"value + [controls.Gain, 0]"}},
        {"property":"opacity","static":[67.5],"track":{"keys":[]},
         "expression":{"enabled":false,"code":"value"}},
        {"property":"scale","track":{"keys":[]}}
    ])
}
fn moved(mut value: Value, context: &str, layer: &str) -> Value {
    value["target"]["contextId"] = json!(context);
    value["target"]["layerUid"] = json!(layer);
    value
}
fn fixture(enabled: bool) -> Value {
    let layers = [
        layer("a", "A", &[(0, "ink-a"), (8, "ink-a")]),
        layer("b", "B", &[(0, "ink-b")]),
        layer("c", "C", &[(0, "ink-c")]),
    ];
    let mut first = holder("scene-root", "a", None, properties(enabled));
    first["controls"] = controls();
    first["properties"]
        .as_array_mut()
        .unwrap()
        .push(json!({"property":"xc_gain",
        "static":[6.25],"track":{"keys":[{"frame":1.5,"v":[11],"hold":true}]},
        "expression":{"enabled":false,"code":"value * 2"}}));
    let mut element = holder("scene-root", "a", Some("ink-a"), properties(enabled));
    element["controls"] = controls();
    // A real custom-control authored value is valid here because its descriptor
    // and full property data stay together on this selected element.
    element["properties"]
        .as_array_mut()
        .unwrap()
        .push(json!({"property":"xc_gain",
        "static":[3.75],"track":{"keys":[{"frame":2.5,"v":[9],"hold":true}]}}));
    let mut second = holder("scene-root", "b", None, properties(!enabled));
    second["controls"] = json!([]);
    let unrelated = holder(
        "scene-root",
        "c",
        None,
        json!([{"property":"rotation","static":[23]}]),
    );
    let mut old_object = record("a", "old-ink", 0);
    old_object["target"]["contextId"] = json!("sym-existing");
    let old_holder = holder(
        "sym-existing",
        "a",
        Some("old-ink"),
        json!([{"property":"opacity","static":[91.25]}]),
    );
    document(
        &layers,
        json!({"propertyHolders":[first,element,second,unrelated,old_holder],
        "componentTables":{"components":{"format":"nemo.native-component-set","formatVersion":1,
            "components":[{"symbolId":"sym-existing","totalFrames":24,
                "layers":[{"layerUid":"a"}],"objects":[old_object]}],"instances":[]}}}),
    )
}
fn owner(value: &Value) -> NativeObjectHistory {
    NativeObjectHistory::new("component-motion", decode(value)).unwrap()
}
fn bytes(owner: &NativeObjectHistory) -> Vec<u8> {
    encode_storage_project(
        owner
            .acquire_snapshot(owner.content_revision())
            .unwrap()
            .document(),
    )
    .unwrap()
}
fn saved(owner: &NativeObjectHistory) -> Value {
    serde_json::from_slice(&bytes(owner)).unwrap()
}
fn prepare(owner: &NativeObjectHistory, layers: &[&str]) -> PreparedComponentSetup {
    let request =
        json!({"command":"object.component.convert","contextId":"scene-root","layerUids":layers});
    owner
        .prepare_component_setup_json(SetupKind::Convert, request.to_string().as_bytes())
        .unwrap()
}
fn inner_raw(mut value: Value, layer: &str) -> Value {
    value = moved(value, SYMBOL, layer);
    value.as_object_mut().unwrap().remove("controls");
    value["properties"]
        .as_array_mut()
        .unwrap()
        .retain(|property| property["property"] != "xc_gain");
    for property in value["properties"].as_array_mut().unwrap() {
        property.as_object_mut().unwrap().remove("expression");
    }
    value
}
fn outer_expressions(value: &Value) -> Value {
    json!({"target":{"contextId":"scene-root","layerUid":INSTANCE},"controls":value["controls"],
        "properties":[{"property":"position","expression":value["properties"][0]["expression"]},
            {"property":"opacity","expression":value["properties"][1]["expression"]},value["properties"][3]]})
}

#[test]
fn single_conversion_retargets_element_motion_but_keeps_outer_layer_and_other_context_bytes() {
    for enabled in [false, true] {
        let input = fixture(enabled);
        let mut history = owner(&input);
        let original = bytes(&history);
        let old = history.acquire_snapshot(0).unwrap();
        let candidate = prepare(&history, &["a"]);
        assert_eq!(candidate.symbol_id(), Some(SYMBOL));
        assert_eq!(candidate.created(), None);
        assert_eq!(bytes(&history), original);
        let mut expected = input["propertyHolders"].clone();
        expected[1] = moved(expected[1].clone(), SYMBOL, INNER);
        assert_eq!(wire(candidate.document())["propertyHolders"], expected);
        assert_eq!(history.commit_component_setup(candidate).unwrap(), 1);
        assert_eq!(history.history_depths(), (1, 0));
        assert_eq!(saved(&history)["propertyHolders"], expected);
        assert_eq!(encode_storage_project(old.document()).unwrap(), original);
        assert_eq!(
            wire(&decode_storage_project(&bytes(&history)).unwrap())["propertyHolders"],
            expected
        );
    }
}

#[test]
fn merge_splits_first_source_expressions_once_and_keeps_raw_keys_inside_in_layer_order() {
    for selected in [["b", "a"], ["a", "b"]] {
        let input = fixture(true);
        let mut history = owner(&input);
        let original = bytes(&history);
        let candidate = prepare(&history, &selected);
        let holders = input["propertyHolders"].as_array().unwrap();
        let expected = json!([
            inner_raw(holders[0].clone(), "a"),
            outer_expressions(&holders[0]),
            moved(holders[1].clone(), SYMBOL, "a"),
            moved(holders[2].clone(), SYMBOL, "b"),
            holders[3],
            holders[4]
        ]);
        assert_eq!(wire(candidate.document())["propertyHolders"], expected);
        assert_eq!(bytes(&history), original);
        assert_eq!(history.commit_component_setup(candidate).unwrap(), 1);
        let after = saved(&history);
        assert_eq!(after["propertyHolders"], expected);
        assert_eq!(
            after["componentTables"]["components"]["components"][1]["layers"][0]["layerUid"],
            "a"
        );
        assert_eq!(
            after["componentTables"]["components"]["components"][1]["layers"][1]["layerUid"],
            "b"
        );
        let outer = after["propertyHolders"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|holder| {
                holder["target"] == json!({"contextId":"scene-root","layerUid":INSTANCE})
            })
            .collect::<Vec<_>>();
        assert_eq!(outer.len(), 1, "first-source lift occurs exactly once");
        assert_eq!(
            outer[0]["properties"][2], holders[0]["properties"][3],
            "declared custom-control raw/track/expression travels with its definition"
        );
        assert!(expected[0]["properties"]
            .as_array()
            .unwrap()
            .iter()
            .all(|p| p["property"] != "xc_gain"));
        assert!(expected[0]["properties"]
            .as_array()
            .unwrap()
            .iter()
            .all(|p| p.get("expression").is_none()));
        assert_eq!(
            expected[2]["properties"][0]["expression"]["enabled"], true,
            "element expressions remain inside, not lifted"
        );
        assert_eq!(
            expected[3]["properties"][0]["expression"]["enabled"], false,
            "nonfirst layer keeps full disabled expression data"
        );
    }
}

#[test]
fn converted_motion_is_one_history_entry_with_exact_save_reopen_undo_redo_and_retained_snapshots() {
    for layers in [vec!["a"], vec!["b", "a"]] {
        let mut history = owner(&fixture(true));
        let before = bytes(&history);
        let old = history.acquire_snapshot(0).unwrap();
        let candidate = prepare(&history, &layers);
        let planned = encode_storage_project(candidate.document()).unwrap();
        assert_eq!(history.history_depths(), (0, 0));
        assert_eq!(history.commit_component_setup(candidate).unwrap(), 1);
        assert_eq!(history.history_depths(), (1, 0));
        let after = bytes(&history);
        let retained = history.acquire_snapshot(1).unwrap();
        assert_eq!(after, planned);
        let reopened = decode_storage_project(&after).unwrap();
        assert_eq!(encode_storage_project(&reopened).unwrap(), after);
        assert_eq!(encode_storage_project(old.document()).unwrap(), before);
        assert_eq!(history.undo(1).unwrap(), 2);
        assert_eq!(bytes(&history), before);
        assert_eq!(history.history_depths(), (0, 1));
        assert_eq!(encode_storage_project(retained.document()).unwrap(), after);
        assert_eq!(history.redo(2).unwrap(), 3);
        assert_eq!(bytes(&history), after);
        assert_eq!(history.history_depths(), (1, 0));
    }
}

#[test]
fn motion_conversion_candidate_refuses_foreign_and_stale_commits_without_partial_holder_writes() {
    let input = fixture(true);
    let mut history = owner(&input);
    let candidate = prepare(&history, &["a", "b"]);
    let mut other = owner(&input);
    let other_before = bytes(&other);
    assert_eq!(
        other.commit_component_setup(candidate.clone()).unwrap_err(),
        DispatchErrorCode::WrongDocument
    );
    assert_eq!(
        (bytes(&other), other.history_depths()),
        (other_before, (0, 0))
    );
    let mut foreign = NativeObjectHistory::new("foreign", decode(&input)).unwrap();
    let foreign_before = bytes(&foreign);
    assert_eq!(
        foreign
            .commit_component_setup(candidate.clone())
            .unwrap_err(),
        DispatchErrorCode::WrongInstance
    );
    assert_eq!(bytes(&foreign), foreign_before);
    history.commit_component_setup(candidate.clone()).unwrap();
    let after = bytes(&history);
    assert_eq!(
        history.commit_component_setup(candidate).unwrap_err(),
        DispatchErrorCode::StaleRevision
    );
    assert_eq!((bytes(&history), history.history_depths()), (after, (1, 0)));
}

#[test]
fn component_holder_presence_is_not_manufactured_when_no_motion_was_authored() {
    for members in [json!({}), json!({"propertyHolders":[]})] {
        let value = document(
            &[
                layer("a", "A", &[(0, "ink-a")]),
                layer("b", "B", &[(0, "ink-b")]),
            ],
            members,
        );
        for selected in [uids(&["a"]), uids(&["b", "a"])] {
            let plan = plan_convert(&decode(&value), 1, &selected).unwrap();
            assert_eq!(
                wire(&plan.document).get("propertyHolders"),
                value.get("propertyHolders")
            );
        }
    }
}

#[cfg(feature = "evaluation")]
#[test]
fn raw_sampling_of_converted_layer_and_element_tracks_uses_real_nested_owner_and_retained_frame_values(
) {
    use crate::object_document::PropertyHolderTarget;
    use crate::object_frame_packet::sample_property_raw;
    let tracks = json!([
        {"target":{"contextId":"scene-root","layerUid":"a"},
         "controls":[{"key":"xc_gain","name":"Gain","type":"number","default":[2.5]}],"properties":[
            {"property":"position","static":[1,2],"track":{"keys":[
                {"frame":-0.5,"v":[3,4],"hold":true},{"frame":4.25,"v":[9,10]}]}},
            {"property":"xc_gain","static":[6.25],"track":{"keys":[
                {"frame":1.5,"v":[11],"hold":true},{"frame":5.5,"v":[19]}]},
             "expression":{"enabled":false,"code":"value * 2"}}]},
        {"target":{"contextId":"scene-root","layerUid":"a","strokeId":"ink-a"},"properties":[
            {"property":"opacity","static":[75],"track":{"keys":[
                {"frame":-0.5,"v":[25],"hold":true},{"frame":4.25,"v":[90]}]}}]}
    ]);
    let input = document(
        &[
            layer("a", "A", &[(0, "ink-a")]),
            layer("b", "B", &[(0, "ink-b")]),
        ],
        json!({"propertyHolders":tracks}),
    );
    let mut history = owner(&input);
    let candidate = prepare(&history, &["a", "b"]);
    history.commit_component_setup(candidate).unwrap();
    let snapshot = history.acquire_snapshot(1).unwrap();
    let layer_target: PropertyHolderTarget =
        serde_json::from_value(json!({"contextId":SYMBOL,"layerUid":"a"})).unwrap();
    let element_target: PropertyHolderTarget =
        serde_json::from_value(json!({"contextId":SYMBOL,"layerUid":"a","strokeId":"ink-a"}))
            .unwrap();
    for (frame, position, opacity) in [
        (-2.0, vec![3.0, 4.0], vec![25.0]),
        (2.5, vec![3.0, 4.0], vec![25.0]),
        (8.0, vec![9.0, 10.0], vec![90.0]),
    ] {
        assert_eq!(
            sample_property_raw(&snapshot, &layer_target, "position", frame).unwrap(),
            position
        );
        assert_eq!(
            sample_property_raw(&snapshot, &element_target, "opacity", frame).unwrap(),
            opacity
        );
    }
    let outer: PropertyHolderTarget =
        serde_json::from_value(json!({"contextId":"scene-root","layerUid":INSTANCE})).unwrap();
    assert_eq!(
        sample_property_raw(&snapshot, &outer, "xc_gain", 2.5).unwrap(),
        vec![11.0]
    );
    assert_eq!(
        sample_property_raw(&snapshot, &outer, "xc_gain", 8.0).unwrap(),
        vec![19.0]
    );
    assert!(snapshot.document().property_holders().unwrap()[0]
        .properties()
        .iter()
        .all(|p| p.property() != "xc_gain"));
    let reopened = decode_storage_project(&bytes(&history)).unwrap();
    let reopened_history = NativeObjectHistory::new("reopened-motion", reopened).unwrap();
    let reopened_snapshot = reopened_history.acquire_snapshot(0).unwrap();
    assert_eq!(
        sample_property_raw(&reopened_snapshot, &outer, "xc_gain", 2.5).unwrap(),
        vec![11.0]
    );
}

#[test]
fn first_source_lift_depends_on_layer_order_not_holder_encounter_or_request_order() {
    let mut input = fixture(true);
    let holders = input["propertyHolders"].as_array().unwrap().clone();
    input["propertyHolders"] = json!([holders[2], holders[1], holders[3], holders[0], holders[4]]);
    let source = decode(&input);
    let before = wire(&source);
    let plan = plan_convert(&source, 1, &uids(&["b", "a"])).unwrap();
    assert_eq!(
        wire(&plan.document)["propertyHolders"],
        json!([
            moved(holders[2].clone(), SYMBOL, "b"),
            moved(holders[1].clone(), SYMBOL, "a"),
            holders[3],
            inner_raw(holders[0].clone(), "a"),
            outer_expressions(&holders[0]),
            holders[4]
        ])
    );
    assert_eq!(wire(&source), before);
}

#[test]
fn explicit_empty_controls_lift_once_while_missing_controls_and_empty_tracks_stay_distinct() {
    for present in [false, true] {
        let mut first = holder(
            "scene-root",
            "a",
            None,
            json!([{"property":"scale","track":{"keys":[]}}]),
        );
        if present {
            first["controls"] = json!([]);
        }
        let second = holder("scene-root", "b", None, json!([]));
        let input = document(
            &[layer("a", "A", &[(0, "a")]), layer("b", "B", &[(0, "b")])],
            json!({"propertyHolders":[first,second]}),
        );
        let mut history = owner(&input);
        let candidate = prepare(&history, &["b", "a"]);
        history.commit_component_setup(candidate).unwrap();
        let mut raw = moved(first.clone(), SYMBOL, "a");
        raw.as_object_mut().unwrap().remove("controls");
        let mut expected = vec![raw];
        if present {
            expected.push(
                json!({"target":{"contextId":"scene-root","layerUid":INSTANCE},
                "controls":[],"properties":[]}),
            );
        }
        expected.push(moved(second, SYMBOL, "b"));
        assert_eq!(saved(&history)["propertyHolders"], json!(expected));
        let after = bytes(&history);
        assert_eq!(
            encode_storage_project(&decode_storage_project(&after).unwrap()).unwrap(),
            after
        );
    }
}

#[test]
fn custom_only_first_holder_keeps_empty_inner_container_and_one_complete_outer_control_chain() {
    let first = json!({"target":{"contextId":"scene-root","layerUid":"a"},"controls":controls(),
        "properties":[{"property":"xc_gain","static":[4.5],"track":{"keys":[{"frame":3.25,"v":[7]}]},
            "expression":{"enabled":true,"code":"value"}}]});
    let input = document(
        &[layer("a", "A", &[(0, "a")]), layer("b", "B", &[(0, "b")])],
        json!({"propertyHolders":[first]}),
    );
    let mut history = owner(&input);
    let candidate = prepare(&history, &["a", "b"]);
    history.commit_component_setup(candidate).unwrap();
    let mut outer = first;
    outer["target"]["layerUid"] = json!(INSTANCE);
    assert_eq!(
        saved(&history)["propertyHolders"],
        json!([
            {"target":{"contextId":SYMBOL,"layerUid":"a"},"properties":[]},outer
        ])
    );
    assert_eq!(history.history_depths(), (1, 0));
    assert!(decode_storage_project(&bytes(&history)).is_ok());
}
