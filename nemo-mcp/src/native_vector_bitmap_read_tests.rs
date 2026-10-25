//! Stored bitmap output crosses the full immutable query contract, never vector writers.
use super::*;
use serde_json::json;

fn anchor() -> Value {
    let point =
        |x, y| json!({"point":{"x":x,"y":y},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}});
    json!({"schemaVersion":1,"family":"editable-vector-path",
        "target":{"contextId":"scene-root","layerUid":"L","strokeId":"anchor","frameScope":{"kind":"authored","frame":3}},
        "geometry":{"kind":"cubic-path","closed":true,"coordinateSpace":"document-world","handleSpace":"relative","segments":[point(4,4),point(40,4),point(4,40)]},
        "fill":{"kind":"solid","r":0,"g":1,"b":0,"a":1},
        "vector":{"stroke":null,"hasRealStroke":false,"strokeWidth":8,"strokeCap":"round","strokeJoin":"round","opacity":1},
        "vectorExt":{"bitmapBrush":{"bitmapBrushSpec":{"tip":"hard","size":8,"spacing":15,"scatter":0,"opacity":1,"color":"#0000ffff","pressure":true,"seed":17},
            "brushGroupId":"ink group:雪","preTextureStroke":null,"bitmapPressureProfile":[[4,4,0.2],[40,4,2.5]]}}})
}
fn payload(record: &Value) -> Value {
    json!({"atRevision":7,"stableTarget":record["target"]})
}
fn result(record: &Value) -> Value {
    json!({"atRevision":7,"documentSnapshotId":"native-object:document:7","object":record})
}
fn admits(record: &Value) -> bool {
    crate::native_contract::validate_result(
        "query.document.object",
        "document",
        &payload(record),
        &result(record),
    )
}
fn request(record: &Value) -> NativeApplicationRequest {
    serde_json::from_value(json!({"apiVersion":2,"requestId":"bitmap-read","instanceId":"instance","documentId":"document","operation":"query.document.object","payload":payload(record)})).unwrap()
}
fn response(record: &Value) -> NativeApplicationResponse {
    serde_json::from_value(json!({"apiVersion":2,"requestId":"bitmap-read","instanceId":"instance","documentId":"document","contentRevision":7,"ok":true,"result":result(record)})).unwrap()
}
#[test]
fn immutable_query_keeps_full_bitmap_fields_and_native_storage_domains() {
    for tip in [
        "hard",
        "unknown persisted tip",
        " ",
        "data:authored-tip",
        "blob:tip",
    ] {
        let mut value = anchor();
        value["vectorExt"]["bitmapBrush"]["bitmapBrushSpec"]["tip"] = json!(tip);
        value["vectorExt"]["bitmapBrush"]["brushGroupId"] = json!("blob:data:group 雪");
        let original = value.clone();
        assert!(admits(&value));
        let request = request(&value);
        assert!(request.validate_input().is_ok());
        let response = response(&value);
        assert!(crate::wire::validate_native_response(&response, &request).is_ok());
        assert_eq!(response.result.unwrap()["object"], original);
        assert_eq!(value, original);
    }
    let mut value = anchor();
    value["target"]["frameScope"]["kind"] = json!("reference");
    value["vectorExt"]["isLinkedFillCompanion"] = json!(false);
    let fields = &mut value["vectorExt"]["bitmapBrush"];
    fields["bitmapBrushSpec"]["size"] = json!(-1);
    fields["bitmapBrushSpec"]["opacity"] = json!(3);
    fields["bitmapBrushSpec"]["spacing"] = json!(0);
    fields["preTextureOpacity"] = json!(-2);
    fields["bitmapBrushSpec"]["tip"] = json!("雪".repeat(85));
    fields["brushGroupId"] = json!("x".repeat(255));
    assert!(
        admits(&value),
        "storage data does not acquire baker positivity/authored-only rules"
    );
}
#[test]
fn camouflage_optional_presence_and_null_rules_match_stored_owner() {
    let mut value = anchor();
    value["vectorExt"]["bitmapBrush"]
        .as_object_mut()
        .unwrap()
        .remove("bitmapPressureProfile");
    assert!(admits(&value));
    value["vectorExt"]["bitmapBrush"]["preTextureStroke"] = json!("#abc");
    assert!(admits(&value));
    value["fill"] = Value::Null;
    value["vector"]["opacity"] = json!(0);
    value["vectorExt"]["bitmapBrush"]
        .as_object_mut()
        .unwrap()
        .remove("preTextureStroke");
    assert!(!admits(&value));
    value["vectorExt"]["bitmapBrush"]["preTextureOpacity"] = json!(1.5);
    assert!(admits(&value));
    value["vector"]["opacity"] = json!(0.1);
    assert!(!admits(&value));
    let mut value = anchor();
    value["vectorExt"]["bitmapBrush"]
        .as_object_mut()
        .unwrap()
        .remove("preTextureStroke");
    assert!(!admits(&value));
    let mut value = anchor();
    value["vector"]["stroke"] = json!({"kind":"solid","r":0,"g":0,"b":0,"a":1});
    value["vector"]["hasRealStroke"] = json!(true);
    assert!(!admits(&value));
    let mut value = anchor();
    value["vectorExt"]["bitmapBrush"]["preTextureOpacity"] = Value::Null;
    assert!(!admits(&value));
}
#[test]
fn malformed_closed_fields_and_mixed_link_relations_are_refused() {
    let cases = [
        ("/vectorExt/bitmapBrush", Value::Null),
        ("/vectorExt/bitmapBrush/bitmapBrushSpec", json!([])),
        ("/vectorExt/bitmapBrush/bitmapBrushSpec/tip", json!("")),
        (
            "/vectorExt/bitmapBrush/bitmapBrushSpec/tip",
            json!("雪".repeat(86)),
        ),
        (
            "/vectorExt/bitmapBrush/brushGroupId",
            json!("x".repeat(256)),
        ),
        (
            "/vectorExt/bitmapBrush/brushGroupId",
            json!("bad\u{0000}group"),
        ),
        (
            "/vectorExt/bitmapBrush/bitmapBrushSpec/color",
            json!("data:raw"),
        ),
        (
            "/vectorExt/bitmapBrush/bitmapBrushSpec/color",
            json!("#abcd"),
        ),
        ("/vectorExt/bitmapBrush/bitmapBrushSpec/pressure", json!(1)),
        ("/vectorExt/bitmapBrush/bitmapBrushSpec/seed", Value::Null),
        ("/vectorExt/bitmapBrush/preTextureStroke", json!("blob:raw")),
        ("/vectorExt/bitmapBrush/bitmapPressureProfile", Value::Null),
        (
            "/vectorExt/bitmapBrush/bitmapPressureProfile",
            json!([[1, 2]]),
        ),
        (
            "/vectorExt/bitmapBrush/bitmapPressureProfile",
            json!([[1, "2", 3]]),
        ),
        ("/target/contextId", json!("nested")),
        ("/vector/hasRealStroke", json!(true)),
    ];
    for (path, replacement) in cases {
        let mut value = anchor();
        *value.pointer_mut(path).unwrap() = replacement;
        assert!(!admits(&value), "{path}");
    }
    for location in [
        "/vectorExt/bitmapBrush",
        "/vectorExt/bitmapBrush/bitmapBrushSpec",
    ] {
        let mut value = anchor();
        value
            .pointer_mut(location)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("pixels".into(), json!("data:raw"));
        assert!(!admits(&value));
    }
    for key in ["bitmapBrushSpec", "brushGroupId"] {
        let mut value = anchor();
        value["vectorExt"]["bitmapBrush"]
            .as_object_mut()
            .unwrap()
            .remove(key);
        assert!(!admits(&value));
    }
    for key in [
        "tip", "size", "spacing", "scatter", "opacity", "color", "pressure", "seed",
    ] {
        let mut value = anchor();
        value["vectorExt"]["bitmapBrush"]["bitmapBrushSpec"]
            .as_object_mut()
            .unwrap()
            .remove(key);
        assert!(!admits(&value));
    }
    let mut value = anchor();
    value["vectorExt"]["linkedFillId"] = json!("pair");
    assert!(!admits(&value));
    let mut value = anchor();
    value["vectorExt"]["isLinkedFillCompanion"] = json!(true);
    assert!(!admits(&value));
    assert!(serde_json::from_str::<Value>("{\"size\":1e400}").is_err());
}
#[test]
fn query_correlation_bounds_and_write_guards_remain_closed() {
    let value = anchor();
    let query = request(&value);
    let good = response(&value);
    for path in [
        "/result/documentSnapshotId",
        "/result/object/target/strokeId",
    ] {
        let mut response = serde_json::to_value(&good).unwrap();
        *response.pointer_mut(path).unwrap() = json!("wrong");
        let response = serde_json::from_value(response).unwrap();
        assert!(
            crate::wire::validate_native_response(&response, &query).is_err(),
            "{path}"
        );
    }
    let mut wrong_revision = good.clone();
    wrong_revision.content_revision = 8;
    assert!(crate::wire::validate_native_response(&wrong_revision, &query).is_err());
    let mut value = anchor();
    value["vectorExt"]["bitmapBrush"]["bitmapPressureProfile"] = json!(vec![[0, 0, 1]; 4096]);
    assert!(admits(&value));
    assert!(
        crate::wire::validate_native_response(&response(&value), &request(&value)).is_err(),
        "full query reply stays4096"
    );
    value["vectorExt"]["bitmapBrush"]["bitmapPressureProfile"] = json!(vec![[0, 0, 1]; 4097]);
    assert!(!admits(&value));
    for command in ["object.vector.create", "object.vector.replace"] {
        let mut value = anchor();
        if command.ends_with("create") {
            value["target"]["strokeId"] = json!("");
        }
        let payload = json!({"command":command,"record":value});
        assert!(!crate::native_contract::validate_request(VECTOR, &payload));
    }
}

// Object-query envelope identity belongs to call_native, after shape validation.
#[tokio::test]
async fn bitmap_query_identity_is_rejected_by_the_actual_wire_entry() {
    use crate::registry::Endpoint;
    use crate::wire::{call_native, read_json, write_json};
    use tokio_util::sync::CancellationToken;
    for field in [
        None,
        Some("requestId"),
        Some("instanceId"),
        Some("documentId"),
    ] {
        let object = anchor();
        let query = request(&object);
        let mut value = serde_json::to_value(response(&object)).unwrap();
        if let Some(field) = field {
            value[field] = json!("foreign");
        }
        let reply: NativeApplicationResponse = serde_json::from_value(value).unwrap();
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let endpoint = Endpoint {
            instance_id: "instance".into(),
            port: listener.local_addr().unwrap().port(),
            secret: "bitmap-test".into(),
            build_id: "fixture".into(),
        };
        let peer = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let received: Value = read_json(&mut stream).await.unwrap();
            assert_eq!(received["secret"], "bitmap-test");
            assert_eq!(
                received["nativeRequest"]["operation"],
                "query.document.object"
            );
            write_json(&mut stream, &reply).await.unwrap();
        });
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            call_native(&endpoint, query, CancellationToken::new()),
        )
        .await
        .unwrap();
        assert_eq!(result.is_ok(), field.is_none());
        tokio::time::timeout(std::time::Duration::from_secs(5), peer)
            .await
            .unwrap()
            .unwrap();
    }
}
