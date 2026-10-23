//! Component place and expose plans (L04-M, I14/I35): `placeComponentInstance` and
//! `exposeSymbolProperty` / `expose()` of `src/js/app.js` and
//! `src/js/domain/component/exposed-properties.js` at fbe4b8a3, as detached candidate
//! documents over the lane-4 component table. Declared as a private child of
//! `object_fill_owner.rs`.
use crate::commands::object_fill_owner::component_owner::object_component_convert_support::{
    append_layer, empty_component_set, mint_layer_uid, new_instance, require_v2, update,
    with_tables, SetupError, SetupPlan,
};
use crate::object_document::object_component_record_kernels::object_component_records::ExposedProp;
use crate::object_document::{ObjectDocument, ObjectLayer, Presence};
use serde_json::Number;

/// `placeComponentInstance(symId, layerName)`: a new blank locked layer that
/// instantiates an existing component (`once`, speed 1, placed 0, single frame 0).
pub(crate) fn plan_place(
    document: &ObjectDocument,
    next_revision: u64,
    symbol_id: &str,
    name: &str,
) -> Result<SetupPlan, SetupError> {
    require_v2(document)?;
    document
        .component_set()
        .and_then(|set| set.component(symbol_id))
        .ok_or_else(|| SetupError::UnknownComponent(symbol_id.to_string()))?;
    let uid = mint_layer_uid(document, next_revision)?;
    let mut candidate = document.clone();
    let mut layer =
        ObjectLayer::blank_drawing(uid.clone(), name.to_string(), candidate.layers.len());
    layer.locked = Presence::Present(true);
    append_layer(&mut candidate, layer)?;
    with_tables(&mut candidate, |tables| {
        update(&mut tables.components, empty_component_set, |set| {
            set.instances.push(new_instance(&uid, symbol_id));
        });
        Ok(())
    })?;
    Ok(SetupPlan {
        document: candidate,
        changed: true,
        symbol_id: Some(symbol_id.to_string()),
        layer_uid: Some(uid.clone()),
        created: Some(uid),
    })
}

/// One `expose()` entry. The key is made by the caller (`ep_<clock>_<random>`,
/// the L04-E kernel); admission checks its prefix and the target stroke.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ExposeEntry {
    pub(crate) key: String,
    pub(crate) label: String,
    pub(crate) target_stroke_id: String,
    pub(crate) target_field: String,
    pub(crate) default_value: Presence<Number>,
}

/// `exposeSymbolProperty` -> `expose()`: append the declaration to the component.
/// The same key with identical fields is a no-op; the same key with other fields
/// is refused (the baseline would append a second entry that collides).
pub(crate) fn plan_expose(
    document: &ObjectDocument,
    symbol_id: &str,
    entry: &ExposeEntry,
) -> Result<SetupPlan, SetupError> {
    require_v2(document)?;
    let component = document
        .component_set()
        .and_then(|set| set.component(symbol_id))
        .ok_or_else(|| SetupError::UnknownComponent(symbol_id.to_string()))?;
    let prop = ExposedProp {
        key: entry.key.clone(),
        label: entry.label.clone(),
        target_stroke_id: entry.target_stroke_id.clone(),
        target_field: entry.target_field.clone(),
        default_value: entry.default_value.clone(),
    };
    if let Some(existing) = component
        .exposed_props()
        .iter()
        .find(|p| p.key == entry.key)
    {
        return if *existing == prop {
            Ok(SetupPlan::unchanged(document, Some(symbol_id)))
        } else {
            Err(SetupError::DuplicateKey(entry.key.clone()))
        };
    }
    let mut candidate = document.clone();
    with_tables(&mut candidate, |tables| {
        update(&mut tables.components, empty_component_set, |set| match set
            .components
            .iter_mut()
            .find(|c| c.symbol_id == symbol_id)
        {
            Some(found) => {
                update(&mut found.exposed_props, Vec::new, |props| props.push(prop));
                Ok(())
            }
            None => Err(SetupError::UnknownComponent(symbol_id.to_string())),
        })
    })?;
    Ok(SetupPlan {
        document: candidate,
        changed: true,
        symbol_id: Some(symbol_id.to_string()),
        layer_uid: None,
        created: None,
    })
}

#[cfg(test)]
#[path = "../tests/object_component_convert_fixture.rs"]
mod fixture;
#[cfg(test)]
#[path = "../tests/object_component_place.rs"]
mod tests;
