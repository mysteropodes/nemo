//! Original Perspective gestures through public dispatch, sealed history and fresh reopen.
//! Source candidate only; no executed acceptance is claimed.
use crate::history::NativeObjectHistory;
use crate::request_receipts::{DispatchErrorCode as Error, OpacityRequest};
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};

const EDIT: &str = "command.document.object.project.perspective.edit";
const READ: &str = "query.document.object.project.perspective.read";
fn owner(extra: Value) -> NativeObjectHistory {
    let mut project = json!({"canvasW":100,"canvasH":80,"fps":24});
    project
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    let wire = json!({
        "format":"nemo.native-project-document","formatVersion":1,
        "project":project,"viewportReference":null,"assets":[],
        "drawing":{"format":"nemo.native-object-document","formatVersion":1,
            "totalFrames":40,"layers":[{"layerUid":"L"}],"objects":[]}
    });
    NativeObjectHistory::from_project_file("public-perspective", wire.to_string().as_bytes())
        .unwrap()
}
fn pin(o: &NativeObjectHistory) -> ObjectSnapshot {
    o.acquire_snapshot(o.content_revision()).unwrap()
}
fn saved(o: &NativeObjectHistory) -> Vec<u8> {
    let p = pin(o);
    o.serialize_project_file(p.content_revision(), p.snapshot_id())
        .unwrap()
}
fn project(o: &NativeObjectHistory) -> Value {
    serde_json::from_slice::<Value>(&saved(o)).unwrap()["project"].clone()
}
fn payload(action: Value) -> Vec<u8> {
    serde_json::to_vec(&json!({"command":"object.project.perspective.edit","action":action}))
        .unwrap()
}
fn command(o: &NativeObjectHistory, id: &str, action: Value) -> OpacityRequest {
    serde_json::from_value(json!({
        "apiVersion":2,"requestId":id,"instanceId":o.instance_id(),
        "documentId":o.document_id(),"expectedRevision":o.content_revision(),"operation":EDIT,
        "payload":{"command":"object.project.perspective.edit","action":action}
    }))
    .unwrap()
}
fn dispatch(o: &mut NativeObjectHistory, q: OpacityRequest) -> Value {
    let response = o
        .dispatch_object_command(&q)
        .unwrap()
        .expect("registered Perspective edit");
    assert!(serde_json::to_vec(&response).unwrap().len() <= 4096);
    serde_json::to_value(response).unwrap()
}
fn edit(o: &mut NativeObjectHistory, id: &str, action: Value) -> Value {
    let q = command(o, id, action);
    dispatch(o, q)
}
fn request(p: &ObjectSnapshot, id: &str, selector: Value) -> OpacityRequest {
    serde_json::from_value(json!({
        "apiVersion":2,"requestId":id,"instanceId":p.instance_id(),
        "documentId":p.document_id(),"operation":READ,
        "payload":{"atRevision":p.content_revision(),"documentSnapshotId":p.snapshot_id(),
            "contextId":"scene-root","selector":selector}
    }))
    .unwrap()
}
fn read(p: &ObjectSnapshot, q: OpacityRequest) -> Value {
    let response = p.dispatch(q).unwrap();
    assert!(serde_json::to_vec(&response).unwrap().len() <= 4096);
    serde_json::to_value(response).unwrap()
}
fn query(p: &ObjectSnapshot, selector: Value) -> Value {
    read(p, request(p, "perspective-read", selector))
}
fn points(o: &NativeObjectHistory) -> Value {
    project(o)["perspectiveVPs"].clone()
}
fn default_points(mode: &str) -> Value {
    match mode {
        "1pt" | "fisheye" => json!([{"x":50.0,"y":40.0,"locked":false}]),
        "3pt" => json!([
            {"x":-10.0,"y":28.0,"locked":false},
            {"x":110.0,"y":28.0,"locked":false},
            {"x":50.0,"y":144.0,"locked":false}
        ]),
        _ => json!([{"x":-15.0,"y":40.0,"locked":false},{"x":115.0,"y":40.0,"locked":false}]),
    }
}
#[test]
fn all_original_modes_reset_using_native_dimensions_and_reset_preserves_unknown_mode_density() {
    let mut o = owner(
        json!({"perspectiveMode":"unknown","perspectiveDensity":-4.25,
        "perspectiveVPs":[{"x":-12.25,"y":8.5}]}),
    );
    edit(&mut o, "reset-unknown", json!({"kind":"reset"}));
    assert_eq!(points(&o), default_points("2pt"));
    assert_eq!(project(&o)["perspectiveMode"], "unknown");
    assert_eq!(project(&o)["perspectiveDensity"], -4.25);
    for (index, mode) in ["1pt", "2pt", "3pt", "fisheye"].into_iter().enumerate() {
        edit(
            &mut o,
            &format!("mode-{index}"),
            json!({"kind":"setMode","mode":mode}),
        );
        assert_eq!(project(&o)["perspectiveMode"], mode);
        assert_eq!(points(&o), default_points(mode));
        assert_eq!(project(&o)["perspectiveDensity"], -4.25);
        let before = saved(&o);
        let settings = query(&pin(&o), json!({"kind":"settings"}));
        assert_eq!(
            settings["result"]["result"]["effectiveVPCount"],
            default_points(mode).as_array().unwrap().len()
        );
        assert_eq!(saved(&o), before);
    }
}
#[test]
fn move_translate_lock_and_replay_preserve_optional_flags_old_pins_and_unrelated_metadata() {
    let source = json!([
        {"x":-12.25,"y":8.5}, {"x":100.5,"y":-8.25,"locked":true},
        {"x":-12.25,"y":8.5,"locked":false}
    ]);
    let mut o = owner(json!({"perspectiveVPs":source,"perspectiveMode":"custom",
        "perspectiveDensity":-9.75,"workspace":{"panelOrder":null},"guides":{"h":[-0.25]},
        "shadowPalette":[{"id":"same","color":"#AbC"},{"id":"same","color":"#abc"}],
        "comments":[{"text":"Keep","authorName":"Writer"}],"bpm":137.5}));
    let old = pin(&o);
    let before = saved(&o);
    let q = command(
        &o,
        "move",
        json!({"kind":"move","ordinal":1,"x":-0.5,"y":2.25}),
    );
    let response = dispatch(&mut o, q.clone());
    assert_eq!(
        response["result"],
        json!({"applied":true,"historyEntriesAdded":1})
    );
    assert_eq!(dispatch(&mut o, q), response);
    assert_eq!(o.history_depths(), (1, 0));
    assert!(points(&o)[0].get("locked").is_none());
    let old_page = query(&old, json!({"kind":"vpPage","cursor":0,"limit":16}));
    assert_eq!(
        old_page["result"]["result"]["items"][0],
        json!({"ordinal":1,"x":-12.25,"y":8.5})
    );
    edit(
        &mut o,
        "translate",
        json!({"kind":"translate","dx":-3.25,"dy":4.5}),
    );
    assert_eq!(
        points(&o),
        json!([
            {"x":-3.75,"y":6.75},{"x":100.5,"y":-8.25,"locked":true},
            {"x":-15.5,"y":13.0,"locked":false}
        ])
    );
    let changed = saved(&o);
    let mut after: Value = serde_json::from_slice(&changed).unwrap();
    let initial: Value = serde_json::from_slice(&before).unwrap();
    after["project"]["perspectiveVPs"] = initial["project"]["perspectiveVPs"].clone();
    assert_eq!(after, initial);
    o.undo(2).unwrap();
    o.undo(3).unwrap();
    assert_eq!(saved(&o), before);
    o.redo(4).unwrap();
    o.redo(5).unwrap();
    assert_eq!(saved(&o), changed);
    let fresh = NativeObjectHistory::from_project_file("perspective-reopen", &changed).unwrap();
    assert_eq!(saved(&fresh), changed);
    edit(&mut o, "lock", json!({"kind":"lockAll","locked":true}));
    assert!(points(&o)
        .as_array()
        .unwrap()
        .iter()
        .all(|point| point["locked"] == true));
    edit(&mut o, "unlock", json!({"kind":"lockAll","locked":false}));
    assert!(points(&o)
        .as_array()
        .unwrap()
        .iter()
        .all(|point| point["locked"] == false));
}
#[test]
fn locked_move_zero_translation_and_identical_settings_preserve_redo() {
    let mut o = owner(json!({"perspectiveEnabled":false,
        "perspectiveVPs":[{"x":1.25,"y":-3.5,"locked":true},{"x":2.5,"y":4.25}]}));
    edit(
        &mut o,
        "enable",
        json!({"kind":"setEnabled","enabled":true}),
    );
    o.undo(1).unwrap();
    let before = saved(&o);
    for (id, action) in [
        (
            "locked-move",
            json!({"kind":"move","ordinal":1,"x":99.0,"y":99.0}),
        ),
        (
            "zero-translate",
            json!({"kind":"translate","dx":0.0,"dy":0.0}),
        ),
        ("disabled", json!({"kind":"setEnabled","enabled":false})),
    ] {
        assert_eq!(
            edit(&mut o, id, action)["result"],
            json!({"applied":false,"historyEntriesAdded":0})
        );
        assert_eq!(o.content_revision(), 2);
        assert_eq!(o.history_depths(), (0, 1));
        assert_eq!(saved(&o), before);
    }
    o.redo(2).unwrap();
    assert_eq!(project(&o)["perspectiveEnabled"], true);
}
#[test]
fn explicit_lock_move_and_horizon_gestures_materialize_defaults_once_without_targeting_mode() {
    for action in [
        json!({"kind":"lockAll","locked":true}),
        json!({"kind":"move","ordinal":2,"x":-8.25,"y":0.5}),
        json!({"kind":"translate","dx":1.25,"dy":-2.5}),
    ] {
        for supplied in [
            json!({}),
            json!({"perspectiveVPs":null}),
            json!({"perspectiveVPs":[]}),
        ] {
            let mut o = owner(supplied);
            assert_eq!(
                edit(&mut o, "materialize", action.clone())["result"],
                json!({"applied":true,"historyEntriesAdded":1})
            );
            assert!(project(&o).get("perspectiveMode").is_none());
            assert_eq!(points(&o).as_array().unwrap().len(), 2);
            let expected = match action["kind"].as_str().unwrap() {
                "lockAll" => {
                    json!([{"x":-15.0,"y":40.0,"locked":true},{"x":115.0,"y":40.0,"locked":true}])
                }
                "move" => {
                    json!([{"x":-15.0,"y":40.0,"locked":false},{"x":-8.25,"y":0.5,"locked":false}])
                }
                _ => {
                    json!([{"x":-13.75,"y":37.5,"locked":false},{"x":116.25,"y":37.5,"locked":false}])
                }
            };
            assert_eq!(points(&o), expected);
        }
    }
    let mut o = owner(json!({"perspectiveMode":"1pt"}));
    let before = saved(&o);
    assert_eq!(
        edit(
            &mut o,
            "no-horizon",
            json!({"kind":"translate","dx":1.0,"dy":2.0})
        )["error"]["code"],
        "not_found"
    );
    assert_eq!(saved(&o), before);
    assert_eq!(o.history_depths(), (0, 0));
}
#[test]
fn malformed_extra_density_array_dimensions_and_overflow_intents_cannot_publish() {
    let mut o = owner(json!({"perspectiveVPs":[{"x":1.0,"y":2.0},{"x":1.7e308,"y":3.0}]}));
    let before = saved(&o);
    for (i, action) in [
        json!({"kind":"setEnabled","enabled":null}),
        json!({"kind":"setMode","mode":"unknown"}),
        json!({"kind":"setDensity","density":12}),
        json!({"kind":"reset","canvasW":200,"canvasH":100}),
        json!({"kind":"reset","vps":[]}),
        json!({"kind":"lockAll","locked":1}),
        json!({"kind":"move","ordinal":0,"x":1,"y":2}),
        json!({"kind":"move","ordinal":1.5,"x":1,"y":2}),
        json!({"kind":"move","ordinal":9_007_199_254_740_992_u64,"x":1,"y":2}),
        json!({"kind":"move","ordinal":1,"x":null,"y":2}),
        json!({"kind":"translate","dx":1.7e308,"dy":0}),
        json!({"kind":"setMode","mode":"x".repeat(4096)}),
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(
            edit(&mut o, &format!("bad-{i}"), action)["error"]["code"],
            "invalid_request"
        );
        assert_eq!(saved(&o), before);
        assert_eq!(o.history_depths(), (0, 0));
    }
    assert_eq!(
        edit(
            &mut o,
            "absent-ordinal",
            json!({"kind":"move","ordinal":3,"x":1,"y":2})
        )["error"]["code"],
        "not_found"
    );
    for raw in [
        r#"[]"#,
        r#"{"command":"object.project.perspective.edit","action":[]}"#,
        r#"{"command":"object.project.perspective.edit","command":"object.project.perspective.edit","action":{"kind":"reset"}}"#,
        r#"{"command":"object.project.perspective.edit","action":{"kind":"move","ordinal":1,"x":1,"x":2,"y":3}}"#,
        r#"{"command":"object.project.perspective.edit","action":{"kind":"translate","dx":1e309,"dy":0}}"#,
        r#"{"command":"object.project.shadow.edit","action":{"kind":"reset"}}"#,
    ] {
        assert_eq!(
            o.prepare_perspective_delta_json(raw.as_bytes())
                .unwrap_err(),
            Error::InvalidRequest
        );
    }
    assert_eq!(
        o.prepare_perspective_delta_json(&vec![b' '; 4097])
            .unwrap_err(),
        Error::InvalidRequest
    );
    assert_eq!(saved(&o), before);
}
#[test]
fn public_identity_revision_context_cancellation_and_sealed_candidate_fences_remain_atomic() {
    let mut o = owner(json!({}));
    let before = saved(&o);
    for (field, value, code) in [
        ("instanceId", json!("foreign"), "wrong_instance"),
        ("documentId", json!("foreign"), "wrong_document"),
        ("expectedRevision", json!(1), "stale_revision"),
    ] {
        let mut raw = serde_json::to_value(command(&o, field, json!({"kind":"reset"}))).unwrap();
        raw[field] = value;
        assert_eq!(
            dispatch(&mut o, serde_json::from_value(raw).unwrap())["error"]["code"],
            code
        );
    }
    let mut cancelled = command(&o, "cancelled", json!({"kind":"reset"}));
    cancelled.cancelled_before_dispatch = true;
    assert_eq!(
        dispatch(&mut o, cancelled)["error"]["code"],
        "cancelled_before_dispatch"
    );
    assert_eq!(saved(&o), before);
    let candidate = o
        .prepare_perspective_delta_json(&payload(json!({"kind":"reset"})))
        .unwrap();
    let mut foreign =
        NativeObjectHistory::from_project_file("foreign-perspective", &before).unwrap();
    assert_eq!(
        foreign.commit_project_settings(candidate.clone()),
        Err(Error::WrongInstance)
    );
    let mut other_doc =
        NativeObjectHistory::from_project_file("public-perspective", &before).unwrap();
    assert_eq!(
        other_doc.commit_project_settings(candidate.clone()),
        Err(Error::WrongDocument)
    );
    assert_eq!(saved(&foreign), before);
    assert_eq!(saved(&other_doc), before);
    let old = pin(&o);
    edit(
        &mut o,
        "new-revision",
        json!({"kind":"setEnabled","enabled":true}),
    );
    let current = saved(&o);
    assert_eq!(
        o.commit_project_settings(candidate),
        Err(Error::StaleRevision)
    );
    assert_eq!(saved(&o), current);
    let p = pin(&o);
    assert_eq!(
        read(&p, request(&old, "old-pin", json!({"kind":"settings"})))["error"]["code"],
        "stale_revision"
    );
    for (field, value, code) in [
        ("atRevision", json!(0), "stale_revision"),
        ("documentSnapshotId", json!("wrong"), "stale_revision"),
        ("contextId", json!("nested"), "unavailable"),
        ("extra", json!(true), "invalid_request"),
    ] {
        let mut q = request(&p, field, json!({"kind":"settings"}));
        q.payload[field] = value;
        assert_eq!(read(&p, q)["error"]["code"], code);
    }
    for selector in [
        json!({"kind":"vpPage","cursor":0,"limit":0}),
        json!({"kind":"vpPage","cursor":0,"limit":17}),
        json!({"kind":"vpPage","cursor":3,"limit":1}),
        json!({"kind":"vpPage","cursor":9_007_199_254_740_992_u64,"limit":1}),
        json!({"kind":"settings","extra":true}),
        json!(["settings"]),
    ] {
        assert_eq!(query(&p, selector)["error"]["code"], "invalid_request");
    }
    let mut q = request(&p, "read-revision", json!({"kind":"settings"}));
    q.expected_revision = Some(p.content_revision());
    assert_eq!(read(&p, q)["error"]["code"], "invalid_request");
    assert_eq!(
        query(&p, json!({"kind":"vpPage","cursor":2,"limit":1}))["result"]["result"]["items"],
        json!([])
    );
}
#[test]
fn reset_cannot_publish_a_candidate_that_the_actual_project_encoder_cannot_save() {
    const LIMIT: usize = 32 * 1024 * 1024;
    const TARGET: usize = LIMIT - 32;
    let colors = vec!["x".repeat(1024); (LIMIT - 4096) / 1027];
    let source = owner(json!({"palettes":[{"id":"filler","name":"Filler","colors":colors}]}));
    let mut wire: Value = serde_json::from_slice(&saved(&source)).unwrap();
    let mut remaining = TARGET
        .checked_sub(serde_json::to_vec(&wire).unwrap().len())
        .unwrap();
    let colors = wire["project"]["palettes"][0]["colors"]
        .as_array_mut()
        .unwrap();
    while remaining > 0 {
        if remaining < 3 {
            let last = colors.last_mut().unwrap();
            let value = last.as_str().unwrap();
            *last = json!(&value[..value.len() - (3 - remaining)]);
            remaining = 3;
        }
        let bytes = (remaining - 3).min(1024);
        colors.push(json!("x".repeat(bytes)));
        remaining -= bytes + 3;
    }
    let encoded = serde_json::to_vec(&wire).unwrap();
    assert_eq!(encoded.len(), TARGET);
    let o = NativeObjectHistory::from_project_file("perspective-budget", &encoded).unwrap();
    let before = saved(&o);
    assert_eq!(before.len(), TARGET);
    let bytes = payload(json!({"kind":"reset"}));
    assert!(bytes.len() < 128);
    assert_eq!(
        o.prepare_perspective_delta_json(&bytes).unwrap_err(),
        Error::Unavailable
    );
    assert_eq!(saved(&o), before);
    assert_eq!(o.content_revision(), 0);
    assert_eq!(o.history_depths(), (0, 0));
}

#[path = "object_perspective_read.rs"]
mod perspective_read_tests;
