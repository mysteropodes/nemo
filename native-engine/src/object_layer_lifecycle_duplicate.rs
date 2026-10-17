//! Duplicate-layer plan (F07/F08, L02-E): `uniqueLayerName` and the
//! document-visible part of `duplicateLayer` / `dupOne` (`src/js/timeline.js`,
//! fbe4b8a3), over the native layer, object, holder and order records.
//!
//! The baseline copies a layer's frames by JSON round trip, so every stroke
//! keeps its stroke id; native identity is (context, layer, stroke), which
//! makes the copy unique through its fresh layer uid alone. Holders, order
//! entries and elements are therefore re-targeted to the new layer and keep
//! their stroke ids. Folder children, mesh and rig remaps are not records of
//! this document yet.
use std::collections::HashSet;

use crate::object_document::object_group_kernels::object_layer_lifecycle_plan::{
    consult, index_of, order, LayerEditPolicy, LayerOp, LifecycleError, LifecyclePlan,
    SelectionOutcome, MAX_REVISION,
};
use crate::object_document::{ObjectDocument, ObjectRecord, Presence, PropertyHolder};

/// `uniqueLayerName(base)`: `base` when free, else the first free `base N` from 2.
pub(crate) fn unique_layer_name(base: &str, existing: &[&str]) -> String {
    if !existing.contains(&base) {
        return base.to_string();
    }
    let mut number: u64 = 2;
    loop {
        let candidate = format!("{base} {number}");
        if !existing.contains(&candidate.as_str()) {
            return candidate;
        }
        number += 1;
    }
}

/// Copies `source` to the top of the layer array as `native-layer:{revision}:{n}`
/// (the first free suffix, as layer-create picks it), named `uniqueLayerName(name
/// + " copy")`, and makes the copy the whole selection. Every authored field of the
/// layer (visibility, lock, color, parent, timing) is copied with its Presence.
pub(crate) fn plan_duplicate(
    document: &ObjectDocument,
    source: &str,
    next_revision: u64,
    policy: &dyn LayerEditPolicy,
) -> Result<LifecyclePlan, LifecycleError> {
    if next_revision > MAX_REVISION {
        return Err(LifecycleError::RevisionOutOfRange);
    }
    let index = index_of(document, source)?;
    let layer = &document.layers[index];
    let name = layer
        .name
        .value()
        .ok_or_else(|| LifecycleError::SourceUnnamed(source.to_string()))?;
    consult(policy, LayerOp::Duplicate, layer)?;
    let before = document.layers.len();
    let slot = u32::try_from(before).map_err(|_| LifecycleError::TooManyLayers)?;
    let taken: HashSet<&str> = document
        .layers
        .iter()
        .map(|existing| existing.layer_uid.as_str())
        .collect();
    let fresh = (0..=taken.len())
        .map(|suffix| format!("native-layer:{next_revision}:{suffix}"))
        .find(|candidate| !taken.contains(candidate.as_str()))
        .ok_or(LifecycleError::IdUnavailable)?;
    let names: Vec<&str> = document
        .layers
        .iter()
        .filter_map(|existing| existing.name.value().map(String::as_str))
        .collect();
    let mut copy = layer.clone();
    copy.layer_uid = fresh.clone();
    copy.name = Presence::Present(unique_layer_name(&format!("{name} copy"), &names));
    let mut candidate = document.clone();
    candidate.layers.push(copy);
    let records: Vec<ObjectRecord> = document
        .objects
        .iter()
        .filter(|record| record.target.layer_uid == source)
        .map(|record| {
            let mut record = record.clone();
            record.target.layer_uid = fresh.clone();
            record
        })
        .collect();
    candidate.objects.extend(records);
    if let Presence::Present(holders) = &mut candidate.property_holders {
        let copies: Vec<PropertyHolder> = holders
            .iter()
            .filter(|holder| holder.target.layer_uid == source)
            .map(|holder| {
                let mut holder = holder.clone();
                holder.target.layer_uid = fresh.clone();
                holder
            })
            .collect();
        holders.extend(copies);
    }
    order::append_duplicate(&mut candidate, source, &fresh, slot)?;
    Ok(LifecyclePlan {
        document: candidate,
        changed: true,
        selection: Some(SelectionOutcome {
            active: before,
            selected: vec![before],
            anchor: Some(before),
        }),
        removed: Vec::new(),
        touched: Vec::new(),
        spared: None,
        created: Some(fresh),
    })
}
