//! Holder identity and ownership changes of the existing component convert plan.
//! The authoritative table remains ObjectDocument.property_holders. These pure
//! transforms clone only the sealed candidate's authored values; they do not own
//! revisions, run expressions or infer values from a missing provider.
use crate::object_document::{ObjectDocument, Presence, PropertyHolder, PropertyHolderTarget};

fn scene_source(holder: &PropertyHolder, uid: &str) -> bool {
    holder.target.context_id == "scene-root" && holder.target.layer_uid == uid
}

/// The surviving single-layer instance keeps its own Motion. Element holders
/// follow the source's stable stroke IDs into the fresh inner layer.
pub(super) fn single(
    document: &ObjectDocument,
    source_uid: &str,
    symbol_id: &str,
    inner_uid: &str,
) -> Presence<Vec<PropertyHolder>> {
    match &document.property_holders {
        Presence::Missing => Presence::Missing,
        Presence::Present(holders) => Presence::Present(
            holders
                .iter()
                .map(|holder| {
                    let mut moved = holder.clone();
                    if scene_source(holder, source_uid) && holder.target.stroke_id().is_some() {
                        moved.target.context_id = symbol_id.to_owned();
                        moved.target.layer_uid = inner_uid.to_owned();
                    }
                    moved
                })
                .collect(),
        ),
    }
}

/// Split the first source's layer holder at the baseline's lifted ownership:
/// expression controls, their complete value/track records and built-in property
/// expressions belong to the outer instance. Built-in raw Motion stays inside.
/// A control descriptor can never be separated from its raw value/track, because
/// native admission and sampling derive that property's metadata from it.
fn lift_first(inner: &mut PropertyHolder, instance_uid: &str) -> Option<PropertyHolder> {
    let controls = std::mem::take(&mut inner.controls);
    let mut outer = PropertyHolder {
        target: PropertyHolderTarget {
            context_id: "scene-root".to_owned(),
            layer_uid: instance_uid.to_owned(),
            stroke_id: Presence::Missing,
        },
        controls,
        properties: Vec::new(),
    };
    let properties = std::mem::take(&mut inner.properties);
    for mut property in properties {
        let control = outer.controls.value().is_some_and(|controls| {
            controls
                .iter()
                .any(|control| control.key == property.property)
        });
        if control {
            outer.properties.push(property);
            continue;
        }
        if !property.expression.missing() {
            let mut expression = property.clone();
            expression.static_value = Presence::Missing;
            expression.track = Presence::Missing;
            outer.properties.push(expression);
            property.expression = Presence::Missing;
        }
        inner.properties.push(property);
    }
    (!outer.controls.missing() || !outer.properties.is_empty()).then_some(outer)
}

/// Preserve encounter order and all unaffected contexts. The inner holder keeps
/// its original slot; its one lifted outer holder follows immediately. No source
/// layer UID or stroke ID is minted or flattened across component contexts.
pub(super) fn merged(
    document: &ObjectDocument,
    sources: &[String],
    symbol_id: &str,
    instance_uid: &str,
) -> Presence<Vec<PropertyHolder>> {
    let original = match &document.property_holders {
        Presence::Missing => return Presence::Missing,
        Presence::Present(holders) => holders,
    };
    let mut result = Vec::with_capacity(original.len());
    for holder in original {
        let selected =
            holder.target.context_id == "scene-root" && sources.contains(&holder.target.layer_uid);
        let mut inner = holder.clone();
        let outer = if selected {
            inner.target.context_id = symbol_id.to_owned();
            if sources.first().is_some_and(|uid| scene_source(holder, uid))
                && holder.target.stroke_id().is_none()
            {
                lift_first(&mut inner, instance_uid)
            } else {
                None
            }
        } else {
            None
        };
        result.push(inner);
        if let Some(outer) = outer {
            result.push(outer);
        }
    }
    Presence::Present(result)
}
