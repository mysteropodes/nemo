//! Closed placement DTO and receipt validation; the native placement/history owner is authoritative.
use super::exact;
use serde_json::Value;

pub(crate) const OP: &str = "command.document.object.project.placement.edit";
const MAX_OFFSET: u64 = 9_007_199_254_740_991;
fn source_id(value: Option<&Value>) -> bool {
    value
        .and_then(Value::as_str)
        .is_some_and(|s| !s.trim().is_empty() && s.len() <= 255 && !s.contains('\0'))
}
fn offset(value: Option<&Value>) -> bool {
    value
        .and_then(Value::as_f64)
        .is_some_and(|v| v.is_finite() && v.abs() <= MAX_OFFSET as f64)
}
fn target(value: &Value) -> bool {
    match value.get("kind").and_then(Value::as_str) {
        Some("drawing") => exact(value, &["kind", "contextId", "layerUid"], &["strokeId"])
            .is_some_and(|v| {
                source_id(v.get("contextId"))
                    && source_id(v.get("layerUid"))
                    && (!v.contains_key("strokeId") || source_id(v.get("strokeId")))
            }),
        // This shape is declared so the actual owner can return typed unavailable.
        Some("audioTrack") => {
            exact(value, &["kind", "trackUid"], &[]).is_some_and(|v| source_id(v.get("trackUid")))
        }
        _ => false,
    }
}
fn finite(value: Option<&Value>) -> bool {
    value.and_then(Value::as_f64).is_some_and(f64::is_finite)
}
fn curve_points(value: &Value) -> bool {
    value.as_array().is_some_and(|points| {
        points.iter().all(|p| {
            exact(p, &["x", "y"], &["tx", "ty"]).is_some_and(|p| {
                ["x", "y", "tx", "ty"]
                    .iter()
                    .all(|k| p.get(*k).is_none() || finite(p.get(*k)))
            })
        })
    })
}
/// One key (`frame`) or inclusive runs of frames (`ranges`), never both.
fn remap_selection(value: &Value) -> bool {
    exact(value, &[], &["frame", "ranges"]).is_some_and(|s| {
        s.contains_key("frame") != s.contains_key("ranges")
            && (!s.contains_key("frame") || finite(s.get("frame")))
            && (!s.contains_key("ranges")
                || s["ranges"].as_array().is_some_and(|runs| {
                    (1..=512).contains(&runs.len())
                        && runs.iter().all(|r| {
                            r.as_array().is_some_and(|pair| {
                                pair.len() == 2
                                    && finite(pair.first())
                                    && finite(pair.get(1))
                                    && pair[0].as_f64() <= pair[1].as_f64()
                            })
                        })
                }))
    })
}
pub(super) fn request(value: &Value) -> bool {
    match value.get("kind").and_then(Value::as_str) {
        Some("place") => exact(value, &["kind", "placement"], &[]).is_some_and(|v| {
            exact(
                &v["placement"],
                &["id", "assetId", "target", "offsetFrames"],
                &[],
            )
            .is_some_and(|p| {
                source_id(p.get("id"))
                    && source_id(p.get("assetId"))
                    && target(&p["target"])
                    && offset(p.get("offsetFrames"))
                    && (p["target"]["kind"] == "audioTrack" || p["offsetFrames"].as_i64().is_some())
            })
        }),
        Some("setOffset") => exact(value, &["kind", "placementId", "offsetFrames"], &[])
            .is_some_and(|v| source_id(v.get("placementId")) && offset(v.get("offsetFrames"))),
        Some("shiftOffset") => exact(value, &["kind", "placementId", "deltaFrames"], &[])
            .is_some_and(|v| source_id(v.get("placementId")) && offset(v.get("deltaFrames"))),
        Some("remove" | "disableTimeRemap") => exact(value, &["kind", "placementId"], &[])
            .is_some_and(|v| source_id(v.get("placementId"))),
        // T12 video timing: bounded remap intents; the native owner admits the values.
        Some("enableTimeRemap") => exact(
            value,
            &[
                "kind",
                "placementId",
                "inFrame",
                "outFrame",
                "lastSourceFrame",
            ],
            &[],
        )
        .is_some_and(|v| {
            source_id(v.get("placementId"))
                && ["inFrame", "outFrame", "lastSourceFrame"]
                    .iter()
                    .all(|k| finite(v.get(*k)))
        }),
        Some("removeTimeRemapKey") => exact(value, &["kind", "placementId", "frame"], &[])
            .is_some_and(|v| source_id(v.get("placementId")) && finite(v.get("frame"))),
        Some("setTimeRemapKey") => exact(
            value,
            &["kind", "placementId", "frame", "value"],
            &["curvePoints"],
        )
        .is_some_and(|v| {
            source_id(v.get("placementId"))
                && finite(v.get("frame"))
                && finite(v.get("value"))
                && v.get("curvePoints").is_none_or(curve_points)
        }),
        Some("setInterpMode") => exact(value, &["kind", "placementId", "interpMode"], &[])
            .is_some_and(|v| {
                source_id(v.get("placementId"))
                    && v["interpMode"]
                        .as_str()
                        .is_some_and(|m| !m.is_empty() && m.len() <= 32 && !m.contains('\0'))
            }),
        // The selected Time Remap keys under one property key action (shift, delete, tint, curve...).
        Some("editTimeRemapKeys") => {
            exact(value, &["kind", "placementId", "selections", "action"], &[]).is_some_and(|v| {
                source_id(v.get("placementId"))
                    && v["selections"].as_array().is_some_and(|list| {
                        (1..=128).contains(&list.len()) && list.iter().all(remap_selection)
                    })
                    && crate::native_contract::property_track::action(&v["action"])
            })
        }
        _ => false,
    }
}
pub(super) fn result(value: &Value) -> bool {
    super::object::visibility_result(value)
}
pub(crate) fn revision_matches(expected: Option<u64>, revision: u64, value: &Value) -> bool {
    result(value)
        && expected.and_then(|before| before.checked_add(u64::from(value["applied"] == true)))
            == Some(revision)
}

#[cfg(test)]
#[path = "native_project_placement_contract_tests.rs"]
mod tests;
