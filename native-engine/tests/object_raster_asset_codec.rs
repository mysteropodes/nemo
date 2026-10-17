//! Document / codec join of the lane-6 items: the `items` member round trips through the single native codec, every
//! structural refusal is typed, and asset references resolve against a project inventory. Items that live in
//! `objects` are path records; these tests prove the additive member leaves the legacy document untouched.
use super::*;
use crate::object_codec::{
    decode_storage_project, encode_ordered_project, encode_storage_project, migrate_ordered_project,
};
use crate::object_document::object_raster_asset_kernels::object_raster_asset_item::RasterItem;
use crate::object_document::{AssetId, AssetKind, AssetRecord, AssetSource, ItemRecords, Presence};
use serde_json::{json, Value};
use std::sync::Arc;

const TEXT: &str = r##"{"text":"Hi","font":{"kind":"bundled","fontKey":"Roboto-Regular"},"size":48,"color":"#112233","align":"left","anchorTopLeft":{"x":10,"y":20}}"##;

fn target(layer: &str, stroke: &str) -> Value {
    json!({"contextId":"scene-root","frameScope":{"kind":"authored","frame":0},"layerUid":layer,"strokeId":stroke})
}
fn text_block() -> Value {
    serde_json::from_str(TEXT).unwrap()
}
fn raster(asset: &str) -> Value {
    json!({"assetId":asset,"x":200,"y":150,"width":64,"height":32,"opacity":1})
}
fn base(items: Value) -> Value {
    let mut doc = json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":4,
        "layers":[{"layerUid":"L0"},{"layerUid":"L1"}],"objects":[]});
    if !items.is_null() {
        doc["items"] = items;
    }
    doc
}
fn both() -> Value {
    json!({"raster":[{"target":target("L0","r0"),"item":raster("img-1")}],
           "text":[{"target":target("L1","t0"),"block":text_block()}]})
}
fn decode(doc: &Value) -> Result<crate::object_document::ObjectDocument, ObjectCodecError> {
    decode_storage_project(doc.to_string().as_bytes())
}
fn kind(doc: &Value) -> ObjectCodecErrorKind {
    decode(doc).unwrap_err().kind()
}

#[test]
fn a_document_with_both_item_kinds_round_trips_and_a_document_without_items_is_unchanged() {
    let doc = base(both());
    let decoded = decode(&doc).unwrap();
    let encoded = encode_storage_project(&decoded).unwrap();
    assert_eq!(serde_json::from_slice::<Value>(&encoded).unwrap(), doc);
    assert_eq!(decode_storage_project(&encoded).unwrap(), decoded);
    assert_eq!(
        serde_json::to_string(decoded.item_records().unwrap()).unwrap(),
        r##"{"raster":[{"target":{"contextId":"scene-root","frameScope":{"kind":"authored","frame":0},"layerUid":"L0","strokeId":"r0"},"item":{"assetId":"img-1","x":200,"y":150,"width":64,"height":32,"opacity":1}}],"text":[{"target":{"contextId":"scene-root","frameScope":{"kind":"authored","frame":0},"layerUid":"L1","strokeId":"t0"},"block":{"text":"Hi","font":{"kind":"bundled","fontKey":"Roboto-Regular"},"size":48,"color":"#112233","align":"left","anchorTopLeft":{"x":10,"y":20}}}]}"##
    );
    let plain = base(Value::Null);
    let bytes = encode_storage_project(&decode(&plain).unwrap()).unwrap();
    assert!(
        !String::from_utf8(bytes).unwrap().contains("items"),
        "no items member is ever invented"
    );
}

#[test]
fn each_list_may_be_alone_but_never_empty() {
    let raster_only = base(json!({"raster":both()["raster"].clone()}));
    let text_only = base(json!({"text":both()["text"].clone()}));
    for doc in [&raster_only, &text_only] {
        assert_eq!(
            serde_json::from_slice::<Value>(
                &encode_storage_project(&decode(doc).unwrap()).unwrap()
            )
            .unwrap(),
            *doc
        );
    }
    for doc in [
        base(json!({})),
        base(json!({"raster":[]})),
        base(json!({"text":[],"raster":both()["raster"].clone()})),
        base(json!({"raster":both()["raster"].clone(),"text":[]})),
    ] {
        assert_eq!(kind(&doc), ObjectCodecErrorKind::Invalid, "{doc}");
    }
}

#[test]
fn structural_refusals_are_typed() {
    let mut cases: Vec<(&str, Value, ObjectCodecErrorKind)> = Vec::new();
    use ObjectCodecErrorKind::{DuplicateId, Invalid, Unsupported};
    let mut null_container = base(Value::Null);
    null_container["items"] = Value::Null;
    cases.push(("null container", null_container, Invalid));
    cases.push((
        "unknown container member",
        base(json!({"raster":both()["raster"].clone(),"groups":[]})),
        Unsupported,
    ));
    cases.push((
        "raster list is not a list",
        base(json!({"raster":{}})),
        Invalid,
    ));
    let mut dup = both();
    let first_raster = dup["raster"][0].clone();
    dup["raster"].as_array_mut().unwrap().push(first_raster);
    cases.push(("duplicate raster target", base(dup), DuplicateId));
    let mut shared = both();
    shared["text"][0]["target"] = target("L0", "r0");
    cases.push(("raster and text share a target", base(shared), DuplicateId));
    let mut unknown = both();
    unknown["raster"][0]["target"]["layerUid"] = json!("L9");
    cases.push(("unknown layer", base(unknown), Invalid));
    let mut on_widget = both();
    on_widget["raster"][0]["target"]["layerUid"] = json!("W");
    let mut widget_doc = base(on_widget);
    widget_doc["layers"] = json!([{"layerUid":"L1"},
        {"layerUid":"W","widget":{"kind":"slider","x":{"key":"k","min":0,"max":1}}}]);
    cases.push(("items on a widget layer", widget_doc, Invalid));
    let mut late = both();
    late["raster"][0]["target"]["frameScope"]["frame"] = json!(4);
    cases.push(("frame past totalFrames", base(late), Invalid));
    let mut reference = both();
    reference["raster"][0]["target"]["frameScope"]["kind"] = json!("reference");
    cases.push(("reference scope", base(reference), Unsupported));
    let mut other_context = both();
    other_context["raster"][0]["target"]["contextId"] = json!("other");
    cases.push(("foreign context", base(other_context), Invalid));
    let mut empty_stroke = both();
    empty_stroke["raster"][0]["target"]["strokeId"] = json!("");
    cases.push(("empty stroke id", base(empty_stroke), Invalid));
    let mut zero = both();
    zero["raster"][0]["item"]["width"] = json!(0);
    cases.push(("zero width raster", base(zero), Invalid));
    let mut null_member = both();
    null_member["raster"][0]["item"]["opacity"] = Value::Null;
    cases.push(("null opacity", base(null_member), Invalid));
    let mut group = both();
    group["raster"][0]["item"]["groupId"] = json!("g");
    cases.push(("a member of another record kind", base(group), Unsupported));
    let mut blank = both();
    blank["text"][0]["block"]["text"] = json!("   ");
    cases.push(("blank text", base(blank), Invalid));
    let mut overflow = both();
    overflow["text"][0]["block"]["textOverflow"] = json!(false);
    cases.push(("stored overflow", base(overflow), Unsupported));
    let long_layer = format!("L{}", "x".repeat(250));
    let mut holder =
        base(json!({"text":[{"target":target(&long_layer,"t0"),"block":text_block()}]}));
    holder["layers"] = json!([{"layerUid":long_layer}]);
    cases.push(("holder that cannot name a glyph", holder, Invalid));
    for (label, doc, expected) in cases {
        let error = decode(&doc).unwrap_err();
        assert_eq!(error.kind(), expected, "{label}: {}", error.message());
    }
}

#[test]
fn an_item_target_may_not_collide_with_a_path_record_of_the_document() {
    let cases: Value = serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    ))
    .unwrap();
    let mut record = cases["records"][0].clone();
    record["target"] = target("L0", "r0");
    let mut doc = base(both());
    doc["objects"] = json!([record]);
    assert_eq!(kind(&doc), ObjectCodecErrorKind::DuplicateId);
    doc["objects"][0]["target"]["strokeId"] = json!("v0");
    assert!(
        decode(&doc).is_ok(),
        "a different stroke id coexists in the same document"
    );
}

fn plan_with(scopes: Value) -> Value {
    json!({"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
        "migration":"explicit-plan-required","referencePolicy":"unresolved",
        "contexts":[{"contextId":"scene-root","layers":[
            {"layerUid":"L0","naturalPosition":0,"order":{}},{"layerUid":"L1","naturalPosition":1,"order":{}}],
            "scopes":scopes}]})
}
fn element(layer: &str, stroke: &str) -> Value {
    json!({"layerUid":layer,"strokeId":stroke,"naturalPosition":0,"order":{}})
}
fn scope(elements: Vec<Value>) -> Value {
    json!([{"frameScope":{"kind":"authored","frame":0},"elements":elements}])
}
fn covered() -> Value {
    scope(vec![element("L0", "r0"), element("L1", "t0")])
}
fn ordered_doc(scopes: Value) -> Value {
    let mut doc = base(both());
    doc["formatVersion"] = json!(2);
    doc["compositionOrder"] = plan_with(scopes);
    doc
}

#[test]
fn a_v2_document_carries_items_when_its_order_plan_covers_every_one_of_them() {
    let doc = ordered_doc(covered());
    let decoded = decode(&doc).unwrap();
    assert_eq!(decoded.item_records().unwrap().raster_entries().len(), 1);
    assert_eq!(decoded.item_records().unwrap().text_entries().len(), 1);
    let encoded = encode_storage_project(&decoded).unwrap();
    assert_eq!(serde_json::from_slice::<Value>(&encoded).unwrap(), doc);
    assert_eq!(decode_storage_project(&encoded).unwrap(), decoded);
    let mut phantom = ordered_doc(covered());
    phantom["items"]["raster"][0]["target"]["strokeId"] = json!("other");
    for (label, doc) in [
        ("no scopes at all", ordered_doc(json!([]))),
        (
            "a missing text element",
            ordered_doc(scope(vec![element("L0", "r0")])),
        ),
        (
            "a missing raster element",
            ordered_doc(scope(vec![element("L1", "t0")])),
        ),
        ("an element without its item", phantom),
    ] {
        assert!(decode(&doc).is_err(), "{label}");
    }
}

#[test]
fn a_v1_document_with_items_migrates_only_under_a_plan_that_covers_them() {
    let v1 = decode(&base(both())).unwrap();
    let migrated =
        migrate_ordered_project(&v1, plan_with(covered()).to_string().as_bytes()).unwrap();
    let wire: Value = serde_json::from_slice(&encode_ordered_project(&migrated).unwrap()).unwrap();
    assert_eq!(
        wire["items"],
        both(),
        "the migration keeps the items byte for byte"
    );
    assert_eq!(wire["compositionOrder"]["contexts"][0]["scopes"], covered());
    let uncovered = plan_with(json!([])).to_string();
    assert!(migrate_ordered_project(&v1, uncovered.as_bytes()).is_err());
}

fn inventory() -> AssetInventory {
    let linked = |id: &str, kind: AssetKind| {
        Arc::new(
            AssetRecord::new(
                AssetId::new(id.into()).unwrap(),
                format!("{id}.bin"),
                kind,
                AssetSource::linked(format!("media/{id}"), 4096).unwrap(),
            )
            .unwrap(),
        )
    };
    AssetInventory::new(
        vec![
            linked("img-1", AssetKind::Image),
            linked("font-1", AssetKind::Font),
        ],
        Vec::new(),
        8,
        0,
    )
    .unwrap()
}

#[test]
fn asset_references_resolve_against_the_inventory_by_kind() {
    let inv = inventory();
    let with_font = |font: Value| {
        let mut doc = both();
        doc["text"][0]["block"]["font"] = font;
        decode(&base(doc)).unwrap()
    };
    let plain = decode(&base(both())).unwrap();
    assert!(validate_item_assets(&plain, Some(&inv)).is_ok());
    assert!(
        validate_item_assets(&decode(&base(Value::Null)).unwrap(), None).is_ok(),
        "no items: no inventory needed"
    );
    assert!(
        validate_item_assets(&plain, None).is_err(),
        "a raster item needs an inventory"
    );
    assert!(validate_item_assets(
        &with_font(json!({"kind":"asset","assetId":"font-1"})),
        Some(&inv)
    )
    .is_ok());
    for (label, font) in [
        ("missing font", json!({"kind":"asset","assetId":"nope"})),
        ("image as font", json!({"kind":"asset","assetId":"img-1"})),
    ] {
        let error = validate_item_assets(&with_font(font), Some(&inv)).unwrap_err();
        assert_eq!(error.kind(), ObjectCodecErrorKind::Invalid, "{label}");
    }
    let mut ghost = both();
    ghost["raster"][0]["item"]["assetId"] = json!("ghost");
    assert!(validate_item_assets(&decode(&base(ghost)).unwrap(), Some(&inv)).is_err());
    let mut font_as_image = both();
    font_as_image["raster"][0]["item"]["assetId"] = json!("font-1");
    assert!(validate_item_assets(&decode(&base(font_as_image)).unwrap(), Some(&inv)).is_err());
}

#[test]
fn the_record_helpers_name_targets_assets_and_holders() {
    let document = decode(&base(both())).unwrap();
    let items = document.item_records().unwrap();
    let strokes: Vec<&str> = items.targets().map(|t| t.stroke_id()).collect();
    assert_eq!(strokes, vec!["r0", "t0"], "raster first, stored order");
    assert!(items.references_asset(&AssetId::new("img-1".into()).unwrap()));
    assert!(!items.references_asset(&AssetId::new("img-2".into()).unwrap()));
    assert_eq!(
        text_holder_id(&items.text_entries()[0].target),
        "scene-root:L1:t0"
    );
    let item = RasterItem::decode(&raster("img-1").to_string()).unwrap();
    assert_eq!(items.raster_entries()[0].item, item);
    assert_eq!(
        ItemRecords::normalized(Vec::new(), Vec::new()),
        Presence::Missing
    );
}
