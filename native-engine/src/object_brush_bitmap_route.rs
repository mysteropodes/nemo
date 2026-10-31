//! #2361 (DRAW-C): the host route of the bitmap-brush resource operations. One session owner: the charged
//! `BrushBitmapStore` already held by the preview's `ObjectBrushRuntime` (so it is retired when the session closes or is
//! replaced). Publish decodes an embedded PNG asset of the pinned snapshot into that store; inspect and scene are
//! pinned reads of it. No document revision moves and no history entry is added; no pixels cross this seam.
use crate::object_render_scene::brush_scene::BrushRuntime;
use crate::evaluation::object_scene_style_kernels::{
    bitmap_brush_resources_store::BrushBitmapStore,
    bitmap_brush_resources_bitmap::BrushStoreError,
    object_scene_producer_brush_slot::{anchor_key, slot_brush_id},
};
use crate::object_codec::{ImageAssetError, ImageAssetLimits};
use crate::object_document::FrameScopeKind;
use crate::object_frame_packet::{
    prepare_object_exposure_frame, ObjectFrameError, ObjectFrameSelector,
};
use crate::read_queries::{brush_read, BrushAssetError, BrushAssetRequest};
use crate::request_receipts::{
    DispatchError, DispatchErrorCode, OpacityRequest, ResponseEnvelope, APPLICATION_API_VERSION,
};
use crate::revision::ObjectSnapshot;
use serde_json::{json, Map, Value};

const PUBLISH: &str = "command.document.object.brush.bitmap.publish";
const QUERY: &str = "query.document.object.brush.bitmap";
const MAX_SAFE: u64 = 9_007_199_254_740_991;
const MAX_SIDE: u64 = 4096;
const MAX_ENTRIES: u64 = 4096;
const MAX_REQUEST_BYTES: usize = 4096;
/// A refusal is its error code plus a closed reason the client may act on.
type Refusal = (DispatchErrorCode, &'static str);

fn identifier(value: &str) -> bool {
    let bytes = value.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 128
        && bytes[0].is_ascii_alphanumeric()
        && bytes
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || b"._:/-".contains(b))
}
fn opaque(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 255 && !value.contains('\0')
}
fn closed(payload: &Value, keys: &[&str]) -> Option<()> {
    let object = payload.as_object()?;
    (object.len() == keys.len() && keys.iter().all(|key| object.contains_key(*key))).then_some(())
}
fn text<'a>(payload: &'a Value, key: &str, valid: fn(&str) -> bool) -> Result<&'a str, Refusal> {
    payload
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| valid(value))
        .ok_or((DispatchErrorCode::InvalidRequest, "malformed-request"))
}
fn number(payload: &Value, key: &str, max: u64) -> Result<u64, Refusal> {
    payload
        .get(key)
        .and_then(Value::as_u64)
        .filter(|value| *value <= max)
        .ok_or((DispatchErrorCode::InvalidRequest, "malformed-request"))
}
fn side(payload: &Value, key: &str) -> Result<u32, Refusal> {
    let value = number(payload, key, MAX_SIDE)?;
    u32::try_from(value)
        .ok()
        .filter(|value| *value >= 1)
        .ok_or((DispatchErrorCode::InvalidRequest, "malformed-request"))
}

/// The closed reason of a refused publication (a missing asset is `not_found`, a quota refusal `unavailable`).
fn publish_refusal(error: &BrushAssetError) -> Refusal {
    use DispatchErrorCode::*;
    match error {
        BrushAssetError::BareDrawing | BrushAssetError::NoInventory | BrushAssetError::MissingAsset => {
            (NotFound, "asset-missing")
        }
        BrushAssetError::WrongKind(_) => (InvalidRequest, "asset-wrong-kind"),
        BrushAssetError::NotEmbedded => (InvalidRequest, "asset-linked"),
        BrushAssetError::Decode(ImageAssetError::Unsupported) => (InvalidRequest, "asset-unsupported"),
        BrushAssetError::Decode(ImageAssetError::Malformed) => (InvalidRequest, "asset-malformed"),
        BrushAssetError::Decode(ImageAssetError::Allocation) => (Unavailable, "over-quota"),
        BrushAssetError::Decode(_) => (InvalidRequest, "asset-oversize"),
        BrushAssetError::DimensionMismatch { .. } => (InvalidRequest, "asset-wrong-size"),
        BrushAssetError::Budget(_) | BrushAssetError::Store(BrushStoreError::Budget(_)) => {
            (Unavailable, "over-quota")
        }
        BrushAssetError::Store(BrushStoreError::PendingConflict | BrushStoreError::ForeignPending) => {
            (BusyConflict, "publication-in-progress")
        }
        BrushAssetError::Store(BrushStoreError::Unavailable) => (Unavailable, "store-unavailable"),
        BrushAssetError::Store(_) => (InvalidRequest, "brush-rejected"),
        BrushAssetError::Stale => (StaleRevision, "asset-changed"),
    }
}

fn bitmap_row(brush_id: &str, store: &BrushBitmapStore) -> Value {
    match store.peek(brush_id) {
        Some(bitmap) => {
            let (width, height) = bitmap.dimensions();
            json!({"version": bitmap.version().to_string(), "width": width, "height": height,
                "bytes": u64::from(width) * u64::from(height) * 4})
        }
        None => Value::Null,
    }
}

/// The scene entry of one brush-bearing record: its bitmap if the store holds it, or the typed reason it is not drawn.
fn scene_entry(
    packet: &crate::object_frame_packet::ObjectFramePacket,
    store: &BrushBitmapStore,
    binding: &brush_read::Binding<'_>,
) -> Result<Value, Refusal> {
    let (record, _, _, window) = binding;
    let target = record.record().target();
    let anchor = anchor_key(
        packet.instance_id(),
        packet.context_id(),
        &target.layer_uid,
        &target.stroke_id,
    );
    // A trimmed anchor draws its slot bake, an untrimmed one its resident bitmap.
    let brush_id = if window.is_some() {
        slot_brush_id(&anchor)
    } else {
        format!("{anchor}|resident")
    };
    if !opaque(&brush_id) || !opaque(&target.layer_uid) || !opaque(&target.stroke_id) {
        return Err((DispatchErrorCode::Unavailable, "brush-id-unrepresentable"));
    }
    let mut entry = Map::new();
    entry.insert("sourceObjectIndex".into(), record.source_object_index().into());
    entry.insert("layerUid".into(), target.layer_uid.clone().into());
    entry.insert("strokeId".into(), target.stroke_id.clone().into());
    entry.insert("brushId".into(), brush_id.clone().into());
    match bitmap_row(&brush_id, store) {
        Value::Null => entry.insert("unavailable".into(), json!({"code": "ink-missing"})),
        ink => entry.insert("ink".into(), ink),
    };
    Ok(Value::Object(entry))
}

fn frame_refusal(error: ObjectFrameError) -> Refusal {
    use DispatchErrorCode::*;
    match error {
        ObjectFrameError::WrongInstance => (WrongInstance, "wrong-instance"),
        ObjectFrameError::WrongDocument => (WrongDocument, "wrong-document"),
        ObjectFrameError::WrongRevision | ObjectFrameError::WrongSnapshot => {
            (StaleRevision, "stale-snapshot")
        }
        ObjectFrameError::InvalidContext => (NotFound, "context-unknown"),
        ObjectFrameError::FrameOutOfRange => (InvalidRequest, "frame-out-of-range"),
        _ => (Unavailable, "scene-unavailable"),
    }
}

/// Whether the host routes this operation here rather than to the object command chain.
pub(crate) fn owns(operation: &str) -> bool {
    operation == PUBLISH || operation == QUERY
}

/// One publish, inspect or scene request against the current pinned snapshot. Always an envelope: a refusal is a
/// typed `ok: false`, and the store is untouched by it. `None` is a released session.
pub(crate) fn dispatch(
    runtime: Option<&mut BrushRuntime>,
    snapshot: &ObjectSnapshot,
    request: &OpacityRequest,
) -> ResponseEnvelope {
    let (ok, result, error) = match outcome(runtime, snapshot, request) {
        Ok(result) => (true, Some(result), None),
        Err((code, reason)) => (
            false,
            None,
            Some(DispatchError {
                code,
                message: "Native brush resource request refused.".into(),
                details: Some(json!({"reason": reason})),
            }),
        ),
    };
    ResponseEnvelope {
        api_version: APPLICATION_API_VERSION,
        request_id: request.request_id.clone(),
        instance_id: snapshot.instance_id().into(),
        document_id: snapshot.document_id().into(),
        content_revision: snapshot.content_revision(),
        ok,
        result,
        error,
    }
}

fn outcome(
    runtime: Option<&mut BrushRuntime>,
    snapshot: &ObjectSnapshot,
    request: &OpacityRequest,
) -> Result<Value, Refusal> {
    use DispatchErrorCode::*;
    let operation = request.operation();
    if request.api_version != APPLICATION_API_VERSION
        || !owns(operation)
        || ![
            &request.request_id,
            &request.instance_id,
            &request.document_id,
        ]
        .iter()
        .all(|value| identifier(value))
        || !serde_json::to_vec(request).is_ok_and(|bytes| bytes.len() <= MAX_REQUEST_BYTES)
    {
        return Err((InvalidRequest, "malformed-request"));
    }
    if request.instance_id != snapshot.instance_id() {
        return Err((WrongInstance, "wrong-instance"));
    }
    if request.document_id != snapshot.document_id() {
        return Err((WrongDocument, "wrong-document"));
    }
    if request.cancelled_before_dispatch {
        return Err((CancelledBeforeDispatch, "cancelled"));
    }
    let payload = &request.payload;
    if operation == PUBLISH {
        closed(
            payload,
            &["command", "documentSnapshotId", "assetId", "brushId", "width", "height"],
        )
        .filter(|()| payload["command"] == "object.brush.bitmap.publish")
        .ok_or((InvalidRequest, "malformed-request"))?;
        let snapshot_id = text(payload, "documentSnapshotId", identifier)?;
        let (asset_id, brush_id) = (text(payload, "assetId", opaque)?, text(payload, "brushId", opaque)?);
        let (width, height) = (side(payload, "width")?, side(payload, "height")?);
        // A publication moves no revision: it is bound to the one the caller saw.
        if request.expected_revision.is_none_or(|value| value > MAX_SAFE) {
            return Err((InvalidRequest, "malformed-request"));
        }
        if request.expected_revision != Some(snapshot.content_revision())
            || snapshot_id != snapshot.snapshot_id()
        {
            return Err((StaleRevision, "stale-snapshot"));
        }
        return publish_bitmap(runtime, snapshot, asset_id, brush_id, (width, height));
    }
    if request.expected_revision.is_some() {
        return Err((InvalidRequest, "malformed-request"));
    }
    let inspect = payload["action"] == "inspect";
    if inspect {
        closed(payload, &["action", "atRevision", "documentSnapshotId", "brushId"])
    } else if payload["action"] == "scene" {
        closed(
            payload,
            &["action", "atRevision", "documentSnapshotId", "contextId", "frame", "offset", "limit"],
        )
    } else {
        None
    }
    .ok_or((InvalidRequest, "malformed-request"))?;
    let at = number(payload, "atRevision", MAX_SAFE)?;
    let snapshot_id = text(payload, "documentSnapshotId", identifier)?;
    let (brush_id, scene) = if inspect {
        (Some(text(payload, "brushId", opaque)?), None)
    } else {
        let frame = u32::try_from(number(payload, "frame", u64::from(u32::MAX))?)
            .map_err(|_| (InvalidRequest, "malformed-request"))?;
        let offset = number(payload, "offset", MAX_ENTRIES)?;
        if number(payload, "limit", 1)? != 1 {
            return Err((InvalidRequest, "malformed-request"));
        }
        (None, Some((text(payload, "contextId", identifier)?, frame, offset)))
    };
    if at != snapshot.content_revision() || snapshot_id != snapshot.snapshot_id() {
        return Err((StaleRevision, "stale-snapshot"));
    }
    let runtime = runtime.ok_or((Unavailable, "session-released"))?;
    let store = runtime.producer.publisher_mut().store();
    if let Some(brush_id) = brush_id {
        let row = match bitmap_row(brush_id, store) {
            Value::Null => json!({"brushId": brush_id, "resident": false}),
            Value::Object(mut row) => {
                row.insert("brushId".into(), brush_id.into());
                row.insert("resident".into(), true.into());
                Value::Object(row)
            }
            _ => unreachable!("a bitmap row is an object or null"),
        };
        return Ok(json!({"atRevision": at, "documentSnapshotId": snapshot_id, "brush": row}));
    }
    let (context_id, frame, offset) = scene.expect("scene fields were parsed");
    let packet = prepare_object_exposure_frame(
        snapshot,
        &ObjectFrameSelector {
            instance_id: snapshot.instance_id(),
            document_id: snapshot.document_id(),
            content_revision: snapshot.content_revision(),
            document_snapshot_id: snapshot.snapshot_id(),
            context_id,
            scope_kind: FrameScopeKind::Authored,
            frame,
        },
    )
    .map_err(frame_refusal)?;
    let bindings = brush_read::bindings(&packet).map_err(|error| (Unavailable, error.reason))?;
    let total = bindings.len() as u64;
    if offset > total {
        return Err((InvalidRequest, "offset-beyond-total"));
    }
    let entries = match bindings.get(offset as usize) {
        Some(binding) => vec![scene_entry(&packet, store, binding)?],
        None => Vec::new(),
    };
    let next = offset + entries.len() as u64;
    Ok(json!({"atRevision": at, "documentSnapshotId": snapshot_id, "contextId": context_id,
        "frame": frame, "offset": offset, "nextOffset": (next < total).then_some(next),
        "totalEntries": total, "entries": entries}))
}

fn publish_bitmap(
    runtime: Option<&mut BrushRuntime>,
    snapshot: &ObjectSnapshot,
    asset_id: &str,
    brush_id: &str,
    (width, height): (u32, u32),
) -> Result<Value, Refusal> {
    let runtime = runtime.ok_or((DispatchErrorCode::Unavailable, "session-released"))?;
    // The same operating envelope as the native project and attachment admission, narrowed to a brush bake.
    let limits = ImageAssetLimits::new(16 << 20, 256 << 20, 64 << 20, 4096, 4096)
        .map_err(|_| (DispatchErrorCode::Unavailable, "limits-unavailable"))?;
    let publication = runtime
        .producer
        .publisher_mut()
        .publish(
            snapshot,
            BrushAssetRequest {
                asset_id,
                brush_id,
                limits,
                expected_size: Some((width, height)),
            },
            // No compositor is lent to a publication: a full quota is a typed refusal, never an eviction.
            None,
        )
        .map_err(|error| publish_refusal(&error))?;
    Ok(json!({"applied": !publication.reused, "historyEntriesAdded": 0, "brushId": brush_id,
        "version": publication.bitmap.version().to_string(), "width": width, "height": height,
        "bytes": u64::from(width) * u64::from(height) * 4}))
}

#[cfg(test)]
#[path = "../tests/object_brush_bitmap_route.rs"]
mod tests;
