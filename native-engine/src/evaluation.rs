//! Pure evaluation of the bounded native opacity document.
//!
//! Evaluation consumes an already-acquired immutable snapshot. It does not
//! consult a document owner, current UI state, JavaScript, Paper, or a native
//! host, and selecting a context/frame cannot advance content revision.

use crate::animation_curve::{self, CurveSample};
use crate::document::{validate_curve_points, CurvePoint, OpacityKey};
use crate::revision::DocumentSnapshot;
use serde_json::Number;
use std::fmt::{Display, Formatter};

// N22C: private expression noise kernel; its consumers arrive in later leaves.
#[path = "expression_noise.rs"]
#[allow(dead_code)]
pub(crate) mod expression_noise;
// N22D: private random()/randomGauss() kernel over the same lattice hash.
#[path = "expression_random.rs"]
#[allow(dead_code)]
pub(crate) mod expression_random;
// N22E: private math/vector expression helpers (clamp, remap, vectors).
#[path = "expression_math.rs"]
#[allow(dead_code)]
pub(crate) mod expression_math;
// N22F: private time/loop-timing expression helpers (stepTime, loops).
#[path = "expression_time.rs"]
#[allow(dead_code)]
pub(crate) mod expression_time;
// N22G: private Motion parent-chain affine kernel (transforms, chain walks, blends).
#[path = "motion_parent_chain.rs"]
#[allow(dead_code)]
pub(crate) mod motion_parent_chain;
#[path = "property_expression_runtime.rs"]
#[allow(dead_code)]
pub(crate) mod property_expression_runtime;
#[path = "property_transform_eval.rs"]
#[allow(dead_code)]
pub(crate) mod property_transform_eval;
#[path = "property_follow_path_sample.rs"]
#[allow(dead_code)]
pub(crate) mod property_follow_path_sample;
#[path = "property_expression_views.rs"]
#[allow(dead_code)]
pub(crate) mod property_expression_views;
#[path = "property_camera_eval.rs"]
#[allow(dead_code)]
pub(crate) mod property_camera_eval;

pub const SUPPORTED_CONTEXT_ID: &str = "scene-root";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvaluationErrorKind {
    Invalid,
    Unsupported,
    OutOfRange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvaluationError {
    kind: EvaluationErrorKind,
    message: String,
}

impl EvaluationError {
    pub fn kind(&self) -> EvaluationErrorKind {
        self.kind
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    fn new(kind: EvaluationErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

impl Display for EvaluationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for EvaluationError {}

#[derive(Debug, Clone, PartialEq)]
pub struct OpacityEvaluation {
    document_snapshot_id: String,
    document_id: String,
    content_revision: u64,
    context_id: String,
    frame: u32,
    layers: Vec<EvaluatedOpacity>,
}

impl OpacityEvaluation {
    pub fn document_snapshot_id(&self) -> &str {
        &self.document_snapshot_id
    }

    pub fn document_id(&self) -> &str {
        &self.document_id
    }

    pub fn content_revision(&self) -> u64 {
        self.content_revision
    }

    pub fn context_id(&self) -> &str {
        &self.context_id
    }

    pub fn frame(&self) -> u32 {
        self.frame
    }

    pub fn layers(&self) -> &[EvaluatedOpacity] {
        &self.layers
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct EvaluatedOpacity {
    layer_uid: String,
    value: f64,
}

impl EvaluatedOpacity {
    pub fn layer_uid(&self) -> &str {
        &self.layer_uid
    }

    pub fn value(&self) -> f64 {
        self.value
    }
}

/// Evaluate every admitted layer in stored order from one immutable snapshot.
pub fn evaluate(
    snapshot: &DocumentSnapshot,
    context_id: &str,
    frame: u32,
) -> Result<OpacityEvaluation, EvaluationError> {
    if context_id != SUPPORTED_CONTEXT_ID {
        return Err(EvaluationError::new(
            EvaluationErrorKind::Unsupported,
            "only the declared scene-root context is supported",
        ));
    }
    let document = snapshot.document();
    if frame >= document.total_frames() {
        return Err(EvaluationError::new(
            EvaluationErrorKind::OutOfRange,
            format!(
                "frame {frame} is outside totalFrames {}",
                document.total_frames()
            ),
        ));
    }

    let mut layers = Vec::with_capacity(document.layers().len());
    for layer in document.layers() {
        let value = match layer.opacity_keys() {
            Some(keys) => evaluate_track(keys, document.total_frames(), frame)?,
            None => opacity(
                layer.static_opacity().ok_or_else(|| {
                    EvaluationError::new(
                        EvaluationErrorKind::Invalid,
                        "an admitted layer has neither keyed nor static opacity",
                    )
                })?,
                "motionStatic.opacity[0]",
            )?,
        };
        layers.push(EvaluatedOpacity {
            layer_uid: layer.layer_uid().to_owned(),
            value,
        });
    }

    Ok(OpacityEvaluation {
        document_snapshot_id: snapshot.id().to_owned(),
        document_id: snapshot.document_id().to_owned(),
        content_revision: snapshot.content_revision(),
        context_id: context_id.to_owned(),
        frame,
        layers,
    })
}

fn evaluate_track(
    keys: &[OpacityKey],
    total_frames: u32,
    frame: u32,
) -> Result<f64, EvaluationError> {
    validate_track(keys, total_frames)?;
    if frame <= keys[0].frame() {
        return number(keys[0].value(), "motion.opacity.keys[].v[0]");
    }
    let last = &keys[keys.len() - 1];
    if frame >= last.frame() {
        return number(last.value(), "motion.opacity.keys[].v[0]");
    }
    let left_index = keys
        .windows(2)
        .position(|pair| frame < pair[1].frame())
        .ok_or_else(|| {
            EvaluationError::new(
                EvaluationErrorKind::Invalid,
                "opacity key segment could not be selected",
            )
        })?;
    let left = &keys[left_index];
    let right = &keys[left_index + 1];
    let span = right.frame() - left.frame();
    let t = f64::from(frame - left.frame()) / f64::from(span);
    let eased = evaluate_admitted_curve(left.curve_points(), t)?;
    let from = number(left.value(), "motion.opacity.keys[].v[0]")?;
    let to = number(right.value(), "motion.opacity.keys[].v[0]")?;
    let value = from + (to - from) * eased;
    if !value.is_finite() {
        return invalid("interpolated opacity must be finite");
    }
    Ok(value)
}

fn validate_track(keys: &[OpacityKey], total_frames: u32) -> Result<(), EvaluationError> {
    if keys.is_empty() {
        return invalid("motion.opacity.keys must not be empty");
    }
    let mut previous = None;
    for key in keys {
        if key.frame() >= total_frames {
            return invalid(format!(
                "opacity key frame {} is outside totalFrames {total_frames}",
                key.frame()
            ));
        }
        if previous.is_some_and(|frame| frame >= key.frame()) {
            return invalid("opacity key frames must be strictly increasing");
        }
        previous = Some(key.frame());
        opacity(key.value(), "motion.opacity.keys[].v[0]")?;
        validate_curve_points(key.curve_points())
            .map_err(|message| EvaluationError::new(EvaluationErrorKind::Invalid, message))?;
        let (h_out, h_in) = key.handles();
        validate_zero_handle(h_out, "hOut")?;
        validate_zero_handle(h_in, "hIn")?;
    }
    Ok(())
}

fn validate_zero_handle(handle: &[Number; 2], name: &str) -> Result<(), EvaluationError> {
    let values = [number(&handle[0], name)?, number(&handle[1], name)?];
    if values != [0.0, 0.0] {
        return Err(EvaluationError::new(
            EvaluationErrorKind::Unsupported,
            format!("non-zero opacity {name} is unsupported"),
        ));
    }
    Ok(())
}

fn evaluate_admitted_curve(points: &[CurvePoint], x: f64) -> Result<f64, EvaluationError> {
    let points = points
        .iter()
        .map(|point| {
            let (x, y) = point.coordinates();
            let (tx, ty) = point.tangents();
            Ok(CurveSample {
                x: number(x, "curvePoints.x")?,
                y: number(y, "curvePoints.y")?,
                tx: tx
                    .map(|value| number(value, "curvePoints.tx"))
                    .transpose()?,
                ty: ty
                    .map(|value| number(value, "curvePoints.ty"))
                    .transpose()?,
            })
        })
        .collect::<Result<Vec<_>, EvaluationError>>()?;
    Ok(animation_curve::evaluate_curve(&points, x))
}

fn number(value: &Number, context: &str) -> Result<f64, EvaluationError> {
    value.as_f64().ok_or_else(|| {
        EvaluationError::new(
            EvaluationErrorKind::Invalid,
            format!("{context} must be finite"),
        )
    })
}

fn opacity(value: &Number, context: &str) -> Result<f64, EvaluationError> {
    let value = number(value, context)?;
    if !(0.0..=100.0).contains(&value) {
        return invalid(format!("{context} must be in the range 0..100"));
    }
    Ok(value)
}

fn invalid<T>(message: impl Into<String>) -> Result<T, EvaluationError> {
    Err(EvaluationError::new(EvaluationErrorKind::Invalid, message))
}
