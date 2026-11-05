//! The per-stroke tags of a baseline `serP` dict that the native vector record carries in its `vectorExt` slot (mask tags,
//! box angle, stored gradient, linked fill pair, parametric shape, paint-bucket provenance, brush-texture and bitmap-brush
//! anchors, shadow channel) and in its `effects` member. The strict native codec stays the judge of every value; a value it
//! refuses is attributed to the baseline member by `admit.rs`.
use super::classify::is_default;
use super::{Ctx, LegacyImportError};
use serde_json::{json, Map, Value};

/// Baseline stroke members converted here.
pub(super) const MAPPED: &[&str] = &[
    "isMask",
    "maskMode",
    "maskFeather",
    "boxAngle",
    "fillGradient",
    "linkedFillId",
    "isLinkedFillCompanion",
    "paramShape",
    "fillSeed",
    "fillSeeds",
    "fillGapPx",
    "fillWalls",
    "brushGroupId",
    "isBrushTextureCopy",
    "brushTexturePreset",
    "preTextureOpacity",
    "preTextureStroke",
    "channelTag",
    "shadowSwatchId",
    "bitmapBrushSpec",
    "bitmapPressureProfile",
    "effects",
];

/// Native `vectorExt` member for each group of baseline members, in the order a refusal is probed.
pub(super) const EXT_MEMBERS: &[(&str, &[&str])] = &[
    ("isMask", &["isMask", "maskMode", "maskFeather"]),
    ("boxAngle", &["boxAngle"]),
    ("fillGradient", &["fillGradient"]),
    ("linkedFillId", &["linkedFillId", "isLinkedFillCompanion"]),
    ("paramShape", &["paramShape"]),
    (
        "fillTrace",
        &["fillSeed", "fillSeeds", "fillGapPx", "fillWalls"],
    ),
    (
        "brushTexture",
        &[
            "brushGroupId",
            "isBrushTextureCopy",
            "brushTexturePreset",
            "preTextureOpacity",
            "preTextureStroke",
        ],
    ),
    ("shadow", &["channelTag", "shadowSwatchId"]),
    (
        "bitmapBrush",
        &[
            "bitmapBrushSpec",
            "bitmapPressureProfile",
            "brushGroupId",
            "preTextureOpacity",
            "preTextureStroke",
        ],
    ),
];

fn truthy(map: &Map<String, Value>, key: &str) -> bool {
    map.get(key).is_some_and(|v| !is_default(v))
}

fn take(map: &Map<String, Value>, into: &mut Map<String, Value>, from: &str, to: &str) {
    if let Some(value) = map.get(from).filter(|v| !v.is_null()) {
        into.insert(to.into(), value.clone());
    }
}

/// The `vectorExt` members (merged into `ext`) and the record's `effects` of one stroke.
pub(super) fn build(
    path: &str,
    map: &Map<String, Value>,
    ext: &mut Map<String, Value>,
    record: &mut Value,
    ctx: &mut Ctx,
) -> Result<(), LegacyImportError> {
    if truthy(map, "isMask") {
        ext.insert("isMask".into(), json!(true));
    }
    if truthy(map, "maskMode") {
        take(map, ext, "maskMode", "maskMode");
    }
    if truthy(map, "maskFeather") {
        take(map, ext, "maskFeather", "maskFeather");
    }
    if truthy(map, "boxAngle") {
        take(map, ext, "boxAngle", "boxAngle");
    }
    if truthy(map, "fillGradient") {
        take(map, ext, "fillGradient", "fillGradient");
    }
    if truthy(map, "linkedFillId") {
        take(map, ext, "linkedFillId", "linkedFillId");
    }
    if truthy(map, "isLinkedFillCompanion") {
        ext.insert("isLinkedFillCompanion".into(), json!(true));
    }
    if truthy(map, "paramShape") {
        take(map, ext, "paramShape", "paramShape");
    }
    trace(path, map, ext, ctx);
    let anchor = truthy(map, "bitmapBrushSpec");
    let mut texture = Map::new();
    if !anchor {
        if truthy(map, "brushGroupId") {
            take(map, &mut texture, "brushGroupId", "brushGroupId");
        }
        if truthy(map, "isBrushTextureCopy") {
            texture.insert("isBrushTextureCopy".into(), json!(true));
        }
    }
    if truthy(map, "brushTexturePreset") {
        take(
            map,
            &mut texture,
            "brushTexturePreset",
            "brushTexturePreset",
        );
    }
    if !anchor {
        // serP writes the pre-texture members whenever they are defined (a null stroke is a value).
        take(map, &mut texture, "preTextureOpacity", "preTextureOpacity");
        take(map, &mut texture, "preTextureStroke", "preTextureStroke");
    }
    if !texture.is_empty() {
        ext.insert("brushTexture".into(), Value::Object(texture));
    }
    if truthy(map, "channelTag") {
        let mut shadow = Map::new();
        take(map, &mut shadow, "channelTag", "channelTag");
        if truthy(map, "shadowSwatchId") {
            take(map, &mut shadow, "shadowSwatchId", "shadowSwatchId");
        }
        ext.insert("shadow".into(), Value::Object(shadow));
    } else if truthy(map, "shadowSwatchId") {
        ctx.warn(
            format!("{path}.shadowSwatchId"),
            "unknown_field",
            "a swatch id without a shadow channel tag is not carried over",
        );
    }
    if anchor {
        let mut bitmap = Map::new();
        take(map, &mut bitmap, "bitmapBrushSpec", "bitmapBrushSpec");
        take(map, &mut bitmap, "brushGroupId", "brushGroupId");
        take(map, &mut bitmap, "preTextureStroke", "preTextureStroke");
        take(map, &mut bitmap, "preTextureOpacity", "preTextureOpacity");
        if truthy(map, "bitmapPressureProfile") {
            take(
                map,
                &mut bitmap,
                "bitmapPressureProfile",
                "bitmapPressureProfile",
            );
        }
        ext.insert("bitmapBrush".into(), Value::Object(bitmap));
    } else if truthy(map, "bitmapPressureProfile") {
        ctx.warn(
            format!("{path}.bitmapPressureProfile"),
            "unknown_field",
            "a pressure profile without a bitmap brush spec is not carried over",
        );
    }
    if truthy(map, "effects") {
        let effects = map.get("effects").filter(|v| v.is_array()).ok_or_else(|| {
            LegacyImportError::invalid(format!("{path}.effects"), "an array is required")
        })?;
        record["effects"] = effects.clone();
    }
    Ok(())
}

/// `fillSeed` / `fillSeeds` / `fillGapPx` / `fillWalls`: the paint-bucket provenance. A gap or wall list without a seed
/// cannot regenerate anything, so it is dropped with a warning (the fill geometry itself is the stroke).
fn trace(path: &str, map: &Map<String, Value>, ext: &mut Map<String, Value>, ctx: &mut Ctx) {
    let mut fill = Map::new();
    if truthy(map, "fillSeed") {
        take(map, &mut fill, "fillSeed", "seed");
    }
    if truthy(map, "fillSeeds") {
        take(map, &mut fill, "fillSeeds", "seeds");
    }
    if fill.is_empty() {
        for member in ["fillGapPx", "fillWalls"] {
            if truthy(map, member) {
                ctx.warn(
                    format!("{path}.{member}"),
                    "unknown_field",
                    "paint-bucket settings without a seed point are not carried over",
                );
            }
        }
        return;
    }
    if truthy(map, "fillWalls") {
        take(map, &mut fill, "fillWalls", "walls");
    }
    // `fillGapPx !== undefined` is serialised, so an explicit zero is a value.
    take(map, &mut fill, "fillGapPx", "gapPx");
    ext.insert("fillTrace".into(), Value::Object(fill));
}
