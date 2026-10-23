//! Geometry helpers of the Delaunator port (L03-F, F17): the pure functions of
//! `src/js/delaunator.vendor.js` (baseline fbe4b8a3, lines 665-768) that the sweep in
//! `object_image_mesh_delaunay.rs` calls, ported in the baseline's operand order so they
//! are bit-identical to the JS doubles: `pseudoAngle`, `dist`, `inCircle`, `circumradius`,
//! `circumcenter` and the (unstable) `quicksort`.
//!
//! Derived from Delaunator 5.0.1, ISC License, Copyright (c) 2021, Mapbox:
//!
//! Permission to use, copy, modify, and/or distribute this software for any purpose with or
//! without fee is hereby granted, provided that the above copyright notice and this permission
//! notice appear in all copies.
//!
//! THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES WITH REGARD TO THIS
//! SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL
//! THE AUTHOR BE LIABLE FOR ANY SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY
//! DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN ACTION OF
//! CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF OR IN CONNECTION WITH THE USE
//! OR PERFORMANCE OF THIS SOFTWARE.

/// `pseudoAngle`: monotonic in the real angle, in [0, 1].
pub(crate) fn pseudo_angle(dx: f64, dy: f64) -> f64 {
    let p = dx / (dx.abs() + dy.abs());
    (if dy > 0.0 { 3.0 - p } else { 1.0 + p }) / 4.0
}

pub(crate) fn dist(ax: f64, ay: f64, bx: f64, by: f64) -> f64 {
    let dx = ax - bx;
    let dy = ay - by;
    dx * dx + dy * dy
}

/// `inCircle`: true when `p` lies strictly inside the circle through `a`, `b`, `c`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn in_circle(
    ax: f64,
    ay: f64,
    bx: f64,
    by: f64,
    cx: f64,
    cy: f64,
    px: f64,
    py: f64,
) -> bool {
    let dx = ax - px;
    let dy = ay - py;
    let ex = bx - px;
    let ey = by - py;
    let fx = cx - px;
    let fy = cy - py;

    let ap = dx * dx + dy * dy;
    let bp = ex * ex + ey * ey;
    let cp = fx * fx + fy * fy;

    dx * (ey * cp - bp * fy) - dy * (ex * cp - bp * fx) + ap * (ex * fy - ey * fx) < 0.0
}

pub(crate) fn circumradius(ax: f64, ay: f64, bx: f64, by: f64, cx: f64, cy: f64) -> f64 {
    let dx = bx - ax;
    let dy = by - ay;
    let ex = cx - ax;
    let ey = cy - ay;

    let bl = dx * dx + dy * dy;
    let cl = ex * ex + ey * ey;
    let d = 0.5 / (dx * ey - dy * ex);

    let x = (ey * bl - dy * cl) * d;
    let y = (dx * cl - ex * bl) * d;

    x * x + y * y
}

pub(crate) fn circumcenter(ax: f64, ay: f64, bx: f64, by: f64, cx: f64, cy: f64) -> [f64; 2] {
    let dx = bx - ax;
    let dy = by - ay;
    let ex = cx - ax;
    let ey = cy - ay;

    let bl = dx * dx + dy * dy;
    let cl = ex * ex + ey * ey;
    let d = 0.5 / (dx * ey - dy * ex);

    [ax + (ey * bl - dy * cl) * d, ay + (dx * cl - ex * bl) * d]
}

fn swap(ids: &mut [u32], i: isize, j: isize) {
    ids.swap(i as usize, j as usize);
}

fn dk(ids: &[u32], dists: &[f64], k: isize) -> f64 {
    dists[ids[k as usize] as usize]
}

/// `quicksort(ids, dists, left, right)`: sorts `ids` in place by `dists[id]`. NOT stable:
/// the order of equal distances is part of the result and is reproduced as in JS. `dists`
/// must not contain NaN (the sentinels of the median-of-three keep every index in range).
pub(crate) fn quicksort(ids: &mut [u32], dists: &[f64], left: isize, right: isize) {
    if right - left <= 20 {
        let mut i = left + 1;
        while i <= right {
            let temp = ids[i as usize];
            let temp_dist = dists[temp as usize];
            let mut j = i - 1;
            while j >= left && dk(ids, dists, j) > temp_dist {
                ids[(j + 1) as usize] = ids[j as usize];
                j -= 1;
            }
            ids[(j + 1) as usize] = temp;
            i += 1;
        }
        return;
    }
    let median = (left + right) >> 1;
    let mut i = left + 1;
    let mut j = right;
    swap(ids, median, i);
    if dk(ids, dists, left) > dk(ids, dists, right) {
        swap(ids, left, right);
    }
    if dk(ids, dists, i) > dk(ids, dists, right) {
        swap(ids, i, right);
    }
    if dk(ids, dists, left) > dk(ids, dists, i) {
        swap(ids, left, i);
    }
    let temp = ids[i as usize];
    let temp_dist = dists[temp as usize];
    loop {
        loop {
            i += 1;
            if dk(ids, dists, i) >= temp_dist {
                break;
            }
        }
        loop {
            j -= 1;
            if dk(ids, dists, j) <= temp_dist {
                break;
            }
        }
        if j < i {
            break;
        }
        swap(ids, i, j);
    }
    ids[(left + 1) as usize] = ids[j as usize];
    ids[j as usize] = temp;
    if right - i + 1 >= j - left {
        quicksort(ids, dists, i, right);
        quicksort(ids, dists, left, j - 1);
    } else {
        quicksort(ids, dists, left, j - 1);
        quicksort(ids, dists, i, right);
    }
}

/// The collinear branch of `update`: fills `dists` with the signed offsets from point 0 along x
/// (or y when the x offset is zero or NaN), sorts `ids` by them and keeps each strictly larger
/// distance once. `None` when a distance is NaN (a degenerate input the baseline leaves undefined).
pub(crate) fn collinear_order(
    coords: &[f64],
    dists: &mut [f64],
    ids: &mut [u32],
    n: usize,
) -> Option<Vec<u32>> {
    for i in 0..n {
        let dx = coords[2 * i] - coords[0];
        dists[i] = if dx != 0.0 && !dx.is_nan() {
            dx
        } else {
            coords[2 * i + 1] - coords[1]
        };
    }
    if dists.iter().any(|d| d.is_nan()) {
        return None;
    }
    quicksort(ids, dists, 0, n as isize - 1);
    let mut hull = Vec::new();
    let mut d0 = f64::NEG_INFINITY;
    for k in 0..n {
        let id = ids[k];
        let d = dists[id as usize];
        if d > d0 {
            hull.push(id);
            d0 = d;
        }
    }
    Some(hull)
}
