//! Layer rotation / scale / anchor (static and keyed, with a parent) through the real owner -> frame -> scene -> plan
//! chain, against geometry, stroke width and gradient endpoints computed by the frozen baseline (`oracle.json`, made by
//! `fixtures/layer-transform-keys/gen.cjs` from src/js/motion.js at 1065e9d8 and Paper bounds, never from Rust).
use super::{owner, packet, pins, plan};
use crate::object_frame_packet::{prepare_object_exposure_frame, ObjectFrameError};
use serde_json::{json, Value};

const ORACLE: &str = include_str!("fixtures/layer-transform-keys/oracle.json");

fn point(v: &Value) -> Value {
    json!({"x":v[0],"y":v[1]})
}
fn cubic(segments: &Value) -> Value {
    Value::Array(
        segments
            .as_array()
            .unwrap()
            .iter()
            .map(|s| json!({"point":point(&s["point"]),"handleIn":point(&s["handleIn"]),"handleOut":point(&s["handleOut"])}))
            .collect(),
    )
}
fn record(layer: &str, id: &str, source: &Value, stroke: bool, gradient: bool) -> Value {
    let mut record = json!({"schemaVersion":1,"family":"editable-vector-path",
        "target":{"contextId":"scene-root","frameScope":{"kind":"authored","frame":0},"layerUid":layer,"strokeId":id},
        "geometry":{"kind":"cubic-path","closed":true,"coordinateSpace":"document-world","handleSpace":"relative",
            "segments":cubic(&source["segments"])},
        "fill":{"kind":"solid","r":0.25,"g":0.5,"b":0.75,"a":1},
        "vector":{"stroke":null,"hasRealStroke":false,"strokeWidth":3,"strokeCap":"round","strokeJoin":"round","opacity":1}});
    if stroke {
        record["vector"]["stroke"] = json!({"kind":"solid","r":0,"g":0,"b":0,"a":1});
        record["vector"]["hasRealStroke"] = json!(true);
        record["vector"]["strokeWidth"] = source["strokeWidth"].clone();
    }
    if gradient {
        record["vectorExt"] = json!({"fillGradient":{"kind":"linear","from":source["gradient"]["from"],
            "to":source["gradient"]["to"],"stops":[{"offset":0,"color":"#000000"},{"offset":1,"color":"#ffffff"}]}});
    }
    record
}
fn holder(layer: &str, pose: &Value) -> Value {
    let properties: Vec<Value> = pose
        .as_object()
        .unwrap()
        .iter()
        .map(|(name, value)| match value.get("track") {
            Some(track) => json!({"property":name,"track":track}),
            None => json!({"property":name,"static":value["static"]}),
        })
        .collect();
    json!({"target":{"contextId":"scene-root","layerUid":layer},"properties":properties})
}
fn document(oracle: &Value, case: &Value) -> Value {
    let timing = json!({"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing",
        "contextId":"scene-root","keys":[{"frame":0}],"samples":[]});
    let parented = !case["parent"].is_null();
    let mut layers = vec![json!({"layerUid":"L1","timing":timing.clone()})];
    let mut order = vec![json!({"layerUid":"L1","naturalPosition":0,"order":{}})];
    let mut objects = vec![
        record("L1", "a", &oracle["records"]["a"], true, false),
        record("L1", "b", &oracle["records"]["b"], false, true),
    ];
    let mut elements = vec![
        json!({"layerUid":"L1","strokeId":"a","naturalPosition":0,"order":{}}),
        json!({"layerUid":"L1","strokeId":"b","naturalPosition":1,"order":{}}),
    ];
    let mut holders = vec![holder("L1", &case["layer"])];
    if parented {
        layers[0]["parentLayerUid"] = json!("P");
        layers.push(json!({"layerUid":"P","visible":false,"timing":timing}));
        order.push(json!({"layerUid":"P","naturalPosition":1,"order":{}}));
        objects.push(record("P", "p", &oracle["parentRecord"], false, false));
        elements.push(json!({"layerUid":"P","strokeId":"p","naturalPosition":0,"order":{}}));
        holders.push(holder("P", &case["parent"]));
    }
    let mut items = Value::Null;
    if !case["raster"].is_null() {
        let r = &case["raster"];
        items = json!({"raster":[{"target":{"contextId":"scene-root","layerUid":"L1","strokeId":"img",
            "frameScope":{"kind":"authored","frame":0}},
            "item":{"assetId":"ink","x":r["x"],"y":r["y"],"width":r["width"],"height":r["height"],"opacity":1}}]});
        elements.push(json!({"layerUid":"L1","strokeId":"img","naturalPosition":2,"order":{}}));
    }
    let mut document = json!({"format":"nemo.native-object-document","formatVersion":2,"totalFrames":16,"layers":layers,
        "objects":objects,"propertyHolders":holders,
        "compositionOrder":{"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
            "migration":"explicit-plan-required","referencePolicy":"unresolved","contexts":[{"contextId":"scene-root",
                "layers":order,"scopes":[{"frameScope":{"kind":"authored","frame":0},"elements":elements}]}]}});
    if !items.is_null() {
        document["items"] = items;
    }
    document
}
/// A document with a raster item needs its asset in the project envelope; every other case uses the shared owner.
fn open(drawing: &Value) -> crate::history::NativeObjectHistory {
    if drawing.get("items").is_none() {
        return owner(drawing);
    }
    let project = json!({"format":"nemo.native-project-document","formatVersion":2,
        "project":{"canvasW":400,"canvasH":200,"fps":24},"viewportReference":null,
        "assets":[{"id":"ink","name":"Authored ink","kind":"image","source":{"kind":"embedded","mediaType":"image/png",
            "encoding":"base64","data":"iVBORw0KGgoAAAANSUhEUgAAAAIAAAABCAYAAAD0In+KAAAADUlEQVR4nGNgYPgPBgAU9AX7uVYDfQAAAABJRU5ErkJggg=="}}],
        "assetInventory":{"assetIds":["ink"],"placements":[]},"drawing":drawing});
    crate::history::NativeObjectHistory::from_project_file(
        "layer-transform",
        project.to_string().as_bytes(),
    )
    .unwrap()
}
fn near(actual: f64, expected: f64, what: &str) {
    assert!(
        (actual - expected).abs() <= 1e-9,
        "{what}: {actual} != {expected}"
    );
}
fn pair(v: &Value) -> [f64; 2] {
    [v[0].as_f64().unwrap(), v[1].as_f64().unwrap()]
}

/// Every drawn record, its stroke width and gradient endpoints against the oracle frame.
fn check(plan: &crate::object_render_scene::ObjectPaintPlan, frame: &Value, label: &str) {
    assert_eq!(
        plan.commands().len(),
        2,
        "{label}: the hidden parent must not paint"
    );
    for command in plan.commands() {
        let id = command.source().record().target.stroke_id.clone();
        let expected = frame["segments"][id.as_str()].as_array().unwrap();
        let actual = command.contour().segments();
        assert_eq!(actual.len(), expected.len(), "{label} {id}");
        for (n, (got, want)) in actual.iter().zip(expected).enumerate() {
            for (name, g, w) in [
                ("point", got.point, pair(&want["point"])),
                ("handleIn", got.handle_in, pair(&want["handleIn"])),
                ("handleOut", got.handle_out, pair(&want["handleOut"])),
            ] {
                near(g[0], w[0], &format!("{label} {id}[{n}].{name}.x"));
                near(g[1], w[1], &format!("{label} {id}[{n}].{name}.y"));
            }
        }
        let style = command.style().unwrap();
        if id == "a" {
            let width = style.stroke.as_ref().unwrap().stroke.width;
            near(
                width,
                frame["strokeWidth"]["a"].as_f64().unwrap(),
                &format!("{label} stroke width"),
            );
        } else {
            use crate::evaluation::object_scene_style_kernels::object_scene_style_paint::GradientGeometry;
            let Some(GradientGeometry::Linear { from, to }) =
                style.fill.gradient().map(|g| g.geometry)
            else {
                panic!("{label}: the gradient fill is missing");
            };
            for (name, got, want) in [
                ("from", from, &frame["gradient"]["from"]),
                ("to", to, &frame["gradient"]["to"]),
            ] {
                near(
                    got[0],
                    want[0].as_f64().unwrap(),
                    &format!("{label} gradient {name}.x"),
                );
                near(
                    got[1],
                    want[1].as_f64().unwrap(),
                    &format!("{label} gradient {name}.y"),
                );
            }
        }
    }
}

#[test]
fn layer_rotation_scale_anchor_match_the_frozen_baseline_through_the_real_scene_and_paint_plan() {
    let oracle: Value = serde_json::from_str(ORACLE).unwrap();
    assert_eq!(oracle["baseline"], "1065e9d8");
    for case in oracle["cases"].as_array().unwrap() {
        let drawing = document(&oracle, case);
        let o = open(&drawing);
        let s = o.acquire_snapshot(0).unwrap();
        for frame in case["frames"].as_array().unwrap() {
            let label = format!("{} @{}", case["name"].as_str().unwrap(), frame["frame"]);
            let evaluated = packet(&o, &s, frame["frame"].as_u64().unwrap() as u32);
            check(&plan(&evaluated), frame, &label);
        }
        assert_eq!((o.content_revision(), o.history_depths()), (0, (0, 0)));
    }
}

#[test]
fn the_raw_frame_route_admits_the_same_static_and_keyed_keys_and_still_refuses_an_expression() {
    let oracle: Value = serde_json::from_str(ORACLE).unwrap();
    for case in oracle["cases"].as_array().unwrap() {
        let drawing = document(&oracle, case);
        let o = open(&drawing);
        let s = o.acquire_snapshot(0).unwrap();
        for frame in case["frames"].as_array().unwrap() {
            let label = format!("raw {} @{}", case["name"].as_str().unwrap(), frame["frame"]);
            let selector = pins(&s, frame["frame"].as_u64().unwrap() as u32);
            let raw = prepare_object_exposure_frame(&s, &selector)
                .unwrap_or_else(|error| panic!("{label}: {error:?}"));
            let scene =
                crate::object_render_scene::prepare_object_render_scene(&raw, &selector).unwrap();
            let prepared =
                crate::object_render_scene::prepare_object_paint_plan(&scene, &selector).unwrap();
            check(&prepared, frame, &label);
        }
    }
    for property in ["rotation", "scale", "anchor"] {
        let case = oracle["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == "combined")
            .unwrap();
        let mut drawing = document(&oracle, case);
        let live = drawing["propertyHolders"][0]["properties"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|p| p["property"] == property)
            .unwrap();
        live["expression"] = json!({"enabled":true,"code":"[0,0]"});
        let o = open(&drawing);
        let s = o.acquire_snapshot(0).unwrap();
        assert_eq!(
            prepare_object_exposure_frame(&s, &pins(&s, 0)).unwrap_err(),
            ObjectFrameError::Unavailable,
            "{property}"
        );
    }
}
