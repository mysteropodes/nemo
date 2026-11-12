//! Frozen pressure edits through native read/preview, command/history and both storage formats.
#[path = "object_vector_edit_support.rs"]
#[allow(dead_code)]
mod support;
use crate::history::NativeObjectHistory;
use crate::object_codec::{decode_storage_project, subselect::record};
use crate::object_document::subselect::point_type::tests::baseline_intent;
use serde_json::{json, Value};
use support::{bytes, dispatch, document, owner_of, record_at, state, target};
fn oracle() -> Value {
    serde_json::from_str(include_str!("fixtures/subselect-baseline/pressure.json")).unwrap()
}
fn seg(s: &Value) -> Value {
    let p = |k: &str| json!({"x":s[k][0],"y":s[k][1]});
    let mut s2 = json!({"point":p("point"),"handleIn":p("handleIn"),"handleOut":p("handleOut")});
    if let Some(width) = s.get("width") {
        s2["width"] = width.clone();
    }
    s2
}
fn ribbon(case: &Value) -> Value {
    let mut r = record_at("A", "L1", 0, "s-a");
    r["geometry"]["closed"] = json!(true);
    r["geometry"]["segments"] = case["before"]["outline"]
        .as_array()
        .unwrap()
        .iter()
        .map(seg)
        .collect();
    r["vector"]["stroke"] = Value::Null;
    r["vector"]["hasRealStroke"] = json!(false);
    r["fill"] = json!({"kind":"solid","r":1,"g":0,"b":0,"a":1});
    r["vectorExt"] = json!({"pressure":{"centerSegments":case["before"]["center"].as_array().unwrap().iter().map(seg).collect::<Vec<_>>(),"strokeProfile":"bulge","profileBase":9007199254740993u64}});
    if !case["profile"].is_null() {
        r["vectorExt"]["pressure"]["widthProfile"] = case["profile"].clone();
    }
    r
}
fn request(owner: &NativeObjectHistory, intent: &Value) -> Value {
    let mut q = support::request(
        owner,
        "pressure-edit",
        json!({"command":"object.subselect.edit","stableTarget":target("L1",0,"s-a"),"intent":intent}),
    );
    q["operation"] = json!("command.document.object.subselect.edit");
    q
}
fn assert_segments(actual: &Value, expected: &Value) {
    let (actual, expected) = (actual.as_array().unwrap(), expected.as_array().unwrap());
    assert_eq!(actual.len(), expected.len());
    for (a, b) in actual.iter().zip(expected) {
        for key in ["point", "handleIn", "handleOut"] {
            for (axis, n) in [("x", 0), ("y", 1)] {
                assert!(
                    (a[key][axis].as_f64().unwrap() - b[key][n].as_f64().unwrap()).abs() < 1e-8,
                    "{key}.{axis}: {a} != {b}"
                );
            }
        }
        if let Some(width) = b.get("width") {
            assert_eq!(&a["width"], width);
        }
    }
}
#[test]
fn subselect_pressure_all_180_edits_preserve_metadata_preview_history_and_reopen() {
    let oracle = oracle();
    assert_eq!(oracle["cases"].as_array().unwrap().len(), 180);
    for version in [1, 2] {
        for (index, case) in oracle["cases"].as_array().unwrap().iter().enumerate() {
            let input = ribbon(case);
            let mut owner = owner_of(&document(version, vec![input.clone()], None));
            let before = bytes(&owner);
            let snapshot = owner.acquire_snapshot(0).unwrap();
            let intent = serde_json::from_value(case["intent"].clone()).unwrap();
            // #2356: Corner is `collapse` against the frozen outlines; a case whose rule the owner decision replaced is skipped here.
            let Some(intent) = baseline_intent(&record::anchors(&snapshot.document().objects()[0]).unwrap(), &intent) else { continue };
            let case_intent = serde_json::to_value(&intent).unwrap();
            let planned = record::plan(&snapshot.document().objects()[0], &intent).unwrap();
            let read = json!({"apiVersion":2,"requestId":"pressure-preview","instanceId":owner.instance_id(),"documentId":owner.document_id(),"operation":"query.document.object","payload":{"atRevision":0,"documentSnapshotId":snapshot.snapshot_id(),"stableTarget":target("L1",0,"s-a"),"subselect":{"cursor":0,"limit":16,"preview":case_intent}}});
            let preview = serde_json::to_value(
                snapshot
                    .dispatch(serde_json::from_value(read).unwrap())
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(preview["ok"], true, "case {index}: {preview}");
            assert_eq!(preview["result"]["subselect"]["closed"], false);
            assert_eq!(
                preview["result"]["subselect"]["total"],
                case["expected"]["center"].as_array().unwrap().len()
            );
            assert_eq!(bytes(&owner), before);
            let q = request(&owner, &case_intent);
            let response = dispatch(&mut owner, &q);
            assert_eq!(response["ok"], true, "case {index}: {response}");
            let actual = state(&owner);
            let actual = &actual["objects"][0];
            assert_eq!(serde_json::to_value(planned).unwrap(), *actual);
            assert_segments(
                &actual["geometry"]["segments"],
                &case["expected"]["outline"],
            );
            assert_segments(
                &actual["vectorExt"]["pressure"]["centerSegments"],
                &case["expected"]["center"],
            );
            let mut old_meta = input["vectorExt"]["pressure"].clone();
            old_meta.as_object_mut().unwrap().remove("centerSegments");
            let mut new_meta = actual["vectorExt"]["pressure"].clone();
            new_meta.as_object_mut().unwrap().remove("centerSegments");
            assert_eq!(new_meta, old_meta);
            for (a, b) in preview["result"]["subselect"]["segments"]
                .as_array()
                .unwrap()
                .iter()
                .zip(case["expected"]["center"].as_array().unwrap())
            {
                for key in ["point", "handleIn", "handleOut"] {
                    for n in [0, 1] {
                        assert!(
                            (a[key][n].as_f64().unwrap() - b[key][n].as_f64().unwrap()).abs()
                                < 1e-8
                        );
                    }
                }
            }
            let after = bytes(&owner);
            assert_eq!(dispatch(&mut owner, &q), response);
            if after != before {
                assert_eq!(owner.history_depths(), (1, 0));
                owner.undo(1).unwrap();
                assert_eq!(bytes(&owner), before);
                owner.redo(2).unwrap();
                assert_eq!(bytes(&owner), after);
            } else {
                assert_eq!(owner.history_depths(), (0, 0));
            }
            let reopened = NativeObjectHistory::new(
                "pressure-reopen",
                decode_storage_project(&after).unwrap(),
            )
            .unwrap();
            assert_eq!(bytes(&reopened), after);
        }
    }
}
#[test]
fn subselect_pressure_delete_keeps_surviving_widths_and_refuses_partial_consumers() {
    let o = oracle();
    let input = ribbon(&o["cases"][0]);
    let mut owner = owner_of(&document(2, vec![input.clone()], None));
    let q = request(&owner, &json!({"kind":"delete","indices":[1]}));
    let r = dispatch(&mut owner, &q);
    assert_eq!(r["ok"], true, "{r}");
    let result = state(&owner);
    let center = &result["objects"][0]["vectorExt"]["pressure"]["centerSegments"];
    assert_eq!(center.as_array().unwrap().len(), 2);
    assert_eq!(
        center[0],
        input["vectorExt"]["pressure"]["centerSegments"][0]
    );
    assert_eq!(
        center[1],
        input["vectorExt"]["pressure"]["centerSegments"][2]
    );
    let owner = owner_of(&document(2, vec![input.clone()], None));
    let record = &owner.acquire_snapshot(0).unwrap().document().objects()[0].clone();
    let insertion = serde_json::from_value(json!({"kind":"insert","curve":0,"time":0.5})).unwrap();
    assert!(record::plan(record, &insertion).is_err());
    for extra in [
        json!({"linkedFillId":"linked"}),
        json!({"isFillShape":true}),
    ] {
        let mut r = input.clone();
        r["vectorExt"]
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        let record = serde_json::from_value(r).unwrap();
        assert!(record::anchors(&record).is_err());
    }
}

#[test]
fn subselect_pressure_paint_pages_retain_the_same_preview_origin_and_bounded_outline() {
    let o = oracle();
    let case = &o["cases"][15];
    let owner = owner_of(&document(2, vec![ribbon(case)], None));
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let before = bytes(&owner);
    for preview in [false, true] {
        let mut cursor = 0;
        let mut segments = vec![];
        loop {
            let mut page = json!({"cursor":cursor,"limit":8,"paint":true});
            if preview {
                page["preview"] = case["intent"].clone();
            }
            let q = json!({"apiVersion":2,"requestId":"paint-page","instanceId":owner.instance_id(),"documentId":owner.document_id(),"operation":"query.document.object","payload":{"atRevision":0,"documentSnapshotId":snapshot.snapshot_id(),"stableTarget":target("L1",0,"s-a"),"subselect":page}});
            let r = serde_json::to_value(
                snapshot
                    .dispatch(serde_json::from_value(q).unwrap())
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(r["ok"], true, "{r}");
            assert!(serde_json::to_vec(&r).unwrap().len() <= 4096);
            let r = &r["result"]["subselect"];
            assert_eq!(r["closed"], true);
            assert_eq!(r["paint"], true);
            assert_eq!(r["preview"], preview);
            assert_eq!(r["cursor"], cursor);
            segments.extend(r["segments"].as_array().unwrap().iter().map(seg));
            if r["nextCursor"].is_null() {
                assert_eq!(r["total"], segments.len());
                break;
            }
            cursor = r["nextCursor"].as_u64().unwrap() as usize;
        }
        assert_segments(
            &json!(segments),
            &case[if preview { "expected" } else { "before" }]["outline"],
        );
    }
    for paint in [json!(false), Value::Null, json!("true")] {
        let q = json!({"apiVersion":2,"requestId":"paint-invalid","instanceId":owner.instance_id(),"documentId":owner.document_id(),"operation":"query.document.object","payload":{"atRevision":0,"documentSnapshotId":snapshot.snapshot_id(),"stableTarget":target("L1",0,"s-a"),"subselect":{"cursor":0,"limit":8,"paint":paint}}});
        let r = serde_json::to_value(
            snapshot
                .dispatch(serde_json::from_value(q).unwrap())
                .unwrap(),
        )
        .unwrap();
        assert_eq!(r["error"]["code"], "invalid_request");
    }
    assert_eq!(bytes(&owner), before);
    assert_eq!(owner.history_depths(), (0, 0));
}
