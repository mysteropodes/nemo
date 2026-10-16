//! Fixture documents and helpers shared by the L02-E tests (see the doc comment
//! of `object_layer_lifecycle_plan.rs` for the shape of `build`).

use crate::object_document::ObjectDocument;
use serde_json::{json, Value};

pub(crate) const PALETTE: [&str; 5] = ["#5fa875", "#b49be6", "#f0a585", "#e88bae", "#e7c74c"];

pub(crate) struct Spec {
    pub(crate) name: String,
    pub(crate) color: String,
    pub(crate) visible: bool,
}

pub(crate) fn plain(count: usize) -> Vec<Spec> {
    (0..count)
        .map(|index| Spec {
            name: format!("L{index}"),
            color: PALETTE[index % PALETTE.len()].to_string(),
            visible: true,
        })
        .collect()
}

pub(crate) fn uid(index: i64) -> String {
    format!("L{index}")
}

pub(crate) fn record(layer: &str, stroke: &str, frame: u32) -> Value {
    let corner =
        |x: u32| json!({"point":{"x":x,"y":0},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}});
    json!({"schemaVersion":1,"family":"closed-solid-fill-cubic-path",
        "target":{"contextId":"scene-root","frameScope":{"kind":"authored","frame":frame},
            "layerUid":layer,"strokeId":stroke},
        "geometry":{"kind":"cubic-path","closed":true,"coordinateSpace":"document-world",
            "handleSpace":"relative","segments":[corner(0), corner(10)]},
        "fill":{"kind":"solid","r":0.1,"g":0.2,"b":0.3,"a":1}})
}

pub(crate) fn timing() -> Value {
    json!({"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing",
        "contextId":"scene-root","keys":[{"frame":0}],"samples":[]})
}

fn element(layer: &str, stroke: &str) -> Value {
    json!({"layerUid":layer,"strokeId":stroke,"naturalPosition":0,"order":{}})
}

/// The fixture as JSON; `version` 1 has no order plan, 2 has two contexts.
pub(crate) fn build_value(specs: &[Spec], version: u32) -> Value {
    let count = specs.len();
    let mut layers = Vec::new();
    let mut objects = Vec::new();
    let mut holders = Vec::new();
    let mut base_order = Vec::new();
    let mut mirror_order = Vec::new();
    let (mut frame0, mut frame1) = (Vec::new(), Vec::new());
    for (index, spec) in specs.iter().enumerate() {
        let (layer, stroke) = (format!("L{index}"), format!("s{index}"));
        layers.push(
            json!({"layerUid":layer,"name":spec.name,"visible":spec.visible,
            "locked":false,"color":spec.color,"parentLayerUid":null,"timing":timing()}),
        );
        objects.push(record(&layer, &stroke, 0));
        frame0.push(element(&layer, &stroke));
        if index % 2 == 1 {
            objects.push(record(&layer, &stroke, 1));
            frame1.push(element(&layer, &stroke));
        }
        if index % 2 == 0 {
            holders.push(json!({"target":{"contextId":"scene-root","layerUid":layer},
                "properties":[{"property":"position","static":[index,0]}]}));
        }
        if index % 3 == 0 {
            holders.push(json!({"target":{"contextId":"scene-root","layerUid":layer,"strokeId":stroke},
                "controls":[{"key":"xc_a","name":"Amount","type":"number","default":[3]}],
                "properties":[{"property":"opacity","static":[50]},{"property":"xc_a","static":[7]}]}));
        }
        let authored = if index % 2 == 1 {
            json!({"static":[9]})
        } else {
            json!({})
        };
        base_order.push(json!({"layerUid":layer,"naturalPosition":index,"order":authored}));
        mirror_order
            .push(json!({"layerUid":layer,"naturalPosition":count - 1 - index,"order":authored}));
    }
    let mut document = json!({"format":"nemo.native-object-document","formatVersion":version,
        "totalFrames":24,"layers":layers,"objects":objects,"propertyHolders":holders});
    if version == 2 {
        document["compositionOrder"] = json!({"format":"nemo.native-object-order-plan","formatVersion":1,
            "documentFormatVersion":2,"migration":"explicit-plan-required","referencePolicy":"unresolved",
            "contexts":[{"contextId":"scene-root","layers":base_order,"scopes":[
                {"frameScope":{"kind":"authored","frame":0},"elements":frame0},
                {"frameScope":{"kind":"authored","frame":1},"elements":frame1}]},
                {"contextId":"unused-symbol","layers":mirror_order,"scopes":[]}]});
    }
    document
}

pub(crate) fn parse(value: Value) -> ObjectDocument {
    serde_json::from_value(value).expect("fixture parses")
}

pub(crate) fn build(specs: &[Spec], version: u32) -> ObjectDocument {
    let document = parse(build_value(specs, version));
    crate::object_codec::validate_storage(&document).expect("fixture is admitted");
    document
}

/// Storage admission as the transaction leaf runs it on a candidate document.
pub(crate) fn admitted(document: &ObjectDocument) -> bool {
    crate::object_codec::validate_storage(document).is_ok()
}

pub(crate) fn json_of(document: &ObjectDocument) -> Value {
    serde_json::to_value(document).expect("document serializes")
}

pub(crate) fn text_of(document: &ObjectDocument) -> String {
    serde_json::to_string(document).expect("document serializes")
}

/// Natural slot of `uid` in order context `context` (0 scene-root, 1 mirror).
pub(crate) fn slot_of(document: &ObjectDocument, context: usize, uid: &str) -> u32 {
    let plan = document.composition_order.value().expect("order plan");
    let entry = plan.contexts[context]
        .layers
        .iter()
        .find(|entry| entry.layer_uid == uid);
    entry.expect("layer in context").natural_position
}

pub(crate) fn layer_uids(document: &ObjectDocument) -> Vec<String> {
    document
        .layers
        .iter()
        .map(|layer| layer.layer_uid.clone())
        .collect()
}
