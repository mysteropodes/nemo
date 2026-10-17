//! Oracle parser and shared builders of the layer compositing record tests (L08-J). The rows were produced
//! by executing verbatim baseline statements/functions (fbe4b8a3) in a Node vm; see the leaf HANDOFF for the
//! generator. Nothing here derives an expectation from the production code under test.
use crate::object_document::{ObjectLayer, Presence};
use serde::Deserialize;
use serde_json::Value;

pub(crate) const ORACLE: &str = include_str!("fixtures/scene-effect-layer-record/oracle.txt");
pub(crate) const CASES: &str =
    include_str!("../../engineering/application/examples/native-object-v1/cases.json");

/// Every oracle row of one kind, in file order.
pub(crate) fn rows(kind: &str) -> Vec<Value> {
    ORACLE
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .filter(|row| row["kind"] == kind)
        .collect()
}

#[derive(Deserialize)]
struct RawInput {
    id: String,
    input: Box<serde_json::value::RawValue>,
}

/// `(id, raw text of the baseline input layer)` for every `import` row, spelling preserved.
pub(crate) fn raw_import_inputs() -> Vec<(String, String)> {
    ORACLE
        .lines()
        .filter(|line| line.starts_with("{\"kind\":\"import\""))
        .map(|line| {
            let row: RawInput = serde_json::from_str(line).unwrap();
            (row.id, row.input.get().to_string())
        })
        .collect()
}

/// `{"layerUid":"L0", ...members of raw}`: the baseline input as one native layer.
pub(crate) fn layer_text(uid: &str, raw: &str) -> String {
    if raw == "{}" {
        format!("{{\"layerUid\":\"{uid}\"}}")
    } else {
        format!("{{\"layerUid\":\"{uid}\",{}", &raw[1..])
    }
}

/// A v1 document text around the given layer texts (no objects).
pub(crate) fn document_text(layers: &[String]) -> String {
    format!(
        "{{\"format\":\"nemo.native-object-document\",\"formatVersion\":1,\"totalFrames\":1,\"layers\":[{}],\"objects\":[]}}",
        layers.join(",")
    )
}

/// A v2 (explicit composition order) document text around the given `(uid, layer text)` pairs.
pub(crate) fn document_text_v2(layers: &[(&str, String)]) -> String {
    let order: Vec<String> = layers
        .iter()
        .enumerate()
        .map(|(position, (uid, _))| {
            format!("{{\"layerUid\":\"{uid}\",\"naturalPosition\":{position},\"order\":{{}}}}")
        })
        .collect();
    let texts: Vec<&str> = layers.iter().map(|(_, text)| text.as_str()).collect();
    format!(
        "{{\"format\":\"nemo.native-object-document\",\"formatVersion\":2,\"totalFrames\":1,\"layers\":[{}],\"objects\":[],\"compositionOrder\":{{\"format\":\"nemo.native-object-order-plan\",\"formatVersion\":1,\"documentFormatVersion\":2,\"migration\":\"explicit-plan-required\",\"referencePolicy\":\"unresolved\",\"contexts\":[{{\"contextId\":\"scene-root\",\"layers\":[{}],\"scopes\":[{{\"frameScope\":{{\"kind\":\"authored\",\"frame\":0}},\"elements\":[]}}]}}]}}}}",
        texts.join(","),
        order.join(",")
    )
}

/// A bare layer (`ObjectLayer::identified`) carrying the matte members of a JSON description, the way a
/// legacy importer would hand them over before admission.
pub(crate) fn layer_from(value: &Value) -> ObjectLayer {
    let mut layer = ObjectLayer::identified(value["layerUid"].as_str().unwrap().to_string());
    if let Some(mode) = value["matteMode"].as_str() {
        layer.matte_mode = Presence::Present(mode.to_string());
    }
    if let Some(uid) = value["matteSourceLayerUid"].as_str() {
        layer.matte_source_layer_uid = Presence::Present(uid.to_string());
    }
    if let Some(extras) = value["mattesMore"].as_array() {
        layer.mattes_more = Presence::Present(
            extras
                .iter()
                .map(|extra| crate::object_document::MatteRef {
                    uid: extra["uid"].as_str().unwrap().to_string(),
                    mode: extra["mode"].as_str().unwrap().to_string(),
                })
                .collect(),
        );
    }
    layer
}

/// The `(uid, mode)` pairs of a JSON `mattesMore` value (`null` is an empty list).
pub(crate) fn pairs(value: &Value) -> Vec<(String, String)> {
    value
        .as_array()
        .map(|extras| {
            extras
                .iter()
                .map(|extra| {
                    (
                        extra["uid"].as_str().unwrap().to_string(),
                        extra["mode"].as_str().unwrap().to_string(),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}
