//! Adaptive, exact-sign `orient2d` for the image-mesh Delaunay port (L03-F, F17).
//!
//! A line-for-line port of the robust-predicates 3.0.2 `orient2d` that the vendored
//! `src/js/delaunator.vendor.js` (Delaunator 5.0.1, baseline fbe4b8a3, lines 33-296)
//! inlines: Shewchuk's adaptive predicate. Every floating-point operation keeps the
//! baseline's operand order, so the result is bit-identical to the JS double for the
//! same inputs (no fused multiply-add: Rust never contracts `a * b + c`).
//!
//! robust-predicates 3.0.2 (Mourner, built on Shewchuk's public-domain predicates) is released
//! as Unlicense / public domain, so no conditions attach to this file.
//!
//! The predicate is staged. The fast filter answers when the determinant clears the first error
//! bound; otherwise the adaptive stages refine it. [`orient2d_staged`] also reports which stage
//! answered so a test can prove every stage was exercised. The sign convention is the
//! baseline's: the result is `detleft - detright` of `(ay-cy)*(bx-cx) - (ax-cx)*(by-cy)`; its
//! sign is exact, its magnitude is only meaningful from the fast filter.

const EPSILON: f64 = 1.1102230246251565e-16;
const SPLITTER: f64 = 134217729.0;
const RESULT_ERR_BOUND: f64 = (3.0 + 8.0 * EPSILON) * EPSILON;
const CCW_ERR_BOUND_A: f64 = (3.0 + 16.0 * EPSILON) * EPSILON;
const CCW_ERR_BOUND_B: f64 = (2.0 + 12.0 * EPSILON) * EPSILON;
const CCW_ERR_BOUND_C: f64 = (9.0 + 64.0 * EPSILON) * EPSILON * EPSILON;

/// Which stage of the predicate produced the answer.
pub(crate) const STAGE_FAST: u8 = 0;
pub(crate) const STAGE_B: u8 = 1;
pub(crate) const STAGE_TAILS_ZERO: u8 = 2;
pub(crate) const STAGE_C: u8 = 3;
pub(crate) const STAGE_D: u8 = 4;

/// Element `i` of `a`, or 0.0 past the end. The baseline reads one slot past the
/// live length in `sum` (a typed-array `undefined`) but never uses that value.
fn at(a: &[f64], i: usize) -> f64 {
    a.get(i).copied().unwrap_or(0.0)
}

/// `fast_expansion_sum_zeroelim` (baseline `sum`): merges two expansions into `h`
/// without zero components and returns the number of components written.
pub(crate) fn sum(elen: usize, e: &[f64], flen: usize, f: &[f64], h: &mut [f64]) -> usize {
    let mut enow = e[0];
    let mut fnow = f[0];
    let mut eindex = 0usize;
    let mut findex = 0usize;
    let mut q;
    if (fnow > enow) == (fnow > -enow) {
        q = enow;
        eindex += 1;
        enow = at(e, eindex);
    } else {
        q = fnow;
        findex += 1;
        fnow = at(f, findex);
    }
    let mut hindex = 0usize;
    if eindex < elen && findex < flen {
        let qnew;
        let mut hh;
        if (fnow > enow) == (fnow > -enow) {
            qnew = enow + q;
            hh = q - (qnew - enow);
            eindex += 1;
            enow = at(e, eindex);
        } else {
            qnew = fnow + q;
            hh = q - (qnew - fnow);
            findex += 1;
            fnow = at(f, findex);
        }
        q = qnew;
        if hh != 0.0 {
            h[hindex] = hh;
            hindex += 1;
        }
        while eindex < elen && findex < flen {
            let qnew;
            let bvirt;
            if (fnow > enow) == (fnow > -enow) {
                qnew = q + enow;
                bvirt = qnew - q;
                hh = q - (qnew - bvirt) + (enow - bvirt);
                eindex += 1;
                enow = at(e, eindex);
            } else {
                qnew = q + fnow;
                bvirt = qnew - q;
                hh = q - (qnew - bvirt) + (fnow - bvirt);
                findex += 1;
                fnow = at(f, findex);
            }
            q = qnew;
            if hh != 0.0 {
                h[hindex] = hh;
                hindex += 1;
            }
        }
    }
    while eindex < elen {
        let qnew = q + enow;
        let bvirt = qnew - q;
        let hh = q - (qnew - bvirt) + (enow - bvirt);
        eindex += 1;
        enow = at(e, eindex);
        q = qnew;
        if hh != 0.0 {
            h[hindex] = hh;
            hindex += 1;
        }
    }
    while findex < flen {
        let qnew = q + fnow;
        let bvirt = qnew - q;
        let hh = q - (qnew - bvirt) + (fnow - bvirt);
        findex += 1;
        fnow = at(f, findex);
        q = qnew;
        if hh != 0.0 {
            h[hindex] = hh;
            hindex += 1;
        }
    }
    if q != 0.0 || hindex == 0 {
        h[hindex] = q;
        hindex += 1;
    }
    hindex
}

/// `estimate`: left-to-right sum of the first `elen` components.
fn estimate(elen: usize, e: &[f64]) -> f64 {
    let mut q = e[0];
    for v in e.iter().take(elen).skip(1) {
        q += *v;
    }
    q
}

/// Dekker split product: `a * b` as an unevaluated sum `(x, y)`.
fn two_product(a: f64, b: f64) -> (f64, f64) {
    let x = a * b;
    let mut c = SPLITTER * a;
    let ahi = c - (c - a);
    let alo = a - ahi;
    c = SPLITTER * b;
    let bhi = c - (c - b);
    let blo = b - bhi;
    let y = alo * blo - (x - ahi * bhi - alo * bhi - ahi * blo);
    (x, y)
}

/// `(s1 + s0) - (t1 + t0)` as a four-component expansion (the baseline's repeated
/// `_i/_j/_0/u3` block).
fn two_two_diff(s1: f64, s0: f64, t1: f64, t0: f64) -> [f64; 4] {
    let i = s0 - t0;
    let mut bvirt = s0 - i;
    let r0 = s0 - (i + bvirt) + (bvirt - t0);
    let j = s1 + i;
    bvirt = j - s1;
    let z = s1 - (j - bvirt) + (i - bvirt);
    let i2 = z - t1;
    bvirt = z - i2;
    let r1 = z - (i2 + bvirt) + (bvirt - t1);
    let u3 = j + i2;
    bvirt = u3 - j;
    let r2 = j - (u3 - bvirt) + (i2 - bvirt);
    [r0, r1, r2, u3]
}

/// Product pair `a * b` minus `c * d`, each as `two_product`, as an expansion.
fn product_diff(a: f64, b: f64, c: f64, d: f64) -> [f64; 4] {
    let (s1, s0) = two_product(a, b);
    let (t1, t0) = two_product(c, d);
    two_two_diff(s1, s0, t1, t0)
}

/// `orient2dadapt`: the refinement stages, reached when the fast filter is unsure.
fn orient2d_adapt(ax: f64, ay: f64, bx: f64, by: f64, cx: f64, cy: f64, detsum: f64) -> (f64, u8) {
    let acx = ax - cx;
    let bcx = bx - cx;
    let acy = ay - cy;
    let bcy = by - cy;

    let b = product_diff(acx, bcy, acy, bcx);
    let mut det = estimate(4, &b);
    let mut errbound = CCW_ERR_BOUND_B * detsum;
    if det >= errbound || -det >= errbound {
        return (det, STAGE_B);
    }

    let mut bvirt = ax - acx;
    let acxtail = ax - (acx + bvirt) + (bvirt - cx);
    bvirt = bx - bcx;
    let bcxtail = bx - (bcx + bvirt) + (bvirt - cx);
    bvirt = ay - acy;
    let acytail = ay - (acy + bvirt) + (bvirt - cy);
    bvirt = by - bcy;
    let bcytail = by - (bcy + bvirt) + (bvirt - cy);

    if acxtail == 0.0 && acytail == 0.0 && bcxtail == 0.0 && bcytail == 0.0 {
        return (det, STAGE_TAILS_ZERO);
    }

    errbound = CCW_ERR_BOUND_C * detsum + RESULT_ERR_BOUND * det.abs();
    det += (acx * bcytail + bcy * acxtail) - (acy * bcxtail + bcx * acytail);
    if det >= errbound || -det >= errbound {
        return (det, STAGE_C);
    }

    let mut c1 = [0.0f64; 8];
    let mut c2 = [0.0f64; 12];
    let mut d = [0.0f64; 16];
    let u = product_diff(acxtail, bcy, acytail, bcx);
    let c1len = sum(4, &b, 4, &u, &mut c1);
    let u = product_diff(acx, bcytail, acy, bcxtail);
    let c2len = sum(c1len, &c1, 4, &u, &mut c2);
    let u = product_diff(acxtail, bcytail, acytail, bcxtail);
    let dlen = sum(c2len, &c2, 4, &u, &mut d);
    (d[dlen - 1], STAGE_D)
}

/// `orient2d` with the answering stage. See the module note for the sign.
pub(crate) fn orient2d_staged(ax: f64, ay: f64, bx: f64, by: f64, cx: f64, cy: f64) -> (f64, u8) {
    let detleft = (ay - cy) * (bx - cx);
    let detright = (ax - cx) * (by - cy);
    let det = detleft - detright;

    let detsum = (detleft + detright).abs();
    if det.abs() >= CCW_ERR_BOUND_A * detsum {
        return (det, STAGE_FAST);
    }

    let (value, stage) = orient2d_adapt(ax, ay, bx, by, cx, cy, detsum);
    (-value, stage)
}

/// The baseline `orient2d(ax, ay, bx, by, cx, cy)`.
pub(crate) fn orient2d(ax: f64, ay: f64, bx: f64, by: f64, cx: f64, cy: f64) -> f64 {
    orient2d_staged(ax, ay, bx, by, cx, cy).0
}
