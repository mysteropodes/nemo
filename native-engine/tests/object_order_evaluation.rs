//! Manual baseline sequences/ranks, never computed with the native sort itself.
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::object_document::FrameScopeKind;
use crate::object_frame_packet::{
    prepare_object_frame, ObjectFrameError, ObjectFramePacket, ObjectFrameSelector,
};
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};

fn frozen() -> Value {
    serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-order-v2/baseline.json"
    ))
    .unwrap()
}
fn record(layer: &str, id: &str, frame: u32, kind: &str) -> Value {
    json!({"schemaVersion":1,"family":"closed-solid-fill-cubic-path",
        "target":{"contextId":"scene-root","frameScope":{"kind":kind,"frame":frame},"layerUid":layer,"strokeId":id},
        "geometry":{"kind":"cubic-path","closed":true,"coordinateSpace":"document-world","handleSpace":"relative",
        "segments":[{"point":{"x":0,"y":0},"handleIn":{"x":0,"y":0},"handleOut":{"x":1,"y":0}},
        {"point":{"x":10,"y":10},"handleIn":{"x":0,"y":-1},"handleOut":{"x":0,"y":0}}]},
        "fill":{"kind":"solid","r":1,"g":0,"b":0,"a":0.75}})
}
pub(super) fn document(frames: &[u32]) -> Value {
    let mut objects = vec![];
    let mut scopes = vec![];
    for frame in frames {
        let mut elements = vec![];
        for (layer, ids) in [
            ("L0", vec!["a", "b", "c"]),
            ("L1", vec!["a"]),
            ("L2", vec!["a"]),
        ] {
            for (position, id) in ids.into_iter().enumerate() {
                objects.push(record(layer, id, *frame, "authored"));
                elements.push(
                    json!({"layerUid":layer,"strokeId":id,"naturalPosition":position,"order":{}}),
                );
            }
        }
        scopes.push(json!({"frameScope":{"kind":"authored","frame":frame},"elements":elements}));
    }
    objects.push(record("L0", "a", frames[0], "reference"));
    json!({"format":"nemo.native-object-document","formatVersion":2,"totalFrames":51,
        "layers":[{"layerUid":"L2"},{"layerUid":"L0"},{"layerUid":"L1"}],"objects":objects,
        "compositionOrder":{"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
        "contexts":[{"contextId":"scene-root","layers":[
        {"layerUid":"L0","naturalPosition":0,"order":{}},{"layerUid":"L1","naturalPosition":1,"order":{}},
        {"layerUid":"L2","naturalPosition":2,"order":{}}],"scopes":scopes}],
        "migration":"explicit-plan-required","referencePolicy":"unresolved"}})
}
fn order(holder: &Value) -> Value {
    let mut result = json!({});
    if let Some(value) = holder.pointer("/motionStatic/order") {
        result["static"] = value.clone();
    }
    if let Some(value) = holder.pointer("/motion/order") {
        result["track"] = value.clone();
    }
    if let Some(value) = holder.pointer("/expressions/order") {
        result["expression"] = json!({"enabled":value["enabled"],"code":value["code"]});
    }
    result
}
fn snapshot(value: &Value) -> ObjectSnapshot {
    ObjectSnapshot::new(
        "order-eval",
        decode_storage_project(&serde_json::to_vec(value).unwrap()).unwrap(),
    )
    .unwrap()
}
fn selector(snapshot: &ObjectSnapshot, frame: u32) -> ObjectFrameSelector<'_> {
    ObjectFrameSelector {
        instance_id: snapshot.instance_id(),
        document_id: snapshot.document_id(),
        content_revision: snapshot.content_revision(),
        document_snapshot_id: snapshot.snapshot_id(),
        context_id: "scene-root",
        scope_kind: FrameScopeKind::Authored,
        frame,
    }
}
fn packet(snapshot: &ObjectSnapshot, frame: u32) -> ObjectFramePacket {
    prepare_object_frame(snapshot, &selector(snapshot, frame)).unwrap()
}
fn layers(packet: &ObjectFramePacket) -> Vec<String> {
    packet
        .numeric_order()
        .unwrap()
        .layers()
        .iter()
        .map(|layer| {
            packet.source_layers()[layer.source_layer_index()]
                .layer_uid()
                .to_owned()
        })
        .collect()
}
fn rank(packet: &ObjectFramePacket, uid: &str) -> f64 {
    packet
        .numeric_order()
        .unwrap()
        .layers()
        .iter()
        .find(|l| packet.source_layers()[l.source_layer_index()].layer_uid() == uid)
        .unwrap()
        .value()
}
fn elements(packet: &ObjectFramePacket, uid: &str) -> Vec<String> {
    let layer = packet
        .numeric_order()
        .unwrap()
        .layers()
        .iter()
        .find(|l| packet.source_layers()[l.source_layer_index()].layer_uid() == uid)
        .unwrap();
    layer
        .elements()
        .iter()
        .map(|e| {
            packet
                .records()
                .find(|r| r.source_object_index() == e.source_object_index())
                .unwrap()
                .record()
                .target()
                .stroke_id()
                .to_owned()
        })
        .collect()
}

#[test]
fn numeric_order_frozen_static_natural_displacement_collision_round_clamp_and_stable_elements() {
    let fixed = frozen();
    for sample in fixed["layerCases"].as_array().unwrap() {
        let mut value = document(&[7]);
        for (index, holder) in sample["holders"].as_array().unwrap().iter().enumerate() {
            value["compositionOrder"]["contexts"][0]["layers"][index]["order"] = order(holder);
        }
        let source = snapshot(&value);
        let result = packet(&source, 7);
        assert_eq!(
            serde_json::to_value(layers(&result)).unwrap(),
            sample["expected"],
            "{}",
            sample["id"]
        );
        assert_eq!(
            result
                .records()
                .map(|r| r.source_object_index())
                .collect::<Vec<_>>(),
            vec![0, 1, 2, 3, 4]
        );
        assert_eq!(
            serde_json::from_slice::<Value>(&encode_storage_project(source.document()).unwrap())
                .unwrap(),
            value
        );
    }
    for sample in fixed["elementCases"].as_array().unwrap() {
        let mut value = document(&[7]);
        value["objects"].as_array_mut().unwrap().swap(0, 2);
        for (index, id) in ["a", "b", "c"].into_iter().enumerate() {
            if let Some(holder) = sample["holders"].get(id) {
                value["compositionOrder"]["contexts"][0]["scopes"][0]["elements"][index]["order"] =
                    order(holder);
            }
        }
        value["compositionOrder"]["contexts"][0]["scopes"][0]["elements"]
            .as_array_mut()
            .unwrap()
            .reverse();
        let result = packet(&snapshot(&value), 7);
        assert_eq!(
            serde_json::to_value(elements(&result, "L0")).unwrap(),
            sample["expected"],
            "{}",
            sample["id"]
        );
        assert_eq!(elements(&result, "L1"), vec!["a"]);
    }
}

#[test]
fn numeric_order_frozen_linear_held_nonlinear_frames_and_empty_tracks() {
    let fixed = frozen();
    let case = &fixed["animated"]["linearAndHold"];
    let frames: Vec<_> = case["samples"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["frame"].as_u64().unwrap() as u32)
        .collect();
    let mut value = document(&frames);
    for (index, holder) in case["holders"].as_array().unwrap().iter().enumerate() {
        value["compositionOrder"]["contexts"][0]["layers"][index]["order"] = order(holder);
    }
    let source = snapshot(&value);
    for sample in case["samples"].as_array().unwrap() {
        let result = packet(&source, sample["frame"].as_u64().unwrap() as u32);
        assert_eq!(
            serde_json::to_value(layers(&result)).unwrap(),
            sample["expected"]
        );
        for (index, uid) in ["L0", "L1", "L2"].into_iter().enumerate() {
            assert_eq!(rank(&result, uid), sample["ranks"][index].as_f64().unwrap());
        }
    }
    for name in ["nonlinear", "held"] {
        let case = &fixed["animated"][name];
        let samples = case["samples"].as_array().unwrap();
        let frames: Vec<_> = samples
            .iter()
            .filter_map(|s| s["frame"].as_u64().map(|f| f as u32))
            .collect();
        let mut value = document(&frames);
        value["compositionOrder"]["contexts"][0]["layers"][0]["order"] = order(&case["holder"]);
        let source = snapshot(&value);
        for sample in samples.iter().filter(|s| s["frame"].as_u64().is_some()) {
            assert_eq!(
                rank(
                    &packet(&source, sample["frame"].as_u64().unwrap() as u32),
                    "L0"
                ),
                sample["value"].as_f64().unwrap()
            );
        }
    }
    let mut value = document(&[7]);
    value["compositionOrder"]["contexts"][0]["layers"][0]["order"] = json!({"track":{"keys":[]}});
    let result = packet(&snapshot(&value), 7);
    assert_eq!(layers(&result), vec!["L0", "L1", "L2"]);
    assert_eq!(rank(&result, "L0"), 3.0);
}

#[test]
fn numeric_order_elements_evaluate_per_layer_at_exact_authored_frame() {
    let mut value = document(&[0, 5, 10, 15]);
    for scope in value["compositionOrder"]["contexts"][0]["scopes"]
        .as_array_mut()
        .unwrap()
    {
        scope["elements"][0]["order"] = json!({"static":[1]});
        scope["elements"][1]["order"] = json!({"track":{"keys":[
            {"frame":0,"v":[0],"curvePoints":[]}, {"frame":10,"v":[2],"hold":true},
            {"frame":20,"v":[-2]}]}});
    }
    let source = snapshot(&value);
    for (frame, expected) in [
        (0, vec!["a", "b", "c"]),
        (5, vec!["a", "b", "c"]),
        (10, vec!["b", "a", "c"]),
        (15, vec!["b", "a", "c"]),
    ] {
        let result = packet(&source, frame);
        assert_eq!(elements(&result, "L0"), expected);
        assert_eq!(elements(&result, "L1"), vec!["a"]);
        assert_eq!(result.records().count(), 5);
    }
}

#[test]
fn numeric_order_default_null_empty_curves_signed_keys_and_overshoot_are_accountable() {
    let mut value = document(&[5]);
    let keys = json!([{"frame":0,"v":[1]},{"frame":20,"v":[3]}]);
    value["compositionOrder"]["contexts"][0]["layers"][0]["order"] = json!({"track":{"keys":keys}});
    let missing = rank(&packet(&snapshot(&value), 5), "L0");
    assert_eq!(missing, 1.312);
    value["compositionOrder"]["contexts"][0]["layers"][0]["order"]["track"]["keys"][0]
        ["curvePoints"] = Value::Null;
    assert_eq!(rank(&packet(&snapshot(&value), 5), "L0"), missing);
    value["compositionOrder"]["contexts"][0]["layers"][0]["order"]["track"]["keys"][0]
        ["curvePoints"] = json!([]);
    assert_eq!(rank(&packet(&snapshot(&value), 5), "L0"), 1.5);
    // A manually specified curve overshoots y=1: keep the numeric rank, then clamp insertion only.
    value["compositionOrder"]["contexts"][0]["layers"][0]["order"]["track"]["keys"][0]
        ["curvePoints"] = json!([{"x":0,"y":0},{"x":0.25,"y":2},{"x":1,"y":1}]);
    assert_eq!(rank(&packet(&snapshot(&value), 5), "L0"), 5.0);
    assert_eq!(
        layers(&packet(&snapshot(&value), 5)),
        vec!["L0", "L1", "L2"]
    );
    value["compositionOrder"]["contexts"][0]["layers"][0]["order"] = json!({"track":{"keys":[
        {"frame":-0.5,"v":[1],"curvePoints":[]},{"frame":1.5,"v":[3]},{"frame":40,"v":[5]}]}});
    value["compositionOrder"]["contexts"][0]["scopes"][0]["frameScope"]["frame"] = json!(0);
    for r in value["objects"].as_array_mut().unwrap() {
        r["target"]["frameScope"]["frame"] = json!(0);
    }
    assert_eq!(rank(&packet(&snapshot(&value), 0), "L0"), 1.5);
}

#[test]
fn numeric_order_activated_expressions_refuse_but_dormant_disabled_empty_metadata_does_not_execute()
{
    let mut value = document(&[7]);
    let base = &mut value["compositionOrder"]["contexts"][0]["layers"][0]["order"];
    *base = json!({"static":[2],"expression":{"enabled":false,"code":"throw forever"}});
    assert_eq!(rank(&packet(&snapshot(&value), 7), "L0"), 2.0);
    value["compositionOrder"]["contexts"][0]["layers"][0]["order"]["expression"]["enabled"] =
        json!(true);
    let source = snapshot(&value);
    let before = encode_storage_project(source.document()).unwrap();
    assert_eq!(
        prepare_object_frame(&source, &selector(&source, 7)).unwrap_err(),
        ObjectFrameError::Unavailable
    );
    assert_eq!(encode_storage_project(source.document()).unwrap(), before);
    value["compositionOrder"]["contexts"][0]["layers"][0]["order"]["expression"]["code"] =
        json!("");
    assert_eq!(rank(&packet(&snapshot(&value), 7), "L0"), 2.0);
    value["compositionOrder"]["contexts"][0]["layers"][0]["order"] =
        json!({"expression":{"enabled":true,"code":"value+999"}});
    assert_eq!(rank(&packet(&snapshot(&value), 7), "L0"), 3.0);
    value["compositionOrder"]["contexts"][0]["scopes"][0]["elements"][0]["order"] =
        json!({"expression":{"enabled":true,"code":"value+999"}});
    assert_eq!(
        elements(&packet(&snapshot(&value), 7), "L0"),
        vec!["a", "b", "c"]
    );
    value["compositionOrder"]["contexts"][0]["scopes"][0]["elements"][1]["order"] =
        json!({"track":{"keys":[]}});
    let source = snapshot(&value);
    assert_eq!(
        prepare_object_frame(&source, &selector(&source, 7)).unwrap_err(),
        ObjectFrameError::Unavailable
    );
}

#[test]
fn numeric_order_reference_nonroot_bad_pins_and_missing_scope_refuse_without_image_change() {
    let source = snapshot(&document(&[7]));
    let before = encode_storage_project(source.document()).unwrap();
    let mut selected = selector(&source, 7);
    selected.scope_kind = FrameScopeKind::Reference;
    assert_eq!(
        prepare_object_frame(&source, &selected).unwrap_err(),
        ObjectFrameError::Unavailable
    );
    let mut selected = selector(&source, 7);
    selected.context_id = "symbol-A";
    assert_eq!(
        prepare_object_frame(&source, &selected).unwrap_err(),
        ObjectFrameError::Unavailable
    );
    let mut selected = selector(&source, 7);
    selected.instance_id = "wrong";
    assert_eq!(
        prepare_object_frame(&source, &selected).unwrap_err(),
        ObjectFrameError::WrongInstance
    );
    let mut selected = selector(&source, 7);
    selected.document_id = "wrong";
    assert_eq!(
        prepare_object_frame(&source, &selected).unwrap_err(),
        ObjectFrameError::WrongDocument
    );
    let mut selected = selector(&source, 7);
    selected.content_revision = 1;
    assert_eq!(
        prepare_object_frame(&source, &selected).unwrap_err(),
        ObjectFrameError::WrongRevision
    );
    let mut selected = selector(&source, 7);
    selected.document_snapshot_id = "stale";
    assert_eq!(
        prepare_object_frame(&source, &selected).unwrap_err(),
        ObjectFrameError::WrongSnapshot
    );
    assert_eq!(
        prepare_object_frame(&source, &selector(&source, 51)).unwrap_err(),
        ObjectFrameError::FrameOutOfRange
    );
    assert_eq!(
        prepare_object_frame(&source, &selector(&source, 8)).unwrap_err(),
        ObjectFrameError::Unavailable
    );
    assert_eq!(encode_storage_project(source.document()).unwrap(), before);
}

#[cfg(feature = "history")]
#[test]
fn numeric_order_actual_fill_history_retains_old_pinned_sequences_and_source_records() {
    use crate::history::NativeObjectHistory;
    let mut value = document(&[7]);
    value["compositionOrder"]["contexts"][0]["layers"][2]["order"] = json!({"static":[2]});
    let mut owner = NativeObjectHistory::new(
        "order-history",
        decode_storage_project(&serde_json::to_vec(&value).unwrap()).unwrap(),
    )
    .unwrap();
    let zero = owner.acquire_snapshot(0).unwrap();
    let old = packet(&zero, 7);
    assert_eq!(layers(&old), vec!["L0", "L2", "L1"]);
    let candidate = owner
        .prepare_fill_json(
            &serde_json::to_vec(
                &json!({"command":"object.fill.set","stableTarget":value["objects"][0]["target"],
        "fill":{"kind":"solid","r":0,"g":1,"b":0,"a":1}}),
            )
            .unwrap(),
        )
        .unwrap();
    owner.commit_fill(candidate).unwrap();
    owner.undo(1).unwrap();
    owner.redo(2).unwrap();
    for rev in 0..=3 {
        assert_eq!(
            layers(&packet(&owner.acquire_snapshot(rev).unwrap(), 7)),
            vec!["L0", "L2", "L1"]
        );
    }
    assert_eq!(old.content_revision(), 0);
    assert_eq!(
        old.records().next().unwrap().record().target(),
        zero.document().objects()[0].target()
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&encode_storage_project(zero.document()).unwrap()).unwrap(),
        value
    );
}

#[cfg(feature = "object_expression_runtime")]
#[test]
fn real_owner_consumer_receives_frozen_held_and_nonlinear_raw_inputs() {
    use super::expression_consumer::{consume, fingerprint, owner};
    let fixed = frozen();
    for name in ["held", "nonlinear"] {
        let case = &fixed["animated"][name];
        for sample in case["samples"].as_array().unwrap() {
            let Some(frame) = sample["frame"].as_u64().map(|f| f as u32) else {
                continue;
            };
            let mut value = document(&[frame]);
            value["compositionOrder"]["contexts"][0]["layers"][0]["order"] = json!({
                "track":case["holder"]["motion"]["order"],
                "expression":{"enabled":true,"code":"[value]"}});
            let owner = owner(&value);
            let before = fingerprint(&owner);
            let packet = consume(&owner, &owner.acquire_snapshot(0).unwrap(), frame).unwrap();
            assert_eq!(
                rank(&packet, "L0"),
                sample["value"].as_f64().unwrap(),
                "{name}/{frame}"
            );
            assert_eq!(fingerprint(&owner), before);
        }
    }
}

#[cfg(feature = "object_expression_runtime")]
#[test]
fn real_owner_consumer_rejects_full_selector_foreign_stale_and_reference_pins() {
    use super::expression_consumer::{consume, document as active_document, fingerprint, owner};
    use crate::object_expression_runtime::{
        prepare_expression_order_frame_for_owner, ExpressionLimits, ExpressionRefusal,
    };
    let value = active_document("[4]");
    let mut original = owner(&value);
    let zero = original.acquire_snapshot(0).unwrap();
    let before = fingerprint(&original);
    let limits = ExpressionLimits::new(4096, 8_388_608, 262_144, 32).unwrap();
    for (field, error) in [
        ("instance", ObjectFrameError::WrongInstance),
        ("document", ObjectFrameError::WrongDocument),
        ("revision", ObjectFrameError::WrongRevision),
        ("snapshot", ObjectFrameError::WrongSnapshot),
        ("reference", ObjectFrameError::Unavailable),
    ] {
        let mut selected = selector(&zero, 7);
        match field {
            "instance" => selected.instance_id = "foreign",
            "document" => selected.document_id = "foreign",
            "revision" => selected.content_revision = 1,
            "snapshot" => selected.document_snapshot_id = "foreign",
            _ => selected.scope_kind = FrameScopeKind::Reference,
        }
        assert_eq!(
            prepare_expression_order_frame_for_owner(&original, &zero, &selected, limits)
                .unwrap_err(),
            ExpressionRefusal::Pin(error)
        );
    }
    assert_eq!(fingerprint(&original), before);
    let foreign = owner(&value);
    assert_eq!(
        consume(&foreign, &zero, 7).unwrap_err(),
        ExpressionRefusal::Pin(ObjectFrameError::WrongDocument)
    );
    let fill = original.prepare_fill_json(&serde_json::to_vec(&json!({"command":"object.fill.set",
        "stableTarget":value["objects"][0]["target"],"fill":{"kind":"solid","r":0.5,"g":0,"b":0,"a":1}})).unwrap()).unwrap();
    original.commit_fill(fill).unwrap();
    let advanced = fingerprint(&original);
    assert_eq!(
        consume(&original, &zero, 7).unwrap_err(),
        ExpressionRefusal::Pin(ObjectFrameError::WrongRevision)
    );
    assert_eq!(fingerprint(&original), advanced);
    assert!(consume(&original, &original.acquire_snapshot(1).unwrap(), 7).is_ok());
    assert_eq!(fingerprint(&original), advanced);
}
