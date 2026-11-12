//! Common validator cases for the lane-7 brush resource operations, authored UNRUN (no compiler by rule).
use super::*;
use serde_json::json;

fn publish() -> Value {
    json!({"command":"object.brush.bitmap.publish","documentSnapshotId":"snapshot-7","assetId":"asset-project-asset-1",
        "brushId":"i1:abc","width":241,"height":41})
}
fn inspect() -> Value {
    json!({"action":"inspect","atRevision":7,"documentSnapshotId":"snapshot-7","brushId":"i1:abc"})
}
fn scene(offset: u64) -> Value {
    json!({"action":"scene","atRevision":7,"documentSnapshotId":"snapshot-7","contextId":"scene-root","frame":12,
        "offset":offset,"limit":1})
}
fn receipt() -> Value {
    json!({"applied":true,"historyEntriesAdded":0,"brushId":"i1:abc","version":"1","width":241,"height":41,"bytes":39524})
}
fn ink_entry() -> Value {
    json!({"sourceObjectIndex":5,"layerUid":"L1","strokeId":"native-brush:1:0","brushId":"i1:abc",
        "ink":{"version":"3","width":241,"height":41,"bytes":39524}})
}
fn missing_entry() -> Value {
    json!({"sourceObjectIndex":2,"layerUid":"L0","strokeId":"native-brush:2:0","brushId":"i1:m1",
        "unavailable":{"code":"ink-missing"}})
}
fn page(offset: u64, next: Value, total: u64, entries: Vec<Value>) -> Value {
    json!({"atRevision":7,"documentSnapshotId":"snapshot-7","contextId":"scene-root","frame":12,
        "offset":offset,"nextOffset":next,"totalEntries":total,"entries":entries})
}
fn envelope(operation: &str, payload: Value, expected: Option<u64>) -> NativeApplicationRequest {
    NativeApplicationRequest {
        api_version: 2,
        request_id: "brush-7".into(),
        instance_id: "instance-A".into(),
        document_id: "document-A".into(),
        expected_revision: expected,
        operation: operation.into(),
        payload,
        cancelled_before_dispatch: false,
    }
}
fn response(
    request: &NativeApplicationRequest,
    revision: u64,
    result: Value,
) -> NativeApplicationResponse {
    NativeApplicationResponse {
        api_version: 2,
        request_id: request.request_id.clone(),
        instance_id: request.instance_id.clone(),
        document_id: request.document_id.clone(),
        content_revision: revision,
        ok: true,
        result: Some(result),
        error: None,
    }
}

#[test]
fn publication_and_both_reads_are_admitted_in_their_closed_shape() {
    assert!(request(PUBLISH, &publish()));
    assert!(request(QUERY, &inspect()));
    assert!(request(QUERY, &scene(0)));
    assert!(request(QUERY, &scene(4096)));
    assert!(!request("query.document.object.brush", &publish()));
    assert!(owns(PUBLISH) && owns(QUERY) && !owns("command.document.object.asset.attach"));
}

#[test]
fn a_missing_null_or_extra_member_refuses_every_request() {
    for (operation, payload) in [(PUBLISH, publish()), (QUERY, inspect()), (QUERY, scene(0))] {
        for key in payload.as_object().unwrap().keys() {
            let mut missing = payload.clone();
            missing.as_object_mut().unwrap().remove(key);
            assert!(!request(operation, &missing), "missing {key}");
            let mut nulled = payload.clone();
            nulled[key] = Value::Null;
            assert!(!request(operation, &nulled), "null {key}");
        }
        let mut extra = payload.clone();
        extra["bytes"] = json!("QUJD");
        assert!(!request(operation, &extra), "extra member");
    }
}

#[test]
fn sizes_ids_and_selectors_keep_their_bounds() {
    for (key, value) in [
        ("width", json!(0)),
        ("width", json!(4097)),
        ("width", json!(1.5)),
        ("width", json!("241")),
        ("height", json!(-1)),
        ("height", json!(u64::MAX)),
        ("assetId", json!("  ")),
        ("brushId", json!("")),
        ("brushId", json!("a\u{0}b")),
        ("brushId", json!("x".repeat(256))),
        ("brushId", json!("\u{e9}".repeat(128))),
        ("documentSnapshotId", json!("-snapshot")),
        ("command", json!("object.brush.bitmap.remove")),
    ] {
        let mut payload = publish();
        payload[key] = value.clone();
        assert!(!request(PUBLISH, &payload), "{key} {value}");
    }
    let mut edge = publish();
    edge["width"] = json!(4096);
    edge["height"] = json!(4096);
    edge["brushId"] = json!("x".repeat(255));
    assert!(request(PUBLISH, &edge), "the inclusive bounds pass");
    edge["brushId"] = json!("\u{e9}".repeat(127));
    assert!(request(PUBLISH, &edge));
    for (key, value) in [
        ("limit", json!(2)),
        ("limit", json!(0)),
        ("offset", json!(4097)),
        ("offset", json!(-1)),
        ("frame", json!(4294967296u64)),
        ("frame", json!(-1)),
        ("contextId", json!("a b")),
        ("atRevision", json!(9007199254740992u64)),
        ("action", json!("list")),
    ] {
        let mut payload = scene(0);
        payload[key] = value.clone();
        assert!(!request(QUERY, &payload), "{key} {value}");
    }
    let mut last = scene(0);
    last["frame"] = json!(4294967295u64);
    assert!(request(QUERY, &last));
    let mut blank = inspect();
    blank["brushId"] = json!("\t");
    assert!(!request(QUERY, &blank));
}

#[test]
fn the_publication_receipt_names_the_bitmap_it_published_and_adds_no_history() {
    assert!(result(PUBLISH, &publish(), &receipt()));
    let mut reused = receipt();
    reused["applied"] = json!(false);
    reused["version"] = json!("18446744073709551615");
    assert!(
        result(PUBLISH, &publish(), &reused),
        "the largest u64 version"
    );
    for (key, value) in [
        ("applied", json!(1)),
        ("historyEntriesAdded", json!(1)),
        ("brushId", json!("other")),
        ("width", json!(240)),
        ("height", json!(42)),
        ("bytes", json!(39520)),
        ("bytes", json!(39524.5)),
        ("version", json!(1)),
        ("version", json!("0")),
        ("version", json!("01")),
        ("version", json!("")),
        ("version", json!("18446744073709551616")),
        ("version", json!("1a")),
    ] {
        let mut flawed = receipt();
        flawed[key] = value.clone();
        assert!(!result(PUBLISH, &publish(), &flawed), "{key} {value}");
    }
    for key in ["applied", "version", "bytes"] {
        let mut missing = receipt();
        missing.as_object_mut().unwrap().remove(key);
        assert!(!result(PUBLISH, &publish(), &missing), "missing {key}");
    }
    let mut extra = receipt();
    extra["assetId"] = json!("asset-project-asset-1");
    assert!(!result(PUBLISH, &publish(), &extra));
    let mut wide = publish();
    wide["width"] = json!(4096);
    wide["height"] = json!(4096);
    let mut full = receipt();
    full["width"] = json!(4096);
    full["height"] = json!(4096);
    full["bytes"] = json!(67108864u64);
    assert!(
        result(PUBLISH, &wide, &full),
        "the baseline maximum bake costs 64 MiB"
    );
}

#[test]
fn an_inspect_row_is_resident_with_a_cost_or_absent_with_nothing_else() {
    let present = json!({"atRevision":7,"documentSnapshotId":"snapshot-7",
        "brush":{"brushId":"i1:abc","resident":true,"version":"3","width":241,"height":41,"bytes":39524}});
    let absent = json!({"atRevision":7,"documentSnapshotId":"snapshot-7","brush":{"brushId":"i1:abc","resident":false}});
    assert!(result(QUERY, &inspect(), &present));
    assert!(result(QUERY, &inspect(), &absent));
    let edit = |base: &Value, change: &dyn Fn(&mut Value)| {
        let mut v = base.clone();
        change(&mut v);
        v
    };
    for (label, flawed) in [
        (
            "another revision",
            edit(&present, &|v| v["atRevision"] = json!(6)),
        ),
        (
            "another snapshot",
            edit(&present, &|v| v["documentSnapshotId"] = json!("snapshot-6")),
        ),
        (
            "another brush",
            edit(&present, &|v| v["brush"]["brushId"] = json!("other")),
        ),
        ("cost", edit(&present, &|v| v["brush"]["bytes"] = json!(15))),
        (
            "resident without a cost",
            edit(&absent, &|v| v["brush"]["resident"] = json!(true)),
        ),
        (
            "absent with a version",
            edit(&absent, &|v| v["brush"]["version"] = json!("1")),
        ),
        (
            "residency not boolean",
            edit(&absent, &|v| v["brush"]["resident"] = json!("false")),
        ),
        ("extra member", edit(&present, &|v| v["pixels"] = json!([]))),
    ] {
        assert!(!result(QUERY, &inspect(), &flawed), "{label}");
    }
}

#[test]
fn a_scene_page_holds_one_entry_with_a_bitmap_or_a_reason_and_pages_in_order() {
    assert!(result(
        QUERY,
        &scene(0),
        &page(0, json!(1), 2, vec![ink_entry()])
    ));
    assert!(result(
        QUERY,
        &scene(1),
        &page(1, Value::Null, 2, vec![missing_entry()])
    ));
    assert!(
        result(QUERY, &scene(2), &page(2, Value::Null, 2, vec![])),
        "the page at the end is empty"
    );
    assert!(
        result(QUERY, &scene(0), &page(0, Value::Null, 0, vec![])),
        "an empty scene"
    );
    let mut refused = missing_entry();
    refused["unavailable"]["code"] = json!("ink-refused: over budget");
    assert!(result(
        QUERY,
        &scene(0),
        &page(0, Value::Null, 1, vec![refused])
    ));
    // Each flawed page answers the request for ITS OWN offset, so the refusal is the page's flaw and not a selector mismatch.
    for (label, offset, flawed) in [
        (
            "two entries",
            0,
            page(0, json!(2), 3, vec![ink_entry(), ink_entry()]),
        ),
        ("no entry before the end", 0, page(0, json!(1), 2, vec![])),
        (
            "an entry past the end",
            2,
            page(2, Value::Null, 2, vec![ink_entry()]),
        ),
        (
            "next offset skipped",
            0,
            page(0, json!(2), 3, vec![ink_entry()]),
        ),
        (
            "next offset kept at the end",
            1,
            page(1, json!(2), 2, vec![ink_entry()]),
        ),
        (
            "missing next offset",
            0,
            page(0, Value::Null, 2, vec![ink_entry()]),
        ),
        (
            "total beyond the bound",
            0,
            page(0, json!(1), 4097, vec![ink_entry()]),
        ),
        (
            "offset beyond the total",
            3,
            page(3, Value::Null, 2, vec![]),
        ),
        (
            "another offset than asked",
            0,
            page(1, json!(2), 3, vec![ink_entry()]),
        ),
    ] {
        assert!(!result(QUERY, &scene(offset), &flawed), "{label}");
    }
    let mut frame = page(0, json!(1), 2, vec![ink_entry()]);
    frame["frame"] = json!(13);
    assert!(!result(QUERY, &scene(0), &frame), "another frame");
    let mut context = page(0, json!(1), 2, vec![ink_entry()]);
    context["contextId"] = json!("other");
    assert!(!result(QUERY, &scene(0), &context), "another context");
    let mut snapshot = page(0, json!(1), 2, vec![ink_entry()]);
    snapshot["documentSnapshotId"] = json!("snapshot-6");
    assert!(!result(QUERY, &scene(0), &snapshot), "another snapshot");
}

#[test]
fn a_scene_entry_has_exactly_one_of_ink_and_reason_and_a_true_cost() {
    let mut both = ink_entry();
    both["unavailable"] = json!({"code":"ink-missing"});
    let mut neither = ink_entry();
    neither.as_object_mut().unwrap().remove("ink");
    let mut cost = ink_entry();
    cost["ink"]["bytes"] = json!(4);
    let mut version = ink_entry();
    version["ink"]["version"] = json!("0");
    let mut blank = missing_entry();
    blank["unavailable"]["code"] = json!(" ");
    let mut nested = missing_entry();
    nested["unavailable"]["detail"] = json!("x");
    let mut negative = ink_entry();
    negative["sourceObjectIndex"] = json!(-1);
    let mut far = ink_entry();
    far["sourceObjectIndex"] = json!(MAX_SOURCE_INDEX + 1);
    let mut extra = ink_entry();
    extra["pixels"] = json!([]);
    let mut stroke = ink_entry();
    stroke["strokeId"] = json!("  ");
    let mut huge = ink_entry();
    huge["ink"]["width"] = json!(u64::MAX);
    for (label, flawed) in [
        ("both", both),
        ("neither", neither),
        ("cost", cost),
        ("version", version),
        ("blank reason", blank),
        ("reason detail", nested),
        ("negative index", negative),
        ("index beyond the bound", far),
        ("extra member", extra),
        ("blank stroke", stroke),
        ("width u64::MAX (no overflow)", huge),
    ] {
        assert!(!entry(&flawed), "{label}");
        assert!(
            !result(QUERY, &scene(0), &page(0, Value::Null, 1, vec![flawed])),
            "{label}"
        );
    }
    assert!(entry(&ink_entry()) && entry(&missing_entry()));
}

#[test]
fn a_response_keeps_the_revision_of_its_kind_and_stays_in_the_envelope() {
    let publish_request = envelope(PUBLISH, publish(), Some(7));
    assert!(response_matches(
        &publish_request,
        &response(&publish_request, 7, receipt())
    ));
    assert!(!response_matches(
        &publish_request,
        &response(&publish_request, 8, receipt())
    ));
    assert!(!response_matches(
        &publish_request,
        &response(&publish_request, 6, receipt())
    ));
    let read = envelope(QUERY, scene(0), None);
    let answer = page(0, json!(1), 2, vec![ink_entry()]);
    assert!(response_matches(&read, &response(&read, 7, answer.clone())));
    assert!(!response_matches(
        &read,
        &response(&read, 8, answer.clone())
    ));
    for q in [&publish_request, &read] {
        let good = if q.operation == PUBLISH {
            receipt()
        } else {
            answer.clone()
        };
        let mut other_request = response(q, 7, good.clone());
        other_request.request_id = "brush-8".into();
        assert!(!response_matches(q, &other_request));
        let mut foreign = response(q, 7, good.clone());
        foreign.instance_id = "instance-B".into();
        assert!(!response_matches(q, &foreign));
        foreign = response(q, 7, good.clone());
        foreign.document_id = "document-B".into();
        assert!(!response_matches(q, &foreign));
        let mut missing = response(q, 7, good.clone());
        missing.result = None;
        assert!(!response_matches(q, &missing));
    }
    let mut unbounded = receipt();
    unbounded["pad"] = json!("x".repeat(NATIVE_MAX_MESSAGE_BYTES));
    assert!(!response_matches(
        &publish_request,
        &response(&publish_request, 7, unbounded)
    ));
    let unpinned = envelope(QUERY, json!({"action":"inspect"}), None);
    assert!(!response_matches(
        &unpinned,
        &response(&unpinned, 7, json!({}))
    ));
    let other = envelope("query.document.object", json!({}), None);
    assert!(response_matches(&other, &response(&other, 0, json!({}))));
}

#[test]
fn registration_declares_the_operations_available_on_the_session_owned_store_with_no_fallback() {
    let catalog = crate::capabilities::native_catalog();
    for operation in [PUBLISH, QUERY] {
        let descriptor = catalog.capability_for_operation(operation).unwrap();
        assert_eq!(descriptor["id"], "native.object.brush-resources");
        assert_eq!(descriptor["availability"]["state"], "available");
        assert_eq!(descriptor["authority"]["fallback"], "none");
        assert!(crate::capabilities::validate_native_operation_registration(operation).is_ok());
        assert!(crate::capabilities::validate_native_operation(operation).is_ok());
    }
}

#[test]
fn the_envelope_requires_a_revision_for_the_command_and_forbids_one_for_a_read() {
    assert!(envelope(PUBLISH, publish(), Some(7))
        .validate_input()
        .is_ok());
    assert!(envelope(PUBLISH, publish(), None).validate_input().is_err());
    for payload in [inspect(), scene(0)] {
        assert!(envelope(QUERY, payload.clone(), None)
            .validate_input()
            .is_ok());
        assert!(envelope(QUERY, payload, Some(7)).validate_input().is_err());
    }
    let mut open = publish();
    open["pixels"] = json!("QUJD");
    assert!(envelope(PUBLISH, open, Some(7)).validate_input().is_err());
    assert!(envelope(PUBLISH, publish(), Some(7)).validate().is_ok());
    assert!(envelope(QUERY, scene(0), None).validate().is_ok());
}

#[test]
fn the_shared_result_check_routes_the_brush_operations_through_this_module() {
    assert!(crate::native_contract::validate_result(
        PUBLISH,
        "document-A",
        &publish(),
        &receipt()
    ));
    let mut flawed = receipt();
    flawed["bytes"] = json!(1);
    assert!(!crate::native_contract::validate_result(
        PUBLISH,
        "document-A",
        &publish(),
        &flawed
    ));
    let answer = page(0, json!(1), 2, vec![ink_entry()]);
    assert!(crate::native_contract::validate_result(
        QUERY,
        "document-A",
        &scene(0),
        &answer
    ));
    assert!(!crate::native_contract::validate_result(
        QUERY,
        "document-A",
        &scene(1),
        &answer
    ));
    // The request is validated first: a payload outside the closed shape never reaches the receipt check.
    let open = json!({"command":"object.brush.bitmap.erase"});
    assert!(!crate::native_contract::validate_result(
        PUBLISH,
        "document-A",
        &open,
        &receipt()
    ));
}
