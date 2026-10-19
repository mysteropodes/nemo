//! Real curved-outline probes and union for the retained same-colour fill policy.
//! Curves use the retained kurbo adaptive flattener before the existing i_overlay
//! owner; inputs never discard handles as if the anchors were a straight ring.
use super::native::{bounds, contains, interior, polygon};
use super::{RingSeg, UnionProvider};
use crate::object_document::object_group_kernels::object_boolean_ops::{
    flatten_polygons, fold_multi, BoolOp, Island, MultiPolygon, Point,
};
use crate::object_document::object_group_kernels::object_boolean_topology::{
    MergeError, Polygon, ProbeError, TouchProbe,
};

fn cross(a: Point, b: Point, c: Point) -> f64 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}
fn on(p: Point, a: Point, b: Point) -> bool {
    cross(a, b, p).abs() < 1e-9
        && p[0] >= a[0].min(b[0]) - 1e-9
        && p[0] <= a[0].max(b[0]) + 1e-9
        && p[1] >= a[1].min(b[1]) - 1e-9
        && p[1] <= a[1].max(b[1]) + 1e-9
}
fn intersects(a: Point, b: Point, c: Point, d: Point) -> bool {
    (cross(a, b, c) * cross(a, b, d) < 0.0 && cross(c, d, a) * cross(c, d, b) < 0.0)
        || on(a, c, d)
        || on(b, c, d)
        || on(c, a, b)
        || on(d, a, b)
}
fn distance(p: Point, a: Point, b: Point) -> f64 {
    let d = [b[0] - a[0], b[1] - a[1]];
    let n = d[0] * d[0] + d[1] * d[1];
    let t = if n == 0.0 {
        0.0
    } else {
        ((p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1]) / n
    }
    .clamp(0.0, 1.0);
    (p[0] - a[0] - t * d[0]).hypot(p[1] - a[1] - t * d[1])
}
pub(crate) fn polygons(ring: &[RingSeg]) -> MultiPolygon {
    vec![Polygon {
        exterior: polygon(ring, true),
        holes: Vec::new(),
    }]
}

pub(super) struct NativeProbe {
    pub(super) rings: Vec<Vec<RingSeg>>,
    pub(super) stroked: Vec<bool>,
    pub(super) closed: Vec<bool>,
    pub(super) new_index: usize,
}
impl TouchProbe for NativeProbe {
    fn bounds_intersect(&mut self, k: usize) -> Result<bool, ProbeError> {
        let a = bounds(&self.rings[self.new_index]);
        let b = bounds(&self.rings[k]);
        Ok(a.x <= b.x + b.width
            && b.x <= a.x + a.width
            && a.y <= b.y + b.height
            && b.y <= a.y + a.height)
    }
    fn stroke_between(&mut self, k: usize) -> Result<bool, ProbeError> {
        let Some(a) = interior(&self.rings[self.new_index]) else {
            return Ok(false);
        };
        let Some(b) = interior(&self.rings[k]) else {
            return Ok(false);
        };
        for (i, ring) in self.rings.iter().enumerate() {
            if !self.stroked[i] {
                continue;
            }
            let p = polygon(ring, self.closed[i]);
            let n = if self.closed[i] {
                p.len()
            } else {
                p.len().saturating_sub(1)
            };
            if (0..n).any(|j| intersects(a, b, p[j], p[(j + 1) % p.len()])) {
                return Ok(true);
            }
        }
        Ok(false)
    }
    fn strict_touch(&mut self, k: usize) -> Result<bool, ProbeError> {
        let ar = &self.rings[self.new_index];
        let br = &self.rings[k];
        let a = polygon(ar, true);
        let b = polygon(br, true);
        Ok((0..a.len()).any(|i| {
            (0..b.len()).any(|j| intersects(a[i], a[(i + 1) % a.len()], b[j], b[(j + 1) % b.len()]))
        }) || interior(ar).is_some_and(|p| contains(br, p))
            || interior(br).is_some_and(|p| contains(ar, p)))
    }
    fn near_miss(&mut self, k: usize) -> Result<bool, ProbeError> {
        let p = polygon(&self.rings[k], true);
        Ok(self.rings[self.new_index]
            .iter()
            .any(|s| (0..p.len()).any(|i| distance(s.point, p[i], p[(i + 1) % p.len()]) < 3.0)))
    }
}

pub(super) struct NativeUnion {
    pub(super) rings: Vec<Vec<RingSeg>>,
    pub(super) result: MultiPolygon,
}
impl UnionProvider for NativeUnion {
    fn unite(&mut self, k: usize) -> Result<bool, MergeError> {
        self.result = fold_multi(
            BoolOp::Unite,
            &[self.result.clone(), polygons(&self.rings[k])],
        )
        .map_err(|_| MergeError::GeometryUnavailable)?;
        if self.result.is_empty() {
            return Err(MergeError::GeometryUnavailable);
        }
        Ok(true)
    }
}
impl NativeUnion {
    pub(super) fn islands(&self) -> Result<Vec<Island>, MergeError> {
        flatten_polygons(&self.result, true).map_err(MergeError::Topology)
    }
}
