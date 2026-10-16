//! Duplicator per-clone random stream (L04-B, I07).
//!
//! Mirrors `seededRng` (`src/js/tools.js`) and the seed and draw order of
//! `_duplicatorClonePlacement` (`src/js/app.js`) at baseline `fbe4b8a3`.
//!
//! `seededRng(seed)` is a mulberry32-style generator: the state `t` starts at
//! `seed >>> 0` and each call adds `0x6D2B79F5`, mixes with two `Math.imul`
//! rounds and returns `((r ^ r >>> 14) >>> 0) / 4294967296`. Every JavaScript
//! operation on `t` is taken modulo 2^32 (`>>>`, `^`, `Math.imul`), so the
//! stream is the `u32` wrapping arithmetic below; the JavaScript state is an
//! unbounded double but its low 32 bits are all that is ever read.
//!
//! Each clone `k` owns a fresh stream seeded by
//! `((dup.seed || 0) + k * 7919) >>> 0`, so clone streams are decorrelated by
//! the prime stride and a clone's draws never depend on another clone's.
//! A clone draws exactly ten numbers in this order, always, whichever
//! channels are random: `rx, ry, rrr, rsx, rsy, rop, rz, rrx, rry, rhue`.
//! The last four were appended after the first six so existing duplicators
//! keep their pattern; reordering would silently reshuffle every project.

use crate::evaluation::object_component_kernels::duplicator_placement_modes::JsNumber;

/// The prime that decorrelates clone streams (`k * 7919`).
pub(crate) const CLONE_STRIDE: f64 = 7919.0;
/// Draws a clone takes from its stream, whatever the random mode.
pub(crate) const DRAWS_PER_CLONE: usize = 10;

/// `x >>> 0`: truncate toward zero, then reduce modulo 2^32 (NaN and the
/// infinities are 0).
pub(crate) fn to_uint32(x: f64) -> u32 {
    if !x.is_finite() {
        return 0;
    }
    x.trunc().rem_euclid(4_294_967_296.0) as u32
}

/// `Math.imul` on the unsigned view of the same 32 bits.
fn imul(a: u32, b: u32) -> u32 {
    a.wrapping_mul(b)
}

/// `seededRng(seed)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SeededRng {
    t: u32,
}

impl SeededRng {
    /// A stream whose seed is already a `u32`.
    pub(crate) fn new(seed: u32) -> SeededRng {
        SeededRng { t: seed }
    }

    /// `seededRng(seed)` for any JavaScript number (`seed >>> 0`).
    pub(crate) fn from_js_seed(seed: f64) -> SeededRng {
        SeededRng::new(to_uint32(seed))
    }

    /// The 32-bit value before the division.
    pub(crate) fn next_u32(&mut self) -> u32 {
        self.t = self.t.wrapping_add(0x6D2B_79F5);
        let t = self.t;
        let mut r = imul(t ^ (t >> 15), 1 | t);
        r = r.wrapping_add(imul(r ^ (r >> 7), 61 | r)) ^ r;
        r ^ (r >> 14)
    }

    /// One call of the JavaScript closure, in `[0, 1)`.
    pub(crate) fn next_f64(&mut self) -> f64 {
        self.next_u32() as f64 / 4_294_967_296.0
    }
}

/// `((seed || 0) + k * stride) >>> 0`.
pub(crate) fn stream_seed(seed: JsNumber, k: u32, stride: f64) -> u32 {
    to_uint32(seed.or_default(0.0) + (k as f64) * stride)
}

/// Seed of clone `k`'s stream (stride 7919).
pub(crate) fn clone_seed(seed: JsNumber, k: u32) -> u32 {
    stream_seed(seed, k, CLONE_STRIDE)
}

/// The ten draws of one clone, in draw order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CloneDraws {
    pub rx: f64,
    pub ry: f64,
    pub rrr: f64,
    pub rsx: f64,
    pub rsy: f64,
    pub rop: f64,
    pub rz: f64,
    pub rrx: f64,
    pub rry: f64,
    pub rhue: f64,
}

impl CloneDraws {
    /// The draws in order, for comparison with a raw stream.
    pub(crate) fn as_array(&self) -> [f64; DRAWS_PER_CLONE] {
        [
            self.rx, self.ry, self.rrr, self.rsx, self.rsy, self.rop, self.rz, self.rrx, self.rry,
            self.rhue,
        ]
    }
}

/// Ten draws from a fresh stream with the given `u32` seed.
pub(crate) fn draws_for_seed(seed: u32) -> CloneDraws {
    let mut rng = SeededRng::new(seed);
    let rx = rng.next_f64();
    let ry = rng.next_f64();
    let rrr = rng.next_f64();
    let rsx = rng.next_f64();
    let rsy = rng.next_f64();
    let rop = rng.next_f64();
    let rz = rng.next_f64();
    let rrx = rng.next_f64();
    let rry = rng.next_f64();
    let rhue = rng.next_f64();
    CloneDraws {
        rx,
        ry,
        rrr,
        rsx,
        rsy,
        rop,
        rz,
        rrx,
        rry,
        rhue,
    }
}

/// The draws of clone `k` of a duplicator with the given `dup.seed`.
pub(crate) fn clone_draws(seed: JsNumber, k: u32) -> CloneDraws {
    draws_for_seed(clone_seed(seed, k))
}

#[cfg(test)]
#[path = "../tests/duplicator_placement_rng_controls.rs"]
mod controls_tests;
#[cfg(test)]
#[path = "../tests/duplicator_placement_rng.rs"]
mod tests;
