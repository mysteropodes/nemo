//! #2372 Parent A under a rotated or scaled ancestor: the child keeps its WORLD pose. The oracle is the real renderer
//! (owner -> exposure frame -> render scene -> paint plan): every contour point of every stroke before the edit must
//! equal the same point after it, at the edit frame, within 1e-6 -- the plan applies the whole parent chain to the
//! stored geometry with the evaluated pivots, so a wrong pivot, a wrong order or a wrong decomposition moves points.
use super::object_layer_lifecycle_tests::{owner_of, send, timing, value};
use super::*;
use crate::object_document::{FrameScopeKind, ProjectEnvelope, ProjectMetadata};
use crate::object_frame_packet::{prepare_object_exposure_frame, ObjectFrameSelector};
use crate::object_render_scene::{prepare_object_paint_plan, prepare_object_render_scene};
use serde_json::{json, Value};

fn rect(layer: &str, x: f64, y: f64, w: f64, h: f64) -> Value {
    let corner = |x: f64, y: f64| json!({"point":{"x":x,"y":y},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}});
    json!({"schemaVersion":1,"family":"editable-vector-path",
        "target":{"contextId":"scene-root","frameScope":{"kind":"authored","frame":0},
            "layerUid":layer,"strokeId":format!("s{layer}")},
        "geometry":{"kind":"cubic-path","closed":true,"coordinateSpace":"document-world","handleSpace":"relative",
            "segments":[corner(x, y), corner(x + w, y), corner(x + w, y + h), corner(x, y + h)]},
        "fill":{"kind":"solid","r":0.25,"g":0.5,"b":0.75,"a":1},
        "vector":{"stroke":null,"hasRealStroke":false,"strokeWidth":3,"strokeCap":"round","strokeJoin":"round","opacity":1}})
}
fn layer(uid: &str, null: bool) -> Value {
    let mut layer = json!({"layerUid":uid,"name":uid,"visible":true,"locked":false,"color":"#5fa875",
        "parentLayerUid":null,"timing":timing()});
    if null {
        layer["isNullLayer"] = json!(true);
        layer["nullPos"] = json!([123, 45]);
    }
    layer
}
/// Layers: child C (a 100 x 60 rect), vector parents P and Q, a Null N, a grandparent G -- each with its own pose.
fn fixture(extra: &[(&str, &str, Value)]) -> Value {
    let uids = ["C", "P", "Q", "N", "G"];
    let mut holders = vec![
        ("C", "position", json!([3, 7])),
        ("C", "rotation", json!([12])),
        ("P", "position", json!([10, -4])),
        ("P", "rotation", json!([30])),
        ("P", "scale", json!([200, 200])),
        ("Q", "position", json!([-25, 60])),
        ("Q", "rotation", json!([-70])),
        ("Q", "scale", json!([50, 50])),
        ("N", "position", json!([5, 5])),
        ("N", "rotation", json!([45])),
        ("N", "scale", json!([150, 150])),
        ("G", "rotation", json!([20])),
        ("G", "anchor", json!([8, -3])),
    ];
    for (uid, key, v) in extra {
        holders.retain(|(u, k, _)| !(u == uid && k == key));
        holders.push((uid, key, v.clone()));
    }
    let mut property_holders: Vec<Value> = Vec::new();
    for uid in uids {
        let properties: Vec<Value> = holders
            .iter()
            .filter(|(u, _, _)| *u == uid)
            .map(|(_, key, v)| {
                if v.get("keys").is_some() {
                    json!({"property":key,"track":v})
                } else {
                    json!({"property":key,"static":v})
                }
            })
            .collect();
        property_holders.push(
            json!({"target":{"contextId":"scene-root","layerUid":uid},"properties":properties}),
        );
    }
    let drawn = [("C", 0.), ("P", 200.), ("Q", -150.), ("G", 400.)];
    let element = |uid: &str| json!({"layerUid":uid,"strokeId":format!("s{uid}"),"naturalPosition":0,"order":{}});
    json!({"format":"nemo.native-object-document","formatVersion":2,"totalFrames":8,
        "layers":uids.iter().map(|u| layer(u, *u == "N")).collect::<Vec<_>>(),
        "objects":drawn.iter().map(|(u, x)| rect(u, *x, 20., 100., 60.)).collect::<Vec<_>>(),
        "propertyHolders":property_holders,
        "compositionOrder":{"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
            "migration":"explicit-plan-required","referencePolicy":"unresolved","contexts":[{"contextId":"scene-root",
                "layers":uids.iter().enumerate().map(|(n, u)| json!({"layerUid":u,"naturalPosition":n,"order":{}})).collect::<Vec<_>>(),
                "scopes":[{"frameScope":{"kind":"authored","frame":0},
                    "elements":drawn.iter().map(|(u, _)| element(u)).collect::<Vec<_>>()}]}]}})
}
fn owner(document: &Value) -> NativeObjectHistory {
    let document =
        crate::object_codec::decode_storage_project(document.to_string().as_bytes()).unwrap();
    let project = ProjectEnvelope::no_media(ProjectMetadata::new(400, 200, 24.).unwrap(), document);
    NativeObjectHistory::from_admitted_project("keep-world", project).unwrap()
}
/// Every contour point of every stroke the renderer paints at `frame`, by stroke.
fn world(owner: &NativeObjectHistory, frame: u32) -> Vec<(String, Vec<[f64; 2]>)> {
    let s = owner.acquire_snapshot(owner.content_revision()).unwrap();
    let selector = ObjectFrameSelector {
        instance_id: s.instance_id(),
        document_id: s.document_id(),
        content_revision: s.content_revision(),
        document_snapshot_id: s.snapshot_id(),
        context_id: "scene-root",
        scope_kind: FrameScopeKind::Authored,
        frame,
    };
    let packet = prepare_object_exposure_frame(&s, &selector).unwrap();
    let scene = prepare_object_render_scene(&packet, &selector).unwrap();
    let plan = prepare_object_paint_plan(&scene, &selector).unwrap();
    let mut all: Vec<_> = plan
        .commands()
        .map(|c| {
            (
                c.source().record().target.stroke_id.clone(),
                c.contour().segments().iter().map(|p| p.point).collect(),
            )
        })
        .collect();
    all.sort_by(|a, b| a.0.cmp(&b.0));
    all
}
fn same(a: &[(String, Vec<[f64; 2]>)], b: &[(String, Vec<[f64; 2]>)], what: &str) {
    assert_eq!(a.len(), b.len(), "{what}");
    for ((ia, pa), (ib, pb)) in a.iter().zip(b) {
        assert_eq!(ia, ib, "{what}");
        for (p, q) in pa.iter().zip(pb) {
            assert!(
                (p[0] - q[0]).abs() < 1e-6 && (p[1] - q[1]).abs() < 1e-6,
                "{what} {ia}: {p:?} vs {q:?}"
            );
        }
    }
}
fn parent(child: &str, to: Value, frame: u32) -> Value {
    json!({"command":"object.layer.parent.set","contextId":"scene-root","layerUid":child,"parentLayerUid":to,"frame":frame})
}
fn static_of(document: &Value, layer: &str, property: &str) -> Vec<f64> {
    document["propertyHolders"]
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h["target"]["layerUid"] == layer && h["target"].get("strokeId").is_none())
        .and_then(|h| {
            h["properties"]
                .as_array()?
                .iter()
                .find(|p| p["property"] == property)
        })
        .and_then(|p| p["static"].as_array())
        .map(|a| a.iter().map(|n| n.as_f64().unwrap()).collect())
        .unwrap_or_default()
}
fn unavailable(owner: &mut NativeObjectHistory, payload: &Value, what: &str) {
    let (before, depth) = (
        super::object_layer_lifecycle_tests::bytes(owner),
        owner.history_depths(),
    );
    let reply = send(owner, what, "parent.set", payload);
    assert_eq!(
        (reply["ok"].clone(), reply["error"]["code"].clone()),
        (json!(false), json!("unavailable")),
        "{what}: {reply}"
    );
    assert_eq!(
        (
            super::object_layer_lifecycle_tests::bytes(owner),
            owner.history_depths()
        ),
        (before, depth),
        "{what}"
    );
}

#[test]
fn a_rotated_and_scaled_vector_parent_keeps_the_childs_world_pose_as_one_undoable_entry() {
    let mut owner = owner(&fixture(&[]));
    let (before, bytes_before) = (
        world(&owner, 0),
        super::object_layer_lifecycle_tests::bytes(&owner),
    );
    let reply = send(&mut owner, "p1", "parent.set", &parent("C", json!("P"), 0));
    assert_eq!(
        reply["result"],
        json!({"applied":true,"historyEntriesAdded":1}),
        "{reply}"
    );
    same(&before, &world(&owner, 0), "reparent");
    let document = value(&owner);
    assert_eq!(document["layers"][0]["parentLayerUid"], "P");
    // the parent's 30 degrees and 2x are undone by the child: 12 - 30 and 100 / 2
    assert!(
        (static_of(&document, "C", "rotation")[0] + 18.).abs() < 1e-6,
        "{document}"
    );
    assert!(
        (static_of(&document, "C", "scale")[0] - 50.).abs() < 1e-6,
        "{document}"
    );
    assert_eq!(owner.history_depths(), (1, 0));
    let kept = super::object_layer_lifecycle_tests::bytes(&owner);
    assert_eq!(owner.undo(1).unwrap(), 2);
    assert_eq!(
        super::object_layer_lifecycle_tests::bytes(&owner),
        bytes_before,
        "undo is exact"
    );
    same(&before, &world(&owner, 0), "undo");
    assert_eq!(owner.redo(2).unwrap(), 3);
    assert_eq!(
        super::object_layer_lifecycle_tests::bytes(&owner),
        kept,
        "redo is exact"
    );
    // clearing the parent is the inverse: the child returns to its own stored pose
    send(&mut owner, "p2", "parent.set", &parent("C", Value::Null, 0));
    same(&before, &world(&owner, 0), "clear");
    let cleared = value(&owner);
    assert!(
        (static_of(&cleared, "C", "rotation")[0] - 12.).abs() < 1e-6
            && (static_of(&cleared, "C", "scale")[0] - 100.).abs() < 1e-6,
        "{cleared}"
    );
}

#[test]
fn a_rotated_null_a_grandparent_chain_and_a_chain_swap_keep_the_world_pose() {
    let mut owner = owner(&fixture(&[]));
    let before = world(&owner, 0);
    send(&mut owner, "n", "parent.set", &parent("C", json!("N"), 0));
    same(&before, &world(&owner, 0), "under the rotated Null");
    // P under G: the grandparent chain G -> P is applied to the child below it
    send(&mut owner, "g", "parent.set", &parent("P", json!("G"), 0));
    send(&mut owner, "cp", "parent.set", &parent("C", json!("P"), 0));
    let chained = world(&owner, 0);
    send(&mut owner, "cq", "parent.set", &parent("C", json!("Q"), 0));
    same(&chained, &world(&owner, 0), "swap P (under G) for Q");
    send(&mut owner, "cn", "parent.set", &parent("C", Value::Null, 0));
    same(&chained, &world(&owner, 0), "clear");
}

#[test]
fn a_keyed_parent_is_kept_at_the_edit_frame_not_at_frame_zero() {
    let keys = json!({"keys":[{"frame":0,"v":[0]},{"frame":4,"v":[80]}]});
    let mut owner = owner(&fixture(&[("P", "rotation", keys)]));
    let before = world(&owner, 2);
    send(
        &mut owner,
        "keyed",
        "parent.set",
        &parent("C", json!("P"), 2),
    );
    same(&before, &world(&owner, 2), "frame 2");
    let document = value(&owner);
    let rotation = static_of(&document, "C", "rotation")[0];
    assert!(
        rotation < 12.0 - 1.0 && rotation > 12.0 - 80.0,
        "the keyed 0..80 at frame 2 is mid-way: {rotation}"
    );
}

#[test]
fn what_cannot_be_kept_exactly_is_refused_typed_and_changes_nothing() {
    // two rotated parents whose non-uniform scales would skew the child
    let skewing = fixture(&[("P", "scale", json!([200, 100]))]);
    unavailable(&mut owner(&skewing), &parent("C", json!("P"), 0), "skew");
    // a keyed child Rotation that has to change
    let keyed = json!({"keys":[{"frame":0,"v":[0]},{"frame":4,"v":[80]}]});
    unavailable(
        &mut owner(&fixture(&[("C", "rotation", keyed)])),
        &parent("C", json!("P"), 0),
        "keyed-rotation",
    );
    // a keyed child Position that has to move
    let moving = json!({"keys":[{"frame":0,"v":[0,0]},{"frame":4,"v":[8,0]}]});
    unavailable(
        &mut owner(&fixture(&[("C", "position", moving)])),
        &parent("C", json!("P"), 0),
        "keyed-position",
    );
    // a parent with an enabled expression needs the expression owner
    let mut expression = fixture(&[]);
    expression["propertyHolders"][1]["properties"][0] = json!({"property":"position","static":[0,0],"expression":{"enabled":true,"code":"[time,0]"}});
    unavailable(
        &mut owner(&expression),
        &parent("C", json!("P"), 0),
        "expression",
    );
    // a collapsed parent
    unavailable(
        &mut owner(&fixture(&[("P", "scale", json!([0, 0]))])),
        &parent("C", json!("P"), 0),
        "collapsed",
    );
    // the pure-translation chains still use the exact sum (no evaluation needed)
    let plain = fixture(&[
        ("P", "rotation", json!([0])),
        ("P", "scale", json!([100, 100])),
    ]);
    let mut owner = owner(&plain);
    let before = world(&owner, 0);
    let reply = send(
        &mut owner,
        "plain",
        "parent.set",
        &parent("C", json!("P"), 0),
    );
    assert_eq!(reply["result"]["applied"], true, "{reply}");
    same(&before, &world(&owner, 0), "translation only");
}

#[test]
fn a_layer_with_no_evaluated_pivot_is_refused_typed() {
    // the rotated parent holds a legacy contour record: no evaluated pivot is promised for it
    let mut legacy = fixture(&[]);
    legacy["objects"][1]["family"] = json!("closed-solid-fill-cubic-path");
    legacy["objects"][1]
        .as_object_mut()
        .unwrap()
        .remove("vector");
    unavailable(
        &mut owner(&legacy),
        &parent("C", json!("P"), 0),
        "legacy-parent",
    );
    // a child with no geometry at the edit frame
    let mut empty = fixture(&[]);
    empty["objects"].as_array_mut().unwrap().remove(0);
    empty["compositionOrder"]["contexts"][0]["scopes"][0]["elements"]
        .as_array_mut()
        .unwrap()
        .remove(0);
    unavailable(
        &mut owner(&empty),
        &parent("C", json!("P"), 0),
        "empty-child",
    );
}

#[test]
fn the_pure_facade_reports_the_null_rest_point_and_the_tight_bounds_centre() {
    let owner = owner(&fixture(&[]));
    let s = owner.acquire_snapshot(owner.content_revision()).unwrap();
    use crate::object_snapshot::object_parent_pivot_read::layer_pivot_bases;
    let bases = layer_pivot_bases(&s, "scene-root", &["C", "P", "N"], 0).unwrap();
    // C: x 0..100, y 20..80; P: x 200..300; N: its nullPos
    assert_eq!(bases, vec![[50., 50.], [250., 50.], [123., 45.]]);
    assert!(layer_pivot_bases(&s, "scene-root", &["nope"], 0).is_err());
    assert!(layer_pivot_bases(&s, "other-context", &["C"], 0).is_err());
}
