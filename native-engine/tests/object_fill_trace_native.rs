//! Real stored-record providers, retaining the original circle-lens regression bounds
//! from geometry-wasm/src/fill.rs and independently calculated rectangle/union facts.
use super::native::{area, bounds, NativeTrace};
use super::native_merge::{polygons, NativeUnion};
use super::*;
use crate::object_codec::decode_storage_project;
use crate::object_document::object_group_kernels::object_boolean_ops::stack::Scope;
use crate::object_document::ObjectDocument;
use serde_json::{json, Value};

fn segment(p: [f64; 2], i: [f64; 2], o: [f64; 2]) -> Value {
    json!({"point":{"x":p[0],"y":p[1]},"handleIn":{"x":i[0],"y":i[1]},"handleOut":{"x":o[0],"y":o[1]}})
}
fn record(id: &str, segments: Vec<Value>, closed: bool) -> Value {
    json!({"schemaVersion":1,"family":"editable-vector-path","target":{"contextId":"scene-root",
        "layerUid":"L","strokeId":id,"frameScope":{"kind":"authored","frame":0}},
        "geometry":{"kind":"cubic-path","closed":closed,"coordinateSpace":"document-world","handleSpace":"relative","segments":segments},
        "fill":null,"vector":{"stroke":{"kind":"solid","r":0,"g":0,"b":0,"a":1},"hasRealStroke":true,
            "strokeWidth":2,"strokeCap":"round","strokeJoin":"round","opacity":1}})
}
fn doc(records: Vec<Value>) -> ObjectDocument {
    decode_storage_project(
        json!({"format":"nemo.native-object-document","formatVersion":1,
        "totalFrames":2,"layers":[{"layerUid":"L"}],"objects":records})
        .to_string()
        .as_bytes(),
    )
    .unwrap()
}
fn scope() -> Scope {
    Scope {
        context_id: "scene-root".to_string(),
        layer_uid: "L".to_string(),
        frame: 0,
    }
}
fn circle(id: &str, cx: f64) -> Value {
    let cy = 380.0;
    let r = 90.0;
    let k = 49.70562748477142;
    record(
        id,
        vec![
            segment([cx - r, cy], [0.0, k], [0.0, -k]),
            segment([cx, cy - r], [-k, 0.0], [k, 0.0]),
            segment([cx + r, cy], [0.0, -k], [0.0, k]),
            segment([cx, cy + r], [k, 0.0], [-k, 0.0]),
        ],
        true,
    )
}

#[test]
fn stored_open_walls_trace_the_real_1600_square_without_a_test_provider() {
    let corners = [[0.0, 0.0], [40.0, 0.0], [40.0, 40.0], [0.0, 40.0]];
    let records = (0..4)
        .map(|i| {
            record(
                &format!("w{i}"),
                vec![
                    segment(corners[i], [0.0; 2], [0.0; 2]),
                    segment(corners[(i + 1) % 4], [0.0; 2], [0.0; 2]),
                ],
                false,
            )
        })
        .collect();
    let d = doc(records);
    let mut provider = NativeTrace::new(&d, &scope(), [20.0, 20.0], None, None).unwrap();
    let hit = find_fill_region(&mut provider, None).unwrap().unwrap();
    assert!((hit.area - 1600.0).abs() < 1e-6);
    assert_eq!(hit.gap_px, 0);
    assert!(!hit.used_gap);
    let mut ids = hit.wall_ids;
    ids.sort();
    assert_eq!(ids, ["w0", "w1", "w2", "w3"]);
}

#[test]
fn original_crossing_circle_lens_bounds_are_preserved_from_real_cubic_records() {
    let d = doc(vec![circle("left", 220.0), circle("right", 340.0)]);
    let mut provider = NativeTrace::new(&d, &scope(), [280.0, 380.0], None, None).unwrap();
    let hit = find_fill_region(&mut provider, Some(0)).unwrap().unwrap();
    let b = bounds(&hit.ring);
    assert!((b.x - 250.0).abs() < 1.0);
    assert!((b.x + b.width - 310.0).abs() < 1.0);
    assert!((b.y - 312.9).abs() < 1.0);
    assert!((b.y + b.height - 447.1).abs() < 1.0);
    assert_eq!(hit.wall_ids.len(), 2);
    assert!(!hit.used_gap);
}

#[test]
fn curved_union_keeps_both_circles_instead_of_treating_the_four_anchors_as_diamonds() {
    let d = doc(vec![circle("left", 220.0), circle("right", 340.0)]);
    let a = native::ring(&d.objects[0]);
    let b = native::ring(&d.objects[1]);
    let mut provider = NativeUnion {
        rings: vec![a.clone(), b],
        result: polygons(&a),
    };
    assert_eq!(provider.unite(1), Ok(true));
    let islands = provider.islands().unwrap();
    assert_eq!(islands.len(), 1);
    let b = bounds(&islands[0].segments);
    assert!((b.x - 130.0).abs() < 1.0);
    assert!((b.width - 300.0).abs() < 1.0);
    assert!((b.y - 290.0).abs() < 1.0);
    assert!((b.height - 180.0).abs() < 1.0);
    // Analytic radius90, centre separation120 union area45318; cubic circle and
    // the retained subpixel adaptive flattening stay within400 square pixels.
    assert!((area(&islands[0].segments) - 45318.0).abs() < 400.0);
}

#[test]
fn real_raster_fallback_floods_only_a_closed_inflated_wall_region() {
    let p = [[0.0, 0.0], [100.0, 0.0], [100.0, 100.0], [0.0, 100.0]];
    let d = doc(vec![record(
        "box",
        p.iter().map(|&p| segment(p, [0.0; 2], [0.0; 2])).collect(),
        true,
    )]);
    let mut provider = NativeTrace::new(&d, &scope(), [50.0, 50.0], None, None).unwrap();
    let hit = provider.raster().unwrap().unwrap();
    let b = bounds(&hit.ring);
    assert_eq!(hit.gap_px, 24);
    assert_eq!(hit.wall_ids, ["box"]);
    assert!(hit.used_gap);
    assert!(b.x >= 24.0 && b.x <= 27.0 && b.y >= 24.0 && b.y <= 27.0);
    assert!(hit.area > 2000.0 && hit.area < 2700.0);
    let mut outside = NativeTrace::new(&d, &scope(), [1000.0, 1000.0], None, None).unwrap();
    assert!(outside.raster().unwrap().is_none());
}

#[test]
fn stored_wall_restriction_and_frame_scope_cannot_trace_unrelated_records() {
    let d = doc(vec![circle("left", 220.0), circle("right", 340.0)]);
    let only = vec!["missing".to_string()];
    let mut provider = NativeTrace::new(&d, &scope(), [220.0, 380.0], None, Some(&only)).unwrap();
    assert!(find_fill_region(&mut provider, None).unwrap().is_none());
    let mut other = scope();
    other.frame = 1;
    let mut provider = NativeTrace::new(&d, &other, [220.0, 380.0], None, None).unwrap();
    assert!(find_fill_region(&mut provider, None).unwrap().is_none());
}

#[test]
fn a_near_t_junction_subdivides_the_box_on_the_first_gap_step() {
    let box_points = [[0.0, 0.0], [60.0, 0.0], [60.0, 60.0], [0.0, 60.0]];
    let boundary = record(
        "box",
        box_points
            .iter()
            .map(|&p| segment(p, [0.0; 2], [0.0; 2]))
            .collect(),
        true,
    );
    let divider = record(
        "divider",
        vec![
            segment([30.0, 1.0], [0.0; 2], [0.0; 2]),
            segment([30.0, 59.0], [0.0; 2], [0.0; 2]),
        ],
        false,
    );
    let d = doc(vec![boundary, divider]);
    let mut provider = NativeTrace::new(&d, &scope(), [15.0, 30.0], None, None).unwrap();
    let hit = find_fill_region(&mut provider, Some(0)).unwrap().unwrap();
    let b = bounds(&hit.ring);
    assert!((b.x - 0.0).abs() < 1.0);
    assert!((b.width - 30.0).abs() < 1.0);
    assert!(hit.area > 1650.0 && hit.area < 1950.0);
    assert_eq!(hit.wall_ids.len(), 2);
    assert_eq!(hit.gap_px, 0);
}

#[test]
fn a_self_crossing_stroke_has_a_real_node_at_its_own_intersection() {
    let points = [[0.0, 0.0], [80.0, 80.0], [0.0, 80.0], [80.0, 0.0]];
    let d = doc(vec![record(
        "self",
        points
            .iter()
            .map(|&p| segment(p, [0.0; 2], [0.0; 2]))
            .collect(),
        false,
    )]);
    let mut provider = NativeTrace::new(&d, &scope(), [40.0, 65.0], None, None).unwrap();
    let hit = find_fill_region(&mut provider, Some(0)).unwrap().unwrap();
    assert!(
        (hit.area - 1600.0).abs() < 1.0,
        "expected triangle area1600, actual {}",
        hit.area
    );
    assert_eq!(hit.wall_ids, ["self"]);
}
