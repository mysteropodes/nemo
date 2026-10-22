//! Atomic batch import transaction (L10-H; G35, G36, G37, I46): the planner that turns one
//! typed import batch into ONE new `ObjectDocument` sealed to one `ObjectSnapshot` origin.
//!
//! A batch is new layers, vector/raster/Text records, admitted assets and explicit skips. The planner
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
//! - a missing project envelope or required asset/font provider refuses the whole batch;
//!   no item is dropped, no provider is faked and no empty project is synthesized.

use crate::commands::{insert_order_element, ImportBatchResult, ImportSkipReport, OrderEditError};
use crate::history::object_history::object_shot_kernels::object_import_remap_ids::{
    build_remap_table, OriginIds, RemapError, RemapKind, RemapTable, Resolved, SourceEntry,
};
use crate::object_document::{
    AssetInventory, AuthoredOrder, FrameScopeKind, ObjectDocument, ObjectLayer, ObjectTarget,
    OrderLayer, Presence, TimingEdit, VECTOR_FAMILY,
};
use crate::revision::ObjectSnapshot;

#[path = "native_import_transaction_assets.rs"]
mod assets;
#[path = "native_import_transaction_session.rs"]
pub(crate) mod session;
#[path = "native_import_transaction_svg.rs"]
pub(crate) mod svg;
#[path = "native_import_transaction_types.rs"]
mod types;
pub(crate) use types::{
    ImportAsset, ImportBatch, ImportError, ImportRaster, ImportRecord, ImportSkip, ImportText,
    MissingProvider, NewLayer, SkipKind, UnavailableElement,
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

#[cfg(all(test, feature = "project_media_codec"))]
#[path = "../tests/native_import_transaction_asset_controls.rs"]
mod asset_controls;
#[cfg(all(test, feature = "project_media_codec"))]
#[path = "../tests/native_import_transaction_assets.rs"]
mod asset_tests;

/// The after-document (absent for a no-op) and the receipt the commit will report.
#[derive(Debug)]
pub(crate) struct ImportPlan {
    pub(crate) document: Option<ObjectDocument>,
    pub(crate) result: ImportBatchResult,
    pub(crate) asset_inventory: Option<AssetInventory>,
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
                .map(|record| record.target.stroke_id.as_str())
                .chain(
                    document
                        .item_records()
                        .into_iter()
                        .flat_map(|items| items.targets())
                        .map(|target| target.stroke_id.as_str()),
                ),
        )
        .with_ids(
            RemapKind::Asset,
            origin
                .project()
                .and_then(|project| project.asset_inventory())
                .into_iter()
                .flat_map(|inventory| inventory.records())
                .map(|record| record.id().as_str()),
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

/// Shared native target placement for each admitted record family.
fn place_target(
    document: &mut ObjectDocument,
    target: &mut ObjectTarget,
    source_id: &str,
    ids: &OriginIds,
    table: &RemapTable,
    index: usize,
) -> Result<(), ImportError> {
    if !target.stroke_id.is_empty() {
        return Err(ImportError::StrokeIdPreset { index });
    }
    if target.frame_scope.kind != FrameScopeKind::Authored {
        return Err(ImportError::ReferenceScope { index });
    }
    let reference = target.layer_uid.as_str();
    let layer = match table.resolve(ids, RemapKind::Layer, reference) {
        Ok(resolved) => resolved,
        Err(RemapError::DanglingReference { reference, .. }) => {
            return Err(ImportError::UnknownLayer { index, reference })
        }
        Err(RemapError::ReferenceToSkipped { reference, .. }) => {
            return Err(ImportError::SkippedLayer { index, reference })
        }
        Err(other) => return Err(ImportError::Remap(other)),
    };
    target.layer_uid = layer.as_str().to_owned();
    target.stroke_id = table
        .native_id(RemapKind::Stroke, source_id)
        .ok_or(ImportError::Internal)?
        .to_owned();
    if !document.valid_target(target) || target.frame() >= document.total_frames {
        return Err(ImportError::InvalidTarget { index });
    }
    expose_frame(
        document,
        &target.layer_uid,
        target.frame(),
        matches!(layer, Resolved::Batch(_)),
        index,
    )?;
    if let Presence::Present(plan) = &mut document.composition_order {
        insert_order_element(plan, target).map_err(|error| order_error(index, error))?;
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
    for item in &batch.raster {
        sources.push(SourceEntry::mapped(RemapKind::Stroke, &item.source_id));
    }
    for item in &batch.text {
        sources.push(SourceEntry::mapped(RemapKind::Stroke, &item.source_id));
    }
    for item in &batch.assets {
        sources.push(SourceEntry::mapped(RemapKind::Asset, &item.source_id));
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
    let (asset_inventory, asset_mappings) = assets::catalog(origin, batch, &table)?;
    if batch.layers.is_empty() && batch.records.is_empty() && !assets::needs_project(batch) {
        return Ok(ImportPlan {
            document: None,
            asset_inventory: None,
            result: ImportBatchResult {
                applied: false,
                layers: Vec::new(),
                objects: Vec::new(),
                assets: Vec::new(),
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
        let mut placed = record.clone();
        place_target(
            &mut document,
            &mut placed.target,
            &item.source_id,
            &ids,
            &table,
            index,
        )?;
        let stroke = placed.target.stroke_id.clone();
        document.objects.push(placed);
        objects.push((item.source_id.clone(), stroke));
    }
    let inventory = asset_inventory.as_ref().or_else(|| {
        origin
            .project()
            .and_then(|project| project.asset_inventory())
    });
    assets::append_items(&mut document, batch, &ids, &table, inventory, &mut objects)?;
    if let Some(inventory) = inventory {
        crate::object_document::media_timing::object_media_link_edit::check_inventory(
            &document, inventory,
        )
        .map_err(|_| ImportError::InvalidAsset { index: 0 })?;
    }
    if layers.len() + objects.len() + asset_mappings.len() + skipped.len() != table.source_count() {
        return Err(ImportError::Internal);
    }
    Ok(ImportPlan {
        document: Some(document),
        asset_inventory,
        result: ImportBatchResult {
            applied: true,
            layers,
            objects,
            assets: asset_mappings,
            skipped,
        },
    })
}
