//! Closed request and result validation for descriptor-declared native operations.

use crate::contract::bounded_identifier;
use serde_json::{Map, Value};
use std::collections::HashSet;

#[path = "native_media_export_contract.rs"]
mod export;

#[path = "native_font_contract.rs"]
pub(crate) mod font;

#[path = "native_brush_resource_contract.rs"]
pub(crate) mod brush;

#[path = "native_reproduction_contract.rs"]
mod reproduction;

#[path = "native_property_metadata_contract.rs"]
pub(crate) mod metadata;
#[path = "native_object_contract.rs"]
mod object;
#[path = "native_text_read_contract.rs"]
pub(crate) mod text_read;

#[path = "native_audio_track_contract.rs"]
pub(crate) mod audio_track;
#[path = "native_composition_contract.rs"]
pub(crate) mod composition;
#[path = "native_custom_brush_contract.rs"]
pub(crate) mod custom_brush;
#[path = "native_guide_contract.rs"]
pub(crate) mod guide;
#[path = "native_palette_contract.rs"]
pub(crate) mod palette;
#[path = "native_perspective_contract.rs"]
pub(crate) mod perspective;
#[path = "native_project_placement_contract.rs"]
pub(crate) mod placement;
#[path = "native_project_settings_contract.rs"]
pub(crate) mod settings;
#[path = "native_shadow_contract.rs"]
pub(crate) mod shadow;
#[path = "native_workspace_contract.rs"]
pub(crate) mod workspace;

#[path = "native_comments_contract.rs"]
pub(crate) mod comments;

#[path = "native_diagnostics_contract.rs"]
mod diagnostics;
#[path = "native_frame_edit_contract.rs"]
pub(crate) mod frame_edit;
#[path = "native_import_contract.rs"]
pub(crate) mod import;
#[path = "native_item_contract.rs"]
pub(crate) mod item;
#[path = "native_layer_playhead_cut_contract.rs"]
pub(crate) mod layer_cut;
#[path = "native_layer_policy_contract.rs"]
pub(crate) mod layer_policy;
#[path = "native_object_component_contract.rs"]
pub(crate) mod object_component;
#[path = "native_object_edit_contract.rs"]
pub(crate) mod object_edit;
#[path = "native_object_rig_contract.rs"]
pub(crate) mod object_rig;
#[path = "native_property_edit_contract.rs"]
pub(crate) mod property_edit;
#[path = "native_property_read_contract.rs"]
pub(crate) mod property_read;
#[path = "native_property_track_contract.rs"]
pub(crate) mod property_track;
#[path = "native_vector_contract.rs"]
pub(crate) mod vector;
use diagnostics::diagnostics_result;

const OPACITY_CURVE: [[f64; 2]; 5] = [
    [0.0, 0.0],
    [0.25, 0.156],
    [0.5, 0.5],
    [0.75, 0.844],
    [1.0, 1.0],
];

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

pub(crate) fn forbids_expected_revision(operation: &str) -> bool {
    matches!(
        operation,
        "query.document.object"
            | text_read::OP
            | metadata::OP
            | property_read::OP
            | composition::READ
            | palette::READ
            | guide::READ
            | shadow::READ
            | perspective::READ
            | workspace::READ
            | custom_brush::READ
            | comments::READ
            | font::QUERY
            | brush::QUERY
            | "query.document.object.layer.timing"
            | "query.document.serialize"
            | "query.document.evaluate"
            | "query.diagnostics.recent"
            | "command.reproduction.opt_in"
            | "query.reproduction.status"
            | "query.reproduction.export"
            | "query.reproduction.report"
            | "query.reproduction.replay"
    )
}

pub(crate) fn validate_request(operation: &str, payload: &Value) -> bool {
    match operation {
        font::IMPORT | font::QUERY => font::request(operation, payload),
        brush::PUBLISH | brush::QUERY => brush::request(operation, payload),
        "query.document.object" => object::request(payload),
        text_read::OP => text_read::request(payload),
        metadata::OP => metadata::request(payload),
        property_read::OP => property_read::request(payload),
        "query.document.object.layer.timing" => object::timing_request(payload),
        "command.document.object.layer.visibility.set" => object::visibility_request(payload),
        "command.document.object.project.settings.edit" => settings::request(payload),
        composition::READ | composition::EDIT => composition::request(operation, payload),
        palette::READ | palette::EDIT => palette::request(operation, payload),
        guide::READ | guide::EDIT => guide::request(operation, payload),
        shadow::READ | shadow::EDIT => shadow::request(operation, payload),
        perspective::READ | perspective::EDIT => perspective::request(operation, payload),
        workspace::READ | workspace::EDIT => workspace::request(operation, payload),
        comments::READ | comments::EDIT => comments::request(operation, payload),
        custom_brush::READ | custom_brush::EDIT => custom_brush::request(operation, payload),
        placement::OP => placement::request(payload),
        audio_track::OP => audio_track::request(payload),
        layer_policy::OP => layer_policy::request(payload),
        layer_cut::OP => layer_cut::request(payload),
        property_track::OP => property_track::request(payload),
        property_edit::OP => property_edit::request(payload),
        op if frame_edit::declares(op) => frame_edit::request(op, payload),
        op if object_component::admitted(op) => object_component::request(op, payload),
        op if object_rig::admitted(op) => object_rig::request(op, payload),
        op if object_edit::admitted(op) => object_edit::request(op, payload),
        item::ITEM | item::ATTACH | item::REMOVE => item::request(operation, payload),
        vector::VECTOR | vector::PARAMETRIC | vector::EFFECTS => {
            vector::request(operation, payload)
        }
        "command.document.object.layer.create" => object::create_request(payload),
        import::OP => import::request(payload),
        operation if export::owns(operation) => export::request(operation, payload),
        "query.diagnostics.recent" => exact(payload, &[], &[]).is_some(),
        "command.reproduction.opt_in"
        | "query.reproduction.status"
        | "query.reproduction.export" => exact(payload, &[], &[]).is_some(),
        "query.reproduction.replay" => exact(payload, &["bundle"], &[])
            .is_some_and(|value| value.get("bundle").is_some_and(reproduction::bundle)),
        "query.reproduction.report" => reproduction::report_request(payload),
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

fn opacity_curve(value: Option<&Value>) -> bool {
    value.and_then(Value::as_array).is_some_and(|points| {
        points.len() == OPACITY_CURVE.len()
            && points.iter().zip(OPACITY_CURVE).all(|(point, expected)| {
                exact(point, &["x", "y"], &[]).is_some_and(|point| {
                    point.get("x").and_then(Value::as_f64) == Some(expected[0])
                        && point.get("y").and_then(Value::as_f64) == Some(expected[1])
                })
            })
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
                    || !opacity_curve(key.get("curvePoints"))
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
    if matches!(operation, font::IMPORT | font::QUERY) {
        return font::result(operation, payload, result);
    }
    if operation == property_read::OP {
        return property_read::result(request_document_id, payload, result);
    }
    if matches!(operation, composition::READ | composition::EDIT) {
        return composition::result(operation, request_document_id, payload, result);
    }
    if matches!(operation, palette::READ | palette::EDIT) {
        return palette::result(operation, request_document_id, payload, result);
    }
    if matches!(operation, comments::READ | comments::EDIT) {
        return comments::result(operation, request_document_id, payload, result);
    }
    if matches!(operation, custom_brush::READ | custom_brush::EDIT) {
        return custom_brush::result(operation, request_document_id, payload, result);
    }
    if matches!(operation, workspace::READ | workspace::EDIT) {
        return workspace::result(operation, request_document_id, payload, result);
    }
    if matches!(operation, perspective::READ | perspective::EDIT) {
        return perspective::result(operation, request_document_id, payload, result);
    }
    if matches!(operation, shadow::READ | shadow::EDIT) {
        return shadow::result(operation, request_document_id, payload, result);
    }
    if matches!(operation, guide::READ | guide::EDIT) {
        return guide::result(operation, request_document_id, payload, result);
    }
    if brush::owns(operation) {
        return brush::result(operation, payload, result);
    }
    if operation == text_read::OP {
        return text_read::result(request_document_id, payload, result);
    }
    if operation == metadata::OP {
        return metadata::result(request_document_id, payload, result);
    }
    if operation == "query.document.object" {
        return object::result(request_document_id, payload, result);
    }
    if operation == "query.document.object.layer.timing" {
        return object::timing_result(request_document_id, payload, result);
    }
    if operation == property_edit::OP {
        return property_edit::result(result);
    }
    if operation == property_track::OP {
        return property_track::result(result);
    }
    if operation == layer_policy::OP {
        return layer_policy::result(result);
    }
    if operation == layer_cut::OP {
        return layer_cut::result(result);
    }
    if frame_edit::declares(operation) {
        return frame_edit::result_for_operation(operation, result);
    }
    if object_component::admitted(operation) {
        return object_component::result(operation, result);
    }
    if object_rig::admitted(operation) {
        return object_rig::result(operation, result);
    }
    if object_edit::admitted(operation) {
        return object_edit::result(operation, result);
    }
    if item::owns(operation) {
        return item::result(operation, payload, result);
    }
    if vector::owns(operation) {
        return vector::result(operation, payload, result);
    }
    if operation == placement::OP {
        return placement::result(result);
    }
    if operation == audio_track::OP {
        return audio_track::result(result);
    }
    if operation == "command.document.object.project.settings.edit" {
        return settings::result(result);
    }
    if operation == "command.document.object.layer.create" {
        return object::create_result(result);
    }
    if operation == import::OP {
        return import::result(result);
    }
    if operation == "command.document.object.layer.visibility.set" {
        return object::visibility_result(result);
    }
    if operation == "query.diagnostics.recent" {
        return diagnostics_result(result);
    }
    if operation.starts_with("query.reproduction.") || operation == "command.reproduction.opt_in" {
        if operation == "query.reproduction.report" {
            return reproduction::report_result(payload, result);
        }
        return reproduction::validate_result(operation, result);
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
#[path = "native_contract_tests.rs"]
mod tests;
