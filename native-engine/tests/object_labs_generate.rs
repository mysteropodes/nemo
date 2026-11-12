//! U36 (#2362) Labs generators against the baseline oracle (fixtures/labs-generate-oracle.json, generated outside the
//! repository by gen-labs-generate-oracle.cjs from the 1065e9d8 speed-lines/boil-effect/follow-path sources run
//! verbatim, follow-path over the shipped Paper.js), through the real history owner and dispatcher, in the unordered
//! v1 and the ordered v2 document forms: one history entry, undo to the original bytes, redo, atomic refusals.
use super::*;
use crate::history::NativeObjectHistory;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::object_document::{ObjectRecord, Presence};
use serde_json::json;

#[path = "object_frame_edit_document_build.rs"]
#[allow(dead_code)]
mod build;
#[path = "object_exposure_edit_key_rows.rs"]
#[allow(dead_code)]
mod rows;

fn oracle() -> Value {
    serde_json::from_str(include_str!("fixtures/labs-generate-oracle.json")).unwrap()
}
/// Dense cells as native vector records (the fixture builder writes closed filled paths of the legacy family).
fn document(cells: &str, form: build::Build, edit: impl Fn(&mut Value)) -> NativeObjectHistory {
    let mut v = build::document_json(&rows::parse_layers(cells), form);
    for record in v["objects"].as_array_mut().unwrap() {
        record["family"] = json!("editable-vector-path");
        record["vector"] = json!({"stroke":{"kind":"solid","r":0,"g":0,"b":0,"a":1},"hasRealStroke":true,"strokeWidth":2,
            "strokeCap":"round","strokeJoin":"round","miterLimit":10,"dashOffset":0,"opacity":1});
    }
    edit(&mut v);
    let d = decode_storage_project(v.to_string().as_bytes()).unwrap_or_else(|e| panic!("{e}"));
    NativeObjectHistory::new("labs-generate", d).unwrap()
}
fn head(o: &NativeObjectHistory) -> ObjectSnapshot {
    o.acquire_snapshot(o.content_revision()).unwrap()
}
fn bytes(o: &NativeObjectHistory) -> Vec<u8> {
    encode_storage_project(head(o).document()).unwrap()
}
fn run(o: &mut NativeObjectHistory, payload: Value) -> Value {
    let command = payload["command"]
        .as_str()
        .unwrap_or("object.labs.speedLines");
    let known = OPS
        .iter()
        .any(|op| op.ends_with(&command["object.".len().min(command.len())..]));
    let operation = if known {
        format!("command.document.{command}")
    } else {
        OPS[0].to_owned()
    };
    let request = json!({"apiVersion":2,"requestId":format!("labs-{}", o.content_revision()),"instanceId":o.instance_id(),
        "documentId":o.document_id(),"expectedRevision":o.content_revision(),"operation":operation,"payload":payload});
    let typed = serde_json::from_value(request).unwrap();
    serde_json::to_value(o.try_dispatch_object_command(&typed).unwrap().unwrap()).unwrap()
}
/// The cell's records in natural stacking order: (stroke id, points, record).
fn cell(
    o: &NativeObjectHistory,
    layer: &str,
    frame: u32,
) -> Vec<(String, Vec<[f64; 2]>, ObjectRecord)> {
    let s = head(o);
    let d = s.document();
    let mut records: Vec<&ObjectRecord> = d
        .objects
        .iter()
        .filter(|r| {
            r.target.layer_uid == layer
                && r.target.frame_scope.frame == frame
                && r.target.context_id == CONTEXT
                && r.target.frame_scope.kind == crate::object_document::FrameScopeKind::Authored
        })
        .collect();
    if let Presence::Present(plan) = &d.composition_order {
        let position = |id: &str| {
            plan.contexts
                .iter()
                .flat_map(|c| &c.scopes)
                .filter(|scope| scope.frame_scope.frame == frame)
                .flat_map(|scope| &scope.elements)
                .find(|e| e.layer_uid == layer && e.stroke_id == id)
                .map(|e| e.natural_position)
                .unwrap()
        };
        records.sort_by_key(|r| position(&r.target.stroke_id));
    }
    records
        .into_iter()
        .map(|r| {
            let pts = r
                .geometry
                .segments
                .iter()
                .map(|g| [g.point.x.as_f64().unwrap(), g.point.y.as_f64().unwrap()])
                .collect();
            (r.target.stroke_id.clone(), pts, r.clone())
        })
        .collect()
}
fn exposure(o: &NativeObjectHistory, layer: &str, frame: u32) -> &'static str {
    let s = head(o);
    let timing = s
        .document()
        .layers
        .iter()
        .find(|l| l.layer_uid == layer)
        .unwrap()
        .timing
        .value()
        .unwrap()
        .clone();
    timing.exposure(frame).0
}
fn close(actual: &[[f64; 2]], expected: &Value, tolerance: f64, what: &str) {
    let expected = expected.as_array().unwrap();
    assert_eq!(actual.len(), expected.len(), "{what}");
    for (a, e) in actual.iter().zip(expected) {
        for axis in 0..2 {
            let want = e[axis].as_f64().unwrap();
            assert!(
                (a[axis] - want).abs() <= tolerance,
                "{what}: {} vs {want}",
                a[axis]
            );
        }
    }
}
/// One history entry; undo restores the original bytes and redo the generated ones.
fn round_trip(o: &mut NativeObjectHistory, before: &[u8]) {
    assert_eq!(o.history_depths(), (1, 0));
    let after = bytes(o);
    let undone = o.undo(o.content_revision()).unwrap();
    assert_eq!(bytes(o), before);
    o.redo(undone).unwrap();
    assert_eq!(bytes(o), after);
}
const FORMS: [build::Build; 2] = [build::Build::V1, build::Build::V2];

#[test]
fn speed_lines_reproduce_the_seeded_baseline_above_the_shown_drawing() {
    let oracle = oracle();
    for form in FORMS {
        for (case, frame) in oracle["speedLines"]
            .as_array()
            .unwrap()
            .iter()
            .zip([1u32, 0])
        {
            let mut o = document("K=a.1.10,b.2.30;.;.", form, |_| {});
            let before = bytes(&o);
            let opts = &case["opts"];
            let r1 = opts["r1"].as_f64().unwrap_or(1920f64.max(1080.0) * 0.75);
            let reply = run(
                &mut o,
                json!({"command":"object.labs.speedLines","layerUid":"L0","frame":frame,
                "cx":opts["cx"],"cy":opts["cy"],"count":opts["count"],"r0":opts["r0"].as_f64().unwrap_or(r1 * 0.45),"r1":r1,
                "jitter":opts["jitter"].as_f64().unwrap_or(0.35),"seed":case["seed"],"color":[1,0,0,1],"brushSize":4,"opacity":0.5}),
            );
            assert_eq!(reply["ok"], true, "{reply}");
            assert_eq!(
                exposure(&o, "L0", frame),
                "key",
                "a held cell becomes a key first (ensureKeyframe)"
            );
            let written = cell(&o, "L0", frame);
            let lines = case["lines"].as_array().unwrap();
            assert_eq!(written.len(), 2 + lines.len());
            assert_eq!(
                [written[0].0.as_str(), written[1].0.as_str()],
                ["a", "b"],
                "the shown drawing stays below"
            );
            for ((id, points, record), line) in written[2..].iter().zip(lines) {
                assert!(id.starts_with("native-stroke:"));
                close(points, &line["points"], 1e-9, "line");
                let body = record.vector.value().unwrap();
                assert!(
                    (body.stroke_width.as_f64().unwrap() - line["width"].as_f64().unwrap()).abs()
                        < 1e-12
                );
                assert_eq!(body.opacity.as_f64(), line["opacity"].as_f64());
            }
            round_trip(&mut o, &before);
        }
    }
}

#[test]
fn boil_writes_the_baseline_jittered_keys_and_grows_the_timeline() {
    let oracle = oracle();
    for form in FORMS {
        for case in oracle["boil"].as_array().unwrap() {
            let mut o = document("K=a.1.10,b.2.30;.;.", form, |_| {});
            let before = bytes(&o);
            let source = cell(&o, "L0", 0);
            let opts = &case["opts"];
            let reply = run(
                &mut o,
                json!({"command":"object.labs.boil","layerUid":"L0","frame":0,"frames":opts["frames"],
                "amplitude":opts["amplitude"],"seedStep":opts["seedStep"].as_i64().unwrap_or(1)}),
            );
            assert_eq!(reply["ok"], true, "{reply}");
            assert_eq!(
                u64::from(head(&o).document().total_frames),
                case["totalFrames"].as_u64().unwrap()
            );
            for expected in case["cells"].as_array().unwrap() {
                let frame = expected["frame"].as_u64().unwrap() as u32;
                assert_eq!(exposure(&o, "L0", frame), "key");
                let written = cell(&o, "L0", frame);
                for ((id, points, _), stroke) in
                    written.iter().zip(expected["strokes"].as_array().unwrap())
                {
                    assert_eq!(id, stroke["strokeId"].as_str().unwrap());
                    close(points, &stroke["points"], 1e-12, "jittered point");
                }
            }
            assert_eq!(
                cell(&o, "L0", 0)
                    .iter()
                    .map(|c| c.1.clone())
                    .collect::<Vec<_>>(),
                source.iter().map(|c| c.1.clone()).collect::<Vec<_>>(),
                "the source cell is unchanged"
            );
            round_trip(&mut o, &before);
        }
    }
}

#[test]
fn follow_path_bakes_the_independently_sampled_trajectory() {
    let oracle = oracle();
    for form in FORMS {
        for case in oracle["followPath"].as_array().unwrap() {
            let trajectory: Vec<Value> = case["trajectory"].as_array().unwrap().iter().map(|s| json!({
                "point":{"x":s[0],"y":s[1]},"handleIn":{"x":s[2],"y":s[3]},"handleOut":{"x":s[4],"y":s[5]}})).collect();
            let mut o = document("K=a.1.10,b.2.30,p.3.50;.;.", form, |v| {
                let p = v["objects"]
                    .as_array_mut()
                    .unwrap()
                    .iter_mut()
                    .find(|r| r["target"]["strokeId"] == "p")
                    .unwrap();
                p["geometry"]["closed"] = json!(false);
                p["geometry"]["segments"] = json!(trajectory);
            });
            let before = bytes(&o);
            let opts = &case["opts"];
            let reply = run(
                &mut o,
                json!({"command":"object.labs.followPath","layerUid":"L0","frame":0,"pathId":"p",
                "strokeIds":["a"],"frames":opts["frames"],"ease":opts["ease"],
                "removeTrajectory":opts["removeTrajectory"].as_bool().unwrap_or(true)}),
            );
            assert_eq!(reply["ok"], true, "{reply}");
            assert_eq!(
                u64::from(head(&o).document().total_frames),
                case["totalFrames"].as_u64().unwrap()
            );
            let mut fresh = std::collections::HashSet::new();
            for expected in case["cells"].as_array().unwrap() {
                let frame = expected["frame"].as_u64().unwrap() as u32;
                assert_eq!(exposure(&o, "L0", frame), "key");
                let written = cell(&o, "L0", frame);
                let strokes = expected["strokes"].as_array().unwrap();
                assert_eq!(written.len(), strokes.len(), "frame {frame}");
                for ((id, points, _), stroke) in written.iter().zip(strokes) {
                    if stroke["strokeId"] == "fresh" {
                        assert!(
                            id.starts_with("native-stroke:") && fresh.insert(id.clone()),
                            "a fresh id per copy and frame"
                        );
                        close(points, &stroke["points"], 1e-6, "moved copy");
                    } else {
                        assert_eq!(id, stroke["strokeId"].as_str().unwrap());
                        if id != "p" {
                            close(points, &stroke["points"], 0.0, "backdrop");
                        }
                    }
                }
            }
            round_trip(&mut o, &before);
        }
    }
}

#[test]
fn refusals_publish_nothing_and_add_no_history() {
    let speed = json!({"command":"object.labs.speedLines","layerUid":"L0","frame":0,"cx":0,"cy":0,"count":3,"r0":1,"r1":2,
        "jitter":0,"seed":1,"color":[0,0,0,1],"brushSize":1,"opacity":1});
    let with = |base: &Value, key: &str, value: Value| {
        let mut v = base.clone();
        v[key] = value;
        v
    };
    let boil = json!({"command":"object.labs.boil","layerUid":"L0","frame":0,"frames":2,"amplitude":1,"seedStep":1});
    let follow = json!({"command":"object.labs.followPath","layerUid":"L0","frame":0,"pathId":"b","strokeIds":["a"],
        "frames":2,"ease":"linear","removeTrajectory":true});
    let cases = [
        (with(&speed, "layerUid", json!("L9")), "not_found"),
        (with(&speed, "count", json!(401)), "invalid_request"),
        (with(&speed, "extra", json!(1)), "invalid_request"),
        (with(&speed, "frame", json!(3)), "invalid_request"),
        (with(&boil, "frame", json!(2)), "not_found"),
        (with(&boil, "frames", json!(25)), "invalid_request"),
        (with(&follow, "strokeIds", json!(["b"])), "invalid_request"),
        (with(&follow, "strokeIds", json!(["z"])), "not_found"),
        (with(&follow, "ease", json!("bounce")), "invalid_request"),
        (
            with(&follow, "command", json!("object.labs.unknown")),
            "invalid_request",
        ),
    ];
    for (payload, code) in cases {
        let mut o = document("K=a.1.10,b.2.30;.;.|K=c.1.10;.;.", build::Build::V2, |v| {
            v["layers"][0]["timing"]["keys"] = json!([{"frame":0},{"frame":2}]);
        });
        let before = bytes(&o);
        let reply = run(&mut o, payload.clone());
        assert_eq!(reply["ok"], false, "{payload}");
        assert_eq!(reply["error"]["code"], code, "{payload}");
        assert_eq!(
            (bytes(&o), o.history_depths()),
            (before, (0, 0)),
            "{payload}"
        );
    }
    let mut locked = document("K=a.1.10;.;.", build::Build::V1, |v| {
        v["layers"][0]["locked"] = json!(true);
    });
    let before = bytes(&locked);
    assert_eq!(run(&mut locked, speed)["error"]["code"], "unavailable");
    assert_eq!(bytes(&locked), before);
}
