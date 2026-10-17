//! Vello replay of a planned image mesh (F17, F18; leaf L03-S).
//!
//! The scene render and the export render of a meshed raster both call [`replay_mesh_item`] on the
//! same [`MeshSceneItem`] type, so they emit the same scene calls as the shipped renderer
//! `draw_image_mesh` (`geometry-wasm/src/engine.rs`, identical at fbe4b8a3 and at the pinned base):
//!
//! ```text
//! scene.push_layer(Fill::NonZero, Mix::Normal, opacity, view, &outline)
//! for each planned triangle:
//!     scene.fill(Fill::NonZero, view, &ImageBrush(image), Some(place), &dilated_triangle)
//! scene.pop_layer()
//! ```
//!
//! The outer layer clips the whole mesh to its outline (the mask) and carries the opacity ONCE; a
//! triangle's brush carries none, so a semi-transparent mesh does not double-composite along the
//! dilated seams. `place` is never dilated (the sampled pixels stay where they belong); the clip is.
//! `view` is the transform the plan was dilated for, read from the item, so the replay cannot use a
//! different one. A bounding-box outline is replayed as its four corner polygon, which is the path
//! `Rect::to_path` produces (move, three lines, close).
//!
//! `MeshDraw::Nothing` replays nothing: the baseline pushes no layer and draws no flat fallback.
//! The image must be the pixels the plan was built for: a size that differs from the item's
//! `pixel_size()` is refused before any scene call, because the plan's source triangles are in those
//! pixels.
//!
//! Not provided: the render target, readback or any GPU work (the compositor owns those), the image
//! upload and its retention (the item's handle and L06/L08 own that), and the plain-image draw of an
//! unmeshed raster.

use crate::object_render_scene::render_image_mesh_kernels::render_image_mesh_scene::{
    MeshDraw, MeshSceneItem,
};
use vello::kurbo::{Affine, BezPath};
use vello::peniko::{Fill, ImageBrush, ImageData, Mix};
use vello::Scene;

/// What one replay emitted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ReplayReport {
    /// Outer clip layers pushed (0 or 1).
    pub(crate) clip_layers: usize,
    /// Triangle fills emitted inside the layer.
    pub(crate) fills: usize,
}

/// Refusals before any scene call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReplayError {
    /// The pixels are not the image the plan was built for.
    ImageSizeMismatch { plan: (u32, u32), image: (u32, u32) },
}

impl ReplayError {
    pub(crate) fn code(&self) -> &'static str {
        match self {
            ReplayError::ImageSizeMismatch { .. } => "mesh-replay-image-size-mismatch",
        }
    }
}

/// A closed polygon path through `points` in order.
fn polygon(points: &[[f64; 2]]) -> BezPath {
    let mut path = BezPath::new();
    for (index, point) in points.iter().enumerate() {
        if index == 0 {
            path.move_to((point[0], point[1]));
        } else {
            path.line_to((point[0], point[1]));
        }
    }
    path.close_path();
    path
}

/// Replays the item's plan into `scene` with the given pixels.
pub(crate) fn replay_mesh_item<H>(
    scene: &mut Scene,
    item: &MeshSceneItem<H>,
    image: &ImageData,
) -> Result<ReplayReport, ReplayError> {
    let plan_size = item.image().pixel_size();
    let image_size = (image.width, image.height);
    if plan_size != image_size {
        return Err(ReplayError::ImageSizeMismatch {
            plan: plan_size,
            image: image_size,
        });
    }
    let plan = match item.draw() {
        MeshDraw::Plan(plan) => plan,
        MeshDraw::Nothing(_) => {
            return Ok(ReplayReport {
                clip_layers: 0,
                fills: 0,
            })
        }
    };
    let view = Affine::new(item.view());
    let outline = polygon(&plan.outline.points);
    scene.push_layer(Fill::NonZero, Mix::Normal, plan.opacity, view, &outline);
    let brush = ImageBrush::new(image.clone());
    for triangle in &plan.triangles {
        let clip = polygon(&triangle.clip);
        scene.fill(
            Fill::NonZero,
            view,
            &brush,
            Some(Affine::new(triangle.place)),
            &clip,
        );
    }
    scene.pop_layer();
    Ok(ReplayReport {
        clip_layers: 1,
        fills: plan.triangles.len(),
    })
}

#[cfg(test)]
#[path = "../tests/render_image_mesh_scene_replay.rs"]
mod tests;
