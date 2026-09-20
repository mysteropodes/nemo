//! Pure evaluation of the bounded native opacity document.
//!
//! Evaluation consumes an already-acquired immutable snapshot. It does not
//! consult a document owner, current UI state, JavaScript, Paper, or a native
//! host, and selecting a context/frame cannot advance content revision.

use crate::document::{CurvePoint, OpacityKey};
use crate::revision::DocumentSnapshot;
use serde_json::Number;
use std::fmt::{Display, Formatter};

pub const SUPPORTED_CONTEXT_ID: &str = "scene-root";

const CHARACTERIZED_CURVE: [[f64; 2]; 5] = [
    [0.0, 0.0],
    [0.25, 0.156],
    [0.5, 0.5],
    [0.75, 0.844],
    [1.0, 1.0],
];

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
    let eased = evaluate_curve(left.curve_points(), t)?;
    let from = number(left.value(), "motion.opacity.keys[].v[0]")?;
    let to = number(right.value(), "motion.opacity.keys[].v[0]")?;
    Ok(from + (to - from) * eased)
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
        validate_curve(key.curve_points())?;
        let (h_out, h_in) = key.handles();
        validate_zero_handle(h_out, "hOut")?;
        validate_zero_handle(h_in, "hIn")?;
    }
    Ok(())
}

fn validate_curve(points: &[CurvePoint]) -> Result<(), EvaluationError> {
    if points.len() != CHARACTERIZED_CURVE.len() {
        return invalid("curvePoints must contain the five characterized points");
    }
    let mut previous_x = None;
    for (index, (point, expected)) in points.iter().zip(CHARACTERIZED_CURVE).enumerate() {
        let (x, y) = point.coordinates();
        let actual = [number(x, "curvePoints.x")?, number(y, "curvePoints.y")?];
        if previous_x.is_some_and(|value| value >= actual[0]) {
            return invalid("curvePoints.x must be strictly increasing");
        }
        previous_x = Some(actual[0]);
        if actual != expected {
            return Err(EvaluationError::new(
                EvaluationErrorKind::Unsupported,
                format!("curvePoints[{index}] differs from the characterized opacity curve"),
            ));
        }
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

fn evaluate_curve(points: &[CurvePoint], x: f64) -> Result<f64, EvaluationError> {
    validate_curve(points)?;
    let points = points
        .iter()
        .map(|point| {
            let (x, y) = point.coordinates();
            Ok([number(x, "curvePoints.x")?, number(y, "curvePoints.y")?])
        })
        .collect::<Result<Vec<_>, EvaluationError>>()?;
    let x = x.clamp(0.0, 1.0);
    let mut index = 0;
    while index < points.len() - 2 && points[index + 1][0] < x {
        index += 1;
    }
    let p0 = points[index];
    let p3 = points[index + 1];
    let first = tangent(&points, index);
    let second = tangent(&points, index + 1);
    let c1 = [p0[0] + first[0] / 3.0, p0[1] + first[1] / 3.0];
    let c2 = [p3[0] - second[0] / 3.0, p3[1] - second[1] / 3.0];
    let span = p3[0] - p0[0];
    let mut t = if span > 1e-6 { (x - p0[0]) / span } else { 0.0 };
    for _ in 0..8 {
        let error = cubic(t, p0[0], c1[0], c2[0], p3[0]) - x;
        let derivative = cubic_derivative(t, p0[0], c1[0], c2[0], p3[0]);
        if derivative.abs() < 1e-6 {
            break;
        }
        t = (t - error / derivative).clamp(0.0, 1.0);
    }
    Ok(cubic(t, p0[1], c1[1], c2[1], p3[1]))
}

fn tangent(points: &[[f64; 2]], index: usize) -> [f64; 2] {
    let point = points[index];
    let previous = points[index.saturating_sub(1)];
    let next = points[(index + 1).min(points.len() - 1)];
    let x = (next[0] - previous[0]) / 2.0;
    let dx0 = point[0] - previous[0];
    let dx1 = next[0] - point[0];
    let slope0 = if dx0 > 1e-9 {
        (point[1] - previous[1]) / dx0
    } else {
        0.0
    };
    let slope1 = if dx1 > 1e-9 {
        (next[1] - point[1]) / dx1
    } else {
        0.0
    };
    let slope = if index == 0 {
        slope1
    } else if index + 1 == points.len() {
        slope0
    } else if slope0 * slope1 <= 0.0 {
        0.0
    } else {
        let average = (slope0 + slope1) / 2.0;
        let limit = 3.0 * slope0.abs().min(slope1.abs());
        average.clamp(-limit, limit)
    };
    [x, slope * x]
}

fn cubic(t: f64, a: f64, b: f64, c: f64, d: f64) -> f64 {
    let inverse = 1.0 - t;
    inverse * inverse * inverse * a
        + 3.0 * inverse * inverse * t * b
        + 3.0 * inverse * t * t * c
        + t * t * t * d
}

fn cubic_derivative(t: f64, a: f64, b: f64, c: f64, d: f64) -> f64 {
    let inverse = 1.0 - t;
    3.0 * inverse * inverse * (b - a) + 6.0 * inverse * t * (c - b) + 3.0 * t * t * (d - c)
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
