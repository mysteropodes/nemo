//! Rename and recolor plans (F07, L02-E): `renameLayer` and `applyLayerColor`
//! (`src/js/timeline.js`, fbe4b8a3) over the native layer fields.
use crate::object_document::object_group_kernels::object_layer_lifecycle_plan::{
    consult, index_of, resolve_all, LayerEditPolicy, LayerOp, LifecycleError, LifecyclePlan,
};
use crate::object_document::{ObjectDocument, Presence};

/// `renameLayer(idx, n)`: the name is stored as given (no trim, no uniqueness
/// rule). Other layer fields keep their Presence; renaming to the stored name
/// is a no-op.
pub(crate) fn plan_rename(
    document: &ObjectDocument,
    layer_uid: &str,
    name: &str,
    policy: &dyn LayerEditPolicy,
) -> Result<LifecyclePlan, LifecycleError> {
    let index = index_of(document, layer_uid)?;
    if document.layers[index].name.value().map(String::as_str) == Some(name) {
        return Ok(LifecyclePlan::unchanged(document, None));
    }
    consult(document, policy, LayerOp::Rename, &document.layers[index])?;
    let mut candidate = document.clone();
    candidate.layers[index].name = Presence::Present(name.to_string());
    Ok(LifecyclePlan {
        document: candidate,
        changed: true,
        selection: None,
        removed: Vec::new(),
        touched: vec![layer_uid.to_string()],
        spared: None,
        created: None,
    })
}

/// The codec's layer color rule: `#` plus six hexadecimal digits.
fn valid_color(hex: &str) -> bool {
    hex.len() == 7 && hex.starts_with('#') && hex.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit)
}

/// `applyLayerColor(li, hex)`: with more than one selected layer and the clicked
/// layer among them the whole selection is painted, otherwise just the clicked
/// layer. The color is stored as given.
pub(crate) fn plan_recolor(
    document: &ObjectDocument,
    clicked: &str,
    selected: &[String],
    hex: &str,
    policy: &dyn LayerEditPolicy,
) -> Result<LifecyclePlan, LifecycleError> {
    if !valid_color(hex) {
        return Err(LifecycleError::InvalidColor(hex.to_string()));
    }
    let clicked_index = index_of(document, clicked)?;
    let mut targets: Vec<usize> = Vec::new();
    if selected.len() > 1 && selected.iter().any(|uid| uid == clicked) {
        for index in resolve_all(document, selected)? {
            if !targets.contains(&index) {
                targets.push(index);
            }
        }
    } else {
        targets.push(clicked_index);
    }
    targets.retain(|&index| document.layers[index].color.value().map(String::as_str) != Some(hex));
    if targets.is_empty() {
        return Ok(LifecyclePlan::unchanged(document, None));
    }
    for &index in &targets {
        consult(document, policy, LayerOp::Recolor, &document.layers[index])?;
    }
    let mut candidate = document.clone();
    for &index in &targets {
        candidate.layers[index].color = Presence::Present(hex.to_string());
    }
    Ok(LifecyclePlan {
        document: candidate,
        changed: true,
        selection: None,
        removed: Vec::new(),
        touched: targets
            .iter()
            .map(|&index| document.layers[index].layer_uid.clone())
            .collect(),
        spared: None,
        created: None,
    })
}
