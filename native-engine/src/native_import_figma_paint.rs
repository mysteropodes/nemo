//! Matrices, paint resolution and geometry-to-SVG tags of the Figma importer (L10-E, G36).
//!
//! Ports `matIdentity`, `matMultiply`, `matInvert`, `nodeAbsMatrix`, `clamp01`, `firstSolidPaint`,
//! `paintToCss`, `firstImagePaint` and `geometryToPathTags` of `src/js/figma-import.js` at the agreed
//! desktop baseline `fbe4b8a3` (lines 98-190). The arithmetic is written in the baseline's operand
//! order so every double matches bit for bit; the strings go through the JavaScript number printer.
//!
//! Baseline facts kept on purpose:
//! - `fills` / `strokes` are ordered bottom to top, so the LAST visible SOLID paint wins (the
//!   2026-08-29 colour fix) and the last visible IMAGE paint with a non-empty `imageRef` wins;
//! - `paintToCss` multiplies paint opacity, node opacity and colour alpha (each defaulting to 1) in
//!   that order and clamps the product, so a missing colour is opaque black;
//! - `geometryToPathTags` groups the path data by winding rule in first-seen order, joins each group
//!   with single spaces, trims it like `String.prototype.trim`, and yields no tag at all for an empty
//!   array or a missing colour (the caller reports that, see `walk`).

use crate::history::object_history::object_shot_kernels::native_import_figma_walk::jsnum::{
    js_number_to_string, js_round, js_trim,
};
use crate::history::object_history::object_shot_kernels::native_import_figma_walk::model::{
    Color, FigmaNode, GeomPath, Mat, Paint, PaintKind,
};

/// `matIdentity()`.
pub(crate) fn mat_identity() -> Mat {
    Mat {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    }
}

/// `matMultiply(m1, m2)`: applying the result equals applying `m2` then `m1`.
pub(crate) fn mat_multiply(m1: &Mat, m2: &Mat) -> Mat {
    Mat {
        a: m1.a * m2.a + m1.c * m2.b,
        b: m1.b * m2.a + m1.d * m2.b,
        c: m1.a * m2.c + m1.c * m2.d,
        d: m1.b * m2.c + m1.d * m2.d,
        e: m1.a * m2.e + m1.c * m2.f + m1.e,
        f: m1.b * m2.e + m1.d * m2.f + m1.f,
    }
}

/// `matInvert(m)`: a zero or NaN determinant answers the identity (the baseline's `!det`).
pub(crate) fn mat_invert(m: &Mat) -> Mat {
    let det = m.a * m.d - m.b * m.c;
    if det == 0.0 || det.is_nan() {
        return mat_identity();
    }
    Mat {
        a: m.d / det,
        b: -m.b / det,
        c: -m.c / det,
        d: m.a / det,
        e: (m.c * m.f - m.d * m.e) / det,
        f: (m.b * m.e - m.a * m.f) / det,
    }
}

/// `nodeAbsMatrix(node)`: `absoluteTransform` when it has exactly two rows (a missing entry reads as
/// `undefined`, i.e. NaN), else a pure translation from `absoluteBoundingBox`, else the origin.
pub(crate) fn node_abs_matrix(node: &FigmaNode) -> Mat {
    if let Some(t) = &node.abs_transform {
        if t.len() == 2 {
            let at = |row: usize, col: usize| t[row].get(col).copied().unwrap_or(f64::NAN);
            return Mat {
                a: at(0, 0),
                c: at(0, 1),
                e: at(0, 2),
                b: at(1, 0),
                d: at(1, 1),
                f: at(1, 2),
            };
        }
    }
    let (e, f) = match &node.abs_bbox {
        Some(bb) => (bb.x, bb.y),
        None => (0.0, 0.0),
    };
    Mat {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e,
        f,
    }
}

/// `clamp01(v)`: NaN passes through unchanged.
pub(crate) fn clamp01(v: f64) -> f64 {
    if v < 0.0 {
        0.0
    } else if v > 1.0 {
        1.0
    } else {
        v
    }
}

/// `firstSolidPaint(paints)`: the last (top-most) visible SOLID paint.
pub(crate) fn first_solid_paint(paints: &[Paint]) -> Option<&Paint> {
    paints
        .iter()
        .rev()
        .find(|p| p.visible != Some(false) && p.kind == PaintKind::Solid)
}

/// `firstImagePaint(paints)`: the last visible IMAGE paint that has a non-empty `imageRef`.
pub(crate) fn first_image_paint(paints: &[Paint]) -> Option<&Paint> {
    paints.iter().rev().find(|p| {
        p.visible != Some(false)
            && p.kind == PaintKind::Image
            && matches!(p.image_ref.as_deref(), Some(r) if !r.is_empty())
    })
}

/// One colour channel: `Math.round(clamp01(v) * 255)` printed as a JavaScript number.
fn channel(v: f64) -> String {
    js_number_to_string(js_round(clamp01(v) * 255.0))
}

/// `paintToCss(paint, nodeOpacity)`: `None` is the baseline's `null` for a missing paint.
pub(crate) fn paint_to_css(paint: Option<&Paint>, node_opacity: Option<f64>) -> Option<String> {
    let paint = paint?;
    let c = paint.color.unwrap_or(Color {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: Some(1.0),
    });
    let alpha =
        clamp01(paint.opacity.unwrap_or(1.0) * node_opacity.unwrap_or(1.0) * c.a.unwrap_or(1.0));
    Some(format!(
        "rgba({},{},{},{})",
        channel(c.r),
        channel(c.g),
        channel(c.b),
        js_number_to_string(alpha)
    ))
}

/// The `matrix(a b c d e f)` attribute value built from the JavaScript number printer.
pub(crate) fn matrix_attr(mat: &Mat) -> String {
    let parts = [mat.a, mat.b, mat.c, mat.d, mat.e, mat.f];
    let texts: Vec<String> = parts.iter().map(|v| js_number_to_string(*v)).collect();
    format!("matrix({})", texts.join(" "))
}

/// `geometryToPathTags(geomArr, cssColor, mat)`: one `<path>` per winding rule, in first-seen order.
pub(crate) fn geometry_to_path_tags(
    geometry: &[GeomPath],
    css_color: Option<&str>,
    mat: &Mat,
) -> Vec<String> {
    let css = match css_color {
        Some(text) if !text.is_empty() => text,
        _ => return Vec::new(),
    };
    let mut groups: Vec<(&'static str, String)> = Vec::new();
    for g in geometry {
        if g.path.is_empty() {
            continue;
        }
        let rule = if g.winding_rule.as_deref() == Some("EVENODD") {
            "evenodd"
        } else {
            "nonzero"
        };
        match groups.iter_mut().find(|entry| entry.0 == rule) {
            Some(entry) => {
                entry.1.push(' ');
                entry.1.push_str(&g.path);
            }
            None => {
                let mut text = String::from(" ");
                text.push_str(&g.path);
                groups.push((rule, text));
            }
        }
    }
    let transform = matrix_attr(mat);
    groups
        .iter()
        .map(|entry| {
            format!(
                "<path transform=\"{}\" d=\"{}\" fill-rule=\"{}\" fill=\"{}\"/>",
                transform,
                js_trim(&entry.1),
                entry.0,
                css
            )
        })
        .collect()
}

#[cfg(test)]
#[path = "../tests/native_import_figma_paint.rs"]
mod tests;
