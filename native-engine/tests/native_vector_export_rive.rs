//! #2357 G44: the native Rive exporter against the REAL baseline `exportRive` (src/js/rive-export.js @1065e9d8, run
//! whole in a Node vm by gen-rive-oracle.cjs against a deterministic fake Rive Editor). Each case records the baseline's
//! editor conversation call by call with the fake's replies; the native driver must issue exactly the same calls, in the
//! same order, when the same replies are played back. Every plan comes from the actual codec, history owner, frame
//! packet, render scene and paint plan the other exporters use.
use super::driver::{export_rive, RiveEditor};
use super::{RiveExportError, RiveSpec};
use crate::history::NativeObjectHistory;
use crate::object_codec::decode_storage_project;
use crate::object_document::FrameScopeKind;
use crate::object_frame_packet::{prepare_object_frame, ObjectFrameSelector};
use crate::object_render_scene::{
    prepare_object_paint_plan, prepare_object_render_scene, ObjectPaintPlan,
};
use crate::revision::ObjectSnapshot;
use serde_json::Value;

const ORACLE: &str = include_str!("fixtures/native-vector-export-rive/oracle.json");

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
fn spec_of(case: &Value) -> RiveSpec {
    let s = &case["spec"];
    RiveSpec {
        start: s["start"].as_u64().unwrap() as u32,
        end: s["end"].as_u64().unwrap() as u32,
        fps: s["fps"].as_number().unwrap().clone(),
        width: s["width"].as_u64().unwrap() as u32,
        height: s["height"].as_u64().unwrap() as u32,
        background: s["background"].as_str().unwrap().to_string(),
        artboard_name: s["artboardName"].as_str().unwrap().to_string(),
    }
}
/// Structural equality with numbers equal to 1e-9 (handles are recovered from the drawn path's control points).
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
/// Plays the baseline's recorded conversation back, requiring each call to match the one the baseline made.
struct Replay<'a> {
    calls: &'a [Value],
    at: usize,
    case: &'a str,
}
impl RiveEditor for Replay<'_> {
    fn call(&mut self, tool: &str, args: Value) -> Result<Value, RiveExportError> {
        let recorded = self.calls.get(self.at).unwrap_or_else(|| {
            panic!(
                "{}: call {} ({tool}) beyond the baseline's {}",
                self.case,
                self.at,
                self.calls.len()
            )
        });
        assert_eq!(
            tool, recorded["tool"],
            "{}: call {} tool",
            self.case, self.at
        );
        if let Err(diff) = same(
            &args,
            &recorded["args"],
            &format!("call {} {tool}", self.at),
        ) {
            panic!("{}: {diff}", self.case);
        }
        self.at += 1;
        Ok(recorded["reply"].clone())
    }
}

fn history_of(case: &Value) -> NativeObjectHistory {
    #[cfg(feature = "project_media_codec")]
    if !case["project"].is_null() {
        return NativeObjectHistory::from_project_file(
            "rive",
            &serde_json::to_vec(&case["project"]).unwrap(),
        )
        .unwrap();
    }
    NativeObjectHistory::new(
        "rive",
        decode_storage_project(&serde_json::to_vec(&case["document"]).unwrap()).unwrap(),
    )
    .unwrap()
}

#[test]
fn every_oracle_case_holds_the_baseline_conversation() {
    let oracle: Value = serde_json::from_str(ORACLE).unwrap();
    assert_eq!(oracle["commit"].as_str().unwrap().len(), 40);
    let cases = oracle["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 9);
    // Camera and duplicator cases are whole native project files (the stored camera and the duplicator's canvas
    // live in the project), which only the project-file codec loads; the drawing-only target runs the other rows.
    let runs = |case: &&Value| cfg!(feature = "project_media_codec") || case["project"].is_null();
    assert_eq!(
        cases.iter().filter(runs).count(),
        if cfg!(feature = "project_media_codec") {
            9
        } else {
            7
        }
    );
    for case in cases.iter().filter(runs) {
        let id = case["id"].as_str().unwrap();
        let spec = spec_of(case);
        let history = history_of(case);
        let snapshot = history
            .acquire_snapshot(history.content_revision())
            .unwrap();
        let plans: Vec<ObjectPaintPlan> = (spec.start..=spec.end)
            .map(|frame| plan(&snapshot, frame))
            .collect();
        let calls = case["calls"].as_array().unwrap();
        let mut editor = Replay {
            calls,
            at: 0,
            case: id,
        };
        let receipt =
            export_rive(&plans, &spec, &mut editor).unwrap_or_else(|e| panic!("{id}: {e:?}"));
        assert_eq!(editor.at, calls.len(), "{id}: every baseline call was made");
        same(&receipt, &case["result"], "receipt").unwrap_or_else(|d| panic!("{id}: {d}"));
    }
}

#[test]
fn an_editor_that_names_no_artboard_refuses_the_export() {
    struct Mute;
    impl RiveEditor for Mute {
        fn call(&mut self, _: &str, _: Value) -> Result<Value, RiveExportError> {
            Ok(serde_json::json!({}))
        }
    }
    let oracle: Value = serde_json::from_str(ORACLE).unwrap();
    let case = &oracle["cases"][0];
    let spec = spec_of(case);
    let history = NativeObjectHistory::new(
        "rive",
        decode_storage_project(&serde_json::to_vec(&case["document"]).unwrap()).unwrap(),
    )
    .unwrap();
    let snapshot = history
        .acquire_snapshot(history.content_revision())
        .unwrap();
    let plans: Vec<ObjectPaintPlan> = (spec.start..=spec.end)
        .map(|frame| plan(&snapshot, frame))
        .collect();
    assert!(matches!(
        export_rive(&plans, &spec, &mut Mute),
        Err(RiveExportError::Editor { .. })
    ));
    let mut short = spec.clone();
    short.end += 1;
    assert_eq!(
        export_rive(&plans, &short, &mut Mute).unwrap_err(),
        RiveExportError::InvalidRange
    );
}

#[test]
#[cfg(feature = "font_asset_admission")]
fn a_text_block_plans_as_its_vector_text_glyph_paths() {
    // buildGlyphPaths: each contour a closed path filled with the block colour, no stroke; exportRive lanes it
    // by its stroke id like any stroke. The text block must plan exactly as those records (the glyph key as id).
    let oracle: Value = serde_json::from_str(ORACLE).unwrap();
    let case = &oracle["cases"][0];
    let mut with_text = case["document"].clone();
    with_text["items"] = serde_json::json!({"text":[{"target":{"contextId":"scene-root","layerUid":"L0",
        "strokeId":"words","frameScope":{"kind":"authored","frame":0}},"block":{"text":"Hi o",
        "font":{"kind":"bundled","fontKey":"Roboto-Regular"},"size":48,"color":"#336699","align":"left",
        "anchorTopLeft":{"x":10,"y":20},"underline":true}}]});
    let push = |doc: &mut Value, stroke: &Value| {
        let scope = &mut doc["compositionOrder"]["contexts"][0]["scopes"][0]["elements"];
        let next = scope.as_array().unwrap().iter().filter(|e| e["layerUid"] == "L0").count();
        scope.as_array_mut().unwrap().push(serde_json::json!(
            {"layerUid":"L0","strokeId":stroke,"naturalPosition":next,"order":{}}));
    };
    push(&mut with_text, &serde_json::json!("words"));
    let load = |doc: &Value| {
        NativeObjectHistory::new("rive", decode_storage_project(&serde_json::to_vec(doc).unwrap()).unwrap())
            .unwrap()
    };
    let text_history = load(&with_text);
    let text_plan = plan(&text_history.acquire_snapshot(text_history.content_revision()).unwrap(), 0);
    let mut glyphs = case["document"].clone();
    let template = glyphs["objects"][0].clone();
    for entry in crate::object_render_scene::ordered_entries(&text_plan) {
        let crate::object_render_scene::PaintEntry::Text(text) = entry else { continue };
        for (contour, _) in text.item.painted() {
            let mut r = template.clone();
            r["target"] = serde_json::json!({"contextId":"scene-root","frameScope":{"kind":"authored","frame":0},
                "layerUid":"L0","strokeId":contour.key});
            r["geometry"]["closed"] = serde_json::json!(true);
            r["geometry"]["segments"] = serde_json::json!(contour.contour.segments().iter().map(|s| serde_json::json!({
                "point":{"x":s.point[0],"y":s.point[1]},"handleIn":{"x":s.handle_in[0],"y":s.handle_in[1]},
                "handleOut":{"x":s.handle_out[0],"y":s.handle_out[1]}})).collect::<Vec<_>>());
            r["fill"] = serde_json::json!({"kind":"solid","r":51.0/255.0,"g":102.0/255.0,"b":153.0/255.0,"a":1});
            r["vector"] = serde_json::json!({"stroke":null,"hasRealStroke":false,"strokeWidth":1,
                "strokeCap":"butt","strokeJoin":"miter","opacity":contour.opacity});
            push(&mut glyphs, &serde_json::json!(contour.key));
            glyphs["objects"].as_array_mut().unwrap().push(r);
        }
    }
    let glyph_history = load(&glyphs);
    let glyph_plan = plan(&glyph_history.acquire_snapshot(glyph_history.content_revision()).unwrap(), 0);
    let layers = text_plan.packet().source_layers().len();
    let planned = super::strokes_by_layer(&text_plan, layers).unwrap();
    assert!(planned[0].len() > 4, "the stroke and the glyph and underline contours");
    assert_eq!(planned, super::strokes_by_layer(&glyph_plan, layers).unwrap());
}

#[test]
#[cfg(feature = "project_media_codec")]
fn a_video_frame_plans_nothing_as_exportrive_never_sees_it() {
    let oracle: Value = serde_json::from_str(ORACLE).unwrap();
    let case = &oracle["cases"][0];
    let planned = |placed: bool| {
        let timing = serde_json::json!({"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing",
            "contextId":"scene-root","keys":[{"frame":0}],"samples":[]});
        let mut stroke = case["document"]["objects"][0].clone();
        stroke["target"]["layerUid"] = serde_json::json!("D");
        let placements = if placed {
            serde_json::json!([{"id":"p1","assetId":"v1","target":{"kind":"drawing","contextId":"scene-root",
                "layerUid":"V"},"offsetFrames":0}])
        } else {
            serde_json::json!([])
        };
        let project = serde_json::json!({"format":"nemo.native-project-document","formatVersion":2,
            "project":{"canvasW":320,"canvasH":240,"fps":24},
            "drawing":{"format":"nemo.native-object-document","formatVersion":2,"totalFrames":4,
                "layers":[{"layerUid":"D","name":"Ink","timing":timing},{"layerUid":"V","name":"Clip","timing":timing}],
                "objects":[stroke],
                "compositionOrder":{"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
                    "contexts":[{"contextId":"scene-root","layers":[
                        {"layerUid":"D","naturalPosition":0,"order":{}},{"layerUid":"V","naturalPosition":1,"order":{}}],
                    "scopes":[{"frameScope":{"kind":"authored","frame":0},
                        "elements":[{"layerUid":"D","strokeId":stroke["target"]["strokeId"],"naturalPosition":0,"order":{}}]}]}],
                    "migration":"explicit-plan-required","referencePolicy":"unresolved"}},
            "viewportReference":null,
            "assets":[{"id":"v1","name":"take-1","kind":"video","source":{"kind":"linked","locator":"clips/one.mov"}}],
            "assetInventory":{"assetIds":["v1"],"placements":placements}});
        let history =
            NativeObjectHistory::from_project_file("rive-video", project.to_string().as_bytes()).unwrap();
        let painted = plan(&history.acquire_snapshot(history.content_revision()).unwrap(), 0);
        if placed {
            assert!(crate::object_render_scene::ordered_entries(&painted)
                .any(|entry| matches!(entry, crate::object_render_scene::PaintEntry::Video(_))));
        }
        super::strokes_by_layer(&painted, 2).unwrap()
    };
    let with_video = planned(true);
    assert_eq!(with_video[0].len(), 1);
    assert_eq!(with_video, planned(false));
}
