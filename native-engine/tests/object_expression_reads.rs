//! Real admitted records -> native read providers -> guest -> owner pin.
//! No test supplies a sampler, context port or lookup response.
use super::*;
use crate::object_document::{Presence, PropertyHolderTarget};
use crate::object_expression_pin::{prepare_property_expression_request, PropertyEvaluation};
use serde_json::{json, Value};

fn target() -> PropertyHolderTarget {
    PropertyHolderTarget {
        context_id: "scene-root".into(),
        layer_uid: "L0".into(),
        stroke_id: Presence::Missing,
    }
}
fn owner(code: &str) -> NativeObjectHistory {
    let seed = super::tests::owner("opacity", code, true);
    let snapshot = seed.acquire_snapshot(0).unwrap();
    let mut drawing = serde_json::to_value(snapshot.document()).unwrap();
    drawing["layers"][0]["name"] = json!("Ball");
    drawing["layers"]
        .as_array_mut()
        .unwrap()
        .push(json!({"layerUid":"Ctrl","name":"Rig"}));
    drawing["compositionOrder"]["contexts"][0]["layers"]
        .as_array_mut()
        .unwrap()
        .push(json!({"layerUid":"Ctrl","naturalPosition":1,"order":{}}));
    drawing["propertyHolders"][0]["properties"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "property":"rotation","track":{"keys":[{"frame":0,"v":[0]},{"frame":10,"v":[100]}]}
        }));
    drawing["propertyHolders"].as_array_mut().unwrap().push(json!({
        "target":{"contextId":"scene-root","layerUid":"Ctrl"},
        "controls":[{"key":"xc_gain","name":"  Gain  ","type":"number"}],
        "properties":[{"property":"xc_gain","static":[7],"expression":{"enabled":true,"code":"value * 2"}},
            {"property":"position","static":[8,9]}, {"property":"opacity","static":[70]},
            {"property":"rotation","track":{"keys":[{"frame":0,"v":[0]},{"frame":10,"v":[100]}]}}]
    }));
    let envelope = json!({"format":"nemo.native-project-document","formatVersion":1,
        "project":{"canvasW":320,"canvasH":180,"fps":25.0},"drawing":drawing,"viewportReference":null,"assets":[]});
    NativeObjectHistory::from_project_file("provider-test", &serde_json::to_vec(&envelope).unwrap())
        .unwrap()
}
fn limits() -> ExpressionLimits {
    ExpressionLimits::new(4096, 8_388_608, 262_144, 32).unwrap()
}
fn run(code: &str) -> PropertyEvaluation {
    let owner = owner(code);
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let before = owner
        .serialize_project_file(0, snapshot.snapshot_id())
        .unwrap();
    let result =
        evaluate_property_for_owner(&owner, &snapshot, &target(), "opacity", 5., 0, limits())
            .unwrap();
    assert_eq!(
        owner
            .serialize_project_file(0, snapshot.snapshot_id())
            .unwrap(),
        before
    );
    assert_eq!(owner.history_depths(), (0, 0));
    result
}

#[test]
fn real_context_metadata_and_typed_layer_values_reach_the_guest() {
    for (code, expected) in [
        ("comp.width + comp.height + comp.fps + comp.layers", 527.),
        ("thisComp.duration * 25 + thisComp.numLayers", 23.),
        (
            "time * 25 + self.index + (self.name === 'Ball' ? 3 : 0)",
            8.,
        ),
        ("layer('Rig').position[0] + layer('Ctrl').opacity", 78.),
        ("layer('missing') === null ? value : 0", 50.),
        ("layer('Rig').position.at(0)[1]", 9.),
    ] {
        let result = run(code);
        assert_eq!(result.value, vec![expected], "{code}");
        assert_eq!(result.diagnostic, None, "{code}");
    }
}

#[test]
fn controls_read_actual_owner_layer_and_execute_their_stored_expression() {
    for code in [
        "layerControl('Ctrl',' gain ')",
        "layerControl('Rig','GAIN')",
        "layer('Rig').control('Gain')",
        "control('Point')[0] + layerControl('Ctrl','Gain') + 5",
    ] {
        let result = run(code);
        assert_eq!(result.value, vec![14.], "{code}");
        assert_eq!(result.diagnostic, None);
    }
    let result = run("layerControl('Ctrl','deleted')");
    assert_eq!(result.value, vec![50.]);
    assert!(matches!(
        result.diagnostic,
        Some(crate::evaluation::property_expression_runtime::Diagnostic::Runtime { .. })
    ));
}

#[test]
fn raw_self_sampling_never_reenters_its_own_expression() {
    assert_eq!(run("self.at(500) + self.at(-20)").value, vec![100.]);
    let result = run("layer('Rig').at('rotation',5)");
    assert!((result.value[0] - 50.).abs() < 1e-6);
    assert_eq!(result.diagnostic, None);
}

#[test]
fn lookup_uses_the_real_authored_key_track_and_fps() {
    let seed = owner("self.keys.count + key(1).time * 25 + nearestKey(0.4).index");
    let snapshot = seed.acquire_snapshot(0).unwrap();
    let mut drawing = serde_json::to_value(snapshot.document()).unwrap();
    let p = drawing["propertyHolders"][0]["properties"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|p| p["property"] == "opacity")
        .unwrap();
    p["track"] = json!({"keys":[{"frame":2,"v":[20]},{"frame":12,"v":[80]}]});
    let mut project: Value = serde_json::from_slice(
        &seed
            .serialize_project_file(0, snapshot.snapshot_id())
            .unwrap(),
    )
    .unwrap();
    project["drawing"] = drawing;
    let owner =
        NativeObjectHistory::from_project_file("keys", &serde_json::to_vec(&project).unwrap())
            .unwrap();
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let result =
        evaluate_property_for_owner(&owner, &snapshot, &target(), "opacity", 5., 0, limits())
            .unwrap();
    assert_eq!(result.value, vec![6.]);
    assert_eq!(result.diagnostic, None);
}

#[test]
fn foreign_owner_and_same_content_different_snapshot_cannot_publish() {
    let a = owner("layerControl('Ctrl','Gain')");
    let b = owner("layerControl('Ctrl','Gain')");
    let snapshot = a.acquire_snapshot(0).unwrap();
    assert!(
        evaluate_property_for_owner(&b, &snapshot, &target(), "opacity", 0., 0, limits()).is_err()
    );
    let request =
        prepare_property_expression_request(&a, &snapshot, &target(), "opacity", 0., 0).unwrap();
    let candidate = evaluate_property_detached(&request, limits()).unwrap();
    assert!(request.finalize(&b, candidate).is_err());
}
