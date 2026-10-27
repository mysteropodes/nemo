//! #2357 G43: the native Lottie writer against the REAL baseline `lottieBuild` (src/js/export.js @1065e9d8, run in a
//! Node vm over the stored stroke dictionaries by gen-lottie-oracle.cjs). Each case's native document holds the same
//! strokes as records mapped by the shipped vector-stroke mapper; every plan comes from the actual codec, history owner,
//! frame packet, render scene and paint plan the PNG and SVG exports use.
use super::{build_lottie, LottieExportError, LottieSpec};
use crate::history::NativeObjectHistory;
use crate::object_codec::decode_storage_project;
use crate::object_document::FrameScopeKind;
use crate::object_frame_packet::{prepare_object_frame, ObjectFrameSelector};
use crate::object_render_scene::{
    prepare_object_paint_plan, prepare_object_render_scene, ObjectPaintPlan,
};
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};

const ORACLE: &str = include_str!("fixtures/native-vector-export-lottie/oracle.json");

fn owner(value: &Value) -> NativeObjectHistory {
    NativeObjectHistory::new(
        "lottie",
        decode_storage_project(&serde_json::to_vec(value).unwrap()).unwrap(),
    )
    .unwrap()
}
fn plan(snapshot: &ObjectSnapshot, frame: u32) -> ObjectPaintPlan {
    let pins = ObjectFrameSelector {
        instance_id: snapshot.instance_id(),
        document_id: snapshot.document_id(),
        content_revision: snapshot.content_revision(),
        document_snapshot_id: snapshot.snapshot_id(),
        context_id: "scene-root",
        scope_kind: FrameScopeKind::Authored,
        frame,
    };
    let packet = prepare_object_frame(snapshot, &pins).unwrap();
    let scene = prepare_object_render_scene(&packet, &pins).unwrap();
    prepare_object_paint_plan(&scene, &pins).unwrap()
}
fn spec_of(case: &Value) -> LottieSpec {
    let s = &case["spec"];
    LottieSpec {
        start: s["start"].as_u64().unwrap() as u32,
        end: s["end"].as_u64().unwrap() as u32,
        fps: s["fps"].as_number().unwrap().clone(),
        width: s["width"].as_u64().unwrap() as u32,
        height: s["height"].as_u64().unwrap() as u32,
        background: s["background"].as_str().unwrap().to_string(),
    }
}
fn export(case: &Value) -> Result<Value, LottieExportError> {
    let spec = spec_of(case);
    // Camera and duplicator cases are whole native project files (the stored camera and the duplicator's canvas live
    // in the project), which only the project-file codec loads (the application target runs these rows; export_job runs the drawing-only ones).
    #[cfg(feature = "project_media_codec")]
    let history = if case["project"].is_null() {
        owner(&case["document"])
    } else {
        NativeObjectHistory::from_project_file(
            "lottie",
            &serde_json::to_vec(&case["project"]).unwrap(),
        )
        .unwrap()
    };
    #[cfg(not(feature = "project_media_codec"))]
    let history = owner(&case["document"]);
    let snapshot = history
        .acquire_snapshot(history.content_revision())
        .unwrap();
    let plans = (spec.start..=spec.end)
        .map(|frame| plan(&snapshot, frame))
        .collect::<Vec<_>>();
    build_lottie(&plans, &spec)
}
/// Structural equality with numbers equal to 1e-9: handles are recovered from the drawn path's absolute control points,
/// so `(point + handle) - point` may differ from the stored handle in the last place.
fn same(actual: &Value, expected: &Value, at: &str) -> Result<(), String> {
    match (actual, expected) {
        (Value::Number(a), Value::Number(e)) => {
            let (a, e) = (a.as_f64().unwrap(), e.as_f64().unwrap());
            if (a - e).abs() <= 1e-9 * (1.0 + e.abs()) {
                Ok(())
            } else {
                Err(format!("{at}: {a} != {e}"))
            }
        }
        (Value::Array(a), Value::Array(e)) => {
            if a.len() != e.len() {
                return Err(format!("{at}: length {} != {}", a.len(), e.len()));
            }
            a.iter()
                .zip(e)
                .enumerate()
                .try_for_each(|(k, (a, e))| same(a, e, &format!("{at}[{k}]")))
        }
        (Value::Object(a), Value::Object(e)) => {
            let keys = |m: &serde_json::Map<String, Value>| m.keys().cloned().collect::<Vec<_>>();
            if keys(a) != keys(e) {
                return Err(format!("{at}: keys {:?} != {:?}", keys(a), keys(e)));
            }
            a.iter()
                .try_for_each(|(k, v)| same(v, &e[k], &format!("{at}.{k}")))
        }
        _ if actual == expected => Ok(()),
        _ => Err(format!("{at}: {actual} != {expected}")),
    }
}

#[test]
fn every_oracle_case_matches_the_baseline_writer() {
    let oracle: Value = serde_json::from_str(ORACLE).unwrap();
    assert_eq!(oracle["commit"].as_str().unwrap().len(), 40);
    let cases = oracle["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 10);
    let runs = |case: &&Value| cfg!(feature = "project_media_codec") || case["project"].is_null();
    assert_eq!(
        cases.iter().filter(runs).count(),
        if cfg!(feature = "project_media_codec") {
            10
        } else {
            7
        }
    );
    for case in cases.iter().filter(runs) {
        let lottie = export(case).unwrap_or_else(|error| panic!("{}: {error:?}", case["id"]));
        if let Err(diff) = same(&lottie, &case["lottie"], case["id"].as_str().unwrap()) {
            panic!("{diff}");
        }
    }
}

#[test]
fn slots_runs_and_drops_follow_the_baseline_structure() {
    let oracle: Value = serde_json::from_str(ORACLE).unwrap();
    let by_id = |id: &str| {
        oracle["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["id"] == id)
            .unwrap()
            .clone()
    };
    let names = |lottie: &Value| {
        lottie["layers"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| format!("{}@{}-{}", l["nm"].as_str().unwrap(), l["ip"], l["op"]))
            .collect::<Vec<_>>()
    };
    // A slot that appears on frames 2-3 only is its own layer over that run; the background spans the range.
    assert_eq!(
        names(&export(&by_id("slot-runs")).unwrap()),
        ["Rough / shape1@2-4", "Rough / shape0@0-5", "Background@0-5"]
    );
    // The zero-opacity stroke is dropped, so three slots remain.
    let mixed = export(&by_id("filters-and-paint")).unwrap();
    assert_eq!(mixed["layers"].as_array().unwrap().len(), 4);
    assert_eq!(mixed["layers"][0]["bm"], json!(3), "overlay");
}

#[test]
fn a_range_needs_one_plan_per_frame_pinned_in_order() {
    let oracle: Value = serde_json::from_str(ORACLE).unwrap();
    let case = &oracle["cases"][0];
    let spec = spec_of(case);
    let history = owner(&case["document"]);
    let snapshot = history
        .acquire_snapshot(history.content_revision())
        .unwrap();
    let plans = [plan(&snapshot, 0), plan(&snapshot, 2)];
    assert_eq!(
        build_lottie(&plans[..1], &spec),
        Err(LottieExportError::InvalidRange)
    );
    let two = LottieSpec {
        end: 1,
        ..spec.clone()
    };
    assert_eq!(
        build_lottie(&plans, &two),
        Err(LottieExportError::WrongFrame { index: 1 })
    );
    let backwards = LottieSpec {
        start: 2,
        end: 1,
        ..spec
    };
    assert_eq!(
        build_lottie(&plans, &backwards),
        Err(LottieExportError::InvalidRange)
    );
}

#[test]
fn the_comparison_catches_a_moved_anchor_a_missing_member_and_a_changed_paint() {
    let oracle: Value = serde_json::from_str(ORACLE).unwrap();
    let case = &oracle["cases"][0];
    let lottie = export(case).unwrap();
    assert!(same(&lottie, &case["lottie"], "ok").is_ok());
    let mut moved = case["lottie"].clone();
    let x = moved["layers"][0]["shapes"][0]["it"][0]["ks"]["k"][0]["s"][0]["v"][1][0]
        .as_f64()
        .unwrap();
    moved["layers"][0]["shapes"][0]["it"][0]["ks"]["k"][0]["s"][0]["v"][1][0] = json!(x + 1e-6);
    assert!(same(&lottie, &moved, "moved").is_err());
    let mut missing = case["lottie"].clone();
    missing["layers"][1].as_object_mut().unwrap().remove("bm");
    assert!(same(&lottie, &missing, "missing").is_err());
    let mut paint = case["lottie"].clone();
    paint["layers"][0]["shapes"][0]["it"][1]["lc"] = json!(1);
    assert!(same(&lottie, &paint, "paint").is_err());
}

#[cfg(feature = "project_media_codec")]
#[test]
fn a_stored_camera_moves_every_anchor_and_turns_every_handle() {
    let oracle: Value = serde_json::from_str(ORACLE).unwrap();
    let case = oracle["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == "camera-move")
        .unwrap();
    let with = export(case).unwrap();
    let mut flat = case.clone();
    flat["project"] = Value::Null;
    let without = export(&flat).unwrap();
    // Frame 0 sits on the identity key; by frame 4 the camera has zoomed and rolled, so anchors and handles both move.
    let key =
        |l: &Value, k: usize| l["layers"][0]["shapes"][0]["it"][0]["ks"]["k"][k]["s"][0].clone();
    assert!(same(&key(&with, 0), &key(&without, 0), "identity").is_ok());
    assert!(same(&key(&with, 4)["v"], &key(&without, 4)["v"], "v").is_err());
    assert!(same(&key(&with, 4)["i"], &key(&without, 4)["i"], "i").is_err());
}

#[test]
#[cfg(feature = "font_asset_admission")]
fn a_text_block_is_dropped_as_the_baselines_baked_text_raster_is() {
    // `lottieBuild` filters `!sd.isRaster`, and the baseline's text is a raster (`commitText` bakes it to a PNG).
    let oracle: Value = serde_json::from_str(ORACLE).unwrap();
    let case = &oracle["cases"][0];
    let mut with_text = case.clone();
    let document = &mut with_text["document"];
    document["items"] = serde_json::json!({"text":[{"target":{"contextId":"scene-root","layerUid":"L0",
        "strokeId":"words","frameScope":{"kind":"authored","frame":0}},"block":{"text":"Hi",
        "font":{"kind":"bundled","fontKey":"Roboto-Regular"},"size":48,"color":"#112233","align":"left",
        "anchorTopLeft":{"x":10,"y":20}}}]});
    let scope = &mut document["compositionOrder"]["contexts"][0]["scopes"][0]["elements"];
    let next = scope
        .as_array()
        .unwrap()
        .iter()
        .filter(|element| element["layerUid"] == "L0")
        .count();
    scope.as_array_mut().unwrap().push(serde_json::json!(
        {"layerUid":"L0","strokeId":"words","naturalPosition":next,"order":{}}));
    let history = owner(&with_text["document"]);
    let snapshot = history.acquire_snapshot(history.content_revision()).unwrap();
    assert!(
        crate::object_render_scene::ordered_entries(&plan(&snapshot, 0))
            .any(|entry| matches!(entry, crate::object_render_scene::PaintEntry::Text(_))),
        "the frame really paints the text block"
    );
    assert_eq!(export(&with_text).unwrap(), export(case).unwrap());
}
