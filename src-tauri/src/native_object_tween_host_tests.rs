//! #2358: the four tween commands through the public host route (request validation, dispatch, history, save/reopen).
use crate::{application_mcp::ApplicationMcp, native_object_bootstrap::bootstrap_raw};
use nemo_mcp::contract::NativeApplicationRequest;
use serde_json::{json, Value};

fn stroke(frame: u32, x: f64) -> Value {
    let point = |x: f64, y: f64| json!({"point":{"x":x,"y":y},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}});
    json!({"schemaVersion":1,"family":"editable-vector-path",
        "target":{"contextId":"scene-root","frameScope":{"kind":"authored","frame":frame},"layerUid":"L0","strokeId":format!("s{frame}")},
        "geometry":{"kind":"cubic-path","closed":false,"coordinateSpace":"document-world","handleSpace":"relative",
            "segments":[point(x, 10.), point(x + 20., 30.), point(x + 40., 10.)]},
        "fill":null,"vector":{"stroke":{"kind":"solid","r":0,"g":0,"b":0,"a":1},"hasRealStroke":true,"strokeWidth":3,
            "strokeCap":"round","strokeJoin":"round","opacity":1}})
}
fn project() -> Value {
    json!({"format":"nemo.native-project-document","formatVersion":1,
        "project":{"canvasW":64,"canvasH":48,"fps":24},"viewportReference":null,"assets":[],
        "drawing":{"format":"nemo.native-object-document","formatVersion":1,"totalFrames":8,
            "layers":[{"layerUid":"L0","timing":{"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing",
                "contextId":"scene-root","keys":[{"frame":0},{"frame":4}],"samples":[]}}],
            "objects":[stroke(0, 0.), stroke(4, 20.)]}})
}
fn install(state: &ApplicationMcp, project: &Value) -> String {
    let receipt = serde_json::to_value(
        bootstrap_raw(
            state,
            &json!({"apiVersion":2,"requestId":"tween-install","instanceId":state.instance_id(),
                "documentJson":project.to_string()})
            .to_string(),
        )
        .unwrap(),
    )
    .unwrap();
    receipt["documentId"].as_str().unwrap().to_owned()
}
fn saved(state: &ApplicationMcp, id: &str) -> Vec<u8> {
    let info = state.native_project_info(id).unwrap();
    state
        .native_project_file_snapshot(
            id,
            info["contentRevision"].as_u64().unwrap(),
            info["documentSnapshotId"].as_str().unwrap(),
        )
        .unwrap()
        .unwrap()
}
fn send(
    state: &ApplicationMcp,
    id: &str,
    name: &str,
    revision: u64,
    op: &str,
    payload: Value,
) -> Value {
    let request = json!({"apiVersion":2,"requestId":name,"instanceId":state.instance_id(),"documentId":id,
        "expectedRevision":revision,"operation":op,"payload":payload});
    let parsed: NativeApplicationRequest = serde_json::from_value(request).unwrap();
    assert!(
        parsed.validate().is_ok(),
        "{op} must be a declared public operation"
    );
    serde_json::to_value(state.dispatch_native(parsed).unwrap()).unwrap()
}
fn tween(state: &ApplicationMcp, id: &str, name: &str, revision: u64, payload: Value) -> Value {
    let op = format!("command.document.{}", payload["command"].as_str().unwrap());
    send(state, id, name, revision, &op, payload)
}
fn object_frames(file: &[u8]) -> Vec<u64> {
    let saved: Value = serde_json::from_slice(file).unwrap();
    let mut frames: Vec<u64> = saved["drawing"]["objects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["target"]["frameScope"]["frame"].as_u64().unwrap())
        .collect();
    frames.sort_unstable();
    frames
}
fn keys(file: &[u8]) -> Vec<u64> {
    let saved: Value = serde_json::from_slice(file).unwrap();
    saved["drawing"]["layers"][0]["timing"]["keys"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k["frame"].as_u64().unwrap())
        .collect()
}

#[test]
fn generate_split_reassign_regenerate_undo_redo_and_reopen_through_the_public_route() {
    let state = ApplicationMcp::default();
    let id = install(&state, &project());
    let initial = saved(&state, &id);
    assert_eq!(object_frames(&initial), vec![0, 4]);
    let applied = json!({"applied":true,"historyEntriesAdded":1});

    let generate = json!({"command":"object.tween.generate","layerUid":"L0","selection":[0]});
    let first = tween(&state, &id, "generate", 0, generate.clone());
    assert_eq!(first["ok"], true, "{first}");
    assert_eq!(
        (&first["result"], &first["contentRevision"]),
        (&applied, &json!(1))
    );
    let generated = saved(&state, &id);
    assert_eq!(object_frames(&generated), vec![0, 1, 2, 3, 4]);
    assert_eq!(
        tween(&state, &id, "generate", 0, generate),
        first,
        "an exact replay returns the receipt"
    );
    assert_eq!(saved(&state, &id), generated);

    let split = tween(
        &state,
        &id,
        "split",
        1,
        json!({"command":"object.tween.split","layerUid":"L0","frame":2}),
    );
    assert_eq!(
        (&split["result"], &split["contentRevision"]),
        (&applied, &json!(2)),
        "{split}"
    );
    let split_file = saved(&state, &id);
    assert_eq!(keys(&split_file), vec![0, 2, 4]);

    let reassign = tween(
        &state,
        &id,
        "reassign",
        2,
        json!({"command":"object.tween.reassign","layerUid":"L0",
        "span":{"from":0,"to":2,"overrides":[{"aId":"s0","bId":"s0"}]}}),
    );
    assert_eq!(reassign["ok"], true, "{reassign}");
    assert_eq!(reassign["contentRevision"], 3);
    let regenerate = tween(
        &state,
        &id,
        "regenerate",
        3,
        json!({"command":"object.tween.regenerate","layerUid":"L0",
        "options":{"resamplePoints":64,"step":1,"skipManual":true,"easing":[]},"starts":[0, 2]}),
    );
    assert_eq!(regenerate["ok"], true, "{regenerate}");
    assert_eq!(regenerate["contentRevision"], 4);
    let last = saved(&state, &id);
    assert_eq!(object_frames(&last), vec![0, 1, 2, 3, 4]);

    // Undo and redo are revisions of their own: each receipt names the revision the next request expects.
    let mut revision = 4;
    let mut history = |name: &str, action: &str| {
        let reply = send(
            &state,
            &id,
            name,
            revision,
            &format!("command.document.object.{action}"),
            json!({"command":format!("object.{action}")}),
        );
        assert_eq!(reply["ok"], true, "{name}: {reply}");
        revision = reply["contentRevision"].as_u64().unwrap();
    };
    for (n, want) in [
        (0, None),
        (1, Some(&split_file)),
        (2, Some(&generated)),
        (3, Some(&initial)),
    ] {
        history(&format!("undo-{n}"), "undo");
        if let Some(want) = want {
            assert_eq!(&saved(&state, &id), want, "undo {n}");
        }
    }
    for n in 0..4 {
        history(&format!("redo-{n}"), "redo");
    }
    assert_eq!(
        saved(&state, &id),
        last,
        "redo restores the final document byte for byte"
    );

    let fresh = ApplicationMcp::default();
    let reopened = install(&fresh, &serde_json::from_slice(&last).unwrap());
    assert_eq!(
        saved(&fresh, &reopened),
        last,
        "save and reopen round-trips the tweens"
    );
}

#[test]
fn refusals_are_typed_and_leave_the_document_and_history_alone() {
    let state = ApplicationMcp::default();
    let id = install(&state, &project());
    let before = saved(&state, &id);
    let cases = [
        (
            "stale",
            5,
            json!({"command":"object.tween.generate","layerUid":"L0"}),
            "stale_revision",
        ),
        (
            "unknown",
            0,
            json!({"command":"object.tween.generate","layerUid":"missing"}),
            "not_found",
        ),
        (
            "split-held",
            0,
            json!({"command":"object.tween.split","layerUid":"L0","frame":2}),
            "invalid_request",
        ),
        (
            "bad-span",
            0,
            json!({"command":"object.tween.reassign","layerUid":"L0","span":{"from":0,"to":3,"overrides":[]}}),
            "invalid_request",
        ),
    ];
    for (name, revision, payload, code) in cases {
        let reply = tween(&state, &id, name, revision, payload);
        assert_eq!(reply["ok"], false, "{name}: {reply}");
        assert_eq!(reply["error"]["code"], code, "{name}: {reply}");
        assert_eq!(saved(&state, &id), before, "{name}");
    }
    let undo = send(
        &state,
        &id,
        "undo",
        0,
        "command.document.object.undo",
        json!({"command":"object.undo"}),
    );
    assert_eq!(
        undo["error"]["code"], "unavailable",
        "nothing was recorded: {undo}"
    );
}
