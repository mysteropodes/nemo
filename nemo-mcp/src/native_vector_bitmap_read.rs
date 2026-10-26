//! Immutable stored bitmap-anchor read shape; no generic writer or resource admission.
use super::{finite, record_shape};
use crate::native_contract::exact;
use serde_json::Value;

fn identity(value: &Value) -> bool {
    value
        .as_str()
        .is_some_and(|text| !text.is_empty() && text.len() <= 255 && !text.contains('\0'))
}

fn color(value: &Value) -> bool {
    value.as_str().is_some_and(|text| {
        text.strip_prefix('#').is_some_and(|hex| {
            [3, 6, 8].contains(&hex.len()) && hex.bytes().all(|byte| byte.is_ascii_hexdigit())
        })
    })
}

fn fields(value: &Value) -> bool {
    exact(
        value,
        &["bitmapBrushSpec", "brushGroupId"],
        &[
            "preTextureStroke",
            "preTextureOpacity",
            "bitmapPressureProfile",
        ],
    )
    .is_some_and(|object| {
        let spec = &value["bitmapBrushSpec"];
        exact(
            spec,
            &[
                "tip", "size", "spacing", "scatter", "opacity", "color", "pressure", "seed",
            ],
            &[],
        )
        .is_some()
            && identity(&spec["tip"])
            && identity(&value["brushGroupId"])
            && ["size", "spacing", "scatter", "opacity", "seed"]
                .iter()
                .all(|key| finite(&spec[*key]))
            && color(&spec["color"])
            && spec["pressure"].is_boolean()
            && object
                .get("preTextureStroke")
                .is_none_or(|value| value.is_null() || color(value))
            && object.get("preTextureOpacity").is_none_or(finite)
            && object.get("bitmapPressureProfile").is_none_or(|value| {
                value.as_array().is_some_and(|profile| {
                    profile.len() <= 4096
                        && profile.iter().all(|point| {
                            point
                                .as_array()
                                .is_some_and(|parts| parts.len() == 3 && parts.iter().all(finite))
                        })
                })
            })
    })
}

pub(super) fn record(value: &Value) -> bool {
    let Some(bitmap) = value
        .get("vectorExt")
        .and_then(|ext| ext.get("bitmapBrush"))
    else {
        return record_shape(value, false);
    };
    let ext = &value["vectorExt"];
    if !fields(bitmap)
        || value["target"]["contextId"] != "scene-root"
        || ext.get("linkedFillId").is_some()
        || ext["isLinkedFillCompanion"] == true
    {
        return false;
    }
    // Reuse the complete existing shape guard on a detached validation projection;
    // the immutable response retains the exact original fields, values and presence.
    let mut projected = value.clone();
    let extensions = projected["vectorExt"].as_object_mut().unwrap();
    extensions.remove("bitmapBrush");
    if extensions.is_empty() {
        projected.as_object_mut().unwrap().remove("vectorExt");
    }
    if !record_shape(&projected, false) {
        return false;
    }
    if value["fill"].is_null() {
        value["vector"]["opacity"].as_f64() == Some(0.0)
            && bitmap.get("preTextureOpacity").is_some()
    } else {
        value["vector"]["stroke"].is_null() && bitmap.get("preTextureStroke").is_some()
    }
}
