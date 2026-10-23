//! Literal F7 fixtures; shared dense builder/row parser remain the existing files.
use super::{build, rows};
use crate::commands::ObjectFillOwner;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::object_document::ObjectDocument;
use serde_json::{json, Value};

pub(super) fn document(text: &str, form: build::Build) -> ObjectDocument {
    build::document(&rows::parse_layers(text), form)
}
pub(super) fn admit(value: Value) -> ObjectDocument {
    decode_storage_project(value.to_string().as_bytes()).expect("literal fixture is admitted")
}
pub(super) fn owner(document: ObjectDocument) -> ObjectFillOwner {
    ObjectFillOwner::new("blank-key", document).unwrap()
}
pub(super) fn head(owner: &ObjectFillOwner) -> ObjectDocument {
    owner
        .acquire_snapshot(owner.content_revision())
        .unwrap()
        .document()
        .clone()
}
pub(super) fn value(document: &ObjectDocument) -> Value {
    serde_json::to_value(document).unwrap()
}
pub(super) fn bytes(owner: &ObjectFillOwner) -> Vec<u8> {
    encode_storage_project(&head(owner)).unwrap()
}
pub(super) fn payload(layer: &str, cell: u32) -> Vec<u8> {
    json!({"command":"object.layer.key.blank","layerUid":layer,"cell":cell})
        .to_string()
        .into_bytes()
}
/// Full expected snapshot: ONLY target timing and authored cell content/order change.
/// `dense` is hand-written expected timing, not a call to the timing mutation kernel.
pub(super) fn expected_blank(
    input: &ObjectDocument,
    index: usize,
    cell: u32,
    dense: &str,
) -> Value {
    let mut expected = value(input);
    let layer = format!("L{index}");
    expected["layers"][index]["timing"] = build::timing_json(&rows::parse_layers(dense)[0]);
    expected["objects"]
        .as_array_mut()
        .unwrap()
        .retain(|record| {
            let target = &record["target"];
            !(target["contextId"] == "scene-root"
                && target["layerUid"] == layer
                && target["frameScope"]["kind"] == "authored"
                && target["frameScope"]["frame"] == cell)
        });
    if let Some(contexts) = expected
        .get_mut("compositionOrder")
        .and_then(|v| v.get_mut("contexts"))
        .and_then(Value::as_array_mut)
    {
        for context in contexts
            .iter_mut()
            .filter(|c| c["contextId"] == "scene-root")
        {
            for scope in context["scopes"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .filter(|scope| {
                    scope["frameScope"]["kind"] == "authored"
                        && scope["frameScope"]["frame"] == cell
                })
            {
                scope["elements"]
                    .as_array_mut()
                    .unwrap()
                    .retain(|element| element["layerUid"] != layer);
            }
        }
    }
    expected
}
pub(super) fn component(locked: bool) -> ObjectDocument {
    let mut v = value(&document(
        "K=a.1.10;.;.;K=b.2.20;.|K;.;.;K;K",
        build::Build::V2,
    ));
    v["layers"][1]["locked"] = json!(locked);
    let inside = build::record_json("symbol", "authored", "inside", "inner", 0, 3, 30);
    v["componentTables"] = json!({"components":{
        "format":"nemo.native-component-set","formatVersion":1,
        "components":[{"symbolId":"symbol","name":"preserve inner","totalFrames":3,
            "layers":[{"layerUid":"inside"}],"objects":[inside]}],
        "instances":[{"contextId":"scene-root","layerUid":"L1","symbolId":"symbol",
            "symSpeed":2,"symPlacedAt":0,"symMatrix":[1,0,0,1,12,13],
            "frameKeys":[{"frame":0,"componentFrame":1},{"frame":4,"componentFrame":2,"blankOverride":false}]}]
    }});
    admit(v)
}
pub(super) fn target(layer: &str, frame: u32, stroke: &str) -> Value {
    json!({"contextId":"scene-root","frameScope":{"kind":"authored","frame":frame},"layerUid":layer,"strokeId":stroke})
}
/// Reuse one immutable existing text oracle only as admitted source data, never as F7 output.
pub(super) fn mixed_items() -> ObjectDocument {
    let mut v = value(&document(
        "K=a.1.10;.;.;K=b.2.20|K=c.3.30;.;.;K=d.4.40",
        build::Build::V2Extras,
    ));
    let line = include_str!("fixtures/object-text-layout-commands/oracle.txt")
        .lines()
        .find(|line| line.starts_with("CREATE "))
        .unwrap();
    let row: Value = serde_json::from_str(line.strip_prefix("CREATE ").unwrap()).unwrap();
    let image = json!({"assetId":"image-one","x":0,"y":0,"width":20,"height":10,"opacity":1});
    v["items"] = json!({"raster":[{"target":target("L0",0,"raster-zero"),"item":image},
        {"target":target("L1",3,"raster-three"),"item":image}],
        "text":[{"target":target("L0",0,"text-zero"),"block":row["record"]},
        {"target":target("L1",3,"text-three"),"block":row["record"]}]});
    for (frame, layer, raster, text) in [
        (0, "L0", "raster-zero", "text-zero"),
        (3, "L1", "raster-three", "text-three"),
    ] {
        let scope = v["compositionOrder"]["contexts"][0]["scopes"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|scope| scope["frameScope"]["frame"] == frame)
            .unwrap();
        scope["elements"].as_array_mut().unwrap().extend([
            json!({"layerUid":layer,"strokeId":raster,"naturalPosition":1,"order":{"static":[-1]}}),
            json!({"layerUid":layer,"strokeId":text,"naturalPosition":2,"order":{"static":[9]}}),
        ]);
    }
    admit(v)
}
