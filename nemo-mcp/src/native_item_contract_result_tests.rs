use super::*;

#[test]
fn item_receipts_name_the_operation_and_the_stroke_they_ran_on() {
    let all = commands();
    let names = [
        "raster.create",
        "raster.move",
        "raster.resize",
        "raster.rotate",
        "raster.opacity",
        "raster.delete",
        "text.create",
        "text.edit",
        "text.delete",
    ];
    for (payload, name) in all.iter().zip(names) {
        let stroke = if name.ends_with(".create") {
            if name.starts_with("raster.") {
                "native-raster:8:0"
            } else {
                "native-text:8:0"
            }
        } else {
            "native-raster:3:0"
        };
        let good =
            json!({"applied":true,"historyEntriesAdded":1,"operation":name,"strokeId":stroke});
        assert!(result(ITEM, payload, &good), "{name}");
        for bad in [
            json!({"applied":true,"historyEntriesAdded":0,"operation":name,"strokeId":stroke}),
            json!({"applied":false,"historyEntriesAdded":1,"operation":name,"strokeId":stroke}),
            json!({"applied":true,"historyEntriesAdded":1,"operation":"raster.nudge","strokeId":stroke}),
            json!({"applied":true,"historyEntriesAdded":1,"operation":name,"strokeId":"other:9:9"}),
            json!({"applied":true,"historyEntriesAdded":1,"operation":name,"strokeId":stroke,"document":{}}),
        ] {
            assert!(!result(ITEM, payload, &bad), "{name} {bad}");
        }
        let other = if name == "raster.move" {
            "raster.resize"
        } else {
            "raster.move"
        };
        assert!(!result(
            ITEM,
            payload,
            &json!({"applied":true,"historyEntriesAdded":1,"operation":other,"strokeId":stroke})
        ));
    }
    // A created item reports an allocated id of its own family: a text id for a raster create is a different record kind.
    assert!(!result(
        ITEM,
        &all[0],
        &json!({"applied":true,"historyEntriesAdded":1,"operation":"raster.create","strokeId":"native-text:8:0"})
    ));
    assert!(result(
        ITEM,
        &all[1],
        &json!({"applied":false,"historyEntriesAdded":0,"operation":"raster.move","strokeId":"native-raster:3:0"})
    ));
    for operation in [ATTACH, REMOVE] {
        assert!(result(
            operation,
            &Value::Null,
            &json!({"applied":true,"historyEntriesAdded":1})
        ));
        assert!(result(
            operation,
            &Value::Null,
            &json!({"applied":false,"historyEntriesAdded":0})
        ));
        assert!(!result(
            operation,
            &Value::Null,
            &json!({"applied":true,"historyEntriesAdded":0})
        ));
        assert!(!result(
            operation,
            &Value::Null,
            &json!({"applied":true,"historyEntriesAdded":1,"assetId":"asset-x"})
        ));
    }
}

#[test]
fn a_response_advances_the_revision_by_its_own_disposition_and_stays_in_the_envelope() {
    let q = envelope(ITEM, commands()[1].clone(), Some(7));
    let changed = json!({"applied":true,"historyEntriesAdded":1,"operation":"raster.move","strokeId":"native-raster:3:0"});
    let noop = json!({"applied":false,"historyEntriesAdded":0,"operation":"raster.move","strokeId":"native-raster:3:0"});
    assert!(response_matches(&q, &response(&q, 8, changed.clone())));
    assert!(response_matches(&q, &response(&q, 7, noop.clone())));
    assert!(!response_matches(&q, &response(&q, 7, changed.clone())));
    assert!(!response_matches(&q, &response(&q, 8, noop)));
    let mut other_request = response(&q, 8, changed.clone());
    other_request.request_id = "item-8".into();
    assert!(!response_matches(&q, &other_request));
    let mut foreign = response(&q, 8, changed.clone());
    foreign.instance_id = "instance-B".into();
    assert!(!response_matches(&q, &foreign));
    foreign = response(&q, 8, changed.clone());
    foreign.document_id = "document-B".into();
    assert!(!response_matches(&q, &foreign));
    let mut missing = response(&q, 8, changed.clone());
    missing.result = None;
    assert!(!response_matches(&q, &missing));
    let mut unbounded = changed;
    unbounded["pad"] = json!("x".repeat(NATIVE_MAX_MESSAGE_BYTES));
    assert!(!response_matches(&q, &response(&q, 8, unbounded)));
    let near_max = envelope(REMOVE, remove(), Some(OVERFLOWING_REVISION));
    assert!(!response_matches(
        &near_max,
        &response(
            &near_max,
            0,
            json!({"applied":true,"historyEntriesAdded":1})
        )
    ));
    let other = envelope("query.document.object", json!({}), None);
    assert!(response_matches(&other, &response(&other, 0, json!({}))));
}

const OVERFLOWING_REVISION: u64 = u64::MAX;

#[test]
fn the_shared_result_check_routes_the_item_operations_through_this_module() {
    let payload = commands()[1].clone();
    let good = json!({"applied":true,"historyEntriesAdded":1,"operation":"raster.move","strokeId":"native-raster:3:0"});
    assert!(crate::native_contract::validate_result(
        ITEM,
        "document-A",
        &payload,
        &good
    ));
    for flawed in [
        json!({"applied":true,"historyEntriesAdded":1,"operation":"raster.resize","strokeId":"native-raster:3:0"}),
        json!({"applied":true,"historyEntriesAdded":1,"operation":"raster.move","strokeId":"native-raster:9:9"}),
        json!({"applied":true,"historyEntriesAdded":0,"operation":"raster.move","strokeId":"native-raster:3:0"}),
    ] {
        assert!(
            !crate::native_contract::validate_result(ITEM, "document-A", &payload, &flawed),
            "{flawed}"
        );
    }
    // The request is validated first: a payload outside the closed shape never reaches the receipt check.
    let open = json!({"command":"object.vector.create"});
    assert!(!crate::native_contract::validate_result(
        ITEM,
        "document-A",
        &open,
        &good
    ));
    let receipt = json!({"applied":true,"historyEntriesAdded":1});
    assert!(crate::native_contract::validate_result(
        ATTACH,
        "document-A",
        &attach(),
        &receipt
    ));
    assert!(crate::native_contract::validate_result(
        REMOVE,
        "document-A",
        &remove(),
        &receipt
    ));
    assert!(!crate::native_contract::validate_result(
        ATTACH,
        "document-A",
        &attach(),
        &json!({"applied":true,"historyEntriesAdded":1,"assetId":"asset-x"})
    ));
}

#[path = "native_item_contract_component_tests.rs"]
mod component;

#[test]
fn stored_item_select_pivots_cross_public_bytes_but_text_patches_cannot_reanchor() {
    let admitted=|payload: Value| crate::wire::decode_native_request(&serde_json::to_vec(&envelope(ITEM,payload,Some(7))).unwrap()).is_ok();
    for preset in ["tl","tc","tr","ml","mc","mr","bl","bc","br"] {
        let mut raster=json!({"command":"object.raster.create","target":created(),"item":item()});
        raster["item"]["xformAnchorKey"]=json!(preset);raster["item"]["xformAnchorCustom"]=json!([-10,4.25]);
        raster["item"]["skew"]=json!(89.5);assert!(admitted(raster));
        let mut text=json!({"command":"object.text.create","target":created(),"block":block()});
        text["block"]["xformAnchorKey"]=json!(preset);text["block"]["xformAnchorCustom"]=json!([5,-7]);assert!(admitted(text));
    }
    for (key,value) in [("xformAnchorKey",json!("center")),("xformAnchorKey",json!(1)),
        ("xformAnchorCustom",Value::Null),("xformAnchorCustom",json!([1])),("xformAnchorCustom",json!([1,"2"])),
        ("skew",json!(-90)),("skew",json!(90))] {
        for (command,member,body) in [("object.raster.create","item",item()),("object.text.create","block",block())] {
            let mut payload=json!({"command":command,"target":created()});payload[member]=body;payload[member][key]=value.clone();
            assert!(!admitted(payload),"{command}: {key}");
        }
    }
    for (key,value) in [("xformAnchorKey",json!("mc")),("xformAnchorCustom",json!([1,2]))] {
        let mut payload=json!({"command":"object.text.edit","stableTarget":stable(),"patch":{"text":"content only"}});
        payload["patch"][key]=value;assert!(!admitted(payload),"a text patch cannot replace {key}");
    }
}
