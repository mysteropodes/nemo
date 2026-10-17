//! L08-J (G11): the stored layer compositing fields of the baseline layer (`ld`), as optional members of
//! `ObjectLayer`: `blendMode`, the first matte (`matteMode`, `matteSourceLayerUid`) with its additional mattes
//! (`mattesMore`), the layer kind flags `isEffectLayer`, `isFolderLayer`, `folderCollapsed`, the ordered
//! `effects` stack and the borrowed stack `effectsFrom`. Names, member order (the `exportJSON` layer map of
//! `src/js/timeline.js` at fbe4b8a3) and value shapes are the baseline's; the record only stores and admits.
//! Blend, matte and effect EVALUATION stays in `scene_effect_order`, `scene_effect_blend_ref` and
//! `scene_effect_params` (render-scene layer), which a later join feeds from these accessors.
//!
//! Kept from the baseline and pinned against an executed oracle:
//! - absent, an explicit `"normal"` / `"none"` / `false`, and an empty stack are different stored values;
//! - an unknown blend or matte NAME is stored as authored (the baseline renders it as Normal / no matte);
//! - a matte source is a layer UID, never an index, and survives remapping through
//!   [`remap_compositing_refs`] (the `transplant.js` block) and reordering;
//! - the legacy matte (a mode with no source UID means "the layer above") is frozen into an explicit UID by
//!   [`freeze_legacy_matte_sources`], the loop `importJSON` runs after every layer exists;
//! - the UI write rules ([`blend_write`]: Normal clears the member).
//!
//! Deliberate differences, each pinned by a test: an empty `mattesMore` and an empty UID reference are refused
//! (the baseline import silently drops the former and treats the latter as the legacy marker), an effect
//! entry is exactly `{type, enabled, p1..p8}` (no `keys`: keyed effect parameters are property tracks and stay
//! with the property owners), `blendKeys` is not admitted (animated blend belongs to the property provider),
//! `folderCollapsed` needs a folder layer, a layer is not both an effect layer and a folder, and an effect or
//! folder layer owns no drawing objects (the baseline never renders their content: `getEffectiveStrokes`
//! returns `[]` for an effect layer and the folder wire entry carries no items).
//!
//! The canvas background is NOT a layer field: the baseline `isCanvasBackground` entry is synthesized from the
//! project `canvasBg`, which the project settings record already admits.
use crate::object_document::{object_deserialize, ObjectLayer, ObjectRecord, Presence};
use serde::{Deserialize, Serialize};
use serde_json::Number;
use std::collections::HashSet;

/// The 16 baseline blend names, in the order of `BLEND_MODE_LABELS` (`timeline.js`).
pub(crate) const BLEND_MODE_NAMES: [&str; 16] = [
    "normal",
    "multiply",
    "screen",
    "overlay",
    "darken",
    "lighten",
    "colorDodge",
    "colorBurn",
    "hardLight",
    "softLight",
    "difference",
    "exclusion",
    "hue",
    "saturation",
    "color",
    "luminosity",
];
/// The matte modes the matte menus offer (`none` is only ever authored by older files).
pub(crate) const MATTE_MODE_NAMES: [&str; 4] = ["alpha", "alphaInverted", "luma", "lumaInverted"];
/// `addFolderLayer` colours its layer explicitly.
pub(crate) const FOLDER_LAYER_COLOR: &str = "#ffb020";

/// One additional matte (`mattesMore[i]`): a layer UID and a mode name, nothing else.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct MatteRef {
    pub(crate) uid: String,
    pub(crate) mode: String,
}
object_deserialize!(MatteRef {
    uid: String,
    mode: String
});

/// One effect stack entry, the shape `addEffect` pushes: `{type, enabled, p1..p8}`. Each `p` is absent, an
/// explicit null or a number, and the three stay distinct (`undefined` is omitted by the baseline JSON).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct LayerEffect {
    #[serde(rename = "type")]
    pub(crate) effect_type: String,
    pub(crate) enabled: bool,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) p1: Presence<Option<Number>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) p2: Presence<Option<Number>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) p3: Presence<Option<Number>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) p4: Presence<Option<Number>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) p5: Presence<Option<Number>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) p6: Presence<Option<Number>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) p7: Presence<Option<Number>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) p8: Presence<Option<Number>>,
}
object_deserialize!(LayerEffect {
    #[serde(rename = "type")] effect_type: String,
    enabled: bool,
    #[serde(default)] p1: Presence<Option<Number>>,
    #[serde(default)] p2: Presence<Option<Number>>,
    #[serde(default)] p3: Presence<Option<Number>>,
    #[serde(default)] p4: Presence<Option<Number>>,
    #[serde(default)] p5: Presence<Option<Number>>,
    #[serde(default)] p6: Presence<Option<Number>>,
    #[serde(default)] p7: Presence<Option<Number>>,
    #[serde(default)] p8: Presence<Option<Number>>
});

impl LayerEffect {
    /// The stored number of `p<index+1>`; an absent member and an explicit null both give `None`.
    pub(crate) fn param(&self, index: usize) -> Option<f64> {
        let slot = match index {
            0 => &self.p1,
            1 => &self.p2,
            2 => &self.p3,
            3 => &self.p4,
            4 => &self.p5,
            5 => &self.p6,
            6 => &self.p7,
            7 => &self.p8,
            _ => return None,
        };
        slot.value()
            .and_then(Option::as_ref)
            .and_then(Number::as_f64)
    }
}

/// Why a layer's compositing fields are not admissible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LayerCompositingError {
    EmptyReference,
    EmptyMatteStack,
    EffectAndFolder,
    CollapsedWithoutFolder,
    KindLayerHasObjects,
}

impl LayerCompositingError {
    pub(crate) fn message(self) -> &'static str {
        match self {
            LayerCompositingError::EmptyReference => "a layer uid reference must not be empty",
            LayerCompositingError::EmptyMatteStack => "an empty mattesMore must be omitted",
            LayerCompositingError::EffectAndFolder => {
                "a layer is not both an effect layer and a folder layer"
            }
            LayerCompositingError::CollapsedWithoutFolder => {
                "folderCollapsed requires a folder layer"
            }
            LayerCompositingError::KindLayerHasObjects => {
                "an effect layer or a folder layer carries no objects"
            }
        }
    }
}

/// `ld.blendMode = v === 'normal' ? undefined : v` (the blend dropdown write): `None` clears the member.
pub(crate) fn blend_write(mode: &str) -> Option<&str> {
    if mode == "normal" {
        None
    } else {
        Some(mode)
    }
}

fn empty_reference(slot: &Presence<String>) -> bool {
    slot.value().is_some_and(String::is_empty)
}

/// Admission over every declared layer, called once from the shared `validate_content`.
pub(crate) fn validate_layer_compositing(
    layers: &[ObjectLayer],
    objects: &[ObjectRecord],
) -> Result<(), LayerCompositingError> {
    for layer in layers {
        if empty_reference(&layer.matte_source_layer_uid) || empty_reference(&layer.effects_from) {
            return Err(LayerCompositingError::EmptyReference);
        }
        if let Some(extras) = layer.mattes_more.value() {
            if extras.is_empty() {
                return Err(LayerCompositingError::EmptyMatteStack);
            }
            if extras.iter().any(|extra| extra.uid.is_empty()) {
                return Err(LayerCompositingError::EmptyReference);
            }
        }
        if layer.is_effect_layer() && layer.is_folder_layer() {
            return Err(LayerCompositingError::EffectAndFolder);
        }
        if !layer.folder_collapsed.missing() && !layer.is_folder_layer() {
            return Err(LayerCompositingError::CollapsedWithoutFolder);
        }
    }
    let kind_layers: HashSet<&str> = layers
        .iter()
        .filter(|layer| layer.is_effect_layer() || layer.is_folder_layer())
        .map(|layer| layer.layer_uid.as_str())
        .collect();
    if !kind_layers.is_empty()
        && objects
            .iter()
            .any(|record| kind_layers.contains(record.target.layer_uid.as_str()))
    {
        return Err(LayerCompositingError::KindLayerHasObjects);
    }
    Ok(())
}

impl ObjectLayer {
    /// The stored blend name; `None` is absent (an explicit `"normal"` is `Some("normal")`).
    pub(crate) fn blend_mode(&self) -> Option<&str> {
        self.blend_mode.value().map(String::as_str)
    }
    /// The stored first-matte mode; `None` is absent (an explicit `"none"` is `Some("none")`).
    pub(crate) fn matte_mode(&self) -> Option<&str> {
        self.matte_mode.value().map(String::as_str)
    }
    /// The first matte's source layer UID, if one is stored.
    pub(crate) fn matte_source_uid(&self) -> Option<&str> {
        self.matte_source_layer_uid.value().map(String::as_str)
    }
    /// The additional mattes in stored order (empty when the member is absent).
    pub(crate) fn mattes_more(&self) -> &[MatteRef] {
        self.mattes_more.value().map(Vec::as_slice).unwrap_or(&[])
    }
    /// The ordered effect stack; `None` is absent, `Some(&[])` an authored empty stack.
    pub(crate) fn effects(&self) -> Option<&[LayerEffect]> {
        self.effects.value().map(Vec::as_slice)
    }
    /// The UID of the layer whose stack this layer borrows (`effectsFrom`).
    pub(crate) fn effects_from(&self) -> Option<&str> {
        self.effects_from.value().map(String::as_str)
    }
    pub(crate) fn is_effect_layer(&self) -> bool {
        self.is_effect_layer.value().copied().unwrap_or(false)
    }
    pub(crate) fn is_folder_layer(&self) -> bool {
        self.is_folder_layer.value().copied().unwrap_or(false)
    }
    pub(crate) fn folder_collapsed(&self) -> bool {
        self.folder_collapsed.value().copied().unwrap_or(false)
    }
    /// The matte pill is "on" for any mode that is neither empty nor `"none"` (`renderMatteRow`).
    pub(crate) fn matte_on(&self) -> bool {
        self.matte_mode()
            .is_some_and(|mode| !mode.is_empty() && mode != "none")
    }
    /// Every layer UID this layer's compositing fields point at: the first matte source, the additional
    /// mattes in order, then the borrowed stack.
    pub(crate) fn compositing_reference_uids(&self) -> Vec<&str> {
        let mut uids: Vec<&str> = Vec::new();
        uids.extend(self.matte_source_uid());
        uids.extend(self.mattes_more().iter().map(|extra| extra.uid.as_str()));
        uids.extend(self.effects_from());
        uids
    }
    /// `addEffectLayer`: an effect layer starts with an authored empty stack.
    pub(crate) fn blank_effect_layer(
        layer_uid: String,
        name: String,
        existing_layers: usize,
    ) -> Self {
        let mut layer = Self::blank_drawing(layer_uid, name, existing_layers);
        layer.is_effect_layer = Presence::Present(true);
        layer.effects = Presence::Present(Vec::new());
        layer
    }
    /// `addFolderLayer`: expanded, an authored empty stack scoped to the folder, the folder colour. The
    /// baseline also marks the folder as a null layer (`isNullLayer`, `nullShape`); no native null-layer
    /// record exists, so only the folder fields are stored.
    pub(crate) fn blank_folder_layer(
        layer_uid: String,
        name: String,
        existing_layers: usize,
    ) -> Self {
        let mut layer = Self::blank_drawing(layer_uid, name, existing_layers);
        layer.is_folder_layer = Presence::Present(true);
        layer.folder_collapsed = Presence::Present(false);
        layer.effects = Presence::Present(Vec::new());
        layer.color = Presence::Present(FOLDER_LAYER_COLOR.into());
        layer
    }
}

/// The `transplant.js` uid remap of the matte fields, extended to the borrowed stack. `resolve` maps an old
/// UID to its new one, or `None` when that layer did not come along. A first source that does not resolve
/// takes the whole first matte with it (the baseline never copies `matteMode`, and a mode without a source
/// would read as the legacy "layer above"); additional mattes that do not resolve are dropped and an emptied
/// stack is removed; an unresolved `effectsFrom` is dropped. The identity map changes nothing.
pub(crate) fn remap_compositing_refs(
    layer: &mut ObjectLayer,
    resolve: &dyn Fn(&str) -> Option<String>,
) {
    if let Some(old) = layer.matte_source_layer_uid.value().cloned() {
        match resolve(&old) {
            Some(new) => layer.matte_source_layer_uid = Presence::Present(new),
            None => {
                layer.matte_mode = Presence::Missing;
                layer.matte_source_layer_uid = Presence::Missing;
            }
        }
    }
    let kept: Option<Vec<MatteRef>> = layer.mattes_more.value().map(|extras| {
        extras
            .iter()
            .filter_map(|extra| {
                resolve(&extra.uid).map(|uid| MatteRef {
                    uid,
                    mode: extra.mode.clone(),
                })
            })
            .collect()
    });
    if let Some(kept) = kept {
        layer.mattes_more = if kept.is_empty() {
            Presence::Missing
        } else {
            Presence::Present(kept)
        };
    }
    if let Some(old) = layer.effects_from.value().cloned() {
        layer.effects_from = match resolve(&old) {
            Some(new) => Presence::Present(new),
            None => Presence::Missing,
        };
    }
}

/// The `importJSON` matte migration: a layer with a real mode (neither empty nor `"none"`) and no usable
/// source UID takes the UID of the layer directly after it; the last layer stays unset (a no-op matte).
pub(crate) fn freeze_legacy_matte_sources(layers: &mut [ObjectLayer]) {
    for index in 0..layers.len().saturating_sub(1) {
        let source_missing = layers[index]
            .matte_source_layer_uid
            .value()
            .map_or(true, String::is_empty);
        if layers[index].matte_on() && source_missing {
            let next = layers[index + 1].layer_uid.clone();
            layers[index].matte_source_layer_uid = Presence::Present(next);
        }
    }
}

#[cfg(test)]
#[path = "../tests/scene_effect_layer_record_controls.rs"]
mod controls_tests;
#[cfg(test)]
#[path = "../tests/scene_effect_layer_record_rows.rs"]
mod oracle_rows;
#[cfg(test)]
#[path = "../tests/scene_effect_layer_record_refs.rs"]
mod refs_tests;
#[cfg(test)]
#[path = "../tests/scene_effect_layer_record.rs"]
mod tests;
