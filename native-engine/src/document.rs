//! Values admitted by the bounded native opacity document format.
//!
//! This is not Nemo's legacy version-13 project schema. The legacy adapter
//! remains JavaScript-owned until N20 and must explicitly project supported
//! opacity data into this vector-free format.

use serde::{Deserialize, Serialize};
use serde_json::Number;

pub const OPACITY_DOCUMENT_FORMAT: &str = "nemo.native-opacity-document";
pub const OPACITY_DOCUMENT_FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpacityDocument {
    pub(crate) format: String,
    #[serde(rename = "formatVersion")]
    pub(crate) format_version: u32,
    #[serde(rename = "totalFrames")]
    pub(crate) total_frames: u32,
    pub(crate) layers: Vec<OpacityLayer>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpacityLayer {
    #[serde(rename = "layerUid")]
    pub(crate) layer_uid: String,
    #[serde(
        rename = "motionStatic",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) motion_static: Option<StaticMotion>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) motion: Option<AnimatedMotion>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StaticMotion {
    pub(crate) opacity: [Number; 1],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnimatedMotion {
    pub(crate) opacity: OpacityTrack,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpacityTrack {
    pub(crate) keys: Vec<OpacityKey>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpacityKey {
    pub(crate) frame: u32,
    pub(crate) v: [Number; 1],
    #[serde(rename = "curvePoints")]
    pub(crate) curve_points: Vec<CurvePoint>,
    #[serde(rename = "hOut")]
    pub(crate) h_out: [Number; 2],
    #[serde(rename = "hIn")]
    pub(crate) h_in: [Number; 2],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurvePoint {
    pub(crate) x: Number,
    pub(crate) y: Number,
}

impl OpacityDocument {
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    pub fn total_frames(&self) -> u32 {
        self.total_frames
    }

    pub fn layers(&self) -> &[OpacityLayer] {
        &self.layers
    }
}

impl OpacityLayer {
    pub fn layer_uid(&self) -> &str {
        &self.layer_uid
    }

    pub fn static_opacity(&self) -> Option<&Number> {
        self.motion_static.as_ref().map(|motion| &motion.opacity[0])
    }

    pub fn opacity_keys(&self) -> Option<&[OpacityKey]> {
        self.motion
            .as_ref()
            .map(|motion| motion.opacity.keys.as_slice())
    }
}

impl OpacityKey {
    pub fn frame(&self) -> u32 {
        self.frame
    }

    pub fn value(&self) -> &Number {
        &self.v[0]
    }

    pub fn curve_points(&self) -> &[CurvePoint] {
        &self.curve_points
    }

    pub fn handles(&self) -> (&[Number; 2], &[Number; 2]) {
        (&self.h_out, &self.h_in)
    }
}

impl CurvePoint {
    pub fn coordinates(&self) -> (&Number, &Number) {
        (&self.x, &self.y)
    }
}
