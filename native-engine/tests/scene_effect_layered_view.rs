//! #2257: the layered compositor under the viewport's display modes. A Multiply layer over an empty backdrop is the source
//! itself, so the layered image must equal the flat Normal image of the same document at every camera, outline, hidden-guide
//! and transparency-checker setting. Real Metal; a missing adapter is a test FAILURE, never a skip.
use crate::compositor::object_compositor::ObjectOutputSpec;
use crate::compositor::scene_effect_scene_join_stack::{select_composition_path, PathSelection};
use crate::compositor::Compositor;
use crate::history::NativeObjectHistory;
use crate::object_document::FrameScopeKind;
use crate::object_frame_packet::{prepare_object_frame, ObjectFrameSelector};
use crate::object_render_scene::{
    prepare_object_composition, prepare_object_navigated_viewport_composition,
    prepare_object_paint_plan, prepare_object_render_scene, ObjectCompositionWitness,
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

fn vector(id: &str, r: [i32; 4], fill: [f64; 3], shadow: bool) -> Value {
    let mut record = json!({"schemaVersion":1,"family":"editable-vector-path",
        "target":{"contextId":"scene-root","layerUid":"L0","strokeId":id,"frameScope":{"kind":"authored","frame":0}},
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

/// A mask record on L0: an `add` rectangle with the given feather (the layer's masks combine into one silhouette).
fn mask(rect: [i32; 4], feather: f64) -> Value {
    let mut record = vector("mask", rect, [1., 1., 1.], false);
    record["vectorExt"] = json!({"isMask":true,"maskMode":"add","maskFeather":feather});
    record
}

fn timing() -> Value {
    json!({"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing","contextId":"scene-root",
        "keys":[{"frame":0}],"samples":[]})
}

/// Red shadow-tagged square at 40..56, green square at 8..24 / 40..56, blue square at 8..24 / 8..24, all on L0, which
/// takes `own` as extra layer fields (a Multiply blend or an effect stack), an optional mask and an optional empty
/// layer `E1` ABOVE it with `top` fields (an adjustment layer).
fn document(own: Value, mask_record: Option<Value>, top: Option<Value>) -> NativeObjectHistory {
    let mut objects = vec![
        vector("plain", [8, 8, 24, 24], [0., 0., 1.], false),
        vector("guide", [40, 8, 56, 24], [1., 0., 0.], true),
        vector("kept", [8, 40, 24, 56], [0., 1., 0.], false),
    ];
    objects.extend(mask_record);
    let elements: Vec<Value> = objects
        .iter()
        .enumerate()
        .map(|(i, o)| {
            json!({"layerUid":"L0","strokeId":o["target"]["strokeId"],"naturalPosition":i,"order":{}})
        })
        .collect();
    let mut layer = json!({"layerUid":"L0","timing":timing()});
    for (key, value) in own.as_object().into_iter().flatten() {
        layer[key] = value.clone();
    }
    let mut layers = vec![layer];
    let mut order = vec![json!({"layerUid":"L0","naturalPosition":0,"order":{}})];
    if let Some(top) = top {
        let mut upper = json!({"layerUid":"E1","timing":timing()});
        for (key, value) in top.as_object().into_iter().flatten() {
            upper[key] = value.clone();
        }
        layers.push(upper);
        order.push(json!({"layerUid":"E1","naturalPosition":1,"order":{}}));
    }
    let drawing = json!({"format":"nemo.native-object-document","formatVersion":2,"totalFrames":2,
        "layers":layers,"objects":objects,
        "compositionOrder":{"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
            "contexts":[{"contextId":"scene-root","layers":order,
                "scopes":[{"frameScope":{"kind":"authored","frame":0},"elements":elements}]}],
            "migration":"explicit-plan-required","referencePolicy":"unresolved"}});
    NativeObjectHistory::from_project_file(
        "layered-view",
        &serde_json::to_vec(&json!({"format":"nemo.native-project-document","formatVersion":1,
            "project":{"canvasW":64,"canvasH":64,"fps":24},"viewportReference":null,"assets":[],"drawing":drawing}))
        .unwrap(),
    )
    .unwrap()
}

/// The one layer blends with `blend` (a Multiply layer makes the scene layered, an absent blend keeps it flat).
fn guided(blend: Option<&str>) -> NativeObjectHistory {
    document(
        blend.map_or(json!({}), |mode| json!({"blendMode": mode})),
        None,
        None,
    )
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

fn viewport(s: &ObjectSnapshot, zoom: f64, pan: [f64; 2]) -> ObjectCompositionWitness {
    prepare_object_navigated_viewport_composition(&plan(s), &pins(s), None, false, zoom, pan)
        .unwrap()
}

/// Which compositor entry the production selection picks, and the pixels it produces.
fn draw(gpu: &mut Compositor, witness: &ObjectCompositionWitness) -> (bool, Vec<u8>) {
    let effects = witness.scene_effects().expect("production joins").clone();
    let output = ObjectOutputSpec::new(64, 64, MODE).unwrap();
    let (layered, result) = match select_composition_path(&effects, witness.plan()).unwrap() {
        PathSelection::Layered(stack) => {
            (true, gpu.compose_object_layered(witness, output, &stack))
        }
        PathSelection::Flat => (false, gpu.compose_object(witness, output)),
    };
    let bytes = gpu
        .readback_object_rgba8(&result.unwrap())
        .unwrap()
        .bytes()
        .to_vec();
    (layered, bytes)
}

fn at(bytes: &[u8], x: usize, y: usize) -> [u8; 4] {
    bytes[(y * 64 + x) * 4..][..4].try_into().unwrap()
}

/// The layered frame equals the flat one up to the blend pass's one level of premultiplied rounding on an edge.
fn assert_same_image(layered: &[u8], flat: &[u8], label: &str) {
    let worst = layered
        .iter()
        .zip(flat)
        .map(|(a, b)| a.abs_diff(*b))
        .max()
        .unwrap();
    assert!(
        worst <= 1,
        "{label}: layered vs flat, worst channel step {worst}"
    );
}

fn gpu() -> Compositor {
    Compositor::new().expect("actual headless GPU is required; never skip")
}

#[test]
fn the_stored_camera_wraps_every_layered_layer_exactly_once() {
    let mut gpu = gpu();
    let flat_owner = guided(None);
    let layered_owner = guided(Some("multiply"));
    let flat_snapshot = flat_owner.acquire_snapshot(0).unwrap();
    let layered_snapshot = layered_owner.acquire_snapshot(0).unwrap();
    for (zoom, pan) in [(2.0, [0., 0.]), (1.5, [6., -4.]), (0.5, [0., 0.])] {
        let (flat_layered, flat) = draw(&mut gpu, &viewport(&flat_snapshot, zoom, pan));
        let (layered, image) = draw(&mut gpu, &viewport(&layered_snapshot, zoom, pan));
        assert!(
            !flat_layered && layered,
            "zoom {zoom}: flat control vs layered stack"
        );
        assert_same_image(&image, &flat, &format!("zoom {zoom} pan {pan:?}"));
    }
    // Literal anchors: zoom 2 about the centre (32, 32) maps x to 2x - 32, so the blue square 8..24 covers 0..16, the green
    // square 8..24 x 40..56 covers 0..16 x 48..64 and the red guide 40..56 x 8..24 covers 48..64 x 0..16.
    let (_, image) = draw(&mut gpu, &viewport(&layered_snapshot, 2.0, [0., 0.]));
    assert_eq!(
        at(&image, 10, 10),
        [0, 0, 255, 255],
        "inside the zoomed blue square"
    );
    assert_eq!(
        at(&image, 20, 20),
        [255, 255, 255, 255],
        "outside every zoomed square"
    );
    assert_eq!(
        at(&image, 8, 56),
        [0, 255, 0, 255],
        "inside the zoomed green square"
    );
    assert_eq!(
        at(&image, 56, 8),
        [255, 0, 0, 255],
        "inside the zoomed red guide"
    );
}

#[test]
fn the_current_frame_outline_is_drawn_per_layer_and_matches_the_flat_outline() {
    let mut gpu = gpu();
    let flat_owner = guided(None);
    let layered_owner = guided(Some("multiply"));
    let (flat_snapshot, layered_snapshot) = (
        flat_owner.acquire_snapshot(0).unwrap(),
        layered_owner.acquire_snapshot(0).unwrap(),
    );
    let (_, filled) = draw(&mut gpu, &viewport(&layered_snapshot, 1., [0., 0.]));
    let (_, flat) = draw(
        &mut gpu,
        &viewport(&flat_snapshot, 1., [0., 0.])
            .with_viewport_outline(1.)
            .unwrap(),
    );
    let (layered, image) = draw(
        &mut gpu,
        &viewport(&layered_snapshot, 1., [0., 0.])
            .with_viewport_outline(1.)
            .unwrap(),
    );
    assert!(layered);
    assert_same_image(&image, &flat, "outline");
    assert_ne!(image, filled, "the outline replaces the fills");
    assert_eq!(
        at(&image, 16, 16),
        [255, 255, 255, 255],
        "no fill inside an outlined square"
    );
}

#[test]
fn hidden_shadow_guides_leave_the_viewport_layered_frame_and_never_the_export() {
    let mut gpu = gpu();
    let owner = guided(Some("multiply"));
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let (_, shown) = draw(&mut gpu, &viewport(&snapshot, 1., [0., 0.]));
    assert_eq!(
        at(&shown, 48, 16),
        [255, 0, 0, 255],
        "the guide is drawn by default"
    );
    let (layered, hidden) = draw(
        &mut gpu,
        &viewport(&snapshot, 1., [0., 0.])
            .with_viewport_hidden_shadow_guides()
            .unwrap(),
    );
    assert!(layered);
    assert_eq!(
        at(&hidden, 48, 16),
        [255, 255, 255, 255],
        "the guide is hidden"
    );
    assert_eq!(
        at(&hidden, 16, 16),
        [0, 0, 255, 255],
        "the plain square stays"
    );
    assert_eq!(
        at(&hidden, 16, 48),
        [0, 255, 0, 255],
        "the untagged vector stays"
    );
    let flat_owner = guided(None);
    let flat_snapshot = flat_owner.acquire_snapshot(0).unwrap();
    let (_, flat_hidden) = draw(
        &mut gpu,
        &viewport(&flat_snapshot, 1., [0., 0.])
            .with_viewport_hidden_shadow_guides()
            .unwrap(),
    );
    assert_same_image(&hidden, &flat_hidden, "hidden guides");
    let export = prepare_object_composition(&plan(&snapshot), &pins(&snapshot)).unwrap();
    let (export_layered, exported) = draw(&mut gpu, &export);
    assert!(export_layered);
    assert_eq!(
        at(&exported, 48, 16),
        [255, 0, 0, 255],
        "an export keeps its guides"
    );
}

#[test]
fn the_transparency_checker_sits_beneath_the_layered_artwork() {
    let mut gpu = gpu();
    let flat_owner = guided(None);
    let layered_owner = guided(Some("multiply"));
    let (flat_snapshot, layered_snapshot) = (
        flat_owner.acquire_snapshot(0).unwrap(),
        layered_owner.acquire_snapshot(0).unwrap(),
    );
    let with_checker = |s: &ObjectSnapshot| {
        viewport(s, 1., [0., 0.])
            .with_viewport_checker((64, 64), 1.)
            .unwrap()
    };
    let (_, flat) = draw(&mut gpu, &with_checker(&flat_snapshot));
    let (layered, image) = draw(&mut gpu, &with_checker(&layered_snapshot));
    assert!(layered);
    assert_same_image(&image, &flat, "checker");
    let (_, plain) = draw(&mut gpu, &viewport(&layered_snapshot, 1., [0., 0.]));
    assert_ne!(image, plain, "the checker really paints under the artwork");
    assert_eq!(
        at(&image, 16, 16),
        [0, 0, 255, 255],
        "the artwork stays above the checker"
    );
}

fn export_of(owner: &NativeObjectHistory) -> (bool, Vec<u8>, ObjectCompositionWitness) {
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let witness = prepare_object_composition(&plan(&snapshot), &pins(&snapshot)).unwrap();
    let mut gpu = gpu();
    let (layered, bytes) = draw(&mut gpu, &witness);
    (layered, bytes, witness)
}

#[test]
fn a_layer_effect_runs_on_the_layers_own_alpha_and_both_doors_agree() {
    let mut gpu = gpu();
    let invert = json!({"effects": [{"type": "invert", "enabled": true}]});
    let owner = document(invert, None, None);
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let (layered, viewport) = draw(&mut gpu, &viewport(&snapshot, 1., [0., 0.]));
    assert!(layered, "an enabled effect selects the layer stack");
    // Invert of the opaque squares; the transparent rest of the layer stays transparent over the white base.
    assert_eq!(
        at(&viewport, 16, 16),
        [255, 255, 0, 255],
        "blue inverts to yellow"
    );
    assert_eq!(
        at(&viewport, 16, 48),
        [255, 0, 255, 255],
        "green inverts to magenta"
    );
    assert_eq!(
        at(&viewport, 48, 16),
        [0, 255, 255, 255],
        "red inverts to cyan"
    );
    assert_eq!(
        at(&viewport, 40, 40),
        [255, 255, 255, 255],
        "no layer pixel, no inversion"
    );
    let (export_layered, export, _) = export_of(&owner);
    assert!(export_layered);
    assert_eq!(export, viewport, "the export is the viewport image");
    // Negative control: the same document without the effect is flat and un-inverted.
    let (plain_layered, plain, _) = export_of(&guided(None));
    assert!(!plain_layered);
    assert_eq!(at(&plain, 16, 16), [0, 0, 255, 255]);
}

#[test]
fn an_effect_layer_grades_everything_below_it_and_nothing_above() {
    let mut gpu = gpu();
    let owner = document(
        json!({}),
        None,
        Some(json!({"isEffectLayer": true, "effects": [{"type": "invert", "enabled": true}]})),
    );
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let (layered, image) = draw(&mut gpu, &viewport(&snapshot, 1., [0., 0.]));
    assert!(layered);
    assert_eq!(
        at(&image, 16, 16),
        [255, 255, 0, 255],
        "the layer below is inverted"
    );
    assert_eq!(at(&image, 16, 48), [255, 0, 255, 255]);
    // The base colour is applied after every layer, so the empty canvas is never graded.
    assert_eq!(at(&image, 40, 40), [255, 255, 255, 255]);
    // A disabled stack is a pass-through.
    let off = document(
        json!({}),
        None,
        Some(json!({"isEffectLayer": true, "effects": [{"type": "invert", "enabled": false}]})),
    );
    let off_snapshot = off.acquire_snapshot(0).unwrap();
    let (_, plain) = draw(&mut gpu, &viewport(&off_snapshot, 1., [0., 0.]));
    assert_eq!(at(&plain, 16, 16), [0, 0, 255, 255]);
}

#[test]
fn a_custom_shader_refuses_the_frame_typed_before_any_texture() {
    let mut gpu = gpu();
    let owner = document(
        json!({"effects": [{"type": "custom:glitch", "enabled": true}]}),
        None,
        None,
    );
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let witness = viewport(&snapshot, 1., [0., 0.]);
    let effects = witness.scene_effects().unwrap().clone();
    let PathSelection::Layered(stack) = select_composition_path(&effects, witness.plan()).unwrap()
    else {
        panic!("a custom effect still selects the layer stack");
    };
    let refusal = gpu
        .compose_object_layered(
            &witness,
            ObjectOutputSpec::new(64, 64, MODE).unwrap(),
            &stack,
        )
        .err()
        .expect("custom shaders stay with their own owner");
    assert!(
        refusal
            .to_string()
            .contains("layer effect stack unavailable"),
        "{refusal}"
    );
}

#[test]
fn the_mask_feather_softens_the_silhouette_edge_symmetrically() {
    let mut gpu = gpu();
    // The add mask covers x 0..16 and y 0..32: the blue square (8..24) keeps its left half.
    let hard = document(json!({}), Some(mask([0, 0, 16, 32], 0.)), None);
    let soft = document(json!({}), Some(mask([0, 0, 16, 32], 4.)), None);
    let hard_snapshot = hard.acquire_snapshot(0).unwrap();
    let soft_snapshot = soft.acquire_snapshot(0).unwrap();
    let (hard_layered, hard_image) = draw(&mut gpu, &viewport(&hard_snapshot, 1., [0., 0.]));
    let (soft_layered, soft_image) = draw(&mut gpu, &viewport(&soft_snapshot, 1., [0., 0.]));
    assert!(
        hard_layered && soft_layered,
        "a mask selects the layer stack with or without feather"
    );
    assert_eq!(at(&hard_image, 12, 16), [0, 0, 255, 255], "inside the mask");
    assert_eq!(at(&hard_image, 15, 16), [0, 0, 255, 255]);
    assert_eq!(
        at(&hard_image, 16, 16),
        [255, 255, 255, 255],
        "outside the hard edge"
    );
    assert_eq!(at(&hard_image, 20, 16), [255, 255, 255, 255]);
    // Feathered: the edge at x = 16 is a Gaussian ramp. Pixels 15 and 16 sit symmetrically about it, so their coverages
    // add up to one (blue over white leaves red = 255 * (1 - coverage)); the ramp reaches both sides.
    let (left, right) = (at(&soft_image, 15, 16), at(&soft_image, 16, 16));
    assert!(
        left[0] > 0 && right[0] < 255,
        "the edge is soft: {left:?} {right:?}"
    );
    assert!(
        (i32::from(left[0]) + i32::from(right[0]) - 255).abs() <= 2,
        "symmetric about the edge: {left:?} {right:?}"
    );
    assert_eq!(
        at(&soft_image, 8, 16),
        [0, 0, 255, 255],
        "deep inside the mask is untouched"
    );
    assert_eq!(
        at(&soft_image, 30, 16),
        [255, 255, 255, 255],
        "far outside stays clear"
    );
    let (export_layered, export, _) = export_of(&soft);
    assert!(export_layered);
    assert_eq!(export, soft_image, "the export is the viewport image");
}
