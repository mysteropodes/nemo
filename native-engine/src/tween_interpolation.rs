//! Prepared native baseline interpolation. Expensive pair probes run once, not per frame.
use crate::tween_kernels::*;
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
    original_counts: Option<(usize, usize)>,
    pivot: Option<[f64; 2]>,
    articulated: bool,
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
            original_counts,
            pivot: None,
            articulated: false,
        };
        pair.decide();
        Some(pair)
    }
    fn decide(&mut self) {
        self.decision = choose(self);
        self.reduction = self.original_counts.and_then(|(a, b)| {
            reduction_indices(
                &self.a,
                &self.b,
                (((a + b) as f64 / 2.).round() as usize).max(10),
                self.decision.folds.as_ref(),
            )
        });
    }
    /// Opt-in rigid swing: when one rotation and scale explains the pair, turn the
    /// stroke by that whole angle about the motion's own fixed point.
    ///
    /// The baseline turns each key about its own centroid while the centre runs
    /// along the chord, so a point near the real pivot (a shoulder) travels out
    /// and back; it answers by damping the rotation as the centroids separate,
    /// which leaves a swinging limb as a straight blend (measured: a 90 degree
    /// swing about its end is 72% of its length at the midpoint). Moving the
    /// centre on the orbit about the fixed point of `z -> cb + w (z - ca)` keeps
    /// that pivot still for whatever angle is applied, so the damping is not
    /// needed: length is kept and the stroke travels on its arc.
    ///
    /// The swing is admitted when the similarity leaves less than 20% of the
    /// shape's size unexplained; a redrawn, non-rigid pair keeps the baseline
    /// result exactly. Manual arc handles still take precedence.
    pub(crate) fn with_pivot_arc(mut self) -> Self {
        self.articulated = articulated(&self.a, &self.b);
        let Some((theta, scale, rigidity)) = similarity(&self.a, &self.b, self.ca, self.cb) else {
            self.decide();
            return self;
        };
        if rigidity < 1. / 3. {
            self.decide();
            return self;
        }
        let (re, im) = (scale * theta.cos(), scale * theta.sin());
        let den = (1. - re) * (1. - re) + im * im;
        if theta.abs() < 0.06 || den < 1e-9 {
            self.decide();
            return self;
        }
        // p = (cb - w ca) / (1 - w), complex arithmetic.
        let n = [
            self.cb[0] - (re * self.ca[0] - im * self.ca[1]),
            self.cb[1] - (im * self.ca[0] + re * self.ca[1]),
        ];
        let pivot = [
            (n[0] * (1. - re) - n[1] * im) / den,
            (n[1] * (1. - re) + n[0] * im) / den,
        ];
        if !pivot.iter().all(|v| v.is_finite()) {
            self.decide();
            return self;
        }
        self.theta = theta;
        self.scale = scale;
        self.pivot = Some(pivot);
        self.decide();
        self
    }
    /// How far the registered reconstruction replaces the blend at `t`: none at
    /// the keys (their own drawing is kept), all of it through the middle.
    fn swell(&self, t: f64) -> f64 {
        (4. * (std::f64::consts::PI * t.clamp(0., 1.)).sin()).min(1.)
    }
    /// Centre of the straight (handle-free) trajectory at `t`.
    fn center_at(&self, t: f64) -> [f64; 2] {
        match self.pivot {
            Some(pivot) => self.orbit(pivot, t),
            None => mix(self.ca, self.cb, t),
        }
    }
    fn orbit(&self, pivot: [f64; 2], t: f64) -> [f64; 2] {
        let r = rotate(sub(self.ca, pivot), self.theta * t, lerp(1., self.scale, t));
        [pivot[0] + r[0], pivot[1] + r[1]]
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
        let straight = |control: [f64; 2], at: f64| {
            let line = mix(self.ca, self.cb, at);
            norm(sub(control, line)) <= 1e-6 * (1. + norm(sub(self.cb, self.ca)))
        };
        let center = match self.pivot {
            Some(pivot) if straight(out, 1. / 3.) && straight(input, 2. / 3.) => {
                self.orbit(pivot, t)
            }
            _ => [
                cubic(self.ca[0], out[0], input[0], self.cb[0]),
                cubic(self.ca[1], out[1], input[1], self.cb[1]),
            ],
        };
        let mut s = if let Some(fits) = &self.decision.mls {
            local_segments(&self.a, &self.b, fits, t)
        } else {
            self.linear_at(t, center)
        };
        if self.decision.registered {
            let target = registered(&self.a, &self.b, t, self.closed_at(t));
            s = blend_segments(&s, &target, &vec![self.swell(t); s.len()]);
        }
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
