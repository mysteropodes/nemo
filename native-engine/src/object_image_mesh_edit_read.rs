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
