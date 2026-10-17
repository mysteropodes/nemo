//! Frame resolution, the held-frame promotion port and the order-plan helpers
//! shared by the selection-edit and clipboard plans (F08/F15, L02-H).
//!
//! A selection edit acts on the drawing the playhead shows. The baseline
//! (`deleteSelStrokes`, `pasteSelection` in `src/js/timeline.js` /
//! `tools.js` at fbe4b8a3) decides where that drawing is stored from the
//! frame's exposure and the editor mode: a key or an in-between stores in
//! place, a hold in Animation 2D is promoted to a key first, a hold in Motion
//! is edited in the key it inherits from. L02-C owns that decision
//! ([`paste_frame_directive`]); this file only feeds it from the native layer
//! timing. Promotion itself belongs to the L05 frame kernel, which lives
//! above the commands layer, so it is a consumer port here.
use std::collections::HashSet;

use crate::object_document::object_group_kernels::object_clipboard_remap::{
    paste_frame_directive, FrameState, PasteFrameDirective,
};
use crate::object_document::{
    AuthoredOrder, FrameScope, FrameScopeKind, ObjectDocument, ObjectLayer, ObjectRecord,
    OrderElement, OrderScope, Presence,
};

/// Layer timing is a scene-root fact (`LayerTiming::validate`).
const TIMING_CONTEXT: &str = "scene-root";

/// `state.appMode === 'motion'` of the baseline, made an explicit request member.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EditMode {
    Animation2d,
    Motion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PromotionRefusal {
    /// No provider is wired; nothing is invented.
    Unavailable,
    /// The provider refused the cell or returned an unusable result.
    Invalid,
}

/// Consumer port for the L05 `promote_cells` kernel: make `frame` a key of
/// `layer_uid` holding a copy of what it shows (same stroke ids), returning the
/// complete replacement document. Called at most once per edit.
pub(crate) trait HeldFramePromotion {
    fn promote(
        &self,
        document: &ObjectDocument,
        layer_uid: &str,
        frame: u32,
    ) -> Result<ObjectDocument, PromotionRefusal>;
}

/// The owner's stand-in until the L05 kernel is reachable from the commands
/// layer: every promotion is unavailable and the edit publishes nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UnavailablePromotion;

impl HeldFramePromotion for UnavailablePromotion {
    fn promote(
        &self,
        _document: &ObjectDocument,
        _layer_uid: &str,
        _frame: u32,
    ) -> Result<ObjectDocument, PromotionRefusal> {
        Err(PromotionRefusal::Unavailable)
    }
}

/// Where the edit reads and writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Resolved {
    /// The authored frame that holds the drawing.
    At(u32),
    /// Animation 2D hold: promote this frame to a key, then edit it.
    Promote(u32),
    /// Motion hold with no key at or before it: the baseline stores nothing.
    NoKeyframe,
}

fn frame_states(layer: &ObjectLayer, frame: u32) -> Option<Vec<FrameState>> {
    let timing = layer.timing.value()?;
    Some(
        (0..=frame)
            .map(|cell| match timing.exposure(cell).0 {
                "key" => FrameState::Keyframe,
                "interpolated" => FrameState::Interpolated,
                _ => FrameState::Held,
            })
            .collect(),
    )
}

/// The L02-C directive for `frame` of `layer`. Without a timing record (or
/// outside the scene-root context, the only one timing describes) the frame is
/// taken as authored: there is no exposure model to consult.
pub(crate) fn resolve_frame(
    layer: &ObjectLayer,
    context_id: &str,
    frame: u32,
    mode: EditMode,
) -> Resolved {
    let states = match frame_states(layer, frame) {
        Some(states) if context_id == TIMING_CONTEXT => states,
        _ => return Resolved::At(frame),
    };
    match paste_frame_directive(&states, frame as usize, mode == EditMode::Motion) {
        PasteFrameDirective::WriteCurrent => Resolved::At(frame),
        PasteFrameDirective::NeedsKeyframePromotion => Resolved::Promote(frame),
        PasteFrameDirective::WriteIntoHeldKeyframe(key) => {
            Resolved::At(u32::try_from(key).unwrap_or(frame))
        }
        PasteFrameDirective::HeldWithoutKeyframe => Resolved::NoKeyframe,
    }
}

/// `getEffectiveStrokes`: the authored frame whose drawing the playhead shows
/// (a hold shows its key; before every key nothing is shown).
pub(crate) fn effective_frame(layer: &ObjectLayer, context_id: &str, frame: u32) -> Option<u32> {
    let timing = match layer.timing.value() {
        Some(timing) if context_id == TIMING_CONTEXT => timing,
        _ => return Some(frame),
    };
    match timing.exposure(frame) {
        ("held", Some(key), _) => Some(key),
        ("unexposed", _, _) => None,
        _ => Some(frame),
    }
}

/// Authored record of `layer_uid` in `context_id` (any frame).
pub(crate) fn in_layer(record: &ObjectRecord, context_id: &str, layer_uid: &str) -> bool {
    record.target.context_id == context_id
        && record.target.layer_uid == layer_uid
        && record.target.frame_scope.kind == FrameScopeKind::Authored
}

/// Authored record of one cell: layer, context and frame.
pub(crate) fn in_cell(
    record: &ObjectRecord,
    context_id: &str,
    layer_uid: &str,
    frame: u32,
) -> bool {
    in_layer(record, context_id, layer_uid) && record.target.frame_scope.frame == frame
}

pub(crate) fn default_order() -> AuthoredOrder {
    AuthoredOrder {
        static_value: Presence::Missing,
        track: Presence::Missing,
        expression: Presence::Missing,
    }
}

fn ascending(scopes: &[OrderScope]) -> bool {
    scopes
        .windows(2)
        .all(|pair| pair[0].frame_scope.frame < pair[1].frame_scope.frame)
}

/// Re-packs the natural slots of one layer's elements in a scope to `0..k`,
/// keeping their relative order (the order codec demands gapless slots).
fn repack(elements: &mut [OrderElement], layer_uid: &str) {
    let mut slots: Vec<u32> = elements
        .iter()
        .filter(|element| element.layer_uid == layer_uid)
        .map(|element| element.natural_position)
        .collect();
    slots.sort_unstable();
    for element in elements
        .iter_mut()
        .filter(|element| element.layer_uid == layer_uid)
    {
        let rank = slots.partition_point(|slot| *slot < element.natural_position);
        element.natural_position = u32::try_from(rank).unwrap_or(u32::MAX);
    }
}

/// The named strokes leave the order scope of one cell; the layer's remaining
/// elements there are re-packed. A document without an order plan is untouched.
pub(crate) fn remove_elements(
    document: &mut ObjectDocument,
    context_id: &str,
    layer_uid: &str,
    frame: u32,
    strokes: &HashSet<&str>,
) {
    if let Presence::Present(plan) = &mut document.composition_order {
        for context in plan
            .contexts
            .iter_mut()
            .filter(|context| context.context_id == context_id)
        {
            for scope in context.scopes.iter_mut().filter(|scope| {
                scope.frame_scope.kind == FrameScopeKind::Authored
                    && scope.frame_scope.frame == frame
            }) {
                scope.elements.retain(|element| {
                    !(element.layer_uid == layer_uid
                        && strokes.contains(element.stroke_id.as_str()))
                });
                repack(&mut scope.elements, layer_uid);
            }
        }
    }
}

/// New strokes join the top of the layer's stack in one cell (`slot` = the
/// number of elements the layer already has there), with the default authored
/// order a freshly drawn stroke has. The scope is created when absent and an
/// ascending scope list stays ascending.
pub(crate) fn append_elements(
    document: &mut ObjectDocument,
    context_id: &str,
    layer_uid: &str,
    frame: u32,
    strokes: &[String],
) {
    if let Presence::Present(plan) = &mut document.composition_order {
        for context in plan
            .contexts
            .iter_mut()
            .filter(|context| context.context_id == context_id)
        {
            let sorted = ascending(&context.scopes);
            let at = match context.scopes.iter().position(|scope| {
                scope.frame_scope.kind == FrameScopeKind::Authored
                    && scope.frame_scope.frame == frame
            }) {
                Some(at) => at,
                None => {
                    context.scopes.push(OrderScope {
                        frame_scope: FrameScope {
                            kind: FrameScopeKind::Authored,
                            frame,
                        },
                        elements: Vec::new(),
                    });
                    context.scopes.len() - 1
                }
            };
            let scope = &mut context.scopes[at];
            let base = scope
                .elements
                .iter()
                .filter(|element| element.layer_uid == layer_uid)
                .count();
            for (offset, stroke) in strokes.iter().enumerate() {
                scope.elements.push(OrderElement {
                    layer_uid: layer_uid.to_string(),
                    stroke_id: stroke.clone(),
                    natural_position: u32::try_from(base + offset).unwrap_or(u32::MAX),
                    order: default_order(),
                });
            }
            if sorted {
                context.scopes.sort_by_key(|scope| scope.frame_scope.frame);
            }
        }
    }
}

/// Holders keyed on a stroke leave with the last record of that stroke
/// (admission refuses a holder whose element vanished). Layer-level holders and
/// the holder table's presence are untouched.
pub(crate) fn drop_stroke_holders(
    document: &mut ObjectDocument,
    context_id: &str,
    layer_uid: &str,
    gone: &[String],
) {
    if let Presence::Present(holders) = &mut document.property_holders {
        holders.retain(|holder| {
            !(holder.target.context_id == context_id
                && holder.target.layer_uid == layer_uid
                && holder
                    .target
                    .stroke_id
                    .value()
                    .is_some_and(|stroke| gone.contains(stroke)))
        });
    }
}
