//! Original moveFrames and content-only shiftLayerFrames through the existing native owner.
//! Whole-cell timing/content/order is preserved; clipboard normalization is never used. Move's
//! checkpoint eligibility differs from byte change. Shift's caller still owns atomic Motion/range/history.
use crate::commands::ObjectFillOwner;
use crate::object_document::{
    object_deserialize, plan_move_frames, plan_shift_frames, BulkCell, BulkMoveLayer, BulkMovePlan,
    FrameScopeKind, ItemRecords, LayerTimingError, ObjectDocument, ObjectTarget, OrderScope,
    Presence,
};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

const MOVE: &str = "object.frames.move";
const SHIFT: &str = "object.layer.frames.shift";
const CONTEXT: &str = "scene-root";
const MAX_PAYLOAD_BYTES: usize = 4096;
const MAX_SELECTION: usize = 4096;
const MAX_SAFE: i64 = 9_007_199_254_740_991;
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BulkFrameCell {
    pub(crate) layer_uid: String,
    pub(crate) frame: u32,
}
object_deserialize!(BulkFrameCell {
    layer_uid: String,
    frame: u32
});
#[derive(Debug)]
struct Payload {
    command: String,
    layer_uid: String,
    selection: Presence<Vec<BulkFrameCell>>,
    frame: Presence<u32>,
    delta: Presence<i64>,
}
object_deserialize!(Payload { command: String, layer_uid: String,
    #[serde(default)] selection: Presence<Vec<BulkFrameCell>>,
    #[serde(default)] frame: Presence<u32>, #[serde(default)] delta: Presence<i64> });
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BulkFramePair {
    pub(crate) layer_uid: String,
    pub(crate) from_a: u32,
    pub(crate) from_b: u32,
    pub(crate) to_a: i64,
    pub(crate) to_b: i64,
}
fn invalid() -> DispatchErrorCode {
    DispatchErrorCode::InvalidRequest
}
fn refused(error: LayerTimingError) -> DispatchErrorCode {
    match error {
        LayerTimingError::Invalid => invalid(),
        LayerTimingError::Unsupported => DispatchErrorCode::Unavailable,
    }
}
fn layer(document: &ObjectDocument, uid: &str) -> Result<usize, DispatchErrorCode> {
    document
        .layers
        .iter()
        .position(|l| l.layer_uid == uid)
        .ok_or(DispatchErrorCode::NotFound)
}
fn authored(target: &ObjectTarget) -> bool {
    target.context_id == CONTEXT && target.frame_scope.kind == FrameScopeKind::Authored
}

/// Destination cells take their complete original source, never partially updated content.
struct Fan {
    written: BTreeSet<BulkCell>,
    sources: BTreeMap<BulkCell, Vec<BulkCell>>,
    layers: Vec<String>,
    indices: BTreeMap<String, usize>,
}
impl Fan {
    fn new(document: &ObjectDocument, plan: &BulkMovePlan) -> Self {
        let mut written = BTreeSet::new();
        let mut sources: BTreeMap<BulkCell, Vec<BulkCell>> = BTreeMap::new();
        for (dest, source) in &plan.writes {
            written.insert(*dest);
            if let Some(source) = source {
                sources.entry(*source).or_default().push(*dest);
            }
        }
        Self {
            written,
            sources,
            layers: document
                .layers
                .iter()
                .map(|l| l.layer_uid.clone())
                .collect(),
            indices: document
                .layers
                .iter()
                .enumerate()
                .map(|(i, l)| (l.layer_uid.clone(), i))
                .collect(),
        }
    }
    fn dests(&self, source: BulkCell) -> Vec<BulkCell> {
        let mut dests = self.sources.get(&source).cloned().unwrap_or_default();
        if !self.written.contains(&source) {
            dests.push(source);
        }
        dests.sort_unstable();
        dests
    }
    fn target_dests(&self, target: &ObjectTarget) -> Option<Vec<BulkCell>> {
        if !authored(target) {
            return None;
        }
        self.indices.get(&target.layer_uid).map(|index| {
            self.dests(BulkCell {
                layer: *index,
                frame: target.frame_scope.frame,
            })
        })
    }
    fn retarget(&self, target: &mut ObjectTarget, dest: BulkCell) {
        target.layer_uid = self.layers[dest.layer].clone();
        target.frame_scope.frame = dest.frame;
    }
}
fn scope(scopes: &mut Vec<OrderScope>, frame: u32) -> &mut OrderScope {
    let index = match scopes.iter().position(|s| s.frame_scope.frame == frame) {
        Some(index) => index,
        None => {
            scopes.push(OrderScope {
                frame_scope: crate::object_document::FrameScope {
                    kind: FrameScopeKind::Authored,
                    frame,
                },
                elements: vec![],
            });
            scopes.len() - 1
        }
    };
    &mut scopes[index]
}
fn apply(
    document: &ObjectDocument,
    plan: &BulkMovePlan,
) -> Result<ObjectDocument, DispatchErrorCode> {
    let fan = Fan::new(document, plan);
    let mut next = document.clone();
    for (index, timing) in &plan.timings {
        next.layers[*index].timing = Presence::Present(timing.clone());
    }
    next.objects.clear();
    for record in &document.objects {
        match fan.target_dests(&record.target) {
            None => next.objects.push(record.clone()),
            Some(dests) => {
                for dest in dests {
                    let mut moved = record.clone();
                    fan.retarget(&mut moved.target, dest);
                    next.objects.push(moved);
                }
            }
        }
    }
    if let Some(items) = document.items.value() {
        let mut raster = Vec::new();
        let mut text = Vec::new();
        let mut removed = false;
        for record in items.raster_entries() {
            match fan.target_dests(&record.target) {
                None => raster.push(record.clone()),
                Some(dests) => {
                    removed |= dests.is_empty();
                    for dest in dests {
                        let mut moved = record.clone();
                        fan.retarget(&mut moved.target, dest);
                        raster.push(moved);
                    }
                }
            }
        }
        for record in items.text_entries() {
            match fan.target_dests(&record.target) {
                None => text.push(record.clone()),
                Some(dests) => {
                    removed |= dests.is_empty();
                    for dest in dests {
                        let mut moved = record.clone();
                        fan.retarget(&mut moved.target, dest);
                        text.push(moved);
                    }
                }
            }
        }
        if raster.as_slice() != items.raster_entries() || text.as_slice() != items.text_entries() {
            if removed {
                next.items = ItemRecords::normalized(raster, text);
            } else {
                // A pure retarget/copy must not erase an untouched present-empty sibling list.
                let mut kept = items.clone();
                if raster.as_slice() != items.raster_entries() {
                    kept.raster = Presence::Present(raster);
                }
                if text.as_slice() != items.text_entries() {
                    kept.text = Presence::Present(text);
                }
                next.items = Presence::Present(kept);
            }
        }
    }
    if let Presence::Present(order) = &mut next.composition_order {
        for context in order
            .contexts
            .iter_mut()
            .filter(|c| c.context_id == CONTEXT)
        {
            let ascending = context
                .scopes
                .windows(2)
                .all(|w| w[0].frame_scope.frame < w[1].frame_scope.frame);
            let mut scopes = Vec::new();
            for old in std::mem::take(&mut context.scopes) {
                if old.frame_scope.kind != FrameScopeKind::Authored {
                    return Err(DispatchErrorCode::Unavailable);
                }
                if old.elements.is_empty() {
                    scope(&mut scopes, old.frame_scope.frame);
                }
                for element in old.elements {
                    let index = fan
                        .indices
                        .get(&element.layer_uid)
                        .ok_or(DispatchErrorCode::Unavailable)?;
                    for dest in fan.dests(BulkCell {
                        layer: *index,
                        frame: old.frame_scope.frame,
                    }) {
                        let mut moved = element.clone();
                        moved.layer_uid = fan.layers[dest.layer].clone();
                        scope(&mut scopes, dest.frame).elements.push(moved);
                    }
                }
            }
            if ascending {
                scopes.sort_by_key(|s| s.frame_scope.frame);
            }
            context.scopes = scopes;
        }
    }
    if next != *document {
        // An outer component's frameKeys need their own atomic representation join; do not silently
        // move its host timing while preserving contradictory frame metadata.
        if let Some(components) = document.component_set() {
            if plan.timings.iter().any(|(i, _)| {
                components
                    .instance(CONTEXT, &document.layers[*i].layer_uid)
                    .is_some()
            }) {
                return Err(DispatchErrorCode::Unavailable);
            }
        }
        ObjectSnapshot::validate_drawing(&next).map_err(|_| DispatchErrorCode::Unavailable)?;
    }
    Ok(next)
}
fn prepare(
    document: &ObjectDocument,
    payload: &Payload,
) -> Result<(&'static str, BulkMovePlan, ObjectDocument), DispatchErrorCode> {
    let layers: Vec<_> = document
        .layers
        .iter()
        .map(|l| BulkMoveLayer {
            timing: l.timing.value(),
            locked: document.layer_is_locked(l.layer_uid()),
        })
        .collect();
    let plan = match payload.command.as_str() {
        MOVE => {
            if !payload.delta.missing() {
                return Err(invalid());
            }
            let selection = payload.selection.value().ok_or_else(invalid)?;
            let frame = payload.frame.value().copied().ok_or_else(invalid)?;
            if selection.len() > MAX_SELECTION {
                return Err(invalid());
            }
            let mut unique = BTreeSet::new();
            let mut cells = Vec::new();
            for cell in selection {
                if cell.layer_uid.is_empty()
                    || !unique.insert((cell.layer_uid.as_str(), cell.frame))
                {
                    return Err(invalid());
                }
                cells.push(BulkCell {
                    layer: layer(document, &cell.layer_uid)?,
                    frame: cell.frame,
                });
            }
            if cells.is_empty() {
                return Ok((
                    MOVE,
                    plan_move_frames(
                        &layers,
                        &[],
                        BulkCell { layer: 0, frame },
                        document.total_frames,
                    )
                    .map_err(refused)?,
                    document.clone(),
                ));
            }
            plan_move_frames(
                &layers,
                &cells,
                BulkCell {
                    layer: layer(document, &payload.layer_uid)?,
                    frame,
                },
                document.total_frames,
            )
            .map_err(refused)?
        }
        SHIFT => {
            if !payload.selection.missing() || !payload.frame.missing() {
                return Err(invalid());
            }
            let delta = payload.delta.value().copied().ok_or_else(invalid)?;
            if !(-MAX_SAFE..=MAX_SAFE).contains(&delta) {
                return Err(invalid());
            }
            let index = layer(document, &payload.layer_uid)?;
            let mut drawn = BTreeSet::new();
            for record in &document.objects {
                if authored(&record.target) && record.target.layer_uid == payload.layer_uid {
                    drawn.insert(record.target.frame_scope.frame);
                }
            }
            if let Some(items) = document.items.value() {
                for target in items.targets() {
                    if authored(target) && target.layer_uid == payload.layer_uid {
                        drawn.insert(target.frame_scope.frame);
                    }
                }
            }
            plan_shift_frames(&layers, index, delta, &drawn, document.total_frames)
                .map_err(refused)?
        }
        _ => return Err(invalid()),
    };
    let next = apply(document, &plan)?;
    Ok((
        if payload.command == MOVE { MOVE } else { SHIFT },
        plan,
        next,
    ))
}
/// Detached non-Clone candidate pinned to the actual origin; no queue, history or clipboard owner.
#[derive(Debug)]
pub(crate) struct PreparedBulkFrameEdit {
    origin: ObjectSnapshot,
    document: Option<ObjectDocument>,
    command: &'static str,
    document_changed: bool,
    checkpoint_required: bool,
    selection: Option<Vec<BulkFrameCell>>,
    pairs: Vec<BulkFramePair>,
    shifted: bool,
}
impl PreparedBulkFrameEdit {
    pub(crate) fn command(&self) -> &'static str {
        self.command
    }
    pub(crate) fn document_changed(&self) -> bool {
        self.document_changed
    }
    pub(crate) fn checkpoint_required(&self) -> bool {
        self.checkpoint_required
    }
    pub(crate) fn selection(&self) -> Option<&[BulkFrameCell]> {
        self.selection.as_deref()
    }
    pub(crate) fn pairs(&self) -> &[BulkFramePair] {
        &self.pairs
    }
    pub(crate) fn shifted(&self) -> bool {
        self.shifted
    }
}
impl ObjectFillOwner {
    pub(crate) fn prepare_bulk_frame_edit_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedBulkFrameEdit, DispatchErrorCode> {
        if bytes.len() > MAX_PAYLOAD_BYTES {
            return Err(invalid());
        }
        let payload: Payload = serde_json::from_slice(bytes).map_err(|_| invalid())?;
        if payload.layer_uid.is_empty() {
            return Err(invalid());
        }
        let origin = self.current_snapshot();
        let (command, plan, next) = prepare(origin.document(), &payload)?;
        let changed = next != *origin.document();
        let uids: Vec<_> = origin
            .document()
            .layers
            .iter()
            .map(|l| l.layer_uid.clone())
            .collect();
        let selection = plan.selection.map(|cells| {
            cells
                .into_iter()
                .map(|c| BulkFrameCell {
                    layer_uid: uids[c.layer].clone(),
                    frame: c.frame,
                })
                .collect()
        });
        let pairs = plan
            .pairs
            .into_iter()
            .map(|p| BulkFramePair {
                layer_uid: uids[p.layer].clone(),
                from_a: p.from_a,
                from_b: p.from_b,
                to_a: p.to_a,
                to_b: p.to_b,
            })
            .collect();
        Ok(PreparedBulkFrameEdit {
            origin,
            document: if changed || plan.checkpoint_required {
                Some(next)
            } else {
                None
            },
            command,
            document_changed: changed,
            checkpoint_required: plan.checkpoint_required,
            selection,
            pairs,
            shifted: plan.shifted,
        })
    }
    pub(crate) fn commit_bulk_frame_edit(
        &mut self,
        candidate: PreparedBulkFrameEdit,
    ) -> Result<u64, DispatchErrorCode> {
        let current = self.current_snapshot();
        if candidate.origin.instance_id() != current.instance_id() {
            return Err(DispatchErrorCode::WrongInstance);
        }
        if candidate.origin.document_id() != current.document_id() {
            return Err(DispatchErrorCode::WrongDocument);
        }
        if candidate.origin.snapshot_id() != current.snapshot_id()
            || candidate.origin.content_revision() != current.content_revision()
        {
            return Err(DispatchErrorCode::StaleRevision);
        }
        match candidate.document {
            None => Ok(self.content_revision()),
            Some(document) => self
                .revisions
                .publish(document)
                .map_err(|_| DispatchErrorCode::Unavailable),
        }
    }
}
#[cfg(test)]
#[path = "../tests/object_frame_bulk_move.rs"]
mod tests;

#[path = "object_layer_shift.rs"]
mod object_layer_shift;
pub(crate) use object_layer_shift::PreparedLayerShift;
