//! Closed-path curve refit (`Path#simplify(2.5)`), the last step of
//! `_eraseDegenerateSelfLoops` in `src/js/tools.js` at fbe4b8a3 (G19, L02-I).
//!
//! A line-by-line port of Paper.js 0.12.18's `PathFitter` (Schneider's curve
//! fit: chord-length parameterisation, least-squares Bezier, up to five Newton
//! re-parameterisations, split at the worst point) for the one case the
//! boolean pipeline needs: a closed polygon whose handles are all zero. Every
//! arithmetic expression keeps Paper's operand order so the result matches
//! the vendored library to rounding. Open paths are not needed and not
//! provided. The helper type below stands in for `paper.Point` only as far as
//! the fitter reads it.
use crate::object_document::object_group_kernels::object_boolean_ops::RingSeg;

/// Paper's `Numerical.EPSILON` and `Numerical.MACHINE_EPSILON`.
const EPSILON: f64 = 1e-12;
const MACHINE_EPSILON: f64 = 112e-18;

#[derive(Debug, Clone, Copy, PartialEq)]
struct V {
    x: f64,
    y: f64,
}

impl V {
    fn new(x: f64, y: f64) -> V {
        V { x, y }
    }
    fn sub(self, o: V) -> V {
        V::new(self.x - o.x, self.y - o.y)
    }
    fn add(self, o: V) -> V {
        V::new(self.x + o.x, self.y + o.y)
    }
    fn mul(self, k: f64) -> V {
        V::new(self.x * k, self.y * k)
    }
    fn neg(self) -> V {
        V::new(-self.x, -self.y)
    }
    fn dot(self, o: V) -> f64 {
        self.x * o.x + self.y * o.y
    }
    fn length(self) -> f64 {
        (self.x * self.x + self.y * self.y).sqrt()
    }
    /// `Point#normalize(length)`: a zero vector stays zero.
    fn normalize(self, length: f64) -> V {
        let current = self.length();
        let scale = if current != 0.0 {
            length / current
        } else {
            0.0
        };
        V::new(self.x * scale, self.y * scale)
    }
    fn distance(self, o: V) -> f64 {
        let dx = o.x - self.x;
        let dy = o.y - self.y;
        (dx * dx + dy * dy).sqrt()
    }
}

/// One fitted segment: anchor plus relative handles.
#[derive(Debug, Clone, Copy)]
struct Seg {
    point: V,
    handle_in: V,
    handle_out: V,
}

struct Fitter {
    points: Vec<V>,
    closed: bool,
}

/// `Path#simplify(tolerance)` of a closed polygon: `None` when the path has no
/// point at all (Paper returns `false` and leaves the path alone).
pub(crate) fn simplify_closed(points: &[[f64; 2]], tolerance: f64) -> Option<Vec<RingSeg>> {
    let mut distinct: Vec<V> = Vec::new();
    for point in points {
        let next = V::new(point[0], point[1]);
        if distinct.last().is_some_and(|last| *last == next) {
            continue;
        }
        distinct.push(next);
    }
    // `closed`: the last point leads, the first point trails.
    let last = *distinct.last()?;
    let second = distinct[0];
    distinct.insert(0, last);
    distinct.push(second);
    let fitter = Fitter {
        points: distinct,
        closed: true,
    };
    fitter.fit(tolerance).map(|segments| {
        segments
            .into_iter()
            .map(|segment| RingSeg {
                point: [segment.point.x, segment.point.y],
                handle_in: [segment.handle_in.x, segment.handle_in.y],
                handle_out: [segment.handle_out.x, segment.handle_out.y],
            })
            .collect()
    })
}

fn evaluate(degree: usize, controls: &[V], t: f64) -> V {
    let mut work: Vec<V> = controls.to_vec();
    for r in 1..=degree {
        for s in 0..=(degree - r) {
            work[s] = work[s].mul(1.0 - t).add(work[s + 1].mul(t));
        }
    }
    work[0]
}

impl Fitter {
    fn fit(&self, error: f64) -> Option<Vec<Seg>> {
        let n = self.points.len();
        if n == 0 {
            return None;
        }
        let zero = V::new(0.0, 0.0);
        let mut segments = vec![Seg {
            point: self.points[0],
            handle_in: zero,
            handle_out: zero,
        }];
        if n > 1 {
            self.fit_cubic(
                &mut segments,
                error,
                0,
                n - 1,
                self.points[1].sub(self.points[0]),
                self.points[n - 2].sub(self.points[n - 1]),
            );
            if self.closed {
                segments.remove(0);
                segments.pop();
            }
        }
        Some(segments)
    }

    fn fit_cubic(
        &self,
        segments: &mut Vec<Seg>,
        error: f64,
        first: usize,
        last: usize,
        tan1: V,
        tan2: V,
    ) {
        let pts = &self.points;
        if last - first == 1 {
            let p = pts[first];
            let a = pts[last];
            let o = p.distance(a) / 3.0;
            add_curve(
                segments,
                [p, p.add(tan1.normalize(o)), a.add(tan2.normalize(o)), a],
            );
            return;
        }
        let mut u = self.chord_length_parameterize(first, last);
        let mut max_error = error.max(error * error);
        let mut parametrized = true;
        let mut split = 0usize;
        for _ in 0..=4 {
            let bezier = self.generate_bezier(first, last, &u, tan1, tan2);
            let (worst, index) = self.find_max_error(first, last, &bezier, &u);
            if worst < error && parametrized {
                add_curve(segments, bezier);
                return;
            }
            split = index;
            if worst >= max_error {
                break;
            }
            parametrized = self.reparameterize(first, last, &mut u, &bezier);
            max_error = worst;
        }
        let tan_center = pts[split - 1].sub(pts[split + 1]);
        self.fit_cubic(segments, error, first, split, tan1, tan_center);
        self.fit_cubic(segments, error, split, last, tan_center.neg(), tan2);
    }

    fn generate_bezier(&self, first: usize, last: usize, u: &[f64], tan1: V, tan2: V) -> [V; 4] {
        let pts = &self.points;
        let p0 = pts[first];
        let p3 = pts[last];
        let mut a = [[0.0f64; 2]; 2];
        let mut c = [0.0f64; 2];
        for (k, &t) in u.iter().enumerate().take(last - first + 1) {
            let omt = 1.0 - t;
            let b1_base = 3.0 * t * omt;
            let b0 = omt * omt * omt;
            let b1 = b1_base * omt;
            let b2 = b1_base * t;
            let b3 = t * t * t;
            let a1 = tan1.normalize(b1);
            let a2 = tan2.normalize(b2);
            let tmp = pts[first + k].sub(p0.mul(b0 + b1)).sub(p3.mul(b2 + b3));
            a[0][0] += a1.dot(a1);
            a[0][1] += a1.dot(a2);
            a[1][0] = a[0][1];
            a[1][1] += a2.dot(a2);
            c[0] += a1.dot(tmp);
            c[1] += a2.dot(tmp);
        }
        let det = a[0][0] * a[1][1] - a[1][0] * a[0][1];
        let mut alpha1;
        let mut alpha2;
        if det.abs() > EPSILON {
            let det_x = a[0][0] * c[1] - a[1][0] * c[0];
            alpha1 = (c[0] * a[1][1] - c[1] * a[0][1]) / det;
            alpha2 = det_x / det;
        } else {
            let c0 = a[0][0] + a[0][1];
            let c1 = a[1][0] + a[1][1];
            alpha1 = if c0.abs() > EPSILON {
                c[0] / c0
            } else if c1.abs() > EPSILON {
                c[1] / c1
            } else {
                0.0
            };
            alpha2 = alpha1;
        }
        let seg_length = p3.distance(p0);
        let epsilon = EPSILON * seg_length;
        let mut handle1: Option<V> = None;
        let mut handle2: Option<V> = None;
        if alpha1 < epsilon || alpha2 < epsilon {
            alpha1 = seg_length / 3.0;
            alpha2 = alpha1;
        } else {
            let line = p3.sub(p0);
            let h1 = tan1.normalize(alpha1);
            let h2 = tan2.normalize(alpha2);
            if h1.dot(line) - h2.dot(line) > seg_length * seg_length {
                alpha1 = seg_length / 3.0;
                alpha2 = alpha1;
            } else {
                handle1 = Some(h1);
                handle2 = Some(h2);
            }
        }
        [
            p0,
            p0.add(handle1.unwrap_or_else(|| tan1.normalize(alpha1))),
            p3.add(handle2.unwrap_or_else(|| tan2.normalize(alpha2))),
            p3,
        ]
    }

    fn reparameterize(&self, first: usize, last: usize, u: &mut [f64], bezier: &[V; 4]) -> bool {
        for r in first..=last {
            u[r - first] = self.find_root(bezier, self.points[r], u[r - first]);
        }
        for r in 1..u.len() {
            if u[r] <= u[r - 1] {
                return false;
            }
        }
        true
    }

    fn find_root(&self, curve: &[V; 4], point: V, u: f64) -> f64 {
        let mut d1 = [V::new(0.0, 0.0); 3];
        let mut d2 = [V::new(0.0, 0.0); 2];
        for s in 0..=2 {
            d1[s] = curve[s + 1].sub(curve[s]).mul(3.0);
        }
        for s in 0..=1 {
            d2[s] = d1[s + 1].sub(d1[s]).mul(2.0);
        }
        let a = evaluate(3, curve, u);
        let o = evaluate(2, &d1, u);
        let h = evaluate(1, &d2, u);
        let a = a.sub(point);
        let denominator = o.dot(o) + a.dot(h);
        if (-MACHINE_EPSILON..=MACHINE_EPSILON).contains(&denominator) {
            u
        } else {
            u - a.dot(o) / denominator
        }
    }

    fn chord_length_parameterize(&self, first: usize, last: usize) -> Vec<f64> {
        let mut u = vec![0.0f64];
        for i in (first + 1)..=last {
            let previous = u[i - first - 1];
            u.push(previous + self.points[i].distance(self.points[i - 1]));
        }
        let total = u[last - first];
        for value in u.iter_mut().skip(1) {
            *value /= total;
        }
        u
    }

    /// Worst squared distance and its point index (`findMaxError`).
    fn find_max_error(
        &self,
        first: usize,
        last: usize,
        bezier: &[V; 4],
        u: &[f64],
    ) -> (f64, usize) {
        // Paper starts the index at half the span WITHOUT adding `first`; every call
        // with more than one interior point overwrites it, so only the shape matters.
        let mut index = (last - first + 1) / 2;
        let mut worst = 0.0f64;
        for i in (first + 1)..last {
            let d = evaluate(3, bezier, u[i - first]).sub(self.points[i]);
            let dist = d.x * d.x + d.y * d.y;
            if worst <= dist {
                worst = dist;
                index = i;
            }
        }
        (worst, index)
    }
}

/// `addCurve`: the previous anchor learns its outgoing handle, a new anchor
/// carries the incoming one.
fn add_curve(segments: &mut Vec<Seg>, curve: [V; 4]) {
    if let Some(previous) = segments.last_mut() {
        previous.handle_out = curve[1].sub(curve[0]);
    }
    segments.push(Seg {
        point: curve[3],
        handle_in: curve[2].sub(curve[3]),
        handle_out: V::new(0.0, 0.0),
    });
}

#[cfg(test)]
#[path = "../tests/object_boolean_ops_fit.rs"]
mod tests;
