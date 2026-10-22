//! Actual asset and item half of the batch planner. No publisher or encoded-byte copy.
//! Provider-admitted allocations are retained, remapped and checked with the native
//! codecs; the origin catalog and item lists remain unchanged prefixes.
use super::{ImportBatch, ImportError, MissingProvider};
use crate::history::object_history::object_shot_kernels::object_import_remap_ids::{
    OriginIds, RemapKind, RemapTable,
};
use crate::object_document::{
    AssetId, AssetInventory, AssetKind, AssetRecord, ItemRecords, ObjectDocument,
};
use crate::revision::ObjectSnapshot;
use serde_json::Value;
use std::sync::Arc;

pub(super) fn needs_project(batch: &ImportBatch) -> bool {
    !batch.assets.is_empty() || !batch.raster.is_empty() || !batch.text.is_empty()
}

fn unavailable(source: &str, need: MissingProvider) -> ImportError {
    ImportError::ProviderUnavailable {
        source_id: source.to_owned(),
        need,
    }
}

/// The same PNG limits and charged temporary workspace as project admission.
/// This validates a borrowed retained allocation; it never copies encoded bytes.
#[cfg(feature = "project_media_codec")]
fn validate_png(record: &AssetRecord, index: usize) -> Result<(), ImportError> {
    use crate::object_codec::raster_asset_admission::{
        MAX_DIMENSION, MAX_ENCODED_BYTES, MAX_RGBA_BYTES,
    };
    use crate::object_codec::raster_asset_png_port::MAX_DECODER_BYTES;
    use crate::object_codec::{ImageAssetLimits, PngWorkspace, ProjectWireError};
    use crate::object_snapshot::charged_png_workspace;
    use crate::revision::shared_image_resource_budget;
    let source = record.source();
    let bytes = source
        .encoded_allocation()
        .ok_or(ImportError::InvalidAsset { index })?;
    if source.media_type() != Some("image/png") {
        return Err(ImportError::InvalidAsset { index });
    }
    let limits = ImageAssetLimits::new(
        MAX_ENCODED_BYTES,
        MAX_RGBA_BYTES,
        MAX_DECODER_BYTES,
        MAX_DIMENSION,
        MAX_DIMENSION,
    )
    .map_err(|_| ImportError::Internal)?;
    let budget = shared_image_resource_budget();
    crate::object_codec::validate_png_with_buffers(
        bytes,
        limits,
        || {
            charged_png_workspace(budget, || Ok(PngWorkspace::new()))
                .map_err(|_| ProjectWireError::Budget)
        },
        |n| {
            budget
                .allocate_bytes(n)
                .map_err(|_| ProjectWireError::Budget)
        },
    )
    .map_err(|error| match error {
        ProjectWireError::Budget => ImportError::Overflow,
        _ => ImportError::InvalidAsset { index },
    })?;
    Ok(())
}

fn validate_asset(record: &AssetRecord, source: &str, index: usize) -> Result<(), ImportError> {
    match record.kind() {
        AssetKind::Image => {
            #[cfg(feature = "project_media_codec")]
            {
                validate_png(record, index)
            }
            #[cfg(not(feature = "project_media_codec"))]
            {
                Err(unavailable(source, MissingProvider::RasterAsset))
            }
        }
        AssetKind::Font => {
            if record.source().has_font_admission() {
                Ok(())
            } else {
                Err(unavailable(source, MissingProvider::Font))
            }
        }
        _ => Err(ImportError::InvalidAsset { index }),
    }
}

/// None means retain the original catalog, never remove it. An appended catalog
/// keeps every original Arc and placement in the exact original order.
pub(super) fn catalog(
    origin: &ObjectSnapshot,
    batch: &ImportBatch,
    table: &RemapTable,
) -> Result<(Option<AssetInventory>, Vec<(String, String)>), ImportError> {
    if needs_project(batch) && origin.project().is_none() {
        return Err(ImportError::ProjectRequired);
    }
    if batch.assets.is_empty() {
        return Ok((None, Vec::new()));
    }
    let previous = origin
        .project()
        .and_then(|project| project.asset_inventory());
    let mut records = previous.map_or_else(Vec::new, |inventory| inventory.records().to_vec());
    let placements = previous.map_or_else(Vec::new, |inventory| inventory.placements().to_vec());
    let mut mappings = Vec::with_capacity(batch.assets.len());
    for (index, item) in batch.assets.iter().enumerate() {
        validate_asset(&item.record, &item.source_id, index)?;
        let native = table
            .native_id(RemapKind::Asset, &item.source_id)
            .ok_or(ImportError::Internal)?
            .to_owned();
        let record = AssetRecord::new(
            AssetId::new(native.clone()).map_err(|_| ImportError::InvalidAsset { index })?,
            item.record.name().to_owned(),
            item.record.kind(),
            item.record.source().clone(),
        )
        .map_err(|_| ImportError::InvalidAsset { index })?;
        records.push(Arc::new(record));
        mappings.push((item.source_id.clone(), native));
    }
    let inventory =
        AssetInventory::new(records, placements, 1024, 1024).map_err(|_| ImportError::Overflow)?;
    Ok((Some(inventory), mappings))
}

fn asset_reference(
    ids: &OriginIds,
    table: &RemapTable,
    reference: &str,
) -> Result<String, ImportError> {
    table
        .resolve(ids, RemapKind::Asset, reference)
        .map(|resolved| resolved.as_str().to_owned())
        .map_err(ImportError::Remap)
}

/// Changing just the asset reference through the actual codecs preserves every
/// geometry/style presence and numeric spelling class. No geometry is inferred.
pub(super) fn append_items(
    document: &mut ObjectDocument,
    batch: &ImportBatch,
    ids: &OriginIds,
    table: &RemapTable,
    inventory: Option<&AssetInventory>,
    objects: &mut Vec<(String, String)>,
) -> Result<(), ImportError> {
    if batch.raster.is_empty() && batch.text.is_empty() {
        return Ok(());
    }
    let mut raster = document
        .item_records()
        .map_or_else(Vec::new, |items| items.raster_entries().to_vec());
    let mut text = document
        .item_records()
        .map_or_else(Vec::new, |items| items.text_entries().to_vec());
    for (offset, item) in batch.raster.iter().enumerate() {
        let index = batch.records.len() + offset;
        let mut placed = item.entry.clone();
        super::place_target(
            document,
            &mut placed.target,
            &item.source_id,
            ids,
            table,
            index,
        )?;
        let mut value = serde_json::to_value(&placed.item).map_err(|_| ImportError::Internal)?;
        value["assetId"] = Value::String(asset_reference(
            ids,
            table,
            placed.item.asset_id().as_str(),
        )?);
        placed.item =
            serde_json::from_value(value).map_err(|_| ImportError::InvalidAsset { index })?;
        objects.push((item.source_id.clone(), placed.target.stroke_id.clone()));
        raster.push(placed);
    }
    for (offset, item) in batch.text.iter().enumerate() {
        let index = batch.records.len() + batch.raster.len() + offset;
        let mut placed = item.entry.clone();
        super::place_target(
            document,
            &mut placed.target,
            &item.source_id,
            ids,
            table,
            index,
        )?;
        let mut value = serde_json::to_value(&placed.block).map_err(|_| ImportError::Internal)?;
        if value["font"]["kind"] == "asset" {
            let reference = value["font"]["assetId"]
                .as_str()
                .ok_or(ImportError::Internal)?;
            let native = asset_reference(ids, table, reference)?;
            let id =
                AssetId::new(native.clone()).map_err(|_| ImportError::InvalidAsset { index })?;
            let record = inventory
                .and_then(|inventory| inventory.get(&id))
                .ok_or(ImportError::InvalidAsset { index })?;
            if record.kind() != AssetKind::Font || !record.source().has_font_admission() {
                return Err(unavailable(&item.source_id, MissingProvider::Font));
            }
            value["font"]["assetId"] = Value::String(native);
            placed.block =
                serde_json::from_value(value).map_err(|_| ImportError::InvalidAsset { index })?;
        }
        objects.push((item.source_id.clone(), placed.target.stroke_id.clone()));
        text.push(placed);
    }
    document.items = ItemRecords::normalized(raster, text);
    crate::object_codec::validate_item_assets(document, inventory).map_err(|_| {
        ImportError::InvalidAsset {
            index: batch.records.len(),
        }
    })?;
    Ok(())
}
