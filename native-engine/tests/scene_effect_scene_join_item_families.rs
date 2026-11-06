//! #2257 item-level effects on the families an item stack used to be refused on: the copies of a duplicator and the
//! inner records of a placed component. The baseline copies a stroke dict with its `effects` (`cloneStrokeForTransform`,
//! `applyLayerDuplicator`), so every copy is its own effected item graded on its own box; an inner component record
//! carries its `effects` the same way. Real documents, real Metal, viewport = export = PNG. A missing adapter is a
//! test FAILURE, never a skip. The expected pixels derive from the same document drawn WITHOUT the stack (the flat
//! control): the invert shader maps each colour it touches to a hand-computed colour and nothing else moves.
use super::{gpu, run_at, Door};
use crate::compositor::object_compositor::layered::LayerContext;
use crate::compositor::scene_effect_scene_join_stack::{select_composition_path, PathSelection};
use crate::history::NativeObjectHistory;
use crate::object_codec::decode_storage_project;
use crate::object_document::{FrameScopeKind, ProjectEnvelope, ProjectMetadata};
use crate::object_frame_packet::{prepare_object_exposure_frame, ObjectFrameSelector};
use crate::object_render_scene::{
    prepare_object_composition, prepare_object_paint_plan, prepare_object_render_scene,
    ObjectCompositionWitness,
};
use serde_json::{json, Value};

#[path = "object_raster_asset_commands_support.rs"]
mod support;

const RED: [u8; 4] = [255, 0, 0, 255];
const CYAN: [u8; 4] = [0, 255, 255, 255];
const GREEN: [u8; 4] = [0, 255, 0, 255];
const WHITE: [u8; 4] = [255; 4];

fn timing() -> Value {
    json!({"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing",
        "contextId":"scene-root","keys":[{"frame":0}],"samples":[]})
}

fn corner(x: f64, y: f64) -> Value {
    json!({"point":{"x":x,"y":y},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}})
}

fn invert() -> Value {
    json!([{"type": "invert", "enabled": true}])
}

fn rect_record(context: &str, layer: &str, id: &str, (x0, y0, x1, y1): (f64, f64, f64, f64), rgb: [f64; 3]) -> Value {
    let mut record = support::legacy_record(layer, id);
    record["target"] = json!({"contextId":context,"layerUid":layer,"strokeId":id,
        "frameScope":{"kind":"authored","frame":0}});
    record["fill"] = json!({"kind":"solid","r":rgb[0],"g":rgb[1],"b":rgb[2],"a":1});
    record["geometry"]["segments"] =
        json!([corner(x0, y0), corner(x1, y0), corner(x1, y1), corner(x0, y1)]);
    record
}

fn order(layer: &str, strokes: &[&str]) -> Value {
    let elements: Vec<_> = strokes
        .iter()
        .enumerate()
        .map(|(i, s)| json!({"layerUid":layer,"strokeId":s,"naturalPosition":i,"order":{}}))
        .collect();
    json!({"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
        "migration":"explicit-plan-required","referencePolicy":"unresolved",
        "contexts":[{"contextId":"scene-root","layers":[{"layerUid":layer,"naturalPosition":0,"order":{}}],
            "scopes":[{"frameScope":{"kind":"authored","frame":0},"elements":elements}]}]})
}

fn drawing(layer: &str, objects: Vec<Value>, strokes: &[&str], tables: Value) -> Value {
    json!({"format":"nemo.native-object-document","formatVersion":2,"totalFrames":8,
        "layers":[{"layerUid":layer,"timing":timing()}],"objects":objects,
        "compositionOrder":order(layer, strokes),"componentTables":tables})
}

fn owner_of(name: &str, doc: Value, canvas: (u32, u32)) -> NativeObjectHistory {
    let document = decode_storage_project(doc.to_string().as_bytes()).unwrap();
    let project = ProjectEnvelope::no_media(
        ProjectMetadata::new(canvas.0, canvas.1, 24.).unwrap(),
        document,
    );
    NativeObjectHistory::from_admitted_project(name, project).unwrap()
}

/// A single host layer `D` whose one rectangle is duplicated three times in a row (grid 1x3, 40 apart).
fn duplicated(fx: Option<Value>) -> NativeObjectHistory {
    let mut record = rect_record("scene-root", "D", "src", (10., 20., 30., 40.), [1., 0., 0.]);
    if let Some(fx) = fx {
        record["effects"] = fx;
    }
    let tables = json!({"duplicators":{"format":"nemo.native-duplicator-set","formatVersion":1,
        "duplicators":[{"contextId":"scene-root","layerUid":"D",
            "duplicator":{"mode":"grid","rows":1,"cols":3,"spacingX":40}}],"effectorLayers":[]}});
    owner_of("dup-fx", drawing("D", vec![record], &["src"], tables), (160, 60))
}

/// A host layer `host` placing component `comp-a`: a red inner rectangle (`fx` on it) and a green one overlapping it.
fn placed(fx: Option<Value>) -> NativeObjectHistory {
    let mut red = rect_record("comp-a", "inner", "red", (8., 8., 40., 36.), [1., 0., 0.]);
    if let Some(fx) = fx {
        red["effects"] = fx;
    }
    let green = rect_record("comp-a", "inner", "green", (30., 20., 56., 44.), [0., 1., 0.]);
    let tables = json!({"components":{"format":"nemo.native-component-set","formatVersion":1,
        "components":[{"symbolId":"comp-a","totalFrames":4,
            "layers":[{"layerUid":"inner","timing":timing()}],"objects":[red, green]}],
        "instances":[{"contextId":"scene-root","layerUid":"host","symbolId":"comp-a",
            "symPlayMode":"single","symSingleFrame":0,"symMatrix":[1,0,0,1,0,0]}]}});
    let doc = drawing("host", Vec::new(), &[], tables);
    NativeObjectHistory::from_project_file(
        "comp-fx",
        &json!({"format":"nemo.native-project-document","formatVersion":1,
            "project":{"canvasW":64,"canvasH":48,"fps":24},"drawing":doc,
            "viewportReference":null,"assets":[]})
        .to_string()
        .into_bytes(),
    )
    .unwrap()
}

/// The production witness (the join included) of the head at frame 0, drawn through the exposure frame.
fn witness_of(owner: &NativeObjectHistory) -> ObjectCompositionWitness {
    let snapshot = owner.acquire_snapshot(owner.content_revision()).unwrap();
    let pins = ObjectFrameSelector {
        instance_id: snapshot.instance_id(),
        document_id: snapshot.document_id(),
        content_revision: snapshot.content_revision(),
        document_snapshot_id: snapshot.snapshot_id(),
        context_id: "scene-root",
        scope_kind: FrameScopeKind::Authored,
        frame: 0,
    };
    let packet = prepare_object_exposure_frame(&snapshot, &pins).unwrap();
    let scene = prepare_object_render_scene(&packet, &pins).unwrap();
    let plan = prepare_object_paint_plan(&scene, &pins).unwrap();
    prepare_object_composition(&plan, &pins).unwrap()
}

/// Whether the layer stack painted the frame, and the pixels all three doors agree on.
fn frame_of(owner: &NativeObjectHistory, dims: (u32, u32)) -> (bool, Vec<u8>) {
    let mut gpu = gpu();
    let frame = run_at(&mut gpu, dims, &witness_of(owner));
    let door = |door: &Door| match door {
        Door::Pixels(bytes) => bytes.clone(),
        Door::Refused(text) => panic!("the door refused: {text}"),
    };
    let viewport = door(&frame.viewport);
    assert_eq!(viewport, door(&frame.export), "viewport vs export");
    assert_eq!(Some(&viewport), frame.png.as_ref(), "viewport vs PNG");
    (frame.layered, viewport)
}

fn pixel(bytes: &[u8], width: u32, x: usize, y: usize) -> [u8; 4] {
    bytes[(y * width as usize + x) * 4..][..4].try_into().unwrap()
}

fn count(bytes: &[u8], colour: [u8; 4]) -> usize {
    bytes.chunks_exact(4).filter(|p| **p == colour).count()
}

#[test]
fn every_duplicator_copy_carries_its_sources_item_effect() {
    let (flat_layered, flat) = frame_of(&duplicated(None), (160, 60));
    assert!(!flat_layered, "no stack: the flat fast path");
    let (layered, graded) = frame_of(&duplicated(Some(invert())), (160, 60));
    assert!(layered, "an item stack selects the layer stack");
    // The control draws three red copies of the 20x20 source; the invert turns exactly those pixels cyan.
    let reds = count(&flat, RED);
    assert!(reds >= 3 * 20 * 20 - 8, "three copies drawn: {reds}");
    assert_eq!(count(&graded, CYAN), reds, "each red pixel becomes cyan");
    assert_eq!(count(&graded, RED), 0);
    for (g, f) in graded.chunks_exact(4).zip(flat.chunks_exact(4)) {
        let expected = if f == RED { CYAN } else { f.try_into().unwrap() };
        assert_eq!(g, expected);
    }
}

#[test]
fn each_duplicator_copy_is_its_own_run_on_its_own_box() {
    let owner = duplicated(Some(invert()));
    let witness = witness_of(&owner);
    let context = LayerContext::new(&witness).unwrap();
    let effects = witness.scene_effects().unwrap().clone();
    let PathSelection::Layered(stack) = select_composition_path(&effects, witness.plan()).unwrap() else {
        panic!("the item stack selects the layer stack");
    };
    let runs = crate::compositor::scene_effect_gpu_blend::items::runs(&context, 0, &stack.inputs()[0], (160, 60));
    assert_eq!(runs.len(), 3, "three copies, three effected runs");
    let boxes: Vec<(f32, f32, f32, f32)> = runs
        .iter()
        .map(|run| {
            assert_eq!(run.entries.len(), 1);
            run.effect.expect("the copy carries the source stack").1
        })
        .collect();
    // (x, y, width, height): each copy's own 20x20 box, 40 apart, never the union of the three.
    assert!(boxes.iter().all(|b| b.2 == 20. && b.3 == 20.), "{boxes:?}");
    let mut xs: Vec<f32> = boxes.iter().map(|b| b.0).collect();
    xs.sort_by(f32::total_cmp);
    assert_eq!((xs[1] - xs[0], xs[2] - xs[1]), (40., 40.), "{boxes:?}");
}

#[test]
fn a_disabled_duplicator_item_effect_keeps_the_flat_path() {
    let off = json!([{"type": "invert", "enabled": false}]);
    let (layered, plain) = frame_of(&duplicated(Some(off)), (160, 60));
    let (_, control) = frame_of(&duplicated(None), (160, 60));
    assert!(!layered);
    assert_eq!(plain, control);
}

#[test]
fn a_placed_component_inner_record_is_graded_alone_and_the_next_record_composites_above_it() {
    let dims = (64, 48);
    let (flat_layered, flat) = frame_of(&placed(None), dims);
    assert!(!flat_layered);
    let (layered, graded) = frame_of(&placed(Some(invert())), dims);
    assert!(layered, "a component item stack selects the layer stack");
    // Red (8..40 x 8..36) is inverted alone; the plain green rectangle (30..56 x 20..44) is composited above it.
    assert_eq!(pixel(&flat, 64, 12, 12), RED);
    assert_eq!(pixel(&graded, 64, 12, 12), CYAN);
    assert_eq!(pixel(&graded, 64, 35, 30), GREEN, "the overlap keeps the plain record on top, un-inverted");
    assert_eq!(pixel(&graded, 64, 50, 40), GREEN);
    assert_eq!(pixel(&graded, 64, 2, 2), WHITE);
    for (g, f) in graded.chunks_exact(4).zip(flat.chunks_exact(4)) {
        let expected = if f == RED { CYAN } else { f.try_into().unwrap() };
        assert_eq!(g, expected);
    }
}

/// The brush document's witness through the brush funnel, joined by production like every other.
fn brush_witness(owner: &NativeObjectHistory) -> ObjectCompositionWitness {
    let snapshot = owner.acquire_snapshot(owner.content_revision()).unwrap();
    let pins = ObjectFrameSelector {
        instance_id: snapshot.instance_id(),
        document_id: snapshot.document_id(),
        content_revision: snapshot.content_revision(),
        document_snapshot_id: snapshot.snapshot_id(),
        context_id: "scene-root",
        scope_kind: FrameScopeKind::Authored,
        frame: 0,
    };
    let packet = crate::object_frame_packet::prepare_object_frame(&snapshot, &pins).unwrap();
    let scene = super::docs::brush_scene(&packet, &pins).unwrap();
    let plan = prepare_object_paint_plan(&scene, &pins).unwrap();
    prepare_object_composition(&plan, &pins).unwrap()
}

#[test]
fn a_brush_stroke_stack_rides_its_anchor_contour_and_the_ink_draws_plain_as_in_the_baseline() {
    // Baseline: a brush's dabs and bitmap are separate items WITHOUT `effects`; only the (camouflaged) anchor stroke
    // carries the stack. Native: the anchor contour is its own effected run and the ink a plain run; the stack is
    // neither a refusal nor applied to the ink.
    let mut gpu = gpu();
    let fx = super::docs::brush_with_effects(None, None, Some(invert()));
    let witness = brush_witness(&fx);
    let context = LayerContext::new(&witness).unwrap();
    let effects = witness.scene_effects().unwrap().clone();
    let PathSelection::Layered(stack) = select_composition_path(&effects, witness.plan()).unwrap() else {
        panic!("the anchor's stack selects the layer stack");
    };
    let runs = crate::compositor::scene_effect_gpu_blend::items::runs(&context, 0, &stack.inputs()[0], (48, 48));
    let shape: Vec<(usize, bool)> = runs.iter().map(|run| (run.entries.len(), run.effect.is_some())).collect();
    assert_eq!(shape, vec![(1, true), (1, false)], "anchor graded, ink plain");
    let control = run_at(&mut gpu, (48, 48), &brush_witness(&super::docs::brush_with_effects(None, None, None)));
    let graded = run_at(&mut gpu, (48, 48), &witness);
    let (Door::Pixels(control), Door::Pixels(graded)) = (&control.viewport, &graded.viewport) else {
        panic!("both doors draw");
    };
    assert_eq!(graded.len(), control.len());
    assert!(graded != control, "the anchor contour is graded");
    let ink = pixel(control, 48, 24, 24);
    assert_eq!(pixel(graded, 48, 24, 24), ink, "the ink centre is the control's");
}
