//! Actual stored-vector walls for the retained fill graph. Inputs are immutable native
//! records in one context/layer/frame; the provider has no writable document or JS port.
use crate::object_document::object_group_kernels::object_fill_trace_ops::geometry::{self, FillInput, FillResult, Wall};
use crate::object_document::object_group_kernels::object_fill_trace_ops::{FillTraceProvider, RingSeg, TraceFailure, TraceHit, RASTER_GAP};
use crate::object_document::object_group_kernels::object_boolean_ops::stack::Scope;
use crate::object_document::object_group_kernels::object_boolean_ops::{Point, MAX_ABS_COORD};
use crate::object_document::object_group_kernels::object_boolean_topology::Bounds;
use crate::object_document::{ObjectDocument, ObjectPoint, ObjectRecord};
use kurbo::{BezPath, Shape};

#[path = "object_fill_native_crossings.rs"]
mod crossings;
#[path = "object_fill_native_raster.rs"]
mod raster;

pub(crate) fn point(p: &ObjectPoint) -> Point {
    [p.x.as_f64().unwrap(), p.y.as_f64().unwrap()]
}

pub(crate) fn ring(record: &ObjectRecord) -> Vec<RingSeg> {
    record
        .geometry
        .segments
        .iter()
        .map(|s| RingSeg {
            point: point(&s.point),
            handle_in: point(&s.handle_in),
            handle_out: point(&s.handle_out),
        })
        .collect()
}

pub(crate) fn path(ring: &[RingSeg], closed: bool) -> BezPath {
    let mut p = BezPath::new();
    if ring.is_empty() {
        return p;
    }
    p.move_to((ring[0].point[0], ring[0].point[1]));
    let n = if closed { ring.len() } else { ring.len() - 1 };
    for i in 0..n {
        let a = ring[i];
        let b = ring[(i + 1) % ring.len()];
        p.curve_to(
            (a.point[0] + a.handle_out[0], a.point[1] + a.handle_out[1]),
            (b.point[0] + b.handle_in[0], b.point[1] + b.handle_in[1]),
            (b.point[0], b.point[1]),
        );
    }
    if closed {
        p.close_path();
    }
    p
}

pub(crate) fn polygon(ring: &[RingSeg], closed: bool) -> Vec<Point> {
    let p = path(ring, closed);
    let mut out = Vec::new();
    kurbo::flatten(p.elements().iter().copied(), 0.75, |e| {
        if let kurbo::PathEl::MoveTo(p) | kurbo::PathEl::LineTo(p) = e {
            out.push([p.x, p.y]);
        }
    });
    if closed && out.len() > 1 && out.first() == out.last() {
        out.pop();
    }
    out
}

pub(crate) fn bounds(ring: &[RingSeg]) -> Bounds {
    let b = path(ring, true).bounding_box();
    Bounds {
        x: b.x0,
        y: b.y0,
        width: b.width(),
        height: b.height(),
    }
}
pub(crate) fn area(ring: &[RingSeg]) -> f64 {
    path(ring, true).area().abs()
}
pub(crate) fn contains(ring: &[RingSeg], p: Point) -> bool {
    path(ring, true).winding(kurbo::Point::new(p[0], p[1])) != 0
}

/// A point inside a simple filled ring, chosen by an interior scan interval.
pub(crate) fn interior(ring: &[RingSeg]) -> Option<Point> {
    let poly = polygon(ring, true);
    let mut ys: Vec<f64> = poly.iter().map(|p| p[1]).collect();
    ys.sort_by(f64::total_cmp);
    ys.dedup();
    for pair in ys.windows(2) {
        let y = (pair[0] + pair[1]) / 2.0;
        let mut xs = Vec::new();
        for i in 0..poly.len() {
            let a = poly[i];
            let b = poly[(i + 1) % poly.len()];
            if (a[1] > y) != (b[1] > y) {
                xs.push(a[0] + (b[0] - a[0]) * (y - a[1]) / (b[1] - a[1]));
            }
        }
        xs.sort_by(f64::total_cmp);
        for pair in xs.windows(2) {
            let p = [(pair[0] + pair[1]) / 2.0, y];
            if pair[0] < pair[1] && contains(ring, p) {
                return Some(p);
            }
        }
    }
    None
}

#[derive(Clone)]
pub(super) struct StoredWall {
    pub(super) id: String,
    pub(super) ring: Vec<RingSeg>,
    pub(super) closed: bool,
    pub(super) filled: bool,
    pub(super) width: f64,
}

pub(crate) struct NativeTrace {
    pub(super) walls: Vec<StoredWall>,
    pub(super) click: Point,
}

impl NativeTrace {
    pub(crate) fn new(
        document: &ObjectDocument,
        scope: &Scope,
        click: Point,
        exclude: Option<&str>,
        only: Option<&[String]>,
    ) -> Result<Self, TraceFailure> {
        let mut walls = Vec::new();
        for record in document.objects.iter().filter(|r| scope.owns(&r.target)) {
            let id = &record.target.stroke_id;
            if exclude == Some(id.as_str()) || only.is_some_and(|ids| !ids.contains(id)) {
                continue;
            }
            let pressure = record.vector_ext.value().and_then(|e| e.pressure.value());
            let stroked = record.vector().is_some_and(|v| v.solid_stroke().is_some());
            if !stroked && record.solid_fill().is_none() && pressure.is_none() {
                continue;
            }
            let center = pressure
                .and_then(|p| p.center_segments.value())
                .filter(|c| c.len() > 1);
            let ring = match center {
                Some(c) => c
                    .iter()
                    .map(|s| RingSeg {
                        point: point(&s.point),
                        handle_in: point(&s.handle_in),
                        handle_out: point(&s.handle_out),
                    })
                    .collect(),
                None => ring(record),
            };
            if ring
                .iter()
                .flat_map(|s| [s.point, s.handle_in, s.handle_out])
                .flatten()
                .any(|n| !n.is_finite() || n.abs() > MAX_ABS_COORD)
            {
                return Err(TraceFailure::Failed);
            }
            let closed = center.is_none() && record.geometry.closed;
            let near = ring.first().zip(ring.last()).is_some_and(|(a, b)| {
                (a.point[0] - b.point[0]).hypot(a.point[1] - b.point[1]) < 0.5
            });
            walls.push(StoredWall {
                id: id.clone(),
                ring,
                closed: closed || near,
                filled: record.solid_fill().is_some() && center.is_none(),
                width: record
                    .vector()
                    .and_then(|v| v.stroke_width.as_f64())
                    .unwrap_or(2.0),
            });
        }
        Ok(Self { walls, click })
    }
}

impl FillTraceProvider for NativeTrace {
    fn trace(&mut self, gap: u32) -> Result<Option<TraceHit>, TraceFailure> {
        let mut open_walls = Vec::new();
        let mut closed_walls = Vec::new();
        let mut open_ids = Vec::new();
        let mut closed_ids = Vec::new();
        for wall in &self.walls {
            let mut segments = wall.ring.clone();
            if wall.closed {
                closed_walls.push(Wall {
                    segments: segments.clone(),
                });
                closed_ids.push(wall);
                let first = segments[0];
                segments[0].handle_in = [0.0; 2];
                segments.push(RingSeg {
                    point: first.point,
                    handle_in: first.handle_in,
                    handle_out: [0.0; 2],
                });
            }
            open_walls.push(Wall { segments });
            open_ids.push(wall.id.clone());
        }
        let cuts = crossings::find(&open_walls, f64::from(gap).max(6.0));
        let result = geometry::find(FillInput {
            open_walls,
            closed_walls,
            gap_thr: f64::from(gap),
            click: self.click,
            crossings: Some(cuts),
        });
        let (ring, wall_ids, used_gap) = match result {
            FillResult::NotFound => return Ok(None),
            FillResult::ClosedWall { index } => {
                let wall = closed_ids[index];
                (wall.ring.clone(), vec![wall.id.clone()], false)
            }
            FillResult::Traced {
                segments,
                walls,
                used_gap,
            } => (
                segments,
                walls.into_iter().map(|i| open_ids[i].clone()).collect(),
                used_gap,
            ),
        };
        Ok(Some(TraceHit {
            area: area(&ring),
            ring,
            wall_ids,
            used_gap,
            gap_px: gap,
        }))
    }
    fn raster(&mut self) -> Result<Option<TraceHit>, TraceFailure> {
        raster::find(&self.walls, self.click, RASTER_GAP)
    }
}
