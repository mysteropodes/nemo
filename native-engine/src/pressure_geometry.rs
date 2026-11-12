//! Pure pressure ribbon geometry shared by Subselect, Draw/Trim and tween commands.
//! No document mutation, command admission or publication authority.
use kurbo::{BezPath, PathSeg, Rect, Shape};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Error {
    InvalidRequest,
    Unavailable,
}
type R<T> = Result<T, Error>;

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SegOut {
    pub point: [f64; 2],
    pub handle_in: [f64; 2],
    pub handle_out: [f64; 2],
}

#[path = "pressure_path_sampling.rs"]
mod sampling;
pub(crate) use sampling::{add_segs, PathSampler};
#[path = "tween_path_geometry.rs"]
mod path_geometry;
pub(crate) use path_geometry::*;
#[path = "tween_outline_cleanup.rs"]
mod outline_cleanup;
use outline_cleanup::*;
#[path = "pressure_arc.rs"]
mod arc;
#[path = "pressure_boolean.rs"]
mod boolean;
#[path = "pressure_contours.rs"]
mod contours;
#[path = "pressure_graph.rs"]
mod graph;
#[path = "tween_pressure_outline.rs"]
mod outline;
pub(crate) use outline::{pressure_outline, profiled_outline, width_at_frac};

#[cfg(test)]
#[path = "../tests/pressure_geometry.rs"]
mod tests;
