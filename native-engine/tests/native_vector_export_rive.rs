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

#[test]
fn every_oracle_case_holds_the_baseline_conversation() {
    let oracle: Value = serde_json::from_str(ORACLE).unwrap();
    assert_eq!(oracle["commit"].as_str().unwrap().len(), 40);
    let cases = oracle["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 5);
    for case in cases {
        let id = case["id"].as_str().unwrap();
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
