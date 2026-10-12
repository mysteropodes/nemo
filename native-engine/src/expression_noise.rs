//! Pure deterministic `noise()`/`wiggle()` math for Motion expressions.
//!
//! This preserves the characterized desktop JavaScript baseline:
//! `NemoExpressionRandomDomain.hashUnit` (`expression-random.js`) and
//! `hashNoise1D`/`hashNoise2D`/`exprNoise`/`_wiggleAxis`/`exprWiggle`
//! (`motion.js`). Callers own seed provisioning, the evaluation time and the
//! property dimension; nothing here reads a context, counter, clock or global.
//!
//! The macOS desktop runs JavaScriptCore, whose `Math.sin` agrees bit for bit
//! with Rust `f64::sin` there. V8 differs on a few large arguments; it is not
//! a product surface for this baseline.

/// Seed offset `exprNoise` adds so `noise()` does not reuse `wiggle()` streams.
pub const NOISE_SEED_OFFSET: f64 = 7717.0;

/// Per-axis seed offset of a two-dimensional `wiggle()`.
const AXIS_SEED_OFFSET: f64 = 101.0;
/// Per-octave seed offset inside one `wiggle()` axis.
const OCTAVE_SEED_OFFSET: f64 = 977.0;
/// Lattice row stride of the two-dimensional noise hash.
const ROW_STRIDE: f64 = 311.7;
/// From octave index 1024 the doubled frequency overflows to infinity and the
/// JavaScript result is NaN, so more octaves cannot change it.
const MAX_OCTAVE_COUNT: u32 = 1025;

/// The fractional sine hash in `0..1` (NaN for non-finite input).
///
/// Uses `v - v.floor()` like JavaScript, not `fract()`: they differ for
/// negative `v`.
pub fn hash_unit(seed: f64, n: f64) -> f64 {
    let v = (n * 12.9898 + seed * 78.233).sin() * 43758.5453;
    v - v.floor()
}

/// Smoothstep-interpolated 1D value noise in `0..1` (`hashNoise1D`).
pub fn value_noise_1d(seed: f64, x: f64) -> f64 {
    let i = x.floor();
    let f = x - i;
    let a = hash_unit(seed, i);
    let b = hash_unit(seed, i + 1.0);
    let t = f * f * (3.0 - 2.0 * f);
    a + (b - a) * t
}

/// Bilinear smoothstep 2D value noise in `0..1` (`hashNoise2D`).
pub fn value_noise_2d(seed: f64, x: f64, y: f64) -> f64 {
    let xi = x.floor();
    let yi = y.floor();
    let xf = x - xi;
    let yf = y - yi;
    let lattice = |a: f64, b: f64| hash_unit(seed, a + b * ROW_STRIDE);
    let u = xf * xf * (3.0 - 2.0 * xf);
    let v = yf * yf * (3.0 - 2.0 * yf);
    let n00 = lattice(xi, yi);
    let n10 = lattice(xi + 1.0, yi);
    let n01 = lattice(xi, yi + 1.0);
    let n11 = lattice(xi + 1.0, yi + 1.0);
    let a = n00 + (n10 - n00) * u;
    let b = n01 + (n11 - n01) * u;
    a + (b - a) * v
}

/// Expression `noise(x)` in `-1..1` for the holder seed.
pub fn noise_1d(seed: f64, x: f64) -> f64 {
    value_noise_1d(seed + NOISE_SEED_OFFSET, x) * 2.0 - 1.0
}

/// Expression `noise(x, y)` / `noise([x, y])` in `-1..1` for the holder seed.
pub fn noise_2d(seed: f64, x: f64, y: f64) -> f64 {
    value_noise_2d(seed + NOISE_SEED_OFFSET, x, y) * 2.0 - 1.0
}

/// Dimension of the property a `wiggle()` is evaluated for (`PROP_DIM`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WiggleDim {
    One,
    Two,
}

/// A scalar for a 1D property, or independent X/Y streams for a 2D property.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Wiggle {
    Scalar(f64),
    Pair([f64; 2]),
}

/// Octave iterations for an authored `octaves` argument.
///
/// JavaScript applies `Math.max(1, octaves || 1)` and loops while
/// `o < octaves`, so 0, NaN and values below 1 run once and fractions round
/// up. Counts above [`MAX_OCTAVE_COUNT`] already yield NaN, so they are capped
/// there; this also makes `+Infinity` return NaN where JavaScript never ends.
pub fn wiggle_octave_count(octaves: f64) -> u32 {
    if octaves.is_nan() || octaves <= 1.0 {
        return 1;
    }
    if octaves >= f64::from(MAX_OCTAVE_COUNT) {
        return MAX_OCTAVE_COUNT;
    }
    octaves.ceil() as u32
}

/// One fBm-normalized `wiggle()` axis (`_wiggleAxis`).
fn wiggle_axis(
    seed: f64,
    time_seconds: f64,
    axis: f64,
    freq_per_sec: f64,
    amp: f64,
    count: u32,
) -> f64 {
    let mut n = 0.0;
    let mut amp2 = 1.0;
    let mut freq2 = 1.0;
    let mut norm = 0.0;
    for o in 0..count {
        let octave_seed = seed + axis * AXIS_SEED_OFFSET + f64::from(o) * OCTAVE_SEED_OFFSET;
        n += (value_noise_1d(octave_seed, time_seconds * freq_per_sec * freq2) - 0.5) * 2.0 * amp2;
        norm += amp2;
        amp2 *= 0.5;
        freq2 *= 2.0;
    }
    (n / norm) * amp
}

/// Expression `wiggle(freq, amp, octaves)` at the evaluation time.
///
/// Uses the holder seed without the per-property offset that `random()`
/// applies; this baseline quirk is preserved, not fixed.
pub fn wiggle(
    seed: f64,
    time_seconds: f64,
    freq_per_sec: f64,
    amp: f64,
    octaves: f64,
    dim: WiggleDim,
) -> Wiggle {
    let count = wiggle_octave_count(octaves);
    let axis = |index: f64| wiggle_axis(seed, time_seconds, index, freq_per_sec, amp, count);
    match dim {
        WiggleDim::One => Wiggle::Scalar(axis(0.0)),
        WiggleDim::Two => Wiggle::Pair([axis(0.0), axis(1.0)]),
    }
}
