//! L06-L (G01): the raster-item transactions of `object_raster_asset_commands.rs` -- create, move, resize, rotate,
//! opacity and delete -- as pure functions from the sealed origin to the edited document.
//!
//! Every edit goes through the strict L06-C codec: the new item is the existing item re-decoded with exactly the
//! edited members replaced, so absent / default / explicit values and number spelling classes stay as authored and
//! no setter can bypass a validation. A change is judged by the EFFECTIVE value (an absent rotation reads 0, an absent
//! opacity reads 1): equal means an exact no-op (`changed == false`, the origin document unchanged).
//! A rotation of exactly 0 is written the way the baseline `serR` writes it: the member is omitted.
use crate::commands::object_fill_owner::item_commands::{
    allocate_id, asset_code, check_layer, check_target, inventory_of, lists, order_insert,
    order_remove, other_family, store, ItemEdit, ItemPayload,
};
use crate::object_document::object_raster_asset_kernels::object_raster_asset_item::RasterItem;
use crate::object_document::{
    raster_asset_issue, ObjectDocument, ObjectTarget, Presence, RasterEntry,
};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;
use serde_json::{Number, Value};

/// Prefix of the native ids the owner allocates for raster items.
const ID_PREFIX: &str = "native-raster";

pub(crate) fn apply(
    origin: &ObjectSnapshot,
    payload: &ItemPayload,
) -> Result<ItemEdit, DispatchErrorCode> {
    match payload.command.as_str() {
        "object.raster.create" => {
            payload.expects(&["target", "item"])?;
            create(origin, payload)
        }
        "object.raster.move" => {
            payload.expects(&["stableTarget", "x", "y"])?;
            let edits = [("x", number(&payload.x)?), ("y", number(&payload.y)?)];
            modify(origin, payload, "raster.move", &edits)
        }
        "object.raster.resize" => {
            payload.expects(&["stableTarget", "width", "height"])?;
            let edits = [
                ("width", number(&payload.width)?),
                ("height", number(&payload.height)?),
            ];
            modify(origin, payload, "raster.resize", &edits)
        }
        "object.raster.rotate" => {
            payload.expects(&["stableTarget", "rotation"])?;
            modify(
                origin,
                payload,
                "raster.rotate",
                &[("rotation", number(&payload.rotation)?)],
            )
        }
        "object.raster.opacity" => {
            payload.expects(&["stableTarget", "opacity"])?;
            modify(
                origin,
                payload,
                "raster.opacity",
                &[("opacity", number(&payload.opacity)?)],
            )
        }
        "object.raster.delete" => {
            payload.expects(&["stableTarget"])?;
            delete(origin, payload)
        }
        _ => Err(DispatchErrorCode::InvalidRequest),
    }
}

fn number(slot: &Presence<Number>) -> Result<Number, DispatchErrorCode> {
    slot.value()
        .cloned()
        .ok_or(DispatchErrorCode::InvalidRequest)
}

fn stable(payload: &ItemPayload) -> Result<&ObjectTarget, DispatchErrorCode> {
    payload
        .stable_target
        .value()
        .ok_or(DispatchErrorCode::InvalidRequest)
}

fn index_of(document: &ObjectDocument, target: &ObjectTarget) -> Result<usize, DispatchErrorCode> {
    document
        .item_records()
        .and_then(|items| {
            items
                .raster_entries()
                .iter()
                .position(|entry| &entry.target == target)
        })
        .ok_or_else(|| other_family(document, target))
}

fn create(origin: &ObjectSnapshot, payload: &ItemPayload) -> Result<ItemEdit, DispatchErrorCode> {
    let (Some(target), Some(item)) = (payload.target.value(), payload.item.value()) else {
        return Err(DispatchErrorCode::InvalidRequest);
    };
    if !target.stroke_id().is_empty() {
        return Err(DispatchErrorCode::InvalidRequest);
    }
    if let Some(issue) = raster_asset_issue(item, inventory_of(origin)) {
        return Err(asset_code(issue));
    }
    let mut document = origin.document().clone();
    let stroke_id = allocate_id(&document, origin.content_revision(), ID_PREFIX)?;
    let mut target = target.clone();
    target.stroke_id = stroke_id.clone();
    check_target(&document, &target)?;
    check_layer(&document, &target)?;
    order_insert(&mut document, &target)?;
    let (mut raster, text) = lists(&document);
    raster.push(RasterEntry {
        target,
        item: item.clone(),
    });
    store(&mut document, raster, text);
    Ok(ItemEdit {
        document,
        kind: "raster.create",
        stroke_id,
        changed: true,
    })
}

/// The effective values a request would leave the item with, compared with the ones it has.
fn unchanged(item: &RasterItem, edits: &[(&'static str, Number)]) -> bool {
    edits.iter().all(|(member, wanted)| {
        let wanted = wanted.as_f64();
        let current = match *member {
            "x" => Some(item.center().0),
            "y" => Some(item.center().1),
            "width" => Some(item.size().0),
            "height" => Some(item.size().1),
            "rotation" => Some(item.rotation()),
            "opacity" => Some(item.opacity()),
            _ => None,
        };
        current.is_some() && current == wanted
    })
}

/// The item re-decoded with the edited members replaced (a rotation of 0 removes the member).
pub(crate) fn redecode(
    item: &RasterItem,
    edits: &[(&'static str, Number)],
) -> Result<RasterItem, DispatchErrorCode> {
    let text = item.encode().map_err(|_| DispatchErrorCode::Internal)?;
    let Value::Object(mut members) =
        serde_json::from_str::<Value>(&text).map_err(|_| DispatchErrorCode::Internal)?
    else {
        return Err(DispatchErrorCode::Internal);
    };
    for (member, value) in edits {
        if *member == "rotation" && value.as_f64() == Some(0.0) {
            members.remove("rotation");
        } else {
            members.insert((*member).to_string(), Value::Number(value.clone()));
        }
    }
    RasterItem::decode(&Value::Object(members).to_string())
        .map_err(|_| DispatchErrorCode::InvalidRequest)
}

fn modify(
    origin: &ObjectSnapshot,
    payload: &ItemPayload,
    kind: &'static str,
    edits: &[(&'static str, Number)],
) -> Result<ItemEdit, DispatchErrorCode> {
    let target = stable(payload)?;
    let document = origin.document();
    check_target(document, target)?;
    let index = index_of(document, target)?;
    check_layer(document, target)?;
    let stroke_id = target.stroke_id().to_string();
    let current = &document
        .item_records()
        .ok_or(DispatchErrorCode::Internal)?
        .raster_entries()[index]
        .item;
    // Validate the request even when it changes nothing: a refused value is a refusal, not a silent no-op.
    let edited = redecode(current, edits)?;
    if unchanged(current, edits) {
        return Ok(ItemEdit {
            document: document.clone(),
            kind,
            stroke_id,
            changed: false,
        });
    }
    let mut next = document.clone();
    let (mut raster, text) = lists(&next);
    raster[index].item = edited;
    store(&mut next, raster, text);
    Ok(ItemEdit {
        document: next,
        kind,
        stroke_id,
        changed: true,
    })
}

fn delete(origin: &ObjectSnapshot, payload: &ItemPayload) -> Result<ItemEdit, DispatchErrorCode> {
    let target = stable(payload)?;
    let document = origin.document();
    check_target(document, target)?;
    let index = index_of(document, target)?;
    check_layer(document, target)?;
    let mut next = document.clone();
    order_remove(&mut next, target)?;
    let (mut raster, text) = lists(&next);
    raster.remove(index);
    store(&mut next, raster, text);
    Ok(ItemEdit {
        document: next,
        kind: "raster.delete",
        stroke_id: target.stroke_id().to_string(),
        changed: true,
    })
}
