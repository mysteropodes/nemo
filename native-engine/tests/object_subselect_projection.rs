//! Native point-type commits under nested rotated/nonuniform transforms, compared with frozen JS.
#[path = "object_vector_edit_support.rs"]
#[allow(dead_code)]
mod support;
use super::matrices;
use crate::object_frame_packet::{prepare_object_frame, FrameScopeKind, ObjectFrameSelector};
use crate::object_render_scene::prepare_object_render_scene;
use serde_json::{json, Value};
use vello::kurbo::{Affine, PathEl, Point};
fn oracle() -> Value {
    serde_json::from_str(include_str!("fixtures/subselect-baseline/projection.json")).unwrap()
}
fn source() -> Value {
    let o = oracle();
    let mut records = Vec::new();
    for (i, path) in o["paths"].as_array().unwrap().iter().enumerate() {
        let mut r = support::record_at("A", "L1", 0, &format!("s-{i}"));
        r["geometry"]["closed"] = json!(true);
        r["geometry"]["segments"] = json!(path
            .as_array()
            .unwrap()
            .iter()
            .map(|s| {
                let p = |key: &str| json!({"x":s[key][0],"y":s[key][1]});
                json!({"point":p("point"),"handleIn":p("handleIn"),"handleOut":p("handleOut")})
            })
            .collect::<Vec<_>>());
        r["vector"]["stroke"] = Value::Null;
        r["vector"]["hasRealStroke"] = json!(false);
        r["fill"] = json!({"kind":"solid","r":1,"g":0,"b":0,"a":1});
        records.push(r);
    }
    let mut doc = support::document(
        2,
        records,
        Some(json!([
            {"target":{"contextId":"scene-root","layerUid":"L1","strokeId":"s-0"},"properties":o["element"]},
            {"target":{"contextId":"scene-root","layerUid":"L1"},"properties":o["layer"]},
            {"target":{"contextId":"scene-root","layerUid":"L2"},"properties":o["parent"]}
        ])),
    );
    doc["layers"][0]["parentLayerUid"] = json!("L2");
    doc["layers"][1]["isNullLayer"] = json!(true);
    doc["layers"][1]["nullPos"] = json!([50, 40]);
    doc
}
fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-8, "{a} != {b}");
}
fn check(snapshot: &crate::revision::ObjectSnapshot, expected: &Value) {
    let selector = ObjectFrameSelector {
        instance_id: snapshot.instance_id(),
        document_id: snapshot.document_id(),
        content_revision: snapshot.content_revision(),
        document_snapshot_id: snapshot.snapshot_id(),
        context_id: "scene-root",
        scope_kind: FrameScopeKind::Authored,
        frame: 0,
    };
    let packet = prepare_object_frame(snapshot, &selector).unwrap();
    let matrix = Affine::new(matrices(&packet, 0).unwrap()[&0]);
    let coeff = matrix.as_coeffs();
    assert!(
        (coeff[0] * coeff[2] + coeff[1] * coeff[3]).abs() > 0.01,
        "fixture must retain shear"
    );
    let record = &snapshot.document().objects()[0];
    for (seg, want) in record
        .geometry
        .segments
        .iter()
        .zip(expected.as_array().unwrap())
    {
        let p = matrix * Point::new(seg.point.x.as_f64().unwrap(), seg.point.y.as_f64().unwrap());
        near(p.x, want["point"][0].as_f64().unwrap());
        near(p.y, want["point"][1].as_f64().unwrap());
        for (h, key) in [(&seg.handle_in, "handleIn"), (&seg.handle_out, "handleOut")] {
            let x = h.x.as_f64().unwrap();
            let y = h.y.as_f64().unwrap();
            let v = Point::new(coeff[0] * x + coeff[2] * y, coeff[1] * x + coeff[3] * y);
            near(v.x, want[key][0].as_f64().unwrap());
            near(v.y, want[key][1].as_f64().unwrap());
        }
    }
    let scene = prepare_object_render_scene(&packet, &selector).unwrap();
    let entry = scene
        .entries()
        .find(|e| e.source().source_object_index() == 0)
        .unwrap();
    let endpoints: Vec<_> = entry
        .path()
        .elements()
        .iter()
        .filter_map(|e| match e {
            PathEl::MoveTo(p) | PathEl::LineTo(p) | PathEl::CurveTo(_, _, p) => Some(*p),
            _ => None,
        })
        .collect();
    for (p, want) in endpoints.iter().zip(expected.as_array().unwrap()) {
        near(p.x, want["point"][0].as_f64().unwrap());
        near(p.y, want["point"][1].as_f64().unwrap());
    }
}
#[test]
fn subselect_native_affine_and_point_types_match_original_under_scaled_rotated_parent() {
    let mut owner = support::owner_of(&source());
    check(&owner.acquire_snapshot(0).unwrap(), &oracle()["expected"]);
    for edit in oracle()["edits"].as_array().unwrap() {
        let before = support::bytes(&owner);
        let snapshot = owner.acquire_snapshot(0).unwrap();
        let selector = ObjectFrameSelector {
            instance_id: snapshot.instance_id(),
            document_id: snapshot.document_id(),
            content_revision: 0,
            document_snapshot_id: snapshot.snapshot_id(),
            context_id: "scene-root",
            scope_kind: FrameScopeKind::Authored,
            frame: 0,
        };
        let packet = prepare_object_frame(&snapshot, &selector).unwrap();
        let intent = serde_json::from_value(edit["intent"].clone()).unwrap();
        let candidate = crate::object_codec::subselect::record::plan(
            &snapshot.document().objects()[0],
            &intent,
        )
        .unwrap();
        let preview = super::matrices_with_preview(&packet, 0, Some(&candidate)).unwrap()[&0];
        assert_eq!(support::bytes(&owner), before);
        let q = json!({"apiVersion":2,"requestId":format!("type-{}",edit["intent"]["pointType"].as_str().unwrap()),"instanceId":owner.instance_id(),"documentId":owner.document_id(),"operation":"command.document.object.subselect.edit","expectedRevision":owner.content_revision(),"payload":{"command":"object.subselect.edit","stableTarget":support::target("L1",0,"s-0"),"intent":edit["intent"]}});
        let r = owner
            .dispatch_object_command(&serde_json::from_value(q).unwrap())
            .unwrap()
            .unwrap();
        assert!(r.ok, "{r:?}");
        check(
            &owner.acquire_snapshot(owner.content_revision()).unwrap(),
            &edit["expected"],
        );
        let after = owner.acquire_snapshot(owner.content_revision()).unwrap();
        let selector = ObjectFrameSelector {
            instance_id: after.instance_id(),
            document_id: after.document_id(),
            content_revision: after.content_revision(),
            document_snapshot_id: after.snapshot_id(),
            context_id: "scene-root",
            scope_kind: FrameScopeKind::Authored,
            frame: 0,
        };
        let actual = matrices(&prepare_object_frame(&after, &selector).unwrap(), 0).unwrap()[&0];
        for (a, b) in preview.into_iter().zip(actual) {
            near(a, b);
        }

        // Each frozen edit starts from the same canonical source, just as the baseline oracle.
        owner = support::owner_of(&source());
    }
}
#[test]
fn subselect_singular_transform_refuses_without_geometry_fallback() {
    let mut doc = source();
    doc["propertyHolders"][2]["properties"][3]["static"] = json!([0, 160]);
    let owner = support::owner_of(&doc);
    let s = owner.acquire_snapshot(0).unwrap();
    let selector = ObjectFrameSelector {
        instance_id: s.instance_id(),
        document_id: s.document_id(),
        content_revision: 0,
        document_snapshot_id: s.snapshot_id(),
        context_id: "scene-root",
        scope_kind: FrameScopeKind::Authored,
        frame: 0,
    };
    let packet = prepare_object_frame(&s, &selector).unwrap();
    assert!(matrices(&packet, 0).is_err());
    assert_eq!(owner.history_depths(), (0, 0));
}
