//! #2256 Q4: the `duplicator` selector of the component read. The panel shows the stored descriptor with the baseline
//! display defaults (`updateDuplicatorPanel`, 1065e9d8): `dup.mode||'grid'`, `dup.rows||1`, `dup.spacingX||0`, `!!` for
//! the checkboxes, `timeOffset.offsetFrames` 1 and `direction||'forward'` when unset.
use super::fixture::{decode, document, duplicator_table, layer};
use super::*;
use serde_json::{json, Value};

fn snapshot(descriptor: Value) -> ObjectSnapshot {
    let layers = [
        layer("ly_dup", "Host", &[(0, "h1")]),
        layer("ly_plain", "Plain", &[(0, "x1")]),
    ];
    let tables = json!({"duplicators":duplicator_table(&[("ly_dup", descriptor)])});
    let document = decode(&document(&layers, json!({"componentTables":tables})));
    ObjectSnapshot::new("dup-read", document).expect("snapshot")
}
fn ask(snapshot: &ObjectSnapshot, layer_uid: &str) -> ResponseEnvelope {
    let request: OpacityRequest = serde_json::from_value(json!({"apiVersion":2,"requestId":"dup-read",
        "instanceId":snapshot.instance_id(),"documentId":snapshot.document_id(),
        "operation":"query.document.object.component.read",
        "payload":{"atRevision":snapshot.content_revision(),"documentSnapshotId":snapshot.snapshot_id(),
            "selector":{"kind":"duplicator","layerUid":layer_uid}}}))
    .unwrap();
    snapshot.dispatch(request).unwrap()
}

#[test]
fn a_stored_descriptor_is_shown_member_by_member() {
    let snap = snapshot(
        json!({"mode":"radial","rows":4,"cols":5,"spacingX":12.5,"spacingY":-3,"count":9,"radius":321,
        "startAngle":30,"radialOrient":true,"pathLayerUid":null,"pathAlignTangent":true,"seed":4242,
        "staggerRandom":{"position":true,"hue":true},
        "timeOffset":{"enabled":true,"offsetFrames":3,"direction":"centerOut"},"sourceMode":"objects"}),
    );
    let response = ask(&snap, "ly_dup");
    assert!(response.ok, "{:?}", response.error);
    let result = response.result.unwrap();
    assert_eq!(result["kind"], "duplicator");
    assert_eq!(result["atRevision"], snap.content_revision());
    assert_eq!(result["documentSnapshotId"], snap.snapshot_id());
    let expected = json!({"layerUid":"ly_dup","mode":"radial","rows":4,"cols":5,"spacingX":12.5,"spacingY":-3,
        "count":9,"radius":321,"startAngle":30,"radialOrient":true,"pathAlignTangent":true,"pathLayerUid":null,
        "seed":4242,"sourceMode":"objects",
        "staggerRandom":{"position":true,"rotation":false,"scale":false,"opacity":false,"hue":true},
        "timeOffset":{"enabled":true,"offsetFrames":3,"direction":"centerOut"}});
    for (key, want) in expected.as_object().unwrap() {
        assert_eq!(&result[key], want, "{key}");
    }
}

#[test]
fn an_unauthored_descriptor_shows_the_baseline_display_defaults() {
    let result = ask(&snapshot(json!({})), "ly_dup").result.unwrap();
    let expected = json!({"mode":"grid","rows":1,"cols":1,"spacingX":0,"spacingY":0,"count":1,"radius":0,"startAngle":0,
        "radialOrient":false,"pathAlignTangent":false,"pathLayerUid":null,"seed":0,"sourceMode":"shape",
        "staggerRandom":{"position":false,"rotation":false,"scale":false,"opacity":false,"hue":false},
        "timeOffset":{"enabled":false,"offsetFrames":1,"direction":"forward"}});
    for (key, want) in expected.as_object().unwrap() {
        assert_eq!(&result[key], want, "{key}");
    }
    // `dup.rows||1`: a stored zero shows the fallback, as the baseline field does.
    let zero = ask(&snapshot(json!({"rows":0,"cols":0,"count":0})), "ly_dup")
        .result
        .unwrap();
    assert_eq!(
        (&zero["rows"], &zero["cols"], &zero["count"]),
        (&json!(1), &json!(1), &json!(1))
    );
}

#[test]
fn a_layer_without_a_duplicator_or_an_unknown_layer_is_not_found_and_a_stale_pin_is_refused() {
    let snap = snapshot(json!({}));
    for uid in ["ly_plain", "nope"] {
        let response = ask(&snap, uid);
        assert!(!response.ok && response.result.is_none(), "{uid}");
        assert_eq!(
            response.error.unwrap().code(),
            crate::request_receipts::DispatchErrorCode::NotFound,
            "{uid}"
        );
    }
    let stale: OpacityRequest =
        serde_json::from_value(json!({"apiVersion":2,"requestId":"dup-read",
        "instanceId":snap.instance_id(),"documentId":snap.document_id(),
        "operation":"query.document.object.component.read",
        "payload":{"atRevision":snap.content_revision() + 1,"documentSnapshotId":snap.snapshot_id(),
            "selector":{"kind":"duplicator","layerUid":"ly_dup"}}}))
        .unwrap();
    assert_eq!(
        snap.dispatch(stale).unwrap().error.unwrap().code(),
        crate::request_receipts::DispatchErrorCode::StaleRevision
    );
    for selector in [
        json!({"kind":"duplicator"}),
        json!({"kind":"duplicator","layerUid":""}),
        json!({"kind":"duplicator","layerUid":"ly_dup","cursor":0}),
    ] {
        let bad: OpacityRequest = serde_json::from_value(json!({"apiVersion":2,"requestId":"dup-read",
            "instanceId":snap.instance_id(),"documentId":snap.document_id(),
            "operation":"query.document.object.component.read",
            "payload":{"atRevision":snap.content_revision(),"documentSnapshotId":snap.snapshot_id(),"selector":selector}}))
        .unwrap();
        assert_eq!(
            snap.dispatch(bad).unwrap().error.unwrap().code(),
            crate::request_receipts::DispatchErrorCode::InvalidRequest
        );
    }
}
