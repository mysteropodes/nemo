//! Read-queries facade for the image-mesh retopology kernel (F17/F18); no state, no publication
//! authority.
//!
//! The command layer may not depend on the evaluation layer (`commands` allows `read-queries`,
//! `read-queries` allows `evaluation`), so the mesh edit transaction reaches the topology kernel
//! (`setOutline` of `src/js/image-mesh.js` at fbe4b8a3: densify the outline, rebuild the grid,
//! Delaunay triangulate inside the outline, carry the sculpt with `resampleOffsets`) through this
//! one function, exactly like `ObjectSnapshot::sample_property_raw` reaches the raw sampler.
//! Without the `evaluation` feature it reports `KernelUnavailable`; nothing is defaulted, guessed
//! or approximated.
//!
//! Plain numbers in, plain numbers out: the stored record type belongs to the document layer and is
//! converted by the caller. The input is borrowed and never changed, the result is a new owned
//! value, so the caller publishes it (or drops it) atomically.
//!
//! The density control of the baseline editor calls `setOutline` with the CURRENT outline and the new
//! density (`image-mesh-bridge.js` `setDensity`); [`Retopology::Density`] is exactly that call.
//!
//! Binding transactions use the plain member projections below to reach the existing binding
//! kernel. They carry actual raster records, a mesh store and the caller's lifetime sequence;
//! no document, snapshot, command owner or publication authority enters this facade.
use crate::object_codec::{ImageMesh, ImageMeshStore, MeshIdSequence, RasterItem};

/// One mesh in normalized `(0,0)..(1,1)` image space. `verts[0..outline.len())` ARE the outline;
/// `offsets` is a separate array of the same length as `verts`; `cols`/`rows` are the integer grid
/// density a record can hold (`1..=64`).
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(not(feature = "evaluation"), allow(dead_code))]
pub(crate) struct MeshState {
    pub(crate) outline: Vec<[f64; 2]>,
    pub(crate) verts: Vec<[f64; 2]>,
    pub(crate) tris: Vec<u32>,
    pub(crate) offsets: Vec<[f64; 2]>,
    pub(crate) cols: u32,
    pub(crate) rows: u32,
}

/// What to rebuild the mesh from.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(not(feature = "evaluation"), allow(dead_code))]
pub(crate) enum Retopology {
    /// A new closed outline in normalized space, with an optional new density (`setOutline`).
    Outline {
        poly: Vec<[f64; 2]>,
        cols: Option<u32>,
        rows: Option<u32>,
    },
    /// The stored outline under a new density (`setOutline(outline, {cols, rows})`).
    Density { cols: u32, rows: u32 },
}

/// Why the mesh could not be rebuilt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(feature = "evaluation"), allow(dead_code))]
pub(crate) enum RetopologyRefusal {
    /// The topology kernel is not part of this build.
    KernelUnavailable,
    /// Fewer than three outline points (the baseline returns `null`).
    TooFewPoints,
    /// The outline densify step could never finish.
    Densify,
    /// The Delaunay port refused the point set.
    Triangulation,
    /// The resulting density is not an integer in `1..=64`.
    DensityNotRecordable,
}

/// `setOutline` over a detached copy of `state`.
#[cfg(feature = "evaluation")]
pub(crate) fn retopologize(
    state: &MeshState,
    change: &Retopology,
) -> Result<MeshState, RetopologyRefusal> {
    use crate::evaluation::object_image_mesh_kernels::object_image_mesh_topology::{
        set_outline, DensityOpts, MeshTopology, TopologyError,
    };
    let topology = MeshTopology {
        outline: state.outline.clone(),
        cols: f64::from(state.cols),
        rows: f64::from(state.rows),
        verts: state.verts.clone(),
        tris: state.tris.clone(),
        offsets: state.offsets.clone(),
    };
    let (poly, opts) = match change {
        Retopology::Outline { poly, cols, rows } => (
            poly.as_slice(),
            DensityOpts {
                cols: (*cols).map(f64::from),
                rows: (*rows).map(f64::from),
            },
        ),
        Retopology::Density { cols, rows } => (
            state.outline.as_slice(),
            DensityOpts {
                cols: Some(f64::from(*cols)),
                rows: Some(f64::from(*rows)),
            },
        ),
    };
    let next = set_outline(&topology, poly, opts)
        .map_err(|error| match error {
            TopologyError::Densify(_) => RetopologyRefusal::Densify,
            TopologyError::Triangulation(_) => RetopologyRefusal::Triangulation,
            TopologyError::DensityNotRecordable => RetopologyRefusal::DensityNotRecordable,
        })?
        .ok_or(RetopologyRefusal::TooFewPoints)?;
    let (cols, rows) = next
        .record_density()
        .map_err(|_| RetopologyRefusal::DensityNotRecordable)?;
    Ok(MeshState {
        outline: next.outline,
        verts: next.verts,
        tris: next.tris,
        offsets: next.offsets,
        cols,
        rows,
    })
}

/// Without the evaluation layer there is no kernel: a typed refusal, never a guess.
#[cfg(not(feature = "evaluation"))]
pub(crate) fn retopologize(
    state: &MeshState,
    change: &Retopology,
) -> Result<MeshState, RetopologyRefusal> {
    let _ = (state, change);
    Err(RetopologyRefusal::KernelUnavailable)
}

/// An actual raster or an occupied nonraster position; no fabricated item stands for a path/text.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum BindingSlot {
    Raster(RasterItem),
    NonRaster,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct BindingLayer {
    pub(crate) frames: Vec<Vec<BindingSlot>>,
}

/// Detached member values. The sequence is never initialized from, or replaced by, the store.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct BindingState {
    pub(crate) store: ImageMeshStore,
    pub(crate) sequence: MeshIdSequence,
    pub(crate) sets: Vec<Vec<BindingLayer>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BindingTarget {
    pub(crate) set: usize,
    pub(crate) layer: usize,
    pub(crate) frame: usize,
    pub(crate) slot: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum BindingOperation {
    Attach {
        target: BindingTarget,
        mesh: ImageMesh,
    },
    Detach {
        target: BindingTarget,
    },
    Propagate {
        target: BindingTarget,
        mesh_id: Option<String>,
    },
    ReleaseIfUnused {
        mesh_id: String,
    },
    DuplicateLayer {
        set: usize,
        layer: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BindingReport {
    Attach {
        mesh_id: String,
        replaced: Option<String>,
        written: usize,
        skipped: usize,
    },
    Detach {
        mesh_id: Option<String>,
        released: bool,
        written: usize,
        skipped: usize,
    },
    Propagate {
        written: usize,
        skipped: usize,
    },
    Released(bool),
    Duplicated {
        pairs: Vec<(String, String)>,
        dangling: Vec<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BindingRefusal {
    Unavailable,
    UnknownTarget,
    InvalidInput,
}

impl BindingState {
    /// Read the returned member projection without cloning the entire store/layer set for each
    /// raster writeback. This accessor performs no binding operation or document evaluation.
    #[cfg(feature = "evaluation")]
    pub(crate) fn raster_at(&self, target: BindingTarget) -> Result<&RasterItem, BindingRefusal> {
        match self
            .sets
            .get(target.set)
            .and_then(|set| set.get(target.layer))
            .and_then(|layer| layer.frames.get(target.frame))
            .and_then(|frame| frame.get(target.slot))
            .ok_or(BindingRefusal::UnknownTarget)?
        {
            BindingSlot::Raster(item) => Ok(item),
            BindingSlot::NonRaster => Err(BindingRefusal::InvalidInput),
        }
    }

    #[cfg(not(feature = "evaluation"))]
    pub(crate) fn raster_at(&self, target: BindingTarget) -> Result<&RasterItem, BindingRefusal> {
        let _ = target;
        Err(BindingRefusal::Unavailable)
    }
}

#[cfg(feature = "evaluation")]
mod binding_adapter {
    use super::*;
    use crate::evaluation::object_image_mesh_kernels::object_image_mesh_binding as kernel;

    impl From<BindingTarget> for kernel::SlotTarget {
        fn from(target: BindingTarget) -> Self {
            Self {
                set: target.set,
                layer: target.layer,
                frame: target.frame,
                slot: target.slot,
            }
        }
    }

    impl From<BindingState> for kernel::MeshBindingState {
        fn from(state: BindingState) -> Self {
            Self {
                store: state.store,
                sequence: state.sequence,
                sets: state
                    .sets
                    .into_iter()
                    .map(|set| {
                        set.into_iter()
                            .map(|layer| {
                                kernel::BindingLayer {
                                    frames: layer
                                        .frames
                                        .into_iter()
                                        .map(|frame| {
                                            frame
                                                .into_iter()
                                                .map(|slot| match slot {
                                                    BindingSlot::Raster(item) => {
                                                        kernel::BindingSlot::Raster(item)
                                                    }
                                                    BindingSlot::NonRaster => {
                                                        kernel::BindingSlot::NonRaster
                                                    }
                                                })
                                                .collect()
                                        })
                                        .collect(),
                                    // No native video record is provided by this port. Its commands remain
                                    // typed unavailable rather than inventing a video reference provider.
                                    native_video: false,
                                    video_mesh_id: None,
                                }
                            })
                            .collect()
                    })
                    .collect(),
            }
        }
    }

    impl From<kernel::MeshBindingState> for BindingState {
        fn from(state: kernel::MeshBindingState) -> Self {
            Self {
                store: state.store,
                sequence: state.sequence,
                sets: state
                    .sets
                    .into_iter()
                    .map(|set| {
                        set.into_iter()
                            .map(|layer| BindingLayer {
                                frames: layer
                                    .frames
                                    .into_iter()
                                    .map(|frame| {
                                        frame
                                            .into_iter()
                                            .map(|slot| match slot {
                                                kernel::BindingSlot::Raster(item) => {
                                                    BindingSlot::Raster(item)
                                                }
                                                kernel::BindingSlot::NonRaster => {
                                                    BindingSlot::NonRaster
                                                }
                                            })
                                            .collect()
                                    })
                                    .collect(),
                            })
                            .collect()
                    })
                    .collect(),
            }
        }
    }

    fn refusal(error: kernel::BindingError) -> BindingRefusal {
        match error {
            kernel::BindingError::UnknownLayer | kernel::BindingError::UnknownSlot => {
                BindingRefusal::UnknownTarget
            }
            kernel::BindingError::VideoLayerUnavailable => BindingRefusal::Unavailable,
            kernel::BindingError::TargetNotRaster
            | kernel::BindingError::Item(_)
            | kernel::BindingError::Mesh(_) => BindingRefusal::InvalidInput,
        }
    }

    pub(super) fn apply(
        state: BindingState,
        operation: BindingOperation,
    ) -> Result<(BindingState, BindingReport), BindingRefusal> {
        let mut state = kernel::MeshBindingState::from(state);
        let report = match operation {
            BindingOperation::Attach { target, mesh } => {
                let report = state.attach(target.into(), mesh).map_err(refusal)?;
                BindingReport::Attach {
                    mesh_id: report.mesh_id,
                    replaced: report.replaced,
                    written: report.propagate.written,
                    skipped: report.propagate.skipped,
                }
            }
            BindingOperation::Detach { target } => {
                let report = state.detach(target.into()).map_err(refusal)?;
                BindingReport::Detach {
                    mesh_id: report.mesh_id,
                    released: report.released,
                    written: report.propagate.written,
                    skipped: report.propagate.skipped,
                }
            }
            BindingOperation::Propagate { target, mesh_id } => {
                let report = state
                    .propagate(target.into(), mesh_id.as_deref())
                    .map_err(refusal)?;
                BindingReport::Propagate {
                    written: report.written,
                    skipped: report.skipped,
                }
            }
            BindingOperation::ReleaseIfUnused { mesh_id } => {
                BindingReport::Released(state.release_if_unused(&mesh_id).map_err(refusal)?)
            }
            BindingOperation::DuplicateLayer { set, layer } => {
                let report = state.remap_duplicated_layer(set, layer).map_err(refusal)?;
                BindingReport::Duplicated {
                    pairs: report.pairs,
                    dangling: report.dangling,
                }
            }
        };
        Ok((state.into(), report))
    }
}

/// Run the existing all-or-nothing kernel on detached member values. No caller sequence is
/// reconstructed, no raster is replaced by a placeholder, and an error returns no proposal.
#[cfg(feature = "evaluation")]
pub(crate) fn apply_binding(
    state: BindingState,
    operation: BindingOperation,
) -> Result<(BindingState, BindingReport), BindingRefusal> {
    binding_adapter::apply(state, operation)
}

#[cfg(not(feature = "evaluation"))]
pub(crate) fn apply_binding(
    state: BindingState,
    operation: BindingOperation,
) -> Result<(BindingState, BindingReport), BindingRefusal> {
    let _ = (state, operation);
    Err(BindingRefusal::Unavailable)
}
