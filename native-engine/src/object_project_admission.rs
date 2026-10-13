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
    ProjectWireLimits::new(ENVELOPE_BYTES, DRAWING_BYTES)
        .unwrap()
        .with_png_limits(
            ENCODED_BYTES,
            RGBA_BYTES,
            DECODER_BYTES,
            DIMENSION,
            DIMENSION,
        )
        .unwrap()
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
    pub(crate) fn admit_project_file(
        instance: String,
        bytes: &[u8],
    ) -> Result<Self, DispatchErrorCode> {
        Self::from_managed_project_wire(instance, bytes, wire_limits()).map_err(refused)
    }

    pub(crate) fn serialize_project_file(&self) -> Result<Vec<u8>, DispatchErrorCode> {
        let project = self.project().ok_or(DispatchErrorCode::Unavailable)?;
        encode_project_envelope(project, wire_limits()).map_err(refused)
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
