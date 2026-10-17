//! Tests of the vello replay of a planned image mesh (L03-S, acceptance bullet 2).
//!
//! No GPU is touched: a `vello::Scene` is an encoding, so the scene and the export of a frame can be
//! compared field by field (path tags and data, draw tags and data, transforms, clip counters). The
//! reference below is the baseline `draw_image_mesh` call sequence written out literally (a
//! `Rect::to_path` outline for the bounding-box fallback, hand-built triangle paths), so the replay
//! is compared with the shipped renderer's calls and not with itself.

use super::{replay_mesh_item, ReplayError, ReplayReport};
use crate::evaluation::motion_parent_chain::{ChainEntry, ImageRect, MotionMat};
use crate::evaluation::object_image_mesh_kernels::render_image_mesh_plan::{
    MeshDrawPlan, OutlineKind,
};
use crate::evaluation::object_image_mesh_kernels::render_image_mesh_scene_source::oracle_rows::{
    mesh_rows, plain_source, row_named, store_of,
};
use crate::object_render_scene::render_image_mesh_kernels::render_image_mesh_scene::image_helpers::{
    prepare_image_row, raster_image,
};
use crate::object_render_scene::render_image_mesh_kernels::render_image_mesh_scene::{
    prepare_mesh_scene_item, MeshDraw, MeshSceneItem, MeshSceneSource,
};
use std::sync::Arc;
use vello::kurbo::{Affine, BezPath, Rect, Shape};
use vello::peniko::{Blob, Fill, ImageAlphaType, ImageBrush, ImageData, ImageFormat, Mix};
use vello::Scene;

const SHORT_OUTLINE: &str = r#"{"outline":[[0,0],[1,0]],"verts":[[0,0],[1,0],[1,1],[0,1],[0.5,0.5]],"tris":[4,0,1,4,1,2,4,2,3,4,3,0],"offsets":[[0,0],[0,0],[0,0],[0,0],[0,0]],"cols":1,"rows":1}"#;

fn pixels(width: u32, height: u32) -> ImageData {
    ImageData {
        data: Blob::new(Arc::new(vec![255u8; (width * height * 4) as usize])),
        format: ImageFormat::Rgba8,
        alpha_type: ImageAlphaType::Alpha,
        width,
        height,
    }
}

fn same_encoding(a: &Scene, b: &Scene) -> bool {
    let (a, b) = (a.encoding(), b.encoding());
    a.path_tags == b.path_tags
        && a.path_data == b.path_data
        && a.draw_tags == b.draw_tags
        && a.draw_data == b.draw_data
        && a.transforms == b.transforms
        && (a.n_paths, a.n_clips, a.n_open_clips) == (b.n_paths, b.n_clips, b.n_open_clips)
}

fn replayed(item: &MeshSceneItem<()>) -> Scene {
    let (w, h) = item.image().pixel_size();
    let mut scene = Scene::new();
    replay_mesh_item(&mut scene, item, &pixels(w, h)).expect("replays");
    scene
}

/// The baseline `draw_image_mesh` calls, in the baseline's order, for an already planned mesh.
fn baseline_calls(scene: &mut Scene, plan: &MeshDrawPlan, view: [f64; 6], image: &ImageData) {
    let view_tf = Affine::new(view);
    let outline: BezPath = if plan.outline.kind == OutlineKind::Polygon {
        let mut path = BezPath::new();
        for (k, p) in plan.outline.points.iter().enumerate() {
            if k == 0 {
                path.move_to((p[0], p[1]));
            } else {
                path.line_to((p[0], p[1]));
            }
        }
        path.close_path();
        path
    } else {
        let (a, c) = (plan.outline.points[0], plan.outline.points[2]);
        Rect::new(a[0], a[1], c[0], c[1]).to_path(0.1)
    };
    scene.push_layer(Fill::NonZero, Mix::Normal, plan.opacity, view_tf, &outline);
    let brush = ImageBrush::new(image.clone());
    for t in &plan.triangles {
        let mut tri = BezPath::new();
        tri.move_to((t.clip[0][0], t.clip[0][1]));
        tri.line_to((t.clip[1][0], t.clip[1][1]));
        tri.line_to((t.clip[2][0], t.clip[2][1]));
        tri.close_path();
        scene.fill(
            Fill::NonZero,
            view_tf,
            &brush,
            Some(Affine::new(t.place)),
            &tri,
        );
    }
    scene.pop_layer();
}

fn translate(dx: f64) -> Vec<ChainEntry> {
    vec![ChainEntry {
        mat: MotionMat {
            dx,
            dy: 0.0,
            rot: 0.0,
            sx: 1.0,
            sy: 1.0,
            op: 1.0,
            ax: 0.0,
            ay: 0.0,
        },
        pivot: [74.0, 68.0],
    }]
}

fn grid_item(stages: &[ChainEntry], opacity: f64, view: [f64; 6]) -> MeshSceneItem<()> {
    let rows = mesh_rows();
    let row = &rows[row_named(&rows, "grid4_identity")];
    let image = raster_image(row.base, stages, opacity, row.image);
    let store = store_of(&row.mesh);
    let source = MeshSceneSource {
        view,
        ..plain_source("im_1")
    };
    prepare_mesh_scene_item(&image, &source, Some(&store))
        .unwrap()
        .unwrap()
}

const ID: [f64; 6] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

#[test]
fn a_planned_mesh_replays_one_clip_layer_and_one_image_fill_per_triangle() {
    let rows = mesh_rows();
    let (_, item) = prepare_image_row(&rows[row_named(&rows, "grid4_identity")]);
    let tris = item.triangle_count();
    assert_eq!(
        tris,
        rows[row_named(&rows, "grid4_identity")].expect.kept.len()
    );
    assert!(tris > 0);
    let mut scene = Scene::new();
    let report = replay_mesh_item(&mut scene, &item, &pixels(64, 64)).unwrap();
    assert_eq!(
        report,
        ReplayReport {
            clip_layers: 1,
            fills: tris
        }
    );
    let enc = scene.encoding();
    assert_eq!(
        (enc.n_clips, enc.n_open_clips),
        (2, 0),
        "vello counts begin and end of the one layer"
    );
    assert_eq!(
        enc.draw_tags.len(),
        tris + 2,
        "begin clip, one image per triangle, end clip"
    );
    assert_eq!(
        enc.draw_data.len(),
        2 + 3 * tris,
        "two words for the layer, three per image fill"
    );
}

#[test]
fn the_replay_equals_the_baseline_call_sequence_for_a_polygon_and_a_bounding_box_outline() {
    let rows = mesh_rows();
    for name in [
        "grid4_translate",
        "corpus_stored_offset",
        "diamond_outline",
        "grid4_view_zoom_2",
    ] {
        let (_, item) = prepare_image_row(&rows[row_named(&rows, name)]);
        let (w, h) = item.image().pixel_size();
        let mut expected = Scene::new();
        baseline_calls(
            &mut expected,
            item.plan().unwrap(),
            item.view(),
            &pixels(w, h),
        );
        assert!(same_encoding(&replayed(&item), &expected), "{name}");
    }
    let base = ImageRect {
        x: 100.0,
        y: 100.0,
        width: 50.0,
        height: 50.0,
        rotation: 0.0,
    };
    let image = raster_image(base, &[], 1.0, (25, 25));
    let store = store_of(SHORT_OUTLINE);
    let item = prepare_mesh_scene_item(&image, &plain_source("im_1"), Some(&store))
        .unwrap()
        .unwrap();
    assert_eq!(item.plan().unwrap().outline.kind, OutlineKind::BoundingBox);
    let mut expected = Scene::new();
    baseline_calls(&mut expected, item.plan().unwrap(), ID, &pixels(25, 25));
    assert!(
        same_encoding(&replayed(&item), &expected),
        "the four-corner polygon is Rect::to_path"
    );
}

#[test]
fn opacity_is_carried_once_by_the_outer_layer_and_never_by_a_triangle() {
    let opaque = replayed(&grid_item(&[], 1.0, ID));
    let half = replayed(&grid_item(&[], 0.5, ID));
    let (a, b) = (opaque.encoding(), half.encoding());
    assert!(a.draw_tags == b.draw_tags);
    assert_eq!(a.path_data, b.path_data);
    let differing = a
        .draw_data
        .iter()
        .zip(b.draw_data.iter())
        .filter(|(x, y)| x != y)
        .count();
    assert_eq!(
        differing, 1,
        "only the layer's alpha word changes with the item opacity"
    );
}

#[test]
fn scene_and_export_emit_identical_scenes_for_three_sampled_frames() {
    let mut encodings = Vec::new();
    for dx in [0.0, 7.0, 19.5] {
        let stages = translate(dx);
        let scene_item = grid_item(&stages, 0.8, ID);
        let export_item = grid_item(&stages, 0.8, ID);
        let again = scene_item.for_view(ID);
        let (scene, export, third) = (
            replayed(&scene_item),
            replayed(&export_item),
            replayed(&again),
        );
        assert!(same_encoding(&scene, &export), "dx {dx}: scene vs export");
        assert!(
            same_encoding(&scene, &third),
            "dx {dx}: for_view with the same view"
        );
        encodings.push(scene);
    }
    assert!(!same_encoding(&encodings[0], &encodings[1]));
    assert!(!same_encoding(&encodings[1], &encodings[2]));
}

#[test]
fn another_export_view_changes_the_transform_and_the_seam_clips_but_not_the_draw_structure() {
    let item = grid_item(&[], 1.0, ID);
    let zoomed = item.for_view([2.0, 0.0, 0.0, 2.0, 100.0, 50.0]);
    let (a, b) = (replayed(&item), replayed(&zoomed));
    let (ea, eb) = (a.encoding(), b.encoding());
    assert!(ea.draw_tags == eb.draw_tags);
    assert_eq!(
        ea.draw_data, eb.draw_data,
        "image and layer words do not depend on the view"
    );
    assert_ne!(ea.transforms, eb.transforms);
    assert_ne!(
        ea.path_data, eb.path_data,
        "the half-device-pixel dilation follows the view"
    );
}

#[test]
fn pixels_that_are_not_the_planned_image_are_refused_before_any_scene_call() {
    let item = grid_item(&[], 1.0, ID);
    let mut scene = Scene::new();
    let error = replay_mesh_item(&mut scene, &item, &pixels(32, 64)).unwrap_err();
    assert_eq!(
        error,
        ReplayError::ImageSizeMismatch {
            plan: (64, 64),
            image: (32, 64)
        }
    );
    assert_eq!(error.code(), "mesh-replay-image-size-mismatch");
    assert!(scene.encoding().is_empty());
}

#[test]
fn a_skipped_plan_replays_nothing_and_never_falls_back_to_a_flat_image() {
    let tiny = r#"{"outline":[],"verts":[[0,0],[1,1]],"tris":[0,1,0],"offsets":[[0,0],[0,0]],"cols":1,"rows":1}"#;
    let base = ImageRect {
        x: 0.0,
        y: 0.0,
        width: 10.0,
        height: 10.0,
        rotation: 0.0,
    };
    let image = raster_image(base, &[], 1.0, (8, 8));
    let store = store_of(tiny);
    let item = prepare_mesh_scene_item(&image, &plain_source("im_1"), Some(&store))
        .unwrap()
        .unwrap();
    assert!(matches!(item.draw(), MeshDraw::Nothing(_)));
    let mut scene = Scene::new();
    let report = replay_mesh_item(&mut scene, &item, &pixels(8, 8)).unwrap();
    assert_eq!(
        report,
        ReplayReport {
            clip_layers: 0,
            fills: 0
        }
    );
    assert!(scene.encoding().is_empty());
}
