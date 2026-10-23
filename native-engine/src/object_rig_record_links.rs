//! Link report of the native rig record (I11): the typed replacement for the baseline's silent
//! `relinkRigBinds` drop (`src/js/app.js`).
//!
//! `relinkRigBinds` runs after every `loadFrame` and FILTERS the binds: a stroke bind survives only
//! when a child with that `strokeId` exists and its vertex count equals `rest.length`; a mesh bind
//! (tested first whenever `meshId` is truthy) only when a raster carrying the mesh exists, the mesh is
//! in the store and its vertex count equals `rest.length`. Anything else is dropped without a trace
//! ("in doubt, drop"). The native record is never filtered or rewritten: [`bind_link`] reports the
//! same verdicts as data, with the reason, and the evaluation join decides what to do with an
//! unlinked bind.
//!
//! Resolution scope: this root-document provider resolves SCENE_CONTEXT authored records of the SAME layer, the
//! stable stroke identity and the frame the caller names (the baseline links against the current
//! frame's children); held-frame resolution through layer timing, component contexts and the raster
//! that carries a mesh are joins of later leaves (L03-P/L03-R). The mesh store is a port: `None`
//! means "no mesh provider is wired" and is reported as unavailable, never as success or as missing.
use crate::object_document::object_component_record_kernels::object_component_records::SCENE_CONTEXT;
use crate::object_document::object_rig_record::{BindTarget, RigBind, RigRecord, PATH_FAMILIES};
use crate::object_document::{FrameScopeKind, ObjectDocument, ObjectRecord};

/// `meshId -> vertex count` of the image-mesh store, `None` while no store provider is wired.
pub(crate) type MeshVertexLookup<'a> = Option<&'a dyn Fn(&str) -> Option<usize>>;

/// Whether one bind still matches the thing it drives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BindLink {
    /// The target exists and has exactly `rest.len()` vertices (the baseline would keep the bind).
    Linked { vertex_count: usize },
    /// The bind names no usable target (admission refuses this; reported for hand-built records).
    TargetInvalid,
    /// No authored object of the layer has this stroke id at the frame.
    StrokeMissing,
    /// The stroke's vertex count no longer equals `rest.len()` (sculpted, erased or boolean'd since).
    StrokeVertexCount { stroke: usize, rest: usize },
    /// The stroke record is of an object family the rig does not bind (baseline: CompoundPath).
    StrokeFamilyUnsupported,
    /// The mesh id is not in the store.
    MeshMissing,
    /// The mesh was rebuilt at another density.
    MeshVertexCount { mesh: usize, rest: usize },
    /// No mesh store provider is wired: neither linked nor missing.
    MeshStoreUnavailable,
}

/// The link verdict of `bind` on `layer_uid` at `frame`.
pub(crate) fn bind_link(
    bind: &RigBind,
    layer_uid: &str,
    frame: u32,
    objects: &[ObjectRecord],
    meshes: MeshVertexLookup<'_>,
) -> BindLink {
    let rest = bind.rest.len();
    match bind.target() {
        None => BindLink::TargetInvalid,
        Some(BindTarget::Stroke(stroke_id)) => {
            let record = objects.iter().find(|record| {
                let target = record.target();
                target.context_id() == SCENE_CONTEXT
                    && target.layer_uid() == layer_uid
                    && target.stroke_id() == stroke_id
                    && target.frame() == frame
                    && *target.scope_kind() == FrameScopeKind::Authored
            });
            match record {
                None => BindLink::StrokeMissing,
                Some(record) if !PATH_FAMILIES.contains(&record.family.as_str()) => {
                    BindLink::StrokeFamilyUnsupported
                }
                Some(record) => {
                    let stroke = record.geometry.segments.len();
                    if stroke == rest {
                        BindLink::Linked {
                            vertex_count: stroke,
                        }
                    } else {
                        BindLink::StrokeVertexCount { stroke, rest }
                    }
                }
            }
        }
        Some(BindTarget::Mesh(mesh_id)) => match meshes {
            None => BindLink::MeshStoreUnavailable,
            Some(lookup) => match lookup(mesh_id) {
                None => BindLink::MeshMissing,
                Some(mesh) if mesh == rest => BindLink::Linked { vertex_count: mesh },
                Some(mesh) => BindLink::MeshVertexCount { mesh, rest },
            },
        },
    }
}

/// What is wrong with a weight entry's bone reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WeightRefProblem {
    /// `boneId` names no bone (the baseline weigher and `anchorFor` skip such an entry).
    NoSuchBone,
    /// `ptIndex` is beyond the bone's rest or live vertices (`anchorFor` returns null).
    PointOutOfRange,
}

/// One unresolved weight entry: bind, vertex slot and entry position, in record order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WeightRefIssue {
    pub(crate) bind: usize,
    pub(crate) vertex: usize,
    pub(crate) entry: usize,
    pub(crate) problem: WeightRefProblem,
}

/// Every weight entry whose bone or control point does not exist. The record keeps them as authored.
pub(crate) fn weight_ref_issues(rig: &RigRecord) -> Vec<WeightRefIssue> {
    let mut issues = Vec::new();
    for (bind_index, bind) in rig.binds.iter().enumerate() {
        for (vertex, entries) in bind.weights.iter().enumerate() {
            for (entry_index, entry) in entries.iter().enumerate() {
                let problem = match rig.bone(&entry.bone_id) {
                    None => Some(WeightRefProblem::NoSuchBone),
                    Some(bone) => match entry.pt_index.value() {
                        Some(point) => {
                            let point = *point as usize;
                            if point >= bone.rest_segments.len() || point >= bone.segments.len() {
                                Some(WeightRefProblem::PointOutOfRange)
                            } else {
                                None
                            }
                        }
                        None => None,
                    },
                };
                if let Some(problem) = problem {
                    issues.push(WeightRefIssue {
                        bind: bind_index,
                        vertex,
                        entry: entry_index,
                        problem,
                    });
                }
            }
        }
    }
    issues
}

impl ObjectDocument {
    /// Link verdicts of every bind of the rig on `layer_uid` at `frame`, in bind order; `None` when the
    /// layer is missing or carries no rig.
    pub(crate) fn rig_links(
        &self,
        layer_uid: &str,
        frame: u32,
        meshes: MeshVertexLookup<'_>,
    ) -> Option<Vec<BindLink>> {
        let layer = self.layers.iter().find(|l| l.layer_uid == layer_uid)?;
        let rig = layer.rig()?;
        Some(
            rig.binds
                .iter()
                .map(|bind| bind_link(bind, layer_uid, frame, &self.objects, meshes))
                .collect(),
        )
    }
}
