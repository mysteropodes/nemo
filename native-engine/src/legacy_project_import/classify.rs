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
];
/// Editor preferences with no document meaning for a drawn project: recorded in `ignored`, never a refusal.
const PROJECT_PREFERENCES: &[&str] = &[
    "workspace",
    "mediaMode",
    "easingCurve",
    "resamplePts",
    "tweenStep",
    "shyEnabled",
    "motionBlurOn",
    "motionBlurSamples",
    "motionBlurShutter",
    "layerLinkGroups",
];
/// Families a later slice owns (folders, components, StoryBoard, motion arcs, tweens, media, expressions): content refuses.
const PROJECT_LATER: &[&str] = &[
    "layerFolders",
    "storyboard",
    "symbols",
    "customEffects",
    "audioTracks",
    "refMedia",
    "mediaLibrary",
    "motionArcs",
    "tweenOverrides",
    "tweenEasing",
    "trackRoles",
    "imageMeshes",
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
const LAYER_LATER: &[&str] = &[
    "symbolId",
    "symPlayMode",
    "symSpeed",
    "symPlacedAt",
    "symSingleFrame",
    "symMatrix",
    "lfsGroup",
    "lfsIds",
    "lfsSettings",
    "blendMode",
    "blendKeys",
    "folderId",
    "channel",
    "linkGroupId",
    "motion",
    "motionStatic",
    "elementMotion",
    "nativeVideo",
    "matteMode",
    "matteSourceLayerUid",
    "mattesMore",
    "montageId",
    "expressions",
    "isTextLayer",
    "isNullLayer",
    "nullPos",
    "nullShape",
    "isEffectLayer",
    "isFolderLayer",
    "folderCollapsed",
    "isGuideLayer",
    "guidePos",
    "guideOrientation",
    "isWidgetLayer",
    "widget",
    "textAnimators",
    "isEffectorLayer",
    "effector",
    "effects",
    "pathFx",
    "lipSync",
    "footage",
    "videoMeshId",
    "parentKeys",
    "parentLayerUidB",
    "parentsMore",
    "followPath",
    "markers",
    "keyLock",
    "timeRemap",
    "motionBlur",
    "effectsFrom",
    "timeLink",
    "exprControls",
    "threeD",
    "duplicator",
    "rig",
    "groups",
    "shapeNames",
];

pub(crate) fn layer_member(
    path: &str,
    key: &str,
    value: &Value,
    total: u32,
    ctx: &mut Ctx,
) -> Result<(), LegacyImportError> {
    if LAYER_MAPPED.contains(&key) {
        return Ok(());
    }
    let member = format!("{path}.{key}");
    if key == "inPoint" || key == "outPoint" {
        return if range_is_default(key, value, total) {
            Ok(())
        } else {
            Err(LegacyImportError::unsupported(
                member,
                "a layer in/out range has no native owner in this release",
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

/// `inPoint 0` and an `outPoint` at or past the last frame select the whole timeline, so they hide nothing.
fn range_is_default(key: &str, value: &Value, total: u32) -> bool {
    match value {
        Value::Null => true,
        Value::Number(number) => {
            let at = number.as_f64().unwrap_or(f64::NAN);
            if key == "inPoint" {
                at <= 0.0
            } else {
                at >= f64::from(total) - 1.0
            }
        }
        _ => false,
    }
}

/// Frame members: `strokes`, `isKeyframe`, `isInterpolated` are read by the layer pass.
pub(crate) fn frame_member(
    path: &str,
    key: &str,
    value: &Value,
    ctx: &mut Ctx,
) -> Result<(), LegacyImportError> {
    if ["strokes", "isKeyframe", "isInterpolated"].contains(&key) {
        return Ok(());
    }
    if key == "componentFrame" && !is_default(value) {
        return Err(LegacyImportError::unsupported(
            format!("{path}.{key}"),
            "a component frame pick has no native owner in this release",
        ));
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
