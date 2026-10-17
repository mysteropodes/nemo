//! Metal readback of the editable vector family through the production path: document -> snapshot -> packet -> scene -> paint
//! plan -> compositor (L01-I). Declared from object_compositor.rs so it sits inside the compositor tree and can name its private
//! `ObjectOutputSpec`. A missing real adapter is a test FAILURE, never a skip. Every expected pixel is a certified probe of the
//! baseline oracle (`tools/gen.cjs`) or a literal written here from the geometry; Metal readback is never an installed result.
use super::ObjectOutputSpec;
use crate::compositor::Compositor;
use crate::object_render_scene::prepare_object_composition;
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};

#[path = "render_vector_support.rs"]
mod support;
use support::*;

fn headless() -> Compositor {
    Compositor::new().expect("L01-I needs a real headless Metal adapter")
}

/// The straight RGBA8 frame of a snapshot on a white 64x64 canvas.
fn render(gpu: &mut Compositor, snapshot: &ObjectSnapshot) -> Vec<u8> {
    let plan = plan_of(snapshot);
    let prepared = prepare_object_composition(&plan, &selector(snapshot, 0)).expect("witness");
    let output = ObjectOutputSpec::new(64, 64, "white-srgb-normal-rgba8-area").expect("output");
    let result = gpu.compose_object(&prepared, output).expect("compose");
    gpu.readback_object_rgba8(&result)
        .expect("readback")
        .bytes()
        .to_vec()
}

fn render_scene(gpu: &mut Compositor, id: &str) -> Vec<u8> {
    render(gpu, &snapshot(&scene_doc(&scene_row(id).records)))
}

#[test]
fn every_oracle_scene_renders_the_certified_pixels() {
    let mut gpu = headless();
    let mut checked = 0;
    let mut failures = Vec::new();
    for row in scene_rows() {
        let bytes = render(&mut gpu, &snapshot(&scene_doc(&row.records)));
        assert_eq!(bytes.len(), SIZE * SIZE * 4, "{}", row.id);
        failures.extend(
            probe_failures(&bytes, &row.probes, 2)
                .into_iter()
                .map(|f| format!("{}: {f}", row.id)),
        );
        checked += row.probes.len();
    }
    assert_eq!(checked, 136);
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn round_butt_and_square_caps_differ_exactly_where_the_geometry_says() {
    let mut gpu = headless();
    let red = [255, 0, 0, 255];
    let white = [255, 255, 255, 255];
    let round = render_scene(&mut gpu, "line-round-cap");
    let butt = render_scene(&mut gpu, "line-butt-cap");
    let square = render_scene(&mut gpu, "line-square-cap");
    // a pixel inside the round cap's disc but left of the butt end
    assert_eq!(pixel(&round, 11, 31), red);
    assert_eq!(pixel(&butt, 11, 31), white);
    assert_eq!(pixel(&square, 11, 31), red);
    // the corner pixel only a square cap reaches
    assert_eq!(pixel(&round, 10, 26), white);
    assert_eq!(pixel(&butt, 10, 26), white);
    assert_eq!(pixel(&square, 10, 26), red);
    assert_ne!(round, butt);
    assert_ne!(round, square);
}

#[test]
fn paint_order_flips_the_overlap_pixel_and_an_absent_order_is_fill_first() {
    let mut gpu = headless();
    let first = render_scene(&mut gpu, "order-fill-first-explicit");
    let last = render_scene(&mut gpu, "order-stroke-first");
    let absent = render_scene(&mut gpu, "order-absent");
    // the stroke's inner half overlaps the fill: the later paint wins
    assert_eq!(pixel(&first, 20, 32), [255, 0, 0, 255]);
    assert_eq!(pixel(&last, 20, 32), [0, 255, 0, 255]);
    // outside the fill only the stroke paints, in either order
    assert_eq!(pixel(&first, 12, 32), [255, 0, 0, 255]);
    assert_eq!(pixel(&last, 12, 32), [255, 0, 0, 255]);
    assert_ne!(first, last);
    assert_eq!(first, absent);
}

#[test]
fn has_real_stroke_false_draws_no_stroke_over_a_backdrop() {
    let mut gpu = headless();
    let without = render_scene(&mut gpu, "no-real-stroke-over-black");
    let with = render_scene(&mut gpu, "real-stroke-over-black");
    // the pixel just outside the red square stays the black backdrop when there is no real stroke
    assert_eq!(pixel(&without, 19, 32), [0, 0, 0, 255]);
    assert_eq!(pixel(&with, 19, 32), [255, 255, 255, 255]);
    assert_eq!(pixel(&without, 32, 32), [255, 0, 0, 255]);
}

#[test]
fn opacity_is_a_per_paint_product_not_a_group_opacity() {
    let mut gpu = headless();
    let frame = render_scene(&mut gpu, "opacity-half-fill-and-stroke");
    // half-alpha red stroke over half-alpha green fill over white: both paints show through each other
    let got = pixel(&frame, 20, 32);
    for (channel, want) in got.iter().zip([191u8, 127, 63, 255]) {
        assert!(channel.abs_diff(want) <= 2, "{got:?}");
    }
    // a group opacity would have shown (255, 127, 127) here
    assert!(got[0].abs_diff(255) > 8 && got[2].abs_diff(127) > 8);
}

#[test]
fn a_pressure_ribbon_is_painted_as_its_stored_closed_contour_and_nothing_else() {
    let mut gpu = headless();
    let plain = scene_row("ribbon-diamond-fill-only").records[0].clone();
    let mut pressure = plain.clone();
    pressure["vectorExt"] = json!({"pressure":{
        "centerSegments":[
            {"point":{"x":16,"y":32},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0},"width":2},
            {"point":{"x":48,"y":32},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0},"width":12}],
        "widthProfile":[{"t":0,"width":2},{"t":1,"width":12}],
        "strokeProfile":"taper-in","profileBase":12}});
    let flat = render(&mut gpu, &snapshot(&scene_doc(&[plain])));
    let ribbon = render(&mut gpu, &snapshot(&scene_doc(&[pressure])));
    // the centreline, the width profile and the profile intent are stored data; no ribbon is regenerated at draw time
    assert_eq!(flat, ribbon);
    assert_eq!(pixel(&ribbon, 32, 32), [34, 34, 34, 255]);
    assert_eq!(pixel(&ribbon, 16, 16), [255, 255, 255, 255]);
}

#[test]
fn the_legacy_closed_fill_and_the_equivalent_vector_render_the_same_frame() {
    let mut gpu = headless();
    let vector = parts(true, &SQUARE, solid(0., 0.5, 1., 1.), body(json!({})));
    let mut legacy = scene_doc(std::slice::from_ref(&vector));
    let record = legacy["objects"][0].as_object_mut().expect("record");
    record.insert("family".into(), json!("closed-solid-fill-cubic-path"));
    record.remove("vector");
    let old = render(&mut gpu, &snapshot(&legacy));
    let new = render(&mut gpu, &snapshot(&scene_doc(&[vector])));
    assert_eq!(old, new);
    // straight (0, 0.5, 1, 1) over white: the green channel is 127.5 before rounding
    let middle = pixel(&new, 32, 32);
    assert!(
        middle[0] == 0 && middle[1].abs_diff(128) <= 1 && middle[2] == 255 && middle[3] == 255,
        "{middle:?}"
    );
    assert_eq!(pixel(&new, 8, 8), [255, 255, 255, 255]);
}

#[test]
fn an_open_path_fill_closes_with_a_straight_chord() {
    let mut gpu = headless();
    let frame = render_scene(&mut gpu, "open-path-fill-closes-straight");
    assert_eq!(pixel(&frame, 32, 40), [0, 0, 255, 255]);
    // below the chord from (48,48) back to (16,48) there is nothing
    assert_eq!(pixel(&frame, 32, 50), [255, 255, 255, 255]);
}

#[test]
fn an_invisible_vector_draws_nothing_and_a_frame_is_deterministic() {
    let mut gpu = headless();
    let invisible = parts(true, &SQUARE, Value::Null, body(json!({})));
    let snap = snapshot(&scene_doc(&[invisible]));
    let first = render(&mut gpu, &snap);
    assert!(first.chunks(4).all(|p| p == [255, 255, 255, 255]));
    assert_eq!(render(&mut gpu, &snap), first);
}
