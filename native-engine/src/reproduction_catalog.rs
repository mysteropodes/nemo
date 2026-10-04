//! One immutable synthetic origin; eligibility is not opt-in or capture.
use crate::application::{ExportResourceResolver, NativeApplication};
use crate::codec::decode_project;
use crate::document::OpacityDocument;
use crate::export_job::{ExportCompositor, StagedArtifactPort};
use crate::request_receipts::sha256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReproductionFixture<'a> {
    pub id: &'a str,
    pub version: u32,
    pub sha256: &'a str,
}

pub const REPRODUCTION_FIXTURE: ReproductionFixture<'static> = ReproductionFixture {
    id: "native-opacity-static",
    version: 1,
    sha256: "895ca05a43295104301c472287149b1c01aa6ef681db80dcbb79e58a55b36050",
};
const BYTES: &[u8] = include_bytes!("../fixtures/reproduction-opacity-v1.json");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReproductionCatalogError {
    UnknownFixture,
    InvalidFixtureBytes,
    InvalidInstance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReproductionEligibility {
    Eligible,
    NotCatalog,
    NotPristine,
    Released,
    Replacing,
}

// The tuple constructor is private to this module. Only verified catalog bytes
// can produce the marker; it contains no document snapshot or caller metadata.
pub(super) struct CatalogOrigin(());

fn verified_document(
    fixture: ReproductionFixture<'_>,
    bytes: &[u8],
) -> Result<OpacityDocument, ReproductionCatalogError> {
    if fixture != REPRODUCTION_FIXTURE {
        return Err(ReproductionCatalogError::UnknownFixture);
    }
    let digest: String = sha256(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    if digest != REPRODUCTION_FIXTURE.sha256 {
        return Err(ReproductionCatalogError::InvalidFixtureBytes);
    }
    decode_project(bytes).map_err(|_| ReproductionCatalogError::InvalidFixtureBytes)
}

pub(super) fn load(
    fixture: ReproductionFixture<'_>,
) -> Result<(OpacityDocument, CatalogOrigin), ReproductionCatalogError> {
    Ok((verified_document(fixture, BYTES)?, CatalogOrigin(())))
}

impl<P: StagedArtifactPort, C: ExportCompositor, R: ExportResourceResolver>
    NativeApplication<P, C, R>
{
    /// Read-only eligibility for a future explicit opt-in. Never inspect, hash
    /// or serialize an arbitrary document to establish synthetic provenance.
    pub fn reproduction_eligibility(&self) -> ReproductionEligibility {
        if self.reproduction_origin.is_none() {
            return ReproductionEligibility::NotCatalog;
        }
        if self.release.is_some() {
            return ReproductionEligibility::Released;
        }
        if self.replacement.is_some() {
            return ReproductionEligibility::Replacing;
        }
        if self.content_revision() != 0
            || self.history.history_depths() != (0, 0)
            || !self.requests.is_empty()
            || self.history.transactions.has_active()
        {
            return ReproductionEligibility::NotPristine;
        }
        ReproductionEligibility::Eligible
    }
}

#[cfg(test)]
#[path = "../tests/reproduction_catalog.rs"]
mod tests;
