//! Shared fixtures of the L06-L transaction tests: a project owner over a real asset inventory, the strict item
//! payloads and the state fingerprint used to prove "no partial publication".
use crate::commands::PreparedItemEdit;
use crate::history::object_history::NativeObjectHistory;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::object_document::{
    AssetId, AssetInventory, AssetKind, AssetRecord, AssetSource, ProjectEnvelope, ProjectMetadata,
};
use crate::request_receipts::{DispatchErrorCode, OpacityRequest};
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};
use std::sync::Arc;

pub(crate) const OP: &str = "command.document.object.item.edit";
const PNG_HEX: &str = "89504e470d0a1a0a0000000d494844520000000200000002080600000072b60d240000001a49444154789c63105432feef129ad6c050de319361d5ee33ff01378a07ad4ff3864d0000000049454e44ae426082";

/// A real 2x2 PNG (the same bytes the project media tests embed).
pub(crate) fn png() -> Vec<u8> {
    PNG_HEX
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}

fn record(id: &str, kind: AssetKind, source: AssetSource) -> Arc<AssetRecord> {
    Arc::new(
        AssetRecord::new(
            AssetId::new(id.into()).unwrap(),
            format!("{id}.bin"),
            kind,
            source,
        )
        .unwrap(),
    )
}

fn embedded(media: &str, bytes: Vec<u8>) -> AssetSource {
    AssetSource::admitted_embedded(media.into(), Arc::new(bytes), 1 << 20).unwrap()
}

/// `img-1`, `img-2` (embedded PNG), `lnk-1` (linked image), `font-1` (a linked font-kind asset).
pub(crate) fn inventory() -> AssetInventory {
    AssetInventory::new(
        vec![
            record("img-1", AssetKind::Image, embedded("image/png", png())),
            record("img-2", AssetKind::Image, embedded("image/png", png())),
            record(
                "lnk-1",
                AssetKind::Image,
                AssetSource::linked("media/a.png".into(), 4096).unwrap(),
            ),
            // A linked font: an embedded font would need the A2 font admission, which this harness does not build.
            record(
                "font-1",
                AssetKind::Font,
                AssetSource::linked("fonts/a.ttf".into(), 4096).unwrap(),
            ),
        ],
        Vec::new(),
        16,
        0,
    )
    .unwrap()
}

/// Only the two PNG images: the inventory a saved project file can carry without a font admission.
pub(crate) fn images_only() -> AssetInventory {
    AssetInventory::new(
        vec![
            record("img-1", AssetKind::Image, embedded("image/png", png())),
            record("img-2", AssetKind::Image, embedded("image/png", png())),
        ],
        Vec::new(),
        16,
        0,
    )
    .unwrap()
}

/// Three layers: `L0` and `L2` editable, `L1` locked (the persisted F06 flag).
pub(crate) fn drawing(extra: Value) -> Value {
    let mut doc = json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":8,
        "layers":[{"layerUid":"L0"},{"layerUid":"L1","locked":true},{"layerUid":"L2"}],"objects":[]});
    for (member, value) in extra.as_object().unwrap() {
        doc[member] = value.clone();
    }
    doc
}

/// The three layers of [`drawing`] plus `W`, a widget (rig control) layer: it may hold no items.
pub(crate) fn layers_with_widget() -> Value {
    json!([{"layerUid":"L0"},{"layerUid":"L1","locked":true},{"layerUid":"L2"},
        {"layerUid":"W","widget":{"kind":"slider","x":{"key":"k","min":0,"max":1}}}])
}

pub(crate) fn target(layer: &str, stroke: &str) -> Value {
    json!({"contextId":"scene-root","frameScope":{"kind":"authored","frame":0},"layerUid":layer,"strokeId":stroke})
}

pub(crate) fn raster_item(asset: &str, x: i64, y: i64, w: i64, h: i64) -> Value {
    json!({"assetId":asset,"x":x,"y":y,"width":w,"height":h,"opacity":1})
}

pub(crate) fn project(drawing: &Value, inventory: AssetInventory) -> ProjectEnvelope {
    let document = decode_storage_project(drawing.to_string().as_bytes()).unwrap();
    ProjectEnvelope::no_media(ProjectMetadata::new(320, 180, 24.).unwrap(), document)
        .with_asset_inventory(Some(inventory))
}

pub(crate) fn owner(drawing: &Value) -> NativeObjectHistory {
    NativeObjectHistory::from_admitted_project("items", project(drawing, inventory())).unwrap()
}

/// A bare drawing: no project, hence no asset inventory.
pub(crate) fn bare(drawing: &Value) -> NativeObjectHistory {
    NativeObjectHistory::new(
        "bare",
        decode_storage_project(drawing.to_string().as_bytes()).unwrap(),
    )
    .unwrap()
}

pub(crate) fn head(o: &NativeObjectHistory) -> ObjectSnapshot {
    o.acquire_snapshot(o.content_revision()).unwrap()
}

/// The canonical storage bytes of the head drawing.
pub(crate) fn bytes(o: &NativeObjectHistory) -> Vec<u8> {
    encode_storage_project(head(o).document()).unwrap()
}

/// Everything a refused or no-op edit must leave untouched.
pub(crate) fn state(o: &NativeObjectHistory) -> (u64, (usize, usize), String, Vec<u8>) {
    (
        o.content_revision(),
        o.history_depths(),
        head(o).snapshot_id().into(),
        bytes(o),
    )
}

pub(crate) fn prepare(
    o: &NativeObjectHistory,
    payload: &Value,
) -> Result<PreparedItemEdit, DispatchErrorCode> {
    o.prepare_item_edit_json(payload.to_string().as_bytes())
}

/// Prepare and commit one transaction; `Ok(revision)` after it.
pub(crate) fn run(o: &mut NativeObjectHistory, payload: &Value) -> Result<u64, DispatchErrorCode> {
    let candidate = prepare(o, payload)?;
    o.commit_item_edit(candidate)
}

pub(crate) fn request(o: &NativeObjectHistory, id: &str, payload: &Value) -> OpacityRequest {
    serde_json::from_value(
        json!({"apiVersion":2,"requestId":id,"instanceId":o.instance_id(),
        "documentId":o.document_id(),"expectedRevision":o.content_revision(),
        "operation":OP,"payload":payload}),
    )
    .unwrap()
}

pub(crate) fn dispatch(o: &mut NativeObjectHistory, r: &OpacityRequest) -> Value {
    serde_json::to_value(o.try_dispatch_object_command(r).unwrap().unwrap()).unwrap()
}

pub(crate) fn items_of(o: &NativeObjectHistory) -> Value {
    serde_json::to_value(head(o).document()).unwrap()["items"].clone()
}

/// The oracle rows of a fixture text: every line that is not a `#` comment, optionally filtered by a leading tag.
pub(crate) fn rows(text: &str, tag: Option<&str>) -> Vec<Value> {
    text.lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| match tag {
            None => Some(serde_json::from_str(line).expect("oracle row is JSON")),
            Some(tag) => line
                .strip_prefix(tag)
                .and_then(|rest| rest.strip_prefix(' '))
                .map(|rest| serde_json::from_str(rest).expect("oracle row is JSON")),
        })
        .collect()
}

/// A legacy path record of the first example case, moved to the given stable target.
pub(crate) fn legacy_record(layer: &str, stroke: &str) -> Value {
    let cases: Value = serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    ))
    .unwrap();
    let mut record = cases["records"][0].clone();
    record["target"] = target(layer, stroke);
    record
}
