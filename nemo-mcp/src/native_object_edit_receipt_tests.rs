use super::tests::{envelope, response, valid, P};
use super::*;
use crate::contract::{NativeApplicationRequest, NativeApplicationResponse};
use serde_json::{json, Value};

#[test]
fn receipts_are_the_compact_command_result_and_a_duplicate_names_its_minted_layer() {
    for (suffix, _) in valid() {
        let operation = format!("{P}{suffix}");
        if suffix == "clipboard.copy" || suffix == "clipboard.cut" {
            assert!(result(
                &operation,
                &json!({"applied":false,"historyEntriesAdded":0,"clipboard":null})
            ));
            assert!(!result(
                &operation,
                &json!({"applied":false,"historyEntriesAdded":0})
            ));
            continue;
        }
        if suffix == "layer.duplicate" {
            assert!(result(
                &operation,
                &json!({"applied":true,"historyEntriesAdded":1,"layerUid":"native-layer:8:0"})
            ));
            for bad in [
                json!({"applied":true,"historyEntriesAdded":1}),
                json!({"applied":false,"historyEntriesAdded":0,"layerUid":"x"}),
                json!({"applied":true,"historyEntriesAdded":1,"layerUid":""}),
            ] {
                assert!(!result(&operation, &bad), "{bad}");
            }
            continue;
        }
        assert!(
            result(&operation, &json!({"applied":true,"historyEntriesAdded":1})),
            "{suffix}"
        );
        assert!(
            result(
                &operation,
                &json!({"applied":false,"historyEntriesAdded":0})
            ),
            "{suffix}"
        );
        for bad in [
            json!({"applied":true,"historyEntriesAdded":0}),
            json!({"applied":false,"historyEntriesAdded":1}),
            json!({"applied":true,"historyEntriesAdded":1,"layerUid":"x"}),
            json!({"applied":1,"historyEntriesAdded":1}),
            json!({"applied":true}),
            Value::Null,
            json!([]),
        ] {
            assert!(!result(&operation, &bad), "{suffix}: {bad}");
        }
    }
    assert!(!result(
        "command.document.object.layer.create",
        &json!({"applied":true,"historyEntriesAdded":1,"layerUid":"x"})
    ));
}

#[test]
fn the_shared_result_check_routes_the_object_edit_operations_through_this_module() {
    let applied = json!({"applied":true,"historyEntriesAdded":1});
    let noop = json!({"applied":false,"historyEntriesAdded":0});
    let minted = json!({"applied":true,"historyEntriesAdded":1,"layerUid":"native-layer:8:0"});
    for (suffix, payload) in valid() {
        let operation = format!("{P}{suffix}");
        let shared = |body: &Value, receipt: &Value| {
            crate::native_contract::validate_result(&operation, "document-A", body, receipt)
        };
        if suffix == "clipboard.copy" || suffix == "clipboard.cut" {
            assert!(shared(
                &payload,
                &json!({"applied":false,"historyEntriesAdded":0,"clipboard":null})
            ));
            assert!(!shared(&payload, &applied));
            assert!(!shared(&payload, &noop));
        } else if suffix == "layer.duplicate" {
            assert!(shared(&payload, &minted), "{suffix}");
            for flawed in [
                &applied,
                &noop,
                &json!({"applied":true,"historyEntriesAdded":1,"layerUid":""}),
                &json!({"applied":false,"historyEntriesAdded":0,"layerUid":"x"}),
            ] {
                assert!(!shared(&payload, flawed), "{suffix}: {flawed}");
            }
        } else {
            for accepted in [&applied, &noop] {
                assert!(shared(&payload, accepted), "{suffix}: {accepted}");
            }
            for flawed in [
                &minted,
                &json!({"applied":true,"historyEntriesAdded":0}),
                &json!({"applied":false,"historyEntriesAdded":1}),
                &json!({"applied":true,"historyEntriesAdded":1,"extra":true}),
                &Value::Null,
            ] {
                assert!(!shared(&payload, flawed), "{suffix}: {flawed}");
            }
        }
        // The request is validated first: a payload outside the closed shape never reaches the receipt check.
        let mut foreign = payload.clone();
        foreign["command"] = json!("object.layer.policy.edit");
        let receipt = if suffix == "layer.duplicate" {
            &minted
        } else {
            &applied
        };
        assert!(!shared(&foreign, receipt), "{suffix}");
    }
}

#[test]
fn the_revision_advances_by_exactly_the_applied_edit() {
    let changed = json!({"applied":true,"historyEntriesAdded":1});
    let noop = json!({"applied":false,"historyEntriesAdded":0});
    assert!(revision_matches(Some(7), 8, &changed));
    assert!(revision_matches(Some(7), 7, &noop));
    assert!(!revision_matches(Some(7), 7, &changed));
    assert!(!revision_matches(Some(7), 8, &noop));
    assert!(!revision_matches(None, 8, &changed));
    assert!(!revision_matches(Some(u64::MAX), 0, &changed));
}

#[test]
fn responses_must_name_the_same_owner_and_document_and_advance_exactly() {
    let command = envelope("layer.rename", valid()[2].1.clone());
    let reply = |revision: u64, instance: &str, document: &str, applied: bool| {
        response(
            json!({"apiVersion":2,"requestId":"edit-1","instanceId":instance,"documentId":document,
            "contentRevision":revision,"ok":true,"result":{"applied":applied,"historyEntriesAdded":u64::from(applied)}}),
        )
    };
    assert!(response_matches(
        &command,
        &reply(8, "instance-A", "document-A", true)
    ));
    assert!(response_matches(
        &command,
        &reply(7, "instance-A", "document-A", false)
    ));
    assert!(!response_matches(
        &command,
        &reply(7, "instance-A", "document-A", true)
    ));
    assert!(!response_matches(
        &command,
        &reply(8, "instance-A", "document-A", false)
    ));
    assert!(!response_matches(
        &command,
        &reply(8, "instance-B", "document-A", true)
    ));
    assert!(!response_matches(
        &command,
        &reply(8, "instance-A", "document-B", true)
    ));
    let refusal = response(
        json!({"apiVersion":2,"requestId":"edit-1","instanceId":"instance-A","documentId":"document-A",
        "contentRevision":7,"ok":false,"error":{"code":"stale_revision","message":"stale"}}),
    );
    assert!(
        !response_matches(&command, &refusal),
        "a success correlation needs a result"
    );
    let mut foreign = command.clone();
    foreign.operation = "command.document.object.layer.visibility.set".into();
    assert!(
        response_matches(&foreign, &refusal),
        "undeclared operations are not judged here"
    );
}

#[test]
fn the_catalog_declares_each_operation_once_under_five_owner_descriptors() {
    let family = |suffix: &str| match suffix {
        "layer.delete" | "layer.duplicate" | "layer.rename" | "layer.reorder" => {
            "native.object.layer-lifecycle"
        }
        s if s.starts_with("group.") => "native.object.group",
        "selection.delete"
        | "selection.duplicate"
        | "layer.split"
        | "clipboard.paste"
        | "clipboard.copy"
        | "clipboard.cut" => "native.object.selection-edit",
        "fill.trace" | "fill.brush" => "native.object.fill",
        _ => "native.object.boolean",
    };
    let mut declared = 0;
    for (suffix, _) in valid() {
        let operation = format!("{P}{suffix}");
        let descriptor = crate::capabilities::native_catalog()
            .capability_for_operation(&operation)
            .unwrap_or_else(|| panic!("{operation} is not declared"));
        assert_eq!(descriptor["id"], family(suffix), "{suffix}");
        assert_eq!(descriptor["authority"]["fallback"], "none");
        assert_eq!(descriptor["authority"]["adapterOwnsWritableMirror"], false);
        assert_eq!(descriptor["availability"]["state"], "available");
        // A Fill Brush outline carries up to 256 segments: the fill descriptor declares the engine payload bound.
        let bound = if suffix.starts_with("fill.") { 65536 } else { 4096 };
        assert_eq!(descriptor["resourceBoundary"]["maxRequestBytes"], bound, "{suffix}");
        declared += 1;
    }
    assert_eq!(declared, 21);
}

#[test]
fn every_descriptor_example_is_a_valid_request_with_a_correlated_valid_response() {
    let mut seen = 0;
    for descriptor in crate::capabilities::native_catalog().descriptors() {
        let id = descriptor["id"].as_str().unwrap();
        if !id.starts_with("native.object.")
            || ![
                "layer-lifecycle",
                "group",
                "selection-edit",
                "boolean",
                "fill",
            ]
            .iter()
            .any(|f| id.ends_with(f))
        {
            continue;
        }
        for example in descriptor["examples"].as_array().unwrap() {
            let command: NativeApplicationRequest =
                serde_json::from_value(example["request"].clone()).unwrap();
            assert!(admitted(&command.operation), "{id}");
            assert!(command.validate().is_ok(), "{id}: {}", command.operation);
            let reply: NativeApplicationResponse =
                serde_json::from_value(example["response"].clone()).unwrap();
            assert!(
                response_matches(&command, &reply),
                "{id}: {}",
                command.operation
            );
            assert!(
                result(&command.operation, reply.result.as_ref().unwrap()),
                "{id}: {}",
                command.operation
            );
            seen += 1;
        }
    }
    assert_eq!(seen, 21, "one copyable example per declared operation");
}

fn literal_clip() -> Value {
    json!({"contextId":"scene-root","layerUid":"layer-a","frame":2,"records":[{
        "schemaVersion":1,"family":"closed-solid-fill-cubic-path",
        "target":{"contextId":"scene-root","layerUid":"layer-a","strokeId":"stroke-1","frameScope":{"kind":"authored","frame":0}},
        "geometry":{"kind":"cubic-path","closed":true,"coordinateSpace":"document-world","handleSpace":"relative","segments":[{"point":{"x":0,"y":0},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}},{"point":{"x":10,"y":10},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}}]},
        "fill":{"kind":"solid","r":1,"g":0,"b":0,"a":1}
    }]})
}

#[test]
fn clipboard_results_are_closed_and_copy_and_cut_have_distinct_dispositions() {
    let packet = literal_clip();
    for suffix in ["clipboard.copy", "clipboard.cut"] {
        let op = format!("{P}{suffix}");
        let applied = suffix == "clipboard.cut";
        let good =
            json!({"applied":applied,"historyEntriesAdded":u64::from(applied),"clipboard":packet});
        assert!(result(&op, &good));
        let mut mutations = vec![
            json!({"applied":true,"historyEntriesAdded":1,"clipboard":null}),
            json!({"applied":false,"historyEntriesAdded":1,"clipboard":packet}),
            json!({"applied":true,"historyEntriesAdded":0,"clipboard":packet}),
            json!({"applied":"false","historyEntriesAdded":0,"clipboard":packet}),
            json!({"applied":false,"historyEntriesAdded":0,"clipboard":{}}),
        ];
        for key in ["applied", "historyEntriesAdded", "clipboard"] {
            let mut missing = good.clone();
            missing.as_object_mut().unwrap().remove(key);
            mutations.push(missing);
        }
        let mut extra = good.clone();
        extra["extra"] = json!(true);
        mutations.push(extra);
        let mut extra = good.clone();
        extra["clipboard"]["extra"] = json!(true);
        mutations.push(extra);
        let mut extra = good.clone();
        extra["clipboard"]["records"][0]["extra"] = json!(true);
        mutations.push(extra);
        let mut opposite = good.clone();
        opposite["applied"] = json!(!applied);
        opposite["historyEntriesAdded"] = json!(u64::from(!applied));
        mutations.push(opposite);
        for bad in mutations {
            assert!(!result(&op, &bad), "{suffix}: {bad}");
        }
    }
}

#[test]
fn clipboard_correlation_uses_requested_cell_not_the_inherited_record_frame() {
    for suffix in ["clipboard.copy", "clipboard.cut"] {
        let payload = valid()
            .into_iter()
            .find(|(name, _)| *name == suffix)
            .unwrap()
            .1;
        let command = envelope(suffix, payload);
        let applied = suffix == "clipboard.cut";
        let good = json!({"apiVersion":2,"requestId":"edit-1","instanceId":"instance-A","documentId":"document-A",
            "contentRevision":7+u64::from(applied),"ok":true,"result":{"applied":applied,"historyEntriesAdded":u64::from(applied),"clipboard":literal_clip()}});
        assert!(response_matches(&command, &response(good.clone())));
        for (key, wrong) in [
            ("contextId", json!("foreign-context")),
            ("layerUid", json!("foreign-layer")),
            ("frame", json!(0)),
        ] {
            let mut bad = good.clone();
            bad["result"]["clipboard"][key] = wrong;
            assert!(!response_matches(&command, &response(bad)), "{key}");
        }
        let mut bad = good.clone();
        bad["result"]["clipboard"]["unexpected"] = json!(true);
        assert!(!response_matches(&command, &response(bad)));
        let mut bad = good;
        bad["contentRevision"] = json!(9);
        assert!(!response_matches(&command, &response(bad)));
    }
}

#[test]
fn clipboard_effect_presence_survives_both_result_and_paste_admission() {
    for effects in [
        None,
        Some(Value::Null),
        Some(json!([])),
        Some(json!([{"type":"blur","enabled":false,"p1":null,"p2":0,"keys":null}])),
    ] {
        let mut packet = literal_clip();
        if let Some(effects) = effects {
            packet["records"][0]["effects"] = effects;
        }
        for suffix in ["clipboard.copy", "clipboard.cut"] {
            let applied = suffix == "clipboard.cut";
            assert!(result(
                &format!("{P}{suffix}"),
                &json!({"applied":applied,"historyEntriesAdded":u64::from(applied),"clipboard":packet})
            ));
        }
        let mut paste = valid()
            .into_iter()
            .find(|(name, _)| *name == "clipboard.paste")
            .unwrap()
            .1;
        paste["clip"] = packet;
        assert!(request(&format!("{P}clipboard.paste"), &paste));
    }
}

#[test]
fn asset_clipboard_provenance_is_closed_bounded_and_required_in_both_directions() {
    let mut packet = literal_clip();
    packet["rasters"] = json!([{"target":{"contextId":"scene-root","layerUid":"layer-a",
        "strokeId":"ink","frameScope":{"kind":"authored","frame":0}},
        "item":{"assetId":"image-1","x":16,"y":16,"width":8,"height":8,"isBitmapBrush":true,"brushGroupId":"brush-1"}}]);
    packet["source"] = json!({"instanceId":"instance-A","documentId":"document-A","contentRevision":7,"strokeIds":["anchor"]});
    let admitted = |clip: &Value| {
        let mut paste = valid()
            .into_iter()
            .find(|(name, _)| *name == "clipboard.paste")
            .unwrap()
            .1;
        paste["clip"] = clip.clone();
        request(&format!("{P}clipboard.paste"), &paste)
            && result(
                &format!("{P}clipboard.copy"),
                &json!({"applied":false,"historyEntriesAdded":0,"clipboard":clip}),
            )
    };
    assert!(admitted(&packet));
    for case in 0..14 {
        let mut bad = packet.clone();
        match case {
            0 => {
                bad.as_object_mut().unwrap().remove("source");
            }
            1 => {
                bad.as_object_mut().unwrap().remove("rasters");
            }
            2 => bad["rasters"] = json!([]),
            3 => bad["rasters"] = Value::Null,
            4 => bad["rasters"][0]["extra"] = json!(true),
            5 => bad["rasters"][0]["target"]["frameScope"]["extra"] = json!(true),
            6 => bad["rasters"][0]["item"]["encodedBytes"] = json!("not admitted"),
            7 => bad["source"]["extra"] = json!(true),
            8 => bad["source"]["instanceId"] = json!(""),
            9 => bad["source"]["documentId"] = json!(""),
            10 => bad["source"]["contentRevision"] = json!(-1),
            11 => bad["source"]["contentRevision"] = json!(9_007_199_254_740_992u64),
            12 => bad["source"]["strokeIds"] = json!([]),
            _ => bad["source"]["strokeIds"] = json!([""]),
        }
        assert!(!admitted(&bad), "case {case}");
    }
}
