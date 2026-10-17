//! Weight easing and the Wiggly noise of the text range selector (G38b /
//! L06-G): `bezierEaser`, `hashRand` and `smoothNoise1D` of
//! `src/js/text-selector.js` at fbe4b8a3, operation for operation.
//!
//! The easer is bisection, not Newton: at most 24 halvings, with the early exit
//! when `|calcX(t) - x| < 1e-9` (it only removes iterations). Every product is
//! evaluated left to right exactly as the JS source spells it
//! (`3 * u * u * t * x1` is `(((3 * u) * u) * t) * x1`), so the result is the
//! same double.
//!
//! `hashRand` calls `Math.sin`. This is Rust `f64::sin`, exactly as the sibling
//! `expression_noise.rs` (#1675) does for the same hash: the macOS desktop runs
//! JavaScriptCore, whose `Math.sin` agrees bit for bit with `f64::sin` there;
//! V8 is not a product surface. Where the platform libm differs by an ulp of
//! `sin`, the 43758.5453 multiplier turns it into about 5e-12 of hash, so the
//! tests pin trigonometric rows within a bound, not bit for bit (see the oracle
//! reader).

/// `bezierEaser(x1, y1, x2, y2)`; the identity curve returns its input untouched.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Easer {
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
}

impl Easer {
    pub(crate) fn new(x1: f64, y1: f64, x2: f64, y2: f64) -> Self {
        Easer { x1, y1, x2, y2 }
    }

    fn is_identity(&self) -> bool {
        self.x1 == 0.0 && self.y1 == 0.0 && self.x2 == 1.0 && self.y2 == 1.0
    }

    fn calc_x(&self, t: f64) -> f64 {
        let u = 1.0 - t;
        3.0 * u * u * t * self.x1 + 3.0 * u * t * t * self.x2 + t * t * t
    }

    fn calc_y(&self, t: f64) -> f64 {
        let u = 1.0 - t;
        3.0 * u * u * t * self.y1 + 3.0 * u * t * t * self.y2 + t * t * t
    }

    pub(crate) fn apply(&self, x: f64) -> f64 {
        if self.is_identity() {
            return x;
        }
        let mut lo = 0.0;
        let mut hi = 1.0;
        let mut t = x;
        for _ in 0..24 {
            let cx = self.calc_x(t);
            if (cx - x).abs() < 1e-9 {
                break;
            }
            if cx < x {
                lo = t;
            } else {
                hi = t;
            }
            t = (lo + hi) / 2.0;
        }
        self.calc_y(t)
    }
}

/// `hashRand(seed, i)`: `x - floor(x)` of `sin(seed * 12.9898 + i * 78.233) * 43758.5453`.
pub(crate) fn hash_rand(seed: f64, i: f64) -> f64 {
    let x = (seed * 12.9898 + i * 78.233).sin() * 43758.5453;
    x - x.floor()
}

/// `smoothNoise1D(seed, t)`: smoothstep-interpolated random keys at integer `t`, range -1..1.
pub(crate) fn smooth_noise_1d(seed: f64, t: f64) -> f64 {
    let i0 = t.floor();
    let i1 = i0 + 1.0;
    let f = t - i0;
    let v0 = hash_rand(seed, i0) * 2.0 - 1.0;
    let v1 = hash_rand(seed, i1) * 2.0 - 1.0;
    let u = f * f * (3.0 - 2.0 * f);
    v0 + (v1 - v0) * u
}

#[cfg(test)]
#[path = "../tests/text_glyph_selector_curve.rs"]
mod tests;
