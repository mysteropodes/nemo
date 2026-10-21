//! L06-L (G01 / G38a): sealed raster-item and text-block transactions through the one object revision owner.
//! Strict original-byte payload, ONE detached candidate document sealed to the immutable origin snapshot, full
//! storage admission before anything can publish, and publication only through the existing `ObjectRevisionOwner`
//! (history entry, request replay and dispatch live in their existing owners). Nothing here is an authority of its own.
//!
//! Operation `command.document.object.item.edit`; payload member `command` selects the transaction, and exactly
//! the members listed for it are accepted (any other member is a refusal, never ignored):
//! - `object.raster.create`  `target` (strokeId `""`, the owner allocates `native-raster:{revision+1}:{n}`), `item`
//! - `object.raster.move`    `stableTarget`, `x`, `y`        - `object.raster.resize`  `stableTarget`, `width`, `height`
//! - `object.raster.rotate`  `stableTarget`, `rotation`      - `object.raster.opacity` `stableTarget`, `opacity`
//! - `object.raster.delete`  `stableTarget`
//! - `object.text.create`    `target` (strokeId `""` -> `native-text:{revision+1}:{n}`), `block`
//! - `object.text.edit`      `stableTarget`, `patch` (the changed members; `anchorTopLeft` is not editable)
//! - `object.text.delete`    `stableTarget`
//! Raster edits live in `object_raster_asset_edit.rs`, text edits in `object_text_layout_commands.rs`.
use crate::commands::object_fill_owner::text_commands;
use crate::commands::object_fill_owner::text_commands::patch::TextPatch;
use crate::commands::{
    insert_order_element, remove_order_element, ObjectFillOwner, OrderEditError,
};
use crate::object_document::object_raster_asset_kernels::object_raster_asset_item::RasterItem;
use crate::object_document::object_text_layout_kernels::object_text_layout_record::TextBlockRecord;
use crate::object_document::{
    item_asset_issue, object_deserialize, AssetInventory, FrameScopeKind, ItemAssetIssue,
    ItemRecords, ObjectDocument, ObjectTarget, Presence, RasterEntry, TextEntry,
};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;
use serde::Deserialize;
use serde_json::{json, Number, Value};
use std::collections::HashSet;

#[path = "object_raster_asset_edit.rs"]
pub(crate) mod raster;

/// Whole-payload bound of this operation: the 4096-byte ingress bound of every object operation is NOT raised.
const MAX_PAYLOAD_BYTES: usize = 4096;
const MAX_SAFE_REVISION: u64 = 9_007_199_254_740_991;

pub(crate) struct ItemPayload {
    pub(crate) command: String,
    pub(crate) target: Presence<ObjectTarget>,
    pub(crate) stable_target: Presence<ObjectTarget>,
    pub(crate) item: Presence<RasterItem>,
    pub(crate) block: Presence<TextBlockRecord>,
    pub(crate) patch: Presence<TextPatch>,
    pub(crate) x: Presence<Number>,
    pub(crate) y: Presence<Number>,
    pub(crate) width: Presence<Number>,
    pub(crate) height: Presence<Number>,
    pub(crate) rotation: Presence<Number>,
    pub(crate) opacity: Presence<Number>,
    pub(crate) layer_uid: Presence<String>,
    pub(crate) animator:
        Presence<crate::object_document::object_text_layout_kernels::animator::AnimatorRecord>,
    pub(crate) animator_id: Presence<String>,
    pub(crate) edit:
        Presence<crate::commands::object_fill_owner::text_commands::animator::edit::AnimatorEdit>,
}
object_deserialize!(ItemPayload {
    command: String,
    #[serde(default)] target: Presence<ObjectTarget>,
    #[serde(default)] stable_target: Presence<ObjectTarget>,
    #[serde(default)] item: Presence<RasterItem>,
    #[serde(default)] block: Presence<TextBlockRecord>,
    #[serde(default)] patch: Presence<TextPatch>,
    #[serde(default)] x: Presence<Number>,
    #[serde(default)] y: Presence<Number>,
    #[serde(default)] width: Presence<Number>,
    #[serde(default)] height: Presence<Number>,
    #[serde(default)] rotation: Presence<Number>,
    #[serde(default)] opacity: Presence<Number>,
    #[serde(default)] layer_uid: Presence<String>,
    #[serde(default)] animator: Presence<crate::object_document::object_text_layout_kernels::animator::AnimatorRecord>,
    #[serde(default)] animator_id: Presence<String>,
    #[serde(default)] edit: Presence<crate::commands::object_fill_owner::text_commands::animator::edit::AnimatorEdit>
});

impl ItemPayload {
    /// The optional members that are present, in declaration order.
    fn members(&self) -> Vec<&'static str> {
        let mut out = Vec::new();
        let flags = [
            ("target", !self.target.missing()),
            ("stableTarget", !self.stable_target.missing()),
            ("item", !self.item.missing()),
            ("block", !self.block.missing()),
            ("patch", !self.patch.missing()),
            ("x", !self.x.missing()),
            ("y", !self.y.missing()),
            ("width", !self.width.missing()),
            ("height", !self.height.missing()),
            ("rotation", !self.rotation.missing()),
            ("opacity", !self.opacity.missing()),
            ("layerUid", !self.layer_uid.missing()),
            ("animator", !self.animator.missing()),
            ("animatorId", !self.animator_id.missing()),
            ("edit", !self.edit.missing()),
        ];
        for (name, present) in flags {
            if present {
                out.push(name);
            }
        }
        out
    }
    /// Exactly these members, no more and no fewer.
    pub(crate) fn expects(&self, names: &[&str]) -> Result<(), DispatchErrorCode> {
        if self.members() == names {
            Ok(())
        } else {
            Err(DispatchErrorCode::InvalidRequest)
        }
    }
}

/// The edited document and what happened to it. `changed == false` is an exact no-op (the document is the origin's).
pub(crate) struct ItemEdit {
    pub(crate) document: ObjectDocument,
    pub(crate) kind: &'static str,
    pub(crate) stroke_id: String,
    pub(crate) changed: bool,
}

/// Compact result retained by the request receipt and echoed in the response.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ItemEditResult {
    kind: &'static str,
    stroke_id: String,
    applied: bool,
}
impl ItemEditResult {
    pub(crate) fn result(&self) -> Value {
        json!({
            "applied": self.applied,
            "historyEntriesAdded": u64::from(self.applied),
            "operation": self.kind,
            "strokeId": self.stroke_id,
        })
    }
}

/// Detached, non-Clone candidate: only this child can construct its sealed origin.
#[derive(Debug)]
pub(crate) struct PreparedItemEdit {
    origin: ObjectSnapshot,
    document: ObjectDocument,
    result: ItemEditResult,
}
impl PreparedItemEdit {
    /// The result the commit will report (`applied` is false for an exact no-op).
    pub(crate) fn result(&self) -> ItemEditResult {
        self.result.clone()
    }
}

/// The project asset inventory of the sealed origin; `None` for a bare drawing.
pub(crate) fn inventory_of(origin: &ObjectSnapshot) -> Option<&AssetInventory> {
    origin
        .project()
        .and_then(|project| project.asset_inventory())
}

/// An unresolved asset reference keeps its typed disposition: absent -> NotFound, wrong kind -> InvalidRequest, no
/// inventory at all -> Unavailable.
pub(crate) fn asset_code(issue: ItemAssetIssue) -> DispatchErrorCode {
    match issue {
        ItemAssetIssue::NoInventory => DispatchErrorCode::Unavailable,
        ItemAssetIssue::MissingAsset | ItemAssetIssue::MissingFont => DispatchErrorCode::NotFound,
        ItemAssetIssue::NotImage | ItemAssetIssue::NotFont => DispatchErrorCode::InvalidRequest,
    }
}

/// Items need an authored target; reference records remain readable storage.
pub(crate) fn check_target(
    document: &ObjectDocument,
    target: &ObjectTarget,
) -> Result<(), DispatchErrorCode> {
    if !document.valid_target(target) || target.frame() >= document.total_frames() {
        return Err(DispatchErrorCode::InvalidRequest);
    }
    if target.frame_scope.kind == FrameScopeKind::Reference {
        return Err(DispatchErrorCode::Unavailable);
    }
    Ok(())
}

/// The effective Drawing lock gates item edits; a widget layer holds no items.
pub(crate) fn check_layer(
    document: &ObjectDocument,
    target: &ObjectTarget,
) -> Result<(), DispatchErrorCode> {
    let layer = document
        .layers
        .iter()
        .find(|layer| layer.layer_uid() == target.layer_uid())
        .ok_or(DispatchErrorCode::NotFound)?;
    if document.layer_is_locked(layer.layer_uid()) {
        return Err(DispatchErrorCode::Unavailable);
    }
    if layer.widget.value().is_some() {
        return Err(DispatchErrorCode::InvalidRequest);
    }
    Ok(())
}

/// Why the composition order plan of a v2 document could not gain or lose the element of an item.
fn order_code(error: OrderEditError) -> DispatchErrorCode {
    match error {
        OrderEditError::ContextAbsent => DispatchErrorCode::NotFound,
        OrderEditError::ReferenceScope | OrderEditError::PositionOverflow => {
            DispatchErrorCode::Unavailable
        }
        OrderEditError::ElementAbsent => DispatchErrorCode::InvalidRequest,
    }
}

/// A created item gains exactly one scoped order element (top of its layer's natural stack) when the document
/// carries a composition order plan; without a plan (v1) there is nothing to maintain.
pub(crate) fn order_insert(
    document: &mut ObjectDocument,
    target: &ObjectTarget,
) -> Result<(), DispatchErrorCode> {
    if let Presence::Present(plan) = &mut document.composition_order {
        insert_order_element(plan, target).map_err(order_code)?;
    }
    Ok(())
}

/// A deleted item loses exactly its own order element; later elements of the layer close the gap.
pub(crate) fn order_remove(
    document: &mut ObjectDocument,
    target: &ObjectTarget,
) -> Result<(), DispatchErrorCode> {
    if let Presence::Present(plan) = &mut document.composition_order {
        remove_order_element(plan, target).map_err(order_code)?;
    }
    Ok(())
}

/// Revision identity is monotonic across undo/redo; used ids (any record kind) are skipped, so neither preparation
/// nor a refused commit consumes an id.
pub(crate) fn allocate_id(
    document: &ObjectDocument,
    revision: u64,
    prefix: &str,
) -> Result<String, DispatchErrorCode> {
    let next = revision
        .checked_add(1)
        .filter(|next| *next <= MAX_SAFE_REVISION)
        .ok_or(DispatchErrorCode::Unavailable)?;
    let mut taken: HashSet<&str> = document
        .objects
        .iter()
        .map(|record| record.target.stroke_id())
        .collect();
    if let Some(items) = document.item_records() {
        taken.extend(items.targets().map(ObjectTarget::stroke_id));
    }
    (0..=taken.len())
        .map(|n| format!("{prefix}:{next}:{n}"))
        .find(|id| !taken.contains(id.as_str()))
        .ok_or(DispatchErrorCode::Unavailable)
}

/// The two stored lists, cloned out for editing (both empty when the document holds no items).
pub(crate) fn lists(document: &ObjectDocument) -> (Vec<RasterEntry>, Vec<TextEntry>) {
    match document.item_records() {
        Some(items) => (
            items.raster_entries().to_vec(),
            items.text_entries().to_vec(),
        ),
        None => (Vec::new(), Vec::new()),
    }
}

/// Store the edited lists in canonical form (an emptied container is absent again).
pub(crate) fn store(document: &mut ObjectDocument, raster: Vec<RasterEntry>, text: Vec<TextEntry>) {
    document.items = ItemRecords::normalized(raster, text);
}

/// A target that belongs to another record kind is unavailable to this command; an unknown one is not found.
pub(crate) fn other_family(document: &ObjectDocument, target: &ObjectTarget) -> DispatchErrorCode {
    let taken = document
        .objects
        .iter()
        .any(|record| &record.target == target)
        || document
            .item_records()
            .is_some_and(|items| items.targets().any(|candidate| candidate == target));
    if taken {
        DispatchErrorCode::Unavailable
    } else {
        DispatchErrorCode::NotFound
    }
}

impl ObjectFillOwner {
    /// Strict payload preparation directly from the original bytes (a `Value` would erase duplicate members).
    /// The returned candidate is sealed to this owner's current immutable snapshot and already passed the complete
    /// storage admission and asset resolution of the edited document; nothing is published.
    pub(crate) fn prepare_item_edit_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedItemEdit, DispatchErrorCode> {
        if bytes.len() > MAX_PAYLOAD_BYTES {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let payload: ItemPayload =
            serde_json::from_slice(bytes).map_err(|_| DispatchErrorCode::InvalidRequest)?;
        let origin = self.current_snapshot();
        let edit = if payload.command.starts_with("object.raster.") {
            raster::apply(&origin, &payload)?
        } else if payload.command.starts_with("object.text.") {
            text_commands::apply(&origin, &payload)?
        } else {
            return Err(DispatchErrorCode::InvalidRequest);
        };
        ObjectSnapshot::validate_drawing(&edit.document)
            .map_err(|_| DispatchErrorCode::InvalidRequest)?;
        if item_asset_issue(&edit.document, inventory_of(&origin)).is_some() {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        Ok(PreparedItemEdit {
            origin,
            document: edit.document,
            result: ItemEditResult {
                kind: edit.kind,
                stroke_id: edit.stroke_id,
                applied: edit.changed,
            },
        })
    }

    /// Publish a sealed candidate if and only if the origin pins (instance, document, snapshot, content revision)
    /// still match the current head. An exact no-op returns the unchanged head revision.
    pub(crate) fn commit_item_edit(
        &mut self,
        candidate: PreparedItemEdit,
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
        if !candidate.result.applied {
            return Ok(self.content_revision());
        }
        self.revisions
            .publish(candidate.document)
            .map_err(|_| DispatchErrorCode::Unavailable)
    }
}
