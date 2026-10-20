//! Original Shadow add/select through the real public envelope and sealed history.
//! All execution is unrun until the lead applies and validates this source candidate.
use crate::history::NativeObjectHistory;
use crate::request_receipts::{DispatchErrorCode as Error, OpacityRequest};
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};

const EDIT: &str = "command.document.object.project.shadow.edit";
const READ: &str = "query.document.object.project.shadow.read";
fn owner(extra: Value) -> NativeObjectHistory {
    let mut project = json!({"canvasW":64,"canvasH":48,"fps":24});
    project
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    let wire = json!({
        "format":"nemo.native-project-document","formatVersion":1,
        "project":project,"viewportReference":null,"assets":[],
        "drawing":{"format":"nemo.native-object-document","formatVersion":1,
            "totalFrames":40,"layers":[{"layerUid":"L"}],"objects":[]}
    });
    NativeObjectHistory::from_project_file("public-shadow", wire.to_string().as_bytes()).unwrap()
}
fn pin(o: &NativeObjectHistory) -> ObjectSnapshot {
    o.acquire_snapshot(o.content_revision()).unwrap()
}
fn saved(o: &NativeObjectHistory) -> Vec<u8> {
    let p = pin(o);
    o.serialize_project_file(p.content_revision(), p.snapshot_id())
        .unwrap()
}
fn project(o: &NativeObjectHistory) -> Value {
    serde_json::from_slice::<Value>(&saved(o)).unwrap()["project"].clone()
}
fn payload(action: Value) -> Vec<u8> {
    serde_json::to_vec(&json!({"command":"object.project.shadow.edit","action":action})).unwrap()
}
fn command(o: &NativeObjectHistory, id: &str, action: Value) -> OpacityRequest {
    serde_json::from_value(json!({
        "apiVersion":2,"requestId":id,"instanceId":o.instance_id(),
        "documentId":o.document_id(),"expectedRevision":o.content_revision(),
        "operation":EDIT,"payload":{"command":"object.project.shadow.edit","action":action}
    }))
    .unwrap()
}
fn dispatch(o: &mut NativeObjectHistory, q: OpacityRequest) -> Value {
    let response = o
        .dispatch_object_command(&q)
        .unwrap()
        .expect("registered Shadow edit");
    assert!(serde_json::to_vec(&response).unwrap().len() <= 4096);
    serde_json::to_value(response).unwrap()
}
fn edit(o: &mut NativeObjectHistory, id: &str, action: Value) -> Value {
    let q = command(o, id, action);
    dispatch(o, q)
}
fn request(p: &ObjectSnapshot, id: &str, selector: Value) -> OpacityRequest {
    serde_json::from_value(json!({
        "apiVersion":2,"requestId":id,"instanceId":p.instance_id(),
        "documentId":p.document_id(),"operation":READ,
        "payload":{"atRevision":p.content_revision(),"documentSnapshotId":p.snapshot_id(),
            "contextId":"scene-root","selector":selector}
    }))
    .unwrap()
}
fn read(p: &ObjectSnapshot, q: OpacityRequest) -> Value {
    let response = p.dispatch(q).unwrap();
    assert!(serde_json::to_vec(&response).unwrap().len() <= 4096);
    serde_json::to_value(response).unwrap()
}
fn query(p: &ObjectSnapshot, selector: Value) -> Value {
    read(p, request(p, "shadow-read", selector))
}
fn defaults() -> Value {
    json!([
        {"id":"sh1","color":"#ff3355"},{"id":"sh2","color":"#ff8800"},
        {"id":"sh3","color":"#ffdd00"},{"id":"sh4","color":"#22cc55"},
        {"id":"sh5","color":"#2288ff"},{"id":"sh6","color":"#aa33ff"}
    ])
}
#[test]
fn absence_empty_and_supplied_active_id_are_read_only_until_original_explicit_gestures() {
    for supplied in [
        json!({}),
        json!({"shadowPalette":[]}),
        json!({"shadowPalette":[],"shadowActiveId":""}),
        json!({"shadowActiveId":"orphan"}),
    ] {
        let mut o = owner(supplied.clone());
        let before = saved(&o);
        let p = pin(&o);
        let settings = query(&p, json!({"kind":"settings"}));
        let body = &settings["result"]["result"];
        assert_eq!(
            body["shadowPalettePresent"],
            supplied.get("shadowPalette").is_some()
        );
        assert_eq!(body["shadowColorCount"], 0);
        assert_eq!(body.get("shadowActiveId"), supplied.get("shadowActiveId"));
        assert_eq!(
            query(&p, json!({"kind":"palettePage","cursor":0,"limit":16}))["result"]["result"]
                ["items"],
            json!([])
        );
        assert_eq!(saved(&o), before);
        assert_eq!(o.history_depths(), (0, 0));
        assert_eq!(
            edit(
                &mut o,
                "default-select",
                json!({"kind":"select","ordinal":6})
            )["result"],
            json!({"applied":true,"historyEntriesAdded":1})
        );
        assert_eq!(project(&o)["shadowPalette"], defaults());
        assert_eq!(project(&o)["shadowActiveId"], "sh6");
        o.undo(1).unwrap();
        assert_eq!(saved(&o), before);
        let added = edit(
            &mut o,
            "default-add",
            json!({"kind":"add","id":"sh7","color":"#AbCdEf"}),
        );
        assert_eq!(
            added["result"],
            json!({"applied":true,"historyEntriesAdded":1})
        );
        let mut expected = defaults();
        expected
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"sh7","color":"#AbCdEf"}));
        assert_eq!(project(&o)["shadowPalette"], expected);
        assert_eq!(project(&o)["shadowActiveId"], "sh7");
        assert_eq!(o.history_depths(), (1, 0));
    }
}
#[test]
fn add_select_duplicate_identity_replay_old_pin_history_and_fresh_reopen_are_exact() {
    let palette = json!([
        {"id":"dup","color":"#AbC"},{"id":"dup","color":"#abc"},
        {"id":"","color":""}
    ]);
    let mut o = owner(json!({"shadowPalette":palette,"shadowActiveId":"orphan",
        "workspace":{"panelOrder":null},"guides":{"h":[-0.25]},
        "comments":[{"text":"Keep","authorName":"Writer"}],"bpm":137.5}));
    let old = pin(&o);
    let before = saved(&o);
    let q = command(
        &o,
        "shadow-add",
        json!({"kind":"add","id":"dup","color":"#AbC"}),
    );
    let response = dispatch(&mut o, q.clone());
    assert_eq!(
        response["result"],
        json!({"applied":true,"historyEntriesAdded":1})
    );
    assert_eq!(dispatch(&mut o, q), response);
    assert_eq!(o.content_revision(), 1);
    assert_eq!(o.history_depths(), (1, 0));
    let changed = saved(&o);
    let mut after: Value = serde_json::from_slice(&changed).unwrap();
    let initial: Value = serde_json::from_slice(&before).unwrap();
    let mut expected = palette.clone();
    expected
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"dup","color":"#AbC"}));
    assert_eq!(after["project"]["shadowPalette"], expected);
    assert_eq!(after["project"]["shadowActiveId"], "dup");
    after["project"]["shadowPalette"] = initial["project"]["shadowPalette"].clone();
    after["project"]["shadowActiveId"] = initial["project"]["shadowActiveId"].clone();
    assert_eq!(after, initial);
    assert_eq!(
        query(&old, json!({"kind":"settings"}))["result"]["result"]["shadowActiveId"],
        "orphan"
    );
    assert_eq!(
        query(&old, json!({"kind":"palettePage","cursor":0,"limit":16}))["result"]["result"]
            ["items"][1],
        json!({"ordinal":2,"id":"dup","color":"#abc"})
    );
    o.undo(1).unwrap();
    assert_eq!(saved(&o), before);
    o.redo(2).unwrap();
    assert_eq!(saved(&o), changed);
    let fresh = NativeObjectHistory::from_project_file("shadow-reopen", &changed).unwrap();
    assert_eq!(saved(&fresh), changed);
    assert_eq!(
        edit(
            &mut o,
            "select-empty-id",
            json!({"kind":"select","ordinal":3})
        )["result"]["applied"],
        true
    );
    assert_eq!(project(&o)["shadowActiveId"], "");
    assert_eq!(project(&o)["shadowPalette"], expected);
}
#[test]
fn ordinal_select_noop_preserves_redo_and_sealed_candidates_reject_other_owners_and_staleness() {
    let mut o = owner(json!({"shadowPalette":[{"id":"same","color":"A"},
        {"id":"same","color":"B"}],"shadowActiveId":"same"}));
    edit(&mut o, "add", json!({"kind":"add","id":"new","color":"C"}));
    o.undo(1).unwrap();
    let before = saved(&o);
    assert_eq!(
        edit(&mut o, "same-id", json!({"kind":"select","ordinal":2}))["result"],
        json!({"applied":false,"historyEntriesAdded":0})
    );
    assert_eq!(o.content_revision(), 2);
    assert_eq!(o.history_depths(), (0, 1));
    assert_eq!(saved(&o), before);
    let bytes = payload(json!({"kind":"add","id":"later","color":"D"}));
    let candidate = o.prepare_shadow_delta_json(&bytes).unwrap();
    let mut foreign = NativeObjectHistory::from_project_file("foreign-shadow", &before).unwrap();
    assert_eq!(
        foreign.commit_project_settings(candidate.clone()),
        Err(Error::WrongInstance)
    );
    let mut other_doc = NativeObjectHistory::from_project_file("public-shadow", &before).unwrap();
    assert_eq!(
        other_doc.commit_project_settings(candidate.clone()),
        Err(Error::WrongDocument)
    );
    assert_eq!(saved(&foreign), before);
    assert_eq!(saved(&other_doc), before);
    assert_eq!(foreign.history_depths(), (0, 0));
    o.redo(2).unwrap();
    let after_redo = saved(&o);
    assert_eq!(
        o.commit_project_settings(candidate),
        Err(Error::StaleRevision)
    );
    assert_eq!(saved(&o), after_redo);
    assert_eq!(o.history_depths(), (1, 0));
}
#[test]
fn complete_pages_preserve_order_duplicates_spelling_and_make_bounded_forward_progress() {
    let palette: Vec<Value> = (0..35)
        .map(|i| {
            json!({
                "id":if i % 2 == 0 { "same" } else { "" },
                "color":format!("{i}-{}", "é".repeat(450))
            })
        })
        .collect();
    let o = owner(json!({"shadowPalette":palette}));
    let before = saved(&o);
    let p = pin(&o);
    let mut cursor = 0;
    let mut collected = Vec::new();
    loop {
        let response = query(&p, json!({"kind":"palettePage","cursor":cursor,"limit":16}));
        assert_eq!(response["ok"], true);
        let page = &response["result"]["result"];
        assert_eq!(page["shadowColorCount"], 35);
        let items = page["items"].as_array().unwrap();
        assert!(!items.is_empty());
        assert!(items.len() <= 16);
        for item in items {
            assert_eq!(item["ordinal"].as_u64(), Some(collected.len() as u64 + 1));
            collected.push(json!({"id":item["id"],"color":item["color"]}));
        }
        if page["nextCursor"].is_null() {
            break;
        }
        let next = page["nextCursor"].as_u64().unwrap();
        assert!(next > cursor);
        cursor = next;
    }
    assert_eq!(collected, palette);
    assert_eq!(
        query(&p, json!({"kind":"palettePage","cursor":35,"limit":1}))["result"]["result"]["items"],
        json!([])
    );
    assert_eq!(saved(&o), before);
    let huge = "\n".repeat(1024);
    let o = owner(json!({"shadowPalette":[{"id":huge,"color":huge}]}));
    assert_eq!(
        query(
            &pin(&o),
            json!({"kind":"palettePage","cursor":0,"limit":16})
        )["error"]["code"],
        "unavailable"
    );
}
#[test]
fn malformed_uninvented_and_overbudget_intents_are_atomic_and_duplicate_bytes_are_rejected() {
    let mut o = owner(json!({}));
    let before = saved(&o);
    for (i, action) in [
        json!({"kind":"select","ordinal":0}),
        json!({"kind":"select","ordinal":1.5}),
        json!({"kind":"select","ordinal":9_007_199_254_740_992_u64}),
        json!({"kind":"select","id":"sh1"}),
        json!({"kind":"remove","ordinal":1}),
        json!({"kind":"rename","ordinal":1,"id":"x"}),
        json!({"kind":"setPalette","palette":[]}),
        json!({"kind":"add","id":"x","color":null}),
        json!({"kind":"add","id":"\0","color":"x"}),
        json!({"kind":"add","id":"é".repeat(513),"color":"x"}),
        json!({"kind":"add","id":"x","color":"x","armed":true}),
        json!({"kind":"add","id":"\n".repeat(1024),"color":"\n".repeat(1024)}),
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(
            edit(&mut o, &format!("invalid-{i}"), action)["error"]["code"],
            "invalid_request"
        );
        assert_eq!(saved(&o), before);
        assert_eq!(o.history_depths(), (0, 0));
    }
    assert_eq!(
        edit(
            &mut o,
            "missing-ordinal",
            json!({"kind":"select","ordinal":7})
        )["error"]["code"],
        "not_found"
    );
    for raw in [
        r#"[]"#,
        r#"{"command":"object.project.shadow.edit","action":[]}"#,
        r#"{"command":"object.project.shadow.edit","command":"object.project.shadow.edit","action":{"kind":"select","ordinal":1}}"#,
        r#"{"command":"object.project.shadow.edit","action":{"kind":"select","ordinal":1,"ordinal":2}}"#,
        r#"{"command":"object.project.shadow.edit","action":{"kind":"add","id":"x","id":"y","color":"c"}}"#,
    ] {
        assert_eq!(
            o.prepare_shadow_delta_json(raw.as_bytes()).unwrap_err(),
            Error::InvalidRequest
        );
    }
    assert_eq!(saved(&o), before);
}
#[test]
fn public_identity_revision_context_cancellation_and_payload_fences_do_not_publish() {
    let mut o = owner(json!({}));
    let before = saved(&o);
    for (field, value, code) in [
        ("instanceId", json!("foreign"), "wrong_instance"),
        ("documentId", json!("foreign"), "wrong_document"),
        ("expectedRevision", json!(1), "stale_revision"),
    ] {
        let q = command(&o, field, json!({"kind":"select","ordinal":1}));
        let mut raw = serde_json::to_value(q).unwrap();
        raw[field] = value;
        assert_eq!(
            dispatch(&mut o, serde_json::from_value(raw).unwrap())["error"]["code"],
            code
        );
    }
    let mut cancelled = command(&o, "cancelled", json!({"kind":"select","ordinal":1}));
    cancelled.cancelled_before_dispatch = true;
    assert_eq!(
        dispatch(&mut o, cancelled)["error"]["code"],
        "cancelled_before_dispatch"
    );
    assert_eq!(saved(&o), before);
    assert_eq!(o.history_depths(), (0, 0));
    let old = pin(&o);
    edit(&mut o, "new-revision", json!({"kind":"select","ordinal":1}));
    let p = pin(&o);
    assert_eq!(
        read(&p, request(&old, "old-pin", json!({"kind":"settings"})))["error"]["code"],
        "stale_revision"
    );
    for (field, value, code) in [
        ("atRevision", json!(0), "stale_revision"),
        ("documentSnapshotId", json!("wrong"), "stale_revision"),
        ("contextId", json!("nested"), "unavailable"),
        ("extra", json!(true), "invalid_request"),
    ] {
        let mut q = request(&p, field, json!({"kind":"settings"}));
        q.payload[field] = value;
        assert_eq!(read(&p, q)["error"]["code"], code);
    }
    for selector in [
        json!({"kind":"palettePage","cursor":0,"limit":0}),
        json!({"kind":"palettePage","cursor":0,"limit":17}),
        json!({"kind":"palettePage","cursor":7,"limit":1}),
        json!({"kind":"palettePage","cursor":9_007_199_254_740_992_u64,"limit":1}),
        json!({"kind":"settings","extra":true}),
        json!(["settings"]),
    ] {
        assert_eq!(query(&p, selector)["error"]["code"], "invalid_request");
    }
    let mut q = request(&p, "read-revision", json!({"kind":"settings"}));
    q.expected_revision = Some(p.content_revision());
    assert_eq!(read(&p, q)["error"]["code"], "invalid_request");
}
#[test]
fn bounded_add_cannot_publish_a_candidate_that_the_project_encoder_cannot_save() {
    const LIMIT: usize = 32 * 1024 * 1024;
    const TARGET: usize = LIMIT - 512;
    let colors = vec!["x".repeat(1024); (LIMIT - 4096) / 1027];
    let source = owner(json!({"palettes":[{"id":"filler","name":"Filler","colors":colors}]}));
    let mut wire: Value = serde_json::from_slice(&saved(&source)).unwrap();
    let mut remaining = TARGET
        .checked_sub(serde_json::to_vec(&wire).unwrap().len())
        .unwrap();
    let colors = wire["project"]["palettes"][0]["colors"]
        .as_array_mut()
        .unwrap();
    while remaining > 0 {
        if remaining < 3 {
            // One string item costs its bytes plus comma and two quotes.
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
    let o = NativeObjectHistory::from_project_file("shadow-budget", &encoded).unwrap();
    let before = saved(&o);
    assert_eq!(before.len(), TARGET);
    let bytes = payload(json!({"kind":"add","id":"i".repeat(1024),"color":"c".repeat(1024)}));
    assert!(bytes.len() < 4096);
    assert_eq!(
        o.prepare_shadow_delta_json(&bytes).unwrap_err(),
        Error::Unavailable
    );
    assert_eq!(saved(&o), before);
    assert_eq!(o.content_revision(), 0);
    assert_eq!(o.history_depths(), (0, 0));
}
#[test]
fn direct_read_rejects_duplicate_members_without_materializing_missing_metadata() {
    let o = owner(json!({}));
    let p = pin(&o);
    let query = json!({"command":"object.project.shadow.read","instanceId":p.instance_id(),
        "documentId":p.document_id(),"atRevision":p.content_revision(),
        "documentSnapshotId":p.snapshot_id(),"contextId":"scene-root","selector":{"kind":"settings"}});
    let bytes = serde_json::to_vec(&query).unwrap();
    assert_eq!(
        p.query_shadow_metadata_json(&bytes).unwrap()["result"]["shadowPalettePresent"],
        false
    );
    let duplicate = String::from_utf8(bytes).unwrap().replace(
        "\"kind\":\"settings\"",
        "\"kind\":\"settings\",\"kind\":\"settings\"",
    );
    assert_eq!(
        p.query_shadow_metadata_json(duplicate.as_bytes()),
        Err(Error::InvalidRequest)
    );
    let mut wrong = query;
    wrong["command"] = json!("object.project.palette.read");
    assert_eq!(
        p.query_shadow_metadata_json(&serde_json::to_vec(&wrong).unwrap()),
        Err(Error::InvalidRequest)
    );
    assert!(project(&o).get("shadowPalette").is_none());
}
