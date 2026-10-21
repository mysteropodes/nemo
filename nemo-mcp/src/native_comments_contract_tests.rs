use super::*;
use crate::contract::{NativeApplicationRequest, NativeApplicationResponse};
use serde_json::json;
fn draft() -> Value {
    json!({"id":"","x":-0.25,"y":0.5,"frame":-1.5,"text":"note","createdAt":-2.25,"resolved":false})
}
fn edit(action: Value) -> NativeApplicationRequest {
    serde_json::from_value(json!({"apiVersion":2,"requestId":"comments-edit","instanceId":"i","documentId":"d",
        "expectedRevision":7,"operation":EDIT,"payload":{"command":"object.project.comments.edit","action":action}})).unwrap()
}
fn query(selector: Value) -> NativeApplicationRequest {
    serde_json::from_value(json!({"apiVersion":2,"requestId":"comments-read","instanceId":"i","documentId":"d",
        "operation":READ,"payload":{"atRevision":7,"documentSnapshotId":"s7","contextId":"scene-root","selector":selector}})).unwrap()
}
fn response(q: &NativeApplicationRequest, body: Value) -> NativeApplicationResponse {
    let result = if q.operation == READ {
        json!({"instanceId":"i","documentId":"d","atRevision":7,
        "documentSnapshotId":"s7","contextId":"scene-root","result":body})
    } else {
        body
    };
    serde_json::from_value(
        json!({"apiVersion":2,"requestId":q.request_id,"instanceId":"i","documentId":"d",
        "contentRevision":7,"ok":true,"result":result}),
    )
    .unwrap()
}
#[test]
fn original_draft_and_existing_save_keep_closed_raw_field_rules() {
    let q = edit(json!({"kind":"append","comment":draft()}));
    assert!(q.validate().is_ok());
    for key in ["id", "x", "y", "frame", "text", "createdAt", "resolved"] {
        let mut value = draft();
        value.as_object_mut().unwrap().remove(key);
        assert!(edit(json!({"kind":"append","comment":value}))
            .validate()
            .is_err());
    }
    for text in ["", " \n", "\u{feff}\u{00a0}"] {
        let mut value = draft();
        value["text"] = json!(text);
        assert!(edit(json!({"kind":"append","comment":value}))
            .validate()
            .is_err());
        assert!(
            edit(json!({"kind":"update","ordinal":2,"text":text,"resolved":true}))
                .validate()
                .is_ok()
        );
    }
    for key in FIELDS {
        let mut value = draft();
        value[key] = Value::Null;
        assert!(edit(json!({"kind":"append","comment":value}))
            .validate()
            .is_err());
    }
    let mut value = draft();
    value["authorId"] = json!("\0日本語");
    value["authorName"] = json!("");
    value["authorColor"] = json!("raw");
    value["text"] = json!("\u{0085}");
    assert!(edit(json!({"kind":"append","comment":value}))
        .validate()
        .is_ok());
    for action in [
        json!({"kind":"delete","id":"same"}),
        json!({"kind":"delete","ordinal":0}),
        json!({"kind":"resolve","ordinal":1}),
        json!({"kind":"append","comment":{"screenshotDataUrls":[]}}),
    ] {
        assert!(edit(action).validate().is_err());
    }
}
#[test]
fn pinned_pages_and_records_correlate_raw_ordinal_identity_and_complete_budget() {
    let q = query(json!({"kind":"commentsPage","cursor":0,"limit":16}));
    assert!(q.validate().is_ok());
    let page = json!({"kind":"projectCommentsPage","commentsPresent":true,"commentCount":3,"cursor":0,
        "items":[{"ordinal":1,"comment":{}},{"ordinal":2,"comment":{"id":"same","text":""}}],"nextCursor":2});
    assert!(crate::wire::validate_native_response(&response(&q, page.clone()), &q).is_ok());
    for (key, value) in [
        ("nextCursor", json!(1)),
        ("cursor", json!(1)),
        ("commentsPresent", json!(false)),
    ] {
        let mut page = page.clone();
        page[key] = value;
        assert!(crate::wire::validate_native_response(&response(&q, page), &q).is_err());
    }
    let q = query(json!({"kind":"record","ordinal":2}));
    for raw in [
        json!({}),
        json!({"id":"same","authorName":"","resolved":false}),
    ] {
        assert!(crate::wire::validate_native_response(
            &response(
                &q,
                json!({"kind":"projectCommentRecord","ordinal":2,"comment":raw})
            ),
            &q
        )
        .is_ok());
    }
    assert!(crate::wire::validate_native_response(
        &response(
            &q,
            json!({"kind":"projectCommentRecord","ordinal":1,"comment":{}})
        ),
        &q
    )
    .is_err());
    let huge = response(
        &q,
        json!({"kind":"projectCommentRecord","ordinal":2,"comment":{"text":"x".repeat(4096)}}),
    );
    assert!(crate::wire::validate_native_response(&huge, &q).is_err());
    let mut no_revision = edit(json!({"kind":"delete","ordinal":1}));
    no_revision.expected_revision = None;
    assert!(no_revision.validate().is_err());
    let mut read_revision = q.clone();
    read_revision.expected_revision = Some(7);
    assert!(read_revision.validate().is_err());
    let mut request =
        edit(json!({"kind":"update","ordinal":1,"text":"x".repeat(4096),"resolved":false}));
    assert!(request.validate().is_err());
    request.payload["action"]["text"] = json!("");
    assert!(request.validate().is_ok());
}
