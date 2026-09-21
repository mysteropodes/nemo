//! Closed request and result validation for descriptor-declared native reads.

use crate::contract::bounded_identifier;
use serde_json::{json, Map, Value};
use std::collections::HashSet;

pub(crate) fn present_revision<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<u64>, D::Error> {
    serde::Deserialize::deserialize(deserializer).map(Some)
}

fn exact<'a>(
    value: &'a Value,
    required: &[&str],
    optional: &[&str],
) -> Option<&'a Map<String, Value>> {
    let object = value.as_object()?;
    let allowed = required
        .iter()
        .chain(optional)
        .copied()
        .collect::<HashSet<_>>();
    (required.iter().all(|key| object.contains_key(*key))
        && object.keys().all(|key| allowed.contains(key.as_str())))
    .then_some(object)
}

fn revision(value: Option<&Value>) -> Option<u64> {
    value?
        .as_u64()
        .filter(|revision| *revision <= 9_007_199_254_740_991)
}

fn frame(value: Option<&Value>) -> Option<u64> {
    value?.as_u64().filter(|frame| *frame <= u32::MAX as u64)
}

pub(crate) fn validate_request(operation: &str, payload: &Value) -> bool {
    match operation {
        "query.document.serialize" => exact(payload, &["atRevision"], &[])
            .is_some_and(|object| revision(object.get("atRevision")).is_some()),
        "query.document.evaluate" => exact(payload, &["atRevision", "contextId", "frame"], &[])
            .is_some_and(|object| {
                revision(object.get("atRevision")).is_some()
                    && object
                        .get("contextId")
                        .and_then(Value::as_str)
                        .is_some_and(bounded_identifier)
                    && frame(object.get("frame")).is_some()
            }),
        _ => true,
    }
}

fn opacity_vector(value: Option<&Value>) -> bool {
    value.and_then(Value::as_array).is_some_and(|values| {
        values.len() == 1
            && values[0]
                .as_f64()
                .is_some_and(|opacity| (0.0..=100.0).contains(&opacity))
    })
}

fn zero_handle(value: Option<&Value>) -> bool {
    value.and_then(Value::as_array).is_some_and(|parts| {
        parts.len() == 2 && parts.iter().all(|part| part.as_f64() == Some(0.0))
    })
}

fn opacity_document(value: &Value) -> bool {
    let Some(document) = exact(
        value,
        &["format", "formatVersion", "totalFrames", "layers"],
        &[],
    ) else {
        return false;
    };
    let Some(total_frames) = frame(document.get("totalFrames")).filter(|frames| *frames > 0) else {
        return false;
    };
    if document.get("format").and_then(Value::as_str) != Some("nemo.native-opacity-document")
        || document.get("formatVersion").and_then(Value::as_u64) != Some(1)
    {
        return false;
    }
    let Some(layers) = document.get("layers").and_then(Value::as_array) else {
        return false;
    };
    if layers.is_empty() {
        return false;
    }
    let mut layer_ids = HashSet::with_capacity(layers.len());
    for layer in layers {
        let Some(layer) = exact(layer, &["layerUid"], &["motionStatic", "motion"]) else {
            return false;
        };
        let Some(layer_uid) = layer.get("layerUid").and_then(Value::as_str) else {
            return false;
        };
        if !bounded_identifier(layer_uid)
            || !layer_ids.insert(layer_uid)
            || (!layer.contains_key("motionStatic") && !layer.contains_key("motion"))
        {
            return false;
        }
        if let Some(motion) = layer.get("motionStatic") {
            let Some(motion) = exact(motion, &["opacity"], &[]) else {
                return false;
            };
            if !opacity_vector(motion.get("opacity")) {
                return false;
            }
        }
        if let Some(motion) = layer.get("motion") {
            let Some(motion) = exact(motion, &["opacity"], &[])
                .and_then(|motion| exact(motion.get("opacity")?, &["keys"], &[]))
            else {
                return false;
            };
            let Some(keys) = motion.get("keys").and_then(Value::as_array) else {
                return false;
            };
            if keys.is_empty() {
                return false;
            }
            let mut prior = None;
            for key in keys {
                let Some(key) = exact(key, &["frame", "v", "curvePoints", "hOut", "hIn"], &[])
                else {
                    return false;
                };
                let Some(frame) = frame(key.get("frame")).filter(|frame| *frame < total_frames)
                else {
                    return false;
                };
                if prior.is_some_and(|prior| frame <= prior)
                    || !opacity_vector(key.get("v"))
                    || key.get("curvePoints")
                        != Some(&json!([
                            {"x":0,"y":0}, {"x":0.25,"y":0.156}, {"x":0.5,"y":0.5},
                            {"x":0.75,"y":0.844}, {"x":1,"y":1}
                        ]))
                    || !zero_handle(key.get("hOut"))
                    || !zero_handle(key.get("hIn"))
                {
                    return false;
                }
                prior = Some(frame);
            }
        }
    }
    true
}

pub(crate) fn validate_result(
    operation: &str,
    request_document_id: &str,
    payload: &Value,
    result: &Value,
) -> bool {
    if !validate_request(operation, payload) {
        return false;
    }
    if !matches!(
        operation,
        "query.document.serialize" | "query.document.evaluate"
    ) {
        return true;
    }
    let selector = payload.as_object().expect("validated native read payload");
    let at_revision = revision(selector.get("atRevision")).expect("validated revision");
    match operation {
        "query.document.serialize" => exact(
            result,
            &["atRevision", "documentSnapshotId", "document"],
            &[],
        )
        .is_some_and(|result| {
            revision(result.get("atRevision")) == Some(at_revision)
                && result
                    .get("documentSnapshotId")
                    .and_then(Value::as_str)
                    .is_some_and(bounded_identifier)
                && result.get("document").is_some_and(opacity_document)
        }),
        "query.document.evaluate" => exact(
            result,
            &[
                "documentSnapshotId",
                "documentId",
                "contentRevision",
                "contextId",
                "frame",
                "layers",
            ],
            &[],
        )
        .is_some_and(|result| {
            let mut ids = HashSet::new();
            result
                .get("documentSnapshotId")
                .and_then(Value::as_str)
                .is_some_and(bounded_identifier)
                && result.get("documentId").and_then(Value::as_str) == Some(request_document_id)
                && revision(result.get("contentRevision")) == Some(at_revision)
                && result.get("contextId") == selector.get("contextId")
                && result.get("frame") == selector.get("frame")
                && result
                    .get("layers")
                    .and_then(Value::as_array)
                    .is_some_and(|layers| {
                        layers.iter().all(|layer| {
                            exact(layer, &["layerUid", "value"], &[]).is_some_and(|layer| {
                                layer
                                    .get("layerUid")
                                    .and_then(Value::as_str)
                                    .is_some_and(|id| bounded_identifier(id) && ids.insert(id))
                                    && layer
                                        .get("value")
                                        .and_then(Value::as_f64)
                                        .is_some_and(|opacity| (0.0..=100.0).contains(&opacity))
                            })
                        })
                    })
        }),
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn serialized(document: Value) -> Value {
        json!({
            "atRevision": 0,
            "documentSnapshotId": "native-opacity:native-document-1:0",
            "document": document
        })
    }

    fn document() -> Value {
        json!({
            "format": "nemo.native-opacity-document", "formatVersion": 1, "totalFrames": 21,
            "layers": [{"layerUid": "layer-a", "motionStatic": {"opacity": [25]}}]
        })
    }

    #[test]
    fn serialized_document_contract_rejects_open_legacy_duplicate_and_malformed_shapes() {
        let payload = json!({"atRevision": 0});
        let valid = document();
        assert!(validate_result(
            "query.document.serialize",
            "native-document-1",
            &payload,
            &serialized(valid.clone())
        ));
        let mut open = valid.clone();
        open["legacyVersion"] = json!(13);
        let mut duplicate = valid.clone();
        let duplicate_layer = duplicate["layers"][0].clone();
        duplicate["layers"]
            .as_array_mut()
            .unwrap()
            .push(duplicate_layer);
        let mut malformed_track = valid;
        malformed_track["layers"][0] = json!({
            "layerUid": "layer-a", "motion": {"opacity": {"keys": [{
                "frame": 0, "v": [25], "curvePoints": [], "hOut": [0, 0], "hIn": [0, 0]
            }]}}
        });
        for rejected in [
            json!({}),
            json!({"version": 13, "layers": []}),
            open,
            duplicate,
            malformed_track,
        ] {
            assert!(!validate_result(
                "query.document.serialize",
                "native-document-1",
                &payload,
                &serialized(rejected)
            ));
        }
    }
}
