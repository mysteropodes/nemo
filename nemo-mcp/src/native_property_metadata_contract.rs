//! D35 common shape validation; no property catalog/default table or writer.
use super::{exact, revision};
use serde_json::Value;
pub(crate) const OP: &str = "query.document.object.property.metadata";
fn text(v: Option<&Value>) -> bool {
    v.and_then(Value::as_str).is_some_and(|s| !s.is_empty())
}
fn target(v: &Value) -> bool {
    exact(v, &["contextId", "layerUid"], &["strokeId"]).is_some_and(|m| {
        text(m.get("contextId"))
            && text(m.get("layerUid"))
            && m.get("strokeId").is_none_or(|v| text(Some(v)))
    })
}
fn uint(v: Option<&Value>) -> Option<u64> {
    v?.as_u64().filter(|n| *n <= u32::MAX as u64)
}
pub(super) fn request(v: &Value) -> bool {
    exact(v, &["atRevision", "documentSnapshotId", "view"], &[]).is_some_and(|m| {
        revision(m.get("atRevision")).is_some()
            && text(m.get("documentSnapshotId"))
            && m.get("view").is_some_and(|view| {
                let Some(kind) = view.get("kind").and_then(Value::as_str) else {
                    return false;
                };
                if kind == "storedProperty" {
                    return exact(view, &["kind", "target", "property"], &[]).is_some_and(|x| {
                        x.get("target").is_some_and(target) && text(x.get("property"))
                    });
                }
                let required = if kind == "targets" {
                    vec!["kind", "offset", "limit"]
                } else if kind == "properties" {
                    vec!["kind", "target", "offset", "limit"]
                } else {
                    return false;
                };
                exact(view, &required, &[]).is_some_and(|x| {
                    uint(x.get("offset")).is_some()
                        && x.get("limit")
                            .and_then(Value::as_u64)
                            .is_some_and(|n| (1..=8).contains(&n))
                        && (kind == "targets" || x.get("target").is_some_and(target))
                })
            })
    })
}
fn target_item(v: &Value) -> bool {
    match v.get("kind").and_then(Value::as_str) {
        Some("context") => {
            exact(v, &["kind", "contextId"], &[]).is_some() && text(v.get("contextId"))
        }
        Some(kind @ ("layer" | "element")) => {
            exact(v, &["kind", "target"], &[]).is_some()
                && v.get("target").is_some_and(|t| {
                    target(t) && t.get("strokeId").is_some() == (kind == "element")
                })
        }
        _ => false,
    }
}
fn descriptor(v: &Value, stored: bool) -> bool {
    if exact(
        v,
        &[
            "property",
            "dimension",
            "unit",
            "defaultValue",
            "animatable",
            "controlType",
            "controlName",
            "authored",
            "availability",
        ],
        &[],
    )
    .is_none()
    {
        return false;
    }
    let Some(d) = v
        .get("dimension")
        .and_then(Value::as_u64)
        .filter(|d| (1..=4).contains(d))
    else {
        return false;
    };
    let finite = |v: &Value| v.as_f64().is_some_and(f64::is_finite);
    if !text(v.get("property"))
        || !v.get("unit").is_some_and(Value::is_string)
        || !v.get("animatable").is_some_and(Value::is_boolean)
        || !v.get("defaultValue").is_some_and(|x| {
            x.is_null()
                || x.as_array()
                    .is_some_and(|a| a.len() == d as usize && a.iter().all(finite))
        })
        || !["controlName", "controlType"]
            .iter()
            .all(|k| v.get(*k).is_some_and(|x| x.is_null() || x.is_string()))
    {
        return false;
    }
    let a = &v["authored"];
    let authored = match a.get("availability").and_then(Value::as_str) {
        Some("available") => {
            exact(
                a,
                &[
                    "availability",
                    "keyCount",
                    "hasStaticOverride",
                    "expressionEnabled",
                ],
                &[],
            )
            .is_some()
                && uint(a.get("keyCount")).is_some()
                && a["hasStaticOverride"].is_boolean()
                && a["expressionEnabled"].is_boolean()
                && !matches!(v["property"].as_str(), Some("order" | "timeRemap"))
        }
        Some("unavailable") => {
            exact(a, &["availability", "reason"], &[]).is_some()
                && a["reason"] == "separate-property-owner"
                && matches!(v["property"].as_str(), Some("order" | "timeRemap"))
        }
        _ => false,
    };
    let av = &v["availability"];
    authored
        && exact(
            av,
            &["metadata", "applicability", "propertyWrite", "reason"],
            &[],
        )
        .is_some()
        && av["metadata"] == "available"
        && av["propertyWrite"] == "unavailable"
        && av["applicability"] == if stored { "unavailable" } else { "available" }
        && av["reason"]
            == if stored {
                "stored-metadata-does-not-prove-applicability"
            } else {
                "read-only-description-not-command-availability"
            }
}
pub(super) fn result(document: &str, payload: &Value, v: &Value) -> bool {
    if !request(payload)
        || exact(
            v,
            &[
                "instanceId",
                "documentId",
                "atRevision",
                "documentSnapshotId",
                "kind",
                "target",
                "items",
                "nextOffset",
                "unavailableCategories",
            ],
            &[],
        )
        .is_none()
    {
        return false;
    }
    if !v
        .get("instanceId")
        .and_then(Value::as_str)
        .is_some_and(crate::contract::bounded_identifier)
        || v.get("documentId").and_then(Value::as_str) != Some(document)
        || ["atRevision", "documentSnapshotId"]
            .iter()
            .any(|k| v.get(*k) != payload.get(*k))
    {
        return false;
    }
    let view = &payload["view"];
    let kind = view["kind"].as_str().unwrap();
    if v["kind"] != kind {
        return false;
    }
    if kind == "targets" {
        if !v["target"].is_null() {
            return false;
        }
    } else if v["target"] != view["target"] {
        return false;
    }
    let Some(items) = v["items"].as_array() else {
        return false;
    };
    if kind == "storedProperty" {
        if items.len() != 1
            || !v["nextOffset"].is_null()
            || items[0]["property"] != view["property"]
            || !descriptor(&items[0], true)
        {
            return false;
        }
    } else {
        if items.len() > view["limit"].as_u64().unwrap() as usize
            || !items.iter().all(|x| {
                if kind == "targets" {
                    target_item(x)
                } else {
                    descriptor(x, false)
                }
            })
        {
            return false;
        }
        if !v["nextOffset"].is_null()
            && (items.is_empty()
                || uint(v.get("nextOffset"))
                    != uint(view.get("offset"))
                        .and_then(|n| n.checked_add(items.len() as u64))
                        .filter(|n| *n <= u32::MAX as u64))
        {
            return false;
        }
    }
    let Some(cats) = v["unavailableCategories"].as_array() else {
        return false;
    };
    let names = [
        "conditionalMotionRows",
        "pathVerticesTrimStrokeBrushMesh",
        "referenceOnlyElements",
        "expressionDiagnostics",
        "layerEffectMatteReveal",
        "propertyWrites",
    ];
    if cats.len() != names.len()
        || !names
            .iter()
            .all(|name| cats.iter().filter(|x| x["category"] == *name).count() == 1)
        || !cats.iter().all(|x| {
            exact(x, &["category", "state", "reason"], &[]).is_some()
                && x["state"] == "unavailable"
                && text(x.get("reason"))
        })
    {
        return false;
    }
    // This inner bound never replaces the complete shared-envelope 4096 check.
    serde_json::to_vec(v).is_ok_and(|b| b.len() <= 3072)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn payload() -> Value {
        json!({"atRevision":0,"documentSnapshotId":"native-object:doc:0",
        "view":{"kind":"targets","offset":0,"limit":2}})
    }
    fn output() -> Value {
        json!({"instanceId":"instance","documentId":"doc","atRevision":0,
        "documentSnapshotId":"native-object:doc:0","kind":"targets",
        "target":null,"items":[{"kind":"context","contextId":"scene-root"}],"nextOffset":1,
        "unavailableCategories":[
            {"category":"conditionalMotionRows","state":"unavailable","reason":"not-admitted"},
            {"category":"pathVerticesTrimStrokeBrushMesh","state":"unavailable","reason":"not-admitted"},
            {"category":"referenceOnlyElements","state":"unavailable","reason":"not-admitted"},
            {"category":"expressionDiagnostics","state":"unavailable","reason":"not-admitted"},
            {"category":"layerEffectMatteReveal","state":"unavailable","reason":"not-admitted"},
            {"category":"propertyWrites","state":"unavailable","reason":"read-only"}]})
    }
    #[test]
    fn pages_refuse_unknown_views_members_positional_input_and_nonadvancing_cursors() {
        let p = payload();
        assert!(request(&p));
        assert!(result("doc", &p, &output()));
        for bad in [
            json!([]),
            json!({"kind":"targets","offset":0,"limit":0}),
            json!({"kind":"targets","offset":0,"limit":9}),
            json!({"kind":"targets","offset":0,"limit":1,"values":[]}),
        ] {
            let mut q = p.clone();
            q["view"] = bad;
            assert!(!request(&q));
        }
        for (k, x) in [
            ("unexpected", json!(2)),
            ("nextOffset", json!(0)),
            ("atRevision", json!(1)),
            ("documentSnapshotId", json!("other")),
            ("target", json!({})),
        ] {
            let mut out = output();
            out[k] = x;
            assert!(!result("doc", &p, &out), "{k}");
        }
    }
    #[test]
    fn common_capability_admits_metadata_reads_and_refuses_invalid_payloads() {
        let request: crate::contract::NativeApplicationRequest = serde_json::from_value(json!({
            "apiVersion":2,"requestId":"metadata-1","instanceId":"instance","documentId":"doc",
            "operation":OP,"payload":payload()}))
        .unwrap();
        assert!(request.validate().is_ok());
        let mut extra = request.clone();
        extra.payload["unexpected"] = json!(true);
        assert!(extra.validate().is_err());
        let mut bad_page = request.clone();
        bad_page.payload["view"]["limit"] = json!(0);
        assert!(bad_page.validate().is_err());
        let mut write_pin = request;
        write_pin.expected_revision = Some(0);
        assert!(write_pin.validate().is_err());
    }
}
