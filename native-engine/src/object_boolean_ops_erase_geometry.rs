//! The geometry an eraser bite subtracts from (`eraseAtPoint` of `src/js/tools.js` @1065e9d8, B-eraser-tool): the
//! flattened ring of a curved closed record and the stroked outline of an open one. Boolean Unite/Subtract/Exclude
//! keep refusing curved and stroke-only operands; the eraser alone needs them, because a drawn pressure ribbon is a
//! smoothed outline (non-zero handles) and a Line / Pen stroke is open.
//!
//! The baseline flattened a Paper.js curve (WASM `bezpath_to_polygon_in`: flatten tolerance 0.25) and expanded an open
//! stroke with `eraseExpandStrokeToFill` (WASM: `kurbo::stroke`, tolerance 0.1). The same kurbo calls are made here, so
//! the bitten silhouette is the polygon the baseline subtracted from. An open path with a fill is the union of its fill
//! (closed by the straight implicit segment) and its stroke, as `fillGeom.unite(strokeGeom)` was.
use crate::object_document::object_group_kernels::object_boolean_ops::plan::{number_of, PlanError};
use crate::object_document::object_group_kernels::object_boolean_ops::stack::OutlineSpec;
use crate::object_document::object_group_kernels::object_boolean_ops::{
    fold_multi, BoolOp, MultiPolygon, Point,
};
use crate::object_document::object_group_kernels::object_boolean_topology::Polygon;
use crate::object_document::ObjectRecord;
use kurbo::{flatten, stroke, BezPath, Cap, Join, PathEl, Point as KPoint, Stroke, StrokeOpts};

/// `flatten` tolerance of the baseline's polygon conversion.
const FLATTEN_TOLERANCE: f64 = 0.25;
/// `stroke` tolerance of the baseline's stroke expansion.
const STROKE_TOLERANCE: f64 = 0.1;
/// Squared distance under which two consecutive vertices are one (the baseline's `DEDUP_EPS_SQ`).
const DEDUP_EPS_SQ: f64 = 1e-6;

fn anchors(record: &ObjectRecord) -> Vec<([f64; 2], [f64; 2], [f64; 2])> {
    record
        .geometry
        .segments
        .iter()
        .map(|s| {
            (
                [number_of(&s.point.x), number_of(&s.point.y)],
                [number_of(&s.handle_in.x), number_of(&s.handle_in.y)],
                [number_of(&s.handle_out.x), number_of(&s.handle_out.y)],
            )
        })
        .collect()
}

/// A record's centre path as cubics (handles are relative to their anchor); `closed` adds the closing curve.
fn bez(record: &ObjectRecord, closed: bool) -> BezPath {
    let a = anchors(record);
    let mut path = BezPath::new();
    if a.is_empty() {
        return path;
    }
    path.move_to(KPoint::new(a[0].0[0], a[0].0[1]));
    let joint = |from: usize, to: usize, path: &mut BezPath| {
        let (p, _, out) = a[from];
        let (q, inn, _) = a[to];
        path.curve_to(
            KPoint::new(p[0] + out[0], p[1] + out[1]),
            KPoint::new(q[0] + inn[0], q[1] + inn[1]),
            KPoint::new(q[0], q[1]),
        );
    };
    for i in 1..a.len() {
        joint(i - 1, i, &mut path);
    }
    if closed && a.len() > 1 {
        joint(a.len() - 1, 0, &mut path);
        path.close_path();
    }
    path
}

fn straight(record: &ObjectRecord) -> bool {
    record.geometry.segments.iter().all(|s| {
        [&s.handle_in, &s.handle_out]
            .iter()
            .all(|h| h.x.as_f64() == Some(0.0) && h.y.as_f64() == Some(0.0))
    })
}

/// The flattened contours of a path (one per sub-path), consecutive duplicates and a closing repeat dropped.
fn contours(path: &BezPath) -> Vec<Vec<Point>> {
    let mut out: Vec<Vec<Point>> = Vec::new();
    flatten(path.elements().iter().copied(), FLATTEN_TOLERANCE, |el| match el {
        PathEl::MoveTo(p) => out.push(vec![[p.x, p.y]]),
        PathEl::LineTo(p) => {
            if let Some(ring) = out.last_mut() {
                let at = [p.x, p.y];
                let dup = ring.last().is_some_and(|last| {
                    let (dx, dy) = (last[0] - at[0], last[1] - at[1]);
                    dx * dx + dy * dy < DEDUP_EPS_SQ
                });
                if !dup {
                    ring.push(at);
                }
            }
        }
        _ => {}
    });
    for ring in &mut out {
        if ring.len() > 1 {
            let (a, b) = (ring[0], ring[ring.len() - 1]);
            if (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) < DEDUP_EPS_SQ {
                ring.pop();
            }
        }
    }
    out.retain(|ring| ring.len() >= 3);
    out
}

/// The ring of a closed record: its own points when straight (exactly what the record stores), the flattened curve
/// otherwise.
pub(super) fn closed_ring(record: &ObjectRecord) -> Result<Vec<Point>, PlanError> {
    let stroke_id = record.target.stroke_id.clone();
    let ring = if straight(record) {
        anchors(record).iter().map(|a| a.0).collect()
    } else {
        contours(&bez(record, true))
            .into_iter()
            .next()
            .unwrap_or_default()
    };
    if ring.len() < 3 {
        return Err(PlanError::Degenerate(stroke_id));
    }
    Ok(ring)
}

fn cap_of(name: &str) -> Cap {
    match name {
        "round" => Cap::Round,
        "square" => Cap::Square,
        _ => Cap::Butt,
    }
}

fn join_of(name: &str) -> Join {
    match name {
        "round" => Join::Round,
        "bevel" => Join::Bevel,
        _ => Join::Miter,
    }
}

/// An open record as the area an eraser can bite: its stroke expanded to a fill, united with its fill when it has one.
/// `None`: nothing paints (the bite leaves it alone).
pub(super) fn open_area(
    record: &ObjectRecord,
    outline: Option<&OutlineSpec>,
    filled: bool,
) -> Result<Option<MultiPolygon>, PlanError> {
    let mut parts: Vec<MultiPolygon> = Vec::new();
    if filled {
        let fill = contours(&bez(record, true));
        if let Some(ring) = fill.into_iter().next() {
            parts.push(vec![Polygon {
                exterior: ring,
                holes: Vec::new(),
            }]);
        }
    }
    if let Some(spec) = outline {
        let style = Stroke::new(number_of(&spec.width).max(0.1))
            .with_caps(cap_of(&spec.cap))
            .with_join(join_of(&spec.join));
        let expanded = stroke(
            bez(record, false).elements().iter().copied(),
            &style,
            &StrokeOpts::default(),
            STROKE_TOLERANCE,
        );
        let rings = contours(&expanded);
        if !rings.is_empty() {
            parts.push(
                rings
                    .into_iter()
                    .map(|exterior| Polygon {
                        exterior,
                        holes: Vec::new(),
                    })
                    .collect(),
            );
        }
    }
    match parts.len() {
        0 => Ok(None),
        1 => Ok(parts.pop()),
        _ => fold_multi(BoolOp::Unite, &parts)
            .map(Some)
            .map_err(PlanError::Ops),
    }
}
