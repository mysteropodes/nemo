//! Baseline tween admission: render companions and frozen ghosts never enter matching.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TweenPartition {
    pub list: Vec<Value>,
    pub orig: Vec<usize>,
    pub dabs_by_group: BTreeMap<String, Vec<Value>>,
    pub held: Vec<Value>,
    pub backdrops_by_id: BTreeMap<String, Value>,
}

pub(crate) fn split_tweenables(strokes: &[Value], manual: bool) -> TweenPartition {
    let mut out = TweenPartition::default();
    for (i, stroke) in strokes.iter().enumerate() {
        if stroke["isBrushTextureCopy"] == true {
            if let Some(group) = stroke["brushGroupId"].as_str().filter(|s| !s.is_empty()) {
                out.dabs_by_group
                    .entry(group.to_owned())
                    .or_default()
                    .push(stroke.clone());
            }
        } else if stroke["isLinkedFillCompanion"] == true {
            if let Some(id) = stroke["linkedFillId"].as_str().filter(|s| !s.is_empty()) {
                out.backdrops_by_id.insert(id.to_owned(), stroke.clone());
            }
        } else if stroke["isRevisionGhost"] == true {
            // The frozen review ghost belongs only to its original keyframe.
        } else if stroke["isRaster"] == true || (manual && stroke["tweenOn"] != true) {
            out.held.push(stroke.clone());
        } else {
            out.list.push(stroke.clone());
            out.orig.push(i);
        }
    }
    out
}

/// Exact JavaScript mulberry32 (`seededRng`), including wrapping 32-bit multiplication.
#[derive(Clone, Copy)]
pub(crate) struct TweenRng(u32);
impl TweenRng {
    pub(crate) fn new(seed: u32) -> Self {
        Self(seed)
    }
    pub(crate) fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x6D2B79F5);
        let mut r = (self.0 ^ (self.0 >> 15)).wrapping_mul(1 | self.0);
        r = r.wrapping_add((r ^ (r >> 7)).wrapping_mul(61 | r)) ^ r;
        f64::from(r ^ (r >> 14)) / 4294967296.0
    }
}
