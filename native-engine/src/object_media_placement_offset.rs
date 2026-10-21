//! One lossless placement offset value. Drawing/Video keeps the original integer wire.
use crate::object_document::assets::{valid_id, AssetError, AssetId};
use crate::object_document::{AssetInventory, AssetKind, AssetTarget};
use serde_json::Number;
// T12: the stored timing of a video placement.
#[path = "object_media_video_timing.rs"]
mod video_timing;
pub(crate) use video_timing::{RemapIntent, RemapRefusal, VideoTiming, MAX_INTERP_MODE};
const MAX: f64 = 9_007_199_254_740_991.;

pub(crate) fn admitted(target: &AssetTarget, value: &Number) -> bool {
    match target {
        AssetTarget::AudioTrack { .. } => value
            .as_f64()
            .is_some_and(|n| n.is_finite() && n.abs() <= MAX),
        _ => value
            .as_i64()
            .is_some_and(|n| n.unsigned_abs() <= MAX as u64),
    }
}
pub(crate) fn equal(a: &Number, b: &Number) -> bool {
    a.as_f64() == b.as_f64()
}
pub(crate) fn shifted(target: &AssetTarget, before: &Number, delta: &Number) -> Option<Number> {
    if !admitted(target, before) || !admitted(target, delta) {
        return None;
    }
    // Preserve old checked integer math and integer serialization on existing ports.
    if let (Some(a), Some(b)) = (before.as_i64(), delta.as_i64()) {
        let n = Number::from(a.checked_add(b)?);
        return admitted(target, &n).then_some(n);
    }
    let n = Number::from_f64(before.as_f64()? + delta.as_f64()?)?;
    admitted(target, &n).then_some(n)
}
#[derive(Debug, Clone)]
pub(crate) struct AssetPlacement {
    pub(super) id: String,
    pub(super) asset_id: AssetId,
    pub(super) target: AssetTarget,
    pub(super) offset_frames: serde_json::Number,
    pub(super) timing: VideoTiming,
}
impl Eq for AssetPlacement {}
impl PartialEq for AssetPlacement {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.asset_id == other.asset_id
            && self.target == other.target
            && self::equal(&self.offset_frames, &other.offset_frames)
            && self.timing == other.timing
    }
}
impl AssetPlacement {
    pub(crate) fn new(
        id: String,
        asset_id: AssetId,
        target: AssetTarget,
        offset_frames: i64,
    ) -> Result<Self, AssetError> {
        Self::new_number(
            id,
            asset_id,
            target,
            serde_json::Number::from(offset_frames),
        )
    }
    pub(crate) fn new_number(
        id: String,
        asset_id: AssetId,
        target: AssetTarget,
        offset_frames: serde_json::Number,
    ) -> Result<Self, AssetError> {
        if !valid_id(&id) || !target.valid() || !self::admitted(&target, &offset_frames) {
            return Err(AssetError::Placement);
        }
        Ok(Self {
            id,
            asset_id,
            target,
            offset_frames,
            timing: VideoTiming::default(),
        })
    }
    /// The same placement with `timing`: only a drawing target carries it.
    pub(crate) fn with_timing(mut self, timing: VideoTiming) -> Result<Self, AssetError> {
        if !timing.is_empty() && !matches!(self.target, AssetTarget::Drawing { .. }) {
            return Err(AssetError::Placement);
        }
        if !timing.admitted() {
            return Err(AssetError::Placement);
        }
        self.timing = timing;
        Ok(self)
    }
    /// Whether this placement sits on the drawing layer `layer_uid` of `context_id`.
    pub(crate) fn on_drawing_layer(&self, context_id: &str, layer_uid: &str) -> bool {
        matches!(&self.target, AssetTarget::Drawing { context_id: c, layer_uid: l, .. }
            if c == context_id && l == layer_uid)
    }
    pub(crate) fn timing(&self) -> &VideoTiming {
        &self.timing
    }
    pub(crate) fn id(&self) -> &str {
        &self.id
    }
    pub(crate) fn asset_id(&self) -> &AssetId {
        &self.asset_id
    }
    pub(crate) fn target(&self) -> &AssetTarget {
        &self.target
    }
    pub(crate) fn offset_frames(&self) -> &serde_json::Number {
        &self.offset_frames
    }
}

#[cfg(test)]
#[path = "../tests/object_media_placement_offset.rs"]
mod tests;

impl AssetInventory {
    /// T12: the video placement of one drawing layer, the pinned placement read's
    /// lookup (read queries name no document type).
    pub(crate) fn video_placement_on(
        &self,
        context_id: &str,
        layer_uid: &str,
    ) -> Option<&AssetPlacement> {
        self.placements().iter().find(|placement| {
            placement.on_drawing_layer(context_id, layer_uid)
                && self
                    .get(placement.asset_id())
                    .is_some_and(|asset| asset.kind() == AssetKind::Video)
        })
    }
}
