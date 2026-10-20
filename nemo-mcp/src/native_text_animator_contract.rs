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
