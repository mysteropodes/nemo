//! Ring helpers of the text scene join (G38b / L06-K): the baseline underline /
//! strike rectangle and the opt-in coincident-curve normalization.
//!
//! `decoration_segments` is `new Path.Rectangle(new Point(startX, 0), new
//! Size(lineWidth, thickness))` cloned and moved with `clone.position.y =
//! centerY` in `buildVectorTextGroup` (vector-text-bridge.js at fbe4b8a3):
//! Paper lists the corners bottom-left, top-left, top-right, bottom-right with
//! zero handles, and `position.y = v` is a translate by `v - boundsCenterY`
//! where the centre of the unmoved rectangle is `0 + thickness / 2`. The
//! anchors are therefore `y + (centerY - thickness / 2)` and `x` unchanged;
//! the layout kernel pins `centerY` as the exact target, so the shift is the
//! only rounding Paper adds, and this function reproduces it.
//!
//! `drop_coincident_curves` is NOT baseline behavior. Real glyph outlines
//! often carry a zero-length curve (opentype.js emits an `L` that repeats the
//! `M`, 66 of 164 Roboto ASCII paths), which `ClosedCubicPath::new` refuses
//! because Vello drops such a curve anyway. The normalization removes exactly
//! the segment that ends a curve whose controls and end coincide with its start
//! at f32 precision, carrying the removed segment's outgoing control to the
//! previous anchor (as an absolute point, so the traced curves are unchanged).
//! It is selected explicitly by the caller (`GeometryAdmission`); the strict
//! policy never calls it.

use crate::evaluation::object_text_layout_kernels::object_text_layout_kernel::model::Decoration;
use crate::evaluation::object_text_layout_kernels::text_glyph_contours::{
    path_flags, GlyphSegment, PathFlag,
};

fn corner(x: f64, y: f64) -> GlyphSegment {
    GlyphSegment {
        point: [x, y],
        handle_in: [0.0, 0.0],
        handle_out: [0.0, 0.0],
    }
}

/// The four anchors Paper gives the moved rectangle clone of one decoration.
pub(crate) fn decoration_segments(decoration: &Decoration) -> Vec<GlyphSegment> {
    let shift = decoration.center_y - (0.0 + decoration.thickness / 2.0);
    let right = decoration.left + decoration.width;
    let top = 0.0 + shift;
    let bottom = (0.0 + decoration.thickness) + shift;
    vec![
        corner(decoration.left, bottom),
        corner(decoration.left, top),
        corner(right, top),
        corner(right, bottom),
    ]
}

fn first_degenerate(ring: &[GlyphSegment]) -> Option<usize> {
    path_flags(ring).into_iter().find_map(|flag| match flag {
        PathFlag::DegenerateCurve(index) => Some(index),
        PathFlag::SegmentCount(_) => None,
    })
}

fn absolute(anchor: [f64; 2], handle: [f64; 2]) -> [f64; 2] {
    [anchor[0] + handle[0], anchor[1] + handle[1]]
}

/// Remove coincident curves until none is left or only two segments remain
/// (a ring that small is refused by the admission, never silently emptied).
pub(crate) fn drop_coincident_curves(mut ring: Vec<GlyphSegment>) -> Vec<GlyphSegment> {
    while ring.len() > 2 {
        let Some(index) = first_degenerate(&ring) else {
            break;
        };
        let next = (index + 1) % ring.len();
        if next != 0 {
            // The curve index -> next is a point: next's outgoing curve now
            // starts at index and keeps its absolute first control.
            let control = absolute(ring[next].point, ring[next].handle_out);
            let anchor = ring[index].point;
            ring[index].handle_out = [control[0] - anchor[0], control[1] - anchor[1]];
            ring.remove(next);
        } else {
            // The closing curve (last -> first) is a point: the last segment
            // goes and segment 0 takes over its incoming control.
            let control = absolute(ring[index].point, ring[index].handle_in);
            let anchor = ring[0].point;
            ring[0].handle_in = [control[0] - anchor[0], control[1] - anchor[1]];
            ring.remove(index);
        }
    }
    ring
}
