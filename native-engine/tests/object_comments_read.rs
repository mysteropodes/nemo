//! Pinned raw comment pages, refusal budgets and the same sealed owner fences.
use super::*;
fn raw_query(p: &ObjectSnapshot, selector: Value) -> Value {
    json!({"command":"object.project.comments.read","instanceId":p.instance_id(),
        "documentId":p.document_id(),"atRevision":p.content_revision(),
        "documentSnapshotId":p.snapshot_id(),"contextId":"scene-root","selector":selector})
}
#[test]
fn full_record_pages_preserve_partial_duplicates_order_and_make_bounded_progress() {
    let comments: Vec<Value> = (0..35)
        .map(|i| match i {
            0 => json!({}),
            1 => json!({"id":"","text":"","resolved":false}),
            _ => json!({"id":"same","text":format!("{i}-{}","é".repeat(450))}),
        })
        .collect();
    let raw = json!(comments).to_string();
    let o = owner(Some(&raw), json!({}));
    let before = saved(&o);
    let p = pin(&o);
    let mut cursor = 0;
    let mut count = 0;
    loop {
        let response = query(
            &p,
            json!({"kind":"commentsPage","cursor":cursor,"limit":16}),
        );
        assert_eq!(response["ok"], true);
        let page = &response["result"]["result"];
        assert_eq!(page["commentsPresent"], true);
        assert_eq!(page["commentCount"], 35);
        let items = page["items"].as_array().unwrap();
        assert!(!items.is_empty() && items.len() <= 16);
        for item in items {
            assert_eq!(item["ordinal"].as_u64(), Some(count as u64 + 1));
            assert_eq!(item["comment"], comments[count]);
            count += 1;
        }
        if page["nextCursor"].is_null() {
            break;
        }
        let next = page["nextCursor"].as_u64().unwrap();
        assert!(next > cursor);
        cursor = next;
    }
    assert_eq!(count, 35);
    assert_eq!(
        query(&p, json!({"kind":"commentsPage","cursor":35,"limit":16}))["result"]["result"]
            ["items"],
        json!([])
    );
    assert_eq!(saved(&o), before);
    for selector in [
        json!({"kind":"commentsPage","cursor":36,"limit":1}),
        json!({"kind":"commentsPage","cursor":0,"limit":0}),
        json!({"kind":"commentsPage","cursor":0,"limit":17}),
        json!({"kind":"commentsPage","cursor":9_007_199_254_740_992_u64,"limit":1}),
        json!({"kind":"record","ordinal":0}),
        json!({"kind":"record","ordinal":9_007_199_254_740_992_u64}),
        json!({"kind":"settings","extra":true}),
    ] {
        assert_eq!(query(&p, selector)["error"]["code"], "invalid_request");
    }
    assert_eq!(
        query(&p, json!({"kind":"record","ordinal":36}))["error"]["code"],
        "not_found"
    );
}
#[test]
fn oversized_raw_records_and_complete_envelope_overhead_refuse_without_mutation() {
    for field in ["id", "text", "authorName"] {
        let mut record = json!({});
        record[field] = json!("\u{0001}".repeat(1024));
        let raw = json!([record]).to_string();
        let o = owner(Some(&raw), json!({}));
        let before = saved(&o);
        let p = pin(&o);
        for selector in [
            json!({"kind":"record","ordinal":1}),
            json!({"kind":"commentsPage","cursor":0,"limit":16}),
        ] {
            assert_eq!(query(&p, selector)["error"]["code"], "unavailable");
        }
        assert_eq!(query(&p, json!({"kind":"settings"}))["ok"], true);
        assert_eq!(saved(&o), before);
    }
    let o = owner(Some(r#"[{"text":""}]"#), json!({}));
    let p = pin(&o);
    let base = p
        .query_comments_metadata_json(
            &raw_query(&p, json!({"kind":"record","ordinal":1}))
                .to_string()
                .into_bytes(),
        )
        .unwrap();
    let padding = 4096 - serde_json::to_vec(&base).unwrap().len() - 32;
    let raw = json!([{"text":"x".repeat(padding)}]).to_string();
    let o = owner(Some(&raw), json!({}));
    let p = pin(&o);
    let inner = p
        .query_comments_metadata_json(
            &raw_query(&p, json!({"kind":"record","ordinal":1}))
                .to_string()
                .into_bytes(),
        )
        .unwrap();
    assert!(serde_json::to_vec(&inner).unwrap().len() > 4096 - 64);
    assert!(serde_json::to_vec(&inner).unwrap().len() <= 4096);
    let mut q = request(&p, "envelope", json!({"kind":"record","ordinal":1}));
    q.request_id = "r".repeat(128);
    assert_eq!(read(&p, q)["error"]["code"], "unavailable");
}
#[test]
fn null_feedback_fields_bad_ordinals_duplicate_fields_and_byte_limits_are_atomic() {
    let mut o = owner(Some("[{}]"), json!({}));
    let before = saved(&o);
    for (i, field) in [
        "id",
        "x",
        "y",
        "frame",
        "authorId",
        "authorName",
        "authorColor",
        "text",
        "createdAt",
        "resolved",
    ]
    .into_iter()
    .enumerate()
    {
        let mut comment = draft("x");
        comment[field] = Value::Null;
        assert_eq!(
            edit(
                &mut o,
                &format!("null-{i}"),
                json!({"kind":"append","comment":comment})
            )["error"]["code"],
            "invalid_request"
        );
    }
    for (i, action) in [
        json!({"kind":"append","comment":{"screenshotDataUrls":[]}}),
        json!({"kind":"append","comment":null}),
        json!({"kind":"resolve","ordinal":1}),
        json!({"kind":"update","ordinal":1,"text":"x","resolved":true,"authorId":"rewrite"}),
        json!({"kind":"update","ordinal":0,"text":"x","resolved":true}),
        json!({"kind":"delete","ordinal":0}),
        json!({"kind":"delete","ordinal":9_007_199_254_740_992_u64}),
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(
            edit(&mut o, &format!("shape-{i}"), action)["error"]["code"],
            "invalid_request"
        );
    }
    for (i, action) in [
        json!({"kind":"delete","ordinal":2}),
        json!({"kind":"update","ordinal":2,"text":"","resolved":false}),
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(
            edit(&mut o, &format!("missing-{i}"), action)["error"]["code"],
            "not_found"
        );
    }
    for raw in [
        r#"{"command":"object.project.comments.edit","action":{"kind":"update","ordinal":1,"text":"a","text":"b","resolved":false}}"#,
        r#"{"command":"object.project.comments.edit","action":{"kind":"append","comment":{"x":1e309}}}"#,
        r#"{"command":"object.project.comments.edit","action":[]}"#,
        r#"[]"#,
    ] {
        assert_eq!(
            o.prepare_comments_delta_json(raw.as_bytes()).unwrap_err(),
            Error::InvalidRequest
        );
    }
    assert_eq!(
        o.prepare_comments_delta_json(&vec![b' '; 4097])
            .unwrap_err(),
        Error::InvalidRequest
    );
    assert_eq!(
        edit(
            &mut o,
            "oversize",
            json!({"kind":"update","ordinal":1,"text":"x".repeat(4096),"resolved":false})
        )["error"]["code"],
        "invalid_request"
    );
    for raw in [
        "null",
        "{}",
        r#"[{"text":null}]"#,
        r#"[{"x":1e309}]"#,
        r#"[{"text":"a","text":"b"}]"#,
        r#"[{"screenshotDataUrls":[]}]"#,
    ] {
        assert!(NativeObjectHistory::from_project_file(
            "bad-comments",
            &wire(Some(raw), json!({}))
        )
        .is_err());
    }
    assert_eq!(saved(&o), before);
    assert_eq!(o.history_depths(), (0, 0));
}
#[test]
fn sealed_and_public_identity_revision_and_cancellation_fences_preserve_the_candidate() {
    let mut o = owner(None, json!({}));
    let before = saved(&o);
    let action = json!({"kind":"append","comment":draft("x")});
    let candidate = o
        .prepare_comments_delta_json(&payload(action.clone()))
        .unwrap();
    let mut foreign = NativeObjectHistory::from_project_file("foreign-comments", &before).unwrap();
    assert_eq!(
        foreign.commit_project_settings(candidate.clone()),
        Err(Error::WrongInstance)
    );
    let mut other = owner(None, json!({}));
    assert_eq!(
        other.commit_project_settings(candidate.clone()),
        Err(Error::WrongDocument)
    );
    assert_eq!(saved(&foreign), before);
    assert_eq!(saved(&other), before);
    for (field, value, code) in [
        ("instanceId", json!("foreign"), "wrong_instance"),
        ("documentId", json!("foreign"), "wrong_document"),
        ("expectedRevision", json!(1), "stale_revision"),
    ] {
        let mut q = serde_json::to_value(command(&o, field, action.clone())).unwrap();
        q[field] = value;
        assert_eq!(
            dispatch(&mut o, serde_json::from_value(q).unwrap())["error"]["code"],
            code
        );
    }
    let mut q = command(&o, "cancelled", action.clone());
    q.cancelled_before_dispatch = true;
    assert_eq!(
        dispatch(&mut o, q)["error"]["code"],
        "cancelled_before_dispatch"
    );
    let old = pin(&o);
    let mut q = request(&old, "read-revision", json!({"kind":"settings"}));
    q.expected_revision = Some(0);
    assert_eq!(read(&old, q)["error"]["code"], "invalid_request");
    let mut q = request(&old, "cancelled-read", json!({"kind":"settings"}));
    q.cancelled_before_dispatch = true;
    assert_eq!(read(&old, q)["error"]["code"], "cancelled_before_dispatch");
    let duplicate = raw_query(&old, json!({"kind":"settings"}))
        .to_string()
        .replace(
            "\"kind\":\"settings\"",
            "\"kind\":\"settings\",\"kind\":\"settings\"",
        );
    assert_eq!(
        old.query_comments_metadata_json(duplicate.as_bytes()),
        Err(Error::InvalidRequest)
    );
    let oversized = vec![b' '; 4097];
    for bytes in [b"[]".as_slice(), b"{".as_slice(), oversized.as_slice()] {
        assert_eq!(
            old.query_comments_metadata_json(bytes),
            Err(Error::InvalidRequest)
        );
    }
    edit(&mut o, "advance", action);
    let changed = saved(&o);
    assert_eq!(
        o.commit_project_settings(candidate),
        Err(Error::StaleRevision)
    );
    assert_eq!(saved(&o), changed);
    assert_eq!(
        read(&pin(&o), request(&old, "old", json!({"kind":"settings"})))["error"]["code"],
        "stale_revision"
    );
    for (field, value, code) in [
        ("instanceId", json!("wrong"), Error::WrongInstance),
        ("documentId", json!("wrong"), Error::WrongDocument),
        ("atRevision", json!(1), Error::StaleRevision),
        ("documentSnapshotId", json!("wrong"), Error::StaleRevision),
        ("contextId", json!("nested"), Error::Unavailable),
        (
            "command",
            json!("object.project.workspace.read"),
            Error::InvalidRequest,
        ),
    ] {
        let mut q = raw_query(&old, json!({"kind":"settings"}));
        q[field] = value;
        assert_eq!(
            old.query_comments_metadata_json(&q.to_string().into_bytes()),
            Err(code)
        );
    }
}
#[test]
fn bounded_comment_append_cannot_publish_an_unsaveable_candidate() {
    const LIMIT: usize = 32 * 1024 * 1024;
    const TARGET: usize = LIMIT - 64;
    let colors = vec!["x".repeat(1024); (LIMIT - 4096) / 1027];
    let source = owner(
        None,
        json!({"palettes":[{"id":"filler","name":"Filler","colors":colors}]}),
    );
    let mut wire: Value = serde_json::from_slice(&saved(&source)).unwrap();
    let mut remaining = TARGET
        .checked_sub(serde_json::to_vec(&wire).unwrap().len())
        .unwrap();
    let colors = wire["project"]["palettes"][0]["colors"]
        .as_array_mut()
        .unwrap();
    while remaining > 0 {
        if remaining < 3 {
            let last = colors.last_mut().unwrap();
            let value = last.as_str().unwrap();
            *last = json!(&value[..value.len() - (3 - remaining)]);
            remaining = 3;
        }
        let bytes = (remaining - 3).min(1024);
        colors.push(json!("x".repeat(bytes)));
        remaining -= bytes + 3;
    }
    let encoded = serde_json::to_vec(&wire).unwrap();
    assert_eq!(encoded.len(), TARGET);
    let o = NativeObjectHistory::from_project_file("comment-budget", &encoded).unwrap();
    let before = saved(&o);
    assert_eq!(before.len(), TARGET);
    let bytes = payload(json!({"kind":"append","comment":draft(&"x".repeat(1024))}));
    assert!(bytes.len() < 4096);
    assert_eq!(
        o.prepare_comments_delta_json(&bytes).unwrap_err(),
        Error::Unavailable
    );
    assert_eq!(saved(&o), before);
    assert_eq!(o.content_revision(), 0);
    assert_eq!(o.history_depths(), (0, 0));
}
