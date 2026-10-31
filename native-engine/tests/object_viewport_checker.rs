//! #2373: the viewport-only transparency checker is painted by the engine BEHIND the artwork; an export never has one.
//! Literal pixel oracles on the actual headless GPU; no AppKit claim.
use crate::compositor::{object_compositor::ObjectOutputSpec, Compositor};
use crate::history::NativeObjectHistory;
use crate::object_codec::decode_storage_project;
use crate::object_document::{FrameScopeKind, ProjectEnvelope, ProjectMetadata};
use crate::object_frame_packet::{prepare_object_frame, ObjectFrameSelector};
use crate::object_render_scene::viewport_checker::ViewportChecker;
use crate::object_render_scene::{
    paint_test_document, prepare_object_composition, prepare_object_navigated_viewport_composition,
    prepare_object_paint_plan, prepare_object_render_scene, prepare_object_viewport_composition,
    viewport_checker_quality_suffix, ObjectCompositionWitness,
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
        "checker",
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

#[test]
fn the_checker_grid_is_the_baseline_one() {
    // 64 x 64 canvas at zoom 1: 8 screen px tiles are 8 document units, 8 x 8 tiles.
    assert_eq!(
        ViewportChecker::new((64, 64), 1.).unwrap().tile_counts(),
        (8, 8)
    );
    // Twice the zoom, half the document tile.
    assert_eq!(
        ViewportChecker::new((64, 64), 2.).unwrap().tile_counts(),
        (16, 16)
    );
    // Far zoomed out the tile stays a whole canvas at least.
    assert_eq!(
        ViewportChecker::new((64, 64), 0.001).unwrap().tile_counts(),
        (1, 1)
    );
    // 1920 x 1080 at zoom 20 would need ~13e6 tiles: the tile grows until at most ~4000 remain (baseline MAX_CHECK_TILES).
    let (cols, rows) = ViewportChecker::new((1920, 1080), 20.)
        .unwrap()
        .tile_counts();
    assert!(
        cols * rows <= 4200 && cols * rows >= 3800,
        "{cols} x {rows}"
    );
    for bad in [0., -1., f64::NAN, f64::INFINITY] {
        assert!(ViewportChecker::new((64, 64), bad).is_none(), "{bad}");
    }
    assert!(ViewportChecker::new((0, 64), 1.).is_none());
}

#[test]
fn the_checker_is_part_of_the_viewport_cache_identity_and_only_a_viewport_witness_takes_it() {
    let owner = owner(Some("#00000000"));
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let plain = viewport(&snapshot);
    let one = viewport(&snapshot)
        .with_viewport_checker((64, 64), 1.)
        .unwrap();
    let two = viewport(&snapshot)
        .with_viewport_checker((64, 64), 2.)
        .unwrap();
    assert_eq!(
        one.expected_quality(),
        format!(
            "{}{}",
            plain.expected_quality(),
            viewport_checker_quality_suffix(1.)
        )
    );
    assert_ne!(one.expected_quality(), two.expected_quality());
    assert_ne!(one.expected_quality(), plain.expected_quality());
    let export = prepare_object_composition(&plan(&snapshot), &pins(&snapshot)).unwrap();
    assert!(
        export.clone().with_viewport_checker((64, 64), 1.).is_err(),
        "an export witness never carries the checker"
    );
    assert!(viewport(&snapshot)
        .with_viewport_checker((64, 64), f64::NAN)
        .is_err());
    assert_eq!(export.expected_quality(), "final");
}

#[test]
fn gpu_the_checker_is_behind_the_artwork_where_the_document_is_transparent() {
    let mut gpu = Compositor::new().expect("actual headless GPU is required; never skip");
    let owner = owner(Some("#00000000"));
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let plain = pixels(&mut gpu, &viewport(&snapshot));
    let checked = pixels(
        &mut gpu,
        &viewport(&snapshot)
            .with_viewport_checker((64, 64), 1.)
            .unwrap(),
    );
    let (mut transparent, mut opaque) = (0, 0);
    for y in 0..64 {
        for x in 0..64 {
            let (before, after) = (at(&plain, x, y), at(&checked, x, y));
            if before[3] == 0 {
                // Transparent document: the 8 px baseline checker shows, 222 on (column + row) even tiles, 255 on the others.
                let tile = if (x / 8 + y / 8) % 2 == 0 { 222 } else { 255 };
                assert!(near(after, grey(tile)), "({x},{y}) {after:?}");
                transparent += 1;
            } else if before[3] == 255 {
                // The artwork pixels are exactly what they were without the checker.
                assert_eq!(after, before, "({x},{y})");
                opaque += 1;
            }
        }
    }
    assert!(
        transparent > 3000 && opaque > 200,
        "{transparent} / {opaque}"
    );
    assert!(inside_square(30, 30) && at(&checked, 30, 30) == [255, 0, 0, 255]);
    // The first tile is the darker one, its right neighbour the lighter one: the checker is not a tint over the artwork.
    assert!(near(at(&checked, 4, 4), grey(222)) && near(at(&checked, 12, 4), grey(255)));
}

#[test]
fn gpu_the_checker_replaces_an_opaque_document_background_like_the_baseline() {
    let mut gpu = Compositor::new().expect("actual headless GPU is required; never skip");
    let owner = owner(Some("#336699"));
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let plain = pixels(&mut gpu, &viewport(&snapshot));
    let checked = pixels(
        &mut gpu,
        &viewport(&snapshot)
            .with_viewport_checker((64, 64), 1.)
            .unwrap(),
    );
    assert!(
        near(at(&plain, 4, 4), [51, 102, 153, 255]),
        "{:?}",
        at(&plain, 4, 4)
    );
    assert!(near(at(&checked, 4, 4), grey(222)) && near(at(&checked, 12, 4), grey(255)));
    assert_eq!(at(&checked, 30, 30), [255, 0, 0, 255]);
}

#[test]
fn gpu_the_checker_follows_viewport_navigation_and_keeps_its_screen_size() {
    let mut gpu = Compositor::new().expect("actual headless GPU is required; never skip");
    let owner = owner(Some("#00000000"));
    let snapshot = owner.acquire_snapshot(0).unwrap();
    // Zoom 2 about the centre: the witness' own navigation makes the checker's document tile 4, its screen tile still 8 px.
    let witness = prepare_object_navigated_viewport_composition(
        &plan(&snapshot),
        &pins(&snapshot),
        None,
        false,
        2.,
        [0., 0.],
    )
    .unwrap()
    .with_viewport_checker((64, 64), 1.)
    .unwrap();
    let checked = pixels(&mut gpu, &witness);
    for (x, y) in [(2, 2), (10, 2), (2, 10), (10, 10), (50, 2), (2, 50)] {
        let tile = if (x / 8 + y / 8) % 2 == 0 { 222 } else { 255 };
        assert!(
            near(at(&checked, x, y), grey(tile)),
            "({x},{y}) {:?}",
            at(&checked, x, y)
        );
    }
}

#[test]
fn gpu_an_export_never_contains_the_checker_and_stays_byte_identical() {
    let mut gpu = Compositor::new().expect("actual headless GPU is required; never skip");
    for background in [Some("#00000000"), Some("#336699"), None] {
        let owner = owner(background);
        let snapshot = owner.acquire_snapshot(0).unwrap();
        let export = prepare_object_composition(&plan(&snapshot), &pins(&snapshot)).unwrap();
        let before = pixels(&mut gpu, &export);
        // The same compositor paints a viewport frame WITH the checker in between: nothing of it may leak into an export.
        let viewport_checked = pixels(
            &mut gpu,
            &viewport(&snapshot)
                .with_viewport_checker((64, 64), 1.)
                .unwrap(),
        );
        assert!(near(at(&viewport_checked, 4, 4), grey(222)));
        let after = pixels(&mut gpu, &export);
        assert_eq!(before, after, "{background:?}");
        // The export keeps the document background: every pixel outside the artwork is the corner pixel, no checker tile.
        let corner = at(&after, 0, 0);
        for y in 0..64 {
            for x in 0..64 {
                assert!(
                    inside_square(x, y)
                        || at(&after, x, y) == corner
                        || (corner[3] == 0 && at(&after, x, y)[3] == 0),
                    "{background:?} ({x},{y}) {:?} vs {corner:?}",
                    at(&after, x, y)
                );
            }
        }
    }
}

#[test]
fn gpu_a_canvas_shown_smaller_than_its_composition_keeps_an_eight_screen_pixel_tile() {
    let mut gpu = Compositor::new().expect("actual headless GPU is required; never skip");
    let owner = owner(Some("#00000000"));
    let snapshot = owner.acquire_snapshot(0).unwrap();
    // The 64 px composition is shown in a 32 css px element: 8 screen px are 16 composition px, so the grid is 4 x 4.
    let checked = pixels(
        &mut gpu,
        &viewport(&snapshot)
            .with_viewport_checker((64, 64), 0.5)
            .unwrap(),
    );
    assert!(near(at(&checked, 4, 4), grey(222)) && near(at(&checked, 12, 12), grey(222)));
    assert!(near(at(&checked, 20, 4), grey(255)) && near(at(&checked, 4, 20), grey(255)));
    assert!(near(at(&checked, 20, 20), grey(222)));
}
