//! #2257 item-level effects: an object that carries an enabled effect stack is isolated, graded on its own box and
//! composited back in authored order (baseline `paint_layer_with_element_effects`): consecutive plain items form one
//! run, an effected item is its own run. Real documents, real Metal, both doors (viewport, export, PNG), hand-computed
//! pixels. A missing adapter is a test FAILURE, never a skip.
use super::{gpu, run_at, witness_of_owner, Door};
use crate::history::NativeObjectHistory;
use serde_json::{json, Value};

#[path = "object_raster_asset_commands_support.rs"]
mod support;

const DIMS: (u32, u32) = (320, 180);
const RED: [u8; 4] = [255, 0, 0, 255];
const GREEN: [u8; 4] = [0, 255, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];
const CYAN: [u8; 4] = [0, 255, 255, 255];
const YELLOW: [u8; 4] = [255, 255, 0, 255];
const MAGENTA: [u8; 4] = [255, 0, 255, 255];
const WHITE: [u8; 4] = [255; 4];

type Bounds = (f64, f64, f64, f64);

fn timing() -> Value {
    json!({"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing",
        "contextId":"scene-root","keys":[{"frame":0}],"samples":[]})
}

/// One rectangle object; `effects` is its own item-level stack.
fn item(layer: &str, id: &str, (x0, y0, x1, y1): Bounds, rgb: [f64; 3], effects: Option<Value>) -> Value {
    let corner = |x: f64, y: f64| json!({"point":{"x":x,"y":y},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}});
    let mut record = json!({"schemaVersion":1,"family":"editable-vector-path",
        "target":support::target(layer, id),
        "geometry":{"kind":"cubic-path","closed":true,"coordinateSpace":"document-world","handleSpace":"relative",
            "segments":[corner(x0, y0), corner(x1, y0), corner(x1, y1), corner(x0, y1)]},
        "fill":{"kind":"solid","r":rgb[0],"g":rgb[1],"b":rgb[2],"a":1},
        "vector":{"stroke":null,"hasRealStroke":false,"strokeWidth":3,"strokeCap":"round","strokeJoin":"round","opacity":1}});
    if let Some(effects) = effects {
        record["effects"] = effects;
    }
    record
}

fn invert() -> Option<Value> {
    Some(json!([{"type": "invert", "enabled": true}]))
}

/// One layer bottom-up: uid, extra layer fields, items.
struct Layer {
    uid: &'static str,
    extra: Value,
    items: Vec<Value>,
}

fn layer(uid: &'static str, extra: Value, items: Vec<Value>) -> Layer {
    Layer { uid, extra, items }
}

fn document(layers: &[Layer]) -> Value {
    document_at(layers, 0)
}

/// The document authored at `frame` (its records, scope and layer timing key all sit there).
fn document_at(layers: &[Layer], frame: u32) -> Value {
    let (mut out, mut records, mut ordered, mut elements) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for (index, l) in layers.iter().enumerate() {
        let mut value = json!({"layerUid":l.uid,"name":l.uid,"visible":true,"locked":false,
            "color":"#5fa875","parentLayerUid":null,"timing":timing()});
        for (key, field) in l.extra.as_object().into_iter().flatten() {
            value[key] = field.clone();
        }
        value["timing"]["keys"] = json!([{"frame": frame}]);
        out.push(value);
        ordered.push(json!({"layerUid":l.uid,"naturalPosition":index,"order":{}}));
        for (n, record) in l.items.iter().enumerate() {
            elements.push(json!({"layerUid":l.uid,"strokeId":record["target"]["strokeId"],"naturalPosition":n,"order":{}}));
            let mut record = record.clone();
            record["target"]["frameScope"]["frame"] = json!(frame);
            records.push(record);
        }
    }
    json!({"format":"nemo.native-object-document","formatVersion":2,"totalFrames":8,
        "layers":out,"objects":records,"propertyHolders":[],
        "compositionOrder":{"format":"nemo.native-object-order-plan","formatVersion":1,
            "documentFormatVersion":2,"migration":"explicit-plan-required","referencePolicy":"unresolved",
            "contexts":[{"contextId":"scene-root","layers":ordered,
                "scopes":[{"frameScope":{"kind":"authored","frame":frame},"elements":elements}]}]}})
}

fn owner(doc: &Value) -> NativeObjectHistory {
    NativeObjectHistory::from_admitted_project("items", support::project(doc, support::images_only())).unwrap()
}

fn pixel(bytes: &[u8], x: usize, y: usize) -> [u8; 4] {
    bytes[(y * DIMS.0 as usize + x) * 4..][..4].try_into().unwrap()
}

/// Both doors and the decoded export PNG hold one image; returns it and whether the layer stack painted it.
fn frame_of(doc: &Value) -> (bool, Vec<u8>) {
    let mut gpu = gpu();
    let frame = run_at(&mut gpu, DIMS, &witness_of_owner(&owner(doc)));
    let door = |door: &Door| match door {
        Door::Pixels(bytes) => bytes.clone(),
        Door::Refused(text) => panic!("the door refused: {text}"),
    };
    let viewport = door(&frame.viewport);
    assert_eq!(viewport, door(&frame.export), "viewport vs export");
    assert_eq!(Some(&viewport), frame.png.as_ref(), "viewport vs PNG");
    (frame.layered, viewport)
}

/// A blue (20..60), a red (40..80, `fx`) and a green (60..100) square, in this authored order on one layer.
fn trio(fx: Option<Value>, extra: Value) -> Value {
    document(&[layer(
        "L",
        extra,
        vec![
            item("L", "a", (20., 20., 60., 60.), [0., 0., 1.], None),
            item("L", "b", (40., 40., 80., 80.), [1., 0., 0.], fx),
            item("L", "c", (60., 60., 100., 100.), [0., 1., 0.], None),
        ],
    )])
}

#[test]
fn an_effected_item_is_graded_alone_and_the_runs_composite_in_authored_order() {
    let (layered, image) = frame_of(&trio(invert(), json!({})));
    assert!(layered, "an item-level effect selects the layer stack");
    assert_eq!(pixel(&image, 30, 30), BLUE, "the run below the effected item");
    assert_eq!(pixel(&image, 50, 50), CYAN, "the inverted item covers the run below it");
    assert_eq!(pixel(&image, 45, 70), CYAN, "red inverts to cyan");
    assert_eq!(pixel(&image, 70, 70), GREEN, "the run above covers the inverted item");
    assert_eq!(pixel(&image, 90, 90), GREEN);
    assert_eq!(pixel(&image, 10, 10), WHITE, "nothing is graded outside the items");
    // Negative control: the same document without the stack is the flat sequence.
    let (plain_layered, plain) = frame_of(&trio(None, json!({})));
    assert!(!plain_layered);
    assert_eq!(pixel(&plain, 50, 50), RED);
    assert_eq!(pixel(&plain, 45, 70), RED);
    // A disabled stack is not an effect.
    let (off_layered, _) = frame_of(&trio(Some(json!([{"type": "invert", "enabled": false}])), json!({})));
    assert!(!off_layered);
}

#[test]
fn the_item_stack_runs_before_the_layer_stack() {
    let layer_invert = json!({"effects": [{"type": "invert", "enabled": true}]});
    let (layered, image) = frame_of(&trio(invert(), layer_invert));
    assert!(layered);
    assert_eq!(pixel(&image, 45, 70), RED, "item invert then layer invert gives red again");
    assert_eq!(pixel(&image, 50, 50), RED);
    assert_eq!(pixel(&image, 30, 30), YELLOW, "a plain item gets the layer invert only");
    assert_eq!(pixel(&image, 90, 90), MAGENTA);
    assert_eq!(pixel(&image, 10, 10), WHITE);
}

#[test]
fn a_folder_child_keeps_its_item_stacks() {
    let child = |extra: Value| {
        let mut layer = layer(
            "K",
            extra,
            vec![
                item("K", "a", (20., 20., 60., 60.), [0., 0., 1.], None),
                item("K", "b", (40., 40., 80., 80.), [1., 0., 0.], invert()),
            ],
        );
        layer.extra["parentLayerUid"] = json!("F");
        layer
    };
    let folder = layer("F", json!({"isNullLayer":true,"nullPos":[0,0],"isFolderLayer":true,"folderCollapsed":false,"effects":[]}), vec![]);
    let (layered, image) = frame_of(&document(&[child(json!({})), folder]));
    assert!(layered, "an item stack inside a folder needs the layer stack");
    assert_eq!(pixel(&image, 30, 30), BLUE);
    assert_eq!(pixel(&image, 50, 50), CYAN, "the child's inverted item covers its earlier item");
    assert_eq!(pixel(&image, 45, 70), CYAN);
    assert_eq!(pixel(&image, 10, 10), WHITE);
}

// ---- values against the baseline shaders, keyed parameters, matteOn ----

use crate::compositor::object_compositor::ObjectOutputSpec;
use crate::compositor::scene_effect_scene_join_stack::{select_composition_path, PathSelection};
use crate::compositor::Compositor;
use crate::object_document::FrameScopeKind;
use crate::object_frame_packet::{prepare_object_frame, ObjectFrameSelector};
use crate::object_render_scene::{
    prepare_object_composition, prepare_object_navigated_viewport_composition,
    prepare_object_paint_plan, prepare_object_render_scene, ObjectCompositionWitness,
};

fn pins_at<'a>(s: &'a crate::revision::ObjectSnapshot, frame: u32) -> ObjectFrameSelector<'a> {
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

fn plan_at(s: &crate::revision::ObjectSnapshot, frame: u32) -> crate::object_render_scene::ObjectPaintPlan {
    let selector = pins_at(s, frame);
    let packet = prepare_object_frame(s, &selector).unwrap();
    prepare_object_paint_plan(&prepare_object_render_scene(&packet, &selector).unwrap(), &selector).unwrap()
}

fn bytes_of(gpu: &mut Compositor, witness: &ObjectCompositionWitness) -> (bool, Vec<u8>) {
    let effects = witness.scene_effects().expect("production joins").clone();
    let output = ObjectOutputSpec::new(DIMS.0, DIMS.1, "project-srgb-normal-rgba8-area").unwrap();
    let (layered, result) = match select_composition_path(&effects, witness.plan()).unwrap() {
        PathSelection::Layered(stack) => (true, gpu.compose_object_layered(witness, output, &stack)),
        PathSelection::Flat => (false, gpu.compose_object(witness, output)),
    };
    (layered, gpu.readback_object_rgba8(&result.unwrap()).unwrap().bytes().to_vec())
}

/// The viewport and the export image of the document at `frame`; they must be one image.
fn at_frame(doc: &Value, frame: u32) -> (bool, Vec<u8>) {
    let history = owner(doc);
    let snapshot = history.acquire_snapshot(history.content_revision()).unwrap();
    let plan = plan_at(&snapshot, frame);
    let mut gpu = gpu();
    let viewport = prepare_object_navigated_viewport_composition(&plan, &pins_at(&snapshot, frame), None, false, 1., [0., 0.]).unwrap();
    let export = prepare_object_composition(&plan, &pins_at(&snapshot, frame)).unwrap();
    let (layered, image) = bytes_of(&mut gpu, &viewport);
    let (export_layered, exported) = bytes_of(&mut gpu, &export);
    assert_eq!((layered, &image), (export_layered, &exported), "viewport vs export at frame {frame}");
    (layered, image)
}

fn near(actual: [u8; 4], expected: [u8; 4], tolerance: u8, label: &str) {
    let worst = actual.iter().zip(expected).map(|(a, e)| a.abs_diff(e)).max().unwrap();
    assert!(worst <= tolerance, "{label}: {actual:?} vs {expected:?}");
}

/// One item `rgb` at `bounds` on a layer by itself, with the item's own `effects` and the layer's `extra`.
fn single(bounds: Bounds, rgb: [f64; 3], item_fx: Option<Value>, layer_fx: Value) -> Value {
    single_at(bounds, rgb, item_fx, layer_fx, 0)
}

fn single_at(bounds: Bounds, rgb: [f64; 3], item_fx: Option<Value>, layer_fx: Value, frame: u32) -> Value {
    document_at(&[layer("L", layer_fx, vec![item("L", "a", bounds, rgb, item_fx)])], frame)
}

#[test]
fn item_colour_adjust_gives_the_baseline_shader_values() {
    // color_adjust.wgsl: (c - 0.5) * (1 + contrast) + 0.5 + brightness, clamped, on the straight colour.
    let bright = json!([{"type": "colorAdjust", "enabled": true, "p1": 0.2}]);
    let (layered, image) = frame_of(&single((20., 20., 100., 100.), [1., 0., 0.], Some(bright), json!({})));
    assert!(layered);
    near(pixel(&image, 60, 60), [255, 51, 51, 255], 1, "brightness +0.2 on red");
    assert_eq!(pixel(&image, 10, 10), WHITE);
    let contrast = json!([{"type": "colorAdjust", "enabled": true, "p2": 0.5}]);
    let colour = [204. / 255., 51. / 255., 128. / 255.];
    let (_, image) = frame_of(&single((20., 20., 100., 100.), colour, Some(contrast), json!({})));
    // r: 0.3 * 1.5 + 0.5 = 0.95 -> 242; g: -0.3 * 1.5 + 0.5 = 0.05 -> 13; b: 0.002 * 1.5 + 0.5 -> 128.
    near(pixel(&image, 60, 60), [242, 13, 128, 255], 1, "contrast +0.5");
}

#[test]
fn the_layer_vignette_darkens_by_the_baseline_falloff() {
    // vignette.wgsl over the layer texture: d = distance(uv, centre) / 0.7071, start = radius,
    // darken = 1 - clamp((d - start) / (1 - start)) * strength.
    let vignette = json!({"effects": [{"type": "vignette", "enabled": true, "p1": 1.0, "p2": 0.4}]});
    let (layered, image) = frame_of(&single((0., 0., 320., 180.), [1., 0., 0.], None, vignette));
    assert!(layered);
    let expected = |x: f64, y: f64| {
        let (u, v) = ((x + 0.5) / 320., (y + 0.5) / 180.);
        let d = ((u - 0.5).powi(2) + (v - 0.5).powi(2)).sqrt() / 0.7071;
        let falloff = ((d - 0.4) / 0.6).clamp(0., 1.);
        (255. * (1. - falloff)).round() as u8
    };
    for (x, y) in [(160usize, 90usize), (80, 45), (10, 10), (300, 20), (2, 2)] {
        near(pixel(&image, x, y), [expected(x as f64, y as f64), 0, 0, 255], 2, &format!("vignette at {x},{y}"));
    }
    assert_eq!(pixel(&image, 160, 90), RED, "the centre is untouched");
}

/// Standard normal CDF (Abramowitz & Stegun 7.1.26).
fn phi(x: f64) -> f64 {
    let t = 1. / (1. + 0.327_591_1 * (x.abs() / 2f64.sqrt()));
    let poly = t * (0.254_829_592 + t * (-0.284_496_736 + t * (1.421_413_741 + t * (-1.453_152_027 + t * 1.061_405_429))));
    let erf = 1. - poly * (-(x * x) / 2.).exp();
    0.5 * (1. + if x >= 0. { erf } else { -erf })
}

#[test]
fn an_item_blur_is_a_gaussian_edge_ramp_of_the_baseline_sigma() {
    // blur.wgsl: sigma = radius / 3. A blue item over white: red = 255 * (1 - coverage), coverage = Phi(d / sigma).
    let blur = json!([{"type": "blur", "enabled": true, "p1": 9}]);
    let (layered, image) = frame_of(&single((100., 40., 220., 140.), [0., 0., 1.], Some(blur), json!({})));
    assert!(layered);
    for (x, distance_inside) in [(99usize, -0.5f64), (100, 0.5), (101, 1.5), (103, 3.5), (105, 5.5), (96, -3.5)] {
        let coverage = phi(distance_inside / 3.);
        let red = (255. * (1. - coverage)).round() as u8;
        near(pixel(&image, x, 90), [red, red, 255, 255], 5, &format!("ramp at x = {x}"));
    }
    assert_eq!(pixel(&image, 160, 90), BLUE, "deep inside the item");
    assert_eq!(pixel(&image, 60, 90), WHITE, "far outside");
}

#[test]
fn a_built_in_item_effect_equals_the_same_stack_on_that_item_alone() {
    // The built-in shaders are position independent (only custom WGSL reads the box), so B graded as an item beside a
    // far item A must be the pixels of B graded as the only item of its layer.
    for kind in ["halftone", "groundShadow", "contourBrut", "pixelate", "sepia"] {
        let stack = json!([{"type": kind, "enabled": true}]);
        let b = (200., 60., 280., 120.);
        let layer_level = document(&[layer("L", json!({"effects": stack}), vec![item("L", "b", b, [0.9, 0.3, 0.2], None)])]);
        let item_level = document(&[layer("L", json!({}), vec![
            item("L", "a", (10., 10., 40., 40.), [0., 0., 1.], None),
            item("L", "b", b, [0.9, 0.3, 0.2], Some(stack)),
        ])]);
        let (_, alone) = frame_of(&layer_level);
        let (_, beside) = frame_of(&item_level);
        let mut painted = 0;
        for y in 40..140 {
            for x in 180..320 {
                painted += usize::from(pixel(&alone, x, y) != WHITE);
                assert_eq!(pixel(&alone, x, y), pixel(&beside, x, y), "{kind} at {x},{y}");
            }
        }
        assert!(painted > 0, "{kind}: the stack drew something over B");
    }
}

#[test]
fn the_item_box_handed_to_an_effect_is_that_items_own_device_box() {
    use crate::compositor::object_compositor::layered::LayerContext;
    let history = owner(&document(&[layer("L", json!({}), vec![
        item("L", "a", (10., 10., 40., 40.), [0., 0., 1.], None),
        item("L", "b", (200., 60., 280., 120.), [0.9, 0.3, 0.2], invert()),
    ])]));
    let snapshot = history.acquire_snapshot(history.content_revision()).unwrap();
    let witness = prepare_object_composition(&plan_at(&snapshot, 0), &pins_at(&snapshot, 0)).unwrap();
    let context = LayerContext::new(&witness).unwrap();
    let entries = context.layer_entries(0);
    assert_eq!(entries.len(), 2);
    assert_eq!(context.entry_bbox(&entries[0], DIMS), (10., 10., 30., 30.));
    assert_eq!(context.entry_bbox(&entries[1], DIMS), (200., 60., 80., 60.));
    assert_eq!(context.layer_bbox(Some(0), DIMS), (10., 10., 270., 110.), "the layer box is the union");
    // The compositor hands exactly those boxes to the effect runs: A alone is plain, B is its own run on B's box,
    // and a plain item after B starts a new run.
    let effects = witness.scene_effects().unwrap().clone();
    let PathSelection::Layered(stack) = select_composition_path(&effects, witness.plan()).unwrap() else {
        panic!("an item stack selects the layer stack");
    };
    let runs = crate::compositor::scene_effect_gpu_blend::items::runs(&context, 0, &stack.inputs()[0], DIMS);
    let shape: Vec<(usize, Option<(f32, f32, f32, f32)>)> =
        runs.iter().map(|run| (run.entries.len(), run.effect.map(|(_, bbox)| bbox))).collect();
    assert_eq!(shape, vec![(1, None), (1, Some((200., 60., 80., 60.)))]);
}

#[test]
fn consecutive_plain_items_form_one_run_and_an_effected_item_is_its_own() {
    use crate::compositor::object_compositor::layered::LayerContext;
    let rect = |id: &str, x: f64, fx: Option<Value>| item("L", id, (x, 20., x + 20., 60.), [0., 0., 1.], fx);
    let history = owner(&document(&[layer("L", json!({}), vec![
        rect("a", 10., None), rect("b", 40., None), rect("c", 70., invert()), rect("d", 100., None),
        rect("e", 130., None), rect("f", 160., invert()), rect("g", 190., invert()),
    ])]));
    let snapshot = history.acquire_snapshot(history.content_revision()).unwrap();
    let witness = prepare_object_composition(&plan_at(&snapshot, 0), &pins_at(&snapshot, 0)).unwrap();
    let context = LayerContext::new(&witness).unwrap();
    let effects = witness.scene_effects().unwrap().clone();
    let PathSelection::Layered(stack) = select_composition_path(&effects, witness.plan()).unwrap() else {
        panic!("an item stack selects the layer stack");
    };
    let runs = crate::compositor::scene_effect_gpu_blend::items::runs(&context, 0, &stack.inputs()[0], DIMS);
    let shape: Vec<(usize, bool)> = runs.iter().map(|run| (run.entries.len(), run.effect.is_some())).collect();
    assert_eq!(shape, vec![(2, false), (1, true), (2, false), (1, true), (1, true)]);
}

fn keyed(p1_static: f64, keys: &[(u32, f64)]) -> Value {
    let keys: Vec<Value> = keys.iter().map(|(frame, v)| json!({"frame": frame, "v": [v]})).collect();
    json!({"type": "colorAdjust", "enabled": true, "p1": p1_static, "keys": {"p1": {"keys": keys}}})
}

#[test]
fn keyed_effect_parameters_are_sampled_at_the_frame_on_items_and_layers() {
    // Brightness keyed 0 -> 0.4 over frames 0..4 on the default ease (0.5 at the middle); the static 0.9 never shows.
    for (level, expected) in [(0u32, 0u8), (2, 51), (4, 102), (6, 102)] {
        let stack = json!([keyed(0.9, &[(0, 0.), (4, 0.4)])]);
        let on_item = single_at((20., 20., 100., 100.), [0., 0., 1.], Some(stack.clone()), json!({}), level);
        let on_layer = single_at((20., 20., 100., 100.), [0., 0., 1.], None, json!({"effects": stack}), level);
        for (label, doc) in [("item", on_item), ("layer", on_layer)] {
            let (layered, image) = at_frame(&doc, level);
            assert!(layered, "{label}");
            near(pixel(&image, 60, 60), [expected, expected, 255, 255], 1, &format!("{label} at frame {level}"));
        }
    }
}

#[test]
fn a_keyed_matte_on_switches_the_matte_off_between_frames_and_both_doors_agree() {
    // T (red 20..100) is matted by S (blue 40..60 x 40..60) while matteOn >= 50; the track runs 100 -> 0 over frames 0..4.
    let doc_at = |frame: u32| {
        let mut doc = document_at(&[
            layer("T", json!({"matteMode": "alpha", "matteSourceLayerUid": "S"}), vec![item("T", "t", (20., 20., 100., 100.), [1., 0., 0.], None)]),
            layer("S", json!({}), vec![item("S", "s", (40., 40., 60., 60.), [0., 0., 1.], None)]),
        ], frame);
        doc["propertyHolders"] = json!([{"target": {"contextId": "scene-root", "layerUid": "T"},
            "properties": [{"property": "matteOn", "track": {"keys": [
                {"frame": 0, "v": [100], "hold": true}, {"frame": 4, "v": [0]}]}}]}]);
        doc
    };
    let (layered, on) = at_frame(&doc_at(0), 0);
    assert!(layered);
    assert_eq!(pixel(&on, 50, 50), RED, "matted: T shows inside the matte source");
    assert_eq!(pixel(&on, 30, 30), WHITE, "matted: nothing of T outside it");
    let (_, off) = at_frame(&doc_at(4), 4);
    assert_eq!(pixel(&off, 30, 30), RED, "matte off: T is whole");
    assert_eq!(pixel(&off, 50, 50), BLUE, "matte off: the source paints as its own layer, above T");
    // The held key keeps the matte through the segment.
    let (_, held) = at_frame(&doc_at(2), 2);
    assert_eq!(held, on);
}

#[test]
fn effect_radii_follow_the_navigation_zoom_and_an_export_stays_at_document_pixels() {
    // Baseline `effect_zoom` = the view zoom. Blur 9 at zoom 2 is radius 18 (sigma 6); the edge x = 100 lands at 40
    // (zoom about the centre 160): coverage Phi(d / 6). The export of the same plan keeps radius 9 (sigma 3) at x = 100.
    let doc = single((100., 40., 220., 140.), [0., 0., 1.], Some(json!([{"type": "blur", "enabled": true, "p1": 9}])), json!({}));
    let history = owner(&doc);
    let snapshot = history.acquire_snapshot(history.content_revision()).unwrap();
    let plan = plan_at(&snapshot, 0);
    let mut gpu = gpu();
    let zoomed = prepare_object_navigated_viewport_composition(&plan, &pins_at(&snapshot, 0), None, false, 2., [0., 0.]).unwrap();
    let (_, image) = bytes_of(&mut gpu, &zoomed);
    for (x, d) in [(39usize, -0.5f64), (40, 0.5), (44, 4.5), (36, -3.5), (50, 10.5)] {
        let red = (255. * (1. - phi(d / 6.))).round() as u8;
        near(pixel(&image, x, 90), [red, red, 255, 255], 5, &format!("zoom 2 ramp at x = {x}"));
    }
    let export = prepare_object_composition(&plan, &pins_at(&snapshot, 0)).unwrap();
    let (_, image) = bytes_of(&mut gpu, &export);
    let red = |d: f64| (255. * (1. - phi(d / 3.))).round() as u8;
    near(pixel(&image, 99, 90), [red(-0.5), red(-0.5), 255, 255], 5, "export ramp outside");
    near(pixel(&image, 104, 90), [red(4.5), red(4.5), 255, 255], 5, "export ramp inside");
}

fn child(uid: &'static str, extra: Value, items: Vec<Value>) -> Layer {
    let mut layer = layer(uid, extra, items);
    layer.extra["parentLayerUid"] = json!("F");
    layer
}

fn folder(extra: Value) -> Layer {
    let mut fields = json!({"isNullLayer":true,"nullPos":[0,0],"isFolderLayer":true,"folderCollapsed":false});
    for (key, value) in extra.as_object().into_iter().flatten() {
        fields[key] = value.clone();
    }
    layer("F", fields, vec![])
}

#[test]
fn a_folder_child_layer_effect_grades_only_that_child_and_a_folder_effect_the_flattened_group() {
    let j = |extra: Value| child("J", extra, vec![item("J", "j", (20., 20., 60., 60.), [0., 0., 1.], None)]);
    let k = |extra: Value| child("K", extra, vec![item("K", "k", (40., 40., 80., 80.), [1., 0., 0.], None)]);
    let fx = json!({"effects": [{"type": "invert", "enabled": true}]});
    // The child's own stack (baseline `paint_folder_children`): K alone is inverted, then folded over J.
    let (layered, image) = frame_of(&document(&[j(json!({})), k(fx.clone()), folder(json!({"effects": []}))]));
    assert!(layered);
    assert_eq!(pixel(&image, 30, 30), BLUE, "J is untouched");
    assert_eq!(pixel(&image, 50, 50), CYAN, "the graded K covers J");
    assert_eq!(pixel(&image, 70, 70), CYAN, "K alone, graded");
    assert_eq!(pixel(&image, 10, 10), WHITE);
    // The folder's own stack runs on the flattened group: J inverts to yellow, K over it to cyan.
    let (layered, image) = frame_of(&document(&[j(json!({})), k(json!({})), folder(fx)]));
    assert!(layered);
    assert_eq!(pixel(&image, 30, 30), YELLOW);
    assert_eq!(pixel(&image, 50, 50), CYAN, "K over J, then the group inverts");
    assert_eq!(pixel(&image, 70, 70), CYAN);
    assert_eq!(pixel(&image, 10, 10), WHITE, "the empty canvas is never graded");
}
