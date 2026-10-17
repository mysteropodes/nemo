//! Figma node-walk kernel (L10-E, G36): the pure decisions of `walkNode` in `src/js/figma-import.js`.
//!
//! Port of `walkNode` (baseline `fbe4b8a3`, lines 233-290) over the typed [`FigmaNode`] model. It
//! collects, in frame-local space (`frame_inv * node absolute matrix`), SVG `<path>` tags for the
//! vector geometry, text descriptors, image-fill descriptors and the baseline's skipped messages.
//! Network, token, font fetching, image download and Paper/Raster/vector-text insertion are NOT here:
//! the adapter parses the response into the model (see `model`), the plan child (`plan`) turns the
//! walk output into a typed frame plan, and texts/images that need a provider stay typed requests.
//!
//! Baseline order inside one node, which the oracle rows pin:
//! 1. an invisible node (`visible === false`) is ignored with its whole subtree;
//! 2. an image fill wins over `fillGeometry` (the 2026-08-30 "image vanishes" fix) and still emits its
//!    `strokeGeometry`; the node is a leaf;
//! 3. any `fillGeometry` / `strokeGeometry` makes the node a leaf; fill then stroke, each painted with
//!    the top-most visible SOLID paint;
//! 4. a TEXT node becomes a text record (colour defaults to `rgba(0,0,0,1)`, width to 200);
//! 5. a container recurses; a childless node other than GROUP is skipped as `TYPE (name)`.
//!
//! The baseline is silent in three places. The kernel keeps its output identical and adds typed,
//! additive records instead of dropping: a geometry array that produced no tag
//! ([`Unpaintable`]), children of a node that ended the walk ([`Unwalked`]), and (in `plan`) empty
//! texts and invisible frames. The baseline's dead `no paintable geometry` message (guarded by a
//! condition that was already false) is not reproduced.

use crate::history::object_history::object_shot_kernels::native_import_figma_walk::model::{
    FigmaNode, GeometryRole, ImageRecord, Mat, TextRecord, Unpaintable, UnpaintableReason,
    Unwalked, UnwalkedCause, WalkOut,
};
use crate::history::object_history::object_shot_kernels::native_import_figma_walk::paint::{
    first_image_paint, first_solid_paint, geometry_to_path_tags, mat_multiply, node_abs_matrix,
    paint_to_css,
};

#[path = "native_import_figma_ids.rs"]
pub(crate) mod ids;
#[path = "native_import_figma_jsnum.rs"]
pub(crate) mod jsnum;
#[path = "native_import_figma_model.rs"]
pub(crate) mod model;
#[path = "native_import_figma_paint.rs"]
pub(crate) mod paint;
#[path = "native_import_figma_plan.rs"]
pub(crate) mod plan;
#[path = "native_import_figma_requests.rs"]
pub(crate) mod requests;

/// Native stack guard: a chain of containers deeper than this is reported, not followed.
pub(crate) const MAX_WALK_DEPTH: usize = 512;

/// The skipped-message text for `node.type` (`undefined` when absent).
fn type_text(node: &FigmaNode) -> String {
    match &node.node_type {
        Some(text) => text.clone(),
        None => "undefined".to_string(),
    }
}

/// `walkNode(node, frameInv, out)`.
pub(crate) fn walk_node(node: &FigmaNode, frame_inv: &Mat, out: &mut WalkOut) {
    walk_at(node, frame_inv, out, 0);
}

fn note_unwalked(out: &mut WalkOut, node: &FigmaNode, cause: UnwalkedCause) {
    if node.children.is_empty() {
        return;
    }
    out.unwalked.push(Unwalked {
        name: node.name.clone(),
        node_type: node.node_type.clone(),
        children: node.children.len(),
        cause,
    });
}

/// Appends the tags of one geometry array, or reports why it produced none.
fn emit_geometry(
    out: &mut WalkOut,
    node: &FigmaNode,
    role: GeometryRole,
    css: Option<String>,
    mat: &Mat,
) {
    let geometry = match role {
        GeometryRole::Fill => &node.fill_geometry,
        GeometryRole::Stroke => &node.stroke_geometry,
    };
    let tags = geometry_to_path_tags(geometry, css.as_deref(), mat);
    if tags.is_empty() {
        let reason = if matches!(css.as_deref(), Some(text) if !text.is_empty()) {
            UnpaintableReason::EmptyPaths
        } else {
            UnpaintableReason::NoSolidPaint
        };
        out.unpaintable.push(Unpaintable {
            name: node.name.clone(),
            node_type: node.node_type.clone(),
            role,
            reason,
        });
    }
    out.svg_tags.extend(tags);
}

fn walk_at(node: &FigmaNode, frame_inv: &Mat, out: &mut WalkOut, depth: usize) {
    if node.visible == Some(false) {
        return;
    }
    let mat = mat_multiply(frame_inv, &node_abs_matrix(node));

    if let Some(image_paint) = first_image_paint(&node.fills) {
        let (w, h) = match &node.abs_bbox {
            Some(bb) => (bb.width, bb.height),
            None => (0.0, 0.0),
        };
        out.images.push(ImageRecord {
            image_ref: image_paint.image_ref.clone().unwrap_or_default(),
            mat,
            w,
            h,
            opacity: node.opacity.unwrap_or(1.0),
            name: node.name.clone(),
        });
        if !node.stroke_geometry.is_empty() {
            let css = paint_to_css(first_solid_paint(&node.strokes), node.opacity);
            emit_geometry(out, node, GeometryRole::Stroke, css, &mat);
        }
        note_unwalked(out, node, UnwalkedCause::ImageFill);
        return;
    }

    let has_fill_geometry = !node.fill_geometry.is_empty();
    let has_stroke_geometry = !node.stroke_geometry.is_empty();
    if has_fill_geometry || has_stroke_geometry {
        if has_fill_geometry {
            let css = paint_to_css(first_solid_paint(&node.fills), node.opacity);
            emit_geometry(out, node, GeometryRole::Fill, css, &mat);
        }
        if has_stroke_geometry {
            let css = paint_to_css(first_solid_paint(&node.strokes), node.opacity);
            emit_geometry(out, node, GeometryRole::Stroke, css, &mat);
        }
        note_unwalked(out, node, UnwalkedCause::GeometryLeaf);
        return;
    }

    if node.node_type.as_deref() == Some("TEXT") {
        let width = match &node.abs_bbox {
            Some(bb) => bb.width,
            None => 200.0,
        };
        let color = paint_to_css(first_solid_paint(&node.fills), node.opacity)
            .unwrap_or_else(|| "rgba(0,0,0,1)".to_string());
        out.texts.push(TextRecord {
            text: node.characters.clone().unwrap_or_default(),
            name: node.name.clone(),
            x: mat.e,
            y: mat.f,
            w: width,
            style: node.style.clone(),
            color,
        });
        return;
    }

    if !node.children.is_empty() {
        if depth + 1 >= MAX_WALK_DEPTH {
            note_unwalked(out, node, UnwalkedCause::DepthLimit);
            return;
        }
        for child in &node.children {
            walk_at(child, frame_inv, out, depth + 1);
        }
        return;
    }

    if node.node_type.as_deref() != Some("GROUP") {
        let name = match node.name.as_deref() {
            Some(text) if !text.is_empty() => text,
            _ => "?",
        };
        out.skipped.push(format!("{} ({})", type_text(node), name));
    }
}

#[cfg(test)]
#[path = "../tests/native_import_figma_walk_rows.rs"]
mod rows;

#[cfg(test)]
#[path = "../tests/native_import_figma_walk_decode.rs"]
mod decode;

#[cfg(test)]
#[path = "../tests/native_import_figma_walk_flows.rs"]
mod flows;

#[cfg(test)]
#[path = "../tests/native_import_figma_walk.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/native_import_figma_walk_controls.rs"]
mod controls;
