//! Document half of the key retime (F10, `moveKeyframe` at fbe4b8a3): the sparse plan of
//! `LayerTiming::retime_key` decides which frames carry keys and samples and which old frame's content each
//! written cell carries; this file moves that CONTENT in an admitted `ObjectDocument` and revalidates the
//! complete result. It is a child of the L05-B document kernel only so the kernel's scope helpers stay private;
//! it adds no second owner of frame content.
//!
//! What follows the plan: the scene-root authored records of the retimed layer and, for a v2 document, that
//! layer's scene-root composition-order elements. A record keeps its stroke id, geometry, fill, natural
//! position and authored order; a plan that lands one old cell on several frames (the dense hole fill copies the
//! nearest sample) copies the record, exactly as an F6 copy does. Other layers, Reference records, other contexts,
//! `referencePolicy` and the property holder table never change. A scope that loses all its elements because the
//! content moved away is dropped with the frame it described; an element-less scope stays where it is. The
//! input is only borrowed: a refusal cannot leave a partial edit.
use crate::commands::object_fill_owner::object_frame_edit_document::{
    ascending, layer_index, scene_authored, scene_contexts, scope_at, timing_of, FrameEditError,
};
use crate::object_document::{
    FrameScopeKind, ObjectDocument, OrderContext, OrderScope, Presence, RetimeEdit, RetimePair,
    RetimePlan,
};
use crate::revision::ObjectSnapshot;
use std::collections::{BTreeMap, BTreeSet};

/// A changed document with the numbers the arc owner and the caller need.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DocumentRetime {
    pub(crate) document: ObjectDocument,
    /// The frame the dragged key landed on after the ripple clamp.
    pub(crate) applied_to: u32,
    /// Every consecutive key pair of the old timing with its new frames (the baseline `rekeyTweenPairData` calls).
    pub(crate) pairs: Vec<RetimePair>,
}

/// A published replacement or nothing to publish.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum RetimeOutcome {
    Changed(DocumentRetime),
    NoChange,
}

/// Where the content authored at each old frame lands: the written frames drop their own old content and take
/// the content of the frame their write names; every other frame keeps what it has.
struct Fan {
    written: BTreeSet<u32>,
    sources: BTreeMap<u32, Vec<u32>>,
}
impl Fan {
    fn new(writes: &[(u32, Option<u32>)]) -> Self {
        let mut written = BTreeSet::new();
        let mut sources: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
        for &(dest, source) in writes {
            written.insert(dest);
            if let Some(source) = source {
                sources.entry(source).or_default().push(dest);
            }
        }
        Self { written, sources }
    }
    /// The frames the content authored at `frame` lands on, ascending; empty when it is dropped.
    fn dests(&self, frame: u32) -> Vec<u32> {
        let mut out: Vec<u32> = self.sources.get(&frame).cloned().unwrap_or_default();
        if !self.written.contains(&frame) {
            out.push(frame);
        }
        out.sort_unstable();
        out
    }
}

fn fan_scopes(context: &mut OrderContext, layer: &str, fan: &Fan) -> Result<(), FrameEditError> {
    if context
        .scopes
        .iter()
        .any(|scope| scope.frame_scope.kind != FrameScopeKind::Authored)
    {
        return Err(FrameEditError::Unsupported);
    }
    let sorted = ascending(&context.scopes);
    let mut next: Vec<OrderScope> = Vec::new();
    for scope in std::mem::take(&mut context.scopes) {
        let frame = scope.frame_scope.frame;
        if scope.elements.is_empty() {
            scope_at(&mut next, frame);
            continue;
        }
        for element in scope.elements {
            if element.layer_uid == layer {
                for dest in fan.dests(frame) {
                    scope_at(&mut next, dest).elements.push(element.clone());
                }
            } else {
                scope_at(&mut next, frame).elements.push(element);
            }
        }
    }
    if sorted {
        next.sort_by_key(|scope| scope.frame_scope.frame);
    }
    context.scopes = next;
    Ok(())
}

/// `moveKeyframe` on one layer: the key at `from` goes to `to`, every later key moves by the same delta (clamped
/// at the last frame), and the in-between spans whose end keys moved are retimed. `from == to` or a ripple that
/// clamps to zero is `NoChange`.
pub(crate) fn retime_key(
    document: &ObjectDocument,
    layer_uid: &str,
    from: u32,
    to: u32,
) -> Result<RetimeOutcome, FrameEditError> {
    retime_key_with_active(document, layer_uid, from, to, None)
}

/// Legacy moveKeyframe omits the Arc layer argument, selecting the active UI
/// layer even when another layer's drawing key moves. Keep that captured UID
/// explicit; without it a nonempty Arc store cannot be safely retimed.
pub(crate) fn retime_key_with_active(
    document: &ObjectDocument,
    layer_uid: &str,
    from: u32,
    to: u32,
    active_layer_uid: Option<&str>,
) -> Result<RetimeOutcome, FrameEditError> {
    let index = layer_index(document, layer_uid)?;
    let timing = timing_of(document, index)?;
    let RetimePlan {
        timing,
        applied_to,
        writes,
        pairs,
    } = match timing.retime_key(from, to, document.total_frames)? {
        RetimeEdit::NoChange => return Ok(RetimeOutcome::NoChange),
        RetimeEdit::Changed(plan) => plan,
    };
    let arc_layer_uid = match active_layer_uid {
        Some(uid) => {
            layer_index(document, uid)?;
            Some(uid)
        }
        None if document.has_motion_arc_entries() => return Err(FrameEditError::Unsupported),
        None => None,
    };
    let fan = Fan::new(&writes);
    let mut next = document.clone();
    next.layers[index].timing = Presence::Present(timing);
    let records = std::mem::take(&mut next.objects);
    let mut moved = Vec::with_capacity(records.len());
    for record in records {
        if scene_authored(&record) && record.target.layer_uid == layer_uid {
            for dest in fan.dests(record.target.frame_scope.frame) {
                let mut copy = record.clone();
                copy.target.frame_scope.frame = dest;
                moved.push(copy);
            }
        } else {
            moved.push(record);
        }
    }
    next.objects = moved;
    for context in scene_contexts(&mut next) {
        fan_scopes(context, layer_uid, &fan)?;
    }
    // The baseline rekeys every consecutive pair in source order. Apply the
    // Arc replacement to this same detached content/order candidate, so a bound
    // failure cannot publish moved content with stale handles or extra history.
    let arc_pairs: Vec<_> = pairs
        .iter()
        .map(|pair| {
            (
                i64::from(pair.from_a),
                i64::from(pair.from_b),
                i64::from(pair.to_a),
                i64::from(pair.to_b),
            )
        })
        .collect();
    if let Some(uid) = arc_layer_uid {
        next.retime_motion_arcs(uid, &arc_pairs)
            .map_err(|_| FrameEditError::Invalid)?;
    }
    ObjectSnapshot::validate_drawing(&next).map_err(|_| FrameEditError::Invalid)?;
    Ok(RetimeOutcome::Changed(DocumentRetime {
        document: next,
        applied_to,
        pairs,
    }))
}
