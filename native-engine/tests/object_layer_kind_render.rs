//! #2371 D36: the renderer ignores the Null, Guide and Effect layers the creation adds, and a Null that adopts a posed layer
//! leaves it where it was (a folder layer is not part of this oracle: the exposure packet does not take one yet). The oracle is the real renderer (owner ->
//! exposure frame -> render scene -> paint plan): every contour point of every stroke before the edit must equal the same
//! point after it, at the edit frame, within 1e-6.
use crate::history::object_history::object_layer_lifecycle_tests::{send, timing, value};
use crate::history::object_history::NativeObjectHistory;
use crate::object_document::{FrameScopeKind, ProjectEnvelope, ProjectMetadata};
use crate::object_frame_packet::{prepare_object_exposure_frame, ObjectFrameSelector};
use crate::object_render_scene::{prepare_object_paint_plan, prepare_object_render_scene};
use serde_json::{json, Value};

fn rect(layer: &str, x: f64) -> Value {
    let corner = |x: f64, y: f64| json!({"point":{"x":x,"y":y},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}});
    json!({"schemaVersion":1,"family":"editable-vector-path",
        "target":{"contextId":"scene-root","frameScope":{"kind":"authored","frame":0},"layerUid":layer,"strokeId":format!("s{layer}")},
        "geometry":{"kind":"cubic-path","closed":true,"coordinateSpace":"document-world","handleSpace":"relative",
            "segments":[corner(x, 20.), corner(x + 100., 20.), corner(x + 100., 80.), corner(x, 80.)]},
        "fill":{"kind":"solid","r":0.25,"g":0.5,"b":0.75,"a":1},
        "vector":{"stroke":null,"hasRealStroke":false,"strokeWidth":3,"strokeCap":"round","strokeJoin":"round","opacity":1}})
}
fn layer(uid: &str) -> Value {
    json!({"layerUid":uid,"name":uid,"visible":true,"locked":false,"color":"#5fa875","parentLayerUid":null,"timing":timing()})
}
/// C (a 100 x 60 rect, rotated and offset) and D (another rect).
fn fixture() -> Value {
    let uids = ["C", "D"];
    let holder = |uid: &str, props: Value| json!({"target":{"contextId":"scene-root","layerUid":uid},"properties":props});
    let elements: Vec<Value> = ["C", "D"]
        .iter()
        .map(|u| json!({"layerUid":u,"strokeId":format!("s{u}"),"naturalPosition":0,"order":{}}))
        .collect();
    json!({"format":"nemo.native-object-document","formatVersion":2,"totalFrames":8,
        "layers":uids.iter().map(|u| layer(u)).collect::<Vec<_>>(),
        "objects":[rect("C", 0.), rect("D", 200.)],
        "propertyHolders":[
            holder("C", json!([{"property":"position","static":[3, 7]},{"property":"rotation","static":[12]}])),
            ],
        "compositionOrder":{"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
            "migration":"explicit-plan-required","referencePolicy":"unresolved","contexts":[{"contextId":"scene-root",
                "layers":uids.iter().enumerate().map(|(n, u)| json!({"layerUid":u,"naturalPosition":n,"order":{}})).collect::<Vec<_>>(),
                "scopes":[{"frameScope":{"kind":"authored","frame":0},
                    "elements":elements}]}]}})
}
fn owner(document: &Value) -> NativeObjectHistory {
    let document =
        crate::object_codec::decode_storage_project(document.to_string().as_bytes()).unwrap();
    let project = ProjectEnvelope::no_media(ProjectMetadata::new(400, 200, 24.).unwrap(), document);
    NativeObjectHistory::from_admitted_project("folder-world", project).unwrap()
}
/// Every contour point of every stroke the renderer paints at frame 0, by stroke.
fn world(owner: &NativeObjectHistory) -> Vec<(String, Vec<[f64; 2]>)> {
    let s = owner.acquire_snapshot(owner.content_revision()).unwrap();
    let selector = ObjectFrameSelector {
        instance_id: s.instance_id(),
        document_id: s.document_id(),
        content_revision: s.content_revision(),
        document_snapshot_id: s.snapshot_id(),
        context_id: "scene-root",
        scope_kind: FrameScopeKind::Authored,
        frame: 0,
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

#[test]
fn the_renderer_ignores_the_null_guide_and_effect_layers_creation_adds() {
    let mut owner = owner(&fixture());
    let before = world(&owner);
    for (n, (kind, extra)) in [
        ("null", json!({"children":["C"]})),
        ("guide", json!({"pos":[10, 10]})),
        ("effect", json!({})),
    ]
    .into_iter()
    .enumerate()
    {
        let mut payload = json!({"command":"object.layer.kind.create","kind":kind,"name":format!("K{n}"),"frame":0});
        for (key, member) in extra.as_object().unwrap() {
            payload[key] = member.clone();
        }
        let reply = send(&mut owner, &format!("k{n}"), "kind.create", &payload);
        assert_eq!(reply["result"]["applied"], true, "{kind}: {reply}");
        same(&before, &world(&owner), kind);
    }
    assert_eq!(value(&owner)["layers"].as_array().unwrap().len(), 5);
    assert_eq!(
        value(&owner)["layers"][0]["parentLayerUid"],
        value(&owner)["layers"][2]["layerUid"],
        "the Null adopted C"
    );
    let total = owner.history_depths().0;
    for _ in 0..total {
        let at = owner.content_revision();
        owner.undo(at).unwrap();
    }
    same(&before, &world(&owner), "after undoing every creation");
}
