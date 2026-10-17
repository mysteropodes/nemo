//! Typed preparation of an asset attach / remove from one immutable snapshot
//! (L06-H, G01 / G02 / G27): the read-queries half of the transaction.
//!
//! `prepare_asset_attachment` runs the document-layer admission policy
//! (`object_raster_asset_admission::decide`) with two injected ports - the accepted
//! SHA-256 and a PNG validity check that charges its decoder scratch to the shared
//! image-payload budget - and, only for a genuinely NEW asset, reserves the encoded
//! bytes in the shared encoded-retention ledger, copies the borrowed input into the
//! reserved allocation and builds the immutable `AssetRecord`. An exact duplicate
//! of a held asset prepares to `Unchanged` and touches no budget. Every refusal
//! happens before, or releases, every charge: the permit is declared before the
//! buffer and the record, so the charge outlives every unbound byte (the discipline
//! of `prepare_reference_attachment_using`).
//!
//! The result is a sealed `PreparedAssetEdit` that pins the origin snapshot; the
//! command owner (`object_raster_asset_owner`) re-checks the pins at commit and the
//! revision store (`object_raster_asset_attach`) publishes. Nothing here mutates a
//! document, history or receipt store.
//!
//! Layering: this file is in the read-queries layer, which may not import the
//! document layer, so the document names it needs come through two module aliases
//! in the codec file (`object_codec.rs`): the admission kernel, its PNG port (for the
//! decoder bound and the error-class map, so they cannot drift from the kernel's own)
//! and the in-use predicate module, which also re-exports the asset value types.
//!
//! A NEW asset is also dry-run through the ONE save path (the next project is encoded
//! with the same wire limits `serialize_project_file` uses), so a state that could not
//! be saved - the viewport-reference id collision, the 32 MiB envelope, the 1024 wire
//! entries that count a reference outside the catalog - is refused before anything is
//! bound or published. Removal is never dry-run: it can only make a project smaller.
//!
//! The `_using` entry exposes the three byte-reservation steps and the CPU budget so a
//! test can measure private ledgers.
use crate::object_codec::raster_asset_admission::{
    decide, AdmissionDecision, AdmissionPorts, AdmissionRequest, PngCheck, Refusal, MAX_ASSETS,
    MAX_DIMENSION, MAX_ENCODED_BYTES, MAX_RGBA_BYTES,
};
use crate::object_codec::raster_asset_png_port::{map_error, MAX_DECODER_BYTES};
use crate::object_codec::raster_asset_use::{
    asset_in_use, AssetId, AssetInventory, AssetKind, AssetRecord, AssetSource,
};
use crate::object_codec::{
    validate_png_with_buffers, ImageAssetError, ImageAssetLimits, PngWorkspace,
    ProjectReferenceState,
};
use crate::object_snapshot::charged_png_workspace;
use crate::request_receipts::{sha256, DispatchErrorCode};
use crate::revision::{shared_image_resource_budget, ImageResourceBudget, ObjectSnapshot};
use std::sync::Arc;

/// A typed refusal: the dispatch code plus a stable machine reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AssetRefusal {
    code: DispatchErrorCode,
    reason: &'static str,
}
impl AssetRefusal {
    fn new(code: DispatchErrorCode, reason: &'static str) -> Self {
        Self { code, reason }
    }
    fn resource() -> Self {
        Self::new(DispatchErrorCode::Unavailable, "resource_unavailable")
    }
    fn bare() -> Self {
        Self::new(DispatchErrorCode::Unavailable, "bare_drawing")
    }
    pub(crate) fn code(&self) -> DispatchErrorCode {
        self.code
    }
    pub(crate) fn reason(&self) -> &'static str {
        self.reason
    }
    /// One row per admission refusal; nothing is collapsed into a generic error.
    fn from_admission(refusal: &Refusal) -> Self {
        use DispatchErrorCode::{BusyConflict, InvalidRequest, Unavailable};
        match refusal {
            Refusal::Empty => Self::new(InvalidRequest, "empty"),
            Refusal::Oversize => Self::new(InvalidRequest, "oversize"),
            Refusal::UnrecognizedSignature => Self::new(InvalidRequest, "unrecognized_signature"),
            Refusal::TypeMismatch { .. } => Self::new(InvalidRequest, "type_mismatch"),
            Refusal::Name => Self::new(InvalidRequest, "name"),
            Refusal::RequestId => Self::new(InvalidRequest, "request_id"),
            Refusal::DecoderUnavailable { .. } => Self::new(Unavailable, "decoder_unavailable"),
            Refusal::Corrupt => Self::new(InvalidRequest, "corrupt"),
            Refusal::UnsupportedEncoding => Self::new(Unavailable, "unsupported_encoding"),
            Refusal::ImageTooLarge => Self::new(Unavailable, "image_too_large"),
            Refusal::ResourceUnavailable => Self::resource(),
            Refusal::IdCollision => Self::new(BusyConflict, "id_collision"),
            Refusal::CatalogFull => Self::new(Unavailable, "catalog_full"),
        }
    }
}

/// What a prepared edit will do to the catalog.
#[derive(Debug)]
pub(crate) enum AssetEdit {
    /// A new embedded image record, bytes already reserved and bound.
    Add(Arc<AssetRecord>),
    /// The asset to drop; it exists, is an image and is not in use.
    Remove(AssetId),
    /// Nothing to publish: an exact duplicate, or removal of an absent id.
    Unchanged,
}

/// Sealed one-shot value: only this file builds it, and it pins its origin.
#[derive(Debug)]
pub(crate) struct PreparedAssetEdit {
    origin: ObjectSnapshot,
    edit: AssetEdit,
}
impl PreparedAssetEdit {
    pub(crate) fn origin(&self) -> &ObjectSnapshot {
        &self.origin
    }
    pub(crate) fn changed(&self) -> bool {
        !matches!(self.edit, AssetEdit::Unchanged)
    }
    pub(crate) fn into_edit(self) -> AssetEdit {
        self.edit
    }
}

enum CheckError {
    Png(ImageAssetError),
    Workspace,
}
impl From<ImageAssetError> for CheckError {
    fn from(error: ImageAssetError) -> Self {
        Self::Png(error)
    }
}

/// Strict PNG validity under the accepted admission bounds. The decoder scratch
/// is charged to `cpu` and returned before this function does.
fn charged_png_check_in(bytes: &[u8], cpu: &ImageResourceBudget) -> PngCheck {
    let Ok(limits) = ImageAssetLimits::new(
        MAX_ENCODED_BYTES,
        MAX_RGBA_BYTES,
        MAX_DECODER_BYTES,
        MAX_DIMENSION,
        MAX_DIMENSION,
    ) else {
        return PngCheck::Unavailable;
    };
    let checked = validate_png_with_buffers(
        bytes,
        limits,
        || {
            charged_png_workspace(cpu, || Ok(PngWorkspace::new()))
                .map_err(|_| CheckError::Workspace)
        },
        |n| cpu.allocate_bytes(n).map_err(|_| CheckError::Workspace),
    );
    match checked {
        Ok((width, height)) => PngCheck::Valid { width, height },
        Err(CheckError::Workspace) => PngCheck::Unavailable,
        Err(CheckError::Png(error)) => map_error(error),
    }
}

impl ObjectSnapshot {
    pub(crate) fn prepare_asset_attachment(
        &self,
        request_id: &str,
        name: &str,
        declared_media_type: &str,
        bytes: &[u8],
    ) -> Result<PreparedAssetEdit, AssetRefusal> {
        self.prepare_asset_attachment_using(
            request_id,
            name,
            declared_media_type,
            bytes,
            shared_image_resource_budget(),
            Self::reserve_encoded,
            |p| p.allocate().map_err(|_| "encoded allocation unavailable"),
            |p, a| p.bind(a).map_err(|_| "encoded binding unavailable"),
        )
    }

    // Crate-private ports exist for negative allocation / lifetime controls. The
    // production caller above always selects the shared ledgers.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn prepare_asset_attachment_using<R>(
        &self,
        request_id: &str,
        name: &str,
        declared_media_type: &str,
        bytes: &[u8],
        cpu: &ImageResourceBudget,
        reserve: impl FnOnce(usize) -> Result<R, &'static str>,
        allocate: impl FnOnce(&mut R) -> Result<Vec<u8>, &'static str>,
        bind: impl FnOnce(&mut R, &Arc<Vec<u8>>) -> Result<(), &'static str>,
    ) -> Result<PreparedAssetEdit, AssetRefusal> {
        let project = self.project().ok_or_else(AssetRefusal::bare)?;
        let held;
        let inventory = match project.asset_inventory() {
            Some(inventory) => inventory,
            None => {
                held = AssetInventory::new(Vec::new(), Vec::new(), MAX_ASSETS, 0)
                    .map_err(|_| AssetRefusal::resource())?;
                &held
            }
        };
        let request = AdmissionRequest {
            request_id,
            name,
            declared_media_type,
            bytes,
        };
        let digest = |input: &[u8]| sha256(input);
        let png = |input: &[u8]| charged_png_check_in(input, cpu);
        let ports = AdmissionPorts {
            digest: &digest,
            png: &png,
        };
        let new = match decide(&request, inventory, &ports) {
            AdmissionDecision::Refused(refusal) => {
                return Err(AssetRefusal::from_admission(&refusal))
            }
            AdmissionDecision::Duplicate { .. } => {
                return Ok(PreparedAssetEdit {
                    origin: self.clone(),
                    edit: AssetEdit::Unchanged,
                })
            }
            AdmissionDecision::New(new) => new,
        };
        // The viewport reference lives outside the catalog but shares the id space of
        // the wire: the encoder refuses a catalog record whose id equals the
        // reference id unless it shares the reference allocation, and a NEW asset
        // never does (decision 7). Refuse it typed, before any reservation.
        if let ProjectReferenceState::StillImage(reference) = project.reference_state() {
            if reference.id() == new.id.as_str() {
                return Err(AssetRefusal::new(
                    DispatchErrorCode::BusyConflict,
                    "id_collision",
                ));
            }
        }
        // Declaration order: the permit outlives the buffer, the allocation and
        // the record, on success, on every refusal and on unwinding.
        let mut permit = reserve(bytes.len()).map_err(|_| AssetRefusal::resource())?;
        let mut buffer = allocate(&mut permit).map_err(|_| AssetRefusal::resource())?;
        if buffer.len() != bytes.len() || buffer.capacity() != bytes.len() {
            return Err(AssetRefusal::resource());
        }
        buffer.copy_from_slice(bytes);
        let allocation = Arc::new(buffer);
        let source = AssetSource::admitted_embedded(
            new.media_type.to_string(),
            Arc::clone(&allocation),
            MAX_ENCODED_BYTES,
        )
        .map_err(|_| AssetRefusal::resource())?;
        let record = AssetRecord::new(new.id, new.name, new.kind, source)
            .map_err(|_| AssetRefusal::new(DispatchErrorCode::InvalidRequest, "name"))?;
        // Dry-run the one save path on the next project (the record, the catalog and
        // the reference together) before the allocation is bound or published. The
        // transient encoding is dropped here; a refusal releases the permit above.
        let record = Arc::new(record);
        let mut records = inventory.records().to_vec();
        records.push(Arc::clone(&record));
        let placements = inventory.placements().to_vec();
        let placement_limit = placements.len();
        let next = AssetInventory::new(records, placements, MAX_ASSETS, placement_limit)
            .map_err(|_| AssetRefusal::new(DispatchErrorCode::Unavailable, "catalog_full"))?;
        Self::check_candidate_saveable(&project.with_asset_inventory(Some(next)))
            .map_err(|_| AssetRefusal::new(DispatchErrorCode::Unavailable, "project_unsaveable"))?;
        bind(&mut permit, &allocation).map_err(|_| AssetRefusal::resource())?;
        Ok(PreparedAssetEdit {
            origin: self.clone(),
            edit: AssetEdit::Add(record),
        })
    }

    /// Removal of an absent id is a silent no-op, as `removeEntry` is in the
    /// baseline media library; a non-image record and an in-use asset are refused.
    pub(crate) fn prepare_asset_removal(
        &self,
        asset_id: &str,
    ) -> Result<PreparedAssetEdit, AssetRefusal> {
        let project = self.project().ok_or_else(AssetRefusal::bare)?;
        let id = AssetId::new(asset_id.to_string())
            .map_err(|_| AssetRefusal::new(DispatchErrorCode::InvalidRequest, "asset_id"))?;
        let Some(record) = project
            .asset_inventory()
            .and_then(|inventory| inventory.get(&id))
        else {
            return Ok(PreparedAssetEdit {
                origin: self.clone(),
                edit: AssetEdit::Unchanged,
            });
        };
        if record.kind() != AssetKind::Image {
            return Err(AssetRefusal::new(
                DispatchErrorCode::InvalidRequest,
                "asset_kind",
            ));
        }
        if asset_in_use(project, &id) {
            return Err(AssetRefusal::new(
                DispatchErrorCode::BusyConflict,
                "asset_in_use",
            ));
        }
        Ok(PreparedAssetEdit {
            origin: self.clone(),
            edit: AssetEdit::Remove(id),
        })
    }
}

#[cfg(all(test, feature = "history", feature = "project_media_codec"))]
#[path = "../tests/object_raster_asset_attach_prepare.rs"]
mod object_raster_asset_attach_prepare;
#[cfg(all(test, feature = "history", feature = "project_media_codec"))]
#[path = "../tests/object_raster_asset_attach_prepare_refusals.rs"]
mod object_raster_asset_attach_prepare_refusals;
