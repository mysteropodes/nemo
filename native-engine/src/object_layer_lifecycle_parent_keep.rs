//! Parent A keeping the world pose under a rotated or scaled ancestor (#2372): the number-only solve.
//!
//! A layer's Motion is the affine `p -> pivot + R(rotation) * S(scale) * (p - pivot) + position`, `pivot` being its
//! evaluated pivot base plus its anchor (`transform_segment` of `motion_parent_chain.rs`). The world pose of a child
//! is the composition of its own affine with the affines of its ancestors, nearest first. Re-parenting keeps that
//! composition: `new_chain * own' == old_chain * own`, so the child's own affine becomes
//! `own' = new_chain^-1 * old_chain * own`. Its linear part must still be a rotation times a diagonal scale (the only
//! shape Rotation and Scale can store); a skew is a typed refusal, never an approximation. The pivot (base plus
//! anchor) is unchanged, so only Position, Rotation and Scale move. Baseline `setLayerParent` folds the pivot's
//! world displacement into Position alone (`src/js/motion.js` at 1065e9d8), which is exact for translation-only
//! ancestors; this solve is exact for every ancestor pose the stored values can express.
use std::f64::consts::PI;

/// One layer's stored Motion values and its evaluated pivot base.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Pose {
    pub(crate) position: [f64; 2],
    pub(crate) anchor: [f64; 2],
    pub(crate) rotation: f64,
    /// Percent, like the stored property.
    pub(crate) scale: [f64; 2],
    pub(crate) base: [f64; 2],
}

/// The child's new stored values.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Kept {
    pub(crate) position: [f64; 2],
    pub(crate) rotation: f64,
    pub(crate) scale: [f64; 2],
}

/// `x' = a x + c y + e`, `y' = b x + d y + f`.
#[derive(Debug, Clone, Copy)]
struct Affine([f64; 6]);

impl Affine {
    const IDENTITY: Self = Self([1., 0., 0., 1., 0., 0.]);
    fn of(pose: &Pose) -> Self {
        let (s, c) = (pose.rotation * PI / 180.).sin_cos();
        let (sx, sy) = (pose.scale[0] / 100., pose.scale[1] / 100.);
        let (a, b, cc, d) = (c * sx, s * sx, -s * sy, c * sy);
        let pivot = [pose.base[0] + pose.anchor[0], pose.base[1] + pose.anchor[1]];
        Self([
            a,
            b,
            cc,
            d,
            pivot[0] + pose.position[0] - (a * pivot[0] + cc * pivot[1]),
            pivot[1] + pose.position[1] - (b * pivot[0] + d * pivot[1]),
        ])
    }
    /// `self` applied after `inner`.
    fn after(self, inner: Self) -> Self {
        let (m, n) = (self.0, inner.0);
        Self([
            m[0] * n[0] + m[2] * n[1],
            m[1] * n[0] + m[3] * n[1],
            m[0] * n[2] + m[2] * n[3],
            m[1] * n[2] + m[3] * n[3],
            m[0] * n[4] + m[2] * n[5] + m[4],
            m[1] * n[4] + m[3] * n[5] + m[5],
        ])
    }
    fn apply(self, p: [f64; 2]) -> [f64; 2] {
        let m = self.0;
        [
            m[0] * p[0] + m[2] * p[1] + m[4],
            m[1] * p[0] + m[3] * p[1] + m[5],
        ]
    }
    fn invert(self) -> Option<Self> {
        let m = self.0;
        let det = m[0] * m[3] - m[1] * m[2];
        let scale = m[0].abs() + m[1].abs() + m[2].abs() + m[3].abs();
        if !det.is_finite() || scale == 0. || det.abs() < 1e-12 * scale * scale {
            return None;
        }
        let (a, b, c, d) = (m[3] / det, -m[1] / det, -m[2] / det, m[0] / det);
        Some(Self([
            a,
            b,
            c,
            d,
            -(a * m[4] + c * m[5]),
            -(b * m[4] + d * m[5]),
        ]))
    }
}

/// Ancestors nearest first, applied in order.
fn chain_affine(chain: &[Pose]) -> Affine {
    chain
        .iter()
        .fold(Affine::IDENTITY, |acc, pose| Affine::of(pose).after(acc))
}

/// Why the pose cannot be kept exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Refusal {
    Singular,
    Skew,
}

impl Refusal {
    pub(crate) fn reason(self) -> &'static str {
        match self {
            Self::Singular => "a collapsed (zero scale) parent cannot keep the child in place",
            Self::Skew => "the parents' rotation and non-uniform scale would skew the child",
        }
    }
}

/// Whole values and round-off noise below 1e-9 snap to the nearest 1e-9, like the values the editor writes.
fn snap(v: f64) -> f64 {
    let r = (v * 1e9).round() / 1e9;
    if r == 0. {
        0.
    } else {
        r
    }
}

/// The child's values under `new` that keep the world pose it has under `old`.
pub(crate) fn keep_world_pose(child: &Pose, old: &[Pose], new: &[Pose]) -> Result<Kept, Refusal> {
    let inverse = chain_affine(new).invert().ok_or(Refusal::Singular)?;
    let own = Affine::of(child);
    let next = inverse.after(chain_affine(old)).after(own);
    if !next.0.iter().all(|v| v.is_finite()) {
        return Err(Refusal::Singular);
    }
    let [a, b, c, d, _, _] = next.0;
    let (n1, n2) = (a.hypot(b), c.hypot(d));
    if n1 < 1e-12 || n2 < 1e-12 {
        return Err(Refusal::Singular);
    }
    if (a * c + b * d).abs() > 1e-9 * n1 * n2 {
        return Err(Refusal::Skew);
    }
    // A child stored with a negative X scale keeps that sign, so a mirrored layer is not rewritten as a half turn.
    let flip = if child.scale[0] < 0. { -1. } else { 1. };
    let theta = (flip * b).atan2(flip * a);
    let sy = -theta.sin() * c + theta.cos() * d;
    // The same rotation as a whole turn: the one nearest the stored value, so an unchanged pose reads unchanged.
    let degrees = theta * 180. / PI;
    let rotation = degrees + 360. * ((child.rotation - degrees) / 360.).round();
    let pivot = [
        child.base[0] + child.anchor[0],
        child.base[1] + child.anchor[1],
    ];
    let moved = next.apply(pivot);
    Ok(Kept {
        position: [
            snap(moved[0] - child.base[0] - child.anchor[0]),
            snap(moved[1] - child.base[1] - child.anchor[1]),
        ],
        rotation: snap(rotation),
        scale: [snap(flip * n1 * 100.), snap(sy * 100.)],
    })
}

#[cfg(test)]
#[path = "../tests/object_layer_parent_keep_math.rs"]
mod tests;
