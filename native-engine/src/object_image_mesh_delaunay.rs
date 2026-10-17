//! Delaunay triangulation for the image-mesh topology (L03-F, F17): a port of the baseline
//! (fbe4b8a3) `src/js/delaunator.vendor.js`, Delaunator 5.0.1 (class lines 302-662; helpers in
//! `object_image_mesh_delaunay_geom.rs`, robust predicate in `object_image_mesh_orient.rs`).
//!
//! The mesh grids are heavily co-circular, so every cell diagonal is decided by tie-breaking:
//! seed selection, the (unstable) quicksort of the sweep order, the hull-hash probe and the
//! legalize loop. All of it keeps the baseline's operand and branch order, so `triangles`,
//! `halfedges` and `hull` equal the JS typed arrays element for element; indices are `u32`.
//! Fewer than three points, a collinear set (empty `triangles`; `hull` lists the points by
//! increasing x, then y) and duplicates within `EPSILON` behave exactly as in JS. Inputs the
//! baseline turns into undefined values (a non-finite coordinate, a pseudo-angle of 0/0) are
//! typed errors here ([`delaunay_from`]). The baseline Delaunator does not throw on them (it
//! returns garbage arrays); [`triangulate_raw`] maps every error to an empty list as a chosen
//! refusal, so a caller that needs the typed reason must call [`delaunay_from`].
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

use crate::evaluation::object_image_mesh_kernels::object_image_mesh_delaunay_geom::{
    circumcenter, circumradius, collinear_order, dist, in_circle, pseudo_angle, quicksort,
};
use crate::evaluation::object_image_mesh_kernels::object_image_mesh_orient::orient2d;

const EPSILON: f64 = f64::EPSILON;
const EDGE_STACK_LEN: usize = 512;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DelaunayError {
    NonFiniteCoordinate { point: usize },
    DegenerateGeometry,
    HullProbeFailed,
    TriangleCapacity,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Delaunay {
    pub(crate) triangles: Vec<u32>,
    pub(crate) halfedges: Vec<i32>,
    pub(crate) hull: Vec<u32>,
}

/// The Delaunator instance state (the baseline class fields).
struct Builder {
    coords: Vec<f64>,
    triangles: Vec<u32>,
    halfedges: Vec<i32>,
    triangles_len: usize,
    hash_size: usize,
    hull_prev: Vec<u32>,
    hull_next: Vec<u32>,
    hull_tri: Vec<u32>,
    hull_hash: Vec<i32>,
    ids: Vec<u32>,
    dists: Vec<f64>,
    cx: f64,
    cy: f64,
    hull_start: usize,
}

impl Builder {
    fn new(coords: Vec<f64>) -> Builder {
        let n = coords.len() >> 1;
        let max_triangles = (2 * n).saturating_sub(5);
        let hash_size = (n as f64).sqrt().ceil() as usize;
        Builder {
            coords,
            triangles: vec![0; max_triangles * 3],
            halfedges: vec![0; max_triangles * 3],
            triangles_len: 0,
            hash_size,
            hull_prev: vec![0; n],
            hull_next: vec![0; n],
            hull_tri: vec![0; n],
            hull_hash: vec![0; hash_size],
            ids: vec![0; n],
            dists: vec![0.0; n],
            cx: 0.0,
            cy: 0.0,
            hull_start: 0,
        }
    }

    fn xy(&self, i: usize) -> (f64, f64) {
        (self.coords[2 * i], self.coords[2 * i + 1])
    }

    fn hash_key(&self, x: f64, y: f64) -> Result<usize, DelaunayError> {
        let size = self.hash_size as f64;
        let key = (pseudo_angle(x - self.cx, y - self.cy) * size).floor() % size;
        if key.is_finite() && key >= 0.0 {
            Ok(key as usize)
        } else {
            Err(DelaunayError::DegenerateGeometry)
        }
    }

    fn link(&mut self, a: usize, b: i32) {
        self.halfedges[a] = b;
        if b != -1 {
            self.halfedges[b as usize] = a as i32;
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn add_triangle(
        &mut self,
        i0: usize,
        i1: usize,
        i2: usize,
        a: i32,
        b: i32,
        c: i32,
    ) -> Result<usize, DelaunayError> {
        let t = self.triangles_len;
        if t + 3 > self.triangles.len() {
            return Err(DelaunayError::TriangleCapacity);
        }
        self.triangles[t] = i0 as u32;
        self.triangles[t + 1] = i1 as u32;
        self.triangles[t + 2] = i2 as u32;
        self.link(t, a);
        self.link(t + 1, b);
        self.link(t + 2, c);
        self.triangles_len += 3;
        Ok(t)
    }

    /// `_legalize`: flips illegal edges with the recursion replaced by a fixed stack.
    fn legalize(&mut self, a_start: usize) -> usize {
        let mut stack = [0u32; EDGE_STACK_LEN];
        let mut i = 0usize;
        let mut a = a_start;
        let mut ar;
        loop {
            let b = self.halfedges[a];
            let a0 = a - a % 3;
            ar = a0 + (a + 2) % 3;
            if b == -1 {
                if i == 0 {
                    break;
                }
                i -= 1;
                a = stack[i] as usize;
                continue;
            }
            let b = b as usize;
            let b0 = b - b % 3;
            let al = a0 + (a + 1) % 3;
            let bl = b0 + (b + 2) % 3;
            let p0 = self.triangles[ar] as usize;
            let pr = self.triangles[a] as usize;
            let pl = self.triangles[al] as usize;
            let p1 = self.triangles[bl] as usize;
            let (p0x, p0y) = self.xy(p0);
            let (prx, pry) = self.xy(pr);
            let (plx, ply) = self.xy(pl);
            let (p1x, p1y) = self.xy(p1);
            if in_circle(p0x, p0y, prx, pry, plx, ply, p1x, p1y) {
                self.triangles[a] = p1 as u32;
                self.triangles[b] = p0 as u32;
                let hbl = self.halfedges[bl];
                if hbl == -1 {
                    let mut e = self.hull_start;
                    loop {
                        if self.hull_tri[e] as usize == bl {
                            self.hull_tri[e] = a as u32;
                            break;
                        }
                        e = self.hull_prev[e] as usize;
                        if e == self.hull_start {
                            break;
                        }
                    }
                }
                self.link(a, hbl);
                let har = self.halfedges[ar];
                self.link(b, har);
                self.link(ar, bl as i32);
                let br = b0 + (b + 1) % 3;
                if i < EDGE_STACK_LEN {
                    stack[i] = br as u32;
                    i += 1;
                }
            } else {
                if i == 0 {
                    break;
                }
                i -= 1;
                a = stack[i] as usize;
            }
        }
        ar
    }
}

fn collinear_hull(b: &mut Builder, n: usize) -> Result<Delaunay, DelaunayError> {
    let hull = collinear_order(&b.coords, &mut b.dists, &mut b.ids, n)
        .ok_or(DelaunayError::DegenerateGeometry)?;
    Ok(Delaunay {
        triangles: Vec::new(),
        halfedges: Vec::new(),
        hull,
    })
}

/// `Delaunator.from(points, x, y)` followed by reading `triangles`, `halfedges` and `hull`.
pub(crate) fn delaunay_from(points: &[[f64; 2]]) -> Result<Delaunay, DelaunayError> {
    if let Some(point) = points
        .iter()
        .position(|p| !p[0].is_finite() || !p[1].is_finite())
    {
        return Err(DelaunayError::NonFiniteCoordinate { point });
    }
    let n = points.len();
    let mut b = Builder::new(points.iter().flat_map(|p| [p[0], p[1]]).collect());
    b.update(n)
}

impl Builder {
    fn update(&mut self, n: usize) -> Result<Delaunay, DelaunayError> {
        let (mut min_x, mut min_y) = (f64::INFINITY, f64::INFINITY);
        let (mut max_x, mut max_y) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
        for i in 0..n {
            let (x, y) = self.xy(i);
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
            self.ids[i] = i as u32;
        }
        let cx = (min_x + max_x) / 2.0;
        let cy = (min_y + max_y) / 2.0;

        // seed point close to the centre
        let mut seed0: Option<usize> = None;
        let mut min_dist = f64::INFINITY;
        for i in 0..n {
            let (x, y) = self.xy(i);
            let d = dist(cx, cy, x, y);
            if d < min_dist {
                seed0 = Some(i);
                min_dist = d;
            }
        }
        // the point closest to the seed, then the third forming the smallest circumcircle
        let mut seed1: Option<usize> = None;
        let mut seed2: Option<usize> = None;
        let mut min_radius = f64::INFINITY;
        if let Some(a) = seed0 {
            let (ax, ay) = self.xy(a);
            let mut min_dist = f64::INFINITY;
            for i in 0..n {
                if i == a {
                    continue;
                }
                let (x, y) = self.xy(i);
                let d = dist(ax, ay, x, y);
                if d < min_dist && d > 0.0 {
                    seed1 = Some(i);
                    min_dist = d;
                }
            }
            if let Some(c) = seed1 {
                let (bx, by) = self.xy(c);
                for i in 0..n {
                    if i == a || i == c {
                        continue;
                    }
                    let (x, y) = self.xy(i);
                    let r = circumradius(ax, ay, bx, by, x, y);
                    if r < min_radius {
                        seed2 = Some(i);
                        min_radius = r;
                    }
                }
            }
        }
        let (i0, mut i1, mut i2) = match (seed0, seed1, seed2) {
            (Some(a), Some(c), Some(d)) if min_radius != f64::INFINITY => (a, c, d),
            _ => return collinear_hull(self, n),
        };
        let (i0x, i0y) = self.xy(i0);
        let (i1x, i1y) = self.xy(i1);
        let (i2x, i2y) = self.xy(i2);
        // counter-clockwise orientation of the seed triangle
        if orient2d(i0x, i0y, i1x, i1y, i2x, i2y) < 0.0 {
            std::mem::swap(&mut i1, &mut i2);
        }
        let (i1x, i1y) = self.xy(i1);
        let (i2x, i2y) = self.xy(i2);

        let center = circumcenter(i0x, i0y, i1x, i1y, i2x, i2y);
        self.cx = center[0];
        self.cy = center[1];
        for i in 0..n {
            let (x, y) = self.xy(i);
            self.dists[i] = dist(x, y, self.cx, self.cy);
        }
        if self.dists.iter().any(|d| d.is_nan()) {
            return Err(DelaunayError::DegenerateGeometry);
        }
        quicksort(&mut self.ids, &self.dists, 0, n as isize - 1);

        // the seed triangle is the starting hull
        self.hull_start = i0;
        let mut hull_size: usize = 3;
        self.hull_next[i0] = i1 as u32;
        self.hull_prev[i2] = i1 as u32;
        self.hull_next[i1] = i2 as u32;
        self.hull_prev[i0] = i2 as u32;
        self.hull_next[i2] = i0 as u32;
        self.hull_prev[i1] = i0 as u32;
        self.hull_tri[i0] = 0;
        self.hull_tri[i1] = 1;
        self.hull_tri[i2] = 2;
        self.hull_hash.fill(-1);
        let key = self.hash_key(i0x, i0y)?;
        self.hull_hash[key] = i0 as i32;
        let key = self.hash_key(i1x, i1y)?;
        self.hull_hash[key] = i1 as i32;
        let key = self.hash_key(i2x, i2y)?;
        self.hull_hash[key] = i2 as i32;
        self.triangles_len = 0;
        self.add_triangle(i0, i1, i2, -1, -1, -1)?;

        let (mut xp, mut yp) = (0.0f64, 0.0f64);
        for k in 0..n {
            let i = self.ids[k] as usize;
            let (x, y) = self.xy(i);
            // skip near-duplicate points
            if k > 0 && (x - xp).abs() <= EPSILON && (y - yp).abs() <= EPSILON {
                continue;
            }
            xp = x;
            yp = y;
            // skip seed triangle points
            if i == i0 || i == i1 || i == i2 {
                continue;
            }
            self.insert_point(i, &mut hull_size)?;
        }

        let mut hull = Vec::with_capacity(hull_size);
        let mut e = self.hull_start;
        for _ in 0..hull_size {
            hull.push(e as u32);
            e = self.hull_next[e] as usize;
        }
        Ok(Delaunay {
            triangles: self.triangles[..self.triangles_len].to_vec(),
            halfedges: self.halfedges[..self.triangles_len].to_vec(),
            hull,
        })
    }

    /// One iteration of the sweep for point `i`: find a visible hull edge by the angular
    /// hash, add the first triangle, walk forward then backward adding and legalizing.
    fn insert_point(&mut self, i: usize, hull_size: &mut usize) -> Result<(), DelaunayError> {
        let (x, y) = self.xy(i);
        let mut start: i32 = 0;
        let key = self.hash_key(x, y)?;
        for j in 0..self.hash_size {
            start = self.hull_hash[(key + j) % self.hash_size];
            if start != -1 && start as u32 != self.hull_next[start as usize] {
                break;
            }
        }
        if start < 0 {
            return Err(DelaunayError::HullProbeFailed);
        }
        let start = self.hull_prev[start as usize] as usize;
        let mut e = start;
        loop {
            let q = self.hull_next[e] as usize;
            let (ex, ey) = self.xy(e);
            let (qx, qy) = self.xy(q);
            if orient2d(x, y, ex, ey, qx, qy) >= 0.0 {
                e = q;
                if e == start {
                    return Ok(()); // likely a near-duplicate point; skip it
                }
            } else {
                break;
            }
        }
        // the first triangle from the point
        let next_e = self.hull_next[e] as usize;
        let mut t = self.add_triangle(e, i, next_e, -1, -1, self.hull_tri[e] as i32)?;
        // flip triangles from the point until they satisfy the Delaunay condition
        self.hull_tri[i] = self.legalize(t + 2) as u32;
        self.hull_tri[e] = t as u32;
        *hull_size += 1;

        // walk forward through the hull
        let mut n2 = self.hull_next[e] as usize;
        loop {
            let q = self.hull_next[n2] as usize;
            let (nx, ny) = self.xy(n2);
            let (qx, qy) = self.xy(q);
            if orient2d(x, y, nx, ny, qx, qy) < 0.0 {
                let (hi, hn) = (self.hull_tri[i] as i32, self.hull_tri[n2] as i32);
                t = self.add_triangle(n2, i, q, hi, -1, hn)?;
                self.hull_tri[i] = self.legalize(t + 2) as u32;
                self.hull_next[n2] = n2 as u32; // mark as removed
                *hull_size = hull_size
                    .checked_sub(1)
                    .ok_or(DelaunayError::HullProbeFailed)?;
                n2 = q;
            } else {
                break;
            }
        }
        // walk backward from the other side
        if e == start {
            loop {
                let q = self.hull_prev[e] as usize;
                let (qx, qy) = self.xy(q);
                let (ex, ey) = self.xy(e);
                if orient2d(x, y, qx, qy, ex, ey) < 0.0 {
                    let (he, hq) = (self.hull_tri[e] as i32, self.hull_tri[q] as i32);
                    t = self.add_triangle(q, i, e, -1, he, hq)?;
                    self.legalize(t + 2);
                    self.hull_tri[q] = t as u32;
                    self.hull_next[e] = e as u32; // mark as removed
                    *hull_size = hull_size
                        .checked_sub(1)
                        .ok_or(DelaunayError::HullProbeFailed)?;
                    e = q;
                } else {
                    break;
                }
            }
        }
        // update the hull indices and save the two new edges in the hash table
        self.hull_start = e;
        self.hull_prev[i] = e as u32;
        self.hull_next[e] = i as u32;
        self.hull_prev[n2] = i as u32;
        self.hull_next[i] = n2 as u32;
        let key = self.hash_key(x, y)?;
        self.hull_hash[key] = i as i32;
        let (ex, ey) = self.xy(e);
        let key = self.hash_key(ex, ey)?;
        self.hull_hash[key] = e as i32;
        Ok(())
    }
}

/// The unfiltered half of the baseline `triangulate(pts, outlineCount)`: `[]` when there are
/// fewer than three points, when the set is collinear, or (a deliberate refusal, not baseline
/// parity) when [`delaunay_from`] returns any error, so this list alone cannot tell a refused
/// input from a collinear one; otherwise the plain `u32` triangle index list. The centroid-in-outline filter
/// belongs to the topology leaf that consumes this list.
pub(crate) fn triangulate_raw(points: &[[f64; 2]]) -> Vec<u32> {
    if points.len() < 3 {
        return Vec::new();
    }
    match delaunay_from(points) {
        Ok(d) => d.triangles,
        Err(_) => Vec::new(),
    }
}

#[cfg(test)]
#[path = "../tests/object_image_mesh_delaunay_controls.rs"]
mod controls_tests;
#[cfg(test)]
#[path = "../tests/object_image_mesh_delaunay_rows.rs"]
mod oracle_rows;
#[cfg(test)]
#[path = "../tests/object_image_mesh_delaunay.rs"]
mod tests;
