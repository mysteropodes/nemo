//! Pure on-curve easing math shared by native evaluation consumers.
//!
//! This preserves the characterized `src/js/animation/curve.js` algorithm.
//! It borrows values only; document admission and persistence belong to callers.

/// One waypoint and its optional authored tangent vector.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CurveSample {
    pub x: f64,
    pub y: f64,
    /// `Some(0.0)` is an authored tangent, not an automatic one.
    pub tx: Option<f64>,
    /// Defaults to zero for an authored tangent; ignored when `tx` is absent.
    pub ty: Option<f64>,
}

/// Evaluate borrowed waypoints without mutation, sorting or normalization.
///
/// Callers own finite-value/import validation. Normal authored curves have
/// ordered x coordinates and endpoints 0/1; legacy degenerate spans are not
/// repaired here. Empty/single curves return raw input before clamping. Longer
/// curves clamp input only, retain unbounded output and use eight Newton steps
/// unless the derivative is too small. A solve that has not reached `x` is
/// finished by bisection on the monotone `x(t)`; a converged Newton result is
/// kept bit-for-bit. Mirrors `src/js/animation/curve.js`.
pub fn evaluate_curve(points: &[CurveSample], x: f64) -> f64 {
    if points.len() < 2 {
        return x;
    }
    let x = x.clamp(0.0, 1.0);
    let mut index = 0;
    while index < points.len() - 2 && points[index + 1].x < x {
        index += 1;
    }
    let p0 = points[index];
    let p3 = points[index + 1];
    let span = p3.x - p0.x;
    let first = handle(tangent(points, index), span);
    let second = handle(tangent(points, index + 1), span);
    let c1 = [p0.x + first[0], p0.y + first[1]];
    let c2 = [p3.x - second[0], p3.y - second[1]];
    let mut t = if span > 1e-6 { (x - p0.x) / span } else { 0.0 };
    for _ in 0..8 {
        let error = cubic(t, p0.x, c1[0], c2[0], p3.x) - x;
        let derivative = cubic_derivative(t, p0.x, c1[0], c2[0], p3.x);
        if derivative.abs() < 1e-6 {
            break;
        }
        t = (t - error / derivative).clamp(0.0, 1.0);
    }
    if span > 1e-6 && (cubic(t, p0.x, c1[0], c2[0], p3.x) - x).abs() > 1e-9 {
        let (mut low, mut high) = (0.0_f64, 1.0_f64);
        for _ in 0..64 {
            t = (low + high) / 2.0;
            if cubic(t, p0.x, c1[0], c2[0], p3.x) < x {
                low = t;
            } else {
                high = t;
            }
        }
        t = (low + high) / 2.0;
    }
    cubic(t, p0.y, c1[1], c2[1], p3.y)
}

/// One third of a tangent, shortened along its own direction when its x reach
/// leaves the segment. A longer handle folds `x(t)` back, so `y` stops being a
/// function of `x` and the solve jumps between roots. In-span handles and the
/// legacy degenerate spans are returned unchanged.
fn handle(tangent: [f64; 2], span: f64) -> [f64; 2] {
    let x = tangent[0] / 3.0;
    let y = tangent[1] / 3.0;
    if !(span > 1e-6) {
        return [x, y];
    }
    if x < 0.0 {
        return [0.0, y];
    }
    if x > span {
        return [span, y * (span / x)];
    }
    [x, y]
}

fn tangent(points: &[CurveSample], index: usize) -> [f64; 2] {
    let point = points[index];
    if let Some(tx) = point.tx {
        return [tx, point.ty.unwrap_or(0.0)];
    }
    let previous = points[index.saturating_sub(1)];
    let next = points[(index + 1).min(points.len() - 1)];
    let x = (next.x - previous.x) / 2.0;
    let dx0 = point.x - previous.x;
    let dx1 = next.x - point.x;
    let slope0 = if dx0 > 1e-9 {
        (point.y - previous.y) / dx0
    } else {
        0.0
    };
    let slope1 = if dx1 > 1e-9 {
        (next.y - point.y) / dx1
    } else {
        0.0
    };
    let slope = if index == 0 {
        slope1
    } else if index + 1 == points.len() {
        slope0
    } else if slope0 * slope1 <= 0.0 {
        0.0
    } else {
        let average = (slope0 + slope1) / 2.0;
        let limit = 3.0 * slope0.abs().min(slope1.abs());
        average.clamp(-limit, limit)
    };
    [x, slope * x]
}

fn cubic(t: f64, a: f64, b: f64, c: f64, d: f64) -> f64 {
    let inverse = 1.0 - t;
    inverse * inverse * inverse * a
        + 3.0 * inverse * inverse * t * b
        + 3.0 * inverse * t * t * c
        + t * t * t * d
}

fn cubic_derivative(t: f64, a: f64, b: f64, c: f64, d: f64) -> f64 {
    let inverse = 1.0 - t;
    3.0 * inverse * inverse * (b - a) + 6.0 * inverse * t * (c - b) + 3.0 * t * t * (d - c)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn auto(x: f64, y: f64) -> CurveSample {
        CurveSample {
            x,
            y,
            tx: None,
            ty: None,
        }
    }
    fn manual(x: f64, y: f64, tx: f64, ty: f64) -> CurveSample {
        CurveSample {
            x,
            y,
            tx: Some(tx),
            ty: Some(ty),
        }
    }
    // Same curves and samples as tests/animation/curve.test.cjs.
    fn folded() -> Vec<(&'static str, Vec<CurveSample>)> {
        vec![
            (
                "lateWaypoint",
                vec![auto(0., 0.), auto(0.9, 0.2), auto(1., 1.)],
            ),
            (
                "earlyWaypoint",
                vec![auto(0., 0.), auto(0.05, 0.6), auto(1., 1.)],
            ),
            (
                "crowdedEnds",
                vec![
                    auto(0., 0.),
                    auto(0.02, 0.3),
                    auto(0.5, 0.5),
                    auto(0.98, 0.7),
                    auto(1., 1.),
                ],
            ),
            (
                "longManual",
                vec![manual(0., 0., 4., 0.), manual(1., 1., 4., 0.)],
            ),
            (
                "negativeManual",
                vec![manual(0., 0., -1., 1.), manual(1., 1., 1., 0.)],
            ),
        ]
    }

    #[test]
    fn handles_leaving_their_span_no_longer_step_the_value() {
        for (name, points) in folded() {
            let mut previous = evaluate_curve(&points, 0.0);
            let mut worst = 0.0_f64;
            for i in 1..=20000 {
                let y = evaluate_curve(&points, f64::from(i) / 20000.0);
                worst = worst.max((y - previous).abs());
                previous = y;
            }
            assert!(worst < 0.05, "{name}: largest step {worst}");
            for point in &points {
                assert!((evaluate_curve(&points, point.x) - point.y).abs() < 1e-9);
            }
        }
    }

    #[test]
    fn limited_curves_match_the_javascript_kernel() {
        let curves = folded();
        let pick = |name: &str| &curves.iter().find(|(n, _)| *n == name).unwrap().1;
        for (name, x, y) in [
            ("lateWaypoint", 0.5, 0.08083535766687819),
            ("lateWaypoint", 0.95, 0.30785132460126435),
            ("lateWaypoint", 0.99, 0.7825917922680397),
            ("earlyWaypoint", 0.01, 0.3287537342245592),
            ("crowdedEnds", 0.99, 0.7404442467254743),
            ("longManual", 0.25, 0.029724605511925205),
            ("negativeManual", 0.1, 0.28148538221488),
        ] {
            let actual = evaluate_curve(pick(name), x);
            assert!((actual - y).abs() < 1e-12, "{name} x={x}: {actual}");
        }
    }

    #[test]
    fn in_span_handles_and_degenerate_spans_are_returned_unchanged() {
        assert_eq!(handle([1.26, 0.3], 1.0), [1.26 / 3.0, 0.3 / 3.0]);
        assert_eq!(handle([3.0, 6.0], 1.0), [1.0, 2.0]);
        assert_eq!(handle([6.0, 6.0], 1.0), [1.0, 1.0]);
        assert_eq!(handle([-1.0, 3.0], 1.0), [0.0, 1.0]);
        assert_eq!(handle([9.0, 3.0], 0.0), [3.0, 1.0]);
    }
}
