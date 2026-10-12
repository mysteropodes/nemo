//! Pure deterministic `random()`/`randomGauss()` draws for Motion expressions.
//!
//! This preserves the characterized desktop JavaScript baseline:
//! `NemoExpressionRandomDomain.rand01FromInputs`/`gaussFromUniforms`/
//! `randomWith` (`expression-random.js`) and the counter discipline of
//! `exprSeed`/`_rand01`/`_gauss01`/`_propSeedOffset`/`aliasSeedRandom`
//! (`motion.js`). Callers own the holder seed, the evaluation frame and the
//! stream's lifetime (one stream per evaluation); nothing here reads a
//! context, clock or global.
//!
//! The lattice hash is shared with `noise()`/`wiggle()` through
//! [`crate::evaluation::expression_noise::hash_unit`], never copied.

use crate::evaluation::expression_noise::hash_unit;

/// Weight of the rounded evaluation frame in the hash coordinate.
const FRAME_STRIDE: f64 = 1013.13;
/// Weight of the per-evaluation draw counter in the hash coordinate.
const COUNTER_STRIDE: f64 = 7919.0;
/// Box-Muller spread that keeps roughly nine draws in ten inside `0..1`.
const GAUSS_SPREAD: f64 = 0.304;
/// Lower clamp on the first uniform so `ln` stays finite.
const GAUSS_MIN_UNIFORM: f64 = 1e-9;

/// Per-property seed offset (`_propSeedOffset`): `h = (h * 31 + code) | 0`
/// over the UTF-16 code units of the property key, wrapping like JavaScript.
pub fn property_seed_offset(prop: &str) -> i32 {
    prop.encode_utf16().fold(0i32, |h, unit| {
        h.wrapping_mul(31).wrapping_add(i32::from(unit))
    })
}

/// The stream seed an evaluation starts from: holder seed plus the
/// property's offset, so two properties on one holder never draw in lockstep.
pub fn property_seed(holder_seed: f64, prop: &str) -> f64 {
    holder_seed + f64::from(property_seed_offset(prop))
}

/// JavaScript `Math.round`: halves go toward +infinity. Rust `round()`
/// rounds halves away from zero, which differs for negative halves.
fn js_round(x: f64) -> f64 {
    let floor = x.floor();
    if x - floor >= 0.5 {
        floor + 1.0
    } else {
        floor
    }
}

/// One uniform draw in `0..1` (`rand01FromInputs`) for the counter value
/// before its increment. `fixed` or `timeless` drops the frame term, so the
/// draw holds across frames for a given counter position.
pub fn rand01(seed: f64, counter: f64, frame: f64, timeless: bool, fixed: bool) -> f64 {
    let frame_term = if fixed || timeless {
        0.0
    } else {
        js_round(frame * 1000.0) / 1000.0
    };
    hash_unit(
        seed,
        frame_term * FRAME_STRIDE + counter * COUNTER_STRIDE + 0.5,
    )
}

/// Box-Muller transform of two already drawn uniforms (`gaussFromUniforms`),
/// centred on 0.5. A NaN `u1` stays NaN, like `Math.max(1e-9, NaN)`.
pub fn gauss_from_uniforms(u1: f64, u2: f64) -> f64 {
    if u1.is_nan() {
        return f64::NAN;
    }
    0.5 + GAUSS_SPREAD
        * (-2.0 * u1.max(GAUSS_MIN_UNIFORM).ln()).sqrt()
        * (2.0 * std::f64::consts::PI * u2).cos()
}

/// JavaScript `_num` for an already numeric argument: non-finite is 0.
fn finite_or_zero(value: f64) -> f64 {
    if value.is_finite() {
        value
    } else {
        0.0
    }
}

/// The draw state of one expression evaluation (`rngSeed`, `rngCounter`,
/// `frame`, `rngTimeless`). Every draw advances the counter, so repeated
/// calls in one evaluation differ even for the fixed variants.
#[derive(Debug, Clone, PartialEq)]
pub struct RandomStream {
    seed: f64,
    counter: u64,
    frame: f64,
    timeless: bool,
}

impl RandomStream {
    /// The stream `evalExpressionFor` opens for `prop` at `frame`.
    pub fn new(holder_seed: f64, prop: &str, frame: f64) -> Self {
        Self {
            seed: property_seed(holder_seed, prop),
            counter: 0,
            frame,
            timeless: false,
        }
    }

    pub fn seed(&self) -> f64 {
        self.seed
    }

    pub fn counter(&self) -> u64 {
        self.counter
    }

    pub fn timeless(&self) -> bool {
        self.timeless
    }

    /// Expression `seed(n)`: an explicit stream `n + 1` (non-finite `n` as 0)
    /// with the counter reset. The timeless flag is left as it was.
    pub fn reseed(&mut self, n: f64) {
        self.seed = finite_or_zero(n) + 1.0;
        self.counter = 0;
    }

    /// Expression `seedRandom(n, timeless)`: `seed(n)`, then the flag.
    pub fn seed_random(&mut self, n: f64, timeless: bool) {
        self.reseed(n);
        self.timeless = timeless;
    }

    /// One uniform draw in `0..1`; advances the counter by one.
    pub fn uniform(&mut self, fixed: bool) -> f64 {
        let value = rand01(
            self.seed,
            self.counter as f64,
            self.frame,
            self.timeless,
            fixed,
        );
        self.counter += 1;
        value
    }

    /// One bell-shaped draw from two uniform draws; advances the counter by two.
    pub fn gauss(&mut self, fixed: bool) -> f64 {
        let u1 = self.uniform(fixed);
        let u2 = self.uniform(fixed);
        gauss_from_uniforms(u1, u2)
    }
}

/// A `random()` bound: a number, or an array for a per-axis range. Callers
/// convert JavaScript values first: strings, `null`, sparse holes, nested or
/// typed arrays coerce as `_num`/`Array.isArray` would before reaching here.
#[derive(Debug, Clone, PartialEq)]
pub enum RandomBound {
    Scalar(f64),
    Vector(Vec<f64>),
}

/// A drawn value: scalar unless a bound was an array.
#[derive(Debug, Clone, PartialEq)]
pub enum RandomValue {
    Scalar(f64),
    Vector(Vec<f64>),
}

impl RandomBound {
    fn as_vector(&self) -> Vec<f64> {
        match self {
            RandomBound::Scalar(v) => vec![finite_or_zero(*v)],
            RandomBound::Vector(values) => values.clone(),
        }
    }
}

/// Applies the `randomWith` argument shapes to a draw source: no bound gives
/// the raw draw, one bound `0..max`, two bounds `min..max`. With an array
/// bound the result has the longer side's length, missing entries count as
/// 0, and draws are taken axis by axis in order.
pub fn random_with(
    mut draw: impl FnMut() -> f64,
    min_or_max: Option<&RandomBound>,
    max: Option<&RandomBound>,
) -> RandomValue {
    let Some(a) = min_or_max else {
        return RandomValue::Scalar(draw());
    };
    let Some(b) = max else {
        return match a {
            RandomBound::Scalar(v) => RandomValue::Scalar(draw() * finite_or_zero(*v)),
            RandomBound::Vector(values) => {
                RandomValue::Vector(values.iter().map(|v| draw() * finite_or_zero(*v)).collect())
            }
        };
    };
    match (a, b) {
        (RandomBound::Scalar(lo), RandomBound::Scalar(hi)) => {
            let (lo, hi) = (finite_or_zero(*lo), finite_or_zero(*hi));
            RandomValue::Scalar(lo + draw() * (hi - lo))
        }
        _ => {
            let (lows, highs) = (a.as_vector(), b.as_vector());
            let axis =
                |values: &[f64], i: usize| finite_or_zero(values.get(i).copied().unwrap_or(0.0));
            RandomValue::Vector(
                (0..lows.len().max(highs.len()))
                    .map(|i| {
                        let (lo, hi) = (axis(&lows, i), axis(&highs, i));
                        lo + draw() * (hi - lo)
                    })
                    .collect(),
            )
        }
    }
}
