//! Stored baseline two-stop gradients: actual whole-record owner, history, file and shared scene consumer.
//! Source-authored only. Metal is mandatory, never replaced by a stand-in or skipped on adapter failure.
use crate::evaluation::object_scene_style_kernels::object_scene_style_paint::GradientGeometry;
use crate::history::NativeObjectHistory;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::object_document::FrameScopeKind;
use crate::object_frame_packet::{prepare_object_exposure_frame, ObjectFrameSelector};
use crate::object_render_scene::{
    prepare_object_paint_plan, prepare_object_render_scene, ObjectPaintPlan,
};
use crate::request_receipts::DispatchErrorCode;
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};

const ORACLE: &str = include_str!("fixtures/object-scene-style-emit/oracle.txt");
const IDS: [&str; 4] = [
    "gradient-linear",
    "gradient-radial",
    "gradient-linear-with-opacity",
    "gradient-then-stroke-first-order-keeps-gradient-above-stroke",
];
fn rows() -> Vec<Value> {
    IDS.iter()
        .map(|id| {
            ORACLE
                .lines()
                .filter_map(|s| serde_json::from_str::<Value>(s).ok())
                .find(|v| v["id"] == *id)
                .expect("original frozen gradient row")
        })
        .collect()
}
fn gradient_record(row: &Value) -> Value {
    let item = &row["items"][0];
    let mut record = item["record"].clone();
    let g = &item["gradient"];
    record["target"]["strokeId"] = json!("");
    record["vector"]["opacity"] = g["opacity"].clone();
    record["vectorExt"] =
        json!({"fillGradient":{"kind":g["kind"],"from":g["from"],"to":g["to"],"stops":g["stops"]}});
    record
}
fn drawing(objects: Vec<Value>) -> Value {
    let elements: Vec<_> = objects
        .iter()
        .enumerate()
        .map(|(i, r)| {
            json!({"layerUid":"L1",
        "strokeId":r["target"]["strokeId"],"naturalPosition":i,"order":{}})
        })
        .collect();
    let scopes = if elements.is_empty() {
        json!([])
    } else {
        json!([{"frameScope":{"kind":"authored","frame":0},"elements":elements}])
    };
    json!({"format":"nemo.native-object-document","formatVersion":2,"totalFrames":16,
        "layers":[{"layerUid":"L1","timing":{"format":"nemo.native-layer-timing","formatVersion":1,
            "family":"drawing","contextId":"scene-root","keys":[{"frame":0}],"samples":[]}}],"objects":objects,
        "compositionOrder":{"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
            "migration":"explicit-plan-required","referencePolicy":"unresolved","contexts":[{"contextId":"scene-root",
                "layers":[{"layerUid":"L1","naturalPosition":0,"order":{}}],"scopes":scopes}]}})
}
fn owner(doc: &Value) -> NativeObjectHistory {
    NativeObjectHistory::from_project_file(
        "stored-gradient",
        &serde_json::to_vec(&json!({
        "format":"nemo.native-project-document","formatVersion":1,
        "project":{"canvasW":32,"canvasH":32,"fps":24},"drawing":doc,
        "viewportReference":null,"assets":[]}))
        .unwrap(),
    )
    .unwrap()
}
fn snapshot(o: &NativeObjectHistory) -> ObjectSnapshot {
    o.acquire_snapshot(o.content_revision()).unwrap()
}
fn document(s: &ObjectSnapshot) -> Value {
    serde_json::from_slice(&encode_storage_project(s.document()).unwrap()).unwrap()
}
fn saved(o: &NativeObjectHistory) -> Vec<u8> {
    let s = snapshot(o);
    o.serialize_project_file(s.content_revision(), s.snapshot_id())
        .unwrap()
}
fn edit(o: &mut NativeObjectHistory, command: &str, record: Value) -> u64 {
    let candidate = o
        .prepare_vector_edit_json(
            &serde_json::to_vec(&json!({"command":command,"record":record})).unwrap(),
        )
        .unwrap();
    o.commit_vector_edit(candidate).unwrap()
}
fn pins(s: &ObjectSnapshot, frame: u32) -> ObjectFrameSelector<'_> {
    ObjectFrameSelector {
        instance_id: s.instance_id(),
        document_id: s.document_id(),
        content_revision: s.content_revision(),
        document_snapshot_id: s.snapshot_id(),
        context_id: "scene-root",
        scope_kind: FrameScopeKind::Authored,
        frame,
    }
}
fn plan(s: &ObjectSnapshot, frame: u32) -> ObjectPaintPlan {
    let pin = pins(s, frame);
    let packet = prepare_object_exposure_frame(s, &pin).unwrap();
    let scene = prepare_object_render_scene(&packet, &pin).unwrap();
    prepare_object_paint_plan(&scene, &pin).unwrap()
}

#[test]
fn stored_gradient_whole_record_create_replace_clear_history_and_project_reopen() {
    for row in rows() {
        let mut o = owner(&drawing(vec![]));
        let empty = saved(&o);
        let input = gradient_record(&row);
        assert_eq!(edit(&mut o, "object.vector.create", input.clone()), 1);
        let old = snapshot(&o);
        let old_bytes = encode_storage_project(old.document()).unwrap();
        let stored = document(&old)["objects"][0].clone();
        assert_eq!(stored["vectorExt"], input["vectorExt"]);
        assert_eq!(stored["target"]["strokeId"], "native-stroke:1:0");
        assert_eq!(stored["geometry"], input["geometry"]);
        let request = json!({"apiVersion":2,"requestId":"gradient-read","instanceId":old.instance_id(),
            "documentId":old.document_id(),"operation":"query.document.object",
            "payload":{"atRevision":1,"stableTarget":stored["target"]}});
        let read = old.query_json(request.to_string().as_bytes()).unwrap();
        assert_eq!(
            serde_json::to_value(read).unwrap()["result"]["object"],
            stored
        );
        let project = saved(&o);
        let reopened = NativeObjectHistory::from_project_file("reopened", &project).unwrap();
        assert_eq!(document(&snapshot(&reopened)), document(&old));
        assert_eq!(o.history_depths(), (1, 0));
        assert_eq!(o.undo(1), Ok(2));
        assert!(document(&snapshot(&o))["objects"]
            .as_array()
            .unwrap()
            .is_empty());
        assert_eq!(o.redo(2), Ok(3));
        assert_eq!(document(&snapshot(&o))["objects"][0], stored);
        // Whole-record omission is the explicit baseline gradient-off operation, not an implicit fill edit.
        let mut plain = stored.clone();
        plain.as_object_mut().unwrap().remove("vectorExt");
        plain["fill"] = json!({"kind":"solid","r":1,"g":0,"b":0,"a":1});
        assert_eq!(edit(&mut o, "object.vector.replace", plain.clone()), 4);
        assert_eq!(document(&snapshot(&o))["objects"][0], plain);
        assert!(plan(&snapshot(&o), 0)
            .commands()
            .next()
            .unwrap()
            .style()
            .unwrap()
            .fill
            .solid()
            .is_some());
        assert_eq!(o.undo(4), Ok(5));
        assert_eq!(document(&snapshot(&o))["objects"][0], stored);
        assert_eq!(encode_storage_project(old.document()).unwrap(), old_bytes);
        assert_ne!(saved(&o), empty);
    }
}

#[test]
fn stored_gradient_invalid_wire_refuses_before_history_or_source_mutation() {
    let row = &rows()[0];
    let good = gradient_record(row);
    let o = owner(&drawing(vec![]));
    let before = saved(&o);
    let mut bads = Vec::new();
    for key in ["kind", "from", "to", "stops"] {
        let mut bad = good.clone();
        bad["vectorExt"]["fillGradient"]
            .as_object_mut()
            .unwrap()
            .remove(key);
        bads.push(bad);
    }
    for g in [
        Value::Null,
        json!({"kind":"linear","from":[0,0],"to":[32,0],"stops":[]}),
        json!({"kind":"conic","from":[0,0],"to":[32,0],"stops":[{"offset":0,"color":"#000000"},{"offset":1,"color":"#ffffff"}]}),
        json!({"kind":"linear","from":[0,0],"to":[32,0],"stops":[{"offset":0,"color":"#000000"},{"offset":0.5,"color":"#ff0000"},{"offset":1,"color":"#ffffff"}]}),
    ] {
        let mut bad = good.clone();
        bad["vectorExt"]["fillGradient"] = g;
        bads.push(bad);
    }
    let mut extra = good.clone();
    extra["vectorExt"]["fillGradient"]["stops"][0]["ignored"] = json!(true);
    bads.push(extra);
    let mut css = good.clone();
    css["vectorExt"]["fillGradient"]["stops"][0]["color"] = json!("url(paint)");
    bads.push(css);
    let mut huge = good.clone();
    huge["vectorExt"]["fillGradient"]["to"] = json!([1e300, 0]);
    bads.push(huge);
    let mut pressure = good.clone();
    pressure["vectorExt"]["pressure"] = json!({});
    bads.push(pressure);
    for bad in bads {
        let raw = json!({"command":"object.vector.create","record":bad}).to_string();
        assert_eq!(
            o.prepare_vector_edit_json(raw.as_bytes()).unwrap_err(),
            DispatchErrorCode::InvalidRequest
        );
        assert_eq!(saved(&o), before);
        assert_eq!(o.history_depths(), (0, 0));
    }
    let raw = json!({"command":"object.vector.create","record":good}).to_string();
    let duplicate = raw.replace(
        "\"kind\":\"linear\"",
        "\"kind\":\"linear\",\"kind\":\"radial\"",
    );
    assert_ne!(duplicate, raw);
    assert_eq!(
        o.prepare_vector_edit_json(duplicate.as_bytes())
            .unwrap_err(),
        DispatchErrorCode::InvalidRequest
    );
    assert_eq!(saved(&o), before);
    let mut stable = good.clone();
    stable["target"]["strokeId"] = json!("g");
    let mut legacy = stable.clone();
    legacy["family"] = json!("closed-solid-fill-cubic-path");
    legacy.as_object_mut().unwrap().remove("vector");
    legacy["fill"] = json!({"kind":"solid","r":1,"g":0,"b":0,"a":1});
    assert!(decode_storage_project(drawing(vec![legacy]).to_string().as_bytes()).is_err());
    stable.as_object_mut().unwrap().remove("vectorExt");
    let doc = drawing(vec![stable]);
    let decoded = decode_storage_project(doc.to_string().as_bytes()).unwrap();
    let encoded: Value =
        serde_json::from_slice(&encode_storage_project(&decoded).unwrap()).unwrap();
    assert_eq!(encoded, doc, "absent extension remains absent");
}

#[test]
fn gradient_fallback_fill_changes_preserve_paint_and_boolean_results_preserve_source_gradient() {
    let mut r = gradient_record(&rows()[0]);
    r["target"]["strokeId"] = json!("a");
    r["fill"] = json!({"kind":"solid","r":1,"g":0,"b":0,"a":1});
    let mut b = r.clone();
    b["target"]["strokeId"] = json!("b");
    let mut o = owner(&drawing(vec![r.clone(), b]));
    let p = plan(&snapshot(&o), 0);
    let fill = json!({"command":"object.fill.set","stableTarget":r["target"],"fill":{"kind":"solid","r":0,"g":1,"b":0,"a":1}});
    let change = o.prepare_fill_json(fill.to_string().as_bytes()).unwrap();
    assert_eq!(o.commit_fill(change), Ok(1));
    assert_eq!(
        document(&snapshot(&o))["objects"][0]["vectorExt"],
        r["vectorExt"]
    );
    assert_eq!(
        p.commands().next().unwrap().style().unwrap(),
        plan(&snapshot(&o), 0)
            .commands()
            .next()
            .unwrap()
            .style()
            .unwrap()
    );
    let payload = json!({"command":"object.boolean.apply","mode":"unite","layerUid":"L1","contextId":"scene-root","frame":0,"strokeIds":["a","b"]});
    let candidate = o
        .prepare_boolean_op_json(payload.to_string().as_bytes())
        .unwrap();
    assert_eq!(o.commit_boolean_edit(candidate), Ok(2));
    let after = document(&snapshot(&o));
    assert!(!after["objects"].as_array().unwrap().is_empty());
    for made in after["objects"].as_array().unwrap() {
        assert_eq!(
            made["vectorExt"]["fillGradient"],
            r["vectorExt"]["fillGradient"]
        );
    }
    assert_eq!(o.undo(2), Ok(3));
    assert_eq!(
        document(&snapshot(&o))["objects"].as_array().unwrap().len(),
        2
    );
}

#[test]
fn gradient_stops_keep_authored_order_alpha_once_and_position_uses_evaluation_not_held_frame() {
    let mut r = gradient_record(&rows()[0]);
    r["target"]["strokeId"] = json!("g");
    r["vector"]["opacity"] = json!(0.5);
    r["vectorExt"]["fillGradient"]["stops"] =
        json!([{"offset":1,"color":"#11223380"},{"offset":0,"color":"#aabbcc"}]);
    let mut doc = drawing(vec![r.clone()]);
    doc["propertyHolders"] = json!([{"target":{"contextId":"scene-root","layerUid":"L1"},"properties":[
        {"property":"position","track":{"keys":[{"frame":0,"v":[0,0],"curvePoints":[]},{"frame":10,"v":[10,6]}]}},
        {"property":"opacity","static":[50]}]}]);
    let o = owner(&doc);
    let s = snapshot(&o);
    let bytes = encode_storage_project(s.document()).unwrap();
    for (frame, dx, dy) in [(0, 0., 0.), (5, 5., 3.), (10, 10., 6.)] {
        let p = plan(&s, frame);
        assert_eq!(p.packet().source_frame_for_layer(0), Some(0));
        let command = p.commands().next().unwrap();
        let style = command.style().unwrap();
        let gradient = style.fill.gradient().unwrap();
        assert_eq!(
            gradient.geometry,
            GradientGeometry::Linear {
                from: [dx, dy],
                to: [32. + dx, dy]
            }
        );
        assert_eq!(gradient.stops[0].offset, 1.);
        assert_eq!(
            gradient.stops[0].color,
            [17, 34, 51, 32],
            "128 * item .5 * layer .5, once"
        );
        assert_eq!(gradient.stops[1].offset, 0.);
        assert_eq!(gradient.stops[1].color, [170, 187, 204, 64]);
        assert_eq!(
            command.path().elements()[0],
            vello::kurbo::PathEl::MoveTo((dx, dy).into())
        );
    }
    assert_eq!(encode_storage_project(s.document()).unwrap(), bytes);
    assert_eq!(o.history_depths(), (0, 0));
}

#[test]
fn stored_gradient_parametric_translate_scale_and_parameter_set_keep_baseline_endpoint_semantics() {
    let mut r = gradient_record(&rows()[0]);
    r["target"]["strokeId"] = json!("g");
    r["vectorExt"]["paramShape"] = json!({"kind":"rect","box":{"x1":0,"y1":0,"x2":32,"y2":32}});
    let mut o = owner(&drawing(vec![r.clone()]));
    let old = snapshot(&o);
    // The finite-f64 native transform serializes floating endpoints, even when integral.
    for (command, fields, from, to, revision) in [
        (
            "object.parametric.translate",
            json!({"delta":[5,-3]}),
            json!([5.0, -3.0]),
            json!([37.0, -3.0]),
            1,
        ),
        (
            "object.parametric.scale",
            json!({"scale":[2,0.5],"anchor":[1,2]}),
            json!([9.0, -0.5]),
            json!([73.0, -0.5]),
            2,
        ),
        (
            "object.parametric.set",
            json!({"paramShape":{"kind":"rect","tl":4}}),
            json!([9.0, -0.5]),
            json!([73.0, -0.5]),
            3,
        ),
    ] {
        let mut payload = fields;
        payload["command"] = json!(command);
        payload["stableTarget"] = r["target"].clone();
        o.dispatch_vector_param("command.document.object.parametric.edit", &payload)
            .unwrap();
        assert_eq!(o.content_revision(), revision);
        let current = document(&snapshot(&o));
        let gradient = &current["objects"][0]["vectorExt"]["fillGradient"];
        assert_eq!(gradient["from"], from);
        assert_eq!(gradient["to"], to);
        assert_eq!(gradient["stops"], r["vectorExt"]["fillGradient"]["stops"]);
    }
    let serialized = saved(&o);
    let fresh = NativeObjectHistory::from_project_file("parametric-reopen", &serialized).unwrap();
    assert_eq!(document(&snapshot(&fresh)), document(&snapshot(&o)));
    assert_eq!(o.history_depths(), (3, 0));
    assert_eq!(o.undo(3), Ok(4));
    assert_eq!(o.redo(4), Ok(5));
    assert_eq!(document(&old)["objects"][0], r);
}

#[test]
fn stored_gradient_svg_refuses_before_output_instead_of_flat_fill_degradation() {
    use crate::export_job::native_vector_export::native_vector_export_svg::{
        serialize_svg_frame, SvgExportError, SvgFrameSpec,
    };
    let mut r = gradient_record(&rows()[0]);
    r["target"]["strokeId"] = json!("g");
    let o = owner(&drawing(vec![r]));
    let s = snapshot(&o);
    let before = saved(&o);
    let p = plan(&s, 0);
    assert!(matches!(
        serialize_svg_frame(&p, SvgFrameSpec::new(32, 32, 4096).unwrap()),
        Err(SvgExportError::Unsupported {
            source_object_index: 0,
            ..
        })
    ));
    assert_eq!(saved(&o), before);
    assert_eq!(o.history_depths(), (0, 0));
}

#[cfg(feature = "application")]
#[path = "object_stored_gradient_gpu.rs"]
mod gpu;
