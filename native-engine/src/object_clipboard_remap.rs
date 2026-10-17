//! Clipboard / duplicate remap kernel (F15 + F08, L02-C): the pure identity
//! rules behind duplicate-selection, copy / cut / paste and duplicate-layer.
//!
//! Mirrors `_snapshotForClone` / `_materializeClones` / `_remapCombineGroups` /
//! `pasteSelection` of `src/js/tools.js` and `dupOne` of `src/js/timeline.js`
//! at fbe4b8a3. The kernel works on DETACHED snapshots plus an injected
//! [`IdSource`]; it owns no document, history, clock or randomness. A call
//! returns a complete plan or one typed refusal: input problems are decided
//! before the first id is requested, and a provider failure mid-plan discards
//! every id requested so far (nothing is assigned, no partial map is exposed).
use std::collections::HashSet;
use std::fmt;

use crate::object_document::object_group_kernels::object_group_hierarchy::GroupEntry;

#[path = "object_clipboard_remap_groups.rs"]
mod groups;
#[path = "object_clipboard_remap_layer.rs"]
mod layer;

#[allow(unused_imports)]
pub(crate) use groups::GroupRemap;
#[allow(unused_imports)]
pub(crate) use layer::{plan_layer_duplicate, AdmittedKinds, LayerPlan, LayerSource};

/// Pixels a copy is nudged when it lands over its source (`duplicateSelection`, same-place paste).
pub(crate) const DUPLICATE_OFFSET_PX: f64 = 12.0;

/// What a fresh identity is for; the provider may mint a different shape per kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IdKind {
    Stroke,
    LinkedFill,
    BrushGroup,
    CombineGroup,
    Mesh,
}

/// Fresh-identity port (deterministic in tests). `replaces` names the source id
/// being replaced, so a mesh provider knows which record to copy. `None` means
/// the provider cannot supply an id (exhausted, or the source record is gone).
pub(crate) trait IdSource {
    fn fresh(&mut self, kind: IdKind, replaces: Option<&str>) -> Option<String>;
}

/// Reference kinds whose records are not native yet (F17 / I11 / I14); a layer
/// duplicate carrying one is refused unless the caller admits it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReferenceKind {
    Mesh,
    Rig,
    Component,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RemapError {
    EmptySelection,
    NonFiniteOffset,
    DuplicateSourceStrokeId(String),
    DabsWithoutBrushGroup(usize),
    EmptyGroupId,
    DuplicateGroupId(String),
    UnknownGroup(String),
    /// The ancestor walk of a tagged clone revisits a group (the baseline loops forever).
    GroupCycle(String),
    IdUnavailable(IdKind),
    EmptyFreshId(IdKind),
    /// The provider returned an id that is already in use (source, destination or issued).
    IdCollision(String),
    UnadmittedKind(ReferenceKind),
    UnknownReferenceKind(String),
}

impl fmt::Display for RemapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

/// `data.x` truthiness: an absent value and an empty string are the same thing.
pub(crate) fn live(text: &Option<String>) -> Option<&str> {
    text.as_deref().filter(|value| !value.is_empty())
}

/// Wraps the port with the uniqueness rule: a fresh id must be non-empty and
/// differ from every id already in the plan's world and from every earlier one.
pub(crate) struct IdIssuer<'a, S: IdSource + ?Sized> {
    source: &'a mut S,
    taken: HashSet<String>,
}

impl<'a, S: IdSource + ?Sized> IdIssuer<'a, S> {
    pub(crate) fn new(source: &'a mut S, taken: HashSet<String>) -> Self {
        IdIssuer { source, taken }
    }

    pub(crate) fn issue(
        &mut self,
        kind: IdKind,
        replaces: Option<&str>,
    ) -> Result<String, RemapError> {
        let id = self
            .source
            .fresh(kind, replaces)
            .ok_or(RemapError::IdUnavailable(kind))?;
        if id.is_empty() {
            return Err(RemapError::EmptyFreshId(kind));
        }
        if !self.taken.insert(id.clone()) {
            return Err(RemapError::IdCollision(id));
        }
        Ok(id)
    }
}

/// `(old, new)` lookup by the old key.
pub(crate) fn lookup<'m>(map: &'m [(String, String)], key: &str) -> Option<&'m str> {
    map.iter()
        .find(|pair| pair.0.as_str() == key)
        .map(|pair| pair.1.as_str())
}

/// A dab (brush-texture copy) that rides along with its anchor.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct DabSource {
    pub(crate) stroke_id: Option<String>,
    pub(crate) group_id: Option<String>,
}

/// The identity fields of one `_snapshotForClone` snapshot. Absent and empty
/// strings are the same thing (baseline truthiness).
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct ItemSource {
    pub(crate) stroke_id: Option<String>,
    pub(crate) group_id: Option<String>,
    pub(crate) brush_group_id: Option<String>,
    pub(crate) track_role_id: Option<String>,
    pub(crate) attached_to_role_id: Option<String>,
    pub(crate) attach_offset: Option<(f64, f64)>,
    pub(crate) linked_fill_id: Option<String>,
    /// `d.isVectorBrush && d.centerSegments`.
    pub(crate) vector_brush_with_centerline: bool,
    /// The snapshot carries `vbLinkedFill` (a live, non-removed backdrop fill).
    pub(crate) linked_fill_snapshot: bool,
    pub(crate) dabs: Vec<DabSource>,
}

pub(crate) struct CloneRequest<'a> {
    pub(crate) items: &'a [ItemSource],
    /// The source layer's group table; `None` skips the group remap (tags are kept).
    pub(crate) source_groups: Option<&'a [GroupEntry]>,
    /// Gids already present in the destination layer's table.
    pub(crate) destination_group_ids: &'a [String],
    /// Pixels added to both axes of every clone and dab (`paste_offset`).
    pub(crate) offset: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ClonedItem {
    pub(crate) source: usize,
    pub(crate) stroke_id: String,
    pub(crate) group_id: Option<String>,
    pub(crate) brush_group_id: Option<String>,
    /// Always `None`: a copy never claims to be the role's occupant.
    pub(crate) track_role_id: Option<String>,
    pub(crate) attached_to_role_id: Option<String>,
    pub(crate) attach_offset: Option<(f64, f64)>,
    pub(crate) linked_fill_id: Option<String>,
    /// A fresh backdrop fill must be created carrying `linked_fill_id`.
    pub(crate) companion: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ClonedDab {
    pub(crate) source: usize,
    pub(crate) dab: usize,
    pub(crate) stroke_id: Option<String>,
    pub(crate) brush_group_id: String,
    pub(crate) group_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ClonePlan {
    pub(crate) offset: f64,
    /// `(old stroke id, fresh stroke id)` for every item that had a source id.
    pub(crate) stroke_map: Vec<(String, String)>,
    pub(crate) items: Vec<ClonedItem>,
    pub(crate) dabs: Vec<ClonedDab>,
    /// Recreated combine groups in table-insertion order (children before parents).
    pub(crate) groups: Vec<GroupRemap>,
}

fn validate_request(req: &CloneRequest<'_>) -> Result<(), RemapError> {
    if req.items.is_empty() {
        return Err(RemapError::EmptySelection);
    }
    if !req.offset.is_finite() {
        return Err(RemapError::NonFiniteOffset);
    }
    for (index, item) in req.items.iter().enumerate() {
        if let Some(id) = live(&item.stroke_id) {
            if req.items[..index]
                .iter()
                .any(|prior| live(&prior.stroke_id) == Some(id))
            {
                return Err(RemapError::DuplicateSourceStrokeId(id.to_string()));
            }
        }
        if !item.dabs.is_empty() && live(&item.brush_group_id).is_none() {
            return Err(RemapError::DabsWithoutBrushGroup(index));
        }
    }
    if let Some(table) = req.source_groups {
        let mut tags: Vec<Option<&str>> = req.items.iter().map(|i| live(&i.group_id)).collect();
        for item in req.items {
            tags.extend(item.dabs.iter().map(|d| live(&d.group_id)));
        }
        groups::validate(table, &tags)?;
    }
    Ok(())
}

fn taken_ids(req: &CloneRequest<'_>) -> HashSet<String> {
    let mut taken = HashSet::new();
    for item in req.items {
        for id in [
            &item.stroke_id,
            &item.group_id,
            &item.brush_group_id,
            &item.linked_fill_id,
        ] {
            if let Some(value) = live(id) {
                taken.insert(value.to_string());
            }
        }
        for dab in &item.dabs {
            for id in [&dab.stroke_id, &dab.group_id] {
                if let Some(value) = live(id) {
                    taken.insert(value.to_string());
                }
            }
        }
    }
    if let Some(table) = req.source_groups {
        for entry in table {
            taken.insert(entry.gid.clone());
            taken.extend(entry.order.iter().cloned());
        }
    }
    taken.extend(req.destination_group_ids.iter().cloned());
    taken
}

fn brush_group_for<S: IdSource + ?Sized>(
    issuer: &mut IdIssuer<'_, S>,
    map: &mut Vec<(String, String)>,
    old: &str,
) -> Result<String, RemapError> {
    if let Some(existing) = lookup(map, old) {
        return Ok(existing.to_string());
    }
    let fresh = issuer.issue(IdKind::BrushGroup, Some(old))?;
    map.push((old.to_string(), fresh.clone()));
    Ok(fresh)
}

/// `_materializeClones` + `_remapCombineGroups`: fresh stroke ids, one fresh
/// brush group per old brush group (anchors and dabs share it), a fresh linked
/// fill pair, cleared track role, kept attachment, recreated combine groups.
pub(crate) fn plan_clone<S: IdSource + ?Sized>(
    req: &CloneRequest<'_>,
    ids: &mut S,
) -> Result<ClonePlan, RemapError> {
    validate_request(req)?;
    let mut issuer = IdIssuer::new(ids, taken_ids(req));
    let mut brush_map: Vec<(String, String)> = Vec::new();
    let mut sid_map: Vec<(String, String)> = Vec::new();
    let mut items: Vec<ClonedItem> = Vec::new();
    for (index, item) in req.items.iter().enumerate() {
        let brush_group_id = match live(&item.brush_group_id) {
            Some(old) => Some(brush_group_for(&mut issuer, &mut brush_map, old)?),
            None => None,
        };
        let old_stroke = live(&item.stroke_id);
        let stroke_id = issuer.issue(IdKind::Stroke, old_stroke)?;
        if let Some(old) = old_stroke {
            sid_map.push((old.to_string(), stroke_id.clone()));
        }
        let companion = item.vector_brush_with_centerline && item.linked_fill_snapshot;
        let linked_fill_id = if companion {
            Some(issuer.issue(IdKind::LinkedFill, live(&item.linked_fill_id))?)
        } else {
            live(&item.linked_fill_id).map(|value| value.to_string())
        };
        items.push(ClonedItem {
            source: index,
            stroke_id,
            group_id: live(&item.group_id).map(|value| value.to_string()),
            brush_group_id,
            track_role_id: None,
            attached_to_role_id: live(&item.attached_to_role_id).map(|value| value.to_string()),
            attach_offset: item.attach_offset,
            linked_fill_id,
            companion,
        });
    }
    let mut dabs: Vec<ClonedDab> = Vec::new();
    for (index, item) in req.items.iter().enumerate() {
        for (dab_index, dab) in item.dabs.iter().enumerate() {
            let brush_group_id = items[index]
                .brush_group_id
                .clone()
                .ok_or(RemapError::DabsWithoutBrushGroup(index))?;
            let stroke_id = match live(&dab.stroke_id) {
                Some(old) => Some(issuer.issue(IdKind::Stroke, Some(old))?),
                None => None,
            };
            dabs.push(ClonedDab {
                source: index,
                dab: dab_index,
                stroke_id,
                brush_group_id,
                group_id: live(&dab.group_id).map(|value| value.to_string()),
            });
        }
    }
    let mut tags: Vec<Option<String>> = items.iter().map(|c| c.group_id.clone()).collect();
    tags.extend(dabs.iter().map(|d| d.group_id.clone()));
    let recreated = match req.source_groups {
        Some(table) => groups::remap(table, &mut tags, &sid_map, &mut issuer)?,
        None => Vec::new(),
    };
    let item_count = items.len();
    for (index, cloned) in items.iter_mut().enumerate() {
        cloned.group_id = tags[index].clone();
    }
    for (index, dab) in dabs.iter_mut().enumerate() {
        dab.group_id = tags[item_count + index].clone();
    }
    Ok(ClonePlan {
        offset: req.offset,
        stroke_map: sid_map,
        items,
        dabs,
        groups: recreated,
    })
}

/// Where a copy was taken (clipboard) or is placed (destination).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ClipSpot {
    pub(crate) layer: usize,
    pub(crate) frame: usize,
}

/// `samePlace ? 12 : 0` of `pasteSelection`; duplicate-selection always uses 12.
pub(crate) fn paste_offset(origin: ClipSpot, destination: ClipSpot) -> f64 {
    if origin == destination {
        DUPLICATE_OFFSET_PX
    } else {
        0.0
    }
}

/// What the destination frame holds (`ld.frames[i]` and its two flags).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FrameState {
    Missing,
    Keyframe,
    Interpolated,
    Held,
}

/// How the transaction must store pasted content, decided from the frame state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PasteFrameDirective {
    /// Keyframe, interpolated or missing frame: store into the current frame.
    WriteCurrent,
    /// Animation 2D, held frame: `ensureKeyframe` first (the transaction leaf promotes).
    NeedsKeyframePromotion,
    /// Motion, held frame: store into the keyframe the hold inherits from.
    WriteIntoHeldKeyframe(usize),
    /// Motion, held frame with no keyframe at or before it: the baseline stores nothing.
    HeldWithoutKeyframe,
}

pub(crate) fn paste_frame_directive(
    frames: &[FrameState],
    current: usize,
    motion: bool,
) -> PasteFrameDirective {
    if frames.get(current) != Some(&FrameState::Held) {
        return PasteFrameDirective::WriteCurrent;
    }
    if !motion {
        return PasteFrameDirective::NeedsKeyframePromotion;
    }
    for index in (0..=current).rev() {
        if frames.get(index) == Some(&FrameState::Keyframe) {
            return PasteFrameDirective::WriteIntoHeldKeyframe(index);
        }
    }
    PasteFrameDirective::HeldWithoutKeyframe
}

#[cfg(test)]
#[path = "../tests/object_clipboard_remap_controls.rs"]
mod controls;
#[cfg(test)]
#[path = "../tests/object_clipboard_remap_layer_controls.rs"]
mod layer_controls;
#[cfg(test)]
#[path = "../tests/object_clipboard_remap_layer_replay.rs"]
mod layer_replay;
#[cfg(test)]
#[path = "../tests/object_clipboard_remap_mutations.rs"]
mod mutations;
#[cfg(test)]
#[path = "../tests/object_clipboard_remap_paste_controls.rs"]
mod paste_controls;
#[cfg(test)]
#[path = "../tests/object_clipboard_remap.rs"]
mod tests;
