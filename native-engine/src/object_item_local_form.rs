//! #2372 A-image-text-flip-skew: the optional stored form of a raster item and of a text block beyond its centre, size and turn.
//!
//! Baseline (1065e9d8 select-bridge xform-scale / xform-skew, timeline.js flipHorizontal): a Paper `scale(-1, 1)` mirrored a Raster
//! or a text block and a skew-zone drag sheared them. The native records store `flipX` / `flipY` (absent reads false) and `skew`
//! (degrees of x-shear, absent reads 0), so a document that never mirrors or shears an item loads and saves byte-identical.
//!
//! The item's local linear map, applied about its centre (raster) or its anchor (text) before the stored turn, is
//! `R(rotation) * F * Sh`: `Sh = [[1, tan skew], [0, 1]]` shears along x, `F = diag(+-1, +-1)` mirrors. A world gesture whose linear part
//! is `M` (read in the record's own space) makes the item's new linear map `N = R(rotation) * F * Sh * M`; [`recompose`] takes `N` back
//! to a stored `(rotation, flip, skew)` plus the two scale factors that go to the size (QR with a positive diagonal), so any invertible
//! gesture has a stored form. A mirror is stored as `flipX` or `flipY`, whichever leaves the smaller turn.
//!
//! Matrices here are `[a b c d]` with `x' = a x + c y`, `y' = b x + d y`.

/// The skew a stored record may carry stays strictly inside a quarter turn.
pub(crate) const MAX_SKEW_DEGREES: f64 = 89.9;
const EPS: f64 = 1e-9;
use crate::object_document::Presence;
use serde_json::Number;

pub(crate) type Lin = [f64; 4];

/// The nine preset pivots of a selection box (a stored `xformAnchorKey`).
pub(crate) const PIVOT_PRESETS: [&str; 9] = ["tl", "tc", "tr", "ml", "mc", "mr", "bl", "bc", "br"];

/// #2372: the optional stored Select pivot of an image or a text block (baseline `serP` `xformAnchorKey` / `xformAnchorCustom`): one of the
/// nine presets, or a custom world point `[x, y]` that wins over it. Absent reads as none, so an old document loads and saves byte-identical.
pub(crate) fn pivot_valid(key: &Presence<String>, custom: &Presence<Vec<Number>>) -> bool {
    key.value().is_none_or(|k| PIVOT_PRESETS.contains(&k.as_str()))
        && custom.value().is_none_or(|p| p.len() == 2 && p.iter().all(|n| n.as_f64().is_some_and(f64::is_finite)))
}

/// The stored preset, if any.
pub(crate) fn pivot_key(key: &Presence<String>) -> Option<String> {
    key.value().cloned()
}

/// The stored custom world point, if any.
pub(crate) fn pivot_custom(custom: &Presence<Vec<Number>>) -> Option<[f64; 2]> {
    match custom.value()?.as_slice() {
        [x, y] => Some([x.as_f64()?, y.as_f64()?]),
        _ => None,
    }
}

/// The mirror flags and the x-shear of one item (identity when all are off).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(crate) struct LocalForm {
    pub(crate) flip_x: bool,
    pub(crate) flip_y: bool,
    /// Degrees of shear along x (`x' = x + tan(skew) * y`).
    pub(crate) skew: f64,
}

/// `p` after `q` (`q` applied first).
pub(crate) fn mul(p: Lin, q: Lin) -> Lin {
    [
        p[0] * q[0] + p[2] * q[1],
        p[1] * q[0] + p[3] * q[1],
        p[0] * q[2] + p[2] * q[3],
        p[1] * q[2] + p[3] * q[3],
    ]
}

/// The turn by `degrees`.
pub(crate) fn turn(degrees: f64) -> Lin {
    let (s, c) = degrees.to_radians().sin_cos();
    [c, s, -s, c]
}

impl LocalForm {
    pub(crate) fn is_identity(&self) -> bool {
        !self.flip_x && !self.flip_y && self.skew == 0.0
    }

    /// `F * Sh`.
    pub(crate) fn linear(&self) -> Lin {
        let k = self.skew.to_radians().tan();
        let (fx, fy) = (if self.flip_x { -1.0 } else { 1.0 }, if self.flip_y { -1.0 } else { 1.0 });
        [fx, 0.0, fx * k, fy]
    }

    /// The affine this form adds to the picture of an item whose turn is `rotation`, about `centre` (in the item's final frame, after the
    /// parent chain): `T(c) R(rho) L R(-rho) T(-c)`, as `[a b c d e f]`. `None` when the form is the identity.
    pub(crate) fn world(&self, centre: [f64; 2], rho: f64) -> Option<[f64; 6]> {
        if self.is_identity() {
            return None;
        }
        let lin = mul(turn(rho), mul(self.linear(), turn(-rho)));
        Some([
            lin[0],
            lin[1],
            lin[2],
            lin[3],
            centre[0] - (lin[0] * centre[0] + lin[2] * centre[1]),
            centre[1] - (lin[1] * centre[0] + lin[3] * centre[1]),
        ])
    }

    /// The text block's glyph stages for this form, in application order, as `(rotation, sx, sy)` of a chain stage about the anchor: the
    /// shear (a turn, a scale along the turned axes and a turn back, its singular value decomposition) then the mirror.
    pub(crate) fn stages(&self) -> Vec<(f64, f64, f64)> {
        let mut out = Vec::new();
        if self.skew != 0.0 {
            let k = self.skew.to_radians().tan();
            // Sh = R(alpha) diag(s1, s2) R(beta).
            let (e, f, g, h): (f64, f64, f64, f64) = (1.0, 0.0, k / 2.0, -k / 2.0);
            let (q, r) = (e.hypot(h), f.hypot(g));
            let (a1, a2) = (g.atan2(f), h.atan2(e));
            out.push(((a2 - a1).to_degrees() / 2.0, 1.0, 1.0));
            out.push(((a2 + a1).to_degrees() / 2.0, q + r, q - r));
        }
        if self.flip_x || self.flip_y {
            out.push((0.0, if self.flip_x { -1.0 } else { 1.0 }, if self.flip_y { -1.0 } else { 1.0 }));
        }
        out
    }
}

/// What a gesture leaves stored.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Recomposed {
    /// Degrees within (-180, 180]; 0 when within a nanodegree of it.
    pub(crate) rotation: f64,
    pub(crate) form: LocalForm,
    /// The factors along the item's own x and y axes that go to the size.
    pub(crate) scale: [f64; 2],
}

fn snap(v: f64) -> f64 {
    if v.abs() < EPS {
        0.0
    } else {
        v
    }
}

fn wrap(mut degrees: f64) -> f64 {
    degrees %= 360.0;
    if degrees > 180.0 {
        degrees -= 360.0;
    } else if degrees <= -180.0 {
        degrees += 360.0;
    }
    degrees
}

/// The stored form of the item whose map was `R(rotation) * F * Sh` after the gesture whose linear part is `m` (read in the record's own
/// space). `None` when the result is singular or not finite.
pub(crate) fn recompose(rotation: f64, form: &LocalForm, m: Lin) -> Option<Recomposed> {
    let n = mul(turn(rotation), mul(form.linear(), m));
    let det = n[0] * n[3] - n[1] * n[2];
    let a = n[0].hypot(n[1]);
    if !n.iter().all(|v| v.is_finite()) || a < 1e-12 || det.abs() < 1e-12 {
        return None;
    }
    let q1 = [n[0] / a, n[1] / a];
    let s = q1[0] * n[2] + q1[1] * n[3];
    let u2 = [n[2] - s * q1[0], n[3] - s * q1[1]];
    let d = u2[0].hypot(u2[1]);
    if d < 1e-12 {
        return None;
    }
    let skew = snap((s / d).atan().to_degrees());
    let (phi, flip_x, flip_y) = if det > 0.0 {
        (wrap(q1[1].atan2(q1[0]).to_degrees()), false, false)
    } else {
        let (as_y, as_x) = (wrap(q1[1].atan2(q1[0]).to_degrees()), wrap((-q1[1]).atan2(-q1[0]).to_degrees()));
        if as_x.abs() <= as_y.abs() {
            (as_x, true, false)
        } else {
            (as_y, false, true)
        }
    };
    // A nanodegree grid keeps composed turns from reading -1e-14.
    let rotation = snap((phi * 1e9).round() / 1e9);
    Some(Recomposed { rotation, form: LocalForm { flip_x, flip_y, skew }, scale: [a, d] })
}
