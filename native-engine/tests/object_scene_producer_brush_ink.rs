//! L07-J (G17): real Metal readback of the brush producer's scene through the one GPU owner.
//! Declared from object_compositor.rs so it can drive `Compositor`'s private scene, renderer
//! and target exactly as `compose_object` does. The pinned scene is produced by the real
//! producer over a real packet, charged store and join registry, drawn anchor-then-ink in the
//! joined order by the scene consumer (`draw_ink`), and read back. Expected pixels are literal
//! (opaque colours, probes away from edges). A missing adapter is an explicit failure, never a
//! skip. Metal readback is not an installed result.

#[path = "object_scene_producer_brush_rows.rs"]
mod rows;

use self::rows::{
    anchor, ink_of, join, payloads, publish, rect, run, source, store, window, with_trim, world,
    PlanBaker, FRAME,
};
use crate::compositor::Compositor;
use crate::evaluation::object_scene_style_kernels::object_scene_producer_brush::{
    BrushInk, BrushStrokeScene, InkState,
};
use crate::evaluation::object_scene_style_kernels::object_scene_producer_brush_slot::TrimSlots;
use crate::object_render_scene::object_scene_producer_brush_ink::{
    draw_ink, ink_placement, InkDrawError,
};
use crate::render_geometry::{ClosedCubicPath, CubicSegment};
use vello::kurbo::Affine;
use vello::peniko::{Color, Fill};
use vello::Scene;

const BLUE: [u8; 4] = [0, 0, 255, 255];
const GREEN: [u8; 4] = [0, 255, 0, 255];
const RED_F: [f64; 4] = [1.0, 0.0, 0.0, 1.0];
const BLUE_F: [f64; 4] = [0.0, 0.0, 1.0, 1.0];
const WHITE: [u8; 4] = [255, 255, 255, 255];
const SIZE: u32 = 48;

fn headless() -> Compositor {
    Compositor::new().expect("L07-J needs a real headless Metal adapter")
}

/// The anchor FILL region the test draws: a stand-in square, not the record's open centerline
/// (the producer payload carries paint only; the anchor contour/transform arrives with the
/// join leaf, see HANDOFF). The trim re-bake, however, is derived from the pinned record.
fn square() -> ClosedCubicPath {
    let corner = |x: f64, y: f64| CubicSegment {
        point: [x, y],
        handle_in: [0.0, 0.0],
        handle_out: [0.0, 0.0],
    };
    ClosedCubicPath::new(vec![
        corner(0.0, 0.0),
        corner(16.0, 0.0),
        corner(16.0, 16.0),
        corner(0.0, 16.0),
    ])
    .expect("square contour")
}

/// Draw the parts back to front: each anchor's fill (opacity folded into alpha), then its
/// ink above it; read the target back as straight RGBA8 over white.
fn render(gpu: &mut Compositor, parts: &[&BrushStrokeScene]) -> Vec<u8> {
    gpu.scene.reset();
    let mut images = Vec::new();
    let path = square();
    for part in parts {
        if let Some(fill) = &part.anchor.fill {
            let mut rgba = fill.rgba_f32().expect("representable fill");
            rgba[3] *= part.anchor.opacity as f32;
            gpu.scene.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                Color::new(rgba),
                None,
                path.path(),
            );
        }
        if let InkState::Ink(ink) = &part.ink {
            images.push(draw_ink(&mut gpu.scene, ink, Affine::IDENTITY).expect("draw ink"));
        }
    }
    for image in &images {
        gpu.renderer.mark_override_image_dirty(image);
    }
    let mut target = gpu.make_object_target(SIZE, SIZE).expect("object target");
    target
        .render(|view| gpu.render_target(view, SIZE, SIZE, Color::WHITE))
        .expect("render scene");
    gpu.readback_texture(target.texture(), SIZE, SIZE)
        .expect("readback")
        .to_vec()
}

fn assert_px(bytes: &[u8], x: usize, y: usize, want: [u8; 4], tolerance: u8) {
    let at = (y * SIZE as usize + x) * 4;
    let got: [u8; 4] = bytes[at..at + 4].try_into().unwrap();
    assert!(
        got.iter()
            .zip(want)
            .all(|(g, w)| g.abs_diff(w) <= tolerance),
        "({x},{y}): {got:?} != {want:?}"
    );
}

fn one_stroke(fill: Option<[f64; 4]>, ink_opacity: f64) -> Vec<u8> {
    let (_snap, pk) = world(FRAME, &[anchor("L0", "a", fill)]);
    let mut store = store(1 << 20);
    publish(&mut store, "ink-a", 4, 4, BLUE);
    let mut src = source(
        "L0",
        "a",
        "g",
        fill.is_some(),
        "ink-a",
        rect(8.0, 8.0, 16.0, 16.0),
    );
    src.ink_opacity = ink_opacity;
    let (mut slots, mut baker) = (TrimSlots::new(), PlanBaker::new(GREEN));
    let scene = join(
        &pk,
        run(&pk, &[src], &mut store, &mut slots, &mut baker).unwrap(),
    )
    .unwrap();
    render(&mut headless(), &payloads(&scene))
}

#[test]
fn the_ink_is_drawn_above_the_anchor_fill() {
    let px = one_stroke(Some(RED_F), 1.0);
    assert_px(&px, 4, 4, [255, 0, 0, 255], 1); // anchor fill only
    assert_px(&px, 12, 4, [255, 0, 0, 255], 1); // above the ink's top edge
    assert_px(&px, 12, 12, BLUE, 1); // ink over the anchor
    assert_px(&px, 20, 20, BLUE, 1); // ink alone, outside the 16 px anchor
    assert_px(&px, 4, 20, WHITE, 1); // neither
    assert_px(&px, 30, 30, WHITE, 1);
}

#[test]
fn the_ink_opacity_blends_over_the_anchor_and_the_background() {
    let px = one_stroke(Some(RED_F), 0.5);
    assert_px(&px, 12, 12, [128, 0, 128, 255], 2);
    assert_px(&px, 20, 20, [128, 128, 255, 255], 2);
    assert_px(&px, 4, 4, [255, 0, 0, 255], 1);
}

#[test]
fn an_unfilled_anchor_is_invisible_and_only_the_ink_shows() {
    let px = one_stroke(None, 1.0);
    assert_px(&px, 4, 4, WHITE, 1);
    assert_px(&px, 12, 12, BLUE, 1);
    assert_px(&px, 20, 20, BLUE, 1);
}

#[test]
fn a_trim_re_bake_is_drawn_from_the_planned_stamps_over_the_anchor() {
    let (_snap, pk) = world(FRAME, &[anchor("L0", "a", Some(BLUE_F))]);
    let mut store = store(1 << 20);
    publish(&mut store, "ink-a", 4, 4, BLUE);
    let src = with_trim(
        source("L0", "a", "g", true, "ink-a", rect(8.0, 8.0, 16.0, 16.0)),
        Some(window(0.0, 20.0)),
    );
    let (mut slots, mut baker) = (TrimSlots::new(), PlanBaker::new(GREEN));
    let scene = join(
        &pk,
        run(&pk, &[src], &mut store, &mut slots, &mut baker).unwrap(),
    )
    .unwrap();
    // The re-bake line (10, 10)..(30, 10) is the 0..20 % window of the pinned record's own
    // centerline (10, 10) -> (110, 10), which is what the provider was handed.
    assert_eq!(baker.anchors.len(), 1);
    assert_eq!(
        baker.anchors[0]
            .segments
            .iter()
            .map(|s| s.point)
            .collect::<Vec<_>>(),
        [[10.0, 10.0], [110.0, 10.0]]
    );
    let px = render(&mut headless(), &payloads(&scene));
    // The first planned stamp is centred on the trimmed line's start (10, 10).
    assert_px(&px, 10, 10, GREEN, 1);
    // Four world pixels below the line the stamps stop (half size <= 3.75): the anchor shows.
    assert_px(&px, 10, 15, [0, 0, 255, 255], 1);
    assert_px(&px, 10, 20, WHITE, 1);
    // The line ends at x = 30: ink to its end, nothing beyond the last stamp.
    assert_px(&px, 28, 10, GREEN, 1);
    assert_px(&px, 40, 10, WHITE, 1);
}

#[test]
fn a_collapsed_trim_window_draws_the_anchor_and_no_ink() {
    let (_snap, pk) = world(FRAME, &[anchor("L0", "a", Some(RED_F))]);
    let mut store = store(1 << 20);
    publish(&mut store, "ink-a", 4, 4, BLUE);
    let src = with_trim(
        source("L0", "a", "g", true, "ink-a", rect(8.0, 8.0, 16.0, 16.0)),
        Some(window(5.0, 5.0)),
    );
    let (mut slots, mut baker) = (TrimSlots::new(), PlanBaker::new(GREEN));
    let scene = join(
        &pk,
        run(&pk, &[src], &mut store, &mut slots, &mut baker).unwrap(),
    )
    .unwrap();
    let px = render(&mut headless(), &payloads(&scene));
    assert_px(&px, 12, 12, [255, 0, 0, 255], 1);
    assert_px(&px, 20, 20, WHITE, 1);
}

#[test]
fn a_scene_keeps_drawing_its_own_version_after_the_ink_was_regenerated() {
    let (_snap, pk) = world(FRAME, &[anchor("L0", "a", None)]);
    let mut store = store(1 << 20);
    publish(&mut store, "ink-a", 4, 4, BLUE);
    let src = [source(
        "L0",
        "a",
        "g",
        false,
        "ink-a",
        rect(8.0, 8.0, 16.0, 16.0),
    )];
    let (mut slots, mut baker) = (TrimSlots::new(), PlanBaker::new(GREEN));
    let old = join(
        &pk,
        run(&pk, &src, &mut store, &mut slots, &mut baker).unwrap(),
    )
    .unwrap();
    publish(&mut store, "ink-a", 4, 4, GREEN);
    let new = join(
        &pk,
        run(&pk, &src, &mut store, &mut slots, &mut baker).unwrap(),
    )
    .unwrap();
    let mut gpu = headless();
    assert_px(&render(&mut gpu, &payloads(&new)), 20, 20, GREEN, 1);
    assert_px(&render(&mut gpu, &payloads(&old)), 20, 20, BLUE, 1);
}

#[test]
fn two_strokes_draw_in_the_joined_plan_order_whatever_the_source_order() {
    let (_snap, pk) = world(FRAME, &[anchor("L0", "a", None), anchor("L1", "b", None)]);
    let mut store = store(1 << 20);
    publish(&mut store, "ink-a", 4, 4, BLUE);
    publish(&mut store, "ink-b", 4, 4, GREEN);
    let a = source("L0", "a", "ga", false, "ink-a", rect(8.0, 8.0, 16.0, 16.0));
    let b = source(
        "L1",
        "b",
        "gb",
        false,
        "ink-b",
        rect(16.0, 16.0, 16.0, 16.0),
    );
    let (mut slots, mut baker) = (TrimSlots::new(), PlanBaker::new(GREEN));
    let mut gpu = headless();
    let mut seen = Vec::new();
    for sources in [[a.clone(), b.clone()], [b, a]] {
        let scene = join(
            &pk,
            run(&pk, &sources, &mut store, &mut slots, &mut baker).unwrap(),
        )
        .unwrap();
        seen.push(render(&mut gpu, &payloads(&scene)));
    }
    // The overlap (20, 20) belongs to the later layer L1; the strokes' own areas keep their colour.
    for px in &seen {
        assert_px(px, 20, 20, GREEN, 1);
        assert_px(px, 12, 12, BLUE, 1);
        assert_px(px, 28, 28, GREEN, 1);
    }
    assert_eq!(seen[0], seen[1]);
}

/// The resident ink of a one-anchor production (no GPU needed).
fn resident_ink() -> BrushInk {
    let (_snap, pk) = world(FRAME, &[anchor("L0", "a", Some(RED_F))]);
    let mut store = store(1 << 20);
    publish(&mut store, "ink-a", 4, 4, BLUE);
    let src = source("L0", "a", "g", true, "ink-a", rect(8.0, 8.0, 16.0, 16.0));
    let (mut slots, mut baker) = (TrimSlots::new(), PlanBaker::new(GREEN));
    let scene = join(
        &pk,
        run(&pk, &[src], &mut store, &mut slots, &mut baker).unwrap(),
    )
    .unwrap();
    ink_of(payloads(&scene)[0]).clone()
}

#[test]
fn an_ink_that_cannot_be_placed_or_blended_is_a_typed_error_and_draws_nothing() {
    let ink = resident_ink();
    // The 4x4 bitmap over the 16x16 rectangle at (8, 8) scales by 4 and translates by 8.
    let placed = ink_placement(&ink).unwrap();
    assert_eq!(placed.as_coeffs(), [4.0, 0.0, 0.0, 4.0, 8.0, 8.0]);
    for (min_x, min_y, w, h) in [
        (f64::NAN, 8.0, 16.0, 16.0),
        (8.0, f64::INFINITY, 16.0, 16.0),
        (8.0, 8.0, f64::NAN, 16.0),
        (8.0, 8.0, 16.0, f64::NEG_INFINITY),
        (8.0, 8.0, 0.0, 16.0),
        (8.0, 8.0, 16.0, -1.0),
    ] {
        let mut bad = ink.clone();
        bad.rect = rect(min_x, min_y, w, h);
        assert_eq!(ink_placement(&bad), Err(InkDrawError::Placement));
        let mut scene = Scene::new();
        assert!(matches!(
            draw_ink(&mut scene, &bad, Affine::IDENTITY),
            Err(InkDrawError::Placement)
        ));
    }
    for opacity in [f64::NAN, f64::INFINITY] {
        let mut bad = ink.clone();
        bad.opacity = opacity;
        let mut scene = Scene::new();
        assert!(matches!(
            draw_ink(&mut scene, &bad, Affine::IDENTITY),
            Err(InkDrawError::Opacity)
        ));
    }
}
