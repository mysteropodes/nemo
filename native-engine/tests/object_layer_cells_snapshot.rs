//! Whole-row cells view against the single-cell read of the same timing query (U31, #2374).
use super::*;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::object_document::ObjectDocument;
use crate::request_receipts::OpacityRequest;
#[path = "object_frame_edit_document_build.rs"]
#[allow(dead_code)]
mod build;
#[path = "object_exposure_edit_key_rows.rs"]
#[allow(dead_code)]
mod rows;
fn snapshot(rows_text: &str) -> ObjectSnapshot {
    let d: ObjectDocument = build::document(&rows::parse_layers(rows_text), build::Build::V2Extras);
    ObjectSnapshot::from_admitted("cells-read".into(), d).unwrap()
}
fn selector(s: &ObjectSnapshot, layer: &str, frame: u32) -> Value {
    json!({"atRevision":s.content_revision(),"documentSnapshotId":s.snapshot_id(),
        "contextId":"scene-root","layerUid":layer,"frame":frame,"view":"cells"})
}
fn dispatch(s: &ObjectSnapshot, payload: &Value) -> crate::request_receipts::ResponseEnvelope {
    let typed: OpacityRequest =
        serde_json::from_value(json!({"apiVersion":2,"requestId":"cells-query",
        "instanceId":s.instance_id(),"documentId":s.document_id(),
        "operation":"query.document.object.layer.timing","payload":payload}))
        .unwrap();
    s.dispatch_layer_timing(&typed).unwrap()
}
/// Every page through the public dispatcher, each complete envelope inside the message bound.
fn pages(s: &ObjectSnapshot, layer: &str) -> (Vec<Value>, usize) {
    let (mut runs, mut count, mut frame) = (Vec::new(), 0, Some(0));
    while let Some(start) = frame {
        let response = dispatch(s, &selector(s, layer, start));
        assert!(response.ok, "{layer}@{start}");
        assert!(serde_json::to_vec(&response).unwrap().len() <= 4096);
        let result = response.result.unwrap();
        assert_eq!(result["frame"], json!(start));
        runs.extend(result["cells"]["runs"].as_array().unwrap().iter().cloned());
        frame = result["cells"]["nextFrame"].as_u64().map(|f| f as u32);
        count += 1;
    }
    (runs, count)
}
fn single(s: &ObjectSnapshot, layer: &str, frame: u32) -> Value {
    let q = json!({"atRevision":s.content_revision(),"documentSnapshotId":s.snapshot_id(),
        "contextId":"scene-root","layerUid":layer,"frame":frame});
    let r = s.query_layer_timing_json(q.to_string().as_bytes()).unwrap();
    json!([r.cell_kind, r.source_frame, r.has_content, r.manual_edit])
}
/// Each run expanded frame by frame equals the single-cell read; the runs tile the row exactly once.
fn assert_row_matches(s: &ObjectSnapshot, layer: &str, runs: &[Value]) {
    let mut frame = 0;
    for run in runs {
        let (start, count) = (
            run[0].as_u64().unwrap() as u32,
            run[1].as_u64().unwrap() as u32,
        );
        assert_eq!(start, frame);
        assert!(count >= 1);
        for f in start..start + count {
            let source = if run[2] == "interpolated" {
                json!(f)
            } else {
                run[3].clone()
            };
            assert_eq!(
                json!([run[2], source, run[4], run[5]]),
                single(s, layer, f),
                "{layer}@{f}"
            );
        }
        frame = start + count;
    }
    assert_eq!(frame, s.document().total_frames);
}

#[test]
fn cells_runs_reproduce_the_single_cell_read_for_keys_holds_samples_and_blanks() {
    let s = snapshot("K=a.1.10;.;I;M;N;K=b.2.20;.;.|.;.;K;.;.;.;.;.");
    let before = encode_storage_project(s.document()).unwrap();
    let (l0, pages0) = pages(&s, "L0");
    assert_eq!(pages0, 1);
    assert_eq!(
        Value::Array(l0.clone()),
        json!([
            [0, 1, "key", 0, true, false],
            [1, 1, "held", 0, true, false],
            [2, 1, "interpolated", 2, false, false],
            [3, 1, "interpolated", 3, false, true],
            [4, 1, "interpolated", 4, false, false],
            [5, 1, "key", 5, true, false],
            [6, 2, "held", 5, true, false]
        ])
    );
    assert_row_matches(&s, "L0", &l0);
    let (l1, _) = pages(&s, "L1");
    assert_eq!(
        Value::Array(l1.clone()),
        json!([
            [0, 2, "unexposed", null, false, false],
            [2, 1, "key", 2, false, false],
            [3, 5, "held", 2, false, false]
        ])
    );
    assert_row_matches(&s, "L1", &l1);
    let mid = s
        .query_layer_cells_json(selector(&s, "L1", 4).to_string().as_bytes())
        .unwrap();
    assert_eq!(
        mid["cells"],
        json!({"runs":[[4,4,"held",2,false,false]],"nextFrame":null})
    );
    assert_eq!(
        encode_storage_project(s.document()).unwrap(),
        before,
        "a read changes nothing"
    );
}

#[test]
fn a_long_row_pages_through_next_frame_with_every_envelope_inside_the_bound() {
    let row: Vec<&str> = (0..600)
        .map(|f| if f % 2 == 0 { "K" } else { "M" })
        .collect();
    let s = snapshot(&row.join(";"));
    let (runs, count) = pages(&s, "L0");
    assert!(count >= 3, "{count} pages");
    assert_eq!(runs.len(), 600);
    assert_row_matches(&s, "L0", &runs);
}

#[test]
fn cells_view_refuses_stale_unknown_and_out_of_range_selectors_and_a_layer_without_timing() {
    let s = snapshot("K=a.1.10;.;.|K;.;.");
    let read = |q: &Value| s.query_layer_cells_json(q.to_string().as_bytes());
    let mut q = selector(&s, "L0", 0);
    q["atRevision"] = json!(1);
    assert_eq!(read(&q), Err(DispatchErrorCode::StaleRevision));
    for (field, bad, expected) in [
        ("frame", json!(3), DispatchErrorCode::InvalidRequest),
        ("view", json!("cell"), DispatchErrorCode::InvalidRequest),
        ("layerUid", json!("L9"), DispatchErrorCode::NotFound),
        (
            "contextId",
            json!("component:x"),
            DispatchErrorCode::Unavailable,
        ),
        (
            "documentSnapshotId",
            json!("other"),
            DispatchErrorCode::StaleRevision,
        ),
    ] {
        let mut q = selector(&s, "L0", 0);
        q[field] = bad;
        assert_eq!(read(&q), Err(expected), "{field}");
    }
    let mut v = serde_json::to_value(s.document()).unwrap();
    v["layers"][0].as_object_mut().unwrap().remove("timing");
    let bare = ObjectSnapshot::from_admitted(
        "cells-read".into(),
        decode_storage_project(v.to_string().as_bytes()).unwrap(),
    )
    .unwrap();
    let q = selector(&bare, "L0", 0);
    assert_eq!(
        bare.query_layer_cells_json(q.to_string().as_bytes()),
        Err(DispatchErrorCode::Unavailable)
    );
    let response = dispatch(&bare, &q);
    assert!(!response.ok && response.result.is_none());
}
