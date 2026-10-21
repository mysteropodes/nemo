//! Atomic batch import transaction (L10-H; G35, G36, G37, I46): the planner that turns one
//! typed import batch into ONE new `ObjectDocument` sealed to one `ObjectSnapshot` origin.
//!
//! A batch is N new layers, M vector records and the importers' own skip reports. The planner
//! is pure: it reads the origin snapshot, allocates every native id through the L10-G remap table
//! (so a refused, stale or cancelled preparation consumed nothing), resolves every record's layer
//! reference through that table (a batch layer is rewritten, an existing layer is preserved byte
//! for byte) and returns the complete after-document or one typed refusal. Nothing is published
//! here: the sealed candidate, the pin checks and the single `ObjectRevisionOwner::publish` live
//! in `native_import_transaction_owner.rs`; the one history entry, request replay and dispatch in
//! the `session` child. Because the after-document exists only as a whole, a failure at item i of
//! N leaves revision, bytes, history depth and redo stack untouched.
//!
//! Baseline facts kept (oracle `fact` rows of tests/fixtures/native-import-transaction):
//! - a new layer starts with one key at frame 0 (`createUserLayer`), and an import at another
//!   frame makes that frame a key first (`importSVGString` -> `ensureKeyframe`); on a NEW layer
//!   the held content is empty, so the key is exactly `LayerTiming::promote_cell`;
//! - records are appended in batch order, after the layer's existing records;
//! - a locked layer is refused before anything is inserted.
//!
//! Dispositions that are NOT the baseline's, stated so nobody mistakes them for it:
//! - an import into an EXISTING layer at a frame that is not already a key is refused
//!   (`KeyframeRequired`): the baseline copies the held strokes into the new key, which is the
//!   lane-5 keyframe transaction, not this one;
//! - an element whose provider is absent (raster, text, font) refuses the whole batch with the
//!   named provider; it is never dropped and never faked.

use crate::commands::{insert_order_element, ImportBatchResult, ImportSkipReport, OrderEditError};
use crate::history::object_history::object_shot_kernels::object_import_remap_ids::{
    build_remap_table, OriginIds, RemapError, RemapKind, Resolved, SourceEntry,
};
use crate::object_document::{
    AuthoredOrder, FrameScopeKind, ObjectDocument, ObjectLayer, OrderLayer, Presence, TimingEdit,
    VECTOR_FAMILY,
};
use crate::revision::ObjectSnapshot;

#[path = "native_import_transaction_session.rs"]
pub(crate) mod session;
#[path = "native_import_transaction_svg.rs"]
pub(crate) mod svg;
#[path = "native_import_transaction_types.rs"]
mod types;
pub(crate) use types::{
    ImportBatch, ImportError, ImportRecord, ImportSkip, MissingProvider, NewLayer, SkipKind,
    UnavailableElement,
};
#[path = "native_import_transaction_wire.rs"]
pub(crate) mod wire;

#[cfg(test)]
#[path = "../tests/native_import_transaction_atomic.rs"]
mod atomic;
#[cfg(test)]
#[path = "../tests/native_import_transaction_chain.rs"]
mod chain;
#[cfg(test)]
#[path = "../tests/native_import_transaction_controls.rs"]
mod controls;
#[cfg(test)]
#[path = "../tests/native_import_transaction_dispatch.rs"]
mod dispatch_tests;
#[cfg(test)]
#[path = "../tests/native_import_transaction_rows.rs"]
mod rows;
#[cfg(test)]
#[path = "../tests/native_import_transaction_support.rs"]
mod support;
#[cfg(test)]
#[path = "../tests/native_import_transaction_svg.rs"]
mod svg_tests;
#[cfg(test)]
#[path = "../tests/native_import_transaction.rs"]
mod tests;

/// The after-document (absent for a no-op) and the receipt the commit will report.
#[derive(Debug)]
pub(crate) struct ImportPlan {
    pub(crate) document: Option<ObjectDocument>,
    pub(crate) result: ImportBatchResult,
}

/// Every id the origin already holds, per kind, with its content revision.
pub(crate) fn origin_ids(origin: &ObjectSnapshot) -> OriginIds {
    let document = origin.document();
    OriginIds::new(origin.content_revision())
        .with_ids(
            RemapKind::Layer,
            document.layers.iter().map(ObjectLayer::layer_uid),
        )
        .with_ids(
            RemapKind::Stroke,
            document
                .objects
                .iter()
                .map(|record| record.target.stroke_id.as_str()),
        )
}

fn order_error(index: usize, error: OrderEditError) -> ImportError {
    match error {
        OrderEditError::ContextAbsent => ImportError::ContextAbsent { index },
        OrderEditError::ReferenceScope => ImportError::ReferenceScope { index },
        OrderEditError::PositionOverflow => ImportError::Overflow,
        OrderEditError::ElementAbsent => ImportError::Internal,
    }
}

fn add_layers(
    document: &mut ObjectDocument,
    batch: &ImportBatch,
    native: &dyn Fn(&str) -> Option<String>,
) -> Result<Vec<(String, String)>, ImportError> {
    let mut created = Vec::with_capacity(batch.layers.len());
    for layer in &batch.layers {
        let uid = native(&layer.source_id).ok_or(ImportError::Internal)?;
        let natural_position =
            u32::try_from(document.layers.len()).map_err(|_| ImportError::Overflow)?;
        if let Presence::Present(plan) = &mut document.composition_order {
            for context in &mut plan.contexts {
                context.layers.push(OrderLayer {
                    layer_uid: uid.clone(),
                    natural_position,
                    order: AuthoredOrder {
                        static_value: Presence::Missing,
                        track: Presence::Missing,
                        expression: Presence::Missing,
                    },
                });
            }
        }
        let existing = document.layers.len();
        document.layers.push(ObjectLayer::blank_drawing(
            uid.clone(),
            layer.name.clone(),
            existing,
        ));
        created.push((layer.source_id.clone(), uid));
    }
    Ok(created)
}

/// Gives the record's frame a key: a batch layer promotes it (its held content is empty, which is
/// the baseline `ensureKeyframe`), an existing layer must already have it.
fn expose_frame(
    document: &mut ObjectDocument,
    layer_uid: &str,
    frame: u32,
    batch_layer: bool,
    index: usize,
) -> Result<(), ImportError> {
    let total = document.total_frames;
    let at = document
        .layers
        .iter()
        .position(|layer| layer.layer_uid == layer_uid)
        .ok_or(ImportError::Internal)?;
    if !batch_layer && document.layer_is_locked(layer_uid) {
        return Err(ImportError::LayerLocked {
            layer_uid: layer_uid.to_owned(),
        });
    }
    let edit = match document.layers[at].timing.value() {
        None => return Ok(()),
        Some(timing) if batch_layer => timing.promote_cell(frame, total),
        Some(timing) => {
            return if timing.exposure(frame).0 == "key" {
                Ok(())
            } else {
                Err(ImportError::KeyframeRequired {
                    layer_uid: layer_uid.to_owned(),
                    frame,
                })
            };
        }
    };
    match edit {
        Ok(TimingEdit::Changed(next)) => document.layers[at].timing = Presence::Present(next),
        Ok(TimingEdit::NoChange) => {}
        Err(_) => return Err(ImportError::Timing { index }),
    }
    Ok(())
}

/// Plans one batch against `origin`. Pure: the origin is never touched and nothing is published.
pub(crate) fn plan_import_batch(
    origin: &ObjectSnapshot,
    batch: &ImportBatch,
) -> Result<ImportPlan, ImportError> {
    if let Some(missing) = batch.unavailable.first() {
        return Err(ImportError::ProviderUnavailable {
            source_id: missing.source_id.clone(),
            need: missing.need,
        });
    }
    let ids = origin_ids(origin);
    let mut sources = Vec::with_capacity(batch.layers.len() + batch.records.len());
    for layer in &batch.layers {
        sources.push(SourceEntry::mapped(RemapKind::Layer, &layer.source_id));
    }
    for item in &batch.records {
        sources.push(SourceEntry::mapped(RemapKind::Stroke, &item.source_id));
    }
    for skip in &batch.skipped {
        sources.push(SourceEntry::skipped(
            skip.kind.remap_kind(),
            &skip.source_id,
            &skip.reason,
        ));
    }
    let table = build_remap_table(&ids, &sources).map_err(ImportError::Remap)?;
    let skipped: Vec<ImportSkipReport> = batch
        .skipped
        .iter()
        .map(|skip| ImportSkipReport {
            kind: skip.kind.label(),
            source_id: skip.source_id.clone(),
            reason: skip.reason.clone(),
        })
        .collect();
    if batch.layers.is_empty() && batch.records.is_empty() {
        return Ok(ImportPlan {
            document: None,
            result: ImportBatchResult {
                applied: false,
                layers: Vec::new(),
                objects: Vec::new(),
                skipped,
            },
        });
    }
    let mut document = origin.document().clone();
    let layers = add_layers(&mut document, batch, &|source| {
        table.native_id(RemapKind::Layer, source).map(str::to_owned)
    })?;
    let mut objects = Vec::with_capacity(batch.records.len());
    for (index, item) in batch.records.iter().enumerate() {
        let record = &item.record;
        if record.schema_version != 1 || record.family != VECTOR_FAMILY {
            return Err(ImportError::NotVector { index });
        }
        if !record.target.stroke_id.is_empty() {
            return Err(ImportError::StrokeIdPreset { index });
        }
        if record.target.frame_scope.kind != FrameScopeKind::Authored {
            return Err(ImportError::ReferenceScope { index });
        }
        let reference = record.target.layer_uid.as_str();
        let layer = match table.resolve(&ids, RemapKind::Layer, reference) {
            Ok(resolved) => resolved,
            Err(RemapError::DanglingReference { reference, .. }) => {
                return Err(ImportError::UnknownLayer { index, reference })
            }
            Err(RemapError::ReferenceToSkipped { reference, .. }) => {
                return Err(ImportError::SkippedLayer { index, reference })
            }
            Err(other) => return Err(ImportError::Remap(other)),
        };
        let stroke = table
            .native_id(RemapKind::Stroke, &item.source_id)
            .ok_or(ImportError::Internal)?
            .to_owned();
        let mut placed = record.clone();
        placed.target.layer_uid = layer.as_str().to_owned();
        placed.target.stroke_id = stroke.clone();
        if !document.valid_target(&placed.target) || placed.target.frame() >= document.total_frames
        {
            return Err(ImportError::InvalidTarget { index });
        }
        let batch_layer = matches!(layer, Resolved::Batch(_));
        expose_frame(
            &mut document,
            &placed.target.layer_uid,
            placed.target.frame(),
            batch_layer,
            index,
        )?;
        if let Presence::Present(plan) = &mut document.composition_order {
            insert_order_element(plan, &placed.target)
                .map_err(|error| order_error(index, error))?;
        }
        document.objects.push(placed);
        objects.push((item.source_id.clone(), stroke));
    }
    if layers.len() + objects.len() + skipped.len() != table.source_count() {
        return Err(ImportError::Internal);
    }
    Ok(ImportPlan {
        document: Some(document),
        result: ImportBatchResult {
            applied: true,
            layers,
            objects,
            skipped,
        },
    })
}
