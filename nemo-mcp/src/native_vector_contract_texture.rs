//! G16 (#2361): serP's brush-texture members on a vector record's `vectorExt.brushTexture`.
use crate::native_contract::exact;
use serde_json::Value;

/// Non-empty strings (255 bytes, no NUL), a `true` copy flag, a finite opacity, a hex stroke or null;
/// an anchor (preset) is never also a dab (copy).
pub(super) fn brush_texture(value: &Value) -> bool {
    let text = |v: &Value| v.as_str().is_some_and(|s| !s.is_empty() && s.len() <= 255 && !s.contains('\0'));
    let hex = |s: &str| {
        [4, 7, 9].contains(&s.len()) && s.starts_with('#') && s.bytes().skip(1).all(|b| b.is_ascii_hexdigit())
    };
    let keys = ["brushGroupId", "isBrushTextureCopy", "brushTexturePreset", "preTextureOpacity", "preTextureStroke", "strokeSeed"];
    exact(value, &[], &keys).is_some_and(|object| {
        !object.is_empty()
            && object.get("brushGroupId").is_none_or(text)
            && object.get("brushTexturePreset").is_none_or(text)
            && object.get("isBrushTextureCopy").is_none_or(|v| v == &Value::Bool(true))
            && object.get("preTextureOpacity").is_none_or(|v| v.as_f64().is_some_and(f64::is_finite))
            && object.get("preTextureStroke").is_none_or(|v| v.is_null() || v.as_str().is_some_and(hex))
            && !(object.contains_key("isBrushTextureCopy") && object.contains_key("brushTexturePreset"))
            // G16 option C: an anchor's stamping seed, a uint32.
            && object.get("strokeSeed").is_none_or(|v| {
                v.as_u64().is_some_and(|s| s <= u64::from(u32::MAX)) && object.contains_key("brushTexturePreset")
            })
    })
}
