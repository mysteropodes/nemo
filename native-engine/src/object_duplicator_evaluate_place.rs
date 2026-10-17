//! Placement, path and pool pieces of the duplicator evaluation (L04-L, I08/I09).
//!
//! The parts of `applyLayerDuplicator` (`src/js/app.js` at baseline `fbe4b8a3`)
//! that need the request and the ports: the eight `dupOffset*` reads, the path
//! of path mode, the Object-mode pool, the per-clone placement (mode offset,
//! seeded stagger, inline and layer effectors) and the Object-mode copy loop.
//! `object_duplicator_evaluate` sequences them in the baseline's order.

use crate::evaluation::object_component_kernels::duplicator_placement_apply::{
    apply_placement, Dup3d as AppliedDup3d, DuplicatorCopy, PlacementApply,
};
use crate::evaluation::object_component_kernels::duplicator_placement_clone::{
    clone_placement, CloneDeltas, ClonePlacementInput, RandMode,
};
use crate::evaluation::object_component_kernels::duplicator_placement_jsnum::truthy;
use crate::evaluation::object_component_kernels::duplicator_placement_matrix::{
    bounds_center, transform_geometry, Descriptor, Matrix, Pivot,
};
use crate::evaluation::object_component_kernels::duplicator_placement_modes::{
    clone_indices, mode_offset, resolve_path, DuplicatorSpec, JsNumber, ModeOffset,
    ModeOffsetError, PathInfo, PathSource, PathStroke, GRID_CAP,
};
use crate::evaluation::object_component_kernels::duplicator_placement_temporal::BuildError;
use crate::evaluation::object_component_kernels::object_duplicator_evaluate_types::{
    ClonePlacementTrace, EvaluateError, Ports, Request, KEY_OFFSET_HUE, KEY_OFFSET_OPACITY,
    KEY_OFFSET_POS, KEY_OFFSET_POS_Z, KEY_OFFSET_ROT, KEY_OFFSET_ROT_X, KEY_OFFSET_ROT_Y,
    KEY_OFFSET_SCALE,
};
use crate::evaluation::object_component_kernels::object_effector_falloff::{at, Effector};
use crate::evaluation::object_component_kernels::object_effector_layer::effective_effectors;
use crate::evaluation::object_component_kernels::object_effector_layers::{
    resolved_effectors, EffectorScope, MotionPort,
};
use crate::evaluation::property_follow_path_sample::SegmentIn;
use crate::object_document::ObjectRecord;
use serde_json::Number;

/// `valueAtFrame(host, key)` as an array; `null` is an error the baseline raises.
fn sample(
    port: &dyn MotionPort,
    uid: &str,
    key: &'static str,
) -> Result<Vec<JsNumber>, EvaluateError> {
    match port.value(uid, key) {
        Ok(Some(values)) => Ok(values),
        Ok(None) => Err(EvaluateError::NullValue(key)),
        Err(unavailable) => Err(EvaluateError::Unavailable(unavailable)),
    }
}

/// The eight `dupOffset*` reads of the baseline, in its order. `x[i]` in an
/// arithmetic position is the JavaScript operand (`undefined` NaN, `null` 0).
pub(crate) fn sample_deltas(
    port: &dyn MotionPort,
    uid: &str,
) -> Result<CloneDeltas, EvaluateError> {
    let pos = sample(port, uid, KEY_OFFSET_POS)?;
    let rot = sample(port, uid, KEY_OFFSET_ROT)?;
    let scale = sample(port, uid, KEY_OFFSET_SCALE)?;
    let opacity = sample(port, uid, KEY_OFFSET_OPACITY)?;
    let hue = sample(port, uid, KEY_OFFSET_HUE)?;
    let pos_z = sample(port, uid, KEY_OFFSET_POS_Z)?;
    let rot_x = sample(port, uid, KEY_OFFSET_ROT_X)?;
    let rot_y = sample(port, uid, KEY_OFFSET_ROT_Y)?;
    let first = |list: &[JsNumber]| at(list, 0).operand();
    Ok(CloneDeltas {
        pos: [at(&pos, 0).operand(), at(&pos, 1).operand()],
        rot: first(&rot),
        scale: [at(&scale, 0).operand(), at(&scale, 1).operand()],
        opacity: first(&opacity),
        hue: at(&hue, 0),
        pos_z: first(&pos_z),
        rot_x: first(&rot_x),
        rot_y: first(&rot_y),
    })
}

fn coordinate(n: &Number) -> f64 {
    n.as_f64().unwrap_or(f64::NAN)
}

/// The segments of a record as the path sampler reads them.
fn segments_of(record: &ObjectRecord) -> Vec<SegmentIn> {
    let pair = |p: &crate::object_document::ObjectPoint| [coordinate(&p.x), coordinate(&p.y)];
    record
        .geometry
        .segments
        .iter()
        .map(|s| SegmentIn {
            point: pair(&s.point),
            handle_in: Some(pair(&s.handle_in)),
            handle_out: Some(pair(&s.handle_out)),
        })
        .collect()
}

/// `_resolveDuplicatorPath`: `Ok(None)` is the baseline's `null`. The path
/// layer's content is only asked for when the layer exists and is not itself
/// a duplicator (`srcLd.duplicator` returns before the strokes are read).
pub(crate) fn path_info(
    req: &Request<'_>,
    ports: &Ports<'_>,
    path_uid: Option<&str>,
) -> Result<Option<PathInfo>, EvaluateError> {
    let uid = match path_uid {
        Some(uid) if ports.sources.has_layer(uid) => uid,
        _ => return Ok(None),
    };
    if req.set.duplicator(req.context_id, uid).is_some() {
        let source = PathSource {
            has_duplicator: true,
            strokes: &[],
        };
        return resolve_path(Some(&source)).map_err(EvaluateError::Path);
    }
    let records = ports.sources.strokes_at(uid, req.frame_idx);
    let segments: Vec<Vec<SegmentIn>> = records.iter().map(segments_of).collect();
    let strokes: Vec<PathStroke<'_>> = records
        .iter()
        .zip(&segments)
        .map(|(record, segs)| PathStroke {
            is_raster: false,
            segments: Some(segs.as_slice()),
            closed: record.geometry.closed,
        })
        .collect();
    let source = PathSource {
        has_duplicator: false,
        strokes: &strokes,
    };
    resolve_path(Some(&source)).map_err(EvaluateError::Path)
}

/// The Object-mode pool: the referenced layers that exist and are not
/// themselves duplicators, in list order (a repeated uid stays repeated).
pub(crate) fn pool_members<'a>(
    req: &Request<'_>,
    ports: &Ports<'_>,
    uids: &'a [String],
) -> Vec<&'a str> {
    uids.iter()
        .map(String::as_str)
        .filter(|uid| ports.sources.has_layer(uid))
        .filter(|uid| req.set.duplicator(req.context_id, uid).is_none())
        .collect()
}

/// A pool member moved so its bounds center sits on the host's pivot
/// (`dx0 || dy0` decides whether anything moves).
fn recentre(
    pooled: Vec<ObjectRecord>,
    pivot: Pivot,
    canvas: [f64; 2],
) -> Result<Vec<ObjectRecord>, EvaluateError> {
    if pooled.is_empty() {
        return Ok(pooled);
    }
    let center = bounds_center(&pooled, canvas[0], canvas[1]).map_err(EvaluateError::Pool)?;
    let (dx, dy) = (pivot.x - center.x, pivot.y - center.y);
    if !truthy(dx) && !truthy(dy) {
        return Ok(pooled);
    }
    let shift = Matrix {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        tx: dx,
        ty: dy,
    };
    let mut moved = Vec::with_capacity(pooled.len());
    for record in pooled {
        let geometry = transform_geometry(&shift, &record.geometry).map_err(EvaluateError::Pool)?;
        let mut copy = record;
        copy.geometry = geometry;
        moved.push(copy);
    }
    Ok(moved)
}

/// `_duplicatorModeOffset` + `_duplicatorClonePlacement` for each clone, the
/// shape the copy loop calls it in. The first failure is kept (a placement
/// closure cannot return it) and surfaced once the loop returns.
pub(crate) struct Placer<'a> {
    pub(crate) spec: DuplicatorSpec<'a>,
    pub(crate) seed: JsNumber,
    pub(crate) rand: RandMode,
    pub(crate) deltas: CloneDeltas,
    pub(crate) count: f64,
    pub(crate) cols: f64,
    pub(crate) is_3d_layer: bool,
    pub(crate) path: Option<PathInfo>,
    pub(crate) inline: Vec<Effector>,
    pub(crate) effectors: Option<Vec<Effector>>,
    pub(crate) scope: EffectorScope<'a>,
    pub(crate) host_uid: &'a str,
    pub(crate) motion: &'a dyn MotionPort,
    pub(crate) trace: Vec<ClonePlacementTrace>,
    pub(crate) failure: Option<EvaluateError>,
}

impl Placer<'_> {
    fn fail(&mut self, error: EvaluateError) {
        if self.failure.is_none() {
            self.failure = Some(error);
        }
    }

    /// The effector list of the clone loop: inline effectors first, then the
    /// layers aimed at the host, sampled when the first clone is placed.
    fn ensure_effectors(&mut self) {
        if self.effectors.is_some() {
            return;
        }
        let from_layers = resolved_effectors(&self.scope, self.host_uid, self.motion);
        match from_layers {
            Ok(list) => self.effectors = Some(effective_effectors(&self.inline, list)),
            Err(unavailable) => {
                self.fail(EvaluateError::Unavailable(unavailable));
                self.effectors = Some(Vec::new());
            }
        }
    }

    pub(crate) fn at(&mut self, k: u32, pivot: Pivot) -> PlacementApply {
        self.ensure_effectors();
        let offset = match mode_offset(&self.spec, k, self.count, self.cols, self.path.as_ref()) {
            Ok(offset) => offset,
            Err(ModeOffsetError::PathInfoRequired) => {
                self.fail(EvaluateError::PathInfoRequired);
                ModeOffset {
                    base_dx: 0.0,
                    base_dy: 0.0,
                    base_rot: 0.0,
                }
            }
        };
        let effectors: &[Effector] = self.effectors.as_deref().unwrap_or(&[]);
        let placed = clone_placement(&ClonePlacementInput {
            seed: self.seed,
            k,
            count: self.count,
            pivot: [pivot.x, pivot.y],
            base: [offset.base_dx, offset.base_dy, offset.base_rot],
            deltas: self.deltas,
            rand: self.rand,
            is_3d_layer: self.is_3d_layer,
            effectors,
        });
        let applied = PlacementApply {
            matrix: Descriptor {
                rot: placed.rot,
                sx: placed.sx,
                sy: placed.sy,
                dx: placed.dx,
                dy: placed.dy,
            },
            opacity_factor: placed.opacity_factor,
            hue_deg: placed.hue_deg,
            dup_3d: placed.dup_3d.map(|d| AppliedDup3d {
                dz: d.dz,
                drx: d.drx,
                dry: d.dry,
            }),
        };
        self.trace.push(ClonePlacementTrace {
            k,
            pivot,
            placement: applied,
        });
        applied
    }
}

/// The Object-mode copy loop: slot `k` pulls pool member `k % pool.len()`,
/// recentred on the host's pivot, and the placement moves it from there.
/// The temporal offset does not apply (`if (pool) .. else if (tOffOn)`).
pub(crate) fn pool_copies(
    req: &Request<'_>,
    ports: &Ports<'_>,
    pool: &[&str],
    seed: JsNumber,
    placer: &mut Placer<'_>,
    pivot: Pivot,
) -> Result<Vec<DuplicatorCopy>, EvaluateError> {
    let indices = clone_indices(placer.count);
    if indices > GRID_CAP as usize {
        return Err(EvaluateError::Build(BuildError::CountAboveCap));
    }
    let canvas = [req.canvas_w, req.canvas_h];
    let mut copies = Vec::new();
    for index in 0..indices {
        let k = index as u32;
        let member = pool[index % pool.len()];
        let pooled = ports.sources.strokes_at(member, req.frame_idx);
        let content = recentre(pooled, pivot, canvas)?;
        let placement = placer.at(k, pivot);
        for record in &content {
            let copy = apply_placement(record, k, seed, &placement, pivot)
                .map_err(|error| EvaluateError::Copy { index: k, error })?;
            copies.push(copy);
        }
    }
    Ok(copies)
}
