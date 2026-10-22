//! Strict file/PNG admission and serialization through immutable project reads.
use crate::object_codec::{
    encode_project_envelope, ImageAssetLimits, ProjectReferenceState, ProjectWireError,
    ProjectWireLimits,
};
use crate::object_snapshot::{
    shared_image_resource_budget, validate_png_in_domain, ObjectSnapshot,
};
use crate::request_receipts::DispatchErrorCode;
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
    let limits = limits.with_audio_limit(ENCODED_BYTES).unwrap();
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
    /// Validate the command-prepared order plan through the existing codec.
    /// The detached admitted document is returned without publication.
    pub(crate) fn migrate_import_object_order(
        &self,
        document: super::ObjectDocument,
        plan_bytes: &[u8],
    ) -> Result<super::ObjectDocument, DispatchErrorCode> {
        crate::object_codec::migrate_ordered_project(&document, plan_bytes)
            .map(|ordered| ordered.into_storage())
            .map_err(|_| DispatchErrorCode::InvalidRequest)
    }

    /// Validate borrowed import PNG bytes with the shared charged workspace.
    /// No encoded allocation is copied or retained by this immutable read.
    pub(crate) fn validate_import_png(&self, source: &[u8]) -> Result<(), DispatchErrorCode> {
        let limits = ImageAssetLimits::new(
            ENCODED_BYTES,
            RGBA_BYTES,
            DECODER_BYTES,
            DIMENSION,
            DIMENSION,
        )
        .map_err(|_| DispatchErrorCode::Internal)?;
        validate_png_in_domain(source, limits, shared_image_resource_budget()).map_err(|error| {
            match error {
                ProjectWireError::Budget => DispatchErrorCode::Unavailable,
                _ => DispatchErrorCode::InvalidRequest,
            }
        })
    }

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

#[path = "object_project_composition_read.rs"]
mod composition;
#[path = "object_project_custom_brush_read.rs"]
mod custom_brush_read;
#[path = "object_project_guide_read.rs"]
mod guide_read;
#[path = "object_project_palette_read.rs"]
mod palette_read;
#[path = "object_project_perspective_read.rs"]
mod perspective_read;
#[path = "object_project_shadow_read.rs"]
mod shadow_read;
#[path = "object_project_workspace_read.rs"]
mod workspace_read;

#[cfg(all(feature = "history", feature = "project_media_codec"))]
#[path = "object_project_comments_read.rs"]
mod comments_read;

#[cfg(all(test, feature = "history"))]
#[path = "../tests/object_palette_read.rs"]
mod palette_read_tests;
