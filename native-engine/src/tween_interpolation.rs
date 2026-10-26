//! Prepared native baseline interpolation. Expensive pair probes run once, not per frame.
use super::*;
#[path = "tween_interpolation_math.rs"]
mod math;
use math::*;
#[path = "tween_interpolation_mls.rs"]
mod mls;
use mls::*;
#[path = "tween_interpolation_correction.rs"]
mod correction;
use correction::*;
#[path = "tween_interpolation_probe.rs"]
mod probe;
use probe::*;

pub(crate) struct PreparedInterpolation {
    a: Vec<SegOut>,
    b: Vec<SegOut>,
    ca: [f64; 2],
    cb: [f64; 2],
    theta: f64,
    scale: f64,
    closed_a: bool,
    closed_b: bool,
    decision: Decision,
    reduction: Option<Vec<usize>>,
}
impl PreparedInterpolation {
    pub(crate) fn new(
        a: Vec<SegOut>,
        b: Vec<SegOut>,
        closed_a: bool,
        closed_b: bool,
        original_counts: Option<(usize, usize)>,
    ) -> Option<Self> {
        if a.len() != b.len()
            || a.len() < 2
            || a.iter().chain(&b).any(|s| {
                s.point
                    .iter()
                    .chain(&s.handle_in)
                    .chain(&s.handle_out)
                    .any(|x| !x.is_finite())
            })
        {
            return None;
        }
        let ca = centroid(&a);
        let cb = centroid(&b);
        let (theta, scale) = rigid_fit(&a, &b, ca, cb);
        let mut pair = Self {
            a,
            b,
            ca,
            cb,
            theta,
            scale,
            closed_a,
            closed_b,
            decision: Decision::default(),
            reduction: None,
        };
        pair.decision = choose(&pair);
        pair.reduction = original_counts.and_then(|(a, b)| {
            reduction_indices(
                &pair.a,
                &pair.b,
                (((a + b) as f64 / 2.).round() as usize).max(10),
                pair.decision.folds.as_ref(),
            )
        });
        Some(pair)
    }
    fn linear_at(&self, t: f64, center: [f64; 2]) -> Vec<SegOut> {
        linear(
            &self.a, &self.b, self.ca, self.cb, self.theta, self.scale, t, center,
        )
    }
    fn closed_at(&self, t: f64) -> bool {
        if t < 0.5 {
            self.closed_a
        } else {
            self.closed_b
        }
    }
    /// Arc controls are absolute positions, as returned by the baseline getArcHandles.
    /// Missing controls are the exact straight-line cubic defaults.
    pub(crate) fn evaluate(
        &self,
        t: f64,
        controls: Option<([f64; 2], [f64; 2])>,
    ) -> Option<Vec<SegOut>> {
        if !t.is_finite() {
            return None;
        }
        let (out, input) = controls.unwrap_or((
            mix(self.ca, self.cb, 1. / 3.),
            mix(self.ca, self.cb, 2. / 3.),
        ));
        if out.iter().chain(&input).any(|v| !v.is_finite()) {
            return None;
        }
        let cubic = |a: f64, o: f64, i: f64, b: f64| {
            let u = 1. - t;
            u * u * u * a + 3. * u * u * t * o + 3. * u * t * t * i + t * t * t * b
        };
        let center = [
            cubic(self.ca[0], out[0], input[0], self.cb[0]),
            cubic(self.ca[1], out[1], input[1], self.cb[1]),
        ];
        let mut s = if let Some(fits) = &self.decision.mls {
            local_segments(&self.a, &self.b, fits, t)
        } else {
            self.linear_at(t, center)
        };
        let weight = self.decision.weight * (std::f64::consts::PI * t.clamp(0., 1.)).sin();
        if weight > 0.001 {
            let target = intrinsic(&self.a, &self.b, t, center, self.closed_at(t), Some(&s));
            let weights: Vec<_> = (0..s.len())
                .map(|i| {
                    let w = weight * self.decision.trust.as_ref().map_or(1., |v| v[i]);
                    if w <= 0.001 {
                        0.
                    } else {
                        w
                    }
                })
                .collect();
            s = blend_segments(&s, &target, &weights);
        }
        if let Some(folds) = &self.decision.folds {
            s = correct_folds(s, folds, t);
        }
        s = harmonise(s, &self.a, &self.b, t, self.closed_at(t));
        if let Some(indices) = &self.reduction {
            s = reduce(&s, indices);
        }
        Some(s)
    }
}
