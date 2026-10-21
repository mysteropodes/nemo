use super::*;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use serde_json::{json, Value};
fn drawing() -> Value {
    BitmapBrushFields::test_drawing()
}
fn decode(wire: &Value) -> Result<crate::object_document::ObjectDocument, ObjectCodecError> {
    decode_storage_project(wire.to_string().as_bytes())
}
#[test]
fn actual_storage_codec_roundtrip_keeps_descriptor_and_existing_companion_as_distinct_sources() {
    let source = drawing();
    let doc = decode(&source).unwrap();
    let bytes = encode_storage_project(&doc).unwrap();
    assert_eq!(serde_json::from_slice::<Value>(&bytes).unwrap(), source);
    assert_eq!(decode_storage_project(&bytes).unwrap(), doc);
    assert_eq!(
        doc.objects()[0]
            .vector_ext
            .value()
            .unwrap()
            .bitmap_brush
            .value()
            .unwrap()
            .brush_group_id,
        "ink group:雪"
    );
    assert_eq!(
        doc.item_records().unwrap().raster_entries()[0]
            .item
            .asset_id()
            .as_str(),
        "ink"
    );
}
#[test]
fn malformed_camouflage_color_spec_profile_and_nonvector_crossing_are_refused_atomically() {
    assert!(decode(&drawing()).is_ok());
    for (pointer, value) in [
        (
            "/objects/0/vectorExt/bitmapBrush/bitmapBrushSpec/color",
            json!("red"),
        ),
        ("/objects/0/vectorExt/bitmapBrush/brushGroupId", json!("")),
        (
            "/objects/0/vectorExt/bitmapBrush/bitmapPressureProfile",
            json!([[0, 1, 2, 3]]),
        ),
        (
            "/objects/0/vectorExt/bitmapBrush/preTextureStroke",
            json!("invalid"),
        ),
        (
            "/objects/0/vector/stroke",
            json!({"kind":"solid","r":1,"g":0,"b":0,"a":1}),
        ),
        ("/objects/0/family", json!("closed-solid-fill-cubic-path")),
    ] {
        let mut wire = drawing();
        *wire.pointer_mut(pointer).unwrap() = value;
        assert!(decode(&wire).is_err(), "{pointer}");
    }
    let mut missing = drawing();
    missing["objects"][0]["vectorExt"]["bitmapBrush"]
        .as_object_mut()
        .unwrap()
        .remove("preTextureStroke");
    assert!(decode(&missing).is_err());
}
#[test]
fn custom_tip_is_preserved_and_not_silently_rewritten_as_a_builtin() {
    let mut wire = drawing();
    wire["objects"][0]["vectorExt"]["bitmapBrush"]["bitmapBrushSpec"]["tip"] =
        json!("abr:opaque imported mask");
    let doc = decode(&wire).unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&encode_storage_project(&doc).unwrap()).unwrap(),
        wire
    );
}
#[test]
fn trim_admission_requires_actual_authored_bitmap_target_and_refuses_expression_or_foreign_holder()
{
    let mut source = drawing();
    source["propertyHolders"] = json!([{"target":{"contextId":"scene-root","layerUid":"L","strokeId":"anchor"},
        "properties":[{"property":"trimEnd","static":[50]}]}]);
    let doc = decode(&source).unwrap();
    assert!(trim_holder_consumed(
        &doc,
        &doc.property_holders().unwrap()[0]
    ));
    let mut plain = source["objects"][0].clone();
    plain["target"]["strokeId"] = json!("plain");
    plain.as_object_mut().unwrap().remove("vectorExt");
    source["objects"].as_array_mut().unwrap().push(plain);
    source["compositionOrder"]["contexts"][0]["scopes"][0]["elements"]
        .as_array_mut()
        .unwrap()
        .push(json!({"layerUid":"L","strokeId":"plain","naturalPosition":2,"order":{}}));
    for mutation in ["expression", "foreign", "control", "other-property"] {
        let mut wire = source.clone();
        match mutation {
            "expression" => {
                wire["propertyHolders"][0]["properties"][0]["expression"] =
                    json!({"enabled":true,"code":"50"})
            }
            "foreign" => wire["propertyHolders"][0]["target"]["strokeId"] = json!("plain"),
            "control" => {
                wire["propertyHolders"][0]["controls"] =
                    json!([{ "key":"c","name":"C","type":"number","default":[0]}])
            }
            _ => wire["propertyHolders"][0]["properties"][0]["property"] = json!("rotation"),
        }
        let doc = decode(&wire).unwrap();
        assert!(
            !trim_holder_consumed(&doc, &doc.property_holders().unwrap()[0]),
            "{mutation}"
        );
    }
}
#[cfg(all(feature = "history", feature = "project_media_codec"))]
#[test]
fn real_history_and_full_project_reopen_keep_brush_fields_and_retained_snapshot() {
    use crate::history::NativeObjectHistory;
    let wire = json!({"format":"nemo.native-project-document","formatVersion":2,"project":{"canvasW":48,"canvasH":48,"fps":24},
        "drawing":drawing(),"viewportReference":null,"assets":[{"id":"ink","name":"Ink","kind":"image","source":{"kind":"embedded",
            "mediaType":"image/png","encoding":"base64","data":crate::object_codec::encoded_wire_test::PNG}}],"assetInventory":{"assetIds":["ink"],"placements":[]}});
    let mut owner =
        NativeObjectHistory::from_project_file("brush-history", wire.to_string().as_bytes())
            .unwrap();
    let old = owner.acquire_snapshot(0).unwrap();
    let before = owner.serialize_project_file(0, old.snapshot_id()).unwrap();
    let payload = json!({"command":"object.fill.set","stableTarget":{"contextId":"scene-root","layerUid":"L","strokeId":"anchor",
        "frameScope":{"kind":"authored","frame":0}},"fill":{"kind":"solid","r":1,"g":0,"b":0,"a":1}});
    let candidate = owner
        .prepare_fill_json(payload.to_string().as_bytes())
        .unwrap();
    assert_eq!(owner.commit_fill(candidate), Ok(1));
    let one = owner.acquire_snapshot(1).unwrap();
    let changed = owner.serialize_project_file(1, one.snapshot_id()).unwrap();
    assert_eq!(old.serialize_project_file().unwrap(), before);
    owner.undo(1).unwrap();
    let two = owner.acquire_snapshot(2).unwrap();
    assert_eq!(
        owner.serialize_project_file(2, two.snapshot_id()).unwrap(),
        before
    );
    owner.redo(2).unwrap();
    let three = owner.acquire_snapshot(3).unwrap();
    assert_eq!(
        owner
            .serialize_project_file(3, three.snapshot_id())
            .unwrap(),
        changed
    );
    let fresh = NativeObjectHistory::from_project_file("brush-fresh", &changed).unwrap();
    let pin = fresh.acquire_snapshot(0).unwrap();
    assert_eq!(
        fresh.serialize_project_file(0, pin.snapshot_id()).unwrap(),
        changed
    );
}

#[test]
fn genuine_component_table_with_bitmap_anchor_refuses_before_silent_component_paint() {
    let original = include_str!("fixtures/object-component-evaluate/oracle.txt")
        .lines()
        .find_map(|line| line.strip_prefix("G\t"))
        .expect("existing real component document");
    let mut wire: Value = serde_json::from_str(original).unwrap();
    assert!(
        decode(&wire).is_ok(),
        "the unmodified genuine component table is admitted"
    );
    let record = &mut wire["componentTables"]["components"]["components"][0]["objects"][0];
    let target = record["target"].clone();
    let mut anchor = drawing()["objects"][0].clone();
    anchor["target"] = target.clone();
    *record = anchor;
    assert_ne!(target["contextId"], json!("scene-root"));
    let error = decode(&wire).unwrap_err();
    assert_eq!(error.kind(), ObjectCodecErrorKind::Unsupported);
    assert_eq!(
        error.message(),
        "bitmap brushes inside component contexts are unavailable"
    );
}
