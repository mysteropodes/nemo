//! #2372: the viewport-only Current frame outline is painted by the engine for the presented frame; an export never has it.
//! Literal pixel oracles on the actual headless GPU; no AppKit claim.
use crate::compositor::{object_compositor::ObjectOutputSpec, Compositor};
use crate::history::NativeObjectHistory;
use crate::object_codec::decode_storage_project;
use crate::object_document::{FrameScopeKind, ProjectEnvelope, ProjectMetadata};
use crate::object_frame_packet::{prepare_object_frame, ObjectFrameSelector};
use crate::object_render_scene::{
    paint_test_document, prepare_object_composition, prepare_object_navigated_viewport_composition,
    prepare_object_paint_plan, prepare_object_render_scene, prepare_object_viewport_composition,
    viewport_checker_quality_suffix, viewport_outline_quality_suffix, ObjectCompositionWitness,
};
use crate::revision::ObjectSnapshot;
use serde_json::json;

const MODE: &str = "project-srgb-normal-rgba8-area";

/// One opaque red square at 24..40 on a 64 x 64 canvas, on the given document background.
fn owner(background: Option<&str>) -> NativeObjectHistory {
    let mut drawing = paint_test_document(&[0]);
    for object in drawing["objects"].as_array_mut().unwrap() {
        object["geometry"]["segments"] =
            json!([[24, 24], [40, 24], [40, 40], [24, 40]].map(|[x, y]| {
                json!({"point":{"x":x,"y":y},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}})
            }));
        object["fill"] = json!({"kind":"solid","r":1,"g":0,"b":0,"a":1});
    }
    let metadata = ProjectMetadata::new(64, 64, 24.)
        .unwrap()
        .with_canvas_background(background)
        .unwrap();
    let drawing = decode_storage_project(&serde_json::to_vec(&drawing).unwrap()).unwrap();
    NativeObjectHistory::from_admitted_project(
        "outline",
        ProjectEnvelope::no_media(metadata, drawing),
    )
    .unwrap()
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
fn grey(value: u8) -> [u8; 4] {
    [value, value, value, 255]
}
fn near(actual: [u8; 4], expected: [u8; 4]) -> bool {
    actual
        .iter()
        .zip(&expected)
        .all(|(a, b)| a.abs_diff(*b) <= 1)
}
fn inside_square(x: usize, y: usize) -> bool {
    (24..40).contains(&x) && (24..40).contains(&y)
}

fn red_square() -> NativeObjectHistory {
    owner(Some("#00000000"))
}

#[test]
fn the_outline_is_part_of_the_viewport_cache_identity_and_only_a_viewport_witness_takes_it() {
    let owner = red_square();
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let plain = viewport(&snapshot);
    let one = viewport(&snapshot).with_viewport_outline(1.).unwrap();
    let half = viewport(&snapshot).with_viewport_outline(0.5).unwrap();
    assert_eq!(
        one.expected_quality(),
        format!(
            "{}{}",
            plain.expected_quality(),
            viewport_outline_quality_suffix(1.)
        )
    );
    assert_ne!(one.expected_quality(), half.expected_quality());
    assert_ne!(one.expected_quality(), plain.expected_quality());
    // With the checker too, the identity carries both, in the order the host applies them.
    let both = viewport(&snapshot)
        .with_viewport_checker((64, 64), 1.)
        .unwrap()
        .with_viewport_outline(1.)
        .unwrap();
    assert_eq!(
        both.expected_quality(),
        format!(
            "{}{}{}",
            plain.expected_quality(),
            viewport_checker_quality_suffix(1.),
            viewport_outline_quality_suffix(1.)
        )
    );
    let export = prepare_object_composition(&plan(&snapshot), &pins(&snapshot)).unwrap();
    assert!(
        export.clone().with_viewport_outline(1.).is_err(),
        "an export witness never carries the outline"
    );
    for bad in [0., -1., f64::NAN, f64::INFINITY] {
        assert!(viewport(&snapshot).with_viewport_outline(bad).is_err(), "{bad}");
    }
    assert_eq!(export.expected_quality(), "final");
    assert_eq!(export.viewport_outline(), None);
    assert_eq!(plain.viewport_outline(), None);
}

#[test]
fn the_outline_width_is_the_baseline_one_at_every_zoom() {
    let owner = red_square();
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let width = |scale: f64| {
        viewport(&snapshot)
            .with_viewport_outline(scale)
            .unwrap()
            .viewport_outline()
            .unwrap()
    };
    assert_eq!(width(1.), 1.);
    assert_eq!(width(0.5), 2.);
    assert_eq!(width(4.), 0.75, "never thinner than 0.75 world units");
    assert_eq!(width(2.), 0.75_f64.max(0.5));
}

#[test]
fn gpu_the_outline_drops_the_fill_and_strokes_the_edge_like_the_baseline() {
    let mut gpu = Compositor::new().expect("actual headless GPU is required; never skip");
    let owner = red_square();
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let plain = pixels(&mut gpu, &viewport(&snapshot));
    assert_eq!(at(&plain, 30, 30), [255, 0, 0, 255], "the plain frame is filled");
    // 0.5 screen px per composition px: the stroke is 2 composition units wide, centred on the square edge at x = 24.
    let outlined = pixels(&mut gpu, &viewport(&snapshot).with_viewport_outline(0.5).unwrap());
    assert_eq!(at(&outlined, 30, 30)[3], 0, "the fill is dropped: {:?}", at(&outlined, 30, 30));
    assert_eq!(at(&outlined, 4, 4)[3], 0, "outside the shape nothing is added");
    for (x, y) in [(23, 30), (24, 30), (39, 30), (40, 30), (30, 23), (30, 40)] {
        assert!(
            near(at(&outlined, x, y), [235, 235, 240, 255]),
            "({x},{y}) {:?}",
            at(&outlined, x, y)
        );
    }
    // The mode is not sticky on the compositor: the next plain viewport frame is filled again, byte for byte.
    assert_eq!(pixels(&mut gpu, &viewport(&snapshot)), plain);
}

#[test]
fn gpu_an_export_never_contains_the_outline_and_stays_byte_identical() {
    let mut gpu = Compositor::new().expect("actual headless GPU is required; never skip");
    for background in [Some("#00000000"), Some("#336699"), None] {
        let owner = owner(background);
        let snapshot = owner.acquire_snapshot(0).unwrap();
        let export = prepare_object_composition(&plan(&snapshot), &pins(&snapshot)).unwrap();
        let before = pixels(&mut gpu, &export);
        // The same compositor paints an outlined viewport frame in between: nothing of it may leak into an export.
        let outlined = pixels(&mut gpu, &viewport(&snapshot).with_viewport_outline(0.5).unwrap());
        assert!(near(at(&outlined, 24, 30), [235, 235, 240, 255]));
        let after = pixels(&mut gpu, &export);
        assert_eq!(before, after, "{background:?}");
        assert!(inside_square(30, 30) && at(&after, 30, 30) == [255, 0, 0, 255]);
    }
}

#[test]
fn gpu_the_outline_keeps_its_screen_width_under_viewport_navigation() {
    let mut gpu = Compositor::new().expect("actual headless GPU is required; never skip");
    let owner = red_square();
    let snapshot = owner.acquire_snapshot(0).unwrap();
    // Zoom 2 about the centre: the square edge at x = 24 lands at 16, and the stroke is the baseline 1 / zoom = 0.5 document unit
    // wide floored at 0.75, so 1.5 screen px: the pixels either side of the edge are covered, the ones further away are not.
    let witness = prepare_object_navigated_viewport_composition(
        &plan(&snapshot),
        &pins(&snapshot),
        None,
        false,
        2.,
        [0., 0.],
    )
    .unwrap()
    .with_viewport_outline(1.)
    .unwrap();
    assert_eq!(witness.viewport_outline(), Some(0.75));
    let outlined = pixels(&mut gpu, &witness);
    assert!(at(&outlined, 16, 32)[3] > 100 && near(at(&outlined, 16, 32), [235, 235, 240, at(&outlined, 16, 32)[3]]));
    assert_eq!(at(&outlined, 24, 32)[3], 0, "the fill is dropped");
    assert_eq!(at(&outlined, 12, 32)[3], 0);
}
