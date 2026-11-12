//! #2356 point-type semantics (owner decision, Ilya 2026-10-31, replacing the 1065e9d8 baseline rules).
//!
//! Smooth: the two handles are collinear and their lengths may differ. Symmetric: collinear and equal. Corner: the
//! tangent angle is broken, both handles are KEPT and move independently. `collapse` removes the handles (what the
//! baseline's Corner did). An optional per-vertex hint (`vectorExt.pointTypes`) records the chosen type; without it the
//! type is inferred from the geometry. Pure functions over detached anchors, no document authority.
use crate::object_document::subselect::Anchor;

/// Handles shorter than this carry no direction.
const MIN: f64 = 1e-9;
/// Relative tolerance of the collinearity and equal-length tests.
const EPS: f64 = 1e-5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Corner,
    Smooth,
    Symmetric,
}
impl Kind {
    pub(crate) fn parse(name: &str) -> Option<Kind> {
        match name {
            "corner" => Some(Kind::Corner),
            "smooth" => Some(Kind::Smooth),
            "symmetric" => Some(Kind::Symmetric),
            _ => None,
        }
    }
    pub(crate) fn name(self) -> &'static str {
        match self {
            Kind::Corner => "corner",
            Kind::Smooth => "smooth",
            Kind::Symmetric => "symmetric",
        }
    }
}
fn len(v: [f64; 2]) -> f64 {
    (v[0] * v[0] + v[1] * v[1]).sqrt()
}

/// The type a vertex's geometry shows: non-collinear handles are a corner, collinear equal ones symmetric, collinear
/// unequal ones smooth. A missing (zero) handle is collinear with anything.
pub(crate) fn infer(a: &Anchor) -> Kind {
    let (i, o) = (a.handle_in, a.handle_out);
    let (li, lo) = (len(i), len(o));
    if li > MIN && lo > MIN {
        let cross = i[0] * o[1] - i[1] * o[0];
        let dot = i[0] * o[0] + i[1] * o[1];
        if dot >= 0.0 || cross.abs() > EPS * li * lo {
            return Kind::Corner;
        }
    }
    if (li - lo).abs() <= EPS * li.max(lo).max(1.0) {
        Kind::Symmetric
    } else {
        Kind::Smooth
    }
}
/// The type of vertex `index`: the stored hint when there is one, else the inferred type.
pub(crate) fn effective(hints: Option<&[Kind]>, anchors: &[Anchor], index: usize) -> Kind {
    hints
        .and_then(|h| h.get(index).copied())
        .unwrap_or_else(|| infer(&anchors[index]))
}
/// The hint array a first typed edit starts from: every vertex at its inferred type.
pub(crate) fn inferred_all(anchors: &[Anchor]) -> Vec<Kind> {
    anchors.iter().map(infer).collect()
}
/// A plain (not Alt) drag of one handle: where the opposite handle goes, `None` to leave it as it is.
/// Symmetric mirrors it, Smooth keeps its own length on the opposite direction, Corner never touches it.
pub(crate) fn opposite_of_drag(
    kind: Kind,
    dragged: [f64; 2],
    opposite: [f64; 2],
) -> Option<[f64; 2]> {
    match kind {
        Kind::Corner => None,
        Kind::Symmetric => Some([-dragged[0], -dragged[1]]),
        Kind::Smooth => {
            let d = len(dragged);
            if d <= MIN {
                return None;
            }
            let k = -len(opposite) / d;
            Some([dragged[0] * k, dragged[1] * k])
        }
    }
}
/// The unit tangent a conversion to Smooth or Symmetric keeps: the bisector of the out handle and the reversed in handle,
/// the one handle's own direction when only one is present, and `None` when neither is (or they cancel), so the caller
/// falls back to the neighbour chord.
pub(crate) fn current_tangent(a: &Anchor) -> Option<[f64; 2]> {
    let unit = |v: [f64; 2]| (len(v) > 0.01).then(|| [v[0] / len(v), v[1] / len(v)]);
    match (unit(a.handle_out), unit([-a.handle_in[0], -a.handle_in[1]])) {
        (Some(o), Some(i)) => {
            let s = [o[0] + i[0], o[1] + i[1]];
            (len(s) > 1e-9).then(|| [s[0] / len(s), s[1] / len(s)])
        }
        (Some(d), None) | (None, Some(d)) => Some(d),
        (None, None) => None,
    }
}

#[cfg(test)]
#[path = "../tests/object_subselect_point_type.rs"]
pub(crate) mod tests;
