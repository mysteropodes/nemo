//! #2256: the enter/exit component-context transition over the resolver's real admitted worlds. Expected frame counts and
//! tab behaviour are read from the world JSON and from the baseline rules (`enterSymbol` / `exitToScene` / `closeSymbolTab`),
//! never from the code under test.
use super::*;
use crate::evaluation::object_component_kernels::object_context_resolver::world::json as world_json;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::request_receipts::DispatchErrorCode;

fn snapshot_from(value: &Value) -> ObjectSnapshot {
    let document = decode_storage_project(value.to_string().as_bytes()).expect("admitted world");
    ObjectSnapshot::new("ws-instance", document).expect("snapshot")
}
fn snapshot() -> ObjectSnapshot {
    snapshot_from(&serde_json::from_str(world_json("D")).unwrap())
}
fn request(snapshot: &ObjectSnapshot, workspace: Value, action: Value) -> OpacityRequest {
    serde_json::from_value(json!({"apiVersion":2,"requestId":"workspace-context",
        "instanceId":snapshot.instance_id(),"documentId":snapshot.document_id(),
        "operation":"query.document.object.workspace.context",
        "payload":{"atRevision":snapshot.content_revision(),
            "documentSnapshotId":snapshot.snapshot_id(),"workspace":workspace,"action":action}}))
    .unwrap()
}
fn ask(snapshot: &ObjectSnapshot, workspace: Value, action: Value) -> ResponseEnvelope {
    let response = snapshot
        .dispatch(request(snapshot, workspace, action))
        .unwrap();
    assert!(serde_json::to_vec(&response).unwrap().len() <= 4096);
    response
}
fn answer(snapshot: &ObjectSnapshot, workspace: Value, action: Value) -> Value {
    let response = ask(snapshot, workspace, action);
    assert!(response.ok, "{:?}", response.error);
    response.result.unwrap()
}
fn refused(snapshot: &ObjectSnapshot, workspace: Value, action: Value) -> DispatchErrorCode {
    let response = ask(snapshot, workspace, action);
    assert!(!response.ok && response.result.is_none());
    response.error.unwrap().code()
}
fn view(frame: u64, work_in: u64, work_out: u64) -> Value {
    json!({"frame":frame,"workIn":work_in,"workOut":work_out})
}
fn scene(frame: u64, tabs: Value) -> Value {
    json!({"activeContextId":null,"openTabs":tabs,"view":view(frame, 0, 23),"sceneView":null})
}
fn inside(id: &str, tabs: Value, frame: u64, scene_frame: u64) -> Value {
    json!({"activeContextId":id,"openTabs":tabs,"view":view(frame, 0, 11),"sceneView":view(scene_frame, 3, 20)})
}
fn enter(id: &str) -> Value {
    json!({"kind":"enter","contextId":id,"entryFrame":null})
}

#[test]
fn entering_a_declared_component_resets_the_playhead_and_work_area_and_saves_the_scene_view() {
    let snap = snapshot();
    let doc: Value = serde_json::from_str(world_json("D")).unwrap();
    let component = &doc["componentTables"]["components"]["components"][0];
    let frames = component["totalFrames"].as_u64().unwrap();
    let out = answer(&snap, scene(5, json!([])), enter("sym_comp"));
    assert_eq!(out["atRevision"], snap.content_revision());
    assert_eq!(out["documentSnapshotId"], snap.snapshot_id());
    assert_eq!(out["outcome"], "entered");
    assert_eq!(out["contextId"], "sym_comp");
    assert_eq!(out["totalFrames"], frames);
    assert_eq!(out["fps"].as_f64(), component["fps"].as_f64());
    assert_eq!(out["workspace"]["activeContextId"], "sym_comp");
    assert_eq!(out["workspace"]["openTabs"], json!(["sym_comp"]));
    assert_eq!(out["workspace"]["view"], view(0, 0, frames - 1));
    // The scene playhead and work area are saved exactly, whatever the scene's own length is.
    assert_eq!(out["workspace"]["sceneView"], view(5, 0, 23));
}

#[test]
fn the_motion_entry_frame_is_clamped_into_the_component() {
    let snap = snapshot();
    let at = |entry: Value| {
        answer(
            &snap,
            scene(2, json!([])),
            json!({"kind":"enter","contextId":"sym_comp","entryFrame":entry}),
        )["workspace"]["view"]["frame"]
            .clone()
    };
    assert_eq!(at(json!(3)), 3);
    assert_eq!(
        at(json!(99)),
        11,
        "Math.max(0, Math.min(totalFrames - 1, target))"
    );
    assert_eq!(at(json!(0)), 0);
}

#[test]
fn entering_the_active_context_is_a_noop_and_another_one_is_refused_as_nested() {
    let snap = snapshot();
    let held = inside("sym_comp", json!(["sym_comp"]), 4, 5);
    let same = answer(&snap, held.clone(), enter("sym_comp"));
    assert_eq!(same["outcome"], "already_active");
    assert_eq!(same["workspace"], held, "the state is untouched");
    let nested = answer(&snap, held.clone(), enter("sym_comp_b"));
    assert_eq!(nested["outcome"], "refused_nested");
    assert_eq!(nested["workspace"], held);
    assert_eq!(nested["contextId"], "sym_comp");
}

#[test]
fn an_undeclared_context_and_the_scene_itself_are_not_found() {
    let snap = snapshot();
    for id in ["sym_unknown", "scene-root", "native-component:9:9"] {
        assert_eq!(
            refused(&snap, scene(0, json!([])), enter(id)),
            DispatchErrorCode::NotFound,
            "{id}"
        );
    }
}

#[test]
fn exit_restores_the_saved_scene_view_exactly_and_keeps_the_tab_open() {
    let snap = snapshot();
    let out = answer(
        &snap,
        inside("sym_comp", json!(["sym_comp"]), 9, 7),
        json!({"kind":"exit"}),
    );
    assert_eq!(out["outcome"], "exited");
    assert_eq!(out["contextId"], "scene-root");
    assert_eq!(out["totalFrames"], 24);
    assert_eq!(out["workspace"]["activeContextId"], Value::Null);
    assert_eq!(out["workspace"]["sceneView"], Value::Null);
    assert_eq!(out["workspace"]["view"], view(7, 3, 20));
    assert_eq!(out["workspace"]["openTabs"], json!(["sym_comp"]));
    let again = answer(&snap, scene(7, json!(["sym_comp"])), json!({"kind":"exit"}));
    assert_eq!(again["outcome"], "not_in_context");
}

#[test]
fn closing_the_active_tab_exits_first_and_closing_another_only_drops_it() {
    let snap = snapshot();
    let both = json!(["sym_comp", "sym_comp_b"]);
    let active = answer(
        &snap,
        inside("sym_comp", both.clone(), 2, 6),
        json!({"kind":"close","contextId":"sym_comp"}),
    );
    assert_eq!(active["outcome"], "closed");
    assert_eq!(active["workspace"]["activeContextId"], Value::Null);
    assert_eq!(active["workspace"]["openTabs"], json!(["sym_comp_b"]));
    assert_eq!(active["workspace"]["view"], view(6, 3, 20));
    let other = answer(
        &snap,
        inside("sym_comp", both, 2, 6),
        json!({"kind":"close","contextId":"sym_comp_b"}),
    );
    assert_eq!(other["outcome"], "closed");
    assert_eq!(other["workspace"]["activeContextId"], "sym_comp");
    assert_eq!(other["workspace"]["openTabs"], json!(["sym_comp"]));
    let none = answer(
        &snap,
        scene(0, json!(["sym_comp"])),
        json!({"kind":"close","contextId":"sym_comp_b"}),
    );
    assert_eq!(none["outcome"], "close_noop");
    assert_eq!(none["workspace"]["openTabs"], json!(["sym_comp"]));
}

#[test]
fn a_context_that_vanished_is_exited_by_refresh_and_its_tab_is_hidden() {
    let snap = snapshot();
    let held = inside("sym_deleted", json!(["sym_comp", "sym_deleted"]), 1, 8);
    let out = answer(&snap, held, json!({"kind":"refresh"}));
    assert_eq!(out["outcome"], "context_gone");
    assert_eq!(out["contextId"], "scene-root");
    assert_eq!(out["workspace"]["activeContextId"], Value::Null);
    assert_eq!(out["workspace"]["view"], view(8, 3, 20));
    assert_eq!(
        out["workspace"]["openTabs"],
        json!(["sym_comp"]),
        "only tabs whose context still exists are listed"
    );
    let fine = answer(
        &snap,
        inside("sym_comp", json!(["sym_comp"]), 1, 8),
        json!({"kind":"refresh"}),
    );
    assert_eq!(fine["outcome"], "refreshed");
    assert_eq!(fine["contextId"], "sym_comp");
}

#[test]
fn the_component_fps_wins_and_a_missing_fps_falls_back_to_the_scene() {
    let mut doc: Value = serde_json::from_str(world_json("D")).unwrap();
    doc["componentTables"]["components"]["components"][0]["fps"] = json!(12);
    doc["componentTables"]["components"]["components"][1]
        .as_object_mut()
        .unwrap()
        .remove("fps");
    let snap = snapshot_from(&doc);
    assert_eq!(
        answer(&snap, scene(0, json!([])), enter("sym_comp"))["fps"],
        12.0
    );
    // No project envelope in the world: the scene falls back to 24 fps like the baseline default.
    assert_eq!(
        answer(&snap, scene(0, json!([])), enter("sym_comp_b"))["fps"],
        24.0
    );
}

#[test]
fn only_this_snapshot_is_answered_and_a_malformed_request_is_invalid() {
    let snap = snapshot();
    let mut stale = request(&snap, scene(0, json!([])), enter("sym_comp"));
    stale.payload["atRevision"] = json!(snap.content_revision() + 1);
    let response = snap.dispatch(stale).unwrap();
    assert!(!response.ok);
    assert_eq!(
        response.error.unwrap().code(),
        DispatchErrorCode::StaleRevision
    );
    let mut other = request(&snap, scene(0, json!([])), enter("sym_comp"));
    other.payload["documentSnapshotId"] = json!("native-object:other:0");
    assert_eq!(
        snap.dispatch(other).unwrap().error.unwrap().code(),
        DispatchErrorCode::StaleRevision
    );
    let bad = |workspace: Value, action: Value| refused(&snap, workspace, action);
    let nine: Vec<String> = (0..9).map(|n| format!("t{n}")).collect();
    for (workspace, action) in [
        (scene(0, json!(nine)), enter("sym_comp")),
        (scene(0, json!(["sym_comp", "sym_comp"])), enter("sym_comp")),
        (
            json!({"activeContextId":"sym_comp","openTabs":["sym_comp"],"view":view(0, 0, 11),"sceneView":null}),
            json!({"kind":"exit"}),
        ),
        (
            json!({"activeContextId":"sym_comp","openTabs":["sym_comp"],"view":view(0, 5, 2),"sceneView":view(0, 0, 23)}),
            json!({"kind":"exit"}),
        ),
        (inside("sym_comp", json!([]), 0, 0), json!({"kind":"exit"})),
        (
            scene(0, json!([])),
            json!({"kind":"enter","contextId":"sym_comp"}),
        ),
        (scene(0, json!([])), json!({"kind":"teleport"})),
        (
            scene(0, json!([])),
            json!({"kind":"exit","contextId":"sym_comp"}),
        ),
    ] {
        assert_eq!(
            bad(workspace.clone(), action.clone()),
            DispatchErrorCode::InvalidRequest,
            "{workspace} {action}"
        );
    }
}

#[test]
fn a_transition_is_pure_it_changes_no_document_revision_or_byte() {
    let snap = snapshot();
    let before = encode_storage_project(snap.document()).unwrap();
    let revision = snap.content_revision();
    let held = scene(5, json!([]));
    let first = answer(&snap, held.clone(), enter("sym_comp"));
    let second = answer(&snap, held, enter("sym_comp"));
    assert_eq!(first, second);
    assert_eq!(snap.content_revision(), revision);
    assert_eq!(encode_storage_project(snap.document()).unwrap(), before);
    // A response header still names the snapshot revision (no advance).
    assert_eq!(
        ask(&snap, scene(0, json!([])), enter("sym_comp")).content_revision,
        revision
    );
}

#[cfg(feature = "scheduler")]
mod presentation {
    use super::*;
    use crate::scheduler::{
        EvaluationKey, FrameScheduler, ObjectSchedulePins, ObjectScheduleScope, OutputSpec,
    };

    fn schedule(
        snap: &ObjectSnapshot,
        context: &str,
        frame: u32,
        scope: ObjectScheduleScope,
    ) -> bool {
        let pins = ObjectSchedulePins {
            instance_id: snap.instance_id().into(),
            document_id: snap.document_id().into(),
            content_revision: snap.content_revision(),
            document_snapshot_id: snap.snapshot_id().into(),
            context_id: context.into(),
            frame,
            scope,
        };
        let key = EvaluationKey::new(
            snap.snapshot_id(),
            context,
            frame,
            "final",
            OutputSpec::new("frame", "rgba8", 64, 64, "srgb", "straight").unwrap(),
            [],
        )
        .unwrap();
        FrameScheduler::new()
            .schedule_object(snap.clone(), pins, key)
            .is_ok()
    }

    #[test]
    fn the_viewport_schedules_a_declared_component_within_its_own_frame_count_only() {
        let snap = snapshot();
        let frames = 12;
        let authored = ObjectScheduleScope::Authored;
        assert!(schedule(&snap, "sym_comp", frames - 1, authored));
        assert!(schedule(&snap, "sym_comp_b", 0, authored));
        assert!(
            !schedule(&snap, "sym_comp", frames, authored),
            "a component frame is bounded by the component, not by the scene"
        );
        assert!(
            schedule(&snap, "scene-root", 23, authored),
            "the scene keeps its own range"
        );
        assert!(!schedule(&snap, "scene-root", 24, authored));
        assert!(!schedule(&snap, "sym_undeclared", 0, authored));
        assert!(!schedule(
            &snap,
            "sym_comp",
            0,
            ObjectScheduleScope::Unsupported
        ));
    }
}

#[test]
fn a_view_beyond_a_shrunk_context_is_clamped_so_leaving_and_closing_always_work() {
    // Undo or an agent edit shrank sym_comp (12 frames) and the scene (24) under the playhead and work area the caller holds.
    let snap = snapshot();
    let held = json!({"activeContextId":"sym_comp","openTabs":["sym_comp"],"view":view(40, 2, 50),"sceneView":view(300, 5, 400)});
    let refreshed = answer(&snap, held.clone(), json!({"kind":"refresh"}));
    assert_eq!(refreshed["outcome"], "refreshed");
    assert_eq!(refreshed["workspace"]["view"], view(11, 2, 11), "the entered view is brought inside the component");
    assert_eq!(refreshed["workspace"]["sceneView"], view(23, 5, 23), "the saved scene view is brought inside the scene");
    let exited = answer(&snap, held.clone(), json!({"kind":"exit"}));
    assert_eq!(exited["outcome"], "exited");
    assert_eq!(exited["contextId"], "scene-root");
    assert_eq!(exited["workspace"]["view"], view(23, 5, 23));
    let closed = answer(&snap, held, json!({"kind":"close","contextId":"sym_comp"}));
    assert_eq!(closed["outcome"], "closed");
    assert_eq!(closed["workspace"]["activeContextId"], Value::Null);
    // A scene view past the scene is clamped as well; a work area that is not ordered is still malformed.
    let scene_only = answer(&snap, json!({"activeContextId":null,"openTabs":[],"view":view(99, 0, 99),"sceneView":null}), json!({"kind":"refresh"}));
    assert_eq!(scene_only["workspace"]["view"], view(23, 0, 23));
}

#[test]
fn a_ninth_open_tab_evicts_the_oldest_tab_that_is_not_the_active_one() {
    // Nine declared components: sym_comp plus eight renamed copies of it (ids are rewritten consistently).
    let mut doc: Value = serde_json::from_str(world_json("D")).unwrap();
    let first = doc["componentTables"]["components"]["components"][0].clone();
    for n in 0..8 {
        let text = first
            .to_string()
            .replace("sym_comp", &format!("sym_x{n}"))
            .replace("ly_comp_", &format!("ly_x{n}_"))
            .replace("s_comp_", &format!("s_x{n}_"));
        doc["componentTables"]["components"]["components"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::from_str(&text).unwrap());
        let objects = first["objects"].as_array().unwrap().len();
        assert!(objects > 0);
    }
    let snap = snapshot_from(&doc);
    let mut tabs = vec!["sym_comp".to_owned(), "sym_comp_b".to_owned()];
    tabs.extend((0..6).map(|n| format!("sym_x{n}")));
    assert_eq!(tabs.len(), 8);
    let out = answer(&snap, scene(0, json!(tabs)), enter("sym_x6"));
    let listed = out["workspace"]["openTabs"].as_array().unwrap();
    assert_eq!(listed.len(), 8, "the strip never exceeds the wire cap");
    assert!(!listed.contains(&json!("sym_comp")), "the oldest tab is evicted");
    assert_eq!(listed.last().unwrap(), "sym_x6");
    assert_eq!(out["workspace"]["activeContextId"], "sym_x6");
    // The answer is one the caller can send straight back.
    let again = answer(&snap, out["workspace"].clone(), json!({"kind":"refresh"}));
    assert_eq!(again["outcome"], "refreshed");
    // The active tab is never the one evicted, even when it is the oldest.
    let mut held = tabs.clone();
    held.insert(0, "sym_x7".to_owned());
    held.truncate(8);
    let active_oldest = json!({"activeContextId":"sym_x7","openTabs":held,"view":view(0, 0, 11),"sceneView":view(0, 0, 23)});
    let kept = answer(&snap, active_oldest, enter("sym_x7"));
    assert_eq!(kept["outcome"], "already_active");
    // Nothing was evicted and nothing reordered: the strip is exactly the one held, the active tab still in it.
    assert_eq!(kept["workspace"]["openTabs"], json!(held));
    assert_eq!(kept["workspace"]["activeContextId"], "sym_x7");
}

fn enter_from(id: &str, layer: &str) -> Value {
    json!({"kind":"enter","contextId":id,"entryFrame":null,"fromLayerUid":layer})
}

#[test]
fn entering_from_an_instance_layer_starts_at_the_frame_that_instance_shows_at_the_playhead() {
    // Expected values come from the baseline `resolveSymbolFrameIdx` (app.js) applied by hand to the world's instances of the
    // 12-frame sym_comp: loop (placed at 4), once, ping-pong, single frame 5 and a doubled speed.
    let snap = snapshot();
    let at = |layer: &str, playhead: u64| {
        let out = answer(&snap, scene(playhead, json!([])), enter_from("sym_comp", layer));
        assert_eq!(out["outcome"], "entered");
        out["workspace"]["view"]["frame"].as_u64().unwrap()
    };
    assert_eq!(at("ly_comp_loop", 10), 6, "loop: (10 - 4) % 12");
    assert_eq!(at("ly_comp_loop", 2), 0, "before the placement the instance shows its first frame");
    assert_eq!(at("ly_comp_loop", 20), 4, "loop wraps: (20 - 4) % 12");
    assert_eq!(at("ly_comp_once", 7), 7, "once plays from frame 0");
    assert_eq!(at("ly_comp_once", 20), 11, "once holds on the last frame");
    assert_eq!(at("ly_comp_ping", 13), 9, "ping-pong: cycle 22, position 13 comes back as 22 - 13");
    assert_eq!(at("ly_comp_single", 17), 5, "single shows its fixed frame");
    assert_eq!(at("ly_comp_speed", 7), 2, "speed 2: (7 * 2) % 12");
    // The work area is still the whole component and the scene view is still saved exactly.
    let out = answer(&snap, scene(10, json!([])), enter_from("sym_comp", "ly_comp_loop"));
    assert_eq!(out["workspace"]["view"], view(6, 0, 11));
    assert_eq!(out["workspace"]["sceneView"], view(10, 0, 23));
}

#[test]
fn an_unknown_or_foreign_instance_enters_at_the_first_frame_and_an_explicit_entry_frame_cannot_be_combined() {
    let snap = snapshot();
    for layer in ["no_such_layer", "ly_compb_loop"] {
        let out = answer(&snap, scene(10, json!([])), enter_from("sym_comp", layer));
        assert_eq!(out["workspace"]["view"]["frame"], 0, "{layer} is not an instance of sym_comp");
    }
    let both = json!({"kind":"enter","contextId":"sym_comp","entryFrame":3,"fromLayerUid":"ly_comp_loop"});
    assert_eq!(refused(&snap, scene(0, json!([])), both), DispatchErrorCode::InvalidRequest);
    let malformed = json!({"kind":"enter","contextId":"sym_comp","entryFrame":null,"fromLayerUid":""});
    assert_eq!(refused(&snap, scene(0, json!([])), malformed), DispatchErrorCode::InvalidRequest);
}

/// Baseline `resolveSymbolFrameIdx`: Time Remap, when its track has keys, names the internal frame directly (`Math.round`, clamped
/// into the component) and overrides the play mode; a track without keys is null and the play mode applies.
fn with_time_remap(layer: &str, remap: Value) -> ObjectSnapshot {
    let mut doc: Value = serde_json::from_str(world_json("D")).unwrap();
    let instance = doc["componentTables"]["components"]["instances"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|i| i["layerUid"] == layer)
        .expect("instance");
    instance["timeRemap"] = remap;
    snapshot_from(&doc)
}
fn entry_frame(snap: &ObjectSnapshot, layer: &str, playhead: u64) -> u64 {
    let out = answer(snap, scene(playhead, json!([])), enter_from("sym_comp", layer));
    assert_eq!(out["outcome"], "entered");
    out["workspace"]["view"]["frame"].as_u64().unwrap()
}
fn remap_key(frame: u64, value: f64) -> Value {
    json!({"frame":frame,"v":[value]})
}

#[test]
fn entering_an_instance_with_a_keyed_time_remap_starts_at_the_frame_the_remap_names() {
    // sym_comp has 12 frames. One key: every playhead reads its value, rounded like Math.round and clamped into 0..11.
    for (value, expected) in [(3.0, 3), (6.6, 7), (6.5, 7), (40.0, 11), (-4.0, 0)] {
        let snap = with_time_remap("ly_comp_loop", json!({"keys":[remap_key(0, value)]}));
        assert_eq!(entry_frame(&snap, "ly_comp_loop", 10), expected, "remap {value}");
    }
    // Two keys: before the first and after the last the end values hold; a held first key keeps its value up to the second.
    let ramp = with_time_remap("ly_comp_loop", json!({"keys":[{"frame":4,"v":[2],"hold":true},remap_key(20, 9.0)]}));
    assert_eq!(entry_frame(&ramp, "ly_comp_loop", 0), 2, "before the first key");
    assert_eq!(entry_frame(&ramp, "ly_comp_loop", 12), 2, "held between the keys");
    assert_eq!(entry_frame(&ramp, "ly_comp_loop", 30), 9, "after the last key");
    // The remap overrides the play mode of the instance (single would show frame 5) and is read at the held playhead.
    let single = with_time_remap("ly_comp_single", json!({"keys":[remap_key(0, 1.0)]}));
    assert_eq!(entry_frame(&single, "ly_comp_single", 17), 1);
}

#[test]
fn a_time_remap_without_keys_falls_back_to_the_play_mode_like_the_baseline_null() {
    let snap = with_time_remap("ly_comp_loop", json!({"keys":[]}));
    assert_eq!(entry_frame(&snap, "ly_comp_loop", 10), 6, "loop: (10 - 4) % 12");
}

#[test]
fn a_time_remap_cannot_be_driven_by_an_expression_so_the_keyed_track_is_always_the_answer() {
    // The document admits no `timeRemap` property record (the track belongs to the instance), so an expression has nowhere to live.
    let mut doc: Value = serde_json::from_str(world_json("D")).unwrap();
    doc["propertyHolders"] = json!([{"target":{"contextId":"scene-root","layerUid":"ly_comp_loop"},
        "properties":[{"property":"timeRemap","expression":{"enabled":true,"code":"5"}}]}]);
    assert!(decode_storage_project(doc.to_string().as_bytes()).is_err());
}
