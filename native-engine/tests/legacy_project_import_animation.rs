//! Legacy project reader, slice 2: Motion, rig and component families. Every family is checked field by field against
//! the hand-authored baseline project `animation.json` (and the pinned migration and component fixtures), with fixed
//! expected values, the typed refusals, and import -> save -> reopen -> edit -> undo/redo through the real owner.
use super::unit_tests::*;
use super::*;
use serde_json::{json, Value};

pub(super) const ANIMATION: &str =
    include_str!("../../tests/fixtures/legacy-import/animation.json");

pub(super) fn drawing(text: &str) -> Value {
    file(&import(text))["drawing"].clone()
}
pub(super) fn holder<'a>(drawing: &'a Value, uid: &str, stroke: Option<&str>) -> &'a Value {
    drawing["propertyHolders"]
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h["target"]["layerUid"] == uid && h["target"]["strokeId"].as_str() == stroke)
        .unwrap_or_else(|| panic!("holder {uid} {stroke:?}"))
}
pub(super) fn property<'a>(holder: &'a Value, name: &str) -> &'a Value {
    holder["properties"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["property"] == name)
        .unwrap_or_else(|| panic!("property {name}"))
}
pub(super) fn property_names(holder: &Value) -> Vec<String> {
    holder["properties"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["property"].as_str().unwrap().to_owned())
        .collect()
}
pub(super) fn animation() -> Value {
    serde_json::from_str(ANIMATION).unwrap()
}
pub(super) fn refused(value: Value) -> LegacyImportError {
    convert_admitted(value.to_string().as_bytes())
        .err()
        .expect("refused")
}

#[test]
fn motion_becomes_property_holders_with_sorted_keys_statics_and_expressions() {
    let d = drawing(ANIMATION);
    let shapes = holder(&d, "ly_shapes", None);
    let position = property(shapes, "position");
    let frames: Vec<i64> = position["track"]["keys"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k["frame"].as_i64().unwrap())
        .collect();
    assert_eq!(frames, [0, 4, 8], "keys are sorted by frame like sortKeys");
    assert_eq!(position["track"]["keys"][1]["v"], json!([20, 5]));
    assert_eq!(position["track"]["keys"][1]["hold"], true);
    assert_eq!(position["track"]["keys"][1]["lockTo"], "in");
    assert_eq!(position["track"]["keys"][2]["hOut"], json!([0, 0]));
    assert_eq!(
        property(shapes, "opacity")["track"]["keys"][1]["v"],
        json!([50])
    );
    let rotation = property(shapes, "rotation");
    assert_eq!(rotation["static"], json!([15]));
    assert_eq!(
        rotation["expression"],
        json!({"enabled": true, "code": "time*10"})
    );
    assert_eq!(property(shapes, "scale")["static"], json!([110, 90]));
    assert!(
        property(shapes, "scale").get("expression").is_none(),
        "an empty disabled expression is the seed, not content"
    );
    assert_eq!(
        shapes["controls"],
        json!([
            {"key": "xc_wave", "name": "Wave", "type": "number", "default": [2]},
            {"key": "xc_pt", "name": "Offset", "type": "point"}])
    );
    let element = holder(&d, "ly_shapes", Some("s_shape_4"));
    assert_eq!(property(element, "opacity")["static"], json!([70]));
    assert_eq!(
        property(element, "position")["track"]["keys"][1]["v"],
        json!([9, 9])
    );
    assert_eq!(
        d["propertyHolders"].as_array().unwrap().len(),
        5,
        "shapes, its shape, child offsets, rig, component inner"
    );
}

#[test]
fn the_order_property_lives_in_the_composition_order_plan() {
    let d = drawing(ANIMATION);
    let plan = &d["compositionOrder"]["contexts"][0];
    let order = &plan["layers"][0]["order"]["track"]["keys"];
    assert_eq!(
        (
            order[0]["v"].clone(),
            order[1]["v"].clone(),
            order[1]["frame"].clone()
        ),
        (json!([0]), json!([3]), json!(8))
    );
    assert!(property_names(holder(&d, "ly_shapes", None))
        .iter()
        .all(|n| n != "order"));
    assert_eq!(plan["layers"][1]["order"], json!({}));
}

#[test]
fn unknown_properties_and_orders_that_cannot_be_held_are_named() {
    let mut doc = animation();
    doc["layers"][0]["motion"]["bogusProperty"] = json!({"keys": [{"frame": 0, "v": [1]}]});
    let error = refused(doc);
    assert_eq!(
        (error.code(), error.path()),
        (LEGACY_UNSUPPORTED_FIELD, "layers[0].motion")
    );
    // A shape drawn on several frames keeps one identity, so one element order cannot name a single drawing.
    let mut doc = animation();
    doc["layers"][0]["frames"][8]["strokes"][0]["strokeId"] = json!("s_shape_4");
    doc["layers"][0]["elementMotion"]["s_shape_4"] =
        json!({"motion": {"order": {"keys": [{"frame": 0, "v": [1]}]}}});
    let error = refused(doc);
    assert_eq!(
        (error.code(), error.path()),
        (
            LEGACY_UNSUPPORTED_FIELD,
            "layers[0].elementMotion.s_shape_4.motion.order"
        )
    );
    let mut doc = animation();
    doc["symbols"]["sym_a"]["layers"][0]["motion"]["order"] =
        json!({"keys": [{"frame": 0, "v": [1]}]});
    let error = refused(doc);
    assert_eq!(
        (error.code(), error.path()),
        (
            LEGACY_UNSUPPORTED_FIELD,
            "symbols.sym_a.layers[0].motion.order"
        )
    );
    let mut doc = animation();
    doc["layers"][0]["expressions"]["rotation"]["code"] = json!("x".repeat(5000));
    assert_eq!(refused(doc).path(), "layers[0].expressions.rotation.code");
}

#[test]
fn layer_kinds_blend_mattes_range_and_flags_keep_their_baseline_values() {
    let d = drawing(ANIMATION);
    let layers = d["layers"].as_array().unwrap();
    let shapes = &layers[0];
    assert_eq!(
        (
            shapes["blendMode"].clone(),
            shapes["keyLock"].clone(),
            shapes["threeD"].clone()
        ),
        (json!("multiply"), json!("layer"), json!(true))
    );
    assert_eq!(
        shapes["range"],
        json!({"inPoint": 2, "outPoint": 9, "markers": [{"frame": 3, "name": "Hit", "color": "#ff0000"}]})
    );
    let child = &layers[1];
    assert_eq!(child["matteMode"], "alphaInverted");
    assert_eq!(child["matteSourceLayerUid"], "ly_shapes");
    assert_eq!(
        child["mattesMore"],
        json!([{"uid": "ly_matte2", "mode": "luma"}]),
        "an extra matte whose source is gone is dropped with a warning"
    );
    assert_eq!(
        child["range"]["timeLink"],
        json!({"uid": "ly_shapes", "mode": "in", "srcAnchor": "out"})
    );
    assert_eq!(
        child["followPath"],
        json!({"targetLayerUid": "ly_null", "align": true})
    );
    assert_eq!(child["parentLayerUid"], "ly_shapes");
    assert_eq!(
        (
            layers[2]["folderId"].clone(),
            layers[2]["shy"].clone(),
            layers[2]["locked"].clone()
        ),
        (json!("fld_a"), json!(true), json!(true))
    );
    assert_eq!(
        (
            layers[3]["isNullLayer"].clone(),
            layers[3]["nullPos"].clone()
        ),
        (json!(true), json!([120, 80]))
    );
    assert_eq!(
        (
            layers[4]["isGuideLayer"].clone(),
            layers[4]["guidePos"].clone(),
            layers[4]["guideOrientation"].clone()
        ),
        (json!(true), json!([30, 40]), json!("horizontal"))
    );
    assert_eq!(
        (
            layers[5]["isFolderLayer"].clone(),
            layers[5]["folderCollapsed"].clone()
        ),
        (json!(true), json!(true))
    );
    assert_eq!(layers[6]["widget"]["kind"], "joystick");
    assert_eq!(
        layers[6]["widget"]["x"],
        json!({"key": "xc_turn", "min": -50, "max": 50, "rest": 0, "label": "Turn"})
    );
    assert_eq!(
        d["layerFolders"],
        json!({"fld_a": {"name": "Group A", "collapsed": true}})
    );
    let converted = import(ANIMATION);
    assert_eq!(
        warnings(&converted),
        [
            (
                "layers[0].elementMotion.ghost_stroke".to_owned(),
                "dangling_element_motion"
            ),
            (
                "layers[1].mattesMore[1]".to_owned(),
                "dangling_matte_source"
            ),
            ("layers[3].nullShape".to_owned(), "display_preference")
        ]
    );
}

#[test]
fn a_key_tint_is_carried_over_and_a_malformed_one_is_named_and_dropped() {
    let mut doc = animation();
    doc["layers"][0]["motion"]["position"]["keys"][2]["color"] = json!("#e8b64c");
    doc["layers"][0]["motion"]["position"]["keys"][0]["color"] = json!("amber");
    let converted = import(&doc.to_string());
    let codes: Vec<_> = warnings(&converted).into_iter().map(|w| w.1).collect();
    assert!(codes.contains(&"invalid_key_color"), "{codes:?}");
    let d = file(&converted)["drawing"].clone();
    let keys = property(holder(&d, "ly_shapes", None), "position")["track"]["keys"].clone();
    let at = |frame: i64| keys.as_array().unwrap().iter().find(|k| k["frame"] == frame).unwrap().clone();
    assert_eq!(at(4)["color"], "#e8b64c");
    assert!(keys.as_array().unwrap().iter().filter(|k| k.get("color").is_some()).count() == 1);
}

#[test]
fn dangling_references_degrade_like_the_baseline_and_bad_values_refuse() {
    let mut doc = animation();
    doc["layers"][1]["timeLink"] = json!({"parentUid": "ly_shapes", "inOffset": 3});
    doc["layers"][2]["folderId"] = json!("fld_missing");
    doc["layers"][1]["followPath"] = json!({"targetLayerUid": "ly_gone"});
    doc["layers"][1]["matteSourceLayerUid"] = json!("ly_gone");
    let converted = import(&doc.to_string());
    let codes: Vec<_> = warnings(&converted).into_iter().map(|w| w.1).collect();
    for code in [
        "dead_time_link",
        "dangling_folder",
        "dangling_follow_path",
        "dangling_matte_source",
    ] {
        assert!(codes.contains(&code), "{code} in {codes:?}");
    }
    let wire = file(&converted)["drawing"].clone();
    assert!(wire["layers"][1].get("matteSourceLayerUid").is_none());
    assert!(wire["layers"][2].get("folderId").is_none());
    for (index, member, value) in [
        (0, "keyLock", json!("sideways")),
        (0, "blendMode", json!(7)),
        (0, "inPoint", json!(1.5)),
        (3, "nullPos", json!([1])),
        (6, "widget", json!(5)),
    ] {
        let mut doc = animation();
        doc["layers"][index][member] = value;
        assert_eq!(
            refused(doc).path(),
            format!("layers[{index}].{member}"),
            "{member}"
        );
    }
}

#[test]
fn migration_fixture_converts_every_family_it_pins() {
    let converted = import(MIGRATION);
    let d = file(&converted)["drawing"].clone();
    let curve = holder(&d, "ly_mig_curve", None);
    let keys = &property(curve, "position")["track"]["keys"];
    assert_eq!(
        keys[0]["curvePoints"],
        json!([{"x": 0, "y": 0}, {"x": 0.25, "y": 0.156}, {"x": 0.5, "y": 0.5}, {"x": 0.75, "y": 0.844}, {"x": 1, "y": 1}])
    );
    assert_eq!(
        keys[1]["curvePoints"][1],
        json!({"x": 0.4, "y": 0.1, "tx": 0.2})
    );
    let link = holder(&d, "ly_mig_timelink", None);
    assert_eq!(
        (
            property(link, "timeLinkInOffset")["static"].clone(),
            property(link, "timeLinkOutOffset")["static"].clone()
        ),
        (json!([3]), json!([-2]))
    );
    assert_eq!(
        d["componentTables"]["duplicators"]["duplicators"][0]["duplicator"]["effectors"][0]
            ["channels"][0],
        json!({"prop": "position", "value": [10, 0]})
    );
    let matte = d["layers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["layerUid"] == "ly_mig_matte")
        .unwrap();
    assert_eq!(
        (
            matte["matteMode"].clone(),
            matte["matteSourceLayerUid"].clone()
        ),
        (json!("alpha"), json!("ly_mig_above"))
    );
    assert!(
        d.get("tweens").is_none(),
        "an automatic span carries no override, easing or manual mode"
    );
}
