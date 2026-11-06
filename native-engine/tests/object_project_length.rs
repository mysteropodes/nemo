//! Project length through the real history owner: the Frames field and Trim to Work Area (timeline.js
//! `setTotalFrames` / `trimToWorkArea` at 1065e9d8). Expected values are hand-derived from the baseline rules.
use super::*;
use serde_json::{json, Value};

#[path = "object_frame_edit_document_build.rs"]
#[allow(dead_code)]
mod build;
#[path = "object_exposure_edit_key_rows.rs"]
#[allow(dead_code)]
mod rows;

/// Six frames, two layers: L0 has strokes at frames 0 and 3, L1 is a bare key at frame 0.
const SIX: &str = "K=a.1.10;.;.;K=b.2.20;.;.|K;.;.;.;.;.";
/// Ten frames: L0 has strokes at frames 0, 3 and 7, L1 a key at 0.
const TEN: &str = "K=a.1.10;.;.;K=b.2.20;.;.;.;K=c.3.30;.;.|K;.;.;.;.;.;.;.;.;.";

fn drawing(text: &str) -> Value {
    let mut v = build::document_json(&rows::parse_layers(text), build::Build::V2Extras);
    v["propertyHolders"] = json!([{"target":{"contextId":"scene-root","layerUid":"L0"},
        "properties":[{"property":"rotation","static":[9],
            "track":{"keys":[{"frame":5,"v":[13],"hold":true,"curvePoints":[],"hIn":[0,0],"hOut":[0,0]}]}}]}]);
    v
}
fn project(text: &str, project: Value, tweak: impl FnOnce(&mut Value)) -> NativeObjectHistory {
    let mut drawing = drawing(text);
    tweak(&mut drawing);
    let wire = json!({"format":"nemo.native-project-document","formatVersion":1,"project":project,
        "drawing":drawing,"viewportReference":null,"assets":[]});
    NativeObjectHistory::from_project_file("length-test", wire.to_string().as_bytes()).unwrap()
}
fn plain(w_in: Option<f64>, w_out: Option<f64>) -> Value {
    let mut p = json!({"canvasW":320,"canvasH":180,"fps":24});
    if let Some(v) = w_in {
        p["waIn"] = json!(v);
    }
    if let Some(v) = w_out {
        p["waOut"] = json!(v);
    }
    p
}
fn wire(o: &NativeObjectHistory) -> Value {
    let snapshot = o.acquire_snapshot(o.content_revision()).unwrap();
    serde_json::from_slice(
        &o.serialize_project_file(o.content_revision(), snapshot.snapshot_id())
            .unwrap(),
    )
    .unwrap()
}
fn apply(o: &mut NativeObjectHistory, action: Value) -> Result<u64, DispatchErrorCode> {
    let bytes = json!({"command":"object.project.settings.edit","action":action}).to_string();
    let candidate = o.prepare_project_settings_json(bytes.as_bytes())?;
    o.commit_project_settings(candidate)
}
fn frames_of(v: &Value, layer: usize) -> Vec<u64> {
    v["drawing"]["layers"][layer]["timing"]["keys"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k["frame"].as_u64().unwrap())
        .collect()
}
fn object_frames(v: &Value) -> Vec<(String, u64)> {
    let mut out: Vec<(String, u64)> = v["drawing"]["objects"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| {
            r["target"]["contextId"] == "scene-root" && r["target"]["frameScope"]["kind"] == "authored"
        })
        .map(|r| {
            (
                r["target"]["strokeId"].as_str().unwrap().to_string(),
                r["target"]["frameScope"]["frame"].as_u64().unwrap(),
            )
        })
        .collect();
    out.sort();
    out
}
fn key_frame(v: &Value) -> Value {
    v["drawing"]["propertyHolders"][0]["properties"][0]["track"]["keys"][0]["frame"].clone()
}

#[test]
fn growing_lengthens_the_timeline_keeps_cells_and_undo_redo_and_reopen_agree() {
    let mut o = project(SIX, plain(Some(1.), Some(4.)), |_| {});
    let before = wire(&o);
    assert_eq!(apply(&mut o, json!({"kind":"setTotalFrames","frames":9})).unwrap(), 1);
    let grown = wire(&o);
    assert_eq!(grown["drawing"]["totalFrames"], json!(9));
    assert_eq!(frames_of(&grown, 0), vec![0, 3]);
    assert_eq!(object_frames(&grown), object_frames(&before));
    // Growing never moves the work area (baseline: only waOut >= v is clamped).
    assert_eq!((grown["project"]["waIn"].clone(), grown["project"]["waOut"].clone()), (json!(1.), json!(4.)));
    assert_eq!(o.history_depths(), (1, 0));
    let reopened =
        NativeObjectHistory::from_project_file("reopened", grown.to_string().as_bytes()).unwrap();
    assert_eq!(wire(&reopened), grown);
    assert_eq!(o.undo(1).unwrap(), 2);
    assert_eq!(wire(&o), before);
    assert_eq!(o.redo(2).unwrap(), 3);
    assert_eq!(wire(&o), grown);
}

#[test]
fn shrinking_drops_cells_past_the_end_and_clamps_the_work_area_like_the_baseline() {
    let mut o = project(SIX, plain(Some(1.), Some(5.)), |v| {
        v["layers"][0]["range"] = json!({"inPoint":2,"outPoint":5});
    });
    let before = wire(&o);
    assert_eq!(apply(&mut o, json!({"kind":"setTotalFrames","frames":3})).unwrap(), 1);
    let after = wire(&o);
    assert_eq!(after["drawing"]["totalFrames"], json!(3));
    assert_eq!(frames_of(&after, 0), vec![0]);
    assert_eq!(object_frames(&after), vec![("a".to_string(), 0)]);
    // waOut 5 >= 3 -> 2; waIn 1 < 2 stays. The layer range is not touched (its reader clamps it).
    assert_eq!((after["project"]["waIn"].clone(), after["project"]["waOut"].clone()), (json!(1.), json!(2.)));
    assert_eq!(after["drawing"]["layers"][0]["range"], json!({"inPoint":2,"outPoint":5}));
    // Motion tracks are not rewritten by the Frames field.
    assert_eq!(key_frame(&after), json!(5));
    assert_eq!(o.undo(1).unwrap(), 2);
    assert_eq!(wire(&o), before);
    assert_eq!(o.redo(2).unwrap(), 3);
    assert_eq!(wire(&o), after);
    let reopened =
        NativeObjectHistory::from_project_file("reopened", after.to_string().as_bytes()).unwrap();
    assert_eq!(wire(&reopened), after);
}

#[test]
fn an_inverted_start_is_pulled_below_the_end_after_the_end_is_clamped() {
    let mut o = project(SIX, plain(Some(5.), Some(5.)), |_| {});
    apply(&mut o, json!({"kind":"setTotalFrames","frames":3})).unwrap();
    let after = wire(&o);
    // waOut 5 -> 2, then waIn 5 >= 2 -> max(0, 2 - 1) = 1.
    assert_eq!((after["project"]["waIn"].clone(), after["project"]["waOut"].clone()), (json!(1.), json!(2.)));
    // An unset work area stays unset: storage absence is not invented.
    let mut bare = project(SIX, plain(None, None), |_| {});
    apply(&mut bare, json!({"kind":"setTotalFrames","frames":3})).unwrap();
    assert!(wire(&bare)["project"].get("waIn").is_none());
    assert!(wire(&bare)["project"].get("waOut").is_none());
}

#[test]
fn the_field_clamps_to_one_through_999_and_the_same_length_is_a_no_op() {
    let mut o = project(SIX, plain(None, None), |_| {});
    apply(&mut o, json!({"kind":"setTotalFrames","frames":0})).unwrap();
    assert_eq!(wire(&o)["drawing"]["totalFrames"], json!(1));
    assert_eq!(frames_of(&wire(&o), 0), vec![0]);
    apply(&mut o, json!({"kind":"setTotalFrames","frames":5000})).unwrap();
    assert_eq!(wire(&o)["drawing"]["totalFrames"], json!(999));
    apply(&mut o, json!({"kind":"setTotalFrames","frames":12.9})).unwrap();
    assert_eq!(wire(&o)["drawing"]["totalFrames"], json!(12));
    let (revision, depths, bytes) = (o.content_revision(), o.history_depths(), wire(&o));
    assert_eq!(apply(&mut o, json!({"kind":"setTotalFrames","frames":12})).unwrap(), revision);
    assert_eq!((o.content_revision(), o.history_depths(), wire(&o)), (revision, depths, bytes));
}

#[test]
fn malformed_length_requests_are_atomic_refusals() {
    let mut o = project(SIX, plain(None, None), |_| {});
    let before = wire(&o);
    for action in [
        json!({"kind":"setTotalFrames"}),
        json!({"kind":"setTotalFrames","frames":"9"}),
        json!({"kind":"setTotalFrames","frames":null}),
        json!({"kind":"setTotalFrames","totalFrames":9}),
        json!({"kind":"setTotalFrames","frames":9,"extra":1}),
        json!({"kind":"trimToWorkArea","frames":9}),
    ] {
        assert_eq!(apply(&mut o, action), Err(DispatchErrorCode::InvalidRequest));
        assert_eq!((wire(&o), o.history_depths()), (before.clone(), (0, 0)));
    }
}

fn trimmed_fixture() -> NativeObjectHistory {
    let mut p = plain(Some(2.), Some(6.));
    p["markers"] = json!([{"frame":0,"name":"early"},{"frame":3,"name":"mid","color":"#f00"},{"frame":9,"name":"late"}]);
    p["cameraKeys"] = json!([{"frame":1,"x":0,"y":0,"w":10},{"frame":2,"x":1,"y":2,"w":10},{"frame":5,"x":3,"y":4,"w":10},{"frame":8,"x":5,"y":6,"w":10}]);
    project(TEN, p, |v| {
        v["layers"][0]["range"] = json!({"inPoint":1,"outPoint":8,"markers":[{"frame":1,"name":"a"},{"frame":4,"name":"b"},{"frame":9,"name":"c"}]});
        v["layers"][1]["range"] = json!({"inPoint":4,"outPoint":5});
    })
}

#[test]
fn trim_keeps_the_work_area_and_rebases_content_ranges_markers_camera_and_motion() {
    let mut o = trimmed_fixture();
    let before = wire(&o);
    assert_eq!(apply(&mut o, json!({"kind":"trimToWorkArea"})).unwrap(), 1);
    let after = wire(&o);
    // Work area 2..=6 keeps five frames.
    assert_eq!(after["drawing"]["totalFrames"], json!(5));
    assert_eq!((after["project"]["waIn"].clone(), after["project"]["waOut"].clone()), (json!(0.), json!(4.)));
    // Cells: frame 3 -> 1 survives; frames 0 and 7 are outside the work area. Hold frame 2 had no key of its own.
    assert_eq!(object_frames(&after), vec![("b".to_string(), 1)]);
    assert_eq!(frames_of(&after, 0), vec![1]);
    assert_eq!(frames_of(&after, 1), Vec::<u64>::new());
    // Layer ranges: in max(0, 1-2) = 0, out min(4, 8-2) = 4; markers keep 0..5 after the shift only.
    assert_eq!(after["drawing"]["layers"][0]["range"],
        json!({"inPoint":0,"outPoint":4,"markers":[{"frame":2,"name":"b"}]}));
    assert_eq!(after["drawing"]["layers"][1]["range"], json!({"inPoint":2,"outPoint":3}));
    // Composition markers and camera keys: only those inside the work area survive, shifted.
    assert_eq!(after["project"]["markers"], json!([{"frame":1.0,"name":"mid","color":"#f00"}]));
    let keys: Vec<(f64, f64)> = after["project"]["cameraKeys"].as_array().unwrap().iter()
        .map(|k| (k["frame"].as_f64().unwrap(), k["x"].as_f64().unwrap())).collect();
    assert_eq!(keys, vec![(0.0, 1.0), (3.0, 3.0)]);
    // Motion key 5 re-bases to 3.
    assert_eq!(key_frame(&after), json!(3.0));
    assert_eq!(o.history_depths(), (1, 0));
    assert_eq!(o.undo(1).unwrap(), 2);
    assert_eq!(wire(&o), before);
    assert_eq!(o.redo(2).unwrap(), 3);
    assert_eq!(wire(&o), after);
    let reopened =
        NativeObjectHistory::from_project_file("reopened", after.to_string().as_bytes()).unwrap();
    assert_eq!(wire(&reopened), after);
}

#[test]
fn trim_with_a_work_area_that_covers_everything_or_is_too_short_changes_nothing() {
    for (w_in, w_out) in [(None, None), (Some(0.), Some(9.)), (Some(4.), Some(4.)), (Some(6.), Some(2.))] {
        let mut o = project(TEN, plain(w_in, w_out), |_| {});
        let (before, revision) = (wire(&o), o.content_revision());
        assert_eq!(apply(&mut o, json!({"kind":"trimToWorkArea"})).unwrap(), revision);
        assert_eq!((wire(&o), o.history_depths()), (before, (0, 0)));
    }
}

#[test]
fn trim_to_a_leading_work_area_needs_no_rebase_and_keeps_the_start() {
    let mut o = project(TEN, plain(Some(0.), Some(4.)), |_| {});
    apply(&mut o, json!({"kind":"trimToWorkArea"})).unwrap();
    let after = wire(&o);
    assert_eq!(after["drawing"]["totalFrames"], json!(5));
    assert_eq!(object_frames(&after), vec![("a".to_string(), 0), ("b".to_string(), 3)]);
    assert_eq!(key_frame(&after), json!(5));
    assert_eq!((after["project"]["waIn"].clone(), after["project"]["waOut"].clone()), (json!(0.), json!(4.)));
}

fn all_cells_inside(v: &Value) {
    let total = v["drawing"]["totalFrames"].as_u64().unwrap();
    assert!(object_frames(v).iter().all(|(_, frame)| *frame < total));
    for layer in v["drawing"]["layers"].as_array().unwrap() {
        for list in ["keys", "samples"] {
            for entry in layer["timing"][list].as_array().unwrap() {
                assert!(entry["frame"].as_u64().unwrap() < total);
            }
        }
    }
    for scope in v["drawing"]["compositionOrder"]["contexts"][0]["scopes"].as_array().unwrap() {
        assert!(scope["frameScope"]["frame"].as_u64().unwrap() < total);
    }
}

#[test]
fn shrinking_then_growing_restores_every_cell_through_undo_redo_and_reopen() {
    let mut o = project(TEN, plain(None, None), |_| {});
    let full = wire(&o);
    assert!(full["project"].get("retainedTail").is_none());
    assert_eq!(apply(&mut o, json!({"kind":"setTotalFrames","frames":3})).unwrap(), 1);
    let small = wire(&o);
    // Nothing past the new end stays in the drawing, so nothing past it can render, export or play ...
    all_cells_inside(&small);
    assert_eq!(object_frames(&small), vec![("a".to_string(), 0)]);
    // ... the cut cells wait in the project, and they survive save and reopen while the timeline is short.
    assert!(small["project"].get("retainedTail").is_some());
    let reopened = NativeObjectHistory::from_project_file("short", small.to_string().as_bytes()).unwrap();
    assert_eq!(wire(&reopened), small);
    let mut reopened = reopened;
    apply(&mut reopened, json!({"kind":"setTotalFrames","frames":10})).unwrap();
    assert_eq!(wire(&reopened)["drawing"], full["drawing"]);
    // Growing brings back every cell: the whole drawing is the one it was before the shrink.
    assert_eq!(apply(&mut o, json!({"kind":"setTotalFrames","frames":10})).unwrap(), 2);
    let again = wire(&o);
    assert_eq!(again["drawing"], full["drawing"]);
    assert_eq!(object_frames(&again), vec![("a".into(), 0), ("b".into(), 3), ("c".into(), 7)]);
    assert_eq!(frames_of(&again, 0), vec![0, 3, 7]);
    assert!(again["project"].get("retainedTail").is_none());
    // Undo and redo move the length and the kept cells together.
    assert_eq!(o.undo(2).unwrap(), 3);
    assert_eq!(wire(&o), small);
    assert_eq!(o.undo(3).unwrap(), 4);
    assert_eq!(wire(&o), full);
    assert_eq!(o.redo(4).unwrap(), 5);
    assert_eq!(wire(&o), small);
    assert_eq!(o.redo(5).unwrap(), 6);
    assert_eq!(wire(&o), again);
    assert_eq!(o.history_depths(), (2, 0));
}

#[test]
fn a_partial_grow_returns_the_nearest_cells_and_a_second_shrink_keeps_the_older_ones() {
    let mut o = project(TEN, plain(None, None), |_| {});
    let full = wire(&o);
    apply(&mut o, json!({"kind":"setTotalFrames","frames":3})).unwrap();
    apply(&mut o, json!({"kind":"setTotalFrames","frames":5})).unwrap();
    let five = wire(&o);
    // Frames 3 and 4 return (cell b at 3); frame 7 is still kept, 2 frames past the new end.
    assert_eq!(object_frames(&five), vec![("a".into(), 0), ("b".into(), 3)]);
    all_cells_inside(&five);
    assert!(five["project"].get("retainedTail").is_some());
    // Shrinking again keeps the new cut in front of the older one: 5 -> 4 sets b aside, c stays behind it.
    apply(&mut o, json!({"kind":"setTotalFrames","frames":3})).unwrap();
    apply(&mut o, json!({"kind":"setTotalFrames","frames":10})).unwrap();
    assert_eq!(wire(&o)["drawing"], full["drawing"]);
    // 10 -> 6 -> 2 -> 10 passes through two cuts and returns the same drawing.
    apply(&mut o, json!({"kind":"setTotalFrames","frames":6})).unwrap();
    apply(&mut o, json!({"kind":"setTotalFrames","frames":2})).unwrap();
    all_cells_inside(&wire(&o));
    apply(&mut o, json!({"kind":"setTotalFrames","frames":10})).unwrap();
    assert_eq!(wire(&o)["drawing"], full["drawing"]);
}

#[test]
fn trim_to_the_work_area_really_cuts_and_forgets_the_kept_cells() {
    let mut o = project(TEN, plain(Some(2.), Some(6.)), |_| {});
    apply(&mut o, json!({"kind":"setTotalFrames","frames":7})).unwrap();
    assert!(wire(&o)["project"].get("retainedTail").is_some());
    apply(&mut o, json!({"kind":"trimToWorkArea"})).unwrap();
    let trimmed = wire(&o);
    assert_eq!(trimmed["drawing"]["totalFrames"], json!(5));
    assert!(trimmed["project"].get("retainedTail").is_none());
    apply(&mut o, json!({"kind":"setTotalFrames","frames":10})).unwrap();
    assert_eq!(object_frames(&wire(&o)), vec![("b".to_string(), 1)]);
}

#[test]
fn a_malformed_kept_tail_is_refused_when_the_project_is_read() {
    let mut o = project(TEN, plain(None, None), |_| {});
    apply(&mut o, json!({"kind":"setTotalFrames","frames":3})).unwrap();
    let good = wire(&o);
    let reopen = |tail: Value| {
        let mut v = good.clone();
        v["project"]["retainedTail"] = tail;
        NativeObjectHistory::from_project_file("bad", v.to_string().as_bytes())
    };
    let mut doubled = good["project"]["retainedTail"].clone();
    let layers = doubled["layers"].as_array().unwrap().clone();
    doubled["layers"] = json!([layers[0].clone(), layers[0].clone()]);
    assert!(reopen(doubled).is_err());
    assert!(reopen(json!({"layers":[{"layerUid":"L0","timing":{"keys":[{"frame":5},{"frame":2}],"samples":[]}}]})).is_err());
    assert!(reopen(json!({"layers":[{"layerUid":"L0","timing":{"keys":[{"frame":4000}],"samples":[]}}]})).is_err());
    assert!(reopen(json!({"unknown":[]})).is_err());
}

#[test]
fn a_component_longer_than_the_project_does_not_stop_the_frames_field() {
    let long = project(SIX, plain(None, None), |v| {
        let inner = build::record_json("symbol", "authored", "inside", "inner", 20, 3, 30);
        v["componentTables"] = json!({"components":{
            "format":"nemo.native-component-set","formatVersion":1,
            "components":[{"symbolId":"symbol","name":"long","totalFrames":30,
                "layers":[{"layerUid":"inside"}],"objects":[inner]}],
            "instances":[{"contextId":"scene-root","layerUid":"L1","symbolId":"symbol",
                "symSpeed":1,"symPlacedAt":0,"symMatrix":[1,0,0,1,0,0],
                "frameKeys":[{"frame":0,"componentFrame":0}]}]
        }});
        // The component's own explicit order plan reaches frame 20, far past the project's last frame.
        v["compositionOrder"]["contexts"].as_array_mut().unwrap().push(json!({"contextId":"symbol",
            "layers":[{"layerUid":"inside","naturalPosition":0,"order":{}}],
            "scopes":[{"frameScope":{"kind":"authored","frame":20},
                "elements":[{"layerUid":"inside","strokeId":"inner","naturalPosition":0,"order":{}}]}]}));
    });
    let mut o = long;
    let before = wire(&o);
    assert_eq!(apply(&mut o, json!({"kind":"setTotalFrames","frames":3})).unwrap(), 1);
    let short = wire(&o);
    assert_eq!(short["drawing"]["totalFrames"], json!(3));
    // The component keeps its own 30 frames and the record at frame 20.
    assert_eq!(short["drawing"]["componentTables"], before["drawing"]["componentTables"]);
    apply(&mut o, json!({"kind":"setTotalFrames","frames":6})).unwrap();
    assert_eq!(wire(&o)["drawing"], before["drawing"]);
}
