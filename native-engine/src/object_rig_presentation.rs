//! #2255: the on-canvas facts of one layer's rig at one frame, from one immutable snapshot, in bounded parts.
//!
//! The Rig canvas tool (`src/js/rig-bridge.js`) draws the bones with their handles and anchors, the influence circle of each bone
//! (Assigner), and hit-tests an anchor or handle (Déplacer), an influence-circle rim and a bound vertex (the weight popover). This
//! module returns exactly those facts, in the six parts a request names (`bones`, `vertices`, `chains`, `binds`, `bound`,
//! `weights`), each a flat list a page of which stays under the response byte limit. Nothing is written, cached or kept.
//!
//! The record is read as it is stored: the live pose `segments` (what `moveSegment` writes), never the rest pose. `bound` is the
//! posed vertex of every bind that a pose evaluation of the same snapshot reports as posed (a bind it cannot pose, a mesh among
//! them, lists nothing and says so in `binds`). Coordinates are document space: they are world space only while the layer and
//! every parent carry no motion (the baseline mapped them through the layer's motion; the pivot of a drawn layer is its content
//! bounds, which only the presented scene knows), so a layer with motion is a typed reason (`motion`) and no geometry, never a
//! guessed map. A control expression on a transform is `expression`, an unreadable one `sample`, a missing parent `parent`.
use crate::object_snapshot::object_rig_pose_read::{evaluate_layer_pose, PoseBind};
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
pub(super) const PARTS: [(&str, usize); 6] = [
    ("bones", 8),
    ("vertices", 16),
    ("chains", 8),
    ("binds", 8),
    ("bound", 64),
    ("weights", 8),
];

/// The answer: why the geometry is unavailable (`None` when it is not) and the whole flat list of the part.
pub(super) struct Presented {
    pub(super) unavailable: Option<&'static str>,
    pub(super) items: Vec<Value>,
}

impl ObjectSnapshot {
    /// The flat list of `part` for `layer_uid`; `bind` and `vertex` select the vertex of `weights` and are zero otherwise.
    pub(super) fn rig_presentation_items(
        &self,
        layer_uid: &str,
        part: &str,
        frame: u32,
        bind: usize,
        vertex: usize,
    ) -> Result<Presented, DispatchErrorCode> {
        let layer = self
            .document()
            .layers()
            .iter()
            .find(|layer| layer.layer_uid() == layer_uid)
            .ok_or(DispatchErrorCode::NotFound)?;
        if let Err(reason) = placement_is_identity(self, layer_uid, f64::from(frame)) {
            return Ok(Presented {
                unavailable: Some(reason),
                items: Vec::new(),
            });
        }
        let Some(rig) = layer.rig() else {
            return Ok(Presented {
                unavailable: None,
                items: Vec::new(),
            });
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
            "binds" | "bound" => {
                let pose = evaluate_layer_pose(self, layer_uid, frame).ok();
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
                    let mut out = Vec::new();
                    for (index, b) in rig.binds.iter().enumerate() {
                        let posed = pose.as_ref().filter(|p| p.binds.get(index) == Some(&PoseBind::Posed));
                        let stroke = b.stroke_id.value().and_then(|id| posed.and_then(|p| p.strokes.iter().find(|s| &s.stroke_id == id)));
                        for (vi, v) in stroke.iter().flat_map(|s| s.vertices.iter()).enumerate() {
                            out.push(json!([index, vi, v.point[0], v.point[1]]));
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

/// The layer and every parent carry no motion matrix at `frame`, so document space is world space.
#[cfg(feature = "evaluation")]
fn placement_is_identity(
    snapshot: &ObjectSnapshot,
    layer_uid: &str,
    frame: f64,
) -> Result<(), &'static str> {
    use crate::object_snapshot::widget_presentation::kernels::layer_mat;
    let layers = snapshot.document().layers();
    let mut seen = vec![layer_uid];
    let mut next = Some(layer_uid);
    while let Some(uid) = next.filter(|_| seen.len() <= 64) {
        if layer_mat(snapshot, uid, frame)?.is_some() {
            return Err("motion");
        }
        let layer = layers
            .iter()
            .find(|candidate| candidate.layer_uid() == uid)
            .ok_or("parent")?;
        next = layer.parent_layer_uid.value().and_then(Option::as_deref);
        if let Some(parent) = next {
            if seen.contains(&parent) {
                break;
            }
            seen.push(parent);
        }
    }
    Ok(())
}
#[cfg(not(feature = "evaluation"))]
fn placement_is_identity(_: &ObjectSnapshot, _: &str, _: f64) -> Result<(), &'static str> {
    Err("sample")
}
