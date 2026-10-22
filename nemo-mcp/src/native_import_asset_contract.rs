//! Closed shape checks for the native import planner's retained assets and real items.
//! No asset bytes, inventory, source locator or provider provenance is admitted here.
use super::{exact, finite, point, source_id, target};
use serde_json::Value;

const FONTS: [&str; 8] = [
    "Roboto-Regular",
    "Roboto-Bold",
    "DejaVuSans-Regular",
    "DejaVuSans-Bold",
    "DejaVuSerif-Regular",
    "DejaVuSerif-Bold",
    "Manrope-Regular",
    "Inter-Regular",
];

fn positive(value: Option<&Value>) -> bool {
    value
        .and_then(Value::as_f64)
        .is_some_and(|n| n.is_finite() && n > 0.0)
}

fn tag(value: &Value) -> bool {
    value
        .as_str()
        .is_some_and(|s| !s.is_empty() && s.len() <= 255 && !s.contains('\0'))
}

fn raster(value: &Value) -> bool {
    exact(
        value,
        &["assetId", "x", "y", "width", "height"],
        &[
            "opacity",
            "rotation",
            "isBitmapBrush",
            "brushGroupId",
            "isBrushTextureCopy",
            "meshId",
        ],
    )
    .is_some_and(|o| {
        source_id(o.get("assetId")).is_some()
            && finite(o.get("x"))
            && finite(o.get("y"))
            && positive(o.get("width"))
            && positive(o.get("height"))
            && o.get("rotation").is_none_or(|n| finite(Some(n)))
            && o.get("opacity").is_none_or(|n| {
                n.as_f64()
                    .is_some_and(|n| n.is_finite() && (0.0..=1.0).contains(&n))
            })
            && ["isBitmapBrush", "isBrushTextureCopy"]
                .iter()
                .all(|k| o.get(*k).is_none_or(Value::is_boolean))
            && ["brushGroupId", "meshId"]
                .iter()
                .all(|k| o.get(*k).is_none_or(tag))
    })
}

fn font(value: &Value) -> bool {
    match value["kind"].as_str() {
        Some("bundled") => exact(value, &["kind", "fontKey"], &[]).is_some_and(|o| {
            o.get("fontKey")
                .and_then(Value::as_str)
                .is_some_and(|key| FONTS.contains(&key))
        }),
        Some("asset") => exact(value, &["kind", "assetId"], &[])
            .is_some_and(|o| source_id(o.get("assetId")).is_some()),
        _ => false,
    }
}

// ECMAScript trim whitespace, matching TextBlockRecord's glyph admission (not Rust trim).
fn trim_space(c: char) -> bool {
    matches!(
        c,
        '\u{0009}'..='\u{000D}' | '\u{0020}' | '\u{00A0}' | '\u{1680}'
            | '\u{2000}'..='\u{200A}' | '\u{2028}' | '\u{2029}' | '\u{202F}'
            | '\u{205F}' | '\u{3000}' | '\u{FEFF}'
    )
}

fn text(value: &Value) -> bool {
    exact(
        value,
        &["text", "font", "size", "color", "align", "anchorTopLeft"],
        &[
            "fixedWidth",
            "fixedHeight",
            "bold",
            "italic",
            "underline",
            "strike",
            "letterSpacing",
            "wordSpacing",
            "lineHeightMult",
            "textCase",
        ],
    )
    .is_some_and(|o| {
        o["text"].as_str().is_some_and(|s| {
            !s.contains('\0')
                && s.encode_utf16().count() <= 65536
                && s.chars().any(|c| !trim_space(c))
        }) && font(&o["font"])
            && positive(o.get("size"))
            && o["color"].as_str().is_some_and(|s| {
                s.strip_prefix('#').is_some_and(|hex| {
                    matches!(hex.len(), 6 | 8) && hex.bytes().all(|b| b.is_ascii_hexdigit())
                })
            })
            && matches!(o["align"].as_str(), Some("left" | "center" | "right"))
            && point(o.get("anchorTopLeft"))
            && ["fixedWidth", "fixedHeight"]
                .iter()
                .all(|k| o.get(*k).is_none_or(|n| n.is_null() || positive(Some(n))))
            && o.get("lineHeightMult").is_none_or(|n| positive(Some(n)))
            && ["letterSpacing", "wordSpacing"]
                .iter()
                .all(|k| o.get(*k).is_none_or(|n| finite(Some(n))))
            && ["bold", "italic", "underline", "strike"]
                .iter()
                .all(|k| o.get(*k).is_none_or(Value::is_boolean))
            && o.get("textCase").is_none_or(|v| {
                matches!(v.as_str(), Some("none" | "upper" | "lower" | "capitalize"))
            })
    })
}

pub(super) fn asset_row(value: &Value) -> bool {
    exact(value, &["sourceId", "assetId"], &[]).is_some_and(|o| {
        source_id(o.get("sourceId")).is_some() && source_id(o.get("assetId")).is_some()
    })
}

fn item_row(value: &Value, member: &str, check: fn(&Value) -> bool) -> bool {
    exact(value, &["sourceId", "entry"], &[]).is_some_and(|o| {
        source_id(o.get("sourceId")).is_some()
            && exact(&o["entry"], &["target", member], &[])
                .is_some_and(|entry| target(entry.get("target")) && check(&entry[member]))
    })
}

pub(super) fn raster_row(value: &Value) -> bool {
    item_row(value, "item", raster)
}

pub(super) fn text_row(value: &Value) -> bool {
    item_row(value, "block", text)
}

pub(super) fn optional_rows<'a>(
    payload: &'a Value,
    key: &str,
    check: fn(&Value) -> bool,
) -> Option<&'a [Value]> {
    match payload.get(key) {
        None => Some(&[]),
        Some(value) => value
            .as_array()
            .filter(|rows| rows.iter().all(check))
            .map(Vec::as_slice),
    }
}

/// Real native source descriptors only; the host resolves bytes outside the owner lock.
fn source_row(value: &Value) -> bool {
    exact(value, &["sourceId", "name", "mediaType", "source"], &[]).is_some_and(|o| {
        source_id(o.get("sourceId")).is_some()
            && source_id(o.get("name")).is_some()
            && match o["mediaType"].as_str() {
                Some("image/png") => exact(&o["source"], &["kind", "path"], &[]).is_some_and(|s| {
                    s["kind"] == "file"
                        && s["path"].as_str().is_some_and(|path| {
                            !path.contains('\0') && std::path::Path::new(path).is_absolute()
                        })
                }),
                Some("font/ttf") => {
                    let source = &o["source"];
                    if source["kind"] == "bundled" {
                        font(source)
                    } else {
                        exact(source, &["kind", "family", "weight", "italic"], &[]).is_some_and(
                            |s| {
                                s["kind"] == "google"
                                    && source_id(s.get("family")).is_some()
                                    && s["weight"]
                                        .as_u64()
                                        .is_some_and(|n| (1..=1000).contains(&n))
                                    && s["italic"].is_boolean()
                            },
                        )
                    }
                }
                _ => false,
            }
    })
}

pub(super) fn source_rows(payload: &Value) -> Option<&[Value]> {
    if payload.get("sources").is_none() {
        return (payload.get("documentSnapshotId").is_none()
            && payload.get("lifecycleGeneration").is_none())
        .then_some(&[]);
    }
    let rows = optional_rows(payload, "sources", source_row)?;
    let snapshot = payload["documentSnapshotId"].as_str()?;
    let lifecycle = payload["lifecycleGeneration"].as_u64()?;
    ((1..=1024).contains(&rows.len())
        && crate::contract::bounded_identifier(snapshot)
        && (1..=9_007_199_254_740_991).contains(&lifecycle))
    .then_some(rows)
}
