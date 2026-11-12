//! #2373: the viewport-only "Shadow guides" switch hides the Shadow Brush items (`vectorExt.shadow.channelTag == "shadow"`) of the
//! presented frame; an export never loses one to it. Literal pixel oracles on the actual headless GPU; no AppKit claim.
use crate::compositor::{object_compositor::ObjectOutputSpec, Compositor};
use crate::history::NativeObjectHistory;
use crate::object_document::FrameScopeKind;
use crate::object_frame_packet::{prepare_object_frame, ObjectFrameSelector};
use crate::object_render_scene::{
    prepare_object_composition, prepare_object_paint_plan, prepare_object_render_scene,
    prepare_object_viewport_composition, viewport_shadow_guides_quality_suffix,
    ObjectCompositionWitness,
};
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};

const MODE: &str = "project-srgb-normal-rgba8-area";

fn segments(r: [i32; 4]) -> Value {
    let [x0, y0, x1, y1] = r;
    json!([[x0, y0], [x1, y0], [x1, y1], [x0, y1]].map(|[x, y]| {
        json!({"point":{"x":x,"y":y},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}})
    }))
}
fn target(layer: &str, id: &str) -> Value {
    json!({"contextId":"scene-root","layerUid":layer,"strokeId":id,"frameScope":{"kind":"authored","frame":0}})
}
/// A plain legacy closed fill square.
fn plain(layer: &str, id: &str, r: [i32; 4], fill: [f64; 3]) -> Value {
    let cases: Value = serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    ))
    .unwrap();
    let mut record = cases["records"][0].clone();
    record["target"] = target(layer, id);
    record["geometry"]["segments"] = segments(r);
    record["fill"] = json!({"kind":"solid","r":fill[0],"g":fill[1],"b":fill[2],"a":1});
    record
}
/// A vector-family closed fill square; `shadow` tags it as Shadow Brush construction ink.
fn vector(layer: &str, id: &str, r: [i32; 4], fill: [f64; 3], shadow: bool) -> Value {
    let mut record = json!({"schemaVersion":1,"family":"editable-vector-path","target":target(layer, id),
        "geometry":{"kind":"cubic-path","closed":true,"coordinateSpace":"document-world","handleSpace":"relative",
            "segments":segments(r)},
        "fill":{"kind":"solid","r":fill[0],"g":fill[1],"b":fill[2],"a":1},
        "vector":{"stroke":null,"hasRealStroke":false,"strokeWidth":1,"strokeCap":"round","strokeJoin":"round",
            "miterLimit":10,"dashOffset":0,"opacity":1}});
    if shadow {
        record["vectorExt"] = json!({"shadow":{"channelTag":"shadow","shadowSwatchId":"sh1"}});
    }
    record
}
fn owner(objects: Vec<Value>) -> NativeObjectHistory {
    let elements: Vec<Value> = objects
        .iter()
        .enumerate()
        .map(|(i, o)| {
            json!({"layerUid":o["target"]["layerUid"],"strokeId":o["target"]["strokeId"],"naturalPosition":i,"order":{}})
        })
        .collect();
    let drawing = json!({"format":"nemo.native-object-document","formatVersion":2,"totalFrames":2,
        "layers":[{"layerUid":"L0","timing":{"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing",
            "contextId":"scene-root","keys":[{"frame":0}],"samples":[]}}],
        "objects":objects,
        "compositionOrder":{"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
            "contexts":[{"contextId":"scene-root","layers":[{"layerUid":"L0","naturalPosition":0,"order":{}}],
                "scopes":[{"frameScope":{"kind":"authored","frame":0},"elements":elements}]}],
            "migration":"explicit-plan-required","referencePolicy":"unresolved"}});
    NativeObjectHistory::from_project_file(
        "shadow",
        &serde_json::to_vec(&json!({"format":"nemo.native-project-document","formatVersion":1,
            "project":{"canvasW":64,"canvasH":64,"fps":24},"viewportReference":null,"assets":[],"drawing":drawing}))
        .unwrap(),
    )
    .unwrap()
}
/// Blue plain square at 8..24, red shadow-tagged vector square at 40..56, green untagged vector square at 8..24 lower down.
fn guided() -> NativeObjectHistory {
    owner(vec![
        plain("L0", "plain", [8, 8, 24, 24], [0., 0., 1.]),
        vector("L0", "guide", [40, 8, 56, 24], [1., 0., 0.], true),
        vector("L0", "kept", [8, 40, 24, 56], [0., 1., 0.], false),
    ])
}
fn pins(s: &ObjectSnapshot) -> ObjectFrameSelector<'_> {
    ObjectFrameSelector {
        instance_id: s.instance_id(),
        document_id: s.document_id(),
        content_revision: s.content_revision(),
        document_snapshot_id: s.snapshot_id(),
        context_id: "scene-root",
        scope_kind: FrameScopeKind::Authored,
        frame: 0,
    }
}
fn plan(s: &ObjectSnapshot) -> crate::object_render_scene::ObjectPaintPlan {
    let selector = pins(s);
    let packet = prepare_object_frame(s, &selector).unwrap();
    prepare_object_paint_plan(
        &prepare_object_render_scene(&packet, &selector).unwrap(),
        &selector,
    )
    .unwrap()
}
fn viewport(s: &ObjectSnapshot) -> ObjectCompositionWitness {
    prepare_object_viewport_composition(&plan(s), &pins(s), None).unwrap()
}
fn pixels(gpu: &mut Compositor, witness: &ObjectCompositionWitness) -> Vec<u8> {
    let result = gpu
        .compose_object(witness, ObjectOutputSpec::new(64, 64, MODE).unwrap())
        .unwrap();
    gpu.readback_object_rgba8(&result).unwrap().bytes().to_vec()
}
fn at(bytes: &[u8], x: usize, y: usize) -> [u8; 4] {
    bytes[(y * 64 + x) * 4..][..4].try_into().unwrap()
}

#[test]
fn the_hidden_guides_are_part_of_the_viewport_cache_identity_and_only_a_viewport_witness_takes_them() {
    let owner = guided();
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let plain = viewport(&snapshot);
    let hidden = viewport(&snapshot).with_viewport_hidden_shadow_guides().unwrap();
    assert_eq!(
        hidden.expected_quality(),
        format!("{}{}", plain.expected_quality(), viewport_shadow_guides_quality_suffix())
    );
    assert!(!plain.viewport_hides_shadow_guides() && hidden.viewport_hides_shadow_guides());
    let export = prepare_object_composition(&plan(&snapshot), &pins(&snapshot)).unwrap();
    assert!(export.clone().with_viewport_hidden_shadow_guides().is_err());
    assert!(!export.viewport_hides_shadow_guides());
    assert_eq!(export.expected_quality(), "final");
}

#[test]
fn gpu_the_hidden_guides_leave_only_the_shadow_tagged_items_out_of_the_viewport() {
    let mut gpu = Compositor::new().expect("actual headless GPU is required; never skip");
    let owner = guided();
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let shown = pixels(&mut gpu, &viewport(&snapshot));
    assert_eq!(at(&shown, 48, 16), [255, 0, 0, 255], "the guide is drawn by default");
    assert_eq!(at(&shown, 16, 16), [0, 0, 255, 255]);
    assert_eq!(at(&shown, 16, 48), [0, 255, 0, 255]);
    let hidden = pixels(
        &mut gpu,
        &viewport(&snapshot).with_viewport_hidden_shadow_guides().unwrap(),
    );
    assert_eq!(at(&hidden, 48, 16), at(&shown, 60, 60), "the guide is gone, the document background shows");
    assert_eq!(at(&hidden, 16, 16), [0, 0, 255, 255], "a plain item is unchanged");
    assert_eq!(at(&hidden, 16, 48), [0, 255, 0, 255], "an untagged vector item is unchanged");
    // The mode is not sticky on the compositor: the next plain viewport frame has the guide again, byte for byte.
    assert_eq!(pixels(&mut gpu, &viewport(&snapshot)), shown);
}

#[test]
fn gpu_the_flag_changes_nothing_for_a_frame_without_guides_and_an_export_keeps_them() {
    let mut gpu = Compositor::new().expect("actual headless GPU is required; never skip");
    let clean = owner(vec![plain("L0", "plain", [8, 8, 24, 24], [0., 0., 1.])]);
    let snapshot = clean.acquire_snapshot(0).unwrap();
    assert_eq!(
        pixels(&mut gpu, &viewport(&snapshot)),
        pixels(&mut gpu, &viewport(&snapshot).with_viewport_hidden_shadow_guides().unwrap()),
        "default and flagged requests render the same pixels when nothing is tagged"
    );
    let owner = guided();
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let export = prepare_object_composition(&plan(&snapshot), &pins(&snapshot)).unwrap();
    let before = pixels(&mut gpu, &export);
    // The same compositor paints a guides-hidden viewport frame in between: nothing of it may leak into an export.
    let hidden = pixels(
        &mut gpu,
        &viewport(&snapshot).with_viewport_hidden_shadow_guides().unwrap(),
    );
    assert_eq!(at(&hidden, 48, 16), at(&hidden, 60, 60));
    let after = pixels(&mut gpu, &export);
    assert_eq!(before, after);
    assert_eq!(at(&after, 48, 16), [255, 0, 0, 255], "an export still draws the guide");
}
