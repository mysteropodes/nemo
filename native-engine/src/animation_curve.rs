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
/// curves clamp input only, retain unbounded output and use exactly eight
/// Newton steps unless the derivative is too small. There is no fallback solve.
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
    let first = tangent(points, index);
    let second = tangent(points, index + 1);
    let c1 = [p0.x + first[0] / 3.0, p0.y + first[1] / 3.0];
    let c2 = [p3.x - second[0] / 3.0, p3.y - second[1] / 3.0];
    let span = p3.x - p0.x;
    let mut t = if span > 1e-6 { (x - p0.x) / span } else { 0.0 };
    for _ in 0..8 {
        let error = cubic(t, p0.x, c1[0], c2[0], p3.x) - x;
        let derivative = cubic_derivative(t, p0.x, c1[0], c2[0], p3.x);
        if derivative.abs() < 1e-6 {
            break;
        }
        t = (t - error / derivative).clamp(0.0, 1.0);
    }
    cubic(t, p0.y, c1[1], c2[1], p3.y)
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
