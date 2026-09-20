//! Pure preparation of one scheduled immutable opacity frame for native composition.
//!
//! Geometry and paint are a separate, versioned immutable resource. This module
//! never derives geometry from a frame number or reads mutable document state.

use crate::evaluation;
use crate::scheduler::{
    EvaluationKey, FrameScheduler, OutputSpec, ScheduledFrame, ViewGeneration, WorkId,
};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderSceneErrorKind {
    Inactive,
    Unsupported,
    InvalidInput,
    ResourceMismatch,
    LayerJoin,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderSceneError {
    kind: RenderSceneErrorKind,
    message: String,
}

impl RenderSceneError {
    pub fn kind(&self) -> RenderSceneErrorKind {
        self.kind
    }

    fn new(kind: RenderSceneErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

impl Display for RenderSceneError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for RenderSceneError {}

/// Opaque sRGB paint. Layer opacity comes exclusively from `evaluation`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpaqueSrgbPaint {
    red: u8,
    green: u8,
    blue: u8,
}

impl OpaqueSrgbPaint {
    pub fn new(red: u8, green: u8, blue: u8) -> Self {
        Self { red, green, blue }
    }

    pub fn rgb(self) -> [u8; 3] {
        [self.red, self.green, self.blue]
    }
}

/// Immutable geometry associated with a stable document `layerUid`.
#[derive(Debug, Clone, PartialEq)]
pub struct LayerGeometry {
    layer_uid: String,
    bounds: [f64; 4],
    transform: [f64; 6],
    paint: OpaqueSrgbPaint,
}

impl LayerGeometry {
    pub fn new(
        layer_uid: impl Into<String>,
        bounds: [f64; 4],
        transform: [f64; 6],
        paint: OpaqueSrgbPaint,
    ) -> Result<Self, RenderSceneError> {
        let layer = Self {
            layer_uid: layer_uid.into(),
            bounds,
            transform,
            paint,
        };
        if layer.layer_uid.is_empty() {
            return Err(RenderSceneError::new(
                RenderSceneErrorKind::InvalidInput,
                "geometry layerUid must be non-empty",
            ));
        }
        if !bounds
            .iter()
            .chain(transform.iter())
            .all(|value| value.is_finite())
            || bounds[2] <= bounds[0]
            || bounds[3] <= bounds[1]
        {
            return Err(RenderSceneError::new(
                RenderSceneErrorKind::InvalidInput,
                "geometry bounds and transform must be finite with positive bounds",
            ));
        }
        if transform[0] * transform[3] - transform[1] * transform[2] == 0.0 {
            return Err(RenderSceneError::new(
                RenderSceneErrorKind::InvalidInput,
                "geometry transform must be invertible",
            ));
        }
        Ok(layer)
    }

    pub fn layer_uid(&self) -> &str {
        &self.layer_uid
    }
}

/// One declared, immutable geometry/paint resource for a scheduled frame.
#[derive(Debug, Clone, PartialEq)]
pub struct GeometryPaintInput {
    resource_id: String,
    resource_version: String,
    layers: Vec<LayerGeometry>,
}

impl GeometryPaintInput {
    pub fn new(
        resource_id: impl Into<String>,
        resource_version: impl Into<String>,
        layers: Vec<LayerGeometry>,
    ) -> Result<Self, RenderSceneError> {
        let input = Self {
            resource_id: resource_id.into(),
            resource_version: resource_version.into(),
            layers,
        };
        if input.resource_id.is_empty() || input.resource_version.is_empty() {
            return Err(RenderSceneError::new(
                RenderSceneErrorKind::InvalidInput,
                "geometry resource ID and version must be non-empty",
            ));
        }
        let mut ids = BTreeSet::new();
        for layer in &input.layers {
            if !ids.insert(layer.layer_uid.clone()) {
                return Err(RenderSceneError::new(
                    RenderSceneErrorKind::LayerJoin,
                    format!("duplicate geometry layerUid {}", layer.layer_uid),
                ));
            }
        }
        Ok(input)
    }

    pub fn resource_id(&self) -> &str {
        &self.resource_id
    }

    pub fn resource_version(&self) -> &str {
        &self.resource_version
    }
}

/// The complete identity N11 assigned to still-live scheduled work.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduledFrameIdentity {
    work_id: WorkId,
    view_generation: ViewGeneration,
    evaluation_key: EvaluationKey,
}

impl ScheduledFrameIdentity {
    pub fn from_scheduled(scheduled: &ScheduledFrame) -> Self {
        Self {
            work_id: scheduled.work_id(),
            view_generation: scheduled.view_generation(),
            evaluation_key: scheduled.key().clone(),
        }
    }

    pub fn work_id(&self) -> WorkId {
        self.work_id
    }

    pub fn view_generation(&self) -> ViewGeneration {
        self.view_generation
    }

    pub fn evaluation_key(&self) -> &EvaluationKey {
        &self.evaluation_key
    }
}

/// Deterministic, fully joined scene input consumed by the GPU compositor.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderScene {
    scheduled_identity: ScheduledFrameIdentity,
    document_id: String,
    content_revision: u64,
    pub(crate) layers: Vec<PreparedLayer>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PreparedLayer {
    pub(crate) bounds: [f64; 4],
    pub(crate) transform: [f64; 6],
    pub(crate) paint: OpaqueSrgbPaint,
    pub(crate) opacity_percent: f64,
}

impl RenderScene {
    pub fn scheduled_identity(&self) -> &ScheduledFrameIdentity {
        &self.scheduled_identity
    }

    pub fn document_snapshot_id(&self) -> &str {
        self.scheduled_identity
            .evaluation_key()
            .document_snapshot_id()
    }

    pub fn document_id(&self) -> &str {
        &self.document_id
    }

    pub fn content_revision(&self) -> u64 {
        self.content_revision
    }

    pub fn context_id(&self) -> &str {
        self.scheduled_identity.evaluation_key().context_id()
    }

    pub fn frame(&self) -> u32 {
        self.scheduled_identity.evaluation_key().frame()
    }

    pub fn quality(&self) -> &str {
        self.scheduled_identity.evaluation_key().quality()
    }

    pub fn output_spec(&self) -> &OutputSpec {
        self.scheduled_identity.evaluation_key().output_spec()
    }

    pub fn layer_count(&self) -> usize {
        self.layers.len()
    }
}

/// Prepare a scene from a still-live scheduled frame without terminalizing it.
pub fn prepare(
    scheduler: &FrameScheduler,
    scheduled: &ScheduledFrame,
    geometry: &GeometryPaintInput,
) -> Result<RenderScene, RenderSceneError> {
    let key = scheduled.key();
    validate_contract(key)?;
    let snapshot = scheduler
        .pinned_snapshot(scheduled.work_id())
        .ok_or_else(|| {
            RenderSceneError::new(
                RenderSceneErrorKind::Inactive,
                "scheduled frame is no longer active and has no pinned snapshot",
            )
        })?;
    if snapshot.id() != key.document_snapshot_id() {
        return Err(RenderSceneError::new(
            RenderSceneErrorKind::Inactive,
            "active snapshot no longer matches the scheduled evaluation key",
        ));
    }
    match key.declared_resource_versions().get(geometry.resource_id()) {
        Some(version) if version == geometry.resource_version() => {}
        _ => {
            return Err(RenderSceneError::new(
                RenderSceneErrorKind::ResourceMismatch,
                "geometry resource ID/version is not declared by the evaluation key",
            ));
        }
    }

    let evaluation =
        evaluation::evaluate(snapshot, key.context_id(), key.frame()).map_err(|error| {
            RenderSceneError::new(
                RenderSceneErrorKind::Unsupported,
                format!("evaluate scheduled snapshot: {error}"),
            )
        })?;
    if evaluation.document_snapshot_id() != key.document_snapshot_id()
        || evaluation.document_id() != snapshot.document_id()
        || evaluation.content_revision() != snapshot.content_revision()
        || evaluation.context_id() != key.context_id()
        || evaluation.frame() != key.frame()
    {
        return Err(RenderSceneError::new(
            RenderSceneErrorKind::Inactive,
            "evaluation identity differs from the active scheduled frame",
        ));
    }

    let mut evaluated = BTreeMap::new();
    for layer in evaluation.layers() {
        if evaluated.insert(layer.layer_uid(), layer.value()).is_some() {
            return Err(RenderSceneError::new(
                RenderSceneErrorKind::LayerJoin,
                format!("duplicate evaluated layerUid {}", layer.layer_uid()),
            ));
        }
    }
    if evaluated.len() != geometry.layers.len() {
        return Err(RenderSceneError::new(
            RenderSceneErrorKind::LayerJoin,
            "geometry and evaluated layerUid sets are not identical",
        ));
    }

    let mut layers = Vec::with_capacity(geometry.layers.len());
    for layer in &geometry.layers {
        let opacity = evaluated.remove(layer.layer_uid()).ok_or_else(|| {
            RenderSceneError::new(
                RenderSceneErrorKind::LayerJoin,
                format!(
                    "geometry layerUid {} has no evaluated opacity",
                    layer.layer_uid()
                ),
            )
        })?;
        layers.push(PreparedLayer {
            bounds: layer.bounds,
            transform: layer.transform,
            paint: layer.paint,
            opacity_percent: opacity,
        });
    }
    if !evaluated.is_empty() {
        return Err(RenderSceneError::new(
            RenderSceneErrorKind::LayerJoin,
            "evaluated layerUid has no immutable geometry input",
        ));
    }

    Ok(RenderScene {
        scheduled_identity: ScheduledFrameIdentity::from_scheduled(scheduled),
        document_id: snapshot.document_id().to_owned(),
        content_revision: snapshot.content_revision(),
        layers,
    })
}

fn validate_contract(key: &EvaluationKey) -> Result<(), RenderSceneError> {
    let output = key.output_spec();
    if key.quality() != "final" {
        return Err(RenderSceneError::new(
            RenderSceneErrorKind::Unsupported,
            "N12 supports only final quality",
        ));
    }
    if output.kind() != "frame"
        || output.format() != "rgba8"
        || output.dimensions() != (320, 180)
        || output.color_interpretation() != "srgb"
        || output.alpha_mode() != "straight"
    {
        return Err(RenderSceneError::new(
            RenderSceneErrorKind::Unsupported,
            "N12 supports only final frame/rgba8/320x180/srgb/straight output",
        ));
    }
    Ok(())
}
