//! Typed vocabulary of the atomic import transaction (L10-H): the batch the importers hand over and
//! the typed refusals the planner and the wire decoder produce. Plain data; no owner, no history.
use crate::history::object_history::object_shot_kernels::object_import_remap_ids::{
    RemapError, RemapKind,
};
use crate::object_document::ObjectRecord;
use crate::request_receipts::DispatchErrorCode;

/// A layer the import creates; its `source_id` is the importer's own handle (Figma frame id,
/// PSD layer index) and is what records name as their layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NewLayer {
    pub(crate) source_id: String,
    pub(crate) name: String,
}

/// A vector record to create. `record.target.layer_uid` is a REFERENCE: the `source_id` of a
/// batch layer or the uid of an existing layer; `record.target.stroke_id` must be empty (the
/// owner mints it, as `object.vector.create` does).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ImportRecord {
    pub(crate) source_id: String,
    pub(crate) record: ObjectRecord,
}

/// What the importer's own plan dropped on purpose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SkipKind {
    Layer,
    Element,
    Asset,
}

impl SkipKind {
    pub(crate) fn remap_kind(self) -> RemapKind {
        match self {
            SkipKind::Layer => RemapKind::Layer,
            SkipKind::Element => RemapKind::Stroke,
            SkipKind::Asset => RemapKind::Asset,
        }
    }
    pub(crate) fn label(self) -> &'static str {
        match self {
            SkipKind::Layer => "layer",
            SkipKind::Element => "element",
            SkipKind::Asset => "asset",
        }
    }
}

/// One element the importer skipped, with the reason it reports (never empty).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ImportSkip {
    pub(crate) kind: SkipKind,
    pub(crate) source_id: String,
    pub(crate) reason: String,
}

/// The provider an element needs and the base does not have.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MissingProvider {
    /// Raster pixels as an immutable asset record (G27 / G38b, lane 6).
    RasterAsset,
    /// A text block record (G38b, lane 6).
    Text,
    /// A retained font (G41, Assistant2).
    Font,
}

impl MissingProvider {
    pub(crate) fn label(self) -> &'static str {
        match self {
            MissingProvider::RasterAsset => "raster-asset",
            MissingProvider::Text => "text",
            MissingProvider::Font => "font",
        }
    }
}

/// An element the importer wants but no provider can hold: the whole batch is refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UnavailableElement {
    pub(crate) source_id: String,
    pub(crate) need: MissingProvider,
}

/// One whole import: everything it creates, skips or cannot hold.
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct ImportBatch {
    pub(crate) layers: Vec<NewLayer>,
    pub(crate) records: Vec<ImportRecord>,
    pub(crate) skipped: Vec<ImportSkip>,
    pub(crate) unavailable: Vec<UnavailableElement>,
}

/// Typed refusals. Every one is produced before anything can publish.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ImportError {
    /// The wire payload is not a strict batch.
    Payload,
    /// The id table refused the batch (invalid, duplicate or colliding source ids...).
    Remap(RemapError),
    /// An element needs a provider that is absent.
    ProviderUnavailable {
        source_id: String,
        need: MissingProvider,
    },
    /// Record `index` is not a schema-1 `editable-vector-path`.
    NotVector { index: usize },
    /// Record `index` arrived with a stroke id; the owner mints it.
    StrokeIdPreset { index: usize },
    /// Record `index` targets a Reference scope (unavailable for drawing).
    ReferenceScope { index: usize },
    /// Record `index` names a layer that is neither in the batch nor in the origin.
    UnknownLayer { index: usize, reference: String },
    /// Record `index` names a layer the importer itself skipped.
    SkippedLayer { index: usize, reference: String },
    /// Record `index` has an invalid context or a frame outside the document.
    InvalidTarget { index: usize },
    /// Record `index` is in a context the composition-order plan does not declare.
    ContextAbsent { index: usize },
    /// The existing layer is locked.
    LayerLocked { layer_uid: String },
    /// The existing layer has no key at the frame (the baseline would copy the held strokes).
    KeyframeRequired { layer_uid: String, frame: u32 },
    /// A new layer's timing could not take the key.
    Timing { index: usize },
    /// A count does not fit the document's `u32` positions.
    Overflow,
    /// The prepared receipt would not fit the bounded response.
    ReceiptTooLarge,
    /// The owner refused to seal (pins, storage admission).
    Dispatch(DispatchErrorCode),
    /// An internal invariant did not hold (never expected).
    Internal,
}

impl ImportError {
    /// The request-receipt vocabulary of the refusal.
    pub(crate) fn code(&self) -> DispatchErrorCode {
        match self {
            ImportError::Payload
            | ImportError::NotVector { .. }
            | ImportError::StrokeIdPreset { .. }
            | ImportError::SkippedLayer { .. }
            | ImportError::InvalidTarget { .. }
            | ImportError::Timing { .. }
            | ImportError::ReceiptTooLarge => DispatchErrorCode::InvalidRequest,
            ImportError::Remap(error) => match error {
                RemapError::RevisionExhausted | RemapError::AllocationExhausted { .. } => {
                    DispatchErrorCode::Unavailable
                }
                _ => DispatchErrorCode::InvalidRequest,
            },
            ImportError::UnknownLayer { .. } | ImportError::ContextAbsent { .. } => {
                DispatchErrorCode::NotFound
            }
            ImportError::ProviderUnavailable { .. }
            | ImportError::ReferenceScope { .. }
            | ImportError::LayerLocked { .. }
            | ImportError::KeyframeRequired { .. }
            | ImportError::Overflow => DispatchErrorCode::Unavailable,
            ImportError::Dispatch(code) => *code,
            ImportError::Internal => DispatchErrorCode::Internal,
        }
    }
    /// A fixed message (the receipt never echoes importer-controlled text).
    pub(crate) fn message(&self) -> &'static str {
        match self {
            ImportError::Payload => "Import batch payload is invalid.",
            ImportError::Remap(_) => "Import batch ids are invalid or unavailable.",
            ImportError::ProviderUnavailable { .. } => {
                "Import needs a provider that is unavailable."
            }
            ImportError::UnknownLayer { .. } | ImportError::SkippedLayer { .. } => {
                "Import record names an unknown or skipped layer."
            }
            ImportError::LayerLocked { .. } | ImportError::KeyframeRequired { .. } => {
                "Import target layer cannot take the records."
            }
            ImportError::ReceiptTooLarge => "Import receipt exceeds the response bound.",
            ImportError::Dispatch(_) => "Import batch could not be sealed.",
            ImportError::Internal => "Import batch failed an internal invariant.",
            _ => "Import record is invalid or unavailable.",
        }
    }
}
