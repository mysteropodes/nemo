//! Private, read-only request pins for a future native Order expression runtime.
//! This module neither executes code nor publishes an evaluated value.
use crate::evaluation::property_expression_runtime::Dim;
#[cfg(feature = "object_expression_runtime")]
use crate::evaluation::property_expression_runtime::{
    evaluate_with_fallback, publish_if_current, Diagnostic, Publish,
};
use crate::history::NativeObjectHistory;
use crate::object_document::{FrameScopeKind, PropertyHolderTarget};
#[cfg(feature = "object_expression_runtime")]
use crate::object_expression_runtime::{DetachedCandidate, ExpressionRefusal, PropertyCandidate};
use crate::object_frame_packet::{
    raw_expression_numeric, raw_expression_order, sample_property_raw, ObjectFrameError,
    ObjectFrameSelector, PropertySampleError,
};
use crate::revision::ObjectSnapshot;

pub(crate) struct ExpressionSelector<'a> {
    pub(crate) frame: ObjectFrameSelector<'a>,
    pub(crate) source_frame: u32,
    pub(crate) layer_uid: &'a str,
    /// None selects layer Order; Some selects one actual scoped stroke's Order.
    pub(crate) stroke_id: Option<&'a str>,
}

/// Its snapshot is retained for read-only provenance, never a writable owner.
pub(crate) struct ExpressionRequest {
    snapshot: ObjectSnapshot,
    context_id: String,
    scope_kind: FrameScopeKind,
    source_frame: u32,
    evaluated_frame: u32,
    layer_uid: String,
    stroke_id: Option<String>,
    code: String,
    raw_numeric: f64,
}

impl ExpressionRequest {
    pub(crate) fn snapshot(&self) -> &ObjectSnapshot {
        &self.snapshot
    }
    pub(crate) fn context_id(&self) -> &str {
        &self.context_id
    }
    pub(crate) fn scope_kind(&self) -> &FrameScopeKind {
        &self.scope_kind
    }
    pub(crate) fn source_frame(&self) -> u32 {
        self.source_frame
    }
    pub(crate) fn evaluated_frame(&self) -> u32 {
        self.evaluated_frame
    }
    pub(crate) fn layer_uid(&self) -> &str {
        &self.layer_uid
    }
    pub(crate) fn stroke_id(&self) -> Option<&str> {
        self.stroke_id.as_deref()
    }
    pub(crate) fn code(&self) -> &str {
        &self.code
    }
    pub(crate) fn raw_numeric(&self) -> f64 {
        self.raw_numeric
    }

    /// Mandatory final check immediately before a future runtime publishes.
    /// A retained old snapshot is never authority for the current owner head.
    pub(crate) fn revalidate(&self, owner: &NativeObjectHistory) -> Result<(), ObjectFrameError> {
        let current = owner
            .acquire_snapshot(owner.content_revision())
            .ok_or(ObjectFrameError::Unavailable)?;
        verify_owner_snapshot(owner, &current, &self.snapshot)
    }

    /// Only this finalizer can extract a scalar from a fully retired request.
    /// A real owner advance during engine work invalidates even a finite result.
    #[cfg(feature = "object_expression_runtime")]
    pub(crate) fn finalize(
        self,
        owner: &NativeObjectHistory,
        candidate: DetachedCandidate,
    ) -> Result<f64, ExpressionRefusal> {
        self.revalidate(owner)?;
        candidate.into_outcome()
    }
}

pub(super) fn verify_owner_snapshot(
    owner: &NativeObjectHistory,
    current: &ObjectSnapshot,
    supplied: &ObjectSnapshot,
) -> Result<(), ObjectFrameError> {
    if supplied.instance_id() != owner.instance_id() {
        return Err(ObjectFrameError::WrongInstance);
    }
    if supplied.document_id() != owner.document_id() {
        return Err(ObjectFrameError::WrongDocument);
    }
    if supplied.content_revision() != owner.content_revision() {
        return Err(ObjectFrameError::WrongRevision);
    }
    if supplied.snapshot_id() != current.snapshot_id()
        || !std::ptr::eq(supplied.document(), current.document())
    {
        return Err(ObjectFrameError::WrongSnapshot);
    }
    Ok(())
}

pub(crate) fn prepare_expression_request(
    owner: &NativeObjectHistory,
    snapshot: &ObjectSnapshot,
    selector: &ExpressionSelector<'_>,
) -> Result<ExpressionRequest, ObjectFrameError> {
    let current = owner
        .acquire_snapshot(owner.content_revision())
        .ok_or(ObjectFrameError::Unavailable)?;
    verify_owner_snapshot(owner, &current, snapshot)?;
    let frame = &selector.frame;
    if frame.instance_id != snapshot.instance_id() {
        return Err(ObjectFrameError::WrongInstance);
    }
    if frame.document_id != snapshot.document_id() {
        return Err(ObjectFrameError::WrongDocument);
    }
    if frame.content_revision != snapshot.content_revision() {
        return Err(ObjectFrameError::WrongRevision);
    }
    if frame.document_snapshot_id != snapshot.snapshot_id() {
        return Err(ObjectFrameError::WrongSnapshot);
    }
    if frame.frame >= snapshot.document().total_frames() {
        return Err(ObjectFrameError::FrameOutOfRange);
    }
    let order = raw_expression_order(
        snapshot,
        frame.context_id,
        &frame.scope_kind,
        selector.source_frame,
        selector.layer_uid,
        selector.stroke_id,
    )?;
    let expression = order
        .expression
        .value()
        .filter(|expression| expression.enabled && !expression.code.is_empty())
        .ok_or(ObjectFrameError::Unavailable)?;
    let raw_numeric = raw_expression_numeric(order, frame.frame)?;
    Ok(ExpressionRequest {
        snapshot: snapshot.clone(),
        context_id: frame.context_id.to_owned(),
        scope_kind: frame.scope_kind.clone(),
        source_frame: selector.source_frame,
        evaluated_frame: frame.frame,
        layer_uid: selector.layer_uid.to_owned(),
        stroke_id: selector.stroke_id.map(str::to_owned),
        code: expression.code.clone(),
        raw_numeric,
    })
}

/// `PROP_DIM` as `src/js/motion.js` declares it; a builtin outside it is
/// `undefined` there (trim, paint, volume), and controls carry their own.
const PROP_DIM: &[(&str, usize)] = &[
    ("position", 2),
    ("anchor", 2),
    ("rotation", 1),
    ("scale", 2),
    ("opacity", 1),
    ("order", 1),
    ("timeRemap", 1),
    ("positionZ", 1),
    ("rotationX", 1),
    ("rotationY", 1),
    ("dupOffsetPos", 2),
    ("dupOffsetRot", 1),
    ("dupOffsetScale", 2),
    ("dupOffsetOpacity", 1),
    ("dupOffsetHue", 1),
    ("dupOffsetPosZ", 1),
    ("dupOffsetRotX", 1),
    ("dupOffsetRotY", 1),
    ("parentBlend", 1),
    ("matteOn", 1),
    ("timeLinkInOffset", 1),
    ("timeLinkOutOffset", 1),
    ("cornerTL", 1),
    ("cornerTR", 1),
    ("cornerBR", 1),
    ("cornerBL", 1),
    ("arcStart", 1),
    ("arcSweep", 1),
    ("arcInner", 1),
    ("starInner", 1),
    ("starCorner", 1),
    ("pathPercent", 1),
    ("pathInfluence", 1),
];

/// One typed Motion property expression, pinned to one retained snapshot.
pub(crate) struct PropertyExpressionRequest {
    snapshot: ObjectSnapshot,
    target: PropertyHolderTarget,
    property: String,
    frame: f64,
    depth: u32,
    code: String,
    raw: Vec<f64>,
    dim: Dim,
}

impl PropertyExpressionRequest {
    pub(crate) fn snapshot(&self) -> &ObjectSnapshot {
        &self.snapshot
    }
    pub(crate) fn target(&self) -> &PropertyHolderTarget {
        &self.target
    }
    pub(crate) fn property(&self) -> &str {
        &self.property
    }
    pub(crate) fn frame(&self) -> f64 {
        self.frame
    }
    pub(crate) fn depth(&self) -> u32 {
        self.depth
    }
    pub(crate) fn code(&self) -> &str {
        &self.code
    }
    pub(crate) fn raw(&self) -> &[f64] {
        &self.raw
    }
    pub(crate) fn dim(&self) -> Dim {
        self.dim
    }
    pub(crate) fn revalidate(&self, owner: &NativeObjectHistory) -> Result<(), ObjectFrameError> {
        let current = owner
            .acquire_snapshot(owner.content_revision())
            .ok_or(ObjectFrameError::Unavailable)?;
        verify_owner_snapshot(owner, &current, &self.snapshot)
    }

    /// The owner check, then the stale gate, then E1's fallback. A timeout
    /// only prepares the disable; committing it belongs to the real owner.
    #[cfg(feature = "object_expression_runtime")]
    pub(crate) fn finalize(
        self,
        owner: &NativeObjectHistory,
        candidate: PropertyCandidate,
    ) -> Result<PropertyEvaluation, ExpressionRefusal> {
        let current = owner
            .acquire_snapshot(owner.content_revision())
            .ok_or(ObjectFrameError::Unavailable)?;
        verify_owner_snapshot(owner, &current, &self.snapshot)?;
        let revision = self.snapshot.content_revision();
        let (outcome, dependencies) =
            match publish_if_current(revision, current.content_revision(), candidate) {
                Publish::Publish(candidate) => candidate.into_parts(),
                Publish::Stale => return Err(ObjectFrameError::WrongRevision.into()),
            };
        let mut fallback = evaluate_with_fallback(self.dim, &self.raw, outcome?);
        if fallback.diagnostic.is_none()
            && dependencies.iter().any(|d| {
                d.diagnostic == Some(Diagnostic::TooDeep)
                    && d.target == self.target
                    && d.property == self.property
            })
        {
            fallback.diagnostic = Some(Diagnostic::TooDeep);
        }
        Ok(PropertyEvaluation {
            value: fallback.value,
            diagnostic: fallback.diagnostic,
            disable: fallback.disable.then_some(PropertyDisableRequest {
                content_revision: revision,
                target: self.target,
                property: self.property,
            }),
            dependencies,
        })
    }
}

/// Detached result: the published value and what Motion would report.
#[cfg(feature = "object_expression_runtime")]
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PropertyEvaluation {
    pub(crate) value: Vec<f64>,
    pub(crate) diagnostic: Option<Diagnostic>,
    pub(crate) disable: Option<PropertyDisableRequest>,
    pub(crate) dependencies: Vec<PropertyDependencyEffect>,
}

/// Inner diagnostics and timeout-disable requests retain their real target.
/// Reading never commits them; the existing owner publishes them with the
/// same final revision check as the outer result.
#[cfg(feature = "object_expression_runtime")]
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PropertyDependencyEffect {
    pub(crate) target: PropertyHolderTarget,
    pub(crate) property: String,
    pub(crate) diagnostic: Option<Diagnostic>,
    pub(crate) disable: bool,
}

/// Motion's `ex.enabled = false` after a timeout, as data for the owner.
#[cfg(feature = "object_expression_runtime")]
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PropertyDisableRequest {
    pub(crate) content_revision: u64,
    pub(crate) target: PropertyHolderTarget,
    pub(crate) property: String,
}

pub(crate) fn prepare_property_expression_request(
    owner: &NativeObjectHistory,
    snapshot: &ObjectSnapshot,
    target: &PropertyHolderTarget,
    property: &str,
    frame: f64,
    depth: u32,
) -> Result<PropertyExpressionRequest, ObjectFrameError> {
    let current = owner
        .acquire_snapshot(owner.content_revision())
        .ok_or(ObjectFrameError::Unavailable)?;
    verify_owner_snapshot(owner, &current, snapshot)?;
    prepare_retained_property_expression_request(snapshot, target, property, frame, depth)
}

/// A dependency read stays on the outer request's immutable snapshot. Only
/// the outer request's owner finalizer can publish any of its results.
pub(crate) fn prepare_retained_property_expression_request(
    snapshot: &ObjectSnapshot,
    target: &PropertyHolderTarget,
    property: &str,
    frame: f64,
    depth: u32,
) -> Result<PropertyExpressionRequest, ObjectFrameError> {
    let raw = sample_property_raw(snapshot, target, property, frame).map_err(|e| match e {
        PropertySampleError::InvalidFrame => ObjectFrameError::FrameOutOfRange,
        PropertySampleError::MissingTarget => ObjectFrameError::InvalidSource,
        PropertySampleError::UnsupportedProperty | PropertySampleError::Unavailable => {
            ObjectFrameError::Unavailable
        }
    })?;
    let holder = snapshot
        .document()
        .property_holders()
        .unwrap_or(&[])
        .iter()
        .find(|holder| holder.target() == target)
        .ok_or(ObjectFrameError::Unavailable)?;
    let code = holder
        .properties()
        .iter()
        .find(|record| record.property() == property)
        .and_then(|record| record.expression.value())
        .filter(|expression| expression.enabled && !expression.code.is_empty())
        .map(|expression| expression.code.clone())
        .ok_or(ObjectFrameError::Unavailable)?;
    let control = holder
        .controls
        .value()
        .into_iter()
        .flatten()
        .any(|c| c.key == property);
    let dim = match PROP_DIM.iter().find(|(name, _)| *name == property) {
        Some((_, n)) => Some(*n),
        None if control => holder.metadata(property).map(|meta| meta.dimension()),
        None => None,
    };
    Ok(PropertyExpressionRequest {
        snapshot: snapshot.clone(),
        target: target.clone(),
        property: property.to_owned(),
        frame,
        depth,
        code,
        raw,
        dim: Dim::for_prop(property, dim),
    })
}
