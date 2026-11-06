//! Legacy project reader, slice 2: components, duplicators, StoryBoard, tween records, motion arcs, the camera track and
//! the import -> save -> reopen -> edit -> undo/redo round trip of the whole animation project.
use super::animation_tests::*;
use super::refusal_tests::saved;
use super::unit_tests::*;
use super::*;
use crate::history::NativeObjectHistory;
use serde_json::{json, Value};

const COMPONENTS: &str = include_str!("../../tests/fixtures/components/project.json");

#[test]
fn components_keep_symbols_instances_picks_exposed_props_and_time_remap() {
    let d = drawing(ANIMATION);
    let tables = &d["componentTables"]["components"];
    let comp = &tables["components"][0];
    assert_eq!(
        (
            comp["symbolId"].clone(),
            comp["name"].clone(),
            comp["totalFrames"].clone(),
            comp["fps"].clone()
        ),
        (json!("sym_a"), json!("Walk"), json!(6), json!(12))
    );
    assert_eq!(comp["objects"].as_array().unwrap().len(), 2);
    assert_eq!(comp["objects"][1]["target"]["frameScope"]["frame"], 3);
    assert_eq!(comp["objects"][1]["target"]["contextId"], "sym_a");
    assert_eq!(comp["exposedProps"][0]["targetField"], "fillColor");
    let inner = holder(&d, "ly_in", None);
    assert_eq!(inner["target"]["contextId"], "sym_a");
    assert_eq!(
        property(inner, "rotation")["track"]["keys"][1]["v"],
        json!([90])
    );
    let instance = &tables["instances"][0];
    assert_eq!(
        (
            instance["layerUid"].clone(),
            instance["symPlayMode"].clone(),
            instance["symSpeed"].clone(),
            instance["symPlacedAt"].clone()
        ),
        (json!("ly_comp"), json!("pingpong"), json!(2), json!(1))
    );
    assert_eq!(instance["symMatrix"], json!([1, 0, 0, 1, 5, 6]));
    assert_eq!(
        instance["frameKeys"],
        json!([{"frame": 5, "componentFrame": 3}])
    );
    assert_eq!(instance["timeRemap"]["keys"][1]["v"], json!([4]));
    // The pinned baseline component fixture: one symbol and its seven instance layers, as the baseline project holds them.
    let pinned = drawing(COMPONENTS);
    let set = &pinned["componentTables"]["components"];
    assert_eq!(
        (
            set["components"].as_array().unwrap().len(),
            set["instances"].as_array().unwrap().len()
        ),
        (1, 7)
    );
    let held = set["instances"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|i| i.get("frameKeys").is_some())
        .count();
    assert_eq!(held, 2, "the held componentFrame and the blank override");
}

#[test]
fn component_content_without_a_native_owner_refuses() {
    for (member, value) in [
        ("tweenOverrides", json!({"a:0-1": []})),
    ] {
        let mut doc = animation();
        doc["symbols"]["sym_a"][member] = value;
        assert_eq!(refused(doc).path(), format!("symbols.sym_a.{member}"));
    }
    let mut doc = animation();
    doc["layers"][0]["timeRemap"] = json!({"keys": [{"frame": 0, "v": [1]}]});
    assert_eq!(refused(doc).path(), "layers[0].timeRemap");
    let mut doc = animation();
    doc["layers"][9]["symbolId"] = json!("sym_missing");
    doc["layers"][9]
        .as_object_mut()
        .unwrap()
        .remove("timeRemap");
    let converted = import(&doc.to_string());
    assert!(warnings(&converted)
        .iter()
        .any(|w| w.1 == "dangling_symbol"));
}

#[test]
fn duplicators_and_effector_layers_keep_their_descriptors_and_migrated_channels() {
    let d = drawing(ANIMATION);
    let tables = &d["componentTables"]["duplicators"];
    let dup = &tables["duplicators"][0];
    assert_eq!(
        (
            dup["layerUid"].clone(),
            dup["duplicator"]["cols"].clone(),
            dup["duplicator"]["seed"].clone()
        ),
        (json!("ly_dup"), json!(3), json!(7))
    );
    assert_eq!(
        dup["duplicator"]["effectors"][0]["channels"],
        json!([{"prop": "position", "value": [10, 0]}, {"prop": "rotation", "value": [5]}])
    );
    assert_eq!(
        tables["effectorLayers"][0],
        json!({"contextId": "scene-root", "layerUid": "ly_eff", "effector": {"targetLayerUid": "ly_dup", "falloff": "radial", "pos": [320, 180]}})
    );
    let mut doc = animation();
    doc["layers"][8]["effector"] = Value::Null;
    assert_eq!(refused(doc).path(), "layers[8].effector");
}

#[test]
fn storyboard_modules_chains_and_view_are_stored_and_version_one_montages_migrate() {
    let d = drawing(ANIMATION);
    let board = &d["componentTables"]["storyboard"];
    assert_eq!(
        (
            board["pan"].clone(),
            board["zoom"].clone(),
            board["nextId"].clone(),
            board["activeMontageId"].clone()
        ),
        (json!({"x": 40, "y": 40}), json!(1), json!(4), json!("sbm2"))
    );
    assert_eq!(
        board["modules"][0],
        json!({"id": "sbm1", "type": "instance", "x": 10, "y": 10, "symbolId": "sym_a", "trimIn": 0, "trimOut": 5, "duration": 6})
    );
    assert_eq!(board["modules"][1]["chain"], json!(["sbm1"]));
    let mut doc = animation();
    doc["storyboard"] = json!({"pan": {"x": 0, "y": 0}, "zoom": 2, "nextId": 3, "activeMontageId": null,
        "modules": [{"id": "sbm1", "type": "montage", "name": "Old", "x": 5, "y": 6, "playhead": 0, "items": [{"symbolId": "sym_a", "trimIn": 1, "trimOut": 4, "duration": 4}]}]});
    let converted = import(&doc.to_string());
    let board = file(&converted)["drawing"]["componentTables"]["storyboard"].clone();
    assert_eq!(board["nextId"], 4);
    assert_eq!(board["modules"][0]["chain"], json!(["sbm3"]));
    assert_eq!(
        board["modules"][1],
        json!({"id": "sbm3", "type": "instance", "symbolId": "sym_a", "x": 5, "y": 6, "trimIn": 1, "trimOut": 4, "duration": 4})
    );
    assert!(converted
        .report
        .migrations
        .contains(&"storyboardV1:1".to_owned()));
    let mut doc = animation();
    doc["storyboard"]["modules"].as_array_mut().unwrap().push(
        json!({"id": "sbm9", "type": "sound", "name": "Beat", "dataB64": "AAAA", "x": 0, "y": 0}),
    );
    assert_eq!(refused(doc).path(), "storyboard.modules[2].dataB64");
}

fn tween_layer<'a>(drawing: &'a Value, uid: &str) -> &'a Value {
    drawing["tweens"]["layers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["layerUid"] == uid)
        .unwrap_or_else(|| panic!("tween layer {uid}"))
}

#[test]
fn tween_records_hold_overrides_easing_manual_mode_and_generated_frames() {
    let mut doc = animation();
    doc["layers"][0]["frames"][5] =
        json!({"strokes": [], "isKeyframe": false, "isInterpolated": true, "isManualEdit": true});
    doc["layers"][0]["frames"][6] =
        json!({"strokes": [], "isKeyframe": false, "isInterpolated": true});
    doc["layers"][0]["frames"][8]["tweenManualMode"] = json!(true);
    doc["layers"][0]["frames"][8]["strokes"][0]["tweenOn"] = json!(true);
    doc["layers"][0]["frames"][0]["strokes"][0]["tweenOn"] = json!(false);
    doc["layers"][0]["frames"][10] =
        json!({"strokes": [], "isKeyframe": true, "isInterpolated": false});
    let d = drawing(&doc.to_string());
    let store = tween_layer(&d, "ly_shapes");
    assert_eq!(
        store["options"],
        json!({"resamplePoints": 40, "step": 2, "skipManual": true, "easing": []})
    );
    assert_eq!(
        store["spans"][0],
        json!({"from": 0, "to": 4, "overrides": [{"aId": "s_shape_0", "bId": "s_shape_4"}]})
    );
    assert_eq!(
        store["spans"][1]["easing"],
        json!([{"x": 0, "y": 0}, {"x": 0.5, "y": 0.2}, {"x": 1, "y": 1}])
    );
    assert_eq!(
        store["spans"][2],
        json!({"from": 8, "to": 10, "overrides": [], "manualStrokes": ["s_shape_8"]})
    );
    let timing = &d["layers"][0]["timing"];
    assert_eq!(
        timing["samples"],
        json!([{"frame": 5, "manualEdit": true}, {"frame": 6}])
    );
    assert_eq!(
        timing["keys"],
        json!([{"frame": 0}, {"frame": 4}, {"frame": 8}, {"frame": 10}])
    );
    // The legacy step curve is replaced by the default one, and a span naming no layer is dropped with a warning.
    let mut doc = animation();
    doc["easingCurve"] = json!({"points": [{"x": 0, "y": 0}, {"x": 0.42, "y": 0}, {"x": 0.58, "y": 1}, {"x": 1, "y": 1}]});
    doc["tweenOverrides"]["ly_gone:0-3"] = json!([{"aId": "a", "bId": "b"}]);
    let converted = import(&doc.to_string());
    assert_eq!(
        file(&converted)["drawing"]["tweens"]["layers"][0]["options"]["easing"],
        json!([])
    );
    assert!(warnings(&converted).contains(&(
        "tweenOverrides.ly_gone:0-3".to_owned(),
        "dangling_tween_span"
    )));
}

#[test]
fn motion_arcs_and_the_camera_track_are_kept() {
    let mut doc = animation();
    doc["motionArcs"] = json!({"ly_shapes:0-4-0": {"out": [5, 6], "in": [1, 2], "cx": 3, "cy": 4, "junk": 1},
        "0-4-1": {"out": [7, 8]}, "ly_gone:0-4-0": {"cx": 1}});
    let converted = import(&doc.to_string());
    let wire = file(&converted);
    assert_eq!(
        wire["drawing"]["motionArcs"],
        json!({"ly_shapes:0-4-0": {"out": [5, 6], "in": [1, 2], "cx": 3, "cy": 4}, "ly_shapes:0-4-1": {"out": [7, 8]}})
    );
    assert!(warnings(&converted)
        .contains(&("motionArcs.ly_gone:0-4-0".to_owned(), "dangling_motion_arc")));
    assert_eq!(
        wire["project"]["cameraKeys"][1],
        json!({"frame": 8, "x": 20, "y": 10, "w": 600, "rot": 2})
    );
    assert_eq!(wire["project"]["cameraLayerOn"], true);
}

#[test]
fn animation_project_saves_reopens_identically_and_edits_undo_and_redo() {
    let converted = import(ANIMATION);
    let mut owner =
        NativeObjectHistory::from_project_file("legacy-animation", &converted.project_file)
            .unwrap();
    let first = saved(&owner);
    let reopened = NativeObjectHistory::from_project_file("legacy-animation-2", &first).unwrap();
    assert_eq!(
        saved(&reopened),
        first,
        "save -> reopen -> save is byte identical"
    );
    let wire: Value = serde_json::from_slice(&first).unwrap();
    let d = &wire["drawing"];
    assert_eq!(d["propertyHolders"].as_array().unwrap().len(), 5);
    assert_eq!(
        d["componentTables"]["components"]["instances"][0]["frameKeys"][0]["componentFrame"],
        3
    );
    assert_eq!(
        tween_layer(d, "ly_shapes")["spans"].as_array().unwrap().len(),
        2
    );
    assert_eq!(
        (owner.content_revision(), owner.history_depths()),
        (0, (0, 0))
    );
    let payload = json!({"command": "object.layer.visibility.set", "layerUid": "ly_shapes", "visible": false});
    let prepared = owner
        .prepare_layer_visibility_json(payload.to_string().as_bytes())
        .unwrap();
    let edited = owner.commit_layer_visibility(prepared).unwrap();
    assert_eq!(
        owner.history_depths(),
        (1, 0),
        "one gesture, one history entry"
    );
    let hidden: Value = serde_json::from_slice(&saved(&owner)).unwrap();
    assert_eq!(hidden["drawing"]["layers"][0]["visible"], false);
    assert_eq!(
        hidden["drawing"]["propertyHolders"], d["propertyHolders"],
        "the edit leaves Motion untouched"
    );
    let undone = owner.undo(edited).unwrap();
    assert_eq!(
        saved(&owner),
        first,
        "undo restores the imported document exactly"
    );
    owner.redo(undone).unwrap();
    let redone: Value = serde_json::from_slice(&saved(&owner)).unwrap();
    assert_eq!(redone, hidden);
}

fn component_layers(doc: &Value) -> &Vec<Value> {
    doc["componentTables"]["components"]["components"][0]["layers"]
        .as_array()
        .unwrap()
}

#[test]
fn component_layers_keep_sparse_timing_blank_keys_and_generated_frames() {
    let mut doc = animation();
    // Baseline `sym_a` layer `ly_in`: keys at 0 and 3 over six frames. Add a blank key at 1 and a generated frame at 5.
    let inner = &mut doc["symbols"]["sym_a"]["layers"][0]["frames"];
    inner[1] = json!({"strokes": [], "isKeyframe": true, "isInterpolated": false});
    inner[5] = json!({"strokes": [], "isKeyframe": false, "isInterpolated": true, "isManualEdit": true});
    let converted = import(&doc.to_string());
    let d = file(&converted)["drawing"].clone();
    let timing = &component_layers(&d)[0]["timing"];
    assert_eq!(timing["contextId"], "scene-root");
    assert_eq!(timing["keys"], json!([{"frame": 0}, {"frame": 1}, {"frame": 3}]));
    assert_eq!(timing["samples"], json!([{"frame": 5, "manualEdit": true}]));
    assert!(
        !warnings(&converted).iter().any(|w| w.1 == "blank_keyframe_in_component"),
        "a blank key is expressible now"
    );
    let objects = d["componentTables"]["components"]["components"][0]["objects"].as_array().unwrap();
    let frames: Vec<i64> = objects.iter().map(|o| o["target"]["frameScope"]["frame"].as_i64().unwrap()).collect();
    assert_eq!(frames, [0, 3], "the blank key authors no record");
}

#[test]
fn an_imported_component_takes_frame_edits_with_undo_and_redo() {
    let converted = import(ANIMATION);
    let mut owner = NativeObjectHistory::from_project_file("legacy-comp-timing", &converted.project_file).unwrap();
    let first = saved(&owner);
    let key_frames = |bytes: &[u8]| -> (Value, Vec<i64>, Vec<i64>) {
        let d: Value = serde_json::from_slice(bytes).unwrap();
        let component = d["drawing"]["componentTables"]["components"]["components"][0].clone();
        let keys = component["layers"][0]["timing"]["keys"].as_array().unwrap().iter().map(|k| k["frame"].as_i64().unwrap()).collect();
        let mut records: Vec<i64> = component["objects"].as_array().unwrap().iter().map(|o| o["target"]["frameScope"]["frame"].as_i64().unwrap()).collect();
        records.sort_unstable();
        (component["totalFrames"].clone(), keys, records)
    };
    assert_eq!(key_frames(&first), (json!(6), vec![0, 3], vec![0, 3]));
    // Insert Frame at cell 1 (ly_in is held there): the key at 3 moves to 4 and the component grows to 7 frames.
    let insert = json!({"command": "object.frames.insert", "contextId": "sym_a", "cell": 1});
    owner.dispatch_frame_structure("command.document.object.frames.insert", &insert).unwrap();
    let inserted = saved(&owner);
    assert_eq!(key_frames(&inserted), (json!(7), vec![0, 4], vec![0, 4]));
    assert_eq!(owner.history_depths(), (1, 0));
    // Duplicate the drawing shown on the held cell 1 into a new key: a drawing now lives on the formerly held cell.
    let duplicate = json!({"command": "object.frames.duplicate", "contextId": "sym_a", "layerUid": "ly_in", "cell": 1});
    owner.dispatch_frame_structure("command.document.object.frames.duplicate", &duplicate).unwrap();
    assert_eq!(key_frames(&saved(&owner)), (json!(8), vec![0, 2, 5], vec![0, 2, 5]));
    assert_eq!(owner.history_depths(), (2, 0));
    let revision = owner.content_revision();
    let revision = owner.undo(revision).unwrap();
    let revision = owner.undo(revision).unwrap();
    assert_eq!(saved(&owner), first, "two undos restore the imported bytes");
    owner.redo(revision).unwrap();
    assert_eq!(saved(&owner), inserted);
}

#[test]
fn generated_layer_uids_are_unique_across_the_scene_and_every_component() {
    let stroke = base_stroke();
    let frames = json!([{"strokes": [stroke], "isKeyframe": true}]);
    let doc = json!({"version": 13, "totalFrames": 4,
        "layers": [{"name": "Scene", "frames": frames}],
        "symbols": {"sym_a": {"name": "A", "totalFrames": 4, "layers": [{"name": "A1", "frames": frames}, {"name": "A2", "layerUid": "ly_legacy_1", "frames": frames}]},
            "sym_b": {"name": "B", "totalFrames": 4, "layers": [{"name": "B1", "frames": frames}]}}});
    let converted = import(&doc.to_string());
    let d = file(&converted)["drawing"].clone();
    let mut uids: Vec<String> = vec![d["layers"][0]["layerUid"].as_str().unwrap().to_owned()];
    for component in d["componentTables"]["components"]["components"].as_array().unwrap() {
        for layer in component["layers"].as_array().unwrap() {
            uids.push(layer["layerUid"].as_str().unwrap().to_owned());
        }
    }
    let mut sorted = uids.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), 4, "no uid repeats across contexts: {uids:?}");
    assert_eq!(uids[0], "ly_legacy_0", "the scene keeps the first generated uid");
    assert!(uids.contains(&"ly_legacy_1".to_owned()), "an explicit uid is kept");
    // A stroke id derived from a layer uid cannot collide either.
    let ids: Vec<&str> = d["componentTables"]["components"]["components"].as_array().unwrap().iter()
        .flat_map(|c| c["objects"].as_array().unwrap()).map(|o| o["target"]["strokeId"].as_str().unwrap()).collect();
    let scene_id = d["objects"][0]["target"]["strokeId"].as_str().unwrap();
    assert!(!ids.contains(&scene_id), "{scene_id} {ids:?}");
}

#[test]
fn project_tween_options_travel_with_generated_frames_and_are_not_ignored() {
    let generated = json!([{"strokes": [base_stroke()], "isKeyframe": true},
        {"strokes": [], "isKeyframe": false, "isInterpolated": true},
        {"strokes": [base_stroke()], "isKeyframe": true}]);
    let mut doc = json!({"version": 13, "totalFrames": 4, "resamplePts": 33, "tweenStep": 3,
        "layers": [{"name": "T", "layerUid": "ly_t", "frames": generated}, {"name": "U", "layerUid": "ly_u", "frames": [{"strokes": [base_stroke()], "isKeyframe": true}]}]});
    let converted = import(&doc.to_string());
    let d = file(&converted)["drawing"].clone();
    let wanted = json!({"resamplePoints": 33, "step": 3, "skipManual": true, "easing": []});
    assert_eq!(tween_layer(&d, "ly_t")["options"], wanted);
    assert_eq!(tween_layer(&d, "ly_t")["spans"], json!([]));
    assert!(!converted.report.ignored.iter().any(|k| k == "resamplePts" || k == "tweenStep"));
    // Native defaults and generated frames need no record at all.
    doc["resamplePts"] = json!(50);
    doc["tweenStep"] = json!(1);
    let d = file(&import(&doc.to_string()))["drawing"].clone();
    assert_eq!(d["tweens"]["layers"].as_array().unwrap().len(), 1, "only the layer with generated frames");
    assert_eq!(tween_layer(&d, "ly_t")["options"]["resamplePoints"], 50);
}

#[test]
fn a_component_keeps_its_own_camera_keys_and_markers() {
    let mut doc = animation();
    doc["symbols"]["sym_a"]["cameraKeys"] = json!([{"frame": 0, "x": 160, "y": 90, "w": 320}, {"frame": 2, "x": 100, "y": 90, "w": 160}]);
    doc["symbols"]["sym_a"]["markers"] = json!([{"frame": 1, "name": "hit", "color": "#e8b64c"}]);
    let d = drawing(&doc.to_string());
    let component = &d["componentTables"]["components"]["components"][0];
    assert_eq!(component["cameraKeys"].as_array().unwrap().len(), 2);
    assert_eq!(component["cameraKeys"][1]["w"], 160);
    assert_eq!(component["markers"][0]["name"], "hit");
    // A symbol that was entered but never edited stores empty tables, which carry nothing.
    let mut quiet = animation();
    quiet["symbols"]["sym_a"]["cameraKeys"] = json!([]);
    quiet["symbols"]["sym_a"]["markers"] = json!([]);
    quiet["symbols"]["sym_a"]["motionArcs"] = json!({});
    let d = drawing(&quiet.to_string());
    let component = &d["componentTables"]["components"]["components"][0];
    assert!(component.get("cameraKeys").is_none() && component.get("markers").is_none());
}
