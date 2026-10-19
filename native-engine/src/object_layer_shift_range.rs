//! Layer-bar writes reuse the actual immutable D27 range resolver and existing property owner.
use crate::commands::object_fill_owner::object_frame_bulk_move::object_layer_shift::{
    integer, invalid, Gesture, Member, Part,
};
use crate::commands::ObjectFillOwner;
use crate::object_document::{
    LayerRange, ObjectDocument, Presence, RangeError, RangeEvaluator, TimeLinkRecord,
};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;
use std::collections::BTreeSet;

fn error(value: RangeError) -> DispatchErrorCode {
    match value {
        RangeError::LayerMissing => DispatchErrorCode::NotFound,
        RangeError::FrameOutOfRange => invalid(),
        _ => DispatchErrorCode::Unavailable,
    }
}
pub(super) fn points(
    document: &ObjectDocument,
    uid: &str,
    frame: u32,
    clamped: bool,
) -> Result<[f64; 2], DispatchErrorCode> {
    let range = RangeEvaluator::for_document(document, "scene-root", frame)
        .map_err(error)?
        .resolve(uid, clamped)
        .map_err(error)?;
    Ok([range.in_point, range.out_point])
}
fn index(document: &ObjectDocument, uid: &str) -> Result<usize, DispatchErrorCode> {
    document
        .layers
        .iter()
        .position(|l| l.layer_uid == uid)
        .ok_or(DispatchErrorCode::NotFound)
}
fn member(
    document: &ObjectDocument,
    uid: &str,
    part: Part,
    frame: u32,
) -> Result<Member, DispatchErrorCode> {
    index(document, uid)?;
    Ok(Member {
        uid: uid.into(),
        part,
        before: points(document, uid, frame, false)?,
    })
}
pub(super) fn members(
    document: &ObjectDocument,
    gesture: &Gesture,
) -> Result<(Vec<Member>, bool), DispatchErrorCode> {
    index(document, &gesture.layer_uid)?;
    let mut seen = BTreeSet::new();
    for selected in &gesture.bar_selection {
        if selected.layer_uid.is_empty() || !seen.insert(selected.layer_uid.as_str()) {
            return Err(invalid());
        }
        index(document, &selected.layer_uid)?;
    }
    let mut group = seen.contains(gesture.layer_uid.as_str()) && gesture.bar_selection.len() > 1;
    let mut members = if group {
        gesture
            .bar_selection
            .iter()
            .map(|s| member(document, &s.layer_uid, s.part, gesture.frame))
            .collect::<Result<Vec<_>, _>>()?
    } else {
        vec![member(
            document,
            &gesture.layer_uid,
            gesture.part,
            gesture.frame,
        )?]
    };
    let originals = members.clone();
    let mut included: BTreeSet<String> = members.iter().map(|m| m.uid.clone()).collect();
    for m in originals {
        let layer = &document.layers[index(document, &m.uid)?];
        if m.part != Part::Both || layer.is_folder_layer.value() != Some(&true) {
            continue;
        }
        for child in document.layers.iter().rev() {
            if child.is_folder_layer.value() != Some(&true)
                && child.layer_uid != m.uid
                && child.parent_layer_uid.value().and_then(Option::as_deref) == Some(m.uid.as_str())
                && included.insert(child.layer_uid.clone())
            {
                members.push(member(
                    document,
                    &child.layer_uid,
                    Part::Both,
                    gesture.frame,
                )?);
                group = true;
            }
        }
    }
    Ok((members, group))
}
fn link(document: &ObjectDocument, uid: &str) -> Option<TimeLinkRecord> {
    document
        .layers
        .iter()
        .find(|l| l.layer_uid == uid)?
        .range
        .value()?
        .time_link
        .value()?
        .clone()
}
fn source(document: &ObjectDocument, uid: &str, link: &TimeLinkRecord) -> bool {
    link.uid != uid && document.layers.iter().any(|l| l.layer_uid == link.uid)
}
pub(super) fn linked(
    document: &ObjectDocument,
    gesture: &Gesture,
    members: &[Member],
) -> Result<Vec<Member>, DispatchErrorCode> {
    let direct: BTreeSet<_> = members.iter().map(|m| m.uid.as_str()).collect();
    let mut result = Vec::new();
    for layer in &document.layers {
        if direct.contains(layer.layer_uid.as_str()) {
            continue;
        }
        let mut cursor = layer.layer_uid.clone();
        let mut found = false;
        for _ in 0..16 {
            let Some(next) = link(document, &cursor) else {
                break;
            };
            if direct.contains(next.uid.as_str()) {
                found = true;
                break;
            }
            if !document.layers.iter().any(|l| l.layer_uid == next.uid) {
                break;
            }
            cursor = next.uid;
        }
        if found {
            result.push(Member {
                uid: layer.layer_uid.clone(),
                part: Part::Both,
                before: points(document, &layer.layer_uid, gesture.frame, true)?,
            });
        }
    }
    Ok(result)
}
fn mode(link: &TimeLinkRecord) -> &str {
    link.mode.value().map_or("both", String::as_str)
}
fn covered(link: &TimeLinkRecord, edge: usize) -> bool {
    if edge == 0 {
        mode(link) != "out"
    } else {
        mode(link) != "in"
    }
}
fn source_edge(link: &TimeLinkRecord, edge: usize) -> usize {
    match link.src_anchor.value().map(String::as_str) {
        Some("in") => 0,
        Some("out") => 1,
        _ => edge,
    }
}
fn offset(
    origin: &ObjectSnapshot,
    document: ObjectDocument,
    uid: &str,
    edge: usize,
    value: f64,
    frame: u32,
) -> Result<ObjectDocument, DispatchErrorCode> {
    if !value.is_finite() {
        return Err(DispatchErrorCode::Unavailable);
    }
    let bytes = serde_json::to_vec(&serde_json::json!({
        "command":"object.property.edit", "target":{"contextId":"scene-root","layerUid":uid},
        "property":if edge == 0 { "timeLinkInOffset" } else { "timeLinkOutOffset" },
        "action":{"kind":"setValue","frame":frame,"values":[value]}
    }))
    .map_err(|_| invalid())?;
    Ok(
        ObjectFillOwner::apply_property_edit_json(origin.clone(), document, &bytes)?
            .into_document(),
    )
}
fn hard(
    document: &mut ObjectDocument,
    uid: &str,
    edge: usize,
    value: f64,
) -> Result<(), DispatchErrorCode> {
    let value = integer(value)?;
    let layer = index(document, uid)?;
    if document.layers[layer].range.missing() {
        document.layers[layer].range = Presence::Present(LayerRange::default());
    }
    if let Presence::Present(range) = &mut document.layers[layer].range {
        let slot = if edge == 0 {
            &mut range.in_point
        } else {
            &mut range.out_point
        };
        *slot = Presence::Present(Some(value));
    }
    Ok(())
}
fn edge(
    origin: &ObjectSnapshot,
    mut document: ObjectDocument,
    gesture: &Gesture,
    uid: &str,
    which: usize,
    value: f64,
) -> Result<ObjectDocument, DispatchErrorCode> {
    if let Some(link) = link(&document, uid) {
        if covered(&link, which) && source(&document, uid, &link) {
            let base =
                points(&document, &link.uid, gesture.frame, false)?[source_edge(&link, which)];
            return offset(origin, document, uid, which, value - base, gesture.frame);
        }
    }
    hard(&mut document, uid, which, value)?;
    Ok(document)
}
pub(super) fn move_ranges(
    origin: &ObjectSnapshot,
    mut document: ObjectDocument,
    gesture: &Gesture,
    members: &[Member],
    group: bool,
    sample: i64,
) -> Result<ObjectDocument, DispatchErrorCode> {
    let total = f64::from(document.total_frames);
    let margin = (total * 0.5).round().max(10.0);
    let dx = sample as f64;
    if !group {
        let m = &members[0];
        let a = m.before[0];
        let b = m.before[1];
        match gesture.part {
            Part::In => {
                document = edge(
                    origin,
                    document,
                    gesture,
                    &m.uid,
                    0,
                    (a + dx).min(b - 1.0).max(-margin),
                )?
            }
            Part::Out => {
                document = edge(
                    origin,
                    document,
                    gesture,
                    &m.uid,
                    1,
                    (b + dx).max(a + 1.0).min(total - 1.0 + margin),
                )?
            }
            Part::Both => {
                let width = b - a;
                let mut start = (a + dx).max(-margin);
                if start + width >= total + margin {
                    start = total - 1.0 + margin - width;
                }
                document = edge(origin, document, gesture, &m.uid, 0, start)?;
                document = edge(origin, document, gesture, &m.uid, 1, start + width)?;
            }
        }
        return Ok(document);
    }
    // Preserve baseline phase order: every in member, every out member, then both members.
    for part in [Part::In, Part::Out] {
        for m in members.iter().filter(|m| m.part == part) {
            let value = if part == Part::In {
                (m.before[0] + dx).min(m.before[1] - 1.0).max(-margin)
            } else {
                (m.before[1] + dx)
                    .max(m.before[0] + 1.0)
                    .min(total - 1.0 + margin)
            };
            document = edge(
                origin,
                document,
                gesture,
                &m.uid,
                usize::from(part == Part::Out),
                value,
            )?;
        }
    }
    let both: Vec<_> = members.iter().filter(|m| m.part == Part::Both).collect();
    let lo = both
        .iter()
        .map(|m| -margin - m.before[0])
        .fold(f64::NEG_INFINITY, f64::max);
    let hi = both
        .iter()
        .map(|m| total - 1.0 + margin - m.before[1])
        .fold(f64::INFINITY, f64::min);
    if lo <= hi {
        let shared = dx.min(hi).max(lo);
        for m in both {
            document = edge(origin, document, gesture, &m.uid, 0, m.before[0] + shared)?;
            document = edge(origin, document, gesture, &m.uid, 1, m.before[1] + shared)?;
        }
    }
    Ok(document)
}
pub(super) fn reconcile(
    origin: &ObjectSnapshot,
    mut document: ObjectDocument,
    gesture: &Gesture,
    members: &[Member],
) -> Result<ObjectDocument, DispatchErrorCode> {
    for m in members {
        let Some(link) = link(&document, &m.uid) else {
            continue;
        };
        if !source(&document, &m.uid, &link) {
            continue;
        }
        let layer = index(&document, &m.uid)?;
        let range = document.layers[layer]
            .range
            .value()
            .ok_or(DispatchErrorCode::Internal)?;
        let resolved = points(&document, &m.uid, gesture.frame, false)?;
        let want = [
            range
                .in_point
                .value()
                .copied()
                .flatten()
                .map_or(resolved[0], |v| v as f64),
            range
                .out_point
                .value()
                .copied()
                .flatten()
                .map_or(resolved[1], |v| v as f64),
        ];
        let source = points(&document, &link.uid, gesture.frame, false)?;
        // Both bases and destinations are captured before either property setter runs.
        for which in 0..2 {
            if covered(&link, which) {
                document = offset(
                    origin,
                    document,
                    &m.uid,
                    which,
                    want[which] - source[source_edge(&link, which)],
                    gesture.frame,
                )?;
            }
        }
        if let Presence::Present(range) = &mut document.layers[layer].range {
            if covered(&link, 0) {
                range.in_point = Presence::Missing;
            }
            if covered(&link, 1) {
                range.out_point = Presence::Missing;
            }
        }
    }
    Ok(document)
}
