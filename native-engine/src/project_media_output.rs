//! bounded deterministic native-project output.
use crate::object_codec::project::{Metadata, ProjectWireError, ProjectWireLimits, FORMAT};
use crate::object_document::{ProjectEnvelope, ProjectReferenceState};
use serde::Serialize;
use serde_json::value::RawValue;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Output<'a, R, A> {
    format: &'static str,
    format_version: u32,
    project: Metadata,
    drawing: &'a RawValue,
    viewport_reference: R,
    assets: A,
}

#[cfg(feature = "project_media_codec")]
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReferenceOutput<'a> {
    kind: &'static str,
    asset_id: &'a str,
    name: &'a str,
    visible: bool,
    opacity: f64,
    offset_frames: i64,
}
#[cfg(feature = "project_media_codec")]
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AssetOutput<'a> {
    id: &'a str,
    media_type: &'static str,
    encoding: &'static str,
    data: Encoded<'a>,
}
#[cfg(feature = "project_media_codec")]
struct Encoded<'a>(&'a [u8]);
#[cfg(feature = "project_media_codec")]
impl Serialize for Encoded<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(&base64::display::Base64Display::new(
            self.0,
            &base64::engine::general_purpose::STANDARD,
        ))
    }
}

// Cap logical bytes and reservation REQUESTS before copying. Allocator
// over-provisioning/actual Vec capacity is not bounded by reserve_exact.
pub(super) struct CappedWriter {
    pub(super) bytes: Vec<u8>,
    pub(super) cap: usize,
    pub(super) refused: bool,
}
impl std::io::Write for CappedWriter {
    fn write(&mut self, chunk: &[u8]) -> std::io::Result<usize> {
        let needed = self.bytes.len().checked_add(chunk.len());
        if needed.is_none_or(|n| n > self.cap) {
            self.refused = true;
            return Err(std::io::Error::other("project wire byte budget"));
        }
        let needed = needed.unwrap();
        if needed > self.bytes.capacity() {
            let capacity = needed
                .max(self.bytes.capacity().saturating_mul(2))
                .min(self.cap);
            if self
                .bytes
                .try_reserve_exact(capacity - self.bytes.len())
                .is_err()
            {
                self.refused = true;
                return Err(std::io::Error::other("project wire reservation"));
            }
        }
        self.bytes.extend_from_slice(chunk);
        Ok(chunk.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn bounded_json(value: &impl Serialize, cap: usize) -> Result<Vec<u8>, ProjectWireError> {
    let mut writer = CappedWriter {
        bytes: Vec::new(),
        cap,
        refused: false,
    };
    serde_json::to_writer(&mut writer, value).map_err(|_| {
        if writer.refused {
            ProjectWireError::Budget
        } else {
            ProjectWireError::Invalid
        }
    })?;
    Ok(writer.bytes)
}

fn encode_legacy_project(
    envelope: &ProjectEnvelope,
    limits: ProjectWireLimits,
) -> Result<Vec<u8>, ProjectWireError> {
    let reference = envelope.reference_state();
    #[cfg(not(feature = "project_media_codec"))]
    if reference != ProjectReferenceState::NoMedia {
        return Err(ProjectWireError::Unsupported);
    }
    #[cfg(feature = "project_media_codec")]
    if let ProjectReferenceState::StillImage(data) = &reference {
        let (cap, _) = limits.png.ok_or(ProjectWireError::Budget)?;
        if data.encoded_png().len() > cap
            || base64::encoded_len(data.encoded_png().len(), true)
                .ok_or(ProjectWireError::Budget)?
                > limits.envelope_bytes
        {
            return Err(ProjectWireError::Budget);
        }
    }
    crate::object_codec::validate_storage(envelope.drawing()).map_err(ProjectWireError::Drawing)?;
    let drawing = bounded_json(envelope.drawing(), limits.drawing_bytes)?;
    let raw =
        serde_json::from_slice::<&RawValue>(&drawing).map_err(|_| ProjectWireError::Invalid)?;
    let project = Metadata::from_metadata(envelope.project());
    #[cfg(feature = "project_media_codec")]
    if let ProjectReferenceState::StillImage(data) = &reference {
        let viewport_reference = ReferenceOutput {
            kind: "still-image",
            asset_id: data.id(),
            name: data.name(),
            visible: data.visible(),
            opacity: data.opacity(),
            offset_frames: data.offset_frames(),
        };
        let assets = [AssetOutput {
            id: data.id(),
            media_type: "image/png",
            encoding: "base64",
            data: Encoded(data.encoded_png()),
        }];
        return bounded_json(
            &Output {
                format: FORMAT,
                format_version: 1,
                project,
                drawing: raw,
                viewport_reference,
                assets,
            },
            limits.envelope_bytes,
        );
    }
    let wire = Output {
        format: FORMAT,
        format_version: 1,
        project,
        drawing: raw,
        viewport_reference: (),
        assets: [(); 0],
    };
    bounded_json(&wire, limits.envelope_bytes)
}

#[cfg(feature = "project_media_codec")]
mod catalog {
    //! deterministic catalog wire output; no resource ownership.
    use crate::object_codec::project::output::{bounded_json, Encoded, ReferenceOutput};
    use crate::object_codec::project::{Metadata, ProjectWireError, ProjectWireLimits};
    use crate::object_document::{
        AssetId, AssetKind, AssetRecord, ProjectEnvelope, ProjectReferenceState,
    };
    use serde::Serialize;
    use serde_json::value::RawValue;
    use std::sync::Arc;

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Output<'a> {
        format: &'static str,
        format_version: u32,
        project: Metadata,
        drawing: &'a RawValue,
        viewport_reference: Option<ReferenceOutput<'a>>,
        assets: Vec<CatalogAsset<'a>>,
        asset_inventory: Catalog<'a>,
    }
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Catalog<'a> {
        asset_ids: Vec<&'a str>,
        placements: [(); 0],
    }
    #[derive(Serialize)]
    struct CatalogAsset<'a> {
        id: &'a str,
        name: &'a str,
        kind: &'static str,
        source: Source<'a>,
    }
    #[derive(Serialize)]
    #[serde(untagged)]
    enum Source<'a> {
        Embedded {
            kind: &'static str,
            #[serde(rename = "mediaType")]
            media_type: &'static str,
            encoding: &'static str,
            data: Encoded<'a>,
        },
        Linked {
            kind: &'static str,
            locator: &'a str,
        },
    }
    fn kind(value: AssetKind) -> &'static str {
        match value {
            AssetKind::Image => "image",
            AssetKind::Video => "video",
            AssetKind::Audio => "audio",
            AssetKind::Font => "font",
        }
    }
    fn embedded<'a>(
        bytes: &'a [u8],
        limits: ProjectWireLimits,
    ) -> Result<Source<'a>, ProjectWireError> {
        let cap = limits.png.ok_or(ProjectWireError::Budget)?.0;
        if bytes.is_empty()
            || bytes.len() > cap
            || base64::encoded_len(bytes.len(), true).ok_or(ProjectWireError::Budget)?
                > limits.envelope_bytes
        {
            return Err(ProjectWireError::Budget);
        }
        Ok(Source::Embedded {
            kind: "embedded",
            media_type: "image/png",
            encoding: "base64",
            data: Encoded(bytes),
        })
    }
    fn asset<'a>(
        record: &'a AssetRecord,
        limits: ProjectWireLimits,
    ) -> Result<CatalogAsset<'a>, ProjectWireError> {
        let source = if let Some(locator) = record.source().locator() {
            if locator.len() > 4096 {
                return Err(ProjectWireError::Budget);
            }
            Source::Linked {
                kind: "linked",
                locator,
            }
        } else {
            if record.kind() != AssetKind::Image
                || record.source().media_type() != Some("image/png")
            {
                return Err(ProjectWireError::Unsupported);
            }
            embedded(
                record
                    .source()
                    .encoded_allocation()
                    .ok_or(ProjectWireError::Invalid)?,
                limits,
            )?
        };
        Ok(CatalogAsset {
            id: record.id().as_str(),
            name: record.name(),
            kind: kind(record.kind()),
            source,
        })
    }
    pub(super) fn encode_catalog(
        envelope: &ProjectEnvelope,
        limits: ProjectWireLimits,
    ) -> Result<Vec<u8>, ProjectWireError> {
        let inventory = envelope
            .asset_inventory()
            .ok_or(ProjectWireError::Invalid)?;
        if !inventory.placements().is_empty() {
            return Err(ProjectWireError::Unsupported);
        }
        if inventory.records().len() > 1024 {
            return Err(ProjectWireError::Budget);
        }
        crate::object_codec::validate_storage(envelope.drawing())
            .map_err(ProjectWireError::Drawing)?;
        let drawing = bounded_json(envelope.drawing(), limits.drawing_bytes)?;
        let drawing =
            serde_json::from_slice::<&RawValue>(&drawing).map_err(|_| ProjectWireError::Invalid)?;
        let mut assets = Vec::new();
        let mut asset_ids = Vec::new();
        for record in inventory.records() {
            asset_ids.push(record.id().as_str());
            assets.push(asset(record, limits)?);
        }
        // Keep this clone alive for borrowed reference fields through serialization.
        let reference = envelope.reference_state();
        let viewport_reference = match &reference {
            ProjectReferenceState::NoMedia => None,
            ProjectReferenceState::StillImage(data) => {
                let id =
                    AssetId::new(data.id().to_owned()).map_err(|_| ProjectWireError::Invalid)?;
                if let Some(record) = inventory.get(&id) {
                    if record.kind() != AssetKind::Image
                        || record.source().media_type() != Some("image/png")
                        || !record
                            .source()
                            .encoded_allocation()
                            .is_some_and(|a| Arc::ptr_eq(a, data.encoded_allocation()))
                    {
                        return Err(ProjectWireError::Invalid);
                    }
                } else {
                    assets.push(CatalogAsset {
                        id: data.id(),
                        name: data.name(),
                        kind: "image",
                        source: embedded(data.encoded_png(), limits)?,
                    });
                }
                Some(ReferenceOutput {
                    kind: "still-image",
                    asset_id: data.id(),
                    name: data.name(),
                    visible: data.visible(),
                    opacity: data.opacity(),
                    offset_frames: data.offset_frames(),
                })
            }
        };
        if assets.len() > 1024 {
            return Err(ProjectWireError::Budget);
        }
        bounded_json(
            &Output {
                format: crate::object_codec::project::FORMAT,
                format_version: 2,
                project: Metadata::from_metadata(envelope.project()),
                drawing,
                viewport_reference,
                assets,
                asset_inventory: Catalog {
                    asset_ids,
                    placements: [],
                },
            },
            limits.envelope_bytes,
        )
    }
}

pub(crate) fn encode_project(
    envelope: &ProjectEnvelope,
    limits: ProjectWireLimits,
) -> Result<Vec<u8>, ProjectWireError> {
    if envelope.asset_inventory().is_some() {
        #[cfg(feature = "project_media_codec")]
        {
            return catalog::encode_catalog(envelope, limits);
        }
        #[cfg(not(feature = "project_media_codec"))]
        {
            return Err(ProjectWireError::Unsupported);
        }
    }
    encode_legacy_project(envelope, limits)
}
