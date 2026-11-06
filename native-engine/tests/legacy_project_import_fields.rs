//! The baseline field catalogue (I03 / I04 join): every member the baseline writes in `exportJSON`, a layer, a frame, `serP`
//! and `serR` has a stated disposition, and each disposition is exercised: refused members refuse with their exact path,
//! ignored members are reported, and a member nobody classified warns instead of vanishing. Adding a member to the
//! baseline catalogue without a disposition fails here.
use super::fields::{disposition, Disposition, Scope};
use super::unit_tests::*;
use super::*;
use serde_json::{json, Value};

const PROJECT: &[&str] = &[
    "version",
    "totalFrames",
    "fps",
    "canvasW",
    "canvasH",
    "canvasBg",
    "waIn",
    "waOut",
    "mediaMode",
    "layers",
    "layerFolders",
    "layerLinkGroups",
    "storyboard",
    "symbols",
    "palettes",
    "activePaletteIdx",
    "customBrushPresets",
    "shadowPalette",
    "shadowActiveId",
    "customEffects",
    "audioTracks",
    "refMedia",
    "workspace",
    "mediaLibrary",
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
    "motionArcs",
    "easingCurve",
    "resamplePts",
    "tweenStep",
    "tweenOverrides",
    "tweenEasing",
    "comments",
    "cameraKeys",
    "cameraLayerOn",
    "markers",
    "shyEnabled",
    "trackRoles",
    "imageMeshes",
    "bpm",
    "bpmOffset",
    "bpmShow",
    "motionBlurOn",
    "motionBlurSamples",
    "motionBlurShutter",
    "guides",
    "exprGlobals",
];
const LAYER: &[&str] = &[
    "name",
    "visible",
    "locked",
    "frames",
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
    "color",
    "motion",
    "motionStatic",
    "elementMotion",
    "inPoint",
    "outPoint",
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
    "layerUid",
    "parentLayerUid",
    "parentKeys",
    "parentLayerUidB",
    "parentsMore",
    "followPath",
    "markers",
    "shy",
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
const FRAME: &[&str] = &[
    "strokes",
    "isKeyframe",
    "isInterpolated",
    "isManualEdit",
    "tweenManualMode",
    "componentFrame",
    "blankOverride",
];
const STROKE: &[&str] = &[
    "segments",
    "closed",
    "strokeColor",
    "hasRealStroke",
    "strokeWidth",
    "strokeCap",
    "strokeJoin",
    "miterLimit",
    "fillColor",
    "opacity",
    "dashArray",
    "dashOffset",
    "paintOrder",
    "isVectorBrush",
    "isFillShape",
    "isFillCloseLine",
    "centerSegments",
    "widthProfile",
    "strokeProfile",
    "profileBase",
    "fillSeed",
    "fillSeeds",
    "fillGapPx",
    "fillWalls",
    "strokeId",
    "brushGroupId",
    "isLinkedFillCompanion",
    "linkedFillId",
    "tweenOn",
    "boxAngle",
    "xformAnchorKey",
    "xformAnchorCustom",
    "isBrushTextureCopy",
    "brushTexturePreset",
    "bitmapBrushSpec",
    "bitmapPressureProfile",
    "preTextureOpacity",
    "preTextureStroke",
    "channelTag",
    "shadowSwatchId",
    "ownerId",
    "ownerName",
    "ownerColor",
    "revisionParentId",
    "isRevisionGhost",
    "revisionAction",
    "preRevisionOpacity",
    "fillGradient",
    "groupId",
    "trackRoleId",
    "attachOffset",
    "attachedToRoleId",
    "elemHidden",
    "paramShape",
    "effects",
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
    "isMask",
    "maskMode",
    "maskFeather",
    "strokeGradientAlongPath",
];
const RASTER: &[&str] = &[
    "isRaster",
    "x",
    "y",
    "width",
    "height",
    "opacity",
    "isBitmapBrush",
    "linked",
    "linkedPath",
    "linkedHandleId",
    "src",
    "rotation",
    "brushGroupId",
    "isBrushTextureCopy",
    "meshId",
    "groupId",
    "isText",
    "text",
    "font",
    "size",
    "color",
    "align",
    "fixedWidth",
    "isTextChar",
    "textGroupId",
    "charIndex",
    "wordIndex",
    "lineIndex",
];

fn catalogue() -> Vec<(Scope, &'static str)> {
    let mut all = Vec::new();
    for (scope, list) in [
        (Scope::Project, PROJECT),
        (Scope::Layer, LAYER),
        (Scope::Stroke, STROKE),
        (Scope::Raster, RASTER),
    ] {
        all.extend(list.iter().map(|k| (scope, *k)));
    }
    all
}

#[test]
fn every_baseline_member_has_a_stated_disposition() {
    let missing: Vec<String> = catalogue()
        .into_iter()
        .filter(|(scope, key)| disposition(*scope, key).is_none())
        .map(|(scope, key)| format!("{scope:?}.{key}"))
        .collect();
    assert!(
        missing.is_empty(),
        "members without a disposition: {missing:?}"
    );
    let total = catalogue().len() + FRAME.len();
    assert_eq!(
        total, 240,
        "the catalogue size is pinned: extend it with the baseline, never silently"
    );
}

fn refused_doc(scope: Scope, key: &str) -> (Value, String) {
    match scope {
        Scope::Project => (
            json!({"layers": [{"frames": []}], key: "x"}),
            key.to_owned(),
        ),
        Scope::Layer => (
            json!({"layers": [{"frames": [], key: "x"}]}),
            format!("layers[0].{key}"),
        ),
        Scope::Stroke => {
            let mut stroke = base_stroke();
            stroke[key] = json!("x");
            (
                plain(stroke),
                format!("layers[0].frames[0].strokes[0].{key}"),
            )
        }
        Scope::Raster => {
            let mut raster = json!({"isRaster": true, "x": 1, "y": 1, "width": 2, "height": 2, "src": "data:image/png;base64,iVBORw0KGgo"});
            raster[key] = json!("x");
            (
                plain(raster),
                format!("layers[0].frames[0].strokes[0].{key}"),
            )
        }
    }
}

#[test]
fn refused_members_refuse_with_their_exact_path_and_the_typed_code() {
    let mut refused = 0;
    for (scope, key) in catalogue() {
        if disposition(scope, key) != Some(Disposition::Refused) {
            continue;
        }
        refused += 1;
        let (doc, path) = refused_doc(scope, key);
        let error = refusal(doc);
        assert_eq!(
            (error.code(), error.path()),
            (LEGACY_UNSUPPORTED_FIELD, path.as_str()),
            "{scope:?}.{key}"
        );
    }
    assert_eq!(refused, 29, "the refused set is pinned (blendKeys left it in #2257)");
}

#[test]
fn ignored_members_are_reported_and_unknown_ones_warn() {
    let converted = import(&json!({"version": 13, "totalFrames": 4, "layers": [{"frames": [], "isTextLayer": true, "footage": {"kind": "still"}}],
        "workspace": {"a": 1}, "mediaMode": "embedded", "shyEnabled": true, "somethingNew": 1}).to_string());
    let ignored: Vec<&str> = converted
        .report
        .ignored
        .iter()
        .map(String::as_str)
        .collect();
    for expected in [
        "workspace",
        "mediaMode",
        "shyEnabled",
        "layers[0].isTextLayer",
        "layers[0].footage",
    ] {
        assert!(
            ignored.contains(&expected),
            "{expected} is reported: {ignored:?}"
        );
    }
    assert!(converted
        .report
        .warnings
        .iter()
        .any(|w| w.path == "somethingNew" && w.code == "unknown_field"));
    let mut stroke = base_stroke();
    stroke["xformAnchorKey"] = json!("tl");
    stroke["ownerName"] = json!("Ana");
    stroke["brandNew"] = json!(1);
    let converted = import(&plain(stroke).to_string());
    let codes: Vec<(String, &str)> = warnings(&converted);
    for (path, code) in [
        (
            "layers[0].frames[0].strokes[0].xformAnchorKey",
            "editor_preference",
        ),
        (
            "layers[0].frames[0].strokes[0].ownerName",
            "review_attribution",
        ),
        ("layers[0].frames[0].strokes[0].brandNew", "unknown_field"),
    ] {
        assert!(
            codes.contains(&(path.to_owned(), code)),
            "{path}: {codes:?}"
        );
    }
}

#[test]
fn frame_members_are_read_not_dropped() {
    for key in FRAME {
        if *key == "strokes" {
            continue;
        }
        let converted = import(&json!({"version": 13, "layers": [{"frames": [{"isKeyframe": true, "strokes": [], *key: true}]}]}).to_string());
        assert!(
            !converted
                .report
                .warnings
                .iter()
                .any(|w| w.path.ends_with(key) && w.code == "unknown_field"),
            "{key} is a read frame member"
        );
    }
}
