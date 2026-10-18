//! Strict file/PNG admission and serialization through immutable project reads.
use crate::object_codec::{
    encode_project_envelope, ImageAssetLimits, ProjectReferenceState, ProjectWireError,
    ProjectWireLimits,
};
use crate::object_snapshot::{
    shared_image_resource_budget, validate_png_in_domain, ObjectSnapshot,
};
use crate::request_receipts::DispatchErrorCode;
use serde::Deserialize;
use std::sync::Arc;

const ENVELOPE_BYTES: usize = 32 * 1024 * 1024;
const DRAWING_BYTES: usize = 8 * 1024 * 1024;
const ENCODED_BYTES: usize = 16 * 1024 * 1024;
const RGBA_BYTES: usize = 256 * 1024 * 1024;
const DECODER_BYTES: usize = 64 * 1024 * 1024;
const DIMENSION: u32 = 8192;

fn wire_limits() -> ProjectWireLimits {
    let limits = ProjectWireLimits::new(ENVELOPE_BYTES, DRAWING_BYTES)
        .unwrap()
        .with_png_limits(
            ENCODED_BYTES,
            RGBA_BYTES,
            DECODER_BYTES,
            DIMENSION,
            DIMENSION,
        )
        .unwrap();
    #[cfg(feature = "font_asset_wire")]
    let limits = limits
        .with_font_limits(crate::object_codec::FontAssetLimits::new(ENCODED_BYTES, 256).unwrap());
    limits
}
fn png_limits() -> ImageAssetLimits {
    ImageAssetLimits::new(
        ENCODED_BYTES,
        RGBA_BYTES,
        DECODER_BYTES,
        DIMENSION,
        DIMENSION,
    )
    .unwrap()
}
fn refused(error: ProjectWireError) -> DispatchErrorCode {
    match error {
        ProjectWireError::Budget | ProjectWireError::Unsupported => DispatchErrorCode::Unavailable,
        _ => DispatchErrorCode::InvalidRequest,
    }
}

/// Construction is confined to strict PNG admission. No public DTO can forge it.
pub(crate) struct PreparedReferenceAttachment {
    origin: ObjectSnapshot,
    reference: ProjectReferenceState,
    changed: bool,
}
impl PreparedReferenceAttachment {
    pub(crate) fn origin(&self) -> &ObjectSnapshot {
        &self.origin
    }
    pub(crate) fn changed(&self) -> bool {
        self.changed
    }
    pub(crate) fn into_reference(self) -> ProjectReferenceState {
        self.reference
    }
}

impl ObjectSnapshot {
    /// Trusted host supplies bytes separately from the bounded scalar request.
    /// This value is not a public DTO, identity publisher or another owner.
    #[cfg(feature = "font_asset_wire")]
    pub(crate) fn admit_font_asset(
        &self,
        id: String,
        name: String,
        source: &[u8],
    ) -> Result<Arc<crate::object_codec::AssetRecord>, DispatchErrorCode> {
        self.admit_font_asset_using(
            id,
            name,
            source,
            Self::reserve_encoded,
            |p| p.allocate().map_err(|_| "encoded allocation unavailable"),
            |p, a| p.bind(a).map_err(|_| "encoded binding unavailable"),
        )
    }
    #[cfg(feature = "font_asset_wire")]
    pub(crate) fn admit_font_asset_using<R>(
        &self,
        id: String,
        name: String,
        source: &[u8],
        reserve: impl FnOnce(usize) -> Result<R, &'static str>,
        allocate: impl FnOnce(&mut R) -> Result<Vec<u8>, &'static str>,
        bind: impl FnOnce(&mut R, &Arc<Vec<u8>>) -> Result<(), &'static str>,
    ) -> Result<Arc<crate::object_codec::AssetRecord>, DispatchErrorCode> {
        use crate::object_codec::{AssetId, AssetKind, AssetRecord};
        let invalid = DispatchErrorCode::InvalidRequest;
        let unavailable = DispatchErrorCode::Unavailable;
        let project = self.project().ok_or(unavailable)?;
        if project
            .asset_inventory()
            .is_some_and(|a| a.records().len() >= 1024 || !a.placements().is_empty())
        {
            return Err(unavailable);
        }
        let id = AssetId::new(id).map_err(|_| invalid)?;
        if name.trim().is_empty() || name.len() > 255 || name.contains('\0') {
            return Err(invalid);
        }
        if project
            .asset_inventory()
            .is_some_and(|a| a.records().iter().any(|r| r.id() == &id))
        {
            return Err(DispatchErrorCode::BusyConflict);
        }
        let limits = crate::object_codec::FontAssetLimits::new(ENCODED_BYTES, 256).unwrap();
        let font_error = |error| match error {
            crate::object_codec::FontAssetError::Invalid => invalid,
            _ => unavailable,
        };
        crate::object_codec::validate_font(source, limits).map_err(font_error)?;
        // Reservation precedes the only retained byte copy; exact original Arc
        // is bound before native catalog publication. Failed paths drop it.
        let mut permit = reserve(source.len()).map_err(|_| unavailable)?;
        let mut bytes = allocate(&mut permit).map_err(|_| unavailable)?;
        if bytes.len() != source.len() || bytes.capacity() != source.len() {
            return Err(unavailable);
        }
        bytes.copy_from_slice(source);
        let bytes = Arc::new(bytes);
        let source = crate::object_codec::admitted_font_source(Arc::clone(&bytes), limits)
            .map_err(font_error)?;
        let record =
            Arc::new(AssetRecord::new(id, name, AssetKind::Font, source).map_err(|_| invalid)?);
        bind(&mut permit, &bytes).map_err(|_| unavailable)?;
        Ok(record)
    }
    pub(crate) fn admit_project_file(
        instance: String,
        bytes: &[u8],
    ) -> Result<Self, DispatchErrorCode> {
        Self::from_managed_project_wire(instance, bytes, wire_limits()).map_err(refused)
    }

    pub fn serialize_project_file(&self) -> Result<Vec<u8>, DispatchErrorCode> {
        let project = self.project().ok_or(DispatchErrorCode::Unavailable)?;
        encode_project_envelope(project, wire_limits()).map_err(refused)
    }

    /// L06-H: dry-run of `serialize_project_file` on a candidate project that has
    /// not been published. Same encoder, same limits; the bytes are discarded.
    pub(crate) fn check_candidate_saveable(
        candidate: &crate::object_codec::ProjectEnvelope,
    ) -> Result<(), DispatchErrorCode> {
        encode_project_envelope(candidate, wire_limits())
            .map(|_| ())
            .map_err(refused)
    }

    pub(crate) fn prepare_reference_attachment(
        &self,
        id: String,
        name: String,
        source: &[u8],
    ) -> Result<PreparedReferenceAttachment, DispatchErrorCode> {
        self.prepare_reference_attachment_using(
            id,
            name,
            source,
            Self::reserve_encoded,
            |p| p.allocate().map_err(|_| "encoded allocation unavailable"),
            |p, a| p.bind(a).map_err(|_| "encoded binding unavailable"),
        )
    }

    // Generic producer ports are crate-private for negative allocation/lifetime
    // controls. The production caller above always selects the shared domain.
    pub(crate) fn prepare_reference_attachment_using<R>(
        &self,
        id: String,
        name: String,
        source: &[u8],
        reserve: impl FnOnce(usize) -> Result<R, &'static str>,
        allocate: impl FnOnce(&mut R) -> Result<Vec<u8>, &'static str>,
        bind: impl FnOnce(&mut R, &Arc<Vec<u8>>) -> Result<(), &'static str>,
    ) -> Result<PreparedReferenceAttachment, DispatchErrorCode> {
        let before = self
            .project()
            .ok_or(DispatchErrorCode::Unavailable)?
            .reference_state();
        validate_png_in_domain(source, png_limits(), shared_image_resource_budget())
            .map_err(refused)?;
        // Declaration order preserves charge until every unbound byte is gone,
        // including validation errors, binding refusal and stack unwinding.
        let mut permit = reserve(source.len()).map_err(|_| DispatchErrorCode::Unavailable)?;
        let mut bytes = allocate(&mut permit).map_err(|_| DispatchErrorCode::Unavailable)?;
        if bytes.len() != source.len() || bytes.capacity() != source.len() {
            return Err(DispatchErrorCode::Unavailable);
        }
        bytes.copy_from_slice(source);
        let (visible, opacity, offset) = match &before {
            ProjectReferenceState::NoMedia => (true, 0.5, 0),
            ProjectReferenceState::StillImage(previous) => (
                previous.visible(),
                previous.opacity(),
                previous.offset_frames(),
            ),
        };
        let reference = ProjectReferenceState::admitted_still(
            id,
            name,
            visible,
            opacity,
            offset,
            bytes,
            ENCODED_BYTES,
        )
        .map_err(|_| DispatchErrorCode::InvalidRequest)?;
        let ProjectReferenceState::StillImage(data) = &reference else {
            unreachable!("strict still-image factory returned no media")
        };
        bind(&mut permit, data.encoded_allocation()).map_err(|_| DispatchErrorCode::Unavailable)?;
        Ok(PreparedReferenceAttachment {
            origin: self.clone(),
            changed: reference != before,
            reference,
        })
    }
}

// D26-P1: detached root-composition reads share the original immutable snapshot.
#[derive(serde::Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
enum CompositionSelector {
    Settings {},
    MarkerAt { index: usize },
    MarkerNamed { name: String },
    MarkerNearest { frame: f64 },
}
struct CompositionRead {
    command: String,
    instance_id: String,
    document_id: String,
    at_revision: u64,
    document_snapshot_id: String,
    context_id: String,
    selector: CompositionSelector,
}
crate::object_document::object_deserialize!(CompositionRead {
    command: String,
    instance_id: String,
    document_id: String,
    at_revision: u64,
    document_snapshot_id: String,
    context_id: String,
    selector: CompositionSelector
});
impl ObjectSnapshot {
    /// Original-byte, bounded query only. No dispatcher/public capability activation.
    pub(crate) fn query_composition_metadata_json(
        &self,
        bytes: &[u8],
    ) -> Result<serde_json::Value, DispatchErrorCode> {
        use DispatchErrorCode::{InvalidRequest, NotFound, StaleRevision, Unavailable};
        if bytes.len() > 4096 {
            return Err(InvalidRequest);
        }
        let shape: serde_json::Value = serde_json::from_slice(bytes).map_err(|_| InvalidRequest)?;
        if !shape
            .get("selector")
            .is_some_and(serde_json::Value::is_object)
        {
            return Err(InvalidRequest);
        }
        let query: CompositionRead = serde_json::from_slice(bytes).map_err(|_| InvalidRequest)?;
        if query.command != "object.project.composition.read" {
            return Err(InvalidRequest);
        }
        if query.instance_id != self.instance_id() {
            return Err(DispatchErrorCode::WrongInstance);
        }
        if query.document_id != self.document_id() {
            return Err(DispatchErrorCode::WrongDocument);
        }
        if query.at_revision != self.content_revision()
            || query.document_snapshot_id != self.snapshot_id()
        {
            return Err(StaleRevision);
        }
        if query.context_id != "scene-root" {
            return Err(Unavailable);
        }
        match &query.selector {
            CompositionSelector::MarkerAt { index: 0 } => return Err(InvalidRequest),
            CompositionSelector::MarkerNamed { name }
                if name.len() > 1024 || name.contains('\0') =>
            {
                return Err(InvalidRequest)
            }
            CompositionSelector::MarkerNearest { frame } if !frame.is_finite() => {
                return Err(InvalidRequest);
            }
            _ => {}
        }
        let metadata = self.project().ok_or(Unavailable)?.project();
        let markers = metadata.composition_markers();
        let result = match query.selector {
            CompositionSelector::Settings {} => {
                let (bpm, offset, show) = metadata.composition_bpm();
                let mut value = serde_json::json!({
                    "markersPresent": markers.is_some(),
                    "markerCount": markers.map_or(0, |values| values.len()),
                    "effectiveBpm": bpm.unwrap_or(120.0),
                    "effectiveBpmOffset": offset.unwrap_or(0.0),
                    "effectiveBpmShow": show.unwrap_or(false)
                });
                if let Some(v) = bpm {
                    value["bpm"] = v.into();
                }
                if let Some(v) = offset {
                    value["bpmOffset"] = v.into();
                }
                if let Some(v) = show {
                    value["bpmShow"] = v.into();
                }
                value
            }
            selector => {
                let values = markers.ok_or(NotFound)?;
                let index = match selector {
                    CompositionSelector::MarkerAt { index } => index
                        .checked_sub(1)
                        .filter(|i| *i < values.len())
                        .ok_or(NotFound)?,
                    CompositionSelector::MarkerNamed { name } => values
                        .iter()
                        .position(|m| m.1.as_deref().unwrap_or("") == name)
                        .ok_or(NotFound)?,
                    CompositionSelector::MarkerNearest { frame } => {
                        if values.is_empty() {
                            return Err(NotFound);
                        }
                        // Match first-stored JS ties, including all-overflow distances.
                        let mut best = 0;
                        let mut distance = f64::INFINITY;
                        for (i, marker) in values.iter().enumerate() {
                            let candidate = (marker.0 - frame).abs();
                            if candidate < distance {
                                best = i;
                                distance = candidate;
                            }
                        }
                        best
                    }
                    CompositionSelector::Settings {} => unreachable!(),
                };
                let (frame, name, color) = &values[index];
                let time = frame / metadata.fps();
                if !time.is_finite() {
                    return Err(Unavailable);
                }
                serde_json::json!({"index": index + 1, "frame": frame, "time": time,
                    "name": name.as_deref().unwrap_or(""),
                    "color": color.as_deref().filter(|v| !v.is_empty())})
            }
        };
        let response = serde_json::json!({"instanceId": self.instance_id(),
            "documentId": self.document_id(), "atRevision": self.content_revision(),
            "documentSnapshotId": self.snapshot_id(), "contextId": "scene-root", "result": result});
        if serde_json::to_vec(&response)
            .map_err(|_| Unavailable)?
            .len()
            > 4096
        {
            return Err(Unavailable);
        }
        Ok(response)
    }
}
