//! The document half of the retained tail: what the Frames field takes off the end of a drawing when it
//! shrinks, and how the same cells are laid back when it grows (baseline setTotalFrames never truncates).
//! Only what `cut` removes moves here: per-layer timing entries, scene-root authored records, raster and text
//! items, and scene-root order scopes. Layers that no longer exist when the timeline grows again drop their
//! cells; everything else returns at the frame it had, counted from the end the timeline had when it was cut.
use crate::object_document::{
    FrameScopeKind, ItemRecords, ObjectDocument, ObjectTarget, Presence, RetainedTail, TailLayer,
};

const CONTEXT: &str = "scene-root";

fn root_cell(target: &ObjectTarget, at: u32) -> bool {
    target.context_id == CONTEXT
        && target.frame_scope.kind == FrameScopeKind::Authored
        && target.frame_scope.frame >= at
}

/// Everything at or after frame `at`, re-based so `at` is frame 0.
pub(super) fn extract(document: &ObjectDocument, at: u32) -> RetainedTail {
    let mut tail = RetainedTail::default();
    for layer in &document.layers {
        if let Some(timing) = layer.timing.value() {
            let cells = timing.tail_from(at);
            if !cells.is_empty() {
                tail.layers.push(TailLayer {
                    layer_uid: layer.layer_uid.clone(),
                    timing: cells,
                });
            }
        }
    }
    tail.objects = document
        .objects
        .iter()
        .filter(|record| root_cell(&record.target, at))
        .cloned()
        .map(|mut record| {
            record.target.frame_scope.frame -= at;
            record
        })
        .collect();
    if let Some(items) = document.item_records() {
        tail.raster = items
            .raster_entries()
            .iter()
            .filter(|entry| root_cell(&entry.target, at))
            .cloned()
            .map(|mut entry| {
                entry.target.frame_scope.frame -= at;
                entry
            })
            .collect();
        tail.text = items
            .text_entries()
            .iter()
            .filter(|entry| root_cell(&entry.target, at))
            .cloned()
            .map(|mut entry| {
                entry.target.frame_scope.frame -= at;
                entry
            })
            .collect();
    }
    if let Some(plan) = document.composition_order.value() {
        tail.scopes = plan
            .contexts
            .iter()
            .filter(|context| context.context_id == CONTEXT)
            .flat_map(|context| context.scopes.iter())
            .filter(|scope| {
                scope.frame_scope.kind == FrameScopeKind::Authored && scope.frame_scope.frame >= at
            })
            .cloned()
            .map(|mut scope| {
                scope.frame_scope.frame -= at;
                scope
            })
            .collect();
    }
    tail
}

/// `grown` is the document already lengthened to its new total; `tail` holds the cells for the new frames,
/// relative to the old end `at`. A typed `Err(())` means the cells no longer fit; the caller grows without them.
pub(super) fn restore(
    grown: &ObjectDocument,
    tail: &RetainedTail,
    at: u32,
) -> Result<ObjectDocument, ()> {
    let mut next = grown.clone();
    let known: std::collections::HashSet<&str> = grown
        .layers
        .iter()
        .map(|layer| layer.layer_uid.as_str())
        .collect();
    for cells in &tail.layers {
        let Some(layer) = next
            .layers
            .iter_mut()
            .find(|l| l.layer_uid == cells.layer_uid)
        else {
            continue;
        };
        if let Some(timing) = layer.timing.value() {
            let laid = timing
                .with_tail(&cells.timing, at, grown.total_frames)
                .map_err(|_| ())?;
            layer.timing = Presence::Present(laid);
        }
    }
    let lay = |target: &mut ObjectTarget| {
        target.frame_scope.frame += at;
        known.contains(target.layer_uid.as_str())
    };
    // Records come back right after the last scene-root authored record, where the cut took them from.
    let slot = next
        .objects
        .iter()
        .rposition(|record| root_cell(&record.target, 0))
        .map_or(0, |last| last + 1);
    let laid: Vec<_> = tail
        .objects
        .iter()
        .cloned()
        .filter_map(|mut record| lay(&mut record.target).then_some(record))
        .collect();
    next.objects.splice(slot..slot, laid);
    if !tail.raster.is_empty() || !tail.text.is_empty() {
        let (mut raster, mut text) = match grown.item_records() {
            Some(items) => (
                items.raster_entries().to_vec(),
                items.text_entries().to_vec(),
            ),
            None => (Vec::new(), Vec::new()),
        };
        raster.extend(
            tail.raster
                .iter()
                .cloned()
                .filter_map(|mut e| lay(&mut e.target).then_some(e)),
        );
        text.extend(
            tail.text
                .iter()
                .cloned()
                .filter_map(|mut e| lay(&mut e.target).then_some(e)),
        );
        next.items = ItemRecords::normalized(raster, text);
    }
    if let Presence::Present(plan) = &mut next.composition_order {
        if let Some(context) = plan.contexts.iter_mut().find(|c| c.context_id == CONTEXT) {
            let sorted = context
                .scopes
                .windows(2)
                .all(|p| p[0].frame_scope.frame < p[1].frame_scope.frame);
            for scope in &tail.scopes {
                let frame = scope.frame_scope.frame + at;
                let elements: Vec<_> = scope
                    .elements
                    .iter()
                    .filter(|element| known.contains(element.layer_uid.as_str()))
                    .cloned()
                    .collect();
                match context
                    .scopes
                    .iter_mut()
                    .find(|s| s.frame_scope.frame == frame)
                {
                    Some(own) => own.elements.extend(elements),
                    None => {
                        let mut laid = scope.clone();
                        laid.frame_scope.frame = frame;
                        laid.elements = elements;
                        context.scopes.push(laid);
                    }
                }
            }
            if sorted {
                context.scopes.sort_by_key(|scope| scope.frame_scope.frame);
            }
        }
    }
    Ok(next)
}

#[cfg(all(test, feature = "project_media_codec"))]
#[path = "../tests/object_project_length_tail.rs"]
mod tail_tests;
