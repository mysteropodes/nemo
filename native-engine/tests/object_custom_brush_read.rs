//! Pinned custom catalog order, complete bounds, malformed records and owner fences.
use super::*;

fn catalog(entries: &[(String, Value)]) -> String {
    let rows: Vec<String> = entries
        .iter()
        .map(|(id, preset)| format!("{}:{}", serde_json::to_string(id).unwrap(), preset))
        .collect();
    format!("{{{}}}", rows.join(","))
}
fn raw_query(p: &ObjectSnapshot, selector: Value) -> Value {
    json!({"command":"object.project.customBrushPreset.read","instanceId":p.instance_id(),
        "documentId":p.document_id(),"atRevision":p.content_revision(),
        "documentSnapshotId":p.snapshot_id(),"contextId":"scene-root","selector":selector})
}
#[test]
fn pages_preserve_order_raw_ids_and_label_presence_with_complete_forward_progress() {
    let entries: Vec<(String, Value)> = (0..35)
        .map(|index| {
            let id = match index {
                0 => "10".into(),
                1 => "2".into(),
                2 => "".into(),
                _ => format!("z-{} 日本語", 35 - index),
            };
            let preset = match index {
                0 => json!({}),
                1 => json!({"label":""}),
                _ => json!({"label":format!("{index}-{}", "é".repeat(450))}),
            };
            (id, preset)
        })
        .collect();
    let raw = catalog(&entries);
    let o = owner(Some(&raw), json!({}));
    let before = saved(&o);
    let p = pin(&o);
    let mut cursor = 0;
    let mut count = 0;
    loop {
        let response = query(&p, json!({"kind":"catalogPage","cursor":cursor,"limit":16}));
        assert_eq!(response["ok"], true);
        let page = &response["result"]["result"];
        assert_eq!(page["customBrushPresetsPresent"], true);
        assert_eq!(page["presetCount"], 35);
        let items = page["items"].as_array().unwrap();
        assert!(!items.is_empty() && items.len() <= 16);
        for item in items {
            assert_eq!(item["ordinal"].as_u64(), Some(count as u64 + 1));
            assert_eq!(item["id"], entries[count].0);
            assert_eq!(item.get("label"), entries[count].1.get("label"));
            count += 1;
        }
        if page["nextCursor"].is_null() {
            break;
        }
        let next = page["nextCursor"].as_u64().unwrap();
        assert!(next > cursor);
        cursor = next;
    }
    assert_eq!(count, entries.len());
    assert_eq!(
        query(&p, json!({"kind":"catalogPage","cursor":35,"limit":1}))["result"]["result"]["items"],
        json!([])
    );
    assert_eq!(saved(&o), before);
    for selector in [
        json!({"kind":"catalogPage","cursor":0,"limit":0}),
        json!({"kind":"catalogPage","cursor":0,"limit":17}),
        json!({"kind":"catalogPage","cursor":36,"limit":1}),
        json!({"kind":"catalogPage","cursor":9_007_199_254_740_992_u64,"limit":1}),
        json!({"kind":"settings","extra":true}),
    ] {
        assert_eq!(query(&p, selector)["error"]["code"], "invalid_request");
    }
}
#[test]
fn individual_id_label_record_and_complete_envelope_limits_refuse_without_truncation() {
    for entries in [
        vec![("\u{0001}".repeat(1024), json!({}))],
        vec![("p".into(), json!({"label":"\u{0001}".repeat(1024)}))],
    ] {
        let raw = catalog(&entries);
        let o = owner(Some(&raw), json!({}));
        let before = saved(&o);
        let p = pin(&o);
        assert_eq!(
            query(&p, json!({"kind":"catalogPage","cursor":0,"limit":16}))["error"]["code"],
            "unavailable"
        );
        if entries[0].0 == "p" {
            assert_eq!(
                query(&p, json!({"kind":"record","id":"p"}))["error"]["code"],
                "unavailable"
            );
        }
        assert_eq!(saved(&o), before);
    }
    let raw = catalog(&[(
        "p".into(),
        json!({"customStamp":{"segments":vec![json!({"point":[0.0,0.0]});1000]}}),
    )]);
    let o = owner(Some(&raw), json!({}));
    assert_eq!(
        query(&pin(&o), json!({"kind":"record","id":"p"}))["error"]["code"],
        "unavailable"
    );
    assert_eq!(
        query(
            &pin(&o),
            json!({"kind":"catalogPage","cursor":0,"limit":16})
        )["ok"],
        true
    );
    let o = owner(Some(r#"{"p":{"label":""}}"#), json!({}));
    let p = pin(&o);
    let base = p
        .query_custom_brush_metadata_json(
            &raw_query(&p, json!({"kind":"record","id":"p"}))
                .to_string()
                .into_bytes(),
        )
        .unwrap();
    let padding = 4096 - serde_json::to_vec(&base).unwrap().len() - 32;
    let raw = catalog(&[("p".into(), json!({"label":"x".repeat(padding)}))]);
    let o = owner(Some(&raw), json!({}));
    let p = pin(&o);
    let inner = p
        .query_custom_brush_metadata_json(
            &raw_query(&p, json!({"kind":"record","id":"p"}))
                .to_string()
                .into_bytes(),
        )
        .unwrap();
    assert!(serde_json::to_vec(&inner).unwrap().len() <= 4096);
    assert!(serde_json::to_vec(&inner).unwrap().len() > 4096 - 64);
    let mut q = request(&p, "bounded", json!({"kind":"record","id":"p"}));
    q.request_id = "r".repeat(128);
    assert_eq!(read(&p, q)["error"]["code"], "unavailable");
}
#[test]
fn all_optional_nulls_bad_shapes_duplicates_and_oversized_requests_are_atomic() {
    let mut o = owner(None, json!({}));
    let before = saved(&o);
    for (index, field) in all_fields().as_object().unwrap().keys().enumerate() {
        let mut fields = serde_json::Map::new();
        fields.insert(field.clone(), Value::Null);
        let preset = Value::Object(fields);
        assert_eq!(
            edit(
                &mut o,
                &format!("null-{index}"),
                json!({"kind":"upsert","id":"p","preset":preset})
            )["error"]["code"],
            "invalid_request"
        );
    }
    for (index, preset) in [
        Value::Null,
        json!([]),
        json!({"favorites":[]}),
        json!({"customStamp":{}}),
        json!({"customStamp":{"segments":[{"point":[0]}]}}),
        json!({"customStamp":{"segments":[{"point":[0,0],"handleIn":null}]}}),
        json!({"customStamp":{"segments":[],"closed":null}}),
        json!({"customStamp":{"segments":[],"pointCount":null}}),
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(
            edit(
                &mut o,
                &format!("shape-{index}"),
                json!({"kind":"upsert","id":"p","preset":preset})
            )["error"]["code"],
            "invalid_request"
        );
    }
    for raw in [
        r#"{"command":"object.project.customBrushPreset.edit","action":{"kind":"upsert","id":"p","preset":{"label":"a","label":"b"}}}"#,
        r#"{"command":"object.project.customBrushPreset.edit","action":{"kind":"upsert","id":"p","id":"q","preset":{}}}"#,
        r#"{"command":"object.project.customBrushPreset.edit","action":{"kind":"upsert","id":"p","preset":{"nibSize":1e309}}}"#,
        r#"{"command":"object.project.customBrushPreset.edit","action":{"kind":"select","id":"p"}}"#,
        r#"{"command":"object.project.customBrushPreset.edit","action":[]}"#,
        r#"[]"#,
    ] {
        assert_eq!(
            o.prepare_custom_brush_delta_json(raw.as_bytes())
                .unwrap_err(),
            Error::InvalidRequest
        );
    }
    assert!(NativeObjectHistory::from_project_file(
        "duplicates",
        &wire(Some(r#"{"p":{},"p":{}}"#), json!({}))
    )
    .is_err());
    assert_eq!(
        edit(
            &mut o,
            "oversize",
            json!({"kind":"upsert","id":"p","preset":{"label":"\u{0001}".repeat(1024)}})
        )["error"]["code"],
        "invalid_request"
    );
    assert_eq!(
        o.prepare_custom_brush_delta_json(&vec![b' '; 4097])
            .unwrap_err(),
        Error::InvalidRequest
    );
    assert_eq!(saved(&o), before);
    assert_eq!(o.history_depths(), (0, 0));
}
#[test]
fn sealed_and_public_identity_revision_and_cancellation_fences_preserve_the_candidate() {
    let mut o = owner(None, json!({}));
    let before = saved(&o);
    let action = json!({"kind":"upsert","id":"p","preset":{}});
    let candidate = o
        .prepare_custom_brush_delta_json(&payload(action.clone()))
        .unwrap();
    let mut foreign = NativeObjectHistory::from_project_file("foreign-brush", &before).unwrap();
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
        old.query_custom_brush_metadata_json(duplicate.as_bytes()),
        Err(Error::InvalidRequest)
    );
    let oversized = vec![b' '; 4097];
    for bytes in [b"[]".as_slice(), b"{".as_slice(), oversized.as_slice()] {
        assert_eq!(
            old.query_custom_brush_metadata_json(bytes),
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
            old.query_custom_brush_metadata_json(&q.to_string().into_bytes()),
            Err(code)
        );
    }
}
#[test]
fn bounded_upsert_cannot_publish_an_unsaveable_candidate() {
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
    let o = NativeObjectHistory::from_project_file("preset-budget", &encoded).unwrap();
    let before = saved(&o);
    assert_eq!(before.len(), TARGET);
    let bytes = payload(json!({"kind":"upsert","id":"p","preset":all_fields()}));
    assert!(bytes.len() < 4096);
    assert_eq!(
        o.prepare_custom_brush_delta_json(&bytes).unwrap_err(),
        Error::Unavailable
    );
    assert_eq!(saved(&o), before);
    assert_eq!(o.content_revision(), 0);
    assert_eq!(o.history_depths(), (0, 0));
}
