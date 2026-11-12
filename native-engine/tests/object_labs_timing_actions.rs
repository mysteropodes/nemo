//! U35 (#2362) Labs timing actions against the baseline dense oracle (fixtures/labs-timing-oracle.json, generated
//! outside the repository by gen-labs-timing-oracle.cjs from the 1065e9d8 sources run verbatim): every frame's kind
//! (key / in-between / plain) and stroke ids, and the grown frame count, through the real history owner in the v1 and
//! v2 forms; one history entry, undo to the original bytes, redo.
use crate::history::NativeObjectHistory;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::object_document::{FrameScopeKind, Presence};
use serde_json::{json, Value};

#[path = "object_frame_edit_document_build.rs"]
#[allow(dead_code)]
mod build;
#[path = "object_exposure_edit_key_rows.rs"]
#[allow(dead_code)]
mod rows;

/// "K=a,b;.;I=i" (oracle DSL) as the fixture DSL "K=a.1.10,b.2.10;.;I=i.1.10".
fn cells(oracle: &str) -> String {
    oracle
        .split(';')
        .map(|cell| match cell.split_once('=') {
            Some((flag, ids)) => {
                let strokes: Vec<String> = ids
                    .split(',')
                    .enumerate()
                    .map(|(z, id)| format!("{id}.{}.10", z + 1))
                    .collect();
                format!("{flag}={}", strokes.join(","))
            }
            None => cell.to_owned(),
        })
        .collect::<Vec<_>>()
        .join(";")
}
fn owner(layers: &[Value], form: build::Build) -> NativeObjectHistory {
    let text: Vec<String> = layers.iter().map(|l| cells(l.as_str().unwrap())).collect();
    let v = build::document_json(&rows::parse_layers(&text.join("|")), form);
    NativeObjectHistory::new(
        "labs-timing",
        decode_storage_project(v.to_string().as_bytes()).unwrap(),
    )
    .unwrap()
}
fn bytes(o: &NativeObjectHistory) -> Vec<u8> {
    encode_storage_project(o.acquire_snapshot(o.content_revision()).unwrap().document()).unwrap()
}
/// Each layer as the oracle reports it: [kind, ids] per frame, ids in natural stacking order.
fn dense(o: &NativeObjectHistory) -> (u64, Vec<Value>) {
    let s = o.acquire_snapshot(o.content_revision()).unwrap();
    let d = s.document();
    let layers = d
        .layers
        .iter()
        .map(|layer| {
            let timing = layer.timing.value().unwrap();
            Value::Array(
                (0..d.total_frames)
                    .map(|f| {
                        let kind = match timing.exposure(f).0 {
                            "key" => "key",
                            "interpolated" => "interp",
                            _ => "plain",
                        };
                        let mut records: Vec<_> = d
                            .objects
                            .iter()
                            .filter(|r| {
                                r.target.layer_uid == layer.layer_uid
                                    && r.target.frame_scope.kind == FrameScopeKind::Authored
                                    && r.target.frame_scope.frame == f
                            })
                            .collect();
                        if let Presence::Present(plan) = &d.composition_order {
                            let at = |id: &str| {
                                plan.contexts
                                    .iter()
                                    .flat_map(|c| &c.scopes)
                                    .filter(|sc| sc.frame_scope.frame == f)
                                    .flat_map(|sc| &sc.elements)
                                    .find(|e| e.layer_uid == layer.layer_uid && e.stroke_id == id)
                                    .map(|e| e.natural_position)
                                    .unwrap()
                            };
                            records.sort_by_key(|r| at(&r.target.stroke_id));
                        }
                        json!([
                            kind,
                            records
                                .iter()
                                .map(|r| r.target.stroke_id.clone())
                                .collect::<Vec<_>>()
                        ])
                    })
                    .collect(),
            )
        })
        .collect();
    (u64::from(d.total_frames), layers)
}
fn run(o: &mut NativeObjectHistory, command: &str, payload: Value) -> Value {
    let mut payload = payload;
    payload["command"] = json!(format!("object.labs.{command}"));
    let request = json!({"apiVersion":2,"requestId":format!("timing-{}", o.content_revision()),"instanceId":o.instance_id(),
        "documentId":o.document_id(),"expectedRevision":o.content_revision(),
        "operation":format!("command.document.object.labs.{command}"),"payload":payload});
    let typed = serde_json::from_value(request).unwrap();
    serde_json::to_value(o.try_dispatch_object_command(&typed).unwrap().unwrap()).unwrap()
}

#[test]
fn every_timing_action_reproduces_the_baseline_dense_frames_in_one_history_entry() {
    let oracle: Value =
        serde_json::from_str(include_str!("fixtures/labs-timing-oracle.json")).unwrap();
    let cases = oracle["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 8);
    for form in [build::Build::V1, build::Build::V2] {
        for case in cases {
            let id = case["id"].as_str().unwrap();
            let layers = case["layers"].as_array().unwrap();
            let mut o = owner(layers, form);
            let before = bytes(&o);
            let b = &case["bounds"];
            let uid = |i: &Value| format!("L{}", i.as_u64().unwrap());
            let (command, payload) = match case["action"].as_str().unwrap() {
                "pingpong" => (
                    "pingpong",
                    json!({"fromLayer":uid(&b["minL"]),"toLayer":uid(&b["maxL"]),
                    "fromFrame":b["minF"],"toFrame":b["maxF"],"times":case["args"]["times"]}),
                ),
                "retime" => (
                    "retime",
                    json!({"fromLayer":uid(&b["minL"]),"toLayer":uid(&b["maxL"]),
                    "fromFrame":b["minF"],"toFrame":b["maxF"],"spacing":case["args"]["spacing"]}),
                ),
                _ => (
                    "interval",
                    json!({"layerUid":"L0","fromFrame":b["minF"],"toFrame":b["maxF"],
                    "count":case["args"]["count"],"ease":case["args"]["ease"]}),
                ),
            };
            let reply = run(&mut o, command, payload);
            assert_eq!(
                reply["result"],
                json!({"applied":true,"historyEntriesAdded":1}),
                "{id} {reply}"
            );
            let (total, dense_layers) = dense(&o);
            assert_eq!(
                total,
                case["output"]["totalFrames"].as_u64().unwrap(),
                "{id}"
            );
            assert_eq!(Value::Array(dense_layers), case["output"]["layers"], "{id}");
            let after = bytes(&o);
            let undone = o.undo(o.content_revision()).unwrap();
            assert_eq!(bytes(&o), before, "{id}");
            o.redo(undone).unwrap();
            assert_eq!(bytes(&o), after, "{id}");
        }
    }
}

#[test]
fn timing_actions_refuse_atomically() {
    let layers = [json!("K=a;.;I=i;K=b;.;.")];
    for (command, payload, code) in [
        (
            "pingpong",
            json!({"fromLayer":"L0","toLayer":"L0","fromFrame":2,"toFrame":2,"times":1}),
            "invalid_request",
        ),
        (
            "pingpong",
            json!({"fromLayer":"L0","toLayer":"L9","fromFrame":0,"toFrame":3,"times":1}),
            "not_found",
        ),
        (
            "pingpong",
            json!({"fromLayer":"L0","toLayer":"L0","fromFrame":0,"toFrame":3,"times":51}),
            "invalid_request",
        ),
        (
            "retime",
            json!({"fromLayer":"L0","toLayer":"L0","fromFrame":4,"toFrame":5,"spacing":2}),
            "not_found",
        ),
        (
            "retime",
            json!({"fromLayer":"L0","toLayer":"L0","fromFrame":0,"toFrame":3,"spacing":13}),
            "invalid_request",
        ),
        (
            "interval",
            json!({"layerUid":"L0","fromFrame":0,"toFrame":1,"count":1,"ease":"linear"}),
            "invalid_request",
        ),
        (
            "interval",
            json!({"layerUid":"L0","fromFrame":0,"toFrame":3,"count":3,"ease":"linear"}),
            "invalid_request",
        ),
        (
            "interval",
            json!({"layerUid":"L0","fromFrame":2,"toFrame":4,"count":1,"ease":"bounce"}),
            "invalid_request",
        ),
    ] {
        let mut o = owner(&layers, build::Build::V2);
        let before = bytes(&o);
        let reply = run(&mut o, command, payload.clone());
        assert_eq!(reply["error"]["code"], code, "{command} {payload} {reply}");
        assert_eq!((bytes(&o), o.history_depths()), (before, (0, 0)));
    }
}
