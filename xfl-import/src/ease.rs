//! Ease curves of a classic tween: the simple quadratic ease and the custom
//! ease, a piecewise cubic Bézier in (time, progress) space.
//!
//! A `<CustomEase>` lists `Point`s as anchor, out-control, in-control,
//! anchor, … (3n + 1 points). `x` is normalised time, `y` is progress and may
//! leave 0..1 (overshoot). Evaluating at time `t` means solving `x(u) = t`
//! on the segment that spans `t`, then returning `y(u)`.

use serde::Serialize;

use crate::model::{Point, Tween, TweenEase};

/// Flash's simple ease: `acceleration` in -100 (ease in) ..= 100 (ease out).
/// At ±100 it is the quadratic `t²` / `t(2 - t)`; in between, a blend with
/// linear, i.e. `t + e·t·(1 - t)` with `e = acceleration / 100`.
pub fn simple_ease(t: f64, acceleration: f64) -> f64 {
    let e = (acceleration / 100.0).clamp(-1.0, 1.0);
    t + e * t * (1.0 - t)
}

/// Why a list of custom ease points cannot be evaluated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CurveError {
    /// Not 3n + 1 points with n ≥ 1.
    PointCount,
    NotFinite,
    /// Anchor times go backwards, or the curve spans no time at all.
    AnchorsNotIncreasing,
}

/// Checks custom ease points. `Ok(true)` when time is monotone along every
/// segment (both controls within the time range of their anchors, which is
/// sufficient), `Ok(false)` when a segment may fold back in time: it is still
/// evaluated, at one of the solutions of `x(u) = t`.
pub fn check_custom_ease(points: &[Point]) -> Result<bool, CurveError> {
    if points.len() < 4 || !(points.len() - 1).is_multiple_of(3) {
        return Err(CurveError::PointCount);
    }
    if points.iter().any(|p| !p.x.is_finite() || !p.y.is_finite()) {
        return Err(CurveError::NotFinite);
    }
    let mut monotone = true;
    for s in points.windows(4).step_by(3) {
        if s[3].x < s[0].x {
            return Err(CurveError::AnchorsNotIncreasing);
        }
        let inside = |x: f64| x >= s[0].x && x <= s[3].x;
        monotone &= inside(s[1].x) && inside(s[2].x);
    }
    if points[points.len() - 1].x <= points[0].x {
        return Err(CurveError::AnchorsNotIncreasing);
    }
    Ok(monotone)
}

fn bezier(p: [f64; 4], u: f64) -> f64 {
    let v = 1.0 - u;
    v * v * v * p[0] + 3.0 * v * v * u * p[1] + 3.0 * v * u * u * p[2] + u * u * u * p[3]
}

fn bezier_derivative(p: [f64; 4], u: f64) -> f64 {
    let v = 1.0 - u;
    3.0 * (v * v * (p[1] - p[0]) + 2.0 * v * u * (p[2] - p[1]) + u * u * (p[3] - p[2]))
}

/// Solves `x(u) = t` on one segment whose anchors bracket `t`. Newton steps,
/// replaced by a bisection step whenever Newton leaves the bracket or the
/// derivative vanishes, so it always converges.
fn solve_parameter(x: [f64; 4], t: f64) -> f64 {
    let (mut lo, mut hi) = (0.0f64, 1.0f64);
    let mut u = ((t - x[0]) / (x[3] - x[0])).clamp(0.0, 1.0);
    for _ in 0..64 {
        let err = bezier(x, u) - t;
        if err.abs() < 1e-14 {
            break;
        }
        if err > 0.0 {
            hi = u;
        } else {
            lo = u;
        }
        if hi - lo < 1e-15 {
            break;
        }
        let slope = bezier_derivative(x, u);
        let newton = u - err / slope;
        u = if slope.abs() > 1e-12 && newton > lo && newton < hi {
            newton
        } else {
            0.5 * (lo + hi)
        };
    }
    u
}

/// Progress at time `t` of a custom ease that passed [`check_custom_ease`].
/// Before the first anchor and after the last one, the anchor's progress.
pub fn custom_ease(points: &[Point], t: f64) -> f64 {
    let (Some(first), Some(last)) = (points.first(), points.last()) else {
        return t;
    };
    if t <= first.x {
        return first.y;
    }
    if t >= last.x {
        return last.y;
    }
    for s in points.windows(4).step_by(3) {
        // A zero-width segment is a vertical step: nothing to solve on it.
        if t > s[3].x || s[3].x <= s[0].x {
            continue;
        }
        let u = solve_parameter([s[0].x, s[1].x, s[2].x, s[3].x], t);
        return bezier([s[0].y, s[1].y, s[2].y, s[3].y], u);
    }
    last.y
}

/// The ease applied to one property.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Ease {
    Simple { acceleration: f64 },
    Custom { points: Vec<Point> },
}

impl Ease {
    /// Progress at time `t` in 0..=1.
    pub fn eval(&self, t: f64) -> f64 {
        match self {
            Ease::Simple { acceleration } => simple_ease(t, *acceleration),
            Ease::Custom { points } => custom_ease(points, t),
        }
    }
}

/// The ease of each property group of a classic tween. Skews follow
/// `rotation` and alpha follows `color`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PropertyEases {
    pub position: Ease,
    pub rotation: Ease,
    pub scale: Ease,
    pub color: Ease,
}

fn note(notes: &mut Vec<String>, text: &str) {
    if !notes.iter().any(|n| n == text) {
        notes.push(text.to_string());
    }
}

/// The ease of one property: its own entry unless the frame uses a single
/// curve, then the `all` entry, then the frame's `acceleration`.
fn resolve_one(tween: &Tween, property: &str, notes: &mut Vec<String>) -> Ease {
    let fallback = Ease::Simple {
        acceleration: tween.acceleration,
    };
    let own = (tween.use_single_ease_curve != Some(true)).then_some(property);
    let pick = |custom: bool| -> Option<&TweenEase> {
        own.into_iter().chain(["all"]).find_map(|target| {
            tween
                .eases
                .iter()
                .find(|e| e.custom == custom && e.target == target)
        })
    };
    if tween.custom_ease {
        let Some(e) = pick(true) else {
            note(
                notes,
                "custom ease without curve points (sampled with the simple ease)",
            );
            return fallback;
        };
        return match check_custom_ease(&e.points) {
            Ok(monotone) => {
                if !monotone {
                    note(
                        notes,
                        "custom ease curve folding back in time (one solution kept)",
                    );
                }
                Ease::Custom {
                    points: e.points.clone(),
                }
            }
            Err(_) => {
                note(
                    notes,
                    "malformed custom ease curve (sampled with the simple ease)",
                );
                fallback
            }
        };
    }
    let Some(e) = pick(false) else {
        return fallback;
    };
    if let Some(method) = &e.method {
        note(
            notes,
            &format!("ease preset {method} (sampled with the simple ease)"),
        );
    }
    Ease::Simple {
        acceleration: e
            .intensity
            .map_or(tween.acceleration, |i| i.clamp(-100.0, 100.0)),
    }
}

/// Per-property eases of a tween, and what could not be reproduced.
pub fn resolve_eases(tween: &Tween) -> (PropertyEases, Vec<String>) {
    let mut notes = Vec::new();
    let eases = PropertyEases {
        position: resolve_one(tween, "position", &mut notes),
        rotation: resolve_one(tween, "rotation", &mut notes),
        scale: resolve_one(tween, "scale", &mut notes),
        color: resolve_one(tween, "color", &mut notes),
    };
    (eases, notes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pts(v: &[(f64, f64)]) -> Vec<Point> {
        v.iter().map(|&(x, y)| Point { x, y }).collect()
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-12
    }

    #[test]
    fn simple_ease_endpoints_and_direction() {
        for acc in [-100.0, -40.0, 0.0, 40.0, 100.0] {
            assert!(close(simple_ease(0.0, acc), 0.0) && close(simple_ease(1.0, acc), 1.0));
        }
        assert!(close(simple_ease(0.5, -100.0), 0.25)); // ease in: slow start
        assert!(close(simple_ease(0.5, 100.0), 0.75)); // ease out: fast start
        assert!(close(simple_ease(0.5, 0.0), 0.5));
    }

    #[test]
    fn linear_custom_ease_is_identity() {
        let third = 1.0 / 3.0;
        let on_thirds = pts(&[
            (0.0, 0.0),
            (third, third),
            (2.0 * third, 2.0 * third),
            (1.0, 1.0),
        ]);
        // Handles lying on the anchors are still the straight line.
        let on_anchors = pts(&[(0.0, 0.0), (0.0, 0.0), (1.0, 1.0), (1.0, 1.0)]);
        for p in [on_thirds, on_anchors] {
            assert_eq!(check_custom_ease(&p), Ok(true));
            for i in 0..=20 {
                let t = i as f64 / 20.0;
                assert!(close(custom_ease(&p, t), t), "t = {t}");
            }
        }
    }

    #[test]
    fn ease_in_matches_hand_computed_values() {
        // x controls at 1/3, 2/3 make x(u) = u; y controls at 0 make y = u³.
        let p = pts(&[(0.0, 0.0), (1.0 / 3.0, 0.0), (2.0 / 3.0, 0.0), (1.0, 1.0)]);
        assert!(close(custom_ease(&p, 0.5), 0.125));
        assert!(close(custom_ease(&p, 0.2), 0.008));
        // Controls (0.5, 0) and (1, 1): at u = 0.5,
        // x = 3·¼·½·0.5 + 3·½·¼·1 + ⅛ = 0.6875 and y = 3·½·¼·1 + ⅛ = 0.5.
        let p = pts(&[(0.0, 0.0), (0.5, 0.0), (1.0, 1.0), (1.0, 1.0)]);
        assert!(close(custom_ease(&p, 0.6875), 0.5));
        assert!(custom_ease(&p, 0.25) < 0.25); // slow start
    }

    #[test]
    fn several_segments_overshoot_and_clamping() {
        // Two segments joined at (0.5, 1.2): the curve overshoots, then settles.
        let p = pts(&[
            (0.0, 0.0),
            (0.1, 0.4),
            (0.4, 1.2),
            (0.5, 1.2),
            (0.6, 1.2),
            (0.9, 1.0),
            (1.0, 1.0),
        ]);
        assert_eq!(check_custom_ease(&p), Ok(true));
        assert!(close(custom_ease(&p, 0.0), 0.0) && close(custom_ease(&p, 1.0), 1.0));
        assert!(close(custom_ease(&p, 0.5), 1.2));
        assert!(custom_ease(&p, 0.75) > 1.0);
        assert!(close(custom_ease(&p, -1.0), 0.0) && close(custom_ease(&p, 2.0), 1.0));
        // The solver's answer satisfies x(u) = t: check against a dense scan.
        let t = 0.3;
        let x = [0.0, 0.1, 0.4, 0.5];
        let best = (0..=200_000)
            .map(|i| i as f64 / 200_000.0)
            .min_by(|a, b| {
                (bezier(x, *a) - t)
                    .abs()
                    .total_cmp(&(bezier(x, *b) - t).abs())
            })
            .unwrap();
        assert!((custom_ease(&p, t) - bezier([0.0, 0.4, 1.2, 1.2], best)).abs() < 1e-4);
    }

    #[test]
    fn malformed_points_are_rejected_and_folds_flagged() {
        assert_eq!(check_custom_ease(&[]), Err(CurveError::PointCount));
        let p = pts(&[(0.0, 0.0), (0.3, 0.3), (0.6, 0.6), (1.0, 1.0), (1.0, 1.0)]);
        assert_eq!(check_custom_ease(&p), Err(CurveError::PointCount));
        let p = pts(&[(0.0, 0.0), (0.3, f64::NAN), (0.6, 0.6), (1.0, 1.0)]);
        assert_eq!(check_custom_ease(&p), Err(CurveError::NotFinite));
        let p = pts(&[(1.0, 0.0), (0.3, 0.3), (0.6, 0.6), (0.0, 1.0)]);
        assert_eq!(check_custom_ease(&p), Err(CurveError::AnchorsNotIncreasing));
        let p = pts(&[(0.0, 0.0), (0.0, 0.3), (0.0, 0.6), (0.0, 1.0)]);
        assert_eq!(check_custom_ease(&p), Err(CurveError::AnchorsNotIncreasing));
        // Controls outside the anchors: may fold back, still solvable.
        let p = pts(&[(0.0, 0.0), (1.5, 0.0), (-0.5, 1.0), (1.0, 1.0)]);
        assert_eq!(check_custom_ease(&p), Ok(false));
        let y = custom_ease(&p, 0.5);
        assert!(y.is_finite() && (0.0..=1.0).contains(&y));
    }
}
