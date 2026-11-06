//! #2257 composition properties sampled at the frame. (1) An EXPRESSED `matteOn` (baseline `matteOnAt` reads `valueAtFrame`, which runs the expression): the owner-held
//! packet carries the expression port's value of the layer at the frame, the join reads it (`>= 50` is matted) and the
//! layered compositor draws the matte, or the layer whole, accordingly. A packet built without the owner's port keeps
//! the typed refusal (the raw track is never substituted). Real documents, real Metal; viewport = export.
use super::gpu;
use crate::compositor::object_compositor::ObjectOutputSpec;
use crate::compositor::scene_effect_scene_join_stack::{select_composition_path, PathSelection};
use crate::history::NativeObjectHistory;
use crate::object_codec::decode_storage_project;
use crate::object_document::{FrameScopeKind, ProjectEnvelope, ProjectMetadata};
use crate::object_expression_runtime::prepare_object_motion_exposure_frame_for_owner;
use crate::object_frame_packet::{prepare_object_exposure_frame, ObjectFrameSelector};
use crate::object_render_scene::{
    prepare_object_composition, prepare_object_paint_plan, prepare_object_render_scene,
};
use serde_json::{json, Value};

#[path = "object_raster_asset_commands_support.rs"]
mod support;

const RED: [u8; 4] = [255, 0, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];
const WHITE: [u8; 4] = [255; 4];

fn corner(x: f64, y: f64) -> Value {
    json!({"point":{"x":x,"y":y},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}})
}

fn record(layer: &str, (x0, y0, x1, y1): (f64, f64, f64, f64), rgb: [f64; 3], frame: u32) -> Value {
    let mut record = support::legacy_record(layer, "s");
    record["target"] = json!({"contextId":"scene-root","layerUid":layer,"strokeId":"s",
        "frameScope":{"kind":"authored","frame":frame}});
    record["fill"] = json!({"kind":"solid","r":rgb[0],"g":rgb[1],"b":rgb[2],"a":1});
    record["geometry"]["segments"] = json!([corner(x0, y0), corner(x1, y0), corner(x1, y1), corner(x0, y1)]);
    record
}

/// T (red 20..100) is matted by S (blue 40..60) while `matteOn` (the expression) reads >= 50.
fn document(code: &str) -> Value {
    let timing = json!({"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing",
        "contextId":"scene-root","keys":[{"frame":0}],"samples":[]});
    let layers = json!([
        {"layerUid":"T","timing":timing,"matteMode":"alpha","matteSourceLayerUid":"S"},
        {"layerUid":"S","timing":timing}]);
    let elements = |layer: &str| json!({"layerUid":layer,"strokeId":"s","naturalPosition":0,"order":{}});
    let frames: Vec<Value> = (0..=4u32)
        .map(|frame| json!({"frameScope":{"kind":"authored","frame":frame},"elements":[elements("T"), elements("S")]}))
        .collect();
    let mut layers = layers;
    for layer in layers.as_array_mut().unwrap() {
        layer["timing"]["keys"] = json!([{"frame":0}]);
    }
    let mut objects = Vec::new();
    for frame in 0..=4u32 {
        objects.push(record("T", (20., 20., 100., 100.), [1., 0., 0.], frame));
        objects.push(record("S", (40., 40., 60., 60.), [0., 0., 1.], frame));
    }
    json!({"format":"nemo.native-object-document","formatVersion":2,"totalFrames":8,
        "layers":layers,"objects":objects,
        "propertyHolders":[{"target":{"contextId":"scene-root","layerUid":"T"},
            "properties":[{"property":"matteOn","static":[100],"expression":{"enabled":true,"code":code}}]}],
        "compositionOrder":{"format":"nemo.native-object-order-plan","formatVersion":1,
            "documentFormatVersion":2,"migration":"explicit-plan-required","referencePolicy":"unresolved",
            "contexts":[{"contextId":"scene-root",
                "layers":[{"layerUid":"T","naturalPosition":0,"order":{}},{"layerUid":"S","naturalPosition":1,"order":{}}],
                "scopes":frames}]}})
}

fn owner(doc: &Value) -> NativeObjectHistory {
    let document = decode_storage_project(doc.to_string().as_bytes()).unwrap();
    let project = ProjectEnvelope::no_media(ProjectMetadata::new(160, 120, 24.).unwrap(), document);
    NativeObjectHistory::from_admitted_project("expressed-matte", project).unwrap()
}

fn selector(s: &crate::revision::ObjectSnapshot, frame: u32) -> ObjectFrameSelector<'_> {
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

/// The export-door image of the owner-held packet at `frame` and whether the layer stack painted it.
fn owner_held(doc: &Value, frame: u32) -> (bool, Vec<u8>) {
    let history = owner(doc);
    let snapshot = history.acquire_snapshot(history.content_revision()).unwrap();
    let pins = selector(&snapshot, frame);
    let mut retained = None;
    let packet = prepare_object_motion_exposure_frame_for_owner(&history, &snapshot, &pins, &mut retained)
        .expect("the owner's expression port evaluates the matte switch");
    assert!(retained.is_none());
    let scene = prepare_object_render_scene(&packet, &pins).unwrap();
    let plan = prepare_object_paint_plan(&scene, &pins).unwrap();
    let witness = prepare_object_composition(&plan, &pins).expect("the evaluated matteOn is joined");
    let effects = witness.scene_effects().unwrap().clone();
    let output = ObjectOutputSpec::new(160, 120, "project-srgb-normal-rgba8-area").unwrap();
    let mut gpu = gpu();
    let (layered, result) = match select_composition_path(&effects, witness.plan()).unwrap() {
        PathSelection::Layered(stack) => (true, gpu.compose_object_layered(&witness, output, &stack)),
        PathSelection::Flat => (false, gpu.compose_object(&witness, output)),
    };
    (layered, gpu.readback_object_rgba8(&result.unwrap()).unwrap().bytes().to_vec())
}

fn pixel(bytes: &[u8], x: usize, y: usize) -> [u8; 4] {
    bytes[(y * 160 + x) * 4..][..4].try_into().unwrap()
}

#[test]
fn an_expressed_matte_switch_is_evaluated_at_the_frame_not_read_from_the_raw_value() {
    // Raw static value 100 (matted) on every frame; the expression `frame * 30` reads 0, 30, 60: off, off, on.
    let doc = document("[frame * 30]");
    let (layered, matted) = owner_held(&doc, 2);
    assert!(layered);
    assert_eq!(pixel(&matted, 50, 50), RED, "matted: T shows inside the matte source");
    assert_eq!(pixel(&matted, 30, 30), WHITE, "matted: nothing of T outside it");
    for frame in [0, 1] {
        let (_, whole) = owner_held(&doc, frame);
        assert_eq!(pixel(&whole, 30, 30), RED, "frame {frame}: the matte is off, T is whole");
        assert_eq!(pixel(&whole, 50, 50), BLUE, "frame {frame}: the source paints as its own layer, above T");
    }
}

#[test]
fn the_matte_switch_value_is_the_first_component_and_50_is_matted() {
    let (_, at_threshold) = owner_held(&document("[frame * 25]"), 2);
    assert_eq!(pixel(&at_threshold, 30, 30), WHITE, "50 is matted");
    let (_, below) = owner_held(&document("[frame * 25 - 1]"), 2);
    assert_eq!(pixel(&below, 30, 30), RED, "49 is not");
}

#[test]
fn a_packet_without_the_owners_expression_port_refuses_the_expressed_switch() {
    let history = owner(&document("[frame * 30]"));
    let snapshot = history.acquire_snapshot(history.content_revision()).unwrap();
    let pins = selector(&snapshot, 2);
    // The strict raw packet carries no evaluated value and refuses an enabled expression outright; the raw 100 is
    // never substituted for it.
    assert!(prepare_object_exposure_frame(&snapshot, &pins).is_err());
}

// ---- (2) the hold-only blend keys (baseline `layerBlendModeAt`) ------------------------------------------------------

/// Red `base` (20..100) under blue `top` (60..140 x 40..100) whose blend is keyed by `keys` (`[(frame, mode)]`).
fn keyed_blend(keys: &[(u32, &str)]) -> Value {
    let timing = json!({"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing",
        "contextId":"scene-root","keys":[{"frame":0}],"samples":[]});
    let keys: Vec<Value> = keys.iter().map(|(frame, mode)| json!({"frame": frame, "mode": mode})).collect();
    let elements = |layer: &str| json!({"layerUid":layer,"strokeId":"s","naturalPosition":0,"order":{}});
    let scopes: Vec<Value> = (0..=4u32)
        .map(|frame| json!({"frameScope":{"kind":"authored","frame":frame},"elements":[elements("base"), elements("top")]}))
        .collect();
    let mut objects = Vec::new();
    for frame in 0..=4u32 {
        objects.push(record("base", (20., 20., 100., 100.), [1., 0., 0.], frame));
        objects.push(record("top", (60., 40., 140., 100.), [0., 0., 1.], frame));
    }
    json!({"format":"nemo.native-object-document","formatVersion":2,"totalFrames":8,
        "layers":[{"layerUid":"base","timing":timing},{"layerUid":"top","timing":timing,"blendKeys":keys}],
        "objects":objects,
        "compositionOrder":{"format":"nemo.native-object-order-plan","formatVersion":1,
            "documentFormatVersion":2,"migration":"explicit-plan-required","referencePolicy":"unresolved",
            "contexts":[{"contextId":"scene-root",
                "layers":[{"layerUid":"base","naturalPosition":0,"order":{}},{"layerUid":"top","naturalPosition":1,"order":{}}],
                "scopes":scopes}]}})
}

/// The export-door image of the raw packet at `frame` and whether the layer stack painted it.
fn raw_frame(doc: &Value, frame: u32) -> (bool, Vec<u8>) {
    let history = owner(doc);
    let snapshot = history.acquire_snapshot(history.content_revision()).unwrap();
    let pins = selector(&snapshot, frame);
    let packet = prepare_object_exposure_frame(&snapshot, &pins).unwrap();
    let scene = prepare_object_render_scene(&packet, &pins).unwrap();
    let plan = prepare_object_paint_plan(&scene, &pins).unwrap();
    let witness = prepare_object_composition(&plan, &pins).unwrap();
    let effects = witness.scene_effects().unwrap().clone();
    let output = ObjectOutputSpec::new(160, 120, "project-srgb-normal-rgba8-area").unwrap();
    let mut gpu = gpu();
    let (layered, result) = match select_composition_path(&effects, witness.plan()).unwrap() {
        PathSelection::Layered(stack) => (true, gpu.compose_object_layered(&witness, output, &stack)),
        PathSelection::Flat => (false, gpu.compose_object(&witness, output)),
    };
    (layered, gpu.readback_object_rgba8(&result.unwrap()).unwrap().bytes().to_vec())
}

const BLACK: [u8; 4] = [0, 0, 0, 255];

#[test]
fn a_keyed_blend_mode_holds_between_its_keys_and_the_layer_stack_draws_it() {
    // Normal from frame 0, Multiply from frame 2: red x blue is black where they overlap, blue stays blue elsewhere.
    let doc = keyed_blend(&[(0, "normal"), (2, "multiply")]);
    for (frame, overlap) in [(0, BLUE), (1, BLUE), (2, BLACK), (3, BLACK), (4, BLACK)] {
        let (layered, image) = raw_frame(&doc, frame);
        assert_eq!(layered, frame >= 2, "frame {frame}: a blended layer selects the layer stack, Normal stays flat");
        assert_eq!(pixel(&image, 80, 60), overlap, "frame {frame}: overlap");
        assert_eq!(pixel(&image, 120, 60), BLUE, "frame {frame}: blue over white is blue in either mode");
        assert_eq!(pixel(&image, 30, 30), RED, "frame {frame}: the base layer alone");
    }
}

#[test]
fn before_its_first_key_a_keyed_layer_holds_the_first_keys_mode() {
    let doc = keyed_blend(&[(2, "multiply")]);
    let (layered, image) = raw_frame(&doc, 0);
    assert!(layered);
    assert_eq!(pixel(&image, 80, 60), BLACK, "frame 0 is before the only key: its mode holds");
}

#[test]
fn the_stored_blend_is_ignored_while_keys_exist_and_keys_equal_the_static_blend_they_replace() {
    let mut keyed = keyed_blend(&[(0, "normal")]);
    keyed["layers"][1]["blendMode"] = json!("multiply");
    let (layered, image) = raw_frame(&keyed, 3);
    assert!(!layered, "the key (Normal) wins over the stored Multiply");
    assert_eq!(pixel(&image, 80, 60), BLUE);
    let mut plain = keyed_blend(&[(0, "multiply")]);
    let (_, with_key) = raw_frame(&plain, 3);
    plain["layers"][1].as_object_mut().unwrap().remove("blendKeys");
    plain["layers"][1]["blendMode"] = json!("multiply");
    let (_, stored) = raw_frame(&plain, 3);
    assert_eq!(with_key, stored, "one key of Multiply is the stored Multiply");
}
