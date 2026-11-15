//! Classic tweens as sampled transforms (slice 4 of #2020, model side).
//!
//! A classic tween runs from a keyframe holding one symbol instance to the
//! next keyframe of the same layer holding an instance of the same symbol.
//! Flash interpolates the decomposed transform, not the matrix: the
//! transformation point's position, the two scales and the two skews
//! (a rotation is equal skews), with a simple quadratic ease.
//!
//! This module only computes; mapping the samples onto Nemo Motion keys is
//! the mapping slice's job.

use std::f64::consts::PI;

use serde::Serialize;

use crate::model::*;

/// Flash's decomposed transform. Angles in radians.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Decomposed {
    /// Transformation point in parent space.
    pub x: f64,
    pub y: f64,
    pub scale_x: f64,
    pub scale_y: f64,
    pub skew_x: f64,
    pub skew_y: f64,
}

impl Decomposed {
    /// A mirrored matrix (negative determinant) decomposes as a negative
    /// `scale_x`, so that a flip tweens by squashing through zero rather
    /// than by spinning one skew by π.
    pub fn from_instance(m: &Matrix, pivot: Point) -> Decomposed {
        let p = m.apply(pivot);
        let flipped = m.a * m.d - m.b * m.c < 0.0;
        let sign = if flipped { -1.0 } else { 1.0 };
        Decomposed {
            x: p.x,
            y: p.y,
            scale_x: sign * (m.a * m.a + m.b * m.b).sqrt(),
            scale_y: (m.c * m.c + m.d * m.d).sqrt(),
            skew_x: (-m.c).atan2(m.d),
            skew_y: (sign * m.b).atan2(sign * m.a),
        }
    }

    /// The matrix that puts `pivot` (in symbol space) at (`x`, `y`).
    pub fn to_matrix(&self, pivot: Point) -> Matrix {
        let mut m = Matrix {
            a: self.scale_x * self.skew_y.cos(),
            b: self.scale_x * self.skew_y.sin(),
            c: -self.scale_y * self.skew_x.sin(),
            d: self.scale_y * self.skew_x.cos(),
            tx: 0.0,
            ty: 0.0,
        };
        let p = m.apply(pivot);
        m.tx = self.x - p.x;
        m.ty = self.y - p.y;
        m
    }

    /// Rotation when the transform has no shear (skew_x == skew_y).
    pub fn rotation(&self) -> Option<f64> {
        (wrap(self.skew_x - self.skew_y).abs() < 1e-9).then_some(self.skew_y)
    }
}

/// Below this, two angles are the same (radians).
const ANGLE_EPS: f64 = 1e-6;

/// Angle into (-π, π].
fn wrap(a: f64) -> f64 {
    let mut r = a % (2.0 * PI);
    if r <= -PI {
        r += 2.0 * PI;
    } else if r > PI {
        r -= 2.0 * PI;
    }
    r
}

/// Flash's simple ease: `acceleration` in -100 (ease in) ..= 100 (ease out).
/// At ±100 it is the quadratic `t²` / `t(2 - t)`; in between, a blend with
/// linear, i.e. `t + e·t·(1 - t)` with `e = acceleration / 100`.
pub fn simple_ease(t: f64, acceleration: f64) -> f64 {
    let e = (acceleration / 100.0).clamp(-1.0, 1.0);
    t + e * t * (1.0 - t)
}

/// Signed angular travel from `from` to `to` for a rotation option.
fn angle_delta(from: f64, to: f64, rotate: Option<&str>, times: u32) -> f64 {
    let mut short = wrap(to - from);
    // XFL matrices carry limited precision: nominally equal angles must not
    // add or lose a whole turn under cw/ccw.
    if short.abs() < ANGLE_EPS {
        short = 0.0;
    }
    let turns = 2.0 * PI * times as f64;
    match rotate {
        // Screen y points down, so clockwise is a positive angle.
        Some("cw") | Some("clockwise") => {
            let d = if short < 0.0 { short + 2.0 * PI } else { short };
            d + turns
        }
        Some("ccw") | Some("counter-clockwise") => {
            let d = if short > 0.0 { short - 2.0 * PI } else { short };
            d - turns
        }
        _ => short,
    }
}

/// One classic tween between two keyframes of a layer.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ClassicTween {
    pub library_item: String,
    pub start_frame: u32,
    /// First frame of the destination keyframe (the tween reaches `to` there).
    pub end_frame: u32,
    pub from: Decomposed,
    pub to: Decomposed,
    pub from_alpha: f64,
    pub to_alpha: f64,
    pub acceleration: f64,
    pub rotate: Option<String>,
    pub rotate_times: u32,
    /// `motionTweenScale="false"`: scale holds the start value over the span.
    pub tween_scale: bool,
    pub pivot: Point,
    /// What the samples do not reproduce (custom ease curve, orient to path,
    /// motion guide). Empty when the samples are exact.
    pub approximations: Vec<String>,
}

impl ClassicTween {
    /// Interpolated transform and alpha at `frame` (clamped to the span).
    pub fn sample(&self, frame: u32) -> (Decomposed, f64) {
        let span = self.end_frame.saturating_sub(self.start_frame).max(1) as f64;
        let t = ((frame.saturating_sub(self.start_frame)) as f64 / span).clamp(0.0, 1.0);
        let k = simple_ease(t, self.acceleration);
        let lerp = |a: f64, b: f64| a + (b - a) * k;
        let rot = self.rotate.as_deref();
        let d = Decomposed {
            x: lerp(self.from.x, self.to.x),
            y: lerp(self.from.y, self.to.y),
            scale_x: if self.tween_scale {
                lerp(self.from.scale_x, self.to.scale_x)
            } else {
                self.from.scale_x
            },
            scale_y: if self.tween_scale {
                lerp(self.from.scale_y, self.to.scale_y)
            } else {
                self.from.scale_y
            },
            skew_x: self.from.skew_x
                + angle_delta(self.from.skew_x, self.to.skew_x, rot, self.rotate_times) * k,
            skew_y: self.from.skew_y
                + angle_delta(self.from.skew_y, self.to.skew_y, rot, self.rotate_times) * k,
        };
        (d, lerp(self.from_alpha, self.to_alpha))
    }
}

/// Why a frame marked as a classic tween could not be turned into one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum TweenSkip {
    /// Not exactly one symbol instance on the start keyframe.
    NotOneInstance,
    /// No later keyframe to tween to.
    NoEndKeyframe,
    /// The end keyframe does not hold one instance of the same symbol.
    EndMismatch,
}

fn single_instance(frame: &Frame) -> Option<&SymbolInstance> {
    match frame.elements.as_slice() {
        [Element::SymbolInstance(i)] => Some(i),
        _ => None,
    }
}

/// Every classic tween of layer `index` of `timeline`, in frame order, plus
/// the frames that are marked as classic tweens but cannot be one (to report).
pub fn classic_tweens(
    timeline: &Timeline,
    index: usize,
) -> (Vec<ClassicTween>, Vec<(u32, TweenSkip)>) {
    let Some(layer) = timeline.layers.get(index) else {
        return (Vec::new(), Vec::new());
    };
    // A layer under a guide layer follows the guide's path (motion guide).
    let guided = layer
        .parent_layer_index
        .and_then(|p| timeline.layers.get(p))
        .is_some_and(|p| p.kind == LayerKind::Guide);
    let mut tweens = Vec::new();
    let mut skipped = Vec::new();
    for (i, f) in layer.frames.iter().enumerate() {
        let Some(tw) = f.tween.as_ref().filter(|t| t.kind == TweenKind::Motion) else {
            continue;
        };
        let Some(a) = single_instance(f) else {
            skipped.push((f.index, TweenSkip::NotOneInstance));
            continue;
        };
        let Some(next) = layer.frames.get(i + 1) else {
            skipped.push((f.index, TweenSkip::NoEndKeyframe));
            continue;
        };
        let Some(b) = single_instance(next).filter(|b| b.library_item == a.library_item) else {
            skipped.push((f.index, TweenSkip::EndMismatch));
            continue;
        };
        // Flash tweens around the start instance's transformation point.
        let pivot = a.transformation_point;
        let mut approximations = Vec::new();
        if tw.custom_ease {
            approximations.push("custom ease curve (sampled with the simple ease)".to_string());
        }
        if tw.orient_to_path {
            approximations.push("orient to path".to_string());
        }
        if guided {
            approximations.push("motion guide (sampled as a straight line)".to_string());
        }
        tweens.push(ClassicTween {
            library_item: a.library_item.clone(),
            start_frame: f.index,
            end_frame: next.index,
            from: Decomposed::from_instance(&a.matrix, pivot),
            to: Decomposed::from_instance(&b.matrix, pivot),
            from_alpha: a.alpha,
            to_alpha: b.alpha,
            acceleration: tw.acceleration,
            rotate: tw.rotate.clone(),
            rotate_times: tw.rotate_times,
            tween_scale: tw.tween_scale,
            pivot,
            approximations,
        });
    }
    (tweens, skipped)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn decompose_round_trips() {
        let pivot = Point { x: 10.0, y: -5.0 };
        let r = 0.7f64;
        let m = Matrix {
            a: 2.0 * r.cos(),
            b: 2.0 * r.sin(),
            c: -3.0 * r.sin(),
            d: 3.0 * r.cos(),
            tx: 40.0,
            ty: 60.0,
        };
        let d = Decomposed::from_instance(&m, pivot);
        assert!(close(d.scale_x, 2.0) && close(d.scale_y, 3.0));
        assert!(close(d.rotation().unwrap(), 0.7));
        let back = d.to_matrix(pivot);
        for (u, v) in [
            (m.a, back.a),
            (m.b, back.b),
            (m.c, back.c),
            (m.d, back.d),
            (m.tx, back.tx),
            (m.ty, back.ty),
        ] {
            assert!(close(u, v), "{m:?} vs {back:?}");
        }
        // A sheared matrix has no single rotation.
        let shear = Matrix {
            c: 0.5,
            ..Matrix::IDENTITY
        };
        assert!(Decomposed::from_instance(&shear, pivot)
            .rotation()
            .is_none());
    }

    #[test]
    fn ease_endpoints_and_direction() {
        for acc in [-100.0, -40.0, 0.0, 40.0, 100.0] {
            assert!(close(simple_ease(0.0, acc), 0.0) && close(simple_ease(1.0, acc), 1.0));
        }
        assert!(close(simple_ease(0.5, -100.0), 0.25)); // ease in: slow start
        assert!(close(simple_ease(0.5, 100.0), 0.75)); // ease out: fast start
        assert!(close(simple_ease(0.5, 0.0), 0.5));
    }

    #[test]
    fn rotation_options() {
        let q = PI / 2.0;
        assert!(close(angle_delta(0.0, q, None, 0), q));
        assert!(close(angle_delta(0.0, -q, None, 0), -q));
        assert!(close(angle_delta(0.0, -q, Some("cw"), 0), 3.0 * q));
        assert!(close(angle_delta(0.0, q, Some("ccw"), 0), -3.0 * q));
        assert!(close(angle_delta(0.0, 0.0, Some("clockwise"), 2), 4.0 * PI));
        // Precision noise around equal angles adds no extra turn.
        assert!(close(
            angle_delta(0.3, 0.3 - 1e-12, Some("clockwise"), 1),
            2.0 * PI
        ));
        assert!(close(
            angle_delta(0.3, 0.3 + 1e-12, Some("counter-clockwise"), 1),
            -2.0 * PI
        ));
    }

    #[test]
    fn flip_is_negative_scale() {
        let pivot = Point { x: 0.0, y: 0.0 };
        let flip = Matrix {
            a: -1.0,
            ..Matrix::IDENTITY
        };
        let d = Decomposed::from_instance(&flip, pivot);
        assert!(close(d.scale_x, -1.0) && close(d.scale_y, 1.0));
        assert!(close(d.rotation().unwrap(), 0.0));
        let back = d.to_matrix(pivot);
        assert!(
            close(back.a, -1.0) && close(back.b, 0.0) && close(back.c, 0.0) && close(back.d, 1.0)
        );
        // Flipped and rotated by 0.4 round-trips too.
        let r = 0.4f64;
        let m = Matrix {
            a: -r.cos(),
            b: -r.sin(),
            c: -r.sin(),
            d: r.cos(),
            ..Matrix::IDENTITY
        };
        let back = Decomposed::from_instance(&m, pivot).to_matrix(pivot);
        for (u, v) in [(m.a, back.a), (m.b, back.b), (m.c, back.c), (m.d, back.d)] {
            assert!(close(u, v), "{m:?} vs {back:?}");
        }
    }
}
