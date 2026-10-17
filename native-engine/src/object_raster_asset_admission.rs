//! Raster/media asset admission policy (G01 / G02 / G27, L06-D): from the bytes,
//! the caller's declared media type, the requested name, a request id and the
//! current `AssetInventory`, decide whether an incoming file is a NEW asset, a
//! DUPLICATE of an asset the project already holds, or REFUSED. The decision is
//! a value: no inventory mutation, no I/O, no retained workspace, no clock or
//! randomness. The attach transaction (L06-H) turns a `New` into a record under
//! the existing sealed owner commit; this kernel is only its admission policy.
//!
//! Content identity is media type AND digest AND byte equality. The digest only
//! narrows candidates (together with the length); bytes decide, so two inputs
//! with equal digests and different bytes are never merged. A duplicate returns
//! the existing `AssetId` and the existing allocation, so the owner ledger's
//! pointer-identity charge rule counts the bytes once. The baseline media
//! library (`addEntry`, media-library.js) never deduplicated and minted ids from
//! `Date.now()` / `Math.random()`; there is no JS oracle for either, so those two
//! rules are characterised against the native accepted values only.
//!
//! Limits mirror object_project_admission.rs (encoded 16 MiB, 8192 px, RGBA
//! 256 MiB, decoder 64 MiB) and the wire (1024 assets, name non-blank, no NUL,
//! at most 255 bytes). Only PNG has a decoder in this build, so every other
//! recognised container is a typed `DecoderUnavailable`, never a silent accept.
//!
//! Layering: this file and the sniffer live in the document layer, which may
//! import nothing outside it, so the digest and the PNG check are injected
//! (`AdmissionPorts`). The owner transaction (L06-H) passes
//! `request_receipts::sha256`; the default PNG port is
//! `object_raster_asset_png_port.rs` (codec layer, `reference_image_asset`).
//!
//! Files: this policy and `object_raster_asset_sniff.rs` (signatures and the
//! baseline name table).
use crate::object_document::{AssetId, AssetInventory, AssetKind};
use std::sync::Arc;

#[cfg(feature = "reference_image_asset")]
#[path = "object_raster_asset_png_port.rs"]
pub(crate) mod png_port;
#[path = "object_raster_asset_sniff.rs"]
pub(crate) mod sniff;

use sniff::sniff_media_type;

pub(crate) const MAX_ENCODED_BYTES: usize = 16 * 1024 * 1024;
pub(crate) const MAX_DIMENSION: u32 = 8192;
pub(crate) const MAX_RGBA_BYTES: usize = 256 * 1024 * 1024;
pub(crate) const MAX_ASSETS: usize = 1024;
pub(crate) const MAX_NAME_BYTES: usize = 255;
const ID_PREFIX: &str = "asset-";
const PNG: &str = "image/png";

/// What a PNG validity port reports about borrowed bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PngCheck {
    Valid { width: u32, height: u32 },
    Corrupt,
    Unsupported,
    TooLarge,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Refusal {
    Empty,
    Oversize,
    UnrecognizedSignature,
    TypeMismatch { sniffed: &'static str },
    Name,
    RequestId,
    DecoderUnavailable { media_type: &'static str },
    Corrupt,
    UnsupportedEncoding,
    ImageTooLarge,
    ResourceUnavailable,
    IdCollision,
    CatalogFull,
}

/// A file the project does not hold yet; everything an attach needs to build
/// the `AssetRecord` once the owner has charged the allocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NewAsset {
    pub(crate) id: AssetId,
    pub(crate) name: String,
    pub(crate) kind: AssetKind,
    pub(crate) media_type: &'static str,
    pub(crate) digest: [u8; 32],
    pub(crate) dimensions: Option<(u32, u32)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AdmissionDecision {
    New(NewAsset),
    Duplicate {
        existing: AssetId,
        allocation: Arc<Vec<u8>>,
    },
    Refused(Refusal),
}

pub(crate) struct AdmissionRequest<'a> {
    pub(crate) request_id: &'a str,
    pub(crate) name: &'a str,
    pub(crate) declared_media_type: &'a str,
    pub(crate) bytes: &'a [u8],
}

/// The two replaceable capabilities of the kernel: the content digest and the
/// PNG validity check. Both are borrowed for one decision.
pub(crate) struct AdmissionPorts<'a> {
    pub(crate) digest: &'a dyn Fn(&[u8]) -> [u8; 32],
    pub(crate) png: &'a dyn Fn(&[u8]) -> PngCheck,
}

/// Empty and oversize inputs are refused before any hashing or sniffing.
pub(crate) fn check_len(len: usize) -> Result<(), Refusal> {
    if len == 0 {
        Err(Refusal::Empty)
    } else if len > MAX_ENCODED_BYTES {
        Err(Refusal::Oversize)
    } else {
        Ok(())
    }
}

/// The wire / `AssetRecord::new` name rule.
pub(crate) fn valid_name(name: &str) -> bool {
    !name.trim().is_empty() && name.len() <= MAX_NAME_BYTES && !name.contains('\0')
}

/// Pure function of the request id; a collision is refused by the caller of
/// this function, never resolved by suffixing.
pub(crate) fn mint_asset_id(request_id: &str) -> Result<AssetId, Refusal> {
    if request_id.trim().is_empty() {
        return Err(Refusal::RequestId);
    }
    AssetId::new(format!("{ID_PREFIX}{request_id}")).map_err(|_| Refusal::RequestId)
}

/// The embedded asset with the same media type, length, digest AND bytes.
pub(crate) fn find_duplicate(
    inventory: &AssetInventory,
    media_type: &str,
    bytes: &[u8],
    digest: &[u8; 32],
    digest_of: &dyn Fn(&[u8]) -> [u8; 32],
) -> Option<(AssetId, Arc<Vec<u8>>)> {
    inventory.records().iter().find_map(|record| {
        let source = record.source();
        let allocation = source.encoded_allocation()?;
        if source.media_type() != Some(media_type)
            || allocation.len() != bytes.len()
            || digest_of(allocation.as_slice()) != *digest
            || allocation.as_slice() != bytes
        {
            return None;
        }
        Some((record.id().clone(), Arc::clone(allocation)))
    })
}

fn checked_dimensions(check: PngCheck) -> Result<(u32, u32), Refusal> {
    match check {
        PngCheck::Valid { width, height } => {
            if width == 0 || height == 0 {
                Err(Refusal::Corrupt)
            } else if width > MAX_DIMENSION || height > MAX_DIMENSION {
                Err(Refusal::ImageTooLarge)
            } else {
                Ok((width, height))
            }
        }
        PngCheck::Corrupt => Err(Refusal::Corrupt),
        PngCheck::Unsupported => Err(Refusal::UnsupportedEncoding),
        PngCheck::TooLarge => Err(Refusal::ImageTooLarge),
        PngCheck::Unavailable => Err(Refusal::ResourceUnavailable),
    }
}

fn run(
    request: &AdmissionRequest<'_>,
    inventory: &AssetInventory,
    ports: &AdmissionPorts<'_>,
) -> Result<AdmissionDecision, Refusal> {
    check_len(request.bytes.len())?;
    let sniffed = sniff_media_type(request.bytes).ok_or(Refusal::UnrecognizedSignature)?;
    if !request
        .declared_media_type
        .eq_ignore_ascii_case(sniffed.media_type)
    {
        return Err(Refusal::TypeMismatch {
            sniffed: sniffed.media_type,
        });
    }
    if !valid_name(request.name) {
        return Err(Refusal::Name);
    }
    let id = mint_asset_id(request.request_id)?;
    if sniffed.media_type != PNG {
        return Err(Refusal::DecoderUnavailable {
            media_type: sniffed.media_type,
        });
    }
    let digest = (ports.digest)(request.bytes);
    // An equal admitted asset is valid by construction: no second decode.
    if let Some((existing, allocation)) =
        find_duplicate(inventory, PNG, request.bytes, &digest, ports.digest)
    {
        return Ok(AdmissionDecision::Duplicate {
            existing,
            allocation,
        });
    }
    let dimensions = checked_dimensions((ports.png)(request.bytes))?;
    if inventory.get(&id).is_some() {
        return Err(Refusal::IdCollision);
    }
    if inventory.records().len() >= MAX_ASSETS {
        return Err(Refusal::CatalogFull);
    }
    Ok(AdmissionDecision::New(NewAsset {
        id,
        name: request.name.to_string(),
        kind: sniffed.kind,
        media_type: sniffed.media_type,
        digest,
        dimensions: Some(dimensions),
    }))
}

pub(crate) fn decide(
    request: &AdmissionRequest<'_>,
    inventory: &AssetInventory,
    ports: &AdmissionPorts<'_>,
) -> AdmissionDecision {
    match run(request, inventory, ports) {
        Ok(decision) => decision,
        Err(refusal) => AdmissionDecision::Refused(refusal),
    }
}

#[cfg(all(test, feature = "commands"))]
#[path = "../tests/object_raster_asset_admission_controls.rs"]
mod controls;
#[cfg(all(test, feature = "commands"))]
#[path = "../tests/object_raster_asset_admission_oracle.rs"]
pub(crate) mod oracle;
#[cfg(all(test, feature = "commands", feature = "reference_image_asset"))]
#[path = "../tests/object_raster_asset_admission_real.rs"]
mod real_validator;
#[cfg(all(test, feature = "commands"))]
#[path = "../tests/object_raster_asset_admission_sniff.rs"]
mod sniff_tests;
#[cfg(all(test, feature = "commands"))]
#[path = "../tests/object_raster_asset_admission.rs"]
mod tests;
