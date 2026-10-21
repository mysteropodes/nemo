use super::*;
use crate::contract::{NativeApplicationRequest, NativeApplicationResponse};
use serde_json::json;
fn query(selector: Value) -> NativeApplicationRequest {
    serde_json::from_value(json!({"apiVersion":2,"requestId":"catalog-read","instanceId":"i","documentId":"d",
        "operation":READ,"payload":{"atRevision":7,"documentSnapshotId":"s7","contextId":"scene-root","selector":selector}})).unwrap()
}
fn edit(action: Value) -> NativeApplicationRequest {
    serde_json::from_value(json!({"apiVersion":2,"requestId":"catalog-edit","instanceId":"i","documentId":"d",
        "operation":EDIT,"expectedRevision":7,"payload":{"command":"object.project.customBrushPreset.edit","action":action}})).unwrap()
}
fn response(q: &NativeApplicationRequest, body: Value) -> NativeApplicationResponse {
    let body = if q.operation == READ {
        json!({"instanceId":"i","documentId":"d","atRevision":7,"documentSnapshotId":"s7","contextId":"scene-root","result":body})
    } else {
        body
    };
    serde_json::from_value(
        json!({"apiVersion":2,"requestId":q.request_id,"instanceId":"i","documentId":"d",
        "contentRevision":7,"ok":true,"result":body}),
    )
    .unwrap()
}
fn wire(q: &NativeApplicationRequest, r: &NativeApplicationResponse) -> bool {
    crate::wire::validate_native_response(r, q).is_ok()
}
#[test]
fn all_original_typed_optional_fields_and_raw_ids_are_admitted_without_normalization() {
    let mut value = json!({"markerTip":false,"customStamp":{"segments":[{"point":[0,-1],"handleOut":[2,3]}],"closed":false,"pointCount":0}});
    for key in NUMBERS {
        value[key] = json!(-0.5);
    }
    for key in STRINGS {
        value[key] = json!("\0雪");
    }
    for id in ["", "2", "01", "__proto__", "raw\0雪"] {
        let q = edit(json!({"kind":"upsert","id":id,"preset":value}));
        assert!(q.validate_input().is_ok());
        assert!(q.validate().is_ok());
        assert!(edit(json!({"kind":"delete","id":id}))
            .validate_input()
            .is_ok());
    }
    for key in NUMBERS
        .into_iter()
        .chain(STRINGS)
        .chain(["markerTip", "customStamp"])
    {
        let mut bad = json!({});
        bad[key] = Value::Null;
        assert!(edit(json!({"kind":"upsert","id":"raw","preset":bad}))
            .validate_input()
            .is_err());
    }
    for value in [
        json!({"unknown":1}),
        json!({"customStamp":{"segments":[{"point":[1]}]}}),
        json!({"customStamp":{"segments":[],"unknown":1}}),
        json!({"label":false}),
    ] {
        assert!(edit(json!({"kind":"upsert","id":"x","preset":value}))
            .validate_input()
            .is_err());
    }
    let mut q = edit(json!({"kind":"delete","id":"missing"}));
    q.expected_revision = None;
    assert!(q.validate_input().is_err());
    assert!(edit(json!({"kind":"rename","id":"x"}))
        .validate_input()
        .is_err());
}
#[test]
fn ordered_pages_preserve_numeric_and_empty_identity_and_require_strict_progress() {
    let q = query(json!({"kind":"catalogPage","cursor":0,"limit":2}));
    assert!(q.validate_input().is_ok());
    let page = json!({"kind":"customBrushPresetPage","customBrushPresetsPresent":true,"presetCount":3,
        "cursor":0,"items":[{"ordinal":1,"id":"10","label":""},{"ordinal":2,"id":"2"}],"nextCursor":2});
    assert!(wire(&q, &response(&q, page.clone())));
    for (key, bad) in [
        ("nextCursor", json!(1)),
        ("items", json!([])),
        ("cursor", json!(1)),
        (
            "items",
            json!([{"ordinal":1,"id":"2"},{"ordinal":2,"id":"2"}]),
        ),
    ] {
        let mut value = page.clone();
        value[key] = bad;
        assert!(!wire(&q, &response(&q, value)));
    }
    for limit in [0, 17] {
        assert!(
            query(json!({"kind":"catalogPage","cursor":0,"limit":limit}))
                .validate_input()
                .is_err()
        );
    }
    let q = query(json!({"kind":"settings"}));
    assert!(wire(
        &q,
        &response(
            &q,
            json!({"customBrushPresetsPresent":false,"presetCount":0})
        )
    ));
    assert!(wire(
        &q,
        &response(
            &q,
            json!({"customBrushPresetsPresent":true,"presetCount":0})
        )
    ));
    assert!(!wire(
        &q,
        &response(
            &q,
            json!({"customBrushPresetsPresent":false,"presetCount":1})
        )
    ));
}
#[test]
fn raw_record_correlation_budget_and_noop_receipt_are_enforced_by_the_real_wire() {
    let q = query(json!({"kind":"record","id":"\0"}));
    let body = json!({"kind":"customBrushPresetRecord","id":"\0","preset":{"label":"","savedBrushSize":0}});
    assert!(wire(&q, &response(&q, body.clone())));
    let mut r = response(&q, body.clone());
    r.result.as_mut().unwrap()["documentSnapshotId"] = json!("stale");
    assert!(!wire(&q, &r));
    let mut wrong = body.clone();
    wrong["id"] = json!("other");
    assert!(!wire(&q, &response(&q, wrong)));
    let mut huge = body;
    huge["preset"]["label"] = json!("x".repeat(4096));
    assert!(!wire(&q, &response(&q, huge)));
    let q = edit(json!({"kind":"delete","id":"missing"}));
    assert!(wire(
        &q,
        &response(&q, json!({"applied":false,"historyEntriesAdded":0}))
    ));
    assert!(!wire(
        &q,
        &response(&q, json!({"applied":false,"historyEntriesAdded":1}))
    ));
    let mut q = edit(json!({"kind":"upsert","id":"x","preset":{}}));
    q.payload["action"]["preset"]["label"] = json!("x".repeat(4096));
    assert!(q.validate_input().is_err());
}
