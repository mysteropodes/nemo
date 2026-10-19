//! Same real clipboard history owner, no alternate clipboard/receipt authority.
use super::tests::{build, dispatch, owner, request, rows, state};
use crate::object_codec::decode_storage_project;
use crate::request_receipts::frame_clipboard_transport as budget;
use serde_json::{json, Value};
const COPY: &str = "command.document.object.frames.clipboard.copy";
const CUT: &str = "command.document.object.frames.clipboard.cut";
fn payload(command: &str) -> Value {
    json!({"command":command,"selection":[{"layerUid":"L0","frame":0}]})
}
#[test]
fn native_limits_only_change_copy_cut_paste_and_keep_record_order_data_bound() {
    for op in [COPY, CUT, "command.document.object.frames.clipboard.paste"] {
        assert_eq!(budget::request_limit(op), 5_246_976);
    }
    for op in [
        "command.document.object.frames.repeat",
        "command.document.object.layer.key.blank",
        "command.document.object.layer.key.move",
        "command.document.object.frames.clipboard.duplicate",
    ] {
        assert_eq!(budget::request_limit(op), 4096);
        assert_eq!(budget::response_limit(op), 4096);
    }
    let mut total = 0;
    assert!(budget::charge(
        &mut total,
        &"x".repeat(budget::DATA_BYTES - 2)
    ));
    assert_eq!(total, budget::DATA_BYTES);
    assert!(!budget::charge(&mut total, &0));
    assert_eq!(total, budget::DATA_BYTES);
}
#[test]
fn real_large_copy_cut_paste_and_retained_replay_share_one_owner() {
    let input = build::document(
        &rows::parse_layers("K=a.1.1,b.1.2,c.1.3,d.1.4,e.1.5,f.1.6,g.1.7,h.1.8;.;.;."),
        build::Build::V1,
    );
    let mut o = owner(input);
    let before = state(&o);
    let copy = request(
        &o,
        "large-copy",
        COPY,
        payload("object.frames.clipboard.copy"),
    );
    let copied = dispatch(&mut o, &copy);
    assert_eq!(copied["ok"], true);
    assert!(serde_json::to_vec(&copied).unwrap().len() > 4096);
    assert_eq!(state(&o), before);
    let cut = request(&o, "large-cut", CUT, payload("object.frames.clipboard.cut"));
    let cut_reply = dispatch(&mut o, &cut);
    assert_eq!(cut_reply["ok"], true);
    assert_eq!(
        cut_reply["result"]["clipboard"],
        copied["result"]["clipboard"]
    );
    assert_eq!(o.history_depths(), (1, 0));
    let packet = &copied["result"]["clipboard"];
    let paste = request(
        &o,
        "large-paste",
        "command.document.object.frames.clipboard.paste",
        json!({"command":"object.frames.clipboard.paste","instanceId":packet["instanceId"],"documentId":packet["documentId"],
            "layerUid":"L0","frame":1,"cells":packet["cells"]}),
    );
    assert!(serde_json::to_vec(&paste).unwrap().len() > 4096);
    assert_eq!(dispatch(&mut o, &paste)["ok"], true);
    let after = state(&o);
    assert_eq!(dispatch(&mut o, &copy), copied);
    assert_eq!(dispatch(&mut o, &cut), cut_reply);
    assert_eq!(state(&o), after);
    let mut changed = cut.clone();
    changed["payload"]["selection"][0]["frame"] = json!(1);
    assert_eq!(
        dispatch(&mut o, &changed)["error"]["code"],
        "invalid_request"
    );
    assert_eq!(state(&o), after);
}
#[test]
fn actual_copy_packet_over_native_four_mib_refuses_cut_before_history_and_retains_refusal() {
    let mut source = build::document_json(&rows::parse_layers("K=a.1.1;.;.;."), build::Build::V1);
    let prototype = source["objects"][0].clone();
    let mut records = Vec::new();
    for index in 0..20_000 {
        let mut r = prototype.clone();
        r["target"]["strokeId"] = json!(format!("record-{index}"));
        records.push(r);
    }
    assert!(
        records
            .iter()
            .map(|r| serde_json::to_vec(r).unwrap().len())
            .sum::<usize>()
            > budget::DATA_BYTES
    );
    source["objects"] = json!(records);
    let mut o = owner(decode_storage_project(&serde_json::to_vec(&source).unwrap()).unwrap());
    let before = state(&o);
    let q = request(
        &o,
        "packet-overflow",
        CUT,
        payload("object.frames.clipboard.cut"),
    );
    let reply = dispatch(&mut o, &q);
    assert_eq!(reply["error"]["code"], "invalid_request");
    assert_eq!(state(&o), before);
    assert_eq!(dispatch(&mut o, &q), reply);
    assert_eq!(state(&o), before);
}
#[test]
fn full_native_request_at_limit_reaches_real_target_lookup_and_over_refuses_atomically() {
    let mut o = owner(build::document(
        &rows::parse_layers("K=a.1.1;.;.;."),
        build::Build::V1,
    ));
    let before = state(&o);
    let mut q = request(
        &o,
        "at-full-limit",
        COPY,
        payload("object.frames.clipboard.copy"),
    );
    q["payload"]["selection"][0]["layerUid"] = json!("");
    let base = serde_json::to_vec(&q).unwrap().len();
    q["payload"]["selection"][0]["layerUid"] = json!("x".repeat(budget::MESSAGE_BYTES - base));
    assert_eq!(serde_json::to_vec(&q).unwrap().len(), budget::MESSAGE_BYTES);
    assert_eq!(dispatch(&mut o, &q)["error"]["code"], "not_found");
    assert_eq!(state(&o), before);
    q["requestId"] = json!("overfulllimit");
    q["payload"]["selection"][0]["layerUid"] = json!(format!(
        "{}x",
        q["payload"]["selection"][0]["layerUid"].as_str().unwrap()
    ));
    assert_eq!(
        serde_json::to_vec(&q).unwrap().len(),
        budget::MESSAGE_BYTES + 1
    );
    assert_eq!(dispatch(&mut o, &q)["error"]["code"], "invalid_request");
    assert_eq!(state(&o), before);
}

#[test]
fn native_valid_record_rich_copy_and_paste_above_4096_records_keep_native_four_mib_bound() {
    let mut source = build::document_json(&rows::parse_layers("K=a.1.1;.;.;."), build::Build::V1);
    let prototype = source["objects"][0].clone();
    let mut records = Vec::new();
    for index in 0..4100 {
        let mut record = prototype.clone();
        record["target"]["strokeId"] = json!(format!("record-{index}"));
        records.push(record);
    }
    assert!(
        records
            .iter()
            .map(|r| serde_json::to_vec(r).unwrap().len())
            .sum::<usize>()
            <= budget::DATA_BYTES
    );
    source["objects"] = json!(records);
    let mut o = owner(decode_storage_project(&serde_json::to_vec(&source).unwrap()).unwrap());
    let before = state(&o);
    let copy = request(
        &o,
        "record-rich-copy",
        COPY,
        payload("object.frames.clipboard.copy"),
    );
    let copied = dispatch(&mut o, &copy);
    assert_eq!(copied["ok"], true);
    assert_eq!(
        copied["result"]["clipboard"]["cells"][0]["records"]
            .as_array()
            .unwrap()
            .len(),
        4100
    );
    assert_eq!(state(&o), before);
    let packet = &copied["result"]["clipboard"];
    let paste = request(
        &o,
        "record-rich-paste",
        "command.document.object.frames.clipboard.paste",
        json!({"command":"object.frames.clipboard.paste","instanceId":packet["instanceId"],"documentId":packet["documentId"],
            "layerUid":"L0","frame":1,"cells":packet["cells"]}),
    );
    assert_eq!(dispatch(&mut o, &paste)["ok"], true);
    assert_eq!(o.history_depths(), (1, 0));
}
