//! Property metadata for a component's exposed keys on its instance layer (L04-K).
//!
//! Baseline: `exposedProps` of `state.symbols[symId]` surface as ordinary Motion properties on
//! every INSTANCE layer (`propsFor`, motion.js:189), keyed by `ep.key`, and their default is the
//! declared `ep.default` (`registerExposedPropMeta`, motion.js:134, one number, `PROP_DEFAULT =
//! [default]`). A key therefore exists only on a layer that places a component declaring it.
//! Native: dimension 1 for numbers; a colour target (`fillColor`/`strokeColor`) is dimension 4
//! (r, g, b, a in 0..255 like every colour property) and has no declared default, because the
//! declaration only authors a number: reading an unset colour is a typed refusal, not a guess.
use crate::object_document::object_component_record_kernels::object_component_records::{
    ComponentSet, SCENE_CONTEXT,
};
use crate::object_document::{PropertyHolder, PropertyHolderTarget, PropertyMetadata};

/// Colour-valued `targetField`s of an exposed declaration.
pub(crate) fn is_colour_field(target_field: &str) -> bool {
    matches!(target_field, "fillColor" | "strokeColor")
}

/// The metadata of `property` on `target` when `target` is a scene-root instance layer whose
/// component declares that key. Element holders, other contexts and foreign keys get `None`.
pub(crate) fn metadata(
    set: Option<&ComponentSet>,
    target: &PropertyHolderTarget,
    property: &str,
) -> Option<PropertyMetadata> {
    if target.context_id() != SCENE_CONTEXT || target.stroke_id().is_some() {
        return None;
    }
    let set = set?;
    let instance = set.instance(SCENE_CONTEXT, target.layer_uid())?;
    let prop = set
        .component(instance.symbol_id())?
        .exposed_props()
        .iter()
        .find(|prop| prop.key == property)?;
    let colour = is_colour_field(&prop.target_field);
    Some(PropertyMetadata {
        dimension: if colour { 4 } else { 1 },
        unit: "",
        default_value: if colour {
            None
        } else {
            prop.default_value.value().map(|n| vec![n.clone()])
        },
        animatable: true,
        control_type: None,
    })
}

/// The holder minus its exposed-key records, or `None` when it carries none. The component scene
/// join consumes exactly these records through the override port; what is left is judged by the
/// ordinary consumer predicates.
pub(crate) fn without_exposed(
    set: Option<&ComponentSet>,
    holder: &PropertyHolder,
) -> Option<PropertyHolder> {
    let is_exposed = |name: &str| metadata(set, holder.target(), name).is_some();
    if !holder.properties.iter().any(|p| is_exposed(&p.property)) {
        return None;
    }
    let mut rest = holder.clone();
    rest.properties.retain(|p| !is_exposed(&p.property));
    Some(rest)
}
