//! Native tween geometry kernels (#2358). Pure geometry; no document/history writer.
//! Baseline: geometry-wasm/src/tweenmatch.rs at 46442ea9. Matching admission belongs
//! to the frame planner, which excludes texture dabs before calling Hungarian.
use kurbo::{BezPath, PathSeg, Point, Rect, Shape};
use serde::{Deserialize, Serialize};

#[path = "tween_sampling.rs"]
mod sampling;
use sampling::*;
#[path = "tween_features.rs"]
mod features;
use features::*;
#[path = "tween_cost.rs"]
mod cost;
use cost::*;
#[path = "tween_assignment.rs"]
mod assignment;
use assignment::*;
#[path = "tween_matching.rs"]
mod matching;
use matching::*;
#[path = "tween_resample.rs"]
mod resample;
use resample::*;
#[path = "tween_align.rs"]
mod align;
use align::*;

#[cfg(test)]
#[path = "../tests/tween_kernels.rs"]
mod tests;

#[path = "tween_partition.rs"]
mod partition;
use partition::*;

#[path = "tween_relational.rs"]
mod relational;
use relational::*;

#[path = "tween_interpolation.rs"]
mod interpolation;
use interpolation::*;

#[path = "tween_pair.rs"]
mod pair;
use pair::*;

#[path = "tween_document_geometry.rs"]
mod document_geometry;
use document_geometry::*;

#[path = "tween_pair_plan.rs"]
mod pair_plan;
use pair_plan::*;

#[path = "tween_piece.rs"]
mod piece;
use piece::*;

#[path = "tween_frame_store.rs"]
mod frame_store;
use frame_store::*;
#[path = "tween_span_generate.rs"]
mod span_generate;
use span_generate::*;

#[path = "tween_document_plan.rs"]
mod document_plan;
pub(crate) use document_plan::{plan_tween, TweenRequest};

#[path = "tween_path_geometry.rs"]
mod path_geometry;
use path_geometry::*;
#[path = "tween_dab_shape.rs"]
mod dab_shape;
use dab_shape::*;

#[path = "tween_dabs.rs"]
mod dabs;
use dabs::*;

#[path = "tween_texture_emit.rs"]
mod texture_emit;
use texture_emit::*;
