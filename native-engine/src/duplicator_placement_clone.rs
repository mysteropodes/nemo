//! Per-clone placement of a duplicator (L04-C, I07/I08).
//!
//! Mirrors `_duplicatorClonePlacement` (`src/js/app.js`) at baseline
//! `fbe4b8a3`: the random-or-linear stagger of clone `k` on position,
//! rotation, scale, opacity and hue (plus the 3D deltas), the effector sums,
//! and the final descriptor (`dx`, `dy`, `rot`, `sx`, `sy`, opacity factor,
//! hue, `dup3D`) that the stamping loop turns into a matrix.
//!
//! The ten random draws of a clone come from the sibling stream kernel
//! (`duplicator_placement_rng`) and are ALWAYS taken, whichever channels are
//! random, so every duplicator keeps its pattern. A random channel uses
//! `(2 * r - 1) * delta`, a linear one `k * delta`. Effectors (inline and
//! resolved effector layers, concatenated by the caller through
//! `object_effector_layer::effective_effectors`) are summed in list order by
//! `object_effector_falloff::apply_effector`.
//!
//! The kernel owns no Motion sampling: the `dupOffset*` deltas, the base
//! offset of the mode (`duplicator_placement_modes::mode_offset`), the pivot
//! and the 3D flag are inputs. Everything is plain `f64` arithmetic in the
//! operand order of the baseline, so the result is bit-identical to it when
//! no effector uses the linear (cos/sin) falloff.

use crate::evaluation::object_component_kernels::duplicator_placement_modes::JsNumber;
use crate::evaluation::object_component_kernels::duplicator_placement_rng::{
    clone_draws, CloneDraws,
};
use crate::evaluation::object_component_kernels::object_effector_falloff::{
    apply_effector, clamp01, ChannelSums, Effector,
};

/// `dup.staggerRandom`: which channels use the random stagger.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct RandMode {
    pub position: bool,
    pub rotation: bool,
    pub scale: bool,
    pub opacity: bool,
    pub hue: bool,
}

/// The `dupOffset*` deltas sampled at the frame. `d_hue` keeps `dHue || 0`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct CloneDeltas {
    pub pos: [f64; 2],
    pub rot: f64,
    pub scale: [f64; 2],
    pub opacity: f64,
    pub hue: JsNumber,
    pub pos_z: f64,
    pub rot_x: f64,
    pub rot_y: f64,
}

/// Inputs of one clone.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ClonePlacementInput<'a> {
    /// `dup.seed`.
    pub seed: JsNumber,
    pub k: u32,
    /// The clone count as the loop bound (may be fractional).
    pub count: f64,
    /// `pivotK`.
    pub pivot: [f64; 2],
    /// The mode offset of the clone: `baseDx`, `baseDy`, `baseRot`.
    pub base: [f64; 3],
    pub deltas: CloneDeltas,
    pub rand: RandMode,
    /// `!!ld.threeD`.
    pub is_3d_layer: bool,
    pub effectors: &'a [Effector],
}

/// `place.dup3D`: only present on a 3D layer when an axis actually moved.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Dup3D {
    pub dz: f64,
    pub drx: f64,
    pub dry: f64,
}

/// The placement descriptor the baseline returns.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ClonePlacement {
    pub dx: f64,
    pub dy: f64,
    pub rot: f64,
    pub sx: f64,
    pub sy: f64,
    pub opacity_factor: f64,
    pub hue_deg: f64,
    pub dup_3d: Option<Dup3D>,
}

/// `random ? (2 * r - 1) * delta : k * delta`.
fn stagger(random: bool, r: f64, k: f64, delta: f64) -> f64 {
    if random {
        (2.0 * r - 1.0) * delta
    } else {
        k * delta
    }
}

/// JavaScript truthiness of a number.
fn nonzero(x: f64) -> bool {
    !(x == 0.0 || x.is_nan())
}

/// `_duplicatorClonePlacement(...)`: the ten draws are those of the clone's
/// own stream, `((dup.seed || 0) + k * 7919) >>> 0`.
pub(crate) fn clone_placement(input: &ClonePlacementInput<'_>) -> ClonePlacement {
    clone_placement_with_draws(input, &clone_draws(input.seed, input.k))
}

/// The placement over given draws. Production passes the clone's own stream
/// ([`clone_placement`]); the controls feed deliberately wrong draws through
/// this seam to prove the oracle notices a skipped, reordered or foreign draw.
pub(crate) fn clone_placement_with_draws(
    input: &ClonePlacementInput<'_>,
    d: &CloneDraws,
) -> ClonePlacement {
    let k = input.k as f64;
    let dl = &input.deltas;
    let rand = &input.rand;
    let [base_dx, base_dy, base_rot] = input.base;
    let hue_delta = dl.hue.or_default(0.0);

    let mut sums = ChannelSums::zero();
    sums.pos = [
        stagger(rand.position, d.rx, k, dl.pos[0]),
        stagger(rand.position, d.ry, k, dl.pos[1]),
    ];
    sums.rot = stagger(rand.rotation, d.rrr, k, dl.rot);
    sums.scale = [
        stagger(rand.scale, d.rsx, k, dl.scale[0]),
        stagger(rand.scale, d.rsy, k, dl.scale[1]),
    ];
    sums.opacity = stagger(rand.opacity, d.rop, k, dl.opacity);
    sums.pos_z = stagger(rand.position, d.rz, k, dl.pos_z);
    sums.rot_x = stagger(rand.rotation, d.rrx, k, dl.rot_x);
    sums.rot_y = stagger(rand.rotation, d.rry, k, dl.rot_y);
    sums.hue = stagger(rand.hue, d.rhue, k, hue_delta);

    let inst_x = input.pivot[0] + base_dx;
    let inst_y = input.pivot[1] + base_dy;
    for eff in input.effectors {
        apply_effector(&mut sums, eff, inst_x, inst_y, input.k, input.count);
    }

    let dup_3d = if input.is_3d_layer
        && (nonzero(sums.pos_z) || nonzero(sums.rot_x) || nonzero(sums.rot_y))
    {
        Some(Dup3D {
            dz: sums.pos_z,
            drx: sums.rot_x,
            dry: sums.rot_y,
        })
    } else {
        None
    };
    ClonePlacement {
        dx: base_dx + sums.pos[0],
        dy: base_dy + sums.pos[1],
        rot: base_rot + sums.rot,
        sx: 1.0 + sums.scale[0] / 100.0,
        sy: 1.0 + sums.scale[1] / 100.0,
        opacity_factor: clamp01(1.0 + sums.opacity / 100.0),
        hue_deg: sums.hue,
        dup_3d,
    }
}

#[cfg(test)]
#[path = "../tests/duplicator_placement_clone_controls.rs"]
mod controls_tests;
#[cfg(test)]
#[path = "../tests/duplicator_placement_clone_pins.rs"]
mod pins_tests;
#[cfg(test)]
#[path = "../tests/duplicator_placement_clone_codec.rs"]
pub(crate) mod test_codec;
#[cfg(test)]
#[path = "../tests/duplicator_placement_clone.rs"]
mod tests;
