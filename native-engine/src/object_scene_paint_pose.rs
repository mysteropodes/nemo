//! Paint-side companion of the whole-layer pose (pure, I/O free).
//!
//! The common vector batch moves the geometry through the layer's own Motion entry and its parent entries. The
//! baseline (`buildSceneJson`, src/js/engine-bridge.js) moves two more things through the same chain, and this value
//! carries exactly those to the typed item style:
//! * `strokeScale`: the product over the chain of `(|sx| + |sy|) / 2`, multiplied into the stroke width only
//!   (`(c.strokeWidth || 1) * strokeScale`); the dash pattern is never scaled on that route;
//! * `transformedGradientPoint`: each gradient endpoint goes through the same `transformSegments` chain as a path point.
//!
//! A pose that is not `carried` (no batch ran for the record) leaves the old raw-Position-only paint untouched.
use crate::evaluation::motion_parent_chain::{transform_segments, ChainEntry, Segment};

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PaintPose {
    carried: bool,
    stroke_scale: f64,
    /// The layer's own entry, absent when its Motion is the identity.
    layer: Option<ChainEntry>,
    /// Its ancestors, immediate parent first; identity entries are absent.
    parents: Vec<ChainEntry>,
}

impl Default for PaintPose {
    fn default() -> Self {
        Self {
            carried: false,
            stroke_scale: 1.0,
            layer: None,
            parents: Vec::new(),
        }
    }
}

fn mean_scale(entry: &ChainEntry) -> f64 {
    (entry.mat.sx.abs() + entry.mat.sy.abs()) / 2.0
}

impl PaintPose {
    pub(crate) fn from_chain(layer: Option<ChainEntry>, parents: &[ChainEntry]) -> Self {
        let stroke_scale = layer
            .iter()
            .chain(parents)
            .fold(1.0, |scale, entry| scale * mean_scale(entry));
        Self {
            carried: true,
            stroke_scale,
            layer,
            parents: parents.to_vec(),
        }
    }

    pub(crate) fn carried(&self) -> bool {
        self.carried
    }

    pub(crate) fn stroke_scale(&self) -> f64 {
        self.stroke_scale
    }

    /// True when a gradient endpoint needs the chain: translation alone is still the raw Position offset.
    pub(crate) fn moves_gradient(&self) -> bool {
        self.carried
            && (!self.parents.is_empty()
                || self
                    .layer
                    .is_some_and(|e| e.mat.rot != 0.0 || e.mat.sx != 1.0 || e.mat.sy != 1.0))
    }

    /// `transformedGradientPoint`: the endpoint through the layer entry, then each ancestor.
    pub(crate) fn gradient_point(&self, point: [f64; 2]) -> [f64; 2] {
        let mut one = [Segment {
            point,
            handle_in: None,
            handle_out: None,
        }];
        for entry in self.layer.iter().chain(&self.parents) {
            one = [transform_segments(&one, entry.pivot, &entry.mat)[0]];
        }
        one[0].point
    }
}

#[cfg(test)]
#[path = "../tests/object_scene_paint_pose.rs"]
mod tests;
