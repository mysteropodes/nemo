//! L07-I (F16, G03a): real Metal readback of the typed scene style through the one GPU owner. Declared from
//! object_compositor.rs so it can drive `Compositor`'s private scene and target exactly as `compose_object` does.
//! Scenes and expected pixels come from tests/fixtures/object-scene-style-emit/oracle.txt; a missing adapter is an
//! explicit failure, never a skip. Metal readback is not an installed result.
use crate::compositor::Compositor;
use crate::evaluation::object_scene_style_kernels::object_scene_style_item::SceneItemStyle;
use crate::evaluation::object_scene_style_kernels::object_scene_style_paint::PaintOrder;
use crate::evaluation::object_scene_style_kernels::object_scene_style_stroke::{
    SceneCap, SceneJoin,
};
use crate::object_render_scene::object_scene_style_emit::prepare_item;
use crate::object_render_scene::object_scene_style_emit::rows::{
    document, mismatches, rows, scene_items, SceneItem,
};
use serde_json::{json, Value};
use vello::kurbo::Affine;
use vello::peniko::{Color, Fill};

fn headless() -> Compositor {
    Compositor::new().expect("L07-I needs a real headless Metal adapter")
}

fn scene_row(id: &str) -> Value {
    rows("scene")
        .into_iter()
        .find(|row| row["id"] == id)
        .unwrap_or_else(|| panic!("no scene row {id}"))
}

fn size_of(row: &Value) -> (u32, u32) {
    (
        row["size"][0].as_u64().expect("width") as u32,
        row["size"][1].as_u64().expect("height") as u32,
    )
}

/// Draw `draw` into the compositor's one retained scene and read the target back as straight RGBA8 over white.
fn render_with(
    gpu: &mut Compositor,
    size: (u32, u32),
    draw: impl FnOnce(&mut vello::Scene),
) -> Vec<u8> {
    gpu.scene.reset();
    draw(&mut gpu.scene);
    let mut target = gpu
        .make_object_target(size.0, size.1)
        .expect("object target");
    target
        .render(|view| gpu.render_target(view, size.0, size.1, Color::WHITE))
        .expect("render scene");
    gpu.readback_texture(target.texture(), size.0, size.1)
        .expect("readback")
        .to_vec()
}

fn render_items(gpu: &mut Compositor, size: (u32, u32), items: &[SceneItem]) -> Vec<u8> {
    render_with(gpu, size, |scene| {
        for item in items {
            prepare_item(&item.style)
                .expect("scene style prepares")
                .draw(scene, Affine::IDENTITY, &item.path);
        }
    })
}

fn render_row(gpu: &mut Compositor, row: &Value) -> Vec<u8> {
    render_items(gpu, size_of(row), &scene_items(row))
}

fn render_mutant(
    gpu: &mut Compositor,
    row: &Value,
    mutate: impl Fn(&mut SceneItemStyle),
) -> Vec<u8> {
    let mut items = scene_items(row);
    for item in &mut items {
        mutate(&mut item.style);
    }
    render_items(gpu, size_of(row), &items)
}

fn pixel(bytes: &[u8], width: usize, x: usize, y: usize) -> [u8; 4] {
    bytes[(y * width + x) * 4..(y * width + x) * 4 + 4]
        .try_into()
        .unwrap()
}

fn assert_row(
    bytes: &[u8],
    width: usize,
    y: usize,
    xs: std::ops::Range<usize>,
    expected: &[[u8; 4]],
) {
    for (x, want) in xs.zip(expected) {
        let got = pixel(bytes, width, x, y);
        assert!(
            got.iter().zip(want).all(|(g, w)| g.abs_diff(*w) <= 1),
            "({x},{y}): {got:?} != {want:?}"
        );
    }
}

const WHITE: [u8; 4] = [255, 255, 255, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];
const RED: [u8; 4] = [255, 0, 0, 255];

#[test]
fn every_oracle_scene_renders_the_baseline_pixels() {
    let mut gpu = headless();
    let mut bad = Vec::new();
    let all = rows("scene");
    assert_eq!(all.len(), 20);
    for row in &all {
        let bytes = render_row(&mut gpu, row);
        bad.extend(mismatches(row, &bytes));
    }
    assert!(
        bad.is_empty(),
        "{} probes differ:\n{}",
        bad.len(),
        bad.join("\n")
    );
}

#[test]
fn fill_first_and_stroke_first_literal_pixel_rows() {
    // Closed rectangle (8,8)-(24,24), fill red, stroke blue 4 wide centred on the edge (x 6..10 at row 16).
    // Fill first: the stroke covers the fill's inner half. Stroke first: the fill covers the stroke's inner half.
    let mut gpu = headless();
    let fill_first = render_row(&mut gpu, &scene_row("rect-fill-first"));
    assert_row(
        &fill_first,
        32,
        16,
        4..12,
        &[WHITE, WHITE, BLUE, BLUE, BLUE, BLUE, RED, RED],
    );
    let stroke_first = render_row(&mut gpu, &scene_row("rect-stroke-first"));
    assert_row(
        &stroke_first,
        32,
        16,
        4..12,
        &[WHITE, WHITE, BLUE, BLUE, RED, RED, RED, RED],
    );
    // The record without a paintOrder member draws like the explicit fillFirst.
    let absent = render_row(
        &mut gpu,
        &scene_row("rect-absent-paint-order-is-fill-first"),
    );
    assert_eq!(absent, fill_first);
    assert_ne!(stroke_first, fill_first);
}

#[test]
fn flipping_only_the_paint_order_turns_one_oracle_scene_into_the_other() {
    let mut gpu = headless();
    let fill_row = scene_row("rect-fill-first");
    let stroke_row = scene_row("rect-stroke-first");
    let flipped = render_mutant(&mut gpu, &fill_row, |style| {
        style.order = PaintOrder::StrokeFirst
    });
    assert!(
        !mismatches(&fill_row, &flipped).is_empty(),
        "the fill-first oracle must reject stroke-first pixels"
    );
    assert!(
        mismatches(&stroke_row, &flipped).is_empty(),
        "same geometry and paints, only the order differs"
    );
    let back = render_mutant(&mut gpu, &stroke_row, |style| {
        style.order = PaintOrder::FillFirst
    });
    assert!(mismatches(&fill_row, &back).is_empty());
}

#[test]
fn cap_join_and_dash_mutations_are_rejected_by_the_oracle() {
    let mut gpu = headless();
    // Caps: every line butt -> the square and round rows lose their overhang.
    let caps = scene_row("caps");
    let butt = render_mutant(&mut gpu, &caps, |style| {
        if let Some(stroke) = style.stroke.as_mut() {
            stroke.stroke.cap = SceneCap::Butt;
        }
    });
    assert!(!mismatches(&caps, &butt).is_empty());
    // Joins: the miter elbow drawn with a bevel is exactly the bevel oracle's picture.
    let miter = scene_row("join-miter");
    let bevelled = render_mutant(&mut gpu, &miter, |style| {
        if let Some(stroke) = style.stroke.as_mut() {
            stroke.stroke.join = SceneJoin::Bevel;
        }
    });
    assert!(!mismatches(&miter, &bevelled).is_empty());
    assert!(mismatches(&scene_row("join-bevel"), &bevelled).is_empty());
    // Dash: dropping the pattern turns the dashed line into a solid one.
    let dashed = scene_row("dash-plain");
    let solid = render_mutant(&mut gpu, &dashed, |style| {
        if let Some(stroke) = style.stroke.as_mut() {
            stroke.stroke.dash = None;
        }
    });
    assert!(!mismatches(&dashed, &solid).is_empty());
}

#[test]
fn the_baked_dash_offset_defect_is_reproduced_not_corrected() {
    // [4,4] at offset 2 bakes to [2,4] (engine.rs drops the consumed part), so the pattern is 2 on, 4 off:
    // pixels 6,7 are on and 8,9 are off. A correct phase shift would keep 8,9 on (true pattern: on 6..10).
    let mut gpu = headless();
    let bytes = render_row(
        &mut gpu,
        &scene_row("dash-offset-2-pins-the-baked-pattern-defect"),
    );
    assert_row(&bytes, 32, 15, 6..10, &[BLUE, BLUE, WHITE, WHITE]);
}

#[test]
fn the_legacy_solid_fill_is_pixel_identical_to_a_direct_vello_fill() {
    // Regression guard for the compositor rewiring: the typed path must draw exactly what `scene.fill` of the same
    // straight float colour draws.
    let seg = |x: f64, y: f64| json!({"point":{"x":x,"y":y},"handleIn":{"x":0.0,"y":0.0},"handleOut":{"x":0.0,"y":0.0}});
    let record = json!({"schemaVersion":1,"family":"closed-solid-fill-cubic-path",
        "target":{"contextId":"scene-root","frameScope":{"kind":"authored","frame":0},"layerUid":"L1","strokeId":"legacy"},
        "geometry":{"kind":"cubic-path","closed":true,"coordinateSpace":"document-world","handleSpace":"relative",
            "segments":[seg(4.0, 4.0), seg(28.0, 4.0), seg(28.0, 28.0), seg(4.0, 28.0)]},
        "fill":{"kind":"solid","r":0.2,"g":0.4,"b":0.6,"a":0.75}});
    let row = json!({"id":"legacy","size":[32,32],"items":[{"record":record.clone()}],"probes":[]});
    let mut gpu = headless();
    let typed = render_row(&mut gpu, &row);
    let doc = document(vec![record]);
    let path = crate::object_render_scene::object_scene_style_emit::rows::bez_of(&doc.objects()[0]);
    let direct = render_with(&mut gpu, (32, 32), |scene| {
        let colour = Color::new([0.2f64 as f32, 0.4f64 as f32, 0.6f64 as f32, 0.75f64 as f32]);
        scene.fill(Fill::NonZero, Affine::IDENTITY, colour, None, &path);
    });
    assert_eq!(typed, direct);
    // Over white, alpha 0.75 of (0.2, 0.4, 0.6): 0.2*0.75 + 0.25 = 0.4 -> 102, 0.55 -> 140, 0.7 -> 178.
    assert_row(&typed, 32, 16, 16..17, &[[102, 140, 178, 255]]);
    assert_row(&typed, 32, 16, 2..3, &[WHITE]);
}
