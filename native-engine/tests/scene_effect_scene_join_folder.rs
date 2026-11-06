//! #2257 folders: a folder's children render at the folder's own place in the stack, the folder's Motion composes
//! onto them as the parent chain does for a Null, Text and images work inside a folder, a Multiply child is
//! flattened inside its folder on the layer stack, and moving a layer into a turned or scaled folder keeps it where
//! it was. Real documents, real Metal, both doors (viewport and export), hand-computed pixels. A missing adapter is
//! a test FAILURE, never a skip.
use super::{gpu, run_at, witness_of_owner, Door, Frame};
use crate::history::NativeObjectHistory;
use serde_json::{json, Value};

#[path = "object_raster_asset_commands_support.rs"]
mod support;

const DIMS: (u32, u32) = (320, 180);
const RED: [u8; 4] = [255, 0, 0, 255];
const GREEN: [u8; 4] = [0, 255, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];
const BLACK: [u8; 4] = [0, 0, 0, 255];
const WHITE: [u8; 4] = [255; 4];

fn timing() -> Value {
    json!({"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing",
        "contextId":"scene-root","keys":[{"frame":0}],"samples":[]})
}

type Bounds = (f64, f64, f64, f64);

/// A closed axis-aligned editable rectangle, filled opaque `rgb`.
fn rect(layer: &str, (x0, y0, x1, y1): Bounds, rgb: [f64; 3]) -> Value {
    let corner = |x: f64, y: f64| json!({"point":{"x":x,"y":y},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}});
    json!({"schemaVersion":1,"family":"editable-vector-path",
        "target":support::target(layer, "s"),
        "geometry":{"kind":"cubic-path","closed":true,"coordinateSpace":"document-world","handleSpace":"relative",
            "segments":[corner(x0, y0), corner(x1, y0), corner(x1, y1), corner(x0, y1)]},
        "fill":{"kind":"solid","r":rgb[0],"g":rgb[1],"b":rgb[2],"a":1},
        "vector":{"stroke":null,"hasRealStroke":false,"strokeWidth":3,"strokeCap":"round","strokeJoin":"round","opacity":1}})
}

/// One bottom-up layer of a test document: uid, parent, optional blend, rectangle and colour.
struct Part {
    uid: &'static str,
    parent: Option<&'static str>,
    blend: Option<&'static str>,
    bounds: Bounds,
    rgb: [f64; 3],
}

fn part(uid: &'static str, parent: Option<&'static str>, bounds: Bounds, rgb: [f64; 3]) -> Part {
    Part {
        uid,
        parent,
        blend: None,
        bounds,
        rgb,
    }
}

/// `parts` bottom-up, then one folder `F` on top (pivot `pivot`, moved by the `motion` property records).
fn document(parts: &[Part], pivot: [i32; 2], motion: &[Value]) -> Value {
    let mut layers: Vec<Value> = Vec::new();
    let mut records = Vec::new();
    let mut ordered = Vec::new();
    let mut elements = Vec::new();
    for (index, p) in parts.iter().enumerate() {
        let mut layer = json!({"layerUid":p.uid,"name":p.uid,"visible":true,"locked":false,
            "color":"#5fa875","parentLayerUid":p.parent,"timing":timing()});
        if let Some(blend) = p.blend {
            layer["blendMode"] = json!(blend);
        }
        layers.push(layer);
        records.push(rect(p.uid, p.bounds, p.rgb));
        ordered.push(json!({"layerUid":p.uid,"naturalPosition":index,"order":{}}));
        elements.push(json!({"layerUid":p.uid,"strokeId":"s","naturalPosition":0,"order":{}}));
    }
    folder_on_top(&mut layers, &mut ordered, parts.len(), pivot);
    let holders = if motion.is_empty() {
        json!([])
    } else {
        json!([{"target":{"contextId":"scene-root","layerUid":"F"},"properties":motion}])
    };
    json!({"format":"nemo.native-object-document","formatVersion":2,"totalFrames":8,
        "layers":layers,"objects":records,"propertyHolders":holders,
        "compositionOrder":order_plan(ordered, elements)})
}

fn folder_on_top(layers: &mut Vec<Value>, ordered: &mut Vec<Value>, below: usize, pivot: [i32; 2]) {
    layers.push(
        json!({"layerUid":"F","name":"Folder","visible":true,"locked":false,"color":"#ffb020",
        "parentLayerUid":null,"isNullLayer":true,"nullPos":pivot,"isFolderLayer":true,
        "folderCollapsed":false,"effects":[],"timing":timing()}),
    );
    ordered.push(json!({"layerUid":"F","naturalPosition":below,"order":{}}));
}

fn order_plan(layers: Vec<Value>, elements: Vec<Value>) -> Value {
    json!({"format":"nemo.native-object-order-plan","formatVersion":1,
        "documentFormatVersion":2,"migration":"explicit-plan-required","referencePolicy":"unresolved",
        "contexts":[{"contextId":"scene-root","layers":layers,
            "scopes":[{"frameScope":{"kind":"authored","frame":0},"elements":elements}]}]})
}

fn owner(doc: &Value) -> NativeObjectHistory {
    NativeObjectHistory::from_admitted_project(
        "folder",
        support::project(doc, support::images_only()),
    )
    .unwrap()
}

fn pixel(bytes: &[u8], x: usize, y: usize) -> [u8; 4] {
    bytes[(y * DIMS.0 as usize + x) * 4..][..4]
        .try_into()
        .unwrap()
}

fn pixels(door: &Door) -> &[u8] {
    match door {
        Door::Pixels(bytes) => bytes,
        Door::Refused(text) => panic!("the door refused: {text}"),
    }
}

/// Both doors and the decoded export PNG hold one image.
fn frame_of(owner: &NativeObjectHistory) -> (Frame, Vec<u8>) {
    let mut gpu = gpu();
    let frame = run_at(&mut gpu, DIMS, &witness_of_owner(owner));
    let viewport = pixels(&frame.viewport).to_vec();
    assert_eq!(viewport, pixels(&frame.export), "viewport vs export");
    assert_eq!(Some(&viewport), frame.png.as_ref(), "viewport vs PNG");
    (frame, viewport)
}

fn turned_and_scaled() -> Vec<Value> {
    vec![
        json!({"property":"position","static":[30,0]}),
        json!({"property":"rotation","static":[90]}),
        json!({"property":"scale","static":[200,200]}),
    ]
}

const CHILD: Bounds = (120., 40., 140., 60.);

#[test]
fn a_visible_child_of_a_plain_folder_renders_where_it_is_authored() {
    let doc = document(
        &[part("L0", Some("F"), CHILD, [1., 0., 0.])],
        [100, 40],
        &[],
    );
    let (frame, bytes) = frame_of(&owner(&doc));
    assert!(!frame.layered, "folders alone fold into the flat sequence");
    assert_eq!(pixel(&bytes, 130, 50), RED);
    assert_eq!(pixel(&bytes, 119, 50), WHITE);
    assert_eq!(pixel(&bytes, 140, 50), WHITE);
    assert_eq!(pixel(&bytes, 130, 60), WHITE);
}

#[test]
fn a_rotated_and_scaled_folder_moves_its_children_to_hand_computed_pixels() {
    // p' = c + R(2 (p - c)) + (30, 0), c = (100, 40), R(dx, dy) = (-dy, dx):
    // the child x 120..140, y 40..60 lands on x 90..130, y 80..120.
    let doc = document(
        &[part("L0", Some("F"), CHILD, [1., 0., 0.])],
        [100, 40],
        &turned_and_scaled(),
    );
    let (_, bytes) = frame_of(&owner(&doc));
    for (x, y) in [(91, 81), (128, 118), (110, 100)] {
        assert_eq!(pixel(&bytes, x, y), RED, "({x},{y}) inside the moved child");
    }
    for (x, y) in [(89, 100), (130, 100), (110, 79), (110, 120), (130, 50)] {
        assert_eq!(pixel(&bytes, x, y), WHITE, "({x},{y}) outside it");
    }
}

#[test]
fn a_folder_paints_its_children_at_its_own_position_not_at_theirs() {
    // Bottom-up: the child (red), a green layer ABOVE it, then the folder. The folder is the topmost layer, so its
    // child covers the green layer where they overlap, though the child sits below it in the stack.
    let doc = document(
        &[
            part("L0", Some("F"), (100., 40., 140., 80.), [1., 0., 0.]),
            part("G", None, (120., 60., 160., 100.), [0., 1., 0.]),
        ],
        [100, 40],
        &[],
    );
    let (_, bytes) = frame_of(&owner(&doc));
    assert_eq!(pixel(&bytes, 130, 70), RED, "child over the green layer");
    assert_eq!(pixel(&bytes, 150, 90), GREEN, "green outside the child");
}

#[test]
fn a_multiply_child_is_flattened_inside_its_folder_on_the_layer_stack() {
    // C green x20..60 y20..60 at the bottom; the folder holds A red x0..50 y0..50 and B blue x30..80 y30..80 (Multiply).
    let mut b = part("B", Some("F"), (30., 30., 80., 80.), [0., 0., 1.]);
    b.blend = Some("multiply");
    let doc = document(
        &[
            part("C", None, (20., 20., 60., 60.), [0., 1., 0.]),
            part("A", Some("F"), (0., 0., 50., 50.), [1., 0., 0.]),
            b,
        ],
        [100, 40],
        &[],
    );
    let (frame, bytes) = frame_of(&owner(&doc));
    assert!(frame.layered, "a Multiply layer needs the layer stack");
    assert_eq!(pixel(&bytes, 40, 40), BLACK, "A x B inside the folder");
    assert_eq!(pixel(&bytes, 10, 10), RED, "A alone");
    // B multiplies A only, never C: over the empty folder backdrop it is itself, and the folder then covers C.
    assert_eq!(pixel(&bytes, 55, 55), BLUE, "B alone, over C");
    assert_eq!(pixel(&bytes, 75, 75), BLUE, "B alone, over the base");
    assert_eq!(
        pixel(&bytes, 55, 25),
        GREEN,
        "C outside the folder's children"
    );
    assert_eq!(pixel(&bytes, 100, 100), WHITE, "untouched base");
}

#[test]
fn an_image_inside_a_turned_and_scaled_folder_lands_on_hand_computed_texels() {
    // Same numbers as the hidden vector parent of the Text/raster chain test, now a folder with pivot (20, 10):
    // scale 200, rotate +90, position (150, 30): p' = c + R(2 (p - c)) + (150, 30), R(dx, dy) = (-dy, dx).
    // The 2x2 image centred at (50, 90) becomes a 4x4 square centred at (10, 100).
    let mut layers = vec![
        json!({"layerUid":"R","name":"R","visible":true,"locked":false,"color":"#5fa875",
        "parentLayerUid":"F","timing":timing()}),
    ];
    let mut ordered = vec![json!({"layerUid":"R","naturalPosition":0,"order":{}})];
    folder_on_top(&mut layers, &mut ordered, 1, [20, 10]);
    let items = json!({"raster":[{"target":support::target("R","raster-0"),
        "item":{"assetId":"img-1","x":50,"y":90,"width":2,"height":2}}]});
    let doc = json!({"format":"nemo.native-object-document","formatVersion":2,"totalFrames":8,
        "layers":layers,"objects":[],"items":items,
        "propertyHolders":[{"target":{"contextId":"scene-root","layerUid":"F"},"properties":[
            {"property":"position","static":[150,30]},{"property":"rotation","static":[90]},
            {"property":"scale","static":[200,200]}]}],
        "compositionOrder":order_plan(ordered, vec![
            json!({"layerUid":"R","strokeId":"raster-0","naturalPosition":0,"order":{}})])});
    let (frame, bytes) = frame_of(&owner(&doc));
    assert!(!frame.layered);
    let mut reader = png::Decoder::new(std::io::Cursor::new(support::png()))
        .read_info()
        .unwrap();
    let mut rgba = vec![0; reader.output_buffer_size().unwrap()];
    let _ = reader.next_frame(&mut rgba).unwrap();
    for (tx, ty, x, y) in [
        (0, 0, 11, 98),
        (1, 0, 11, 101),
        (0, 1, 8, 98),
        (1, 1, 8, 101),
    ] {
        let t = &rgba[(ty * 2 + tx) * 4..(ty * 2 + tx) * 4 + 4];
        let a = f64::from(t[3]) / 255.0;
        let want: Vec<u8> = (0..3)
            .map(|k| (f64::from(t[k]) * a + 255.0 * (1.0 - a)).round() as u8)
            .chain([255])
            .collect();
        let got = pixel(&bytes, x, y);
        assert!(
            got.iter().zip(&want).all(|(g, w)| g.abs_diff(*w) <= 1),
            "texel ({tx},{ty}) at ({x},{y}): {got:?} != {want:?}"
        );
    }
    assert_eq!(pixel(&bytes, 100, 100), WHITE);
}

fn hide(document: &mut Value, uids: &[&str]) {
    for layer in document["layers"].as_array_mut().unwrap() {
        if uids.contains(&layer["layerUid"].as_str().unwrap()) {
            layer["visible"] = json!(false);
        }
    }
}

/// The frame the full joined pipeline (packet, folder ranks, scene join, both doors) paints with `soloed` open.
fn solo_frame(owner: &NativeObjectHistory, soloed: &[&str]) -> Vec<u8> {
    let soloed: Vec<String> = soloed.iter().map(|uid| (*uid).to_owned()).collect();
    let _view = crate::object_frame_packet::solo_view::enter("scene-root", &soloed);
    frame_of(owner).1
}

#[test]
fn gpu_solo_on_a_folder_isolates_its_subtree_and_ignores_the_eyes_of_the_folder_and_its_children() {
    // L0 blue top left (plain). The folder F is hidden by its eye and holds C1 red (top right) and C2 green (lower right, its
    // own eye off); L5 yellow (lower left) is a plain layer with its eye off. Baseline `layerIsEffectivelyVisible`.
    const BLUE_RECT: Bounds = (10., 10., 50., 50.);
    const C1: Bounds = (250., 10., 290., 50.);
    const C2: Bounds = (250., 130., 290., 170.);
    const L5: Bounds = (10., 130., 50., 170.);
    let mut doc = document(
        &[
            part("L0", None, BLUE_RECT, [0., 0., 1.]),
            part("C1", Some("F"), C1, [1., 0., 0.]),
            part("C2", Some("F"), C2, [0., 1., 0.]),
            part("L5", None, L5, [1., 1., 0.]),
        ],
        [100, 40],
        &[],
    );
    hide(&mut doc, &["F", "C2", "L5"]);
    let owner = owner(&doc);
    let at = |bytes: &[u8], b: Bounds| pixel(bytes, ((b.0 + b.2) / 2.) as usize, ((b.1 + b.3) / 2.) as usize);

    let plain = solo_frame(&owner, &[]);
    assert_eq!(at(&plain, BLUE_RECT), BLUE);
    assert_eq!(at(&plain, L5), WHITE, "the eye-off plain layer is hidden");
    assert_eq!(at(&plain, C2), WHITE);
    assert_eq!(at(&plain, C1), WHITE, "a hidden folder hides its children (baseline layerIsEffectivelyVisible)");

    let folder = solo_frame(&owner, &["F"]);
    assert_eq!(at(&folder, C1), RED, "a soloed folder shows its children although its own eye is off");
    assert_eq!(at(&folder, C2), GREEN, "...and a child whose own eye is off");
    assert_eq!(at(&folder, BLUE_RECT), WHITE, "everything outside the subtree is hidden");
    assert_eq!(at(&folder, L5), WHITE);

    let child = solo_frame(&owner, &["C2"]);
    assert_eq!(at(&child, C2), GREEN, "a soloed child shows alone, under a folder whose eye is off");
    assert_eq!(at(&child, C1), WHITE, "a child does not light its folder or its siblings");

    let plain_layer = solo_frame(&owner, &["L5"]);
    assert_eq!(at(&plain_layer, L5), [255, 255, 0, 255], "an eye-off plain layer shows when soloed");
    assert_eq!(at(&plain_layer, C1), WHITE);
    assert_eq!(solo_frame(&owner, &[]), plain, "the next frame without a solo is the plain one");
}
