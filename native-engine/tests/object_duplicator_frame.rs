//! Actual retained project -> production frame -> detached copy expansion.
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::object_document::{FrameScopeKind, ProjectEnvelope, ProjectMetadata};
use crate::object_frame_packet::{prepare_object_exposure_frame, ObjectFrameSelector};
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};

pub(crate) fn fixture(offset: u32) -> Value {
    crate::object_frame_packet::duplicator_sources::tests::duplicated_document(offset)
}
pub(crate) fn snapshot(value: Value) -> ObjectSnapshot {
    let document = decode_storage_project(value.to_string().as_bytes()).unwrap();
    let project = ProjectEnvelope::no_media(ProjectMetadata::new(100, 100, 24.).unwrap(), document);
    ObjectSnapshot::from_admitted_project("duplicator-production-frame".into(), project).unwrap()
}
pub(crate) fn selector(snapshot: &ObjectSnapshot, frame: u32) -> ObjectFrameSelector<'_> {
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

#[test]
fn production_frame_calls_real_temporal_sources_without_persisting_or_selecting_copies() {
    let snapshot = snapshot(fixture(1));
    let before = encode_storage_project(snapshot.document()).unwrap();
    let frame = prepare_object_exposure_frame(&snapshot, &selector(&snapshot, 2)).unwrap();
    assert_eq!(frame.records().len(), 1); // Original source provenance only.
    assert_eq!(frame.duplicator_outputs().len(), 1);
    let copies = &frame.duplicator_outputs()[0];
    assert_eq!(copies.source_indices, [1, 0, 0]);
    assert_eq!(
        copies
            .output
            .copies
            .iter()
            .map(|copy| copy.record.geometry.segments[0].point.x.as_f64().unwrap())
            .collect::<Vec<_>>(),
        [20., 20., 30.]
    );
    assert_eq!(snapshot.document().objects().len(), 3);
    assert_eq!(encode_storage_project(snapshot.document()).unwrap(), before);
    assert_eq!(snapshot.content_revision(), 0);
}

#[test]
fn original_stored_duplicator_properties_reach_the_actual_production_frame() {
    let mut value = fixture(1);
    value["propertyHolders"] = json!([{"target":{"contextId":"scene-root","layerUid":"L0"},
        "properties":[{"property":"dupOffsetPos","static":[2,0]}]}]);
    let snapshot = snapshot(value);
    let frame = prepare_object_exposure_frame(&snapshot, &selector(&snapshot, 2)).unwrap();
    assert_eq!(
        frame.duplicator_outputs()[0]
            .output
            .copies
            .iter()
            .map(|copy| copy.record.geometry.segments[0].point.x.as_f64().unwrap())
            .collect::<Vec<_>>(),
        [20., 22., 34.]
    );
}

#[test]
fn expression_backed_duplicate_properties_keep_the_existing_unavailable_admission() {
    let mut value = fixture(1);
    value["propertyHolders"] = json!([{"target":{"contextId":"scene-root","layerUid":"L0"},
        "properties":[{"property":"dupOffsetPos","static":[2,0],"expression":{"enabled":true,"code":"[9,9]"}}]}]);
    let snapshot = snapshot(value);
    assert!(matches!(
        prepare_object_exposure_frame(&snapshot, &selector(&snapshot, 2)),
        Err(crate::object_frame_packet::ObjectFrameError::Unavailable)
    ));
}

pub(crate) fn empty_host_pool() -> Value {
    let mut value = fixture(0);
    value["layers"][0]["visible"] = json!(false);
    value["layers"]
        .as_array_mut()
        .unwrap()
        .push(json!({"layerUid":"H0","timing":{
        "format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing",
        "contextId":"scene-root","keys":[{"frame":0}],"samples":[]}}));
    value["compositionOrder"]["contexts"][0]["layers"]
        .as_array_mut()
        .unwrap()
        .push(json!({"layerUid":"H0","naturalPosition":1,"order":{}}));
    let descriptor = &mut value["componentTables"]["duplicators"]["duplicators"][0];
    descriptor["layerUid"] = json!("H0");
    descriptor["duplicator"]["sourceMode"] = json!("objects");
    descriptor["duplicator"]["sourceLayerUids"] = json!(["L0"]);
    descriptor["duplicator"]["timeOffset"]["enabled"] = json!(false);
    for record in value["objects"].as_array_mut().unwrap() {
        record["geometry"]["segments"] = json!([
            {"point":{"x":-10,"y":0},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}},
            {"point":{"x":10,"y":0},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}},
            {"point":{"x":0,"y":10},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}}]);
    }
    value
}

#[test]
fn an_empty_host_uses_real_hidden_pool_content_and_never_adds_synthetic_source_records() {
    let snapshot = snapshot(empty_host_pool());
    let before = encode_storage_project(snapshot.document()).unwrap();
    let frame = prepare_object_exposure_frame(&snapshot, &selector(&snapshot, 2)).unwrap();
    assert_eq!(frame.records().len(), 0);
    let copies = &frame.duplicator_outputs()[0];
    assert_eq!(copies.layer_index, 1);
    assert_eq!(copies.source_indices, [1, 1, 1]);
    assert_eq!(
        copies
            .output
            .copies
            .iter()
            .map(|c| [
                c.record.geometry.segments[0].point.x.as_f64().unwrap(),
                c.record.geometry.segments[0].point.y.as_f64().unwrap()
            ])
            .collect::<Vec<_>>(),
        [[40., 45.], [50., 45.], [60., 45.]]
    );
    assert_eq!(encode_storage_project(snapshot.document()).unwrap(), before);
}

#[test]
fn a_hidden_pool_does_not_bypass_its_hosts_admitted_visible_time_window() {
    let mut value = empty_host_pool();
    value["layers"][1]["range"] = json!({"inPoint":0,"outPoint":1});
    let snapshot = snapshot(value);
    let frame = prepare_object_exposure_frame(&snapshot, &selector(&snapshot, 2)).unwrap();
    assert_eq!(frame.source_frame_for_layer(1), None);
    assert!(frame.duplicator_outputs().is_empty());
    assert_eq!(frame.records().len(), 0);
}

#[test]
fn real_effector_layers_sample_stored_offsets_and_hiding_disables_their_contribution() {
    let mut value = fixture(1);
    value["layers"]
        .as_array_mut()
        .unwrap()
        .push(json!({"layerUid":"E0","timing":{
        "format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing",
        "contextId":"scene-root","keys":[{"frame":0}],"samples":[]}}));
    value["compositionOrder"]["contexts"][0]["layers"]
        .as_array_mut()
        .unwrap()
        .push(json!({"layerUid":"E0","naturalPosition":1,"order":{}}));
    value["componentTables"]["duplicators"]["effectorLayers"] = json!([{
        "contextId":"scene-root","layerUid":"E0",
        "effector":{"targetLayerUid":"L0","falloff":"step"}}]);
    value["propertyHolders"] = json!([{"target":{"contextId":"scene-root","layerUid":"E0"},
        "properties":[{"property":"effOffPosition","track":{"keys":[
            {"frame":0,"v":[0,0],"curvePoints":[]},{"frame":4,"v":[8,-4]}]}}]}]);
    for visible in [true, false] {
        value["layers"][1]["visible"] = json!(visible);
        let snapshot = snapshot(value.clone());
        let before = encode_storage_project(snapshot.document()).unwrap();
        let frame = prepare_object_exposure_frame(&snapshot, &selector(&snapshot, 2)).unwrap();
        let copies = &frame.duplicator_outputs()[0];
        let starts = copies
            .output
            .copies
            .iter()
            .map(|copy| {
                let p = &copy.record.geometry.segments[0].point;
                [p.x.as_f64().unwrap(), p.y.as_f64().unwrap()]
            })
            .collect::<Vec<_>>();
        // effector-layer.js step weights0,1/2,1; at frame2 stored offset is[4,-2].
        let expected = if visible {
            [[20., 20.], [22., 19.], [34., 18.]]
        } else {
            [[20., 20.], [20., 20.], [30., 20.]]
        };
        assert_eq!(starts, expected);
        assert_eq!(copies.source_indices, [1, 0, 0]);
        assert_eq!(frame.records().len(), 1);
        assert_eq!(encode_storage_project(snapshot.document()).unwrap(), before);
        assert_eq!(snapshot.content_revision(), 0);
    }
}

// Real retained versions of the original oracle square, path and dupOffset*
// inputs. No oracle World/answer-table provider participates in these tests.
pub(crate) fn retained_mode_document(name: &str) -> Value {
    let mut value = fixture(0);
    value["layers"][0]["locked"] = json!(true);
    for record in value["objects"].as_array_mut().unwrap() {
        record["geometry"]["segments"] = json!([[100, 100], [140, 100], [140, 140], [100, 140]]
            .map(|[x, y]| json!({"point":{"x":x,"y":y},
                "handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}})));
        record["fill"] = json!({"kind":"solid","r":1,"g":0,"b":0,"a":1});
    }
    let descriptor = match name {
        "radial" => json!({"mode":"radial","count":6,"radius":100,"startAngle":0,"seed":21}),
        "path" | "tangent" | "chain" => json!({"mode":"path","count":5,
            "pathLayerUid":"P0","seed":31,"pathAlignTangent":name == "tangent"}),
        "random42" => json!({"mode":"grid","rows":2,"cols":3,"spacingX":60,"spacingY":50,
            "seed":42,"staggerRandom":{"position":true,"rotation":true,"scale":true,"opacity":true,"hue":true}}),
        "random77" | "random77more" | "random78" => json!({"mode":"grid","rows":1,
            "cols":if name == "random77more" {7} else {6},"spacingX":60,"spacingY":50,
            "seed":if name == "random78" {78} else {77},
            "staggerRandom":{"position":true,"rotation":true,"scale":true,"opacity":true,"hue":true}}),
        _ => panic!("unknown original mode fixture: {name}"),
    };
    value["componentTables"]["duplicators"]["duplicators"][0]["duplicator"] = descriptor;
    if name.starts_with("random") {
        value["propertyHolders"] = json!([{"target":{"contextId":"scene-root","layerUid":"L0"},
            "properties":[{"property":"dupOffsetPos","static":[12,-7]},
                {"property":"dupOffsetRot","static":[9]},
                {"property":"dupOffsetScale","static":[6,-4]},
                {"property":"dupOffsetOpacity","static":[-12]},
                {"property":"dupOffsetHue","static":[25]},
                {"property":"dupOffsetPosZ","static":[30]},
                {"property":"dupOffsetRotX","static":[5]},
                {"property":"dupOffsetRotY","static":[-8]}]}]);
    }
    if matches!(name, "path" | "tangent" | "chain") {
        value["layers"]
            .as_array_mut()
            .unwrap()
            .push(json!({"layerUid":"P0","visible":false,
            "timing":{"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing",
                "contextId":"scene-root","keys":[{"frame":0}],"samples":[]}}));
        value["compositionOrder"]["contexts"][0]["layers"]
            .as_array_mut()
            .unwrap()
            .push(json!({"layerUid":"P0","naturalPosition":1,"order":{}}));
        value["compositionOrder"]["contexts"][0]["scopes"][0]["elements"]
            .as_array_mut()
            .unwrap()
            .push(json!({"layerUid":"P0","strokeId":"path","naturalPosition":0,"order":{}}));
        let mut path = value["objects"][0].clone();
        path["family"] = json!("editable-vector-path");
        path["target"]["layerUid"] = json!("P0");
        path["target"]["strokeId"] = json!("path");
        path["geometry"]["closed"] = json!(false);
        path["geometry"]["segments"] = json!([
            [0, 0, 0, 0, 40, 0],
            [200, 50, -40, 0, 40, 0],
            [400, 0, -40, 0, 0, 0]
        ]
        .map(|[x, y, ix, iy, ox, oy]| json!({"point":{"x":x,"y":y},
                "handleIn":{"x":ix,"y":iy},"handleOut":{"x":ox,"y":oy}})));
        path["fill"] = Value::Null;
        path["vector"] = json!({"stroke":{"kind":"solid","r":0,"g":0,"b":0,"a":1},
            "hasRealStroke":true,"strokeWidth":1,"strokeCap":"round","strokeJoin":"round","opacity":1});
        value["objects"].as_array_mut().unwrap().push(path);
        if name == "chain" {
            value["componentTables"]["duplicators"]["duplicators"]
                .as_array_mut()
                .unwrap()
                .push(json!({"contextId":"scene-root","layerUid":"P0",
                    "duplicator":{"mode":"grid","rows":1,"cols":2,"spacingX":60}}));
        }
    }
    value
}

pub(crate) fn assert_original_number(got: f64, expected: f64) {
    assert!(
        got.is_finite() && (got - expected).abs() <= 1e-9 * expected.abs().max(1.),
        "original literal: {got:?} vs {expected:?}"
    );
}

// First anchors transcribed verbatim from object-duplicator-evaluate/oracle.txt
// out rows 1240, 1693, 1726, 640 and 820 respectively; V8/libm tolerance above.
pub(crate) fn original_mode_starts(name: &str) -> Vec<[f64; 2]> {
    let bits: &[[u64; 2]] = match name {
        "radial" => &[
            [0x4069000000000000, 0x4059000000000000],
            [0x4062c00000000000, 0x4067534802c27df2],
            [0x4049000000000004, 0x4067534802c27df3],
            [0x0000000000000000, 0x4059000000000001],
            [0x4048fffffffffffa, 0x402acb7fd3d820e0],
            [0x4062c00000000000, 0x402acb7fd3d820d8],
        ],
        "path" => &[
            [0x4059000000000000, 0x4059000000000000],
            [0x406900000000009c, 0x405f400000000062],
            [0x4072c00000000000, 0x4062c00000000000],
            [0x407900000000004e, 0x405f3fffffffff9e],
            [0x407f400000000000, 0x4059000000000000],
        ],
        "tangent" => &[
            [0x4059000000000000, 0x4059000000000000],
            [0x4069dc076011f7d2, 0x405dfc7940449900],
            [0x4072c00000000000, 0x4062c00000000000],
            [0x4078af1e50112675, 0x40607c076011f705],
            [0x407f400000000000, 0x4059000000000000],
        ],
        "random42" => &[
            [0x405a1a5922412566, 0x405884bfd1607a8e],
            [0x40638cabd03551ea, 0x4057bd16ef6b7a36],
            [0x4069ed0fc6d1aaa4, 0x4058990a4b179e4f],
            [0x40571b77befae8ce, 0x4062bf36029fad7c],
            [0x4063b9f626285f16, 0x4063096c69a9b61a],
            [0x406a6bf02c16c10e, 0x40624d5fe36b8486],
        ],
        "random77" => &[
            [0x4058e38085b6283d, 0x405a26b099512762],
            [0x40634b33c22ba6de, 0x405b149fb8e9e7e7],
            [0x406c92a05e69fa5c, 0x40578fe413c293a5],
            [0x40713661aff6ab02, 0x40596fe3e224014e],
            [0x4074c2e41719d56e, 0x405a6a2a8eb35423],
            [0x4078391971cba1a1, 0x40596375e28be7b2],
        ],
        _ => panic!("no original literal table for {name}"),
    };
    bits.iter()
        .map(|[x, y]| [f64::from_bits(*x), f64::from_bits(*y)])
        .collect()
}

#[test]
fn retained_radial_and_path_modes_reach_real_copies_with_original_literal_placements() {
    for name in ["radial", "path", "tangent"] {
        let snapshot = snapshot(retained_mode_document(name));
        let before = encode_storage_project(snapshot.document()).unwrap();
        let packet = prepare_object_exposure_frame(&snapshot, &selector(&snapshot, 2)).unwrap();
        assert_eq!(packet.records().len(), 1);
        assert_eq!(packet.duplicator_outputs().len(), 1);
        let output = &packet.duplicator_outputs()[0];
        let expected = original_mode_starts(name);
        assert_eq!(output.source_indices, vec![1; expected.len()]);
        assert_eq!(output.output.copies.len(), expected.len());
        let h = 86.60254037844386;
        let moves = if name == "radial" {
            vec![
                [100., 0., 0.],
                [50., h, 0.],
                [-50., h, 0.],
                [-100., 0., 0.],
                [-50., -h, 0.],
                [50., -h, 0.],
            ]
        } else {
            let a = if name == "tangent" {
                17.354024636261325
            } else {
                0.
            };
            vec![
                [0., 0., 0.],
                [100., 25., a],
                [200., 50., 0.],
                [300., 25., -a],
                [400., 0., 0.],
            ]
        };
        for (k, ((copy, point), trace)) in output
            .output
            .copies
            .iter()
            .zip(expected)
            .zip(&output.output.placements)
            .enumerate()
        {
            assert_eq!(copy.dup_index, k as u32);
            assert_eq!(
                copy.clone_id,
                format!("dup{}_{}", if name == "radial" { 21 } else { 31 }, k)
            );
            assert_eq!(
                copy.record.target(),
                snapshot.document().objects()[1].target()
            );
            assert_eq!([trace.pivot.x, trace.pivot.y], [120., 120.]);
            let matrix = trace.placement.matrix;
            for (got, want) in [matrix.dx, matrix.dy, matrix.rot].into_iter().zip(moves[k]) {
                assert_original_number(got, want);
            }
            let p = &copy.record.geometry.segments[0].point;
            assert_original_number(p.x.as_f64().unwrap(), point[0]);
            assert_original_number(p.y.as_f64().unwrap(), point[1]);
        }
        let mut detached = output.output.clone();
        detached.copies[0].record.geometry.segments[0].point.x = 999.into();
        let again = prepare_object_exposure_frame(&snapshot, &selector(&snapshot, 2)).unwrap();
        assert_eq!(again.duplicator_outputs()[0].output, output.output);
        assert_eq!(encode_storage_project(snapshot.document()).unwrap(), before);
        assert_eq!(snapshot.content_revision(), 0);
    }
}

#[test]
fn retained_random_is_pinned_across_reconstruction_seed_changes_and_count_growth() {
    let evaluate = |name| {
        let original = snapshot(retained_mode_document(name));
        let stored = encode_storage_project(original.document()).unwrap();
        // New admission has no inherited RNG or mutable evaluator state.
        let reopened = snapshot(serde_json::from_slice(&stored).unwrap());
        let packet = prepare_object_exposure_frame(&reopened, &selector(&reopened, 2)).unwrap();
        assert_eq!(packet.records().len(), 1);
        let output = packet.duplicator_outputs()[0].output.clone();
        assert_eq!(
            packet.duplicator_outputs()[0].source_indices,
            vec![1; output.copies.len()]
        );
        assert_eq!(encode_storage_project(reopened.document()).unwrap(), stored);
        assert_eq!(reopened.document().objects().len(), 3);
        assert_eq!(reopened.content_revision(), 0);
        output
    };
    for name in ["random42", "random77"] {
        let output = evaluate(name);
        assert_eq!(output, evaluate(name));
        for (k, (copy, point)) in output
            .copies
            .iter()
            .zip(original_mode_starts(name))
            .enumerate()
        {
            assert_eq!(copy.dup_index, k as u32);
            assert_eq!(
                copy.clone_id,
                format!("dup{}_{}", if name == "random42" { 42 } else { 77 }, k)
            );
            let p = &copy.record.geometry.segments[0].point;
            assert_original_number(p.x.as_f64().unwrap(), point[0]);
            assert_original_number(p.y.as_f64().unwrap(), point[1]);
        }
    }
    let six = evaluate("random77");
    let seven = evaluate("random77more");
    assert_eq!((six.copies.len(), seven.copies.len()), (6, 7));
    assert_eq!(&seven.copies[..6], six.copies.as_slice());
    assert_eq!(&seven.placements[..6], six.placements.as_slice());
    let changed = evaluate("random78");
    for k in 0..6 {
        assert_ne!(six.placements[k].placement, changed.placements[k].placement);
        assert_ne!(
            six.copies[k].record.geometry,
            changed.copies[k].record.geometry
        );
    }
    // Original first-three placement channels: pins.rs seed42 table.
    let seeded = evaluate("random42");
    let dx = [2.426490046083927, 57.564577287063, 110.33984452486038];
    let dy = [0.7239321740344167, -6.319422932341695, -5.597637253347784];
    let rot = [6.344384282827377, -2.150093929376453, -8.723436305299401];
    let sx = [
        f64::from_bits(0x3ff0536d7c2d70a4),
        f64::from_bits(0x3ff05c65225ae148),
        f64::from_bits(0x3ff030c188d7ae14),
    ];
    let sy = [
        f64::from_bits(0x3ff06a8e9659999a),
        f64::from_bits(0x3fef3617798ccccd),
        f64::from_bits(0x3feee2d8ef8ccccd),
    ];
    let opacity = [0.9936177898757159, 0.9768035340867937, 1.];
    let hue = [
        -1.3841472449712455,
        -0.24045639438554645,
        14.336772030219436,
    ];
    for k in 0..3 {
        let p = seeded.placements[k].placement;
        for (got, want) in [
            p.matrix.dx,
            p.matrix.dy,
            p.matrix.rot,
            p.matrix.sx,
            p.matrix.sy,
            p.opacity_factor,
            p.hue_deg,
        ]
        .into_iter()
        .zip([dx[k], dy[k], rot[k], sx[k], sy[k], opacity[k], hue[k]])
        {
            assert!(
                got.is_finite() && (got - want).abs() <= 1e-12 * want.abs().max(1.),
                "original seed42 channel at index {k}: {got} vs {want}"
            );
        }
    }
}

#[test]
fn retained_path_layer_that_is_a_duplicator_keeps_the_original_no_chain_fallback() {
    let admitted = snapshot(retained_mode_document("chain"));
    let before = encode_storage_project(admitted.document()).unwrap();
    let snapshot = snapshot(serde_json::from_slice(&before).unwrap());
    let packet = prepare_object_exposure_frame(&snapshot, &selector(&snapshot, 2)).unwrap();
    assert!(packet.duplicator_outputs().is_empty()); // Original oracle: out same.
    let records = packet.records().collect::<Vec<_>>();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].record(), &snapshot.document().objects()[1]);
    assert_eq!(snapshot.document().objects().len(), 4);
    assert_eq!(encode_storage_project(snapshot.document()).unwrap(), before);
    assert_eq!(snapshot.content_revision(), 0);
}
