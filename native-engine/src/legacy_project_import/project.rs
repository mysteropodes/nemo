//! Canvas and project settings (baseline `importJSON` defaults) plus the optional metadata groups the native envelope owns.
use super::{Ctx, LegacyImportError};
use serde_json::{Map, Value};

pub(super) struct Settings {
    pub(super) total: u32,
    pub(super) metadata: Value,
}

/// Members copied verbatim into the envelope; the strict native codec validates their shape.
const PASS_THROUGH: &[&str] = &[
    "bpm",
    "bpmOffset",
    "bpmShow",
    "markers",
    "palettes",
    "colorPalette",
    "activePaletteIdx",
    "shadowPalette",
    "shadowActiveId",
    "guides",
    "perspectiveEnabled",
    "perspectiveMode",
    "perspectiveDensity",
    "perspectiveVPs",
    "symmetryEnabled",
    "symmetryMode",
    "symmetryAxis",
    "symmetryRadialCenter",
    "symmetryRadialSectors",
    "symmetryExtend",
    "comments",
    "customBrushPresets",
    "cameraKeys",
    "cameraLayerOn",
];

fn truthy_number(doc: &Map<String, Value>, key: &str) -> Result<Option<f64>, LegacyImportError> {
    match doc.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(n)) => Ok(n.as_f64().filter(|v| *v != 0.0 && v.is_finite())),
        Some(Value::Bool(false)) => Ok(None),
        Some(_) => Err(LegacyImportError::invalid(key, "a number is required")),
    }
}

fn whole(path: &str, value: f64, largest: f64) -> Result<u32, LegacyImportError> {
    if value.fract() != 0.0 || value < 1.0 || value > largest {
        return Err(LegacyImportError::unsupported(
            path,
            "a positive whole number is required",
        ));
    }
    Ok(value as u32)
}

pub(super) fn read(
    doc: &Map<String, Value>,
    layers: &[Value],
    ctx: &mut Ctx,
) -> Result<Settings, LegacyImportError> {
    // `d.totalFrames||d.layers[0].frames.length`
    let first_len = layers[0]
        .get("frames")
        .and_then(Value::as_array)
        .map_or(0, Vec::len) as f64;
    let total = whole(
        "totalFrames",
        truthy_number(doc, "totalFrames")?.unwrap_or(first_len),
        1_000_000.0,
    )?;
    let fps = truthy_number(doc, "fps")?.unwrap_or(12.0);
    if fps <= 0.0 {
        return Err(LegacyImportError::unsupported(
            "fps",
            "a positive frame rate is required",
        ));
    }
    let width = whole(
        "canvasW",
        truthy_number(doc, "canvasW")?.unwrap_or(1920.0),
        65_536.0,
    )?;
    let height = whole(
        "canvasH",
        truthy_number(doc, "canvasH")?.unwrap_or(1080.0),
        65_536.0,
    )?;
    let background = match doc.get("canvasBg") {
        Some(Value::String(text)) if !text.is_empty() => text.clone(),
        None | Some(Value::Null) | Some(Value::String(_)) => "#ffffff".into(),
        Some(_) => {
            return Err(LegacyImportError::invalid(
                "canvasBg",
                "a colour string is required",
            ))
        }
    };
    let wa_in = truthy_number(doc, "waIn")?.unwrap_or(0.0);
    let wa_out = match doc.get("waOut") {
        None | Some(Value::Null) => f64::from(total) - 1.0,
        Some(Value::Number(n)) => n.as_f64().unwrap_or(0.0),
        Some(_) => return Err(LegacyImportError::invalid("waOut", "a number is required")),
    };
    let mut metadata = Map::new();
    metadata.insert("canvasW".into(), width.into());
    metadata.insert("canvasH".into(), height.into());
    metadata.insert("fps".into(), super::color::number(fps));
    metadata.insert("canvasBg".into(), background.into());
    metadata.insert("waIn".into(), super::color::number(wa_in));
    metadata.insert("waOut".into(), super::color::number(wa_out));
    for key in PASS_THROUGH {
        let Some(value) = doc.get(*key).filter(|v| !v.is_null()) else {
            continue;
        };
        if ctx.omit.contains(key) {
            ctx.warn(
                *key,
                "not_carried_over",
                "the native project refused this member, so it was not carried over",
            );
            continue;
        }
        metadata.insert((*key).into(), value.clone());
    }
    Ok(Settings {
        total,
        metadata: Value::Object(metadata),
    })
}
