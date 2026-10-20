//! Closed animator DTO validation; document writes and glyph evaluation stay native.
use crate::native_contract::{exact, property_read};
use serde_json::Value;
use std::collections::HashSet;
const KEYS: [&str; 11] = [
    "start",
    "end",
    "offset",
    "amount",
    "easeHigh",
    "easeLow",
    "smooth",
    "wiggleMin",
    "wiggleMax",
    "wigglesPerSec",
    "correlation",
];
fn number(v: &Value) -> bool {
    v.as_f64().is_some_and(f64::is_finite)
}
fn nullable(v: &Value) -> bool {
    v.is_null() || number(v)
}
fn opaque(v: &Value) -> bool {
    v.as_str()
        .is_some_and(|s| !s.trim().is_empty() && s.len() <= 255 && !s.contains('\0'))
}
pub(super) fn record(v: &Value) -> bool {
    let Some(fields) = exact(v, &["id", "selector"], &["enabled", "props", "properties"]) else {
        return false;
    };
    if !opaque(&v["id"]) || fields.get("enabled").is_some_and(|v| !v.is_boolean()) {
        return false;
    }
    let Some(selector) = v["selector"].as_object() else {
        return false;
    };
    if !selector.iter().all(|(k, v)| match k.as_str() {
        "selectorType" => matches!(v.as_str(), Some("range" | "wiggly")),
        "units" => matches!(v.as_str(), Some("percent" | "index")),
        "basedOn" => matches!(v.as_str(), Some("chars" | "words" | "lines")),
        key if KEYS.contains(&key) || matches!(key, "shape" | "seed") => nullable(v),
        _ => false,
    }) {
        return false;
    }
    if let Some(props) = fields.get("props") {
        let Some(props) = props.as_object() else {
            return false;
        };
        if !props.iter().all(|(k, v)| match k.as_str() {
            "rotation" | "opacity" => nullable(v),
            "position" | "scale" => v
                .as_array()
                .is_some_and(|v| v.len() == 2 && v.iter().all(nullable)),
            "fillColor" | "strokeColor" => v.as_array().is_some_and(|v| {
                (v.len() == 3 || v.len() == 4)
                    && v[..3].iter().all(number)
                    && v.get(3).is_none_or(nullable)
            }),
            _ => false,
        }) {
            return false;
        }
    }
    if let Some(records) = fields.get("properties") {
        let Some(records) = records.as_array() else {
            return false;
        };
        let mut names = HashSet::new();
        if records.len() > 11 {
            return false;
        }
        for record in records {
            let Some(p) = exact(record, &["property"], &["static", "track", "expression"]) else {
                return false;
            };
            let Some(name) = p["property"].as_str() else {
                return false;
            };
            if !KEYS.contains(&name) || !names.insert(name) {
                return false;
            }
            if p.get("static").is_some_and(|v| {
                !v.as_array()
                    .is_some_and(|v| v.len() == 1 && v.iter().all(number))
            }) {
                return false;
            }
            if let Some(e) = p.get("expression") {
                if exact(e, &["enabled", "code"], &[]).is_none()
                    || !e["enabled"].is_boolean()
                    || !e["code"].as_str().is_some_and(|s| s.len() <= 4096)
                {
                    return false;
                }
            }
            if let Some(t) = p.get("track") {
                if exact(t, &["keys"], &[]).is_none() {
                    return false;
                }
                let Some(keys) = t["keys"].as_array() else {
                    return false;
                };
                if !keys.iter().all(|k| property_read::key(k, 1))
                    || !keys.windows(2).all(|pair| {
                        pair[0]["frame"].as_f64().unwrap() < pair[1]["frame"].as_f64().unwrap()
                    })
                {
                    return false;
                }
            }
        }
    }
    true
}

pub(super) fn edit(v: &Value) -> bool {
    let kind = v["kind"].as_str().unwrap_or("");
    if kind == "setEnabled" {
        return exact(v, &["kind", "enabled"], &[]).is_some() && v["enabled"].is_boolean();
    }
    if kind == "setSelector" {
        return exact(v, &["kind", "field", "value"], &[]).is_some()
            && v["field"].as_str().is_some_and(|field| {
                record(&serde_json::json!({"id":"validation","selector":{field:v["value"]}}))
            });
    }
    if kind == "setProps" {
        return exact(v,&["kind","field","value"],&[]).is_some()&&v["field"].as_str().is_some_and(|field|["position","scale","rotation","opacity","fillColor","strokeColor"].contains(&field)&&((v["value"].is_null()&&matches!(field,"fillColor"|"strokeColor"))||record(&serde_json::json!({"id":"validation","selector":{},"props":{field:v["value"]}}))));
    }
    let Some(property) = v["property"].as_str() else {
        return false;
    };
    if !KEYS.contains(&property) || !number(&v["frame"]) {
        return false;
    }
    let extra = match kind {
        "setValue" | "setKey" => "values",
        "setAnimated" => "animated",
        "removeKey" => "",
        _ => return false,
    };
    let mut required = vec!["kind", "property", "frame"];
    if !extra.is_empty() {
        required.push(extra);
    }
    if exact(
        v,
        &required,
        if kind == "setKey" {
            &["curvePoints"]
        } else {
            &[]
        },
    )
    .is_none()
    {
        return false;
    }
    if kind == "setAnimated" {
        return v["animated"].is_boolean();
    }
    if kind == "removeKey" {
        return true;
    }
    let mut key = serde_json::json!({"frame":v["frame"],"v":v["values"]});
    if let Some(curve) = v.get("curvePoints") {
        key["curvePoints"] = curve.clone();
    }
    property_read::key(&key, 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn text_animator_real_item_contract_accepts_original_nullable_props_and_keyed_selector() {
        let a = json!({"id":"ta","selector":{"amount":null},"props":{"scale":[0,0]},"properties":[{"property":"amount","track":{"keys":[{"frame":0,"v":[0]},{"frame":4,"v":[100]}]}}]});
        assert!(record(&a));
        let p = json!({"command":"object.text.animator.upsert","layerUid":"L0","animator":a});
        assert!(crate::native_contract::item::request(
            crate::native_contract::item::ITEM,
            &p
        ));
        assert!(crate::native_contract::item::result(
            crate::native_contract::item::ITEM,
            &p,
            &json!({"applied":true,"historyEntriesAdded":1,"operation":"text.animator.upsert","strokeId":"ta"})
        ));
        assert!(!crate::native_contract::item::result(
            crate::native_contract::item::ITEM,
            &p,
            &json!({"applied":true,"historyEntriesAdded":1,"operation":"text.animator.upsert","strokeId":"other"})
        ));
        for bad in [
            json!({"id":"ta","selector":{},"props":{"position":[1]}}),
            json!({"id":"ta","selector":{},"properties":[{"property":"amount","static":[null]}]}),
            json!({"id":"ta","selector":{"foreign":2}}),
        ] {
            assert!(!record(&bad));
        }
    }
    #[test]
    fn text_animator_real_pinned_read_contract_accepts_numeric_frame_and_correlates_chunk() {
        let p = json!({"atRevision":0,"documentSnapshotId":"snapshot-0","view":{"kind":"animators","layerUid":"L0","frame":0,"offset":0,"limit":1024}});
        assert!(crate::native_contract::text_read::request(&p));
        let text = r#"{"animators":[],"selectors":[],"hasText":false}"#;
        let mut r = json!({"instanceId":"instance","documentId":"document","atRevision":0,"documentSnapshotId":"snapshot-0","kind":"animators","layerUid":"L0","frame":0.0,"offset":0,"textBytes":text.len(),"text":text,"nextOffset":null});
        assert!(crate::native_contract::text_read::result(
            "document", &p, &r
        ));
        r["frame"] = json!(1);
        assert!(!crate::native_contract::text_read::result(
            "document", &p, &r
        ));
        r["frame"] = json!(0.0);
        r["layerUid"] = json!("other");
        assert!(!crate::native_contract::text_read::result(
            "document", &p, &r
        ));
    }
    #[test]
    fn text_animator_compact_edit_contract_preserves_closed_native_action_shapes() {
        for action in [
            json!({"kind":"setEnabled","enabled":false}),
            json!({"kind":"setProps","field":"scale","value":[0,0]}),
            json!({"kind":"setSelector","field":"basedOn","value":"words"}),
            json!({"kind":"setValue","property":"amount","frame":2,"values":[75]}),
            json!({"kind":"setKey","property":"amount","frame":2,"values":[75],"curvePoints":[{"x":0,"y":0},{"x":1,"y":1}]}),
            json!({"kind":"setAnimated","property":"amount","frame":2,"animated":false}),
        ] {
            assert!(edit(&action), "{action}");
            let p = json!({"command":"object.text.animator.edit","layerUid":"L0","animatorId":"long","edit":action});
            assert!(crate::native_contract::item::request(
                crate::native_contract::item::ITEM,
                &p
            ));
            assert!(crate::native_contract::item::result(
                crate::native_contract::item::ITEM,
                &p,
                &json!({"applied":true,"historyEntriesAdded":1,"operation":"text.animator.edit","strokeId":"long"})
            ));
        }
        for action in [
            json!({"kind":"setEnabled","enabled":false,"value":1}),
            json!({"kind":"setProps","field":"position","value":[1]}),
            json!({"kind":"setValue","property":"shape","frame":0,"values":[2]}),
            json!({"kind":"setAnimated","property":"amount","frame":0,"animated":true,"values":[900]}),
            json!({"kind":"setKey","property":"amount","frame":0,"values":[null]}),
        ] {
            assert!(!edit(&action));
        }
    }
}
