//! Vector text of a baseline project. A text block is stored as glyph outline paths; the root glyph carries the block
//! (`isTextRoot`, text, font, size, colour, align, anchor and typography) and the rest carry only their `groupId`. The native
//! record is the block itself (the glyphs are laid out again from it), so the root becomes a text item and its glyph strokes
//! are dropped. A font outside the bundled table cannot be laid out again, so that block stays as the outline paths it was
//! drawn with, which look identical, and the report says so.
use super::classify::is_default;
use super::{Ctx, LegacyImportError};
use crate::object_document::object_text_layout_kernels::object_text_layout_record::font::FontRef;
use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet};

/// Stroke members a text glyph or root carries on top of the vector members.
pub(super) const MAPPED: &[&str] = &[
    "isVectorText",
    "vectorChar",
    "charIndex",
    "wordIndex",
    "lineIndex",
    "isText",
    "isTextRoot",
    "text",
    "vectorFont",
    "textSize",
    "textColor",
    "textAlign",
    "textFixedWidth",
    "anchorTopLeft",
    "bold",
    "italic",
    "underline",
    "strike",
    "letterSpacing",
    "wordSpacing",
    "lineHeightMult",
    "textCase",
];

fn truthy(map: &Map<String, Value>, key: &str) -> bool {
    map.get(key).is_some_and(|v| !is_default(v))
}

fn colour(text: &str) -> bool {
    [4, 7, 9].contains(&text.len())
        && text.starts_with('#')
        && text.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit)
}

/// Which strokes of one frame are glyphs of a block that becomes a text item: stroke index -> nothing, plus root index -> block.
pub(super) struct Frame {
    pub(super) skip: HashSet<usize>,
    pub(super) roots: HashMap<usize, Value>,
}

pub(super) fn plan(
    path: &str,
    strokes: &[Value],
    ctx: &mut Ctx,
) -> Result<Frame, LegacyImportError> {
    let mut frame = Frame {
        skip: HashSet::new(),
        roots: HashMap::new(),
    };
    let mut groups: HashMap<String, usize> = HashMap::new();
    for (index, stroke) in strokes.iter().enumerate() {
        let Some(map) = stroke.as_object().filter(|m| truthy(m, "isTextRoot")) else {
            continue;
        };
        let spath = format!("{path}.strokes[{index}]");
        let font = map.get("vectorFont").and_then(Value::as_str).unwrap_or("");
        let Ok(font) = FontRef::from_legacy_key(font) else {
            ctx.warn(
                format!("{spath}.vectorFont"),
                "text_outlined",
                "the font is not one of the bundled fonts, so the text is kept as the outline shapes it was drawn with",
            );
            continue;
        };
        let block = block(&spath, map, &font)?;
        if let Some(group) = map
            .get("groupId")
            .and_then(Value::as_str)
            .filter(|g| !g.is_empty())
        {
            groups.insert(group.to_owned(), index);
        }
        frame.roots.insert(index, block);
    }
    for (index, stroke) in strokes.iter().enumerate() {
        let Some(map) = stroke.as_object() else {
            continue;
        };
        if truthy(map, "isTextRoot") {
            continue;
        }
        if truthy(map, "isVectorText") {
            if let Some(group) = map.get("groupId").and_then(Value::as_str) {
                if groups.contains_key(group) {
                    frame.skip.insert(index);
                }
            }
        }
    }
    Ok(frame)
}

fn block(path: &str, map: &Map<String, Value>, font: &FontRef) -> Result<Value, LegacyImportError> {
    let text = map
        .get("text")
        .and_then(Value::as_str)
        .filter(|t| !t.trim().is_empty())
        .ok_or_else(|| LegacyImportError::invalid(format!("{path}.text"), "the text is empty"))?;
    let font = serde_json::to_value(font).map_err(|_| {
        LegacyImportError::invalid(format!("{path}.vectorFont"), "the font cannot be encoded")
    })?;
    let size = map
        .get("textSize")
        .filter(|v| v.as_f64().is_some_and(|n| n != 0.0))
        .cloned()
        .unwrap_or(json!(48));
    let color = map
        .get("textColor")
        .and_then(Value::as_str)
        .filter(|c| !c.is_empty())
        .unwrap_or("#000000");
    if !colour(color) {
        return Err(LegacyImportError::unsupported(
            format!("{path}.textColor"),
            "the text colour is not a hex colour",
        ));
    }
    let align = map
        .get("textAlign")
        .and_then(Value::as_str)
        .filter(|a| !a.is_empty())
        .unwrap_or("left");
    let anchor = map
        .get("anchorTopLeft")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            LegacyImportError::invalid(
                format!("{path}.anchorTopLeft"),
                "the text anchor is required",
            )
        })?;
    let mut block = json!({"text": text, "font": font, "size": size, "color": color, "align": align,
        "anchorTopLeft": {"x": anchor.get("x").cloned().unwrap_or(json!(0)), "y": anchor.get("y").cloned().unwrap_or(json!(0))}});
    if truthy(map, "textFixedWidth") {
        block["fixedWidth"] = map["textFixedWidth"].clone();
    }
    for flag in ["bold", "italic", "underline", "strike"] {
        if truthy(map, flag) {
            block[flag] = json!(true);
        }
    }
    for number in ["letterSpacing", "wordSpacing"] {
        if truthy(map, number) {
            block[number] = map[number].clone();
        }
    }
    if truthy(map, "lineHeightMult") {
        block["lineHeightMult"] = map["lineHeightMult"].clone();
    }
    if map
        .get("textCase")
        .and_then(Value::as_str)
        .is_some_and(|c| !c.is_empty() && c != "none")
    {
        block["textCase"] = map["textCase"].clone();
    }
    Ok(block)
}
