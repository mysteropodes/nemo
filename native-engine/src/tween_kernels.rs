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

#[path = "tween_pressure_outline.rs"]
mod pressure_outline;
use pressure_outline::*;
// Trim Stroke Ends rebuilds a trimmed ribbon with the same outline kernel (object_vector_trim.rs).
pub(crate) use pressure_outline::{profiled_outline, width_at_frac};
pub(crate) use resample::SegOut;

#[path = "tween_outline_cleanup.rs"]
mod outline_cleanup;
use outline_cleanup::*;

#[path = "tween_split_plan.rs"]
mod split_plan;
use split_plan::*;

#[path = "tween_pressure_record.rs"]
mod pressure_record;
use pressure_record::*;

#[path = "tween_backdrop.rs"]
mod backdrop;
use backdrop::*;

#[path = "tween_held_items.rs"]
mod held_items;
use held_items::*;

#[path = "tween_bitmap_plan.rs"]
mod bitmap_plan;
pub(crate) use crate::object_snapshot::tween_resources::{
    prepare_tween_resources, PreparedTweenResources,
};
use bitmap_plan::*;

#[path = "tween_limits.rs"]
pub(crate) mod limits;

#[cfg(all(test, feature = "project_media_codec"))]
pub(crate) use crate::object_snapshot::tween_resources::tests::assert_second_bake_rollback;
