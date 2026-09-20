//! N03's deliberately narrow, host-Rust-only feasibility proof.
//!
//! This crate is neither a production document owner nor a renderer. It accepts
//! one characterized existing fixture, copies its supported data into an owned
//! immutable revision, evaluates that revision without a DOM/Paper/UI object,
//! and exposes a small publication gate for the stale/cancelled proof.

use serde::Serialize;
use serde_json::{Map, Value};
use std::collections::BTreeSet;
use std::fmt::{Display, Formatter};
use std::sync::Arc;

const DOCUMENT_ID: &str = "n03-proof-doc";
const CONTENT_REVISION: u64 = 7;
const CONTEXT_ID: &str = "scene-root";
const SCALE: f64 = 1_000_000.0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProofError {
    Parse(String),
    Unsupported(String),
    Invalid(String),
    OutOfRange(u32),
}

impl Display for ProofError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(message) | Self::Unsupported(message) | Self::Invalid(message) => {
                f.write_str(message)
            }
            Self::OutOfRange(frame) => {
                write!(f, "frame {frame} is outside the immutable proof document")
            }
        }
    }
}

impl std::error::Error for ProofError {}

#[derive(Debug, Clone)]
pub struct NativeDocument {
    document_id: String,
    content_revision: u64,
    fps: u32,
    canvas: [f64; 2],
    layer: Layer,
}

#[derive(Debug, Clone)]
struct Layer {
    layer_uid: String,
    path: Path,
    position_keys: [PositionKey; 2],
}

#[derive(Debug, Clone)]
struct Path {
    stroke_id: String,
    segments: Vec<[f64; 2]>,
}

#[derive(Debug, Clone)]
struct PositionKey {
    frame: u32,
    value: [f64; 2],
    curve: [CurvePoint; 2],
}

#[derive(Debug, Clone)]
struct CurvePoint {
    x: f64,
    y: f64,
    tx: f64,
    ty: f64,
}

#[derive(Debug, Clone)]
pub struct DocumentSnapshot {
    id: String,
    document: Arc<NativeDocument>,
}

impl NativeDocument {
    /// Strictly adapt the one N03 fixture subset into owned Rust values.
    pub fn adapt_curve_workflow(bytes: &[u8]) -> Result<Self, ProofError> {
        let value: Value = serde_json::from_slice(bytes)
            .map_err(|error| ProofError::Parse(format!("fixture JSON: {error}")))?;
        let root = object(&value, "root")?;
        reject_unknown(
            root,
            &[
                "version",
                "totalFrames",
                "fps",
                "canvasW",
                "canvasH",
                "canvasBg",
                "waIn",
                "waOut",
                "layers",
                "symbols",
                "cameraKeys",
            ],
            "root",
        )?;
        require_u32(root, "version", "root", 13)?;
        require_u32(root, "totalFrames", "root", 21)?;
        require_u32(root, "fps", "root", 24)?;
        require_u32(root, "canvasW", "root", 320)?;
        require_u32(root, "canvasH", "root", 180)?;
        require_string(root, "canvasBg", "root", "#ffffff")?;
        require_u32(root, "waIn", "root", 0)?;
        require_u32(root, "waOut", "root", 20)?;
        require_empty_object(root, "symbols", "root")?;
        require_empty_array(root, "cameraKeys", "root")?;

        let layers = array_field(root, "layers", "root")?;
        if layers.len() != 1 {
            return unsupported("only one top-level vector layer is supported");
        }
        let layer = parse_layer(&layers[0])?;
        Ok(Self {
            document_id: DOCUMENT_ID.to_string(),
            content_revision: CONTENT_REVISION,
            fps: 24,
            canvas: [320.0, 180.0],
            layer,
        })
    }

    pub fn snapshot(&self) -> DocumentSnapshot {
        DocumentSnapshot {
            id: format!("{}:r{}", self.document_id, self.content_revision),
            document: Arc::new(self.clone()),
        }
    }

    /// Test-only revision construction: it proves an old snapshot does not reread new data.
    pub fn with_content_revision(&self, revision: u64) -> Self {
        let mut next = self.clone();
        next.content_revision = revision;
        next
    }
}

impl DocumentSnapshot {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn document_id(&self) -> &str {
        &self.document.document_id
    }

    pub fn content_revision(&self) -> u64 {
        self.document.content_revision
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Evaluation {
    pub snapshot_id: String,
    pub context_id: String,
    pub frame: u32,
    pub fps: u32,
    pub canvas: [f64; 2],
    pub item: EvaluatedItem,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EvaluatedItem {
    pub layer_uid: String,
    pub stroke_id: String,
    pub segments: Vec<[f64; 2]>,
    pub position: [f64; 2],
}

pub fn evaluate(
    snapshot: &DocumentSnapshot,
    context_id: &str,
    frame: u32,
) -> Result<Evaluation, ProofError> {
    if context_id != CONTEXT_ID {
        return unsupported("only the fixture's scene-root context is supported");
    }
    if frame > 20 {
        return Err(ProofError::OutOfRange(frame));
    }
    let layer = &snapshot.document.layer;
    Ok(Evaluation {
        snapshot_id: snapshot.id.clone(),
        context_id: context_id.to_string(),
        frame,
        fps: snapshot.document.fps,
        canvas: snapshot.document.canvas,
        item: EvaluatedItem {
            layer_uid: layer.layer_uid.clone(),
            stroke_id: layer.path.stroke_id.clone(),
            segments: layer.path.segments.clone(),
            position: evaluate_position(&layer.position_keys, frame),
        },
    })
}

/// Stable manually ordered JSON bytes; integer microunits avoid float-format drift.
pub fn canonical_property_bytes(evaluation: &Evaluation) -> Result<Vec<u8>, ProofError> {
    let [x, y] = microunits(evaluation.item.position)?;
    Ok(format!(
        "{{\"layerUid\":\"{}\",\"property\":\"position\",\"frame\":{},\"valueMicrounits\":[{},{}]}}",
        evaluation.item.layer_uid, evaluation.frame, x, y
    )
    .into_bytes())
}

/// Stable manually ordered JSON bytes for the evaluated scene description, not a render scene.
pub fn canonical_scene_bytes(evaluation: &Evaluation) -> Result<Vec<u8>, ProofError> {
    let [width, height] = microunits(evaluation.canvas)?;
    let [x, y] = microunits(evaluation.item.position)?;
    let mut segments = String::new();
    for (index, point) in evaluation.item.segments.iter().enumerate() {
        if index > 0 {
            segments.push(',');
        }
        let [point_x, point_y] = microunits(*point)?;
        segments.push_str(&format!("[{point_x},{point_y}]"));
    }
    Ok(format!(
        "{{\"frame\":{},\"fps\":{},\"canvasMicrounits\":[{},{}],\"items\":[{{\"layerUid\":\"{}\",\"strokeId\":\"{}\",\"closed\":true,\"fill\":\"#ff0000\",\"segmentsMicrounits\":[{}],\"transform\":{{\"positionMicrounits\":[{},{}],\"rotationMicroradians\":0,\"scaleMicrounits\":[1000000,1000000],\"opacityMicrounits\":1000000}}}}]}}",
        evaluation.frame,
        evaluation.fps,
        width,
        height,
        evaluation.item.layer_uid,
        evaluation.item.stroke_id,
        segments,
        x,
        y
    )
    .into_bytes())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum PublicationRejection {
    Cancelled,
    WrongDocument,
    StaleRevision,
    StaleGeneration,
}

#[derive(Debug, Clone)]
pub struct PublicationGate {
    document_id: String,
    current_revision: u64,
    last_generation: u64,
    cancelled: BTreeSet<String>,
}

impl PublicationGate {
    pub fn new(snapshot: &DocumentSnapshot) -> Self {
        Self {
            document_id: snapshot.document_id().to_string(),
            current_revision: snapshot.content_revision(),
            last_generation: 0,
            cancelled: BTreeSet::new(),
        }
    }

    pub fn set_current_revision(&mut self, revision: u64) {
        self.current_revision = revision;
    }

    pub fn cancel(&mut self, snapshot: &DocumentSnapshot) {
        self.cancelled.insert(snapshot.id().to_string());
    }

    pub fn publish(
        &mut self,
        snapshot: &DocumentSnapshot,
        adapter_generation: u64,
    ) -> Result<(), PublicationRejection> {
        if self.cancelled.contains(snapshot.id()) {
            return Err(PublicationRejection::Cancelled);
        }
        if snapshot.document_id() != self.document_id {
            return Err(PublicationRejection::WrongDocument);
        }
        if snapshot.content_revision() != self.current_revision {
            return Err(PublicationRejection::StaleRevision);
        }
        if adapter_generation <= self.last_generation {
            return Err(PublicationRejection::StaleGeneration);
        }
        self.last_generation = adapter_generation;
        Ok(())
    }
}

fn parse_layer(value: &Value) -> Result<Layer, ProofError> {
    let layer = object(value, "layer")?;
    reject_unknown(
        layer,
        &[
            "name",
            "visible",
            "locked",
            "frames",
            "color",
            "motion",
            "layerUid",
            "parentLayerUid",
        ],
        "layer",
    )?;
    require_bool(layer, "visible", "layer", true)?;
    require_bool(layer, "locked", "layer", false)?;
    require_string(layer, "layerUid", "layer", "r08_curve_layer")?;
    require_null(layer, "parentLayerUid", "layer")?;
    let frames = array_field(layer, "frames", "layer")?;
    if frames.len() != 120 {
        return unsupported("only the fixture's 120 stored frames are supported");
    }
    let path = parse_frames(frames)?;
    let motion = object_field(layer, "motion", "layer")?;
    reject_unknown(motion, &["position"], "motion")?;
    let position = object_field(motion, "position", "motion")?;
    reject_unknown(position, &["keys"], "position track")?;
    let keys = array_field(position, "keys", "position track")?;
    if keys.len() != 2 {
        return unsupported("only a two-key Position track is supported");
    }
    let first = parse_position_key(&keys[0])?;
    let second = parse_position_key(&keys[1])?;
    if first.frame != 0 || second.frame != 20 || first.frame >= second.frame {
        return unsupported("only the fixture's Position keys at frames 0 and 20 are supported");
    }
    Ok(Layer {
        layer_uid: "r08_curve_layer".to_string(),
        path,
        position_keys: [first, second],
    })
}

fn parse_frames(frames: &[Value]) -> Result<Path, ProofError> {
    let first = object(&frames[0], "frame 0")?;
    reject_unknown(
        first,
        &["strokes", "isKeyframe", "isInterpolated"],
        "frame 0",
    )?;
    require_bool(first, "isKeyframe", "frame 0", true)?;
    require_bool(first, "isInterpolated", "frame 0", false)?;
    let strokes = array_field(first, "strokes", "frame 0")?;
    if strokes.len() != 1 {
        return unsupported("only one closed vector path is supported");
    }
    for (index, frame) in frames.iter().enumerate().skip(1) {
        let frame = object(frame, "held frame")?;
        reject_unknown(
            frame,
            &["strokes", "isKeyframe", "isInterpolated"],
            "held frame",
        )?;
        require_bool(frame, "isKeyframe", "held frame", false)?;
        require_bool(frame, "isInterpolated", "held frame", false)?;
        if !array_field(frame, "strokes", "held frame")?.is_empty() {
            return Err(ProofError::Unsupported(format!(
                "frame-by-frame or tween content at frame {index} is unsupported"
            )));
        }
    }
    parse_path(&strokes[0])
}

fn parse_path(value: &Value) -> Result<Path, ProofError> {
    let stroke = object(value, "path")?;
    reject_unknown(
        stroke,
        &[
            "segments",
            "closed",
            "strokeColor",
            "hasRealStroke",
            "strokeWidth",
            "strokeCap",
            "strokeJoin",
            "miterLimit",
            "fillColor",
            "opacity",
            "dashOffset",
            "strokeId",
        ],
        "path",
    )?;
    require_bool(stroke, "closed", "path", true)?;
    require_bool(stroke, "hasRealStroke", "path", false)?;
    require_string(stroke, "fillColor", "path", "#ff0000")?;
    require_number(stroke, "opacity", "path", 1.0)?;
    require_string(stroke, "strokeId", "path", "r08_curve_rect")?;
    let segments = array_field(stroke, "segments", "path")?;
    if segments.len() != 4 {
        return unsupported("only the fixture's four-segment rectangle is supported");
    }
    let mut points = Vec::with_capacity(4);
    for segment in segments {
        let segment = object(segment, "path segment")?;
        reject_unknown(segment, &["point", "handleIn", "handleOut"], "path segment")?;
        let point = coordinate(value_field(segment, "point", "path segment")?, "point")?;
        if coordinate(
            value_field(segment, "handleIn", "path segment")?,
            "handleIn",
        )? != [0.0, 0.0]
            || coordinate(
                value_field(segment, "handleOut", "path segment")?,
                "handleOut",
            )? != [0.0, 0.0]
        {
            return unsupported("Bezier handles are unsupported");
        }
        points.push(point);
    }
    if points != [[20.0, 80.0], [20.0, 60.0], [40.0, 60.0], [40.0, 80.0]] {
        return unsupported("only the characterized rectangle geometry is supported");
    }
    Ok(Path {
        stroke_id: "r08_curve_rect".to_string(),
        segments: points,
    })
}

fn parse_position_key(value: &Value) -> Result<PositionKey, ProofError> {
    let key = object(value, "Position key")?;
    reject_unknown(
        key,
        &["frame", "v", "curvePoints", "hOut", "hIn", "hold"],
        "Position key",
    )?;
    if key.get("hold").and_then(Value::as_bool) == Some(true) {
        return unsupported("hold keys are unsupported");
    }
    let h_out = coordinate(value_field(key, "hOut", "Position key")?, "hOut")?;
    let h_in = coordinate(value_field(key, "hIn", "Position key")?, "hIn")?;
    if h_out != [0.0, 0.0] || h_in != [0.0, 0.0] {
        return unsupported("spatial Position handles are unsupported");
    }
    let curve = array_field(key, "curvePoints", "Position key")?;
    if curve.len() != 2 {
        return unsupported("only two-point timing curves are supported");
    }
    let first = parse_curve_point(&curve[0])?;
    let second = parse_curve_point(&curve[1])?;
    if first.x != 0.0 || first.y != 0.0 || second.x != 1.0 || second.y != 1.0 {
        return unsupported("timing curves must use normalized endpoints");
    }
    Ok(PositionKey {
        frame: u32_field(key, "frame", "Position key")?,
        value: coordinate(value_field(key, "v", "Position key")?, "Position value")?,
        curve: [first, second],
    })
}

fn parse_curve_point(value: &Value) -> Result<CurvePoint, ProofError> {
    let point = object(value, "curve point")?;
    reject_unknown(point, &["x", "y", "tx", "ty"], "curve point")?;
    Ok(CurvePoint {
        x: number_field(point, "x", "curve point")?,
        y: number_field(point, "y", "curve point")?,
        tx: number_field(point, "tx", "curve point")?,
        ty: number_field(point, "ty", "curve point")?,
    })
}

fn evaluate_position(keys: &[PositionKey; 2], frame: u32) -> [f64; 2] {
    let first = &keys[0];
    let second = &keys[1];
    if frame <= first.frame {
        return first.value;
    }
    if frame >= second.frame {
        return second.value;
    }
    let t = (frame - first.frame) as f64 / (second.frame - first.frame) as f64;
    let eased = eval_curve(&first.curve, t);
    [
        first.value[0] + (second.value[0] - first.value[0]) * eased,
        first.value[1] + (second.value[1] - first.value[1]) * eased,
    ]
}

fn eval_curve(points: &[CurvePoint; 2], x: f64) -> f64 {
    let p0 = &points[0];
    let p3 = &points[1];
    let c1 = (p0.x + p0.tx / 3.0, p0.y + p0.ty / 3.0);
    let c2 = (p3.x - p3.tx / 3.0, p3.y - p3.ty / 3.0);
    let mut t = x.clamp(0.0, 1.0);
    for _ in 0..8 {
        let error = cubic(t, p0.x, c1.0, c2.0, p3.x) - x;
        let derivative = cubic_derivative(t, p0.x, c1.0, c2.0, p3.x);
        if derivative.abs() < 0.000_001 {
            break;
        }
        t = (t - error / derivative).clamp(0.0, 1.0);
    }
    cubic(t, p0.y, c1.1, c2.1, p3.y)
}

fn cubic(t: f64, a: f64, b: f64, c: f64, d: f64) -> f64 {
    let u = 1.0 - t;
    u * u * u * a + 3.0 * u * u * t * b + 3.0 * u * t * t * c + t * t * t * d
}

fn cubic_derivative(t: f64, a: f64, b: f64, c: f64, d: f64) -> f64 {
    let u = 1.0 - t;
    3.0 * u * u * (b - a) + 6.0 * u * t * (c - b) + 3.0 * t * t * (d - c)
}

fn microunits(values: [f64; 2]) -> Result<[i64; 2], ProofError> {
    Ok([microunit(values[0])?, microunit(values[1])?])
}

fn microunit(value: f64) -> Result<i64, ProofError> {
    if !value.is_finite() {
        return Err(ProofError::Invalid(
            "non-finite evaluator output".to_string(),
        ));
    }
    Ok((value * SCALE).round() as i64)
}

fn object<'a>(value: &'a Value, context: &str) -> Result<&'a Map<String, Value>, ProofError> {
    value
        .as_object()
        .ok_or_else(|| ProofError::Invalid(format!("{context} must be an object")))
}

fn object_field<'a>(
    map: &'a Map<String, Value>,
    key: &str,
    context: &str,
) -> Result<&'a Map<String, Value>, ProofError> {
    object(value_field(map, key, context)?, key)
}

fn array_field<'a>(
    map: &'a Map<String, Value>,
    key: &str,
    context: &str,
) -> Result<&'a Vec<Value>, ProofError> {
    value_field(map, key, context)?
        .as_array()
        .ok_or_else(|| ProofError::Invalid(format!("{context}.{key} must be an array")))
}

fn value_field<'a>(
    map: &'a Map<String, Value>,
    key: &str,
    context: &str,
) -> Result<&'a Value, ProofError> {
    map.get(key)
        .ok_or_else(|| ProofError::Invalid(format!("{context}.{key} is required")))
}

fn number_field(map: &Map<String, Value>, key: &str, context: &str) -> Result<f64, ProofError> {
    let number = value_field(map, key, context)?
        .as_f64()
        .ok_or_else(|| ProofError::Invalid(format!("{context}.{key} must be a number")))?;
    if number.is_finite() {
        Ok(number)
    } else {
        Err(ProofError::Invalid(format!(
            "{context}.{key} must be finite"
        )))
    }
}

fn u32_field(map: &Map<String, Value>, key: &str, context: &str) -> Result<u32, ProofError> {
    value_field(map, key, context)?
        .as_u64()
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(|| ProofError::Invalid(format!("{context}.{key} must be a u32")))
}

fn coordinate(value: &Value, context: &str) -> Result<[f64; 2], ProofError> {
    let values = value
        .as_array()
        .ok_or_else(|| ProofError::Invalid(format!("{context} must be a two-number array")))?;
    if values.len() != 2 {
        return Err(ProofError::Invalid(format!(
            "{context} must contain two numbers"
        )));
    }
    let x = values[0]
        .as_f64()
        .filter(|value| value.is_finite())
        .ok_or_else(|| ProofError::Invalid(format!("{context}[0] must be finite")))?;
    let y = values[1]
        .as_f64()
        .filter(|value| value.is_finite())
        .ok_or_else(|| ProofError::Invalid(format!("{context}[1] must be finite")))?;
    Ok([x, y])
}

fn reject_unknown(
    map: &Map<String, Value>,
    allowed: &[&str],
    context: &str,
) -> Result<(), ProofError> {
    for key in map.keys() {
        if !allowed.contains(&key.as_str()) {
            return unsupported(&format!("{context}.{key} is unsupported"));
        }
    }
    Ok(())
}

fn require_u32(
    map: &Map<String, Value>,
    key: &str,
    context: &str,
    expected: u32,
) -> Result<(), ProofError> {
    if u32_field(map, key, context)? == expected {
        Ok(())
    } else {
        unsupported(&format!(
            "{context}.{key} differs from the characterized fixture"
        ))
    }
}

fn require_string(
    map: &Map<String, Value>,
    key: &str,
    context: &str,
    expected: &str,
) -> Result<(), ProofError> {
    if value_field(map, key, context)?.as_str() == Some(expected) {
        Ok(())
    } else {
        unsupported(&format!(
            "{context}.{key} differs from the characterized fixture"
        ))
    }
}

fn require_bool(
    map: &Map<String, Value>,
    key: &str,
    context: &str,
    expected: bool,
) -> Result<(), ProofError> {
    if value_field(map, key, context)?.as_bool() == Some(expected) {
        Ok(())
    } else {
        unsupported(&format!(
            "{context}.{key} differs from the characterized fixture"
        ))
    }
}

fn require_number(
    map: &Map<String, Value>,
    key: &str,
    context: &str,
    expected: f64,
) -> Result<(), ProofError> {
    if number_field(map, key, context)? == expected {
        Ok(())
    } else {
        unsupported(&format!(
            "{context}.{key} differs from the characterized fixture"
        ))
    }
}

fn require_null(map: &Map<String, Value>, key: &str, context: &str) -> Result<(), ProofError> {
    if value_field(map, key, context)?.is_null() {
        Ok(())
    } else {
        unsupported(&format!("{context}.{key} is unsupported"))
    }
}

fn require_empty_array(
    map: &Map<String, Value>,
    key: &str,
    context: &str,
) -> Result<(), ProofError> {
    if array_field(map, key, context)?.is_empty() {
        Ok(())
    } else {
        unsupported(&format!("{context}.{key} is unsupported"))
    }
}

fn require_empty_object(
    map: &Map<String, Value>,
    key: &str,
    context: &str,
) -> Result<(), ProofError> {
    if object_field(map, key, context)?.is_empty() {
        Ok(())
    } else {
        unsupported(&format!("{context}.{key} is unsupported"))
    }
}

fn unsupported<T>(message: &str) -> Result<T, ProofError> {
    Err(ProofError::Unsupported(message.to_string()))
}
