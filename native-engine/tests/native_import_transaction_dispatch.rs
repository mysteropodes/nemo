//! L10-H through the staged dispatcher: strict original-byte payloads, the shared envelope checks
//! (cancel, stale revision, owner pins), exact request replay that never executes twice, and the
//! 4096-byte request/response bounds the dispatcher already enforces for every object operation.
use super::session::MAX_RECEIPT_BYTES;
use super::support::*;
use super::wire::{parse_batch, MAX_PAYLOAD_BYTES};
use super::ImportError;
use serde_json::{json, Value};

fn wire_records() -> (Vec<Value>, Vec<Value>) {
    (
        vec![json!({"sourceId":"w","name":"Wire"})],
        vec![
            json!({"sourceId":"w:0","record":vector_json("w", 0, "", 1)}),
            json!({"sourceId":"w:1","record":vector_json("w", 2, "", 2)}),
            json!({"sourceId":"e:0","record":vector_json("L1", 0, "", 3)}),
        ],
    )
}
fn wire() -> Value {
    let (layers, records) = wire_records();
    wire_batch(layers, records)
}
fn code(response: &Value) -> &str {
    response["error"]["code"].as_str().unwrap_or("")
}

#[test]
fn the_wire_batch_commits_through_the_dispatcher_and_echoes_the_literal_receipt() {
    let mut h = history(1);
    let original = state(&h);
    let req = request(&h, "imp-1", wire());
    assert!(
        serde_json::to_vec(&req).unwrap().len() <= 4096,
        "fits the dispatcher bound"
    );
    let response = dispatch(&mut h, &req);
    assert_eq!(response["ok"], true, "{response}");
    assert_eq!(response["contentRevision"], 1);
    assert_eq!(
        response["result"],
        json!({"applied":true,"historyEntriesAdded":1,
            "layers":[{"sourceId":"w","layerUid":"native-layer:1:0"}],
            "objects":[{"sourceId":"w:0","strokeId":"native-stroke:1:0"},
                {"sourceId":"w:1","strokeId":"native-stroke:1:1"},
                {"sourceId":"e:0","strokeId":"native-stroke:1:2"}],
            "skipped":[]})
    );
    let after = state(&h);
    assert_eq!(
        after["objects"],
        json!([
            vector_json("L0", 0, "s0", 100),
            vector_json("native-layer:1:0", 0, "native-stroke:1:0", 1),
            vector_json("native-layer:1:0", 2, "native-stroke:1:1", 2),
            vector_json("L1", 0, "native-stroke:1:2", 3)
        ])
    );
    assert_eq!(h.history_depths(), (1, 0));
    // The dispatcher's own undo takes the whole batch back in one step.
    let undo = json!({"apiVersion":2,"requestId":"undo-1","instanceId":h.instance_id(),
        "documentId":h.document_id(),"expectedRevision":1,
        "operation":"command.document.object.undo","payload":{"command":"object.undo"}});
    assert_eq!(dispatch(&mut h, &undo)["ok"], true);
    assert_eq!(state(&h), original);
}

#[test]
fn exact_replay_does_not_execute_twice_and_a_changed_body_is_refused() {
    let mut h = history(1);
    let req = request(&h, "imp-1", wire());
    let first = dispatch(&mut h, &req);
    assert_eq!(first["ok"], true);
    let after = frozen(&h);
    assert_eq!(
        dispatch(&mut h, &req),
        first,
        "retained response, byte for byte"
    );
    assert_eq!(frozen(&h), after, "replay published nothing");
    let mut changed = req.clone();
    changed["payload"]["layers"][0]["name"] = json!("Different");
    let refused = dispatch(&mut h, &changed);
    assert_eq!(refused["ok"], false);
    assert_eq!(code(&refused), "invalid_request");
    assert_eq!(frozen(&h), after);
    // Replay survives an undo: the retained answer comes back, the batch is NOT re-applied.
    h.undo(1).unwrap();
    let undone = frozen(&h);
    assert_eq!(dispatch(&mut h, &req), first);
    assert_eq!(frozen(&h), undone);
}

#[test]
fn cancel_stale_revision_and_foreign_owner_publish_nothing() {
    let mut h = history(1);
    let before = frozen(&h);
    let mut cancelled = request(&h, "cancel-1", wire());
    cancelled["cancelledBeforeDispatch"] = json!(true);
    let response = dispatch(&mut h, &cancelled);
    assert_eq!(
        (response["ok"].clone(), code(&response)),
        (json!(false), "cancelled_before_dispatch")
    );
    assert_eq!(
        dispatch(&mut h, &cancelled),
        response,
        "the cancellation is retained"
    );
    let mut stale = request(&h, "stale-1", wire());
    stale["expectedRevision"] = json!(7);
    assert_eq!(code(&dispatch(&mut h, &stale)), "stale_revision");
    let mut foreign = request(&h, "foreign-1", wire());
    foreign["instanceId"] = json!("somebody-else");
    assert_eq!(code(&dispatch(&mut h, &foreign)), "wrong_instance");
    assert_eq!(frozen(&h), before);
}

#[test]
fn malformed_payloads_are_invalid_requests_that_publish_nothing() {
    let corrupt: Vec<(&str, Box<dyn Fn(&mut Value)>)> = vec![
        (
            "unknown top-level member",
            Box::new(|p: &mut Value| p["extra"] = json!(1)),
        ),
        (
            "missing records",
            Box::new(|p: &mut Value| {
                p.as_object_mut().unwrap().remove("records");
            }),
        ),
        (
            "wrong command",
            Box::new(|p: &mut Value| p["command"] = json!("object.vector.create")),
        ),
        (
            "layers not an array",
            Box::new(|p: &mut Value| p["layers"] = json!("w")),
        ),
        (
            "record not an object",
            Box::new(|p: &mut Value| p["records"][0]["record"] = json!("nope")),
        ),
        (
            "unknown record member",
            Box::new(|p: &mut Value| p["records"][0]["record"]["extra"] = json!(1)),
        ),
        (
            "unknown layer member",
            Box::new(|p: &mut Value| p["layers"][0]["extra"] = json!(1)),
        ),
        (
            "bad skip kind",
            Box::new(|p: &mut Value| {
                p["skipped"] = json!([{"kind":"nope","sourceId":"s","reason":"r"}])
            }),
        ),
        (
            "empty skip reason",
            Box::new(|p: &mut Value| {
                p["skipped"] = json!([{"kind":"element","sourceId":"s","reason":" "}])
            }),
        ),
        (
            "bad provider name",
            Box::new(|p: &mut Value| p["unavailable"] = json!([{"sourceId":"s","need":"gpu"}])),
        ),
        (
            "caller-chosen stroke id",
            Box::new(|p: &mut Value| {
                p["records"][0]["record"]["target"]["strokeId"] = json!("mine")
            }),
        ),
        (
            "record for a missing layer",
            Box::new(|p: &mut Value| {
                p["records"][0]["record"]["target"]["layerUid"] = json!("nowhere")
            }),
        ),
        (
            "repeated source id",
            Box::new(|p: &mut Value| p["records"][1]["sourceId"] = json!("w:0")),
        ),
    ];
    for (name, mutate) in corrupt {
        let mut h = history(1);
        prime_redo(&mut h);
        let before = frozen(&h);
        let mut payload = wire();
        mutate(&mut payload);
        let req = request(&h, "bad-1", payload);
        let response = dispatch(&mut h, &req);
        assert_eq!(response["ok"], false, "{name}: {response}");
        assert!(
            ["invalid_request", "not_found"].contains(&code(&response)),
            "{name}: {response}"
        );
        assert_eq!(dispatch(&mut h, &req), response, "{name}: refusal replay");
        assert_eq!(frozen(&h), before, "{name}: nothing published, redo kept");
    }
}

#[test]
fn raw_byte_ingress_refuses_duplicate_members_trailing_bytes_and_oversize() {
    let h = history(1);
    let good = wire().to_string();
    assert!(parse_batch(good.as_bytes()).is_ok());
    let duplicate = good.replacen("\"layers\"", "\"layers\":[],\"layers\"", 1);
    let trailing = format!("{good} x");
    let oversize = format!("{good}{}", " ".repeat(MAX_PAYLOAD_BYTES));
    for (name, raw) in [
        ("duplicate", duplicate),
        ("trailing", trailing),
        ("oversize", oversize),
    ] {
        assert_eq!(
            h.prepare_import_batch_json(raw.as_bytes()).unwrap_err(),
            ImportError::Payload,
            "{name}"
        );
    }
}

#[test]
fn an_absent_provider_is_unavailable_with_the_fixed_message_and_nothing_published() {
    let mut h = history(1);
    let before = frozen(&h);
    let mut payload = wire();
    payload["unavailable"] = json!([{"sourceId":"psd:2","need":"raster-asset"}]);
    let req = request(&h, "psd-1", payload);
    let response = dispatch(&mut h, &req);
    assert_eq!(code(&response), "unavailable");
    assert_eq!(
        response["error"]["message"],
        "Import needs a provider that is unavailable."
    );
    assert_eq!(
        frozen(&h),
        before,
        "the three valid records were not published either"
    );
}

#[test]
fn a_receipt_that_would_not_fit_the_response_is_refused_before_anything_publishes() {
    // Many tiny layers: the request stays small, but every created layer echoes a native id.
    let batch = |n: usize| {
        let layers: Vec<Value> = (0..n)
            .map(|i| json!({"sourceId":format!("l{i:03}"),"name":"n"}))
            .collect();
        wire_batch(layers, vec![])
    };
    // The guard keeps the WORST-case envelope (three 128-byte identifiers plus fixed members) inside
    // the dispatcher's response bound.
    assert!(MAX_RECEIPT_BYTES + 3 * 128 + 256 <= 4096);
    let mut h = history(1);
    let before = frozen(&h);
    let big = request(&h, "big-1", batch(100));
    assert!(
        serde_json::to_vec(&big).unwrap().len() <= 4096,
        "the REQUEST fits"
    );
    let prepared = h
        .prepare_import_batch_json(batch(100).to_string().as_bytes())
        .unwrap();
    assert!(
        serde_json::to_vec(&prepared.result().result())
            .unwrap()
            .len()
            > 4096,
        "the RECEIPT cannot fit any 4096-byte response"
    );
    drop(prepared);
    let response = dispatch(&mut h, &big);
    assert_eq!(code(&response), "invalid_request");
    assert_eq!(
        response["error"]["message"],
        "Import receipt exceeds the response bound."
    );
    assert_eq!(
        frozen(&h),
        before,
        "refused before commit, so nothing published"
    );
    let small_request = request(&h, "small-1", batch(3));
    let small = dispatch(&mut h, &small_request);
    assert_eq!(small["ok"], true, "{small}");
    assert_eq!(small["result"]["layers"].as_array().unwrap().len(), 3);
}
