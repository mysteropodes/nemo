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
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "authored_number"
    )]
    pub(crate) tx: Option<Number>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "authored_number"
    )]
    pub(crate) ty: Option<Number>,
}

// Missing means automatic; an explicitly present tangent must be a number,
// including zero. In particular, null must not silently become an absent field.
fn authored_number<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Number>, D::Error> {
    Number::deserialize(deserializer).map(Some)
}

/// Shared structural admission for stored curves, independent of evaluation.
/// Short arrays retain the kernel's passthrough semantics and authored values.
pub(crate) fn validate_curve_points(points: &[CurvePoint]) -> Result<(), &'static str> {
    let mut previous_x = None;
    for point in points {
        if [&point.x, &point.y]
            .into_iter()
            .chain(point.tx.iter())
            .chain(point.ty.iter())
            .any(|value| !value.as_f64().is_some_and(f64::is_finite))
        {
            return Err("curvePoints coordinates and tangents must be finite numbers");
        }
        let x = point.x.as_f64().expect("finite coordinate checked above");
        if previous_x.is_some_and(|previous| previous >= x) {
            return Err("curvePoints.x must be strictly increasing");
        }
        previous_x = Some(x);
    }
    if points.len() >= 2 && (points[0].x.as_f64() != Some(0.0) || previous_x != Some(1.0)) {
        return Err("curvePoints must start at x=0 and end at x=1");
    }
    Ok(())
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

    pub fn tangents(&self) -> (Option<&Number>, Option<&Number>) {
        (self.tx.as_ref(), self.ty.as_ref())
    }
}
