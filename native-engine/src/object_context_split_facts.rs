//! Layer, stroke and carry-rule facts behind the element-splitting plan (L04-H).
//!
//! The plain-data view of a layer that `layerElements`, `elementLabel` and
//! `splitLayerIntoElementsCore` (`src/js/motion.js`, `src/js/app.js` at baseline
//! `fbe4b8a3`) read, the element listing with its label, and the table of per-layer
//! fields every split piece inherits. Pure data and pure functions: no document,
//! no id minting, no Component creation. The plan itself is `object_context_split`.

use std::collections::BTreeMap;

/// `SM.t('autoNameShape')` / `SM.t('autoNameStroke')` of the active locale.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ElementWords {
    pub shape: String,
    pub stroke: String,
}

/// One stroke of a layer, as far as element listing and splitting read it.
/// Empty ids count as absent, as the baseline's truthiness tests do.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct StrokeFacts {
    pub stroke_id: Option<String>,
    pub is_raster: bool,
    pub has_fill: bool,
    pub is_brush_texture_copy: bool,
    pub is_linked_fill_companion: bool,
    pub linked_fill_id: Option<String>,
    pub group_id: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct FrameFacts {
    pub is_keyframe: bool,
    pub is_interpolated: bool,
    pub strokes: Vec<StrokeFacts>,
}

/// `ld.elementMotion[strokeId]`: which of its two tracks are present.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ElementMotion {
    pub has_motion: bool,
    pub has_motion_static: bool,
}

/// A layer field as the carry tests see it: absent, `null`, defined but falsy,
/// an empty array/object (truthy, no length) or a non-empty value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FieldState {
    Undefined,
    Null,
    Falsy,
    Empty,
    Truthy,
}

impl FieldState {
    pub(crate) fn truthy(self) -> bool {
        matches!(self, FieldState::Empty | FieldState::Truthy)
    }
}

/// A layer (`state.layers[i]`) as splitting reads it. `reference_strokes` are the
/// effective strokes at the playhead (what `layerElements` lists).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct LayerFacts {
    pub name: Option<String>,
    pub has_symbol: bool,
    pub frames: Vec<FrameFacts>,
    pub reference_strokes: Vec<StrokeFacts>,
    pub shape_names: BTreeMap<String, String>,
    pub element_motion: BTreeMap<String, ElementMotion>,
    pub fields: BTreeMap<String, FieldState>,
    /// `Object.keys(ld.groups)`; `None` when the layer has no groups object.
    pub groups: Option<Vec<String>>,
}

impl LayerFacts {
    pub(crate) fn state(&self, field: &str) -> FieldState {
        self.fields
            .get(field)
            .copied()
            .unwrap_or(FieldState::Undefined)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CarryMode {
    /// Assigned as is (a primitive or a shared reference).
    Verbatim,
    /// `JSON.parse(JSON.stringify(v))`.
    DeepCopy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CarryWhen {
    /// Copied whenever defined (`null` included).
    Defined,
    /// `ld.f ? copy : undefined`.
    Truthy,
    /// `ld.f && ld.f.length`.
    NonEmpty,
    /// `ld.f != null`.
    NotNull,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CarryRule {
    pub field: &'static str,
    pub mode: CarryMode,
    pub when: CarryWhen,
}

const fn rule(field: &'static str, mode: CarryMode, when: CarryWhen) -> CarryRule {
    CarryRule { field, mode, when }
}

/// The per-layer fields every piece inherits, in the baseline's key order. The
/// matte fields, `layerUid` and `groups` follow their own rules; `duplicator` is
/// deliberately absent (a copy per piece would multiply it).
pub(crate) const CARRY_RULES: &[CarryRule] = &[
    rule("blendMode", CarryMode::Verbatim, CarryWhen::Defined),
    rule("parentLayerUid", CarryMode::Verbatim, CarryWhen::Defined),
    rule("parentLayerUidB", CarryMode::Verbatim, CarryWhen::Defined),
    rule("timeLink", CarryMode::DeepCopy, CarryWhen::Truthy),
    rule("inPoint", CarryMode::Verbatim, CarryWhen::Defined),
    rule("outPoint", CarryMode::Verbatim, CarryWhen::Defined),
    rule("expressions", CarryMode::DeepCopy, CarryWhen::Truthy),
    rule("exprControls", CarryMode::DeepCopy, CarryWhen::NonEmpty),
    rule("threeD", CarryMode::Verbatim, CarryWhen::Defined),
    rule("motionBlur", CarryMode::Verbatim, CarryWhen::Defined),
    rule("effects", CarryMode::DeepCopy, CarryWhen::NotNull),
    rule("pathFx", CarryMode::DeepCopy, CarryWhen::NotNull),
    rule("blendKeys", CarryMode::DeepCopy, CarryWhen::NotNull),
    rule("parentKeys", CarryMode::DeepCopy, CarryWhen::NotNull),
    rule("parentsMore", CarryMode::DeepCopy, CarryWhen::NotNull),
    rule("followPath", CarryMode::DeepCopy, CarryWhen::NotNull),
    rule("markers", CarryMode::DeepCopy, CarryWhen::NotNull),
    rule("textAnimators", CarryMode::DeepCopy, CarryWhen::NotNull),
    rule("shy", CarryMode::Verbatim, CarryWhen::NotNull),
    rule("keyLock", CarryMode::Verbatim, CarryWhen::NotNull),
    rule("channel", CarryMode::Verbatim, CarryWhen::NotNull),
    rule("effectsFrom", CarryMode::Verbatim, CarryWhen::NotNull),
    rule("folderId", CarryMode::Verbatim, CarryWhen::NotNull),
    rule("linkGroupId", CarryMode::Verbatim, CarryWhen::NotNull),
    rule("isTextLayer", CarryMode::Verbatim, CarryWhen::NotNull),
];

pub(crate) fn rule_applies(when: CarryWhen, state: FieldState) -> bool {
    match when {
        CarryWhen::Defined => state != FieldState::Undefined,
        CarryWhen::Truthy => state.truthy(),
        CarryWhen::NonEmpty => state == FieldState::Truthy,
        CarryWhen::NotNull => !matches!(state, FieldState::Undefined | FieldState::Null),
    }
}

/// One element of the reference frame (`layerElements` entry).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ElementEntry {
    pub stroke_id: Option<String>,
    /// Index into [`LayerFacts::reference_strokes`].
    pub position: usize,
    pub is_raster: bool,
    /// The fill the label sees: the linked companion's when there is one.
    pub has_fill: bool,
}

pub(crate) fn present(id: &Option<String>) -> Option<&str> {
    id.as_deref().filter(|s| !s.is_empty())
}

/// `layerElements`: the reference strokes minus texture copies and companions.
pub(crate) fn layer_elements(layer: &LayerFacts) -> Vec<ElementEntry> {
    let mut companions: BTreeMap<&str, &StrokeFacts> = BTreeMap::new();
    for stroke in &layer.reference_strokes {
        if let (true, Some(link)) = (
            stroke.is_linked_fill_companion,
            present(&stroke.linked_fill_id),
        ) {
            companions.insert(link, stroke);
        }
    }
    let mut out = Vec::new();
    for (position, stroke) in layer.reference_strokes.iter().enumerate() {
        if stroke.is_brush_texture_copy || stroke.is_linked_fill_companion {
            continue;
        }
        let companion = present(&stroke.linked_fill_id).and_then(|link| companions.get(link));
        out.push(ElementEntry {
            stroke_id: present(&stroke.stroke_id).map(|s| s.to_string()),
            position,
            is_raster: stroke.is_raster,
            has_fill: companion.map_or(stroke.has_fill, |c| c.has_fill),
        });
    }
    out
}

/// The reference strokes `layerElements` would stamp a fresh id on: the elements
/// and the linked-fill companions that have none. The owner assigns them.
pub(crate) fn unstamped_positions(layer: &LayerFacts) -> Vec<usize> {
    let mut out = Vec::new();
    for (position, stroke) in layer.reference_strokes.iter().enumerate() {
        let lacks_id = present(&stroke.stroke_id).is_none();
        let companion_with_link =
            stroke.is_linked_fill_companion && present(&stroke.linked_fill_id).is_some();
        let element = !stroke.is_brush_texture_copy && !stroke.is_linked_fill_companion;
        if lacks_id && (companion_with_link || element) {
            out.push(position);
        }
    }
    out
}

/// `elementLabel(entry, idx, ld)`.
pub(crate) fn element_label(
    entry: &ElementEntry,
    index: usize,
    layer: &LayerFacts,
    words: &ElementWords,
) -> String {
    if let Some(name) = entry
        .stroke_id
        .as_ref()
        .and_then(|id| layer.shape_names.get(id))
    {
        if !name.is_empty() {
            return name.clone();
        }
    }
    if entry.is_raster {
        return format!("Image {}", index + 1);
    }
    let word = if entry.has_fill {
        &words.shape
    } else {
        &words.stroke
    };
    format!("{} {}", word, index + 1)
}
