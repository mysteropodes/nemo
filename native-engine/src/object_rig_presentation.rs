//! #2255: the on-canvas facts of one layer's rig at one frame, from one immutable snapshot, in bounded parts.
//!
//! The Rig canvas tool (`src/js/rig-bridge.js`) draws the bones with their handles and anchors, the influence circle of each bone
//! (Assigner), and hit-tests an anchor or handle (Déplacer), an influence-circle rim and a bound vertex (the weight popover). This
//! module returns exactly those facts, in the six parts a request names (`bones`, `vertices`, `chains`, `binds`, `bound`,
//! `weights`), each a flat list a page of which stays under the response byte limit. Nothing is written, cached or kept.
//!
//! The record is read as it is stored: the live pose `segments` (what `moveSegment` writes), never the rest pose; a `preview` (a held
//! pose drag, `object_rig_preview.rs`) reads a CANDIDATE rig instead and writes nothing. `bound` is the posed vertex of every bind that a
//! pose evaluation of the same snapshot reports as posed, `posed` the posed outline of each (a stroke's segments with handles, a mesh's
//! contour). Coordinates are the layer's own space (the space the edits write); a layer with Motion or a parent carries its
//! world map as `matrix` (`object_rig_placement.rs`), computed on the first page of a part only, and a map that cannot be had is a
//! typed reason (`three_d`, `expression`, `sample`, `parent`, `mixed`, `contour`, `empty`) and no geometry, never a guessed map.
use crate::object_snapshot::object_rig_pose_read::{evaluate_layer_pose_over, PoseBind};
use crate::object_snapshot::rig_placement::layer_matrix;
use crate::object_snapshot::rig_preview::{candidate, Preview};
use crate::request_receipts::DispatchErrorCode;
use crate::revision::ObjectSnapshot;
use serde_json::{json, Number, Value};

type Slot<'a> = Option<&'a Option<[Number; 2]>>;
/// A bone handle as the kernels read it: the stored vector, zero when the record has none.
fn vector(slot: Slot) -> [f64; 2] {
    match slot {
        Some(Some(v)) => [v[0].as_f64().unwrap_or(0.0), v[1].as_f64().unwrap_or(0.0)],
        _ => [0.0, 0.0],
    }
}

/// The part a request names and the most items one page of it may carry.
pub(super) const PARTS: [(&str, usize); 7] = [
    ("bones", 8),
    ("vertices", 16),
    ("chains", 8),
    ("binds", 8),
    ("bound", 64),
    ("weights", 8),
    ("posed", 24),
];

/// The answer: why the geometry is unavailable (`None` when it is not) and the whole flat list of the part.
pub(super) struct Presented {
    pub(super) unavailable: Option<&'static str>,
    /// The layer-to-world affine (page 0 only); `None` is the identity.
    pub(super) matrix: Option<[f64; 6]>,
    pub(super) items: Vec<Value>,
}

impl ObjectSnapshot {
    /// The flat list of `part` for `layer_uid`; `select` is `(bind, vertex)` of `weights` and zeros otherwise. A `preview` answers from
    /// the candidate rig it makes; `first_page` asks for the layer-to-world `matrix` (later pages carry none).
    pub(super) fn rig_presentation_items(
        &self,
        layer_uid: &str,
        part: &str,
        frame: u32,
        select: (usize, usize),
        preview: Option<&Preview>,
        first_page: bool,
    ) -> Result<Presented, DispatchErrorCode> {
        let (bind, vertex) = select;
        let layer = self
            .document()
            .layers()
            .iter()
            .find(|layer| layer.layer_uid() == layer_uid)
            .ok_or(DispatchErrorCode::NotFound)?;
        let none = |unavailable| Presented {
            unavailable,
            matrix: None,
            items: Vec::new(),
        };
        let held = match preview {
            Some(p) => Some(
                layer
                    .rig()
                    .and_then(|rig| candidate(rig, p))
                    .ok_or(DispatchErrorCode::InvalidRequest)?,
            ),
            None => None,
        };
        let matrix = if first_page && layer.rig().is_some() {
            match layer_matrix(self, layer_uid, frame, held.as_ref()) {
                Ok(matrix) => matrix,
                Err(reason) => return Ok(none(Some(reason))),
            }
        } else {
            None
        };
        let Some(rig) = held.as_ref().or(layer.rig()) else {
            return Ok(none(None));
        };
        let pair = |value: &[Number; 2]| json!([value[0], value[1]]);
        let items = match part {
            "bones" => rig
                .bones
                .entries()
                .iter()
                .map(|(id, bone)| {
                    let curves: Vec<[[f64; 2]; 3]> = bone
                        .segments
                        .iter()
                        .map(|s| [s.point_value(), vector(s.handle_in.value()), vector(s.handle_out.value())])
                        .collect();
                    json!({"boneId":id,"closed":bone.closed,"radius":bone.radius.value(),
                        "count":bone.segments.len(),"center":bounds_center(&curves, bone.closed)})
                })
                .collect(),
            "vertices" => rig
                .bones
                .entries()
                .iter()
                .flat_map(|(id, bone)| {
                    bone.segments.iter().enumerate().map(move |(vi, s)| (id, vi, s))
                })
                .map(|(id, vi, s)| {
                    json!({"boneId":id,"vi":vi,"point":pair(&s.point),
                        "handleIn":vector(s.handle_in.value()),"handleOut":vector(s.handle_out.value())})
                })
                .collect(),
            "chains" => rig
                .ik_chains
                .entries()
                .iter()
                .map(|(key, chain)| {
                    macro_rules! at {
                        ($r:expr) => {
                            json!({"boneId":$r.bone_id,"vi":$r.vi})
                        };
                    }
                    json!({"endKey":key,"root":at!(chain.root),"joint":at!(chain.joint),"end":at!(chain.end),
                        "l1":chain.l1,"l2":chain.l2,"flip":chain.flip})
                })
                .collect(),
            "binds" | "bound" | "posed" => {
                let pose = evaluate_layer_pose_over(self, layer_uid, frame, held.as_ref()).ok();
                if part == "binds" {
                    rig.binds
                        .iter()
                        .enumerate()
                        .map(|(index, b)| {
                            json!({"index":index,"strokeId":b.stroke_id.value(),"meshId":b.mesh_id.value(),
                                "points":b.rest.len(),"rotate":b.rotate,
                                "state":pose.as_ref().and_then(|p| p.binds.get(index)).map_or("unavailable", state_name)})
                        })
                        .collect()
                } else {
                    // `bound`: [bind, vertex, x, y] of every posed vertex (a mesh's are its posed world vertices).
                    // `posed`: [bind, vertex, x, y, inX, inY, outX, outY, closed] of the posed outline, rounded to 0.01 (a held drag's
                    // preview is presentation only, and the page must stay under the response byte limit).
                    let r2 = |v: f64| (v * 100.0).round() / 100.0;
                    let mut out = Vec::new();
                    for (index, b) in rig.binds.iter().enumerate() {
                        let Some(pose) = pose.as_ref().filter(|p| p.binds.get(index) == Some(&PoseBind::Posed)) else {
                            continue;
                        };
                        let stroke = b.stroke_id.value().and_then(|id| pose.strokes.iter().find(|s| &s.stroke_id == id));
                        let mesh = b.mesh_id.value().and_then(|id| pose.meshes.iter().find(|m| &m.mesh_id == id));
                        for (vi, v) in stroke.iter().flat_map(|s| s.vertices.iter()).enumerate() {
                            out.push(if part == "bound" {
                                json!([index, vi, v.point[0], v.point[1]])
                            } else {
                                json!([index, vi, r2(v.point[0]), r2(v.point[1]), r2(v.handle_in[0]), r2(v.handle_in[1]),
                                    r2(v.handle_out[0]), r2(v.handle_out[1]), u8::from(stroke.is_some_and(|s| s.closed))])
                            });
                        }
                        let limit = if part == "bound" { usize::MAX } else { mesh.map_or(0, |m| m.outline) };
                        for (vi, w) in mesh.iter().flat_map(|m| m.world.iter()).enumerate().take(limit) {
                            out.push(if part == "bound" {
                                json!([index, vi, w[0], w[1]])
                            } else {
                                json!([index, vi, r2(w[0]), r2(w[1]), 0, 0, 0, 0, 1])
                            });
                        }
                    }
                    out
                }
            }
            "weights" => {
                let entries = rig
                    .binds
                    .get(bind)
                    .and_then(|b| b.weights.get(vertex))
                    .ok_or(DispatchErrorCode::NotFound)?;
                entries
                    .iter()
                    .enumerate()
                    .map(|(entry, w)| {
                        json!({"entry":entry,"boneId":w.bone_id,"ptIndex":w.pt_index.value(),"offset":w.offset.value(),"w":w.w})
                    })
                    .collect()
            }
            _ => return Err(DispatchErrorCode::InvalidRequest),
        };
        Ok(Presented {
            unavailable: None,
            matrix,
            items,
        })
    }
}

fn state_name(state: &PoseBind) -> &'static str {
    match state {
        PoseBind::Posed => "posed",
        PoseBind::Skipped => "skipped",
        PoseBind::Unlinked => "unlinked",
        PoseBind::Unavailable => "unavailable",
    }
}

/// The centre of the tight bounds of the bone path (`path.bounds.center`), each curve's extremes included.
fn bounds_center(vertices: &[[[f64; 2]; 3]], closed: bool) -> Value {
    let mut low = [f64::INFINITY; 2];
    let mut high = [f64::NEG_INFINITY; 2];
    let mut take = |p: [f64; 2]| {
        for axis in 0..2 {
            low[axis] = low[axis].min(p[axis]);
            high[axis] = high[axis].max(p[axis]);
        }
    };
    let last = vertices.len().saturating_sub(1);
    let curves = if closed { vertices.len() } else { last };
    for n in 0..vertices.len() {
        take(vertices[n][0]);
    }
    for n in 0..curves {
        let (a, b) = (&vertices[n], &vertices[(n + 1) % vertices.len()]);
        let p = [
            a[0],
            [a[0][0] + a[2][0], a[0][1] + a[2][1]],
            [b[0][0] + b[1][0], b[0][1] + b[1][1]],
            b[0],
        ];
        for axis in 0..2 {
            let (p0, p1, p2, p3) = (p[0][axis], p[1][axis], p[2][axis], p[3][axis]);
            let (qa, qb, qc) = (
                -p0 + 3.0 * p1 - 3.0 * p2 + p3,
                2.0 * p0 - 4.0 * p1 + 2.0 * p2,
                -p0 + p1,
            );
            let mut roots = Vec::new();
            if qa.abs() < 1e-12 {
                if qb.abs() > 1e-12 {
                    roots.push(-qc / qb);
                }
            } else {
                let disc = qb * qb - 4.0 * qa * qc;
                if disc >= 0.0 {
                    let root = disc.sqrt();
                    roots.push((-qb + root) / (2.0 * qa));
                    roots.push((-qb - root) / (2.0 * qa));
                }
            }
            for t in roots.into_iter().filter(|t| *t > 0.0 && *t < 1.0) {
                let u = 1.0 - t;
                let value =
                    u * u * u * p0 + 3.0 * u * u * t * p1 + 3.0 * u * t * t * p2 + t * t * t * p3;
                low[axis] = low[axis].min(value);
                high[axis] = high[axis].max(value);
            }
        }
    }
    if vertices.is_empty() {
        return Value::Null;
    }
    json!([(low[0] + high[0]) / 2.0, (low[1] + high[1]) / 2.0])
}
