//! Strict wire payload of the import batch command (L10-H). Original bytes only: a `Value` would
//! erase duplicate members, so the payload is decoded here directly. Every record is a complete
//! `ObjectRecord` decoded by the one native codec path (object-only, unknown and duplicate members
//! refused); the owner, not the caller, chooses every native id.
//!
//! ```text
//! {"command":"object.import.batch",
//!  "layers":[{"sourceId":"figma:1","name":"Frame 1"}],
//!  "records":[{"sourceId":"figma:1:7","record":{... "target":{"layerUid":"figma:1","strokeId":""} ...}}],
//!  "skipped":[{"kind":"element","sourceId":"svg:9","reason":"unsupported-element:Raster"}],
//!  "unavailable":[{"sourceId":"psd:3","need":"raster-asset"}]}
//! ```
//! `skipped` and `unavailable` default to empty; `layers` and `records` are required. The operation
//! envelope bound of the dispatcher (4096 bytes) applies to the whole request; this module adds no
//! limit of its own beyond the same bound for the raw entry.
use crate::history::object_history::object_shot_kernels::native_import_transaction::{
    ImportBatch, ImportError, ImportRecord, ImportSkip, MissingProvider, NewLayer, SkipKind,
    UnavailableElement,
};
use crate::object_document::{object_deserialize, ObjectRecord};
use serde::Deserialize;

pub(crate) const COMMAND: &str = "object.import.batch";
/// Same bound as every other object operation except the vector edit (whose 64 KiB bound is open).
pub(crate) const MAX_PAYLOAD_BYTES: usize = 4096;

#[derive(Debug)]
struct LayerWire {
    source_id: String,
    name: String,
}
object_deserialize!(LayerWire {
    source_id: String,
    name: String
});

#[derive(Debug)]
struct RecordWire {
    source_id: String,
    record: ObjectRecord,
}
object_deserialize!(RecordWire {
    source_id: String,
    record: ObjectRecord
});

#[derive(Debug)]
struct SkipWire {
    kind: String,
    source_id: String,
    reason: String,
}
object_deserialize!(SkipWire {
    kind: String,
    source_id: String,
    reason: String
});

#[derive(Debug)]
struct UnavailableWire {
    source_id: String,
    need: String,
}
object_deserialize!(UnavailableWire {
    source_id: String,
    need: String
});

#[derive(Debug)]
struct BatchWire {
    command: String,
    layers: Vec<LayerWire>,
    records: Vec<RecordWire>,
    skipped: Vec<SkipWire>,
    unavailable: Vec<UnavailableWire>,
}
object_deserialize!(BatchWire {
    command: String,
    layers: Vec<LayerWire>,
    records: Vec<RecordWire>,
    #[serde(default)] skipped: Vec<SkipWire>,
    #[serde(default)] unavailable: Vec<UnavailableWire>
});

fn skip_kind(label: &str) -> Result<SkipKind, ImportError> {
    match label {
        "layer" => Ok(SkipKind::Layer),
        "element" => Ok(SkipKind::Element),
        "asset" => Ok(SkipKind::Asset),
        _ => Err(ImportError::Payload),
    }
}

fn provider(label: &str) -> Result<MissingProvider, ImportError> {
    match label {
        "raster-asset" => Ok(MissingProvider::RasterAsset),
        "text" => Ok(MissingProvider::Text),
        "font" => Ok(MissingProvider::Font),
        _ => Err(ImportError::Payload),
    }
}

/// Strict decode of the raw bytes into a typed batch.
pub(crate) fn parse_batch(bytes: &[u8]) -> Result<ImportBatch, ImportError> {
    if bytes.len() > MAX_PAYLOAD_BYTES {
        return Err(ImportError::Payload);
    }
    let wire: BatchWire = serde_json::from_slice(bytes).map_err(|_| ImportError::Payload)?;
    if wire.command != COMMAND {
        return Err(ImportError::Payload);
    }
    let skipped = wire
        .skipped
        .into_iter()
        .map(|skip| -> Result<ImportSkip, ImportError> {
            Ok(ImportSkip {
                kind: skip_kind(&skip.kind)?,
                source_id: skip.source_id,
                reason: skip.reason,
            })
        })
        .collect::<Result<Vec<_>, ImportError>>()?;
    let unavailable = wire
        .unavailable
        .into_iter()
        .map(|missing| -> Result<UnavailableElement, ImportError> {
            Ok(UnavailableElement {
                need: provider(&missing.need)?,
                source_id: missing.source_id,
            })
        })
        .collect::<Result<Vec<_>, ImportError>>()?;
    Ok(ImportBatch {
        layers: wire
            .layers
            .into_iter()
            .map(|layer| NewLayer {
                source_id: layer.source_id,
                name: layer.name,
            })
            .collect(),
        records: wire
            .records
            .into_iter()
            .map(|item| ImportRecord {
                source_id: item.source_id,
                record: item.record,
            })
            .collect(),
        skipped,
        unavailable,
    })
}
