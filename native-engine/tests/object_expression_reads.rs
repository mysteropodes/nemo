//! Real admitted records -> native read providers -> guest -> owner pin.
//! No test supplies a sampler, context port or lookup response.
use super::*;
use crate::object_document::{Presence, ProjectEnvelope, ProjectMetadata, PropertyHolderTarget};
use crate::object_expression_pin::{prepare_property_expression_request, PropertyEvaluation};
use serde_json::json;

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
    let drawing =
        crate::object_codec::decode_ordered_project(&serde_json::to_vec(&drawing).unwrap())
            .unwrap();
    let envelope = ProjectEnvelope::no_media(
        ProjectMetadata::new(320, 180, 25.).unwrap(),
        drawing.into_storage(),
    );
    NativeObjectHistory::from_admitted_project("provider-test", envelope).unwrap()
}
fn limits() -> ExpressionLimits {
    ExpressionLimits::new(4096, 8_388_608, 262_144, 32).unwrap()
}
fn run(code: &str) -> PropertyEvaluation {
    let owner = owner(code);
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let before = crate::object_codec::encode_storage_project(snapshot.document()).unwrap();
    let project = snapshot.project().cloned();
    let result =
        evaluate_property_for_owner(&owner, &snapshot, &target(), "opacity", 5., 0, limits())
            .unwrap();
    assert_eq!(
        crate::object_codec::encode_storage_project(owner.acquire_snapshot(0).unwrap().document())
            .unwrap(),
        before
    );
    assert_eq!(
        owner.acquire_snapshot(0).unwrap().project().cloned(),
        project
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
        assert_eq!(result.value, vec![expected], "{code}: {result:?}");
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
        assert_eq!(result.value, vec![14.], "{code}: {result:?}");
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
    let result = run("self.at(500) + self.at(-20)");
    assert_eq!(result.value, vec![100.], "{result:?}");
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
    let drawing =
        crate::object_codec::decode_ordered_project(&serde_json::to_vec(&drawing).unwrap())
            .unwrap();
    let project = snapshot
        .project()
        .unwrap()
        .with_drawing(drawing.into_storage());
    let owner = NativeObjectHistory::from_admitted_project("keys", project).unwrap();
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let result =
        evaluate_property_for_owner(&owner, &snapshot, &target(), "opacity", 5., 0, limits())
            .unwrap();
    assert_eq!(result.value, vec![6.], "{result:?}");
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

fn change_drawing(
    owner: &NativeObjectHistory,
    edit: impl FnOnce(&mut serde_json::Value),
) -> NativeObjectHistory {
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let mut drawing = serde_json::to_value(snapshot.document()).unwrap();
    edit(&mut drawing);
    let drawing =
        crate::object_codec::decode_ordered_project(&serde_json::to_vec(&drawing).unwrap())
            .unwrap();
    NativeObjectHistory::from_admitted_project(
        "changed-provider",
        snapshot
            .project()
            .unwrap()
            .with_drawing(drawing.into_storage()),
    )
    .unwrap()
}

#[test]
fn nonstring_layer_references_and_nullish_controls_cannot_alias_stored_names() {
    for (argument, name) in [
        ("null", "null"),
        ("undefined", "undefined"),
        ("false", "false"),
        ("1", "1"),
    ] {
        let code = format!("layer({argument}) === null ? value : 99");
        let owner = change_drawing(&owner(&code), |d| d["layers"][1]["name"] = json!(name));
        let snapshot = owner.acquire_snapshot(0).unwrap();
        let result =
            evaluate_property_for_owner(&owner, &snapshot, &target(), "opacity", 5., 0, limits())
                .unwrap();
        assert_eq!(
            (result.value, result.diagnostic),
            (vec![50.], None),
            "{argument}"
        );
        let code = format!("layerControl({argument},'Gain')");
        let owner = change_drawing(&self::owner(&code), |d| {
            d["layers"][1]["layerUid"] = json!(name);
            d["compositionOrder"]["contexts"][0]["layers"][1]["layerUid"] = json!(name);
            d["propertyHolders"][1]["target"]["layerUid"] = json!(name);
        });
        let snapshot = owner.acquire_snapshot(0).unwrap();
        let result =
            evaluate_property_for_owner(&owner, &snapshot, &target(), "opacity", 5., 0, limits())
                .unwrap();
        assert_eq!(result.value, vec![50.]);
        assert!(result.diagnostic.is_some(), "{argument}");
    }
    for (name, argument) in [
        ("null", "null"),
        ("undefined", "undefined"),
        ("opacity", "null"),
        ("opacity", "undefined"),
        ("opacity", "''"),
    ] {
        let code = format!("control({argument})");
        let owner = change_drawing(&owner(&code), |d| {
            d["propertyHolders"][0]["controls"][0]["name"] = json!(name)
        });
        let snapshot = owner.acquire_snapshot(0).unwrap();
        let result =
            evaluate_property_for_owner(&owner, &snapshot, &target(), "opacity", 5., 0, limits())
                .unwrap();
        assert_eq!(result.value, vec![50.]);
        assert!(result.diagnostic.is_some());
    }
    // Controls deliberately coerce other names, unlike layer references.
    let owner = change_drawing(&owner("control(false)[0]"), |d| {
        d["propertyHolders"][0]["controls"][0]["name"] = json!("false")
    });
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let result =
        evaluate_property_for_owner(&owner, &snapshot, &target(), "opacity", 5., 0, limits())
            .unwrap();
    assert_eq!((result.value, result.diagnostic), (vec![-5.], None));
}

#[test]
fn dependency_depth_warning_is_attributed_only_to_its_actual_property() {
    use crate::evaluation::property_expression_runtime::Diagnostic;
    let owner = change_drawing(&owner("layerControl('Ctrl','Gain')"), |d| {
        d["propertyHolders"][1]["properties"][0]["expression"]["code"] =
            json!("control('Gain') + 1");
    });
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let result =
        evaluate_property_for_owner(&owner, &snapshot, &target(), "opacity", 5., 0, limits())
            .unwrap();
    assert_eq!((result.value, result.diagnostic), (vec![14.], None));
    assert!(result
        .dependencies
        .iter()
        .any(|d| d.target.layer_uid() == "Ctrl"
            && d.property == "xc_gain"
            && d.diagnostic == Some(Diagnostic::TooDeep)));
    let mut control = target();
    control.layer_uid = "Ctrl".into();
    let result =
        evaluate_property_for_owner(&owner, &snapshot, &control, "xc_gain", 5., 0, limits())
            .unwrap();
    assert_eq!(result.value, vec![15.]);
    assert_eq!(result.diagnostic, Some(Diagnostic::TooDeep));
    assert_eq!(owner.history_depths(), (0, 0));
}

#[test]
fn public_read_bindings_preserve_user_local_shadowing_without_exposing_host_reads() {
    for (code, expected) in [
        ("var self = 3; return self + value;", 53.),
        (
            "self = {at:function(){return 4}}; return self.at()+value;",
            54.,
        ),
        ("function control(){return 9} return control()+value;", 59.),
        (
            "var layer=function(){return 12}; return layer()+value;",
            62.,
        ),
        (
            "function _get(){throw new Error('local')} return control('Point')[0];",
            -5.,
        ),
        ("typeof __read === 'undefined' ? value : 0", 50.),
    ] {
        let result = run(code);
        assert_eq!(result.value, vec![expected], "{code}: {result:?}");
        assert_eq!(result.diagnostic, None, "{code}");
    }
}
