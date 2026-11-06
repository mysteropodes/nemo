//! Which baseline members this slice owns, which belong to a later slice and which are editor preferences.
use super::{Ctx, LegacyImportError};
use serde_json::{Map, Value};

/// JavaScript falsy or empty: absent data, `false`, `0`, `""`, `[]`, `{}`. Such a member carries no content.
pub(crate) fn is_default(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::Bool(flag) => !flag,
        Value::Number(number) => number.as_f64() == Some(0.0),
        Value::String(text) => text.is_empty(),
        Value::Array(items) => items.is_empty(),
        Value::Object(map) => map.is_empty(),
    }
}

/// Top-level members converted into project metadata or the drawing document (slice 1).
pub(super) const PROJECT_MAPPED: &[&str] = &[
    "version",
    "totalFrames",
    "fps",
    "canvasW",
    "canvasH",
    "canvasBg",
    "waIn",
    "waOut",
    "layers",
    "frames",
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
    "layerFolders",
    "storyboard",
    "symbols",
    "tweenOverrides",
    "tweenEasing",
    "easingCurve",
    "resamplePts",
    "tweenStep",
    "motionArcs",
    "audioTracks",
    "refMedia",
    "mediaLibrary",
    "imageMeshes",
];
/// Editor preferences with no document meaning for a drawn project: recorded in `ignored`, never a refusal.
pub(super) const PROJECT_PREFERENCES: &[&str] = &[
    "workspace",
    "mediaMode",
    "shyEnabled",
    "motionBlurOn",
    "motionBlurSamples",
    "motionBlurShutter",
    "layerLinkGroups",
];
/// Families a later slice owns (folders, components, StoryBoard, motion arcs, tweens, media, expressions): content refuses.
pub(super) const PROJECT_LATER: &[&str] = &[
    "customEffects",
    "trackRoles",
    "exprGlobals",
];

pub(crate) fn project_members(
    doc: &Map<String, Value>,
    ctx: &mut Ctx,
) -> Result<(), LegacyImportError> {
    for (key, value) in doc {
        let key = key.as_str();
        if PROJECT_MAPPED.contains(&key) {
            continue;
        }
        if PROJECT_PREFERENCES.contains(&key) {
            if !is_default(value) {
                ctx.ignored.push(key.to_owned());
            }
        } else if PROJECT_LATER.contains(&key) {
            if !is_default(value) {
                return Err(LegacyImportError::unsupported(
                    key,
                    "this project content has no native owner in this release",
                ));
            }
        } else if !is_default(value) {
            ctx.warn(key, "unknown_field", "unknown member, not carried over");
        }
    }
    Ok(())
}

/// Layer members converted by slice 1.
pub(super) const LAYER_MAPPED: &[&str] = &[
    "name",
    "visible",
    "locked",
    "frames",
    "color",
    "layerUid",
    "parentLayerUid",
    "shy",
];
/// Layer members a later slice owns; any content refuses. `inPoint`/`outPoint` are handled by [`range_is_default`].
pub(super) const LAYER_LATER: &[&str] = &[
    "lfsGroup",
    "lfsIds",
    "lfsSettings",
    "channel",
    "linkGroupId",
    "nativeVideo",
    "montageId",
    "lipSync",
    "videoMeshId",
    "parentKeys",
    "parentLayerUidB",
    "parentsMore",
    "motionBlur",
    "shapeNames",
];

/// Layer members the native layer record carries under the same name; the strict codec judges their values and a refusal is
/// attributed to the baseline member by `admit.rs`.
pub(super) const LAYER_PASS: &[&str] = &[
    "textAnimators",
    "effects",
    "effectsFrom",
    "isEffectLayer",
    "pathFx",
    "rig",
];

/// Copy the pass-through layer members that hold content onto the native layer record.
pub(crate) fn layer_pass(map: &Map<String, Value>, record: &mut Value, ctx: &mut Ctx, path: &str) {
    for key in LAYER_PASS {
        if let Some(value) = map.get(*key).filter(|v| !is_default(v)) {
            record[*key] = value.clone();
        }
    }
    if let Some(animators) = record.get_mut("textAnimators").and_then(Value::as_array_mut) {
        for (index, animator) in animators.iter_mut().enumerate() {
            // The baseline selector tests `shape` against its numeric table (square is 1) and a name falls through to square.
            let Some(shape) = animator.pointer_mut("/selector/shape") else { continue };
            if let Some(name) = shape.as_str() {
                let number = match name.to_ascii_lowercase().replace(['_', '-'], "").as_str() {
                    "rampup" => 2,
                    "rampdown" => 3,
                    "triangle" => 4,
                    "round" => 5,
                    "smooth" => 6,
                    _ => 1,
                };
                *shape = Value::from(number);
                ctx.warn(
                    format!("{path}.textAnimators[{index}].selector.shape"),
                    "text_selector_shape_normalised",
                    "the selector shape was a name, which the baseline reads through its numeric table",
                );
            }
        }
    }
    // The text layer marker is derived: a layer holding text items reads as a text layer natively.
    if map.get("isTextLayer").is_some_and(|v| !is_default(v)) {
        ctx.ignored.push(format!("{path}.isTextLayer"));
    }
}

/// Members another module of this reader converts (kinds, Motion, duplicators, components).
fn converted_elsewhere(key: &str) -> bool {
    super::kinds::MAPPED.contains(&key)
        || super::holders::MAPPED.contains(&key)
        || super::dup::MAPPED.contains(&key)
        || super::components::LAYER_MAPPED.contains(&key)
}

pub(crate) fn layer_member(
    path: &str,
    key: &str,
    value: &Value,
    total: u32,
    ctx: &mut Ctx,
) -> Result<(), LegacyImportError> {
    if LAYER_MAPPED.contains(&key) || LAYER_PASS.contains(&key) || key == "isTextLayer" || key == "groups" || converted_elsewhere(key) {
        return Ok(());
    }
    let member = format!("{path}.{key}");
    let _ = total;
    if key == "footage" {
        // Still, image and sequence footage only label a layer whose pictures are ordinary raster strokes; they hold no media.
        let kind = value.get("kind").and_then(Value::as_str);
        return if is_default(value) || matches!(kind, Some("still" | "image" | "sequence")) {
            if !is_default(value) {
                ctx.ignored.push(member);
            }
            Ok(())
        } else {
            Err(LegacyImportError::unsupported(
                member,
                "this footage kind has no native owner in this release",
            ))
        };
    }
    if LAYER_LATER.contains(&key) {
        // `blendMode: "normal"` and `matteMode: "none"` are the baseline defaults.
        let neutral = matches!(
            (key, value.as_str()),
            ("blendMode", Some("normal")) | ("matteMode", Some("none"))
        );
        return if is_default(value) || neutral {
            Ok(())
        } else {
            Err(LegacyImportError::unsupported(
                member,
                "this layer content has no native owner in this release",
            ))
        };
    }
    if !is_default(value) {
        ctx.warn(member, "unknown_field", "unknown member, not carried over");
    }
    Ok(())
}

/// Frame members: `strokes`, `isKeyframe`, `isInterpolated` are read by the layer pass.
pub(crate) fn frame_member(
    path: &str,
    key: &str,
    value: &Value,
    ctx: &mut Ctx,
) -> Result<(), LegacyImportError> {
    let read = [
        "strokes",
        "isKeyframe",
        "isInterpolated",
        "isManualEdit",
        "tweenManualMode",
        "componentFrame",
        "blankOverride",
    ];
    if read.contains(&key) {
        return Ok(());
    }
    if !is_default(value) {
        ctx.warn(
            format!("{path}.{key}"),
            "unknown_field",
            "unknown member, not carried over",
        );
    }
    Ok(())
}
