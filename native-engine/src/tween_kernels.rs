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
