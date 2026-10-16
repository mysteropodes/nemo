//! Literal metadata and authored-fact oracles for the private snapshot provider.
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::request_receipts::DispatchErrorCode;
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};

fn corpus() -> Value {
    serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-property-metadata-v1/cases.json"
    ))
    .unwrap()
}
fn document(value: &Value) -> crate::object_document::ObjectDocument {
    decode_storage_project(value.to_string().as_bytes()).unwrap()
}
fn identity(pin: &ObjectSnapshot, key: &str) -> Value {
    match key {
        "instanceId" => json!(pin.instance_id()),
        "documentId" => json!(pin.document_id()),
        "atRevision" => json!(pin.content_revision()),
        "documentSnapshotId" => json!(pin.snapshot_id()),
        _ => unreachable!(),
    }
}
// Bind only the four runtime identity fields. Stable targets and all descriptors,
// cursors, availability, defaults, ordering and errors remain literal oracles.
fn bind(mut value: Value, fixed: &Value, pin: &ObjectSnapshot) -> Value {
    for key in [
        "instanceId",
        "documentId",
        "atRevision",
        "documentSnapshotId",
    ] {
        let actual = identity(pin, key);
        if value[key] == fixed[key] {
            value[key] = actual;
        } else if value[key] == actual {
            // A deliberately wrong literal document ID may coincide with the
            // process-wide monotonically generated real ID in parallel tests.
            // Preserve the intended mismatch without controlling runtime IDs.
            assert_ne!(key, "atRevision");
            value[key] = json!(format!("fixture-wrong-{key}"));
        }
    }
    value
}
fn bind_raw(raw: &str, fixed: &Value, pin: &ObjectSnapshot) -> String {
    // Preserve the original duplicate-bearing JSON bytes. Do not parse into
    // Value and reserialize, which would silently remove the duplicate oracle.
    assert_eq!(pin.content_revision(), 0);
    let mut bytes = raw.to_owned();
    for key in ["instanceId", "documentId", "documentSnapshotId"] {
        let literal = serde_json::to_string(&fixed[key]).unwrap();
        let actual = serde_json::to_string(&identity(pin, key)).unwrap();
        bytes = bytes.replace(&literal, &actual);
    }
    bytes
}
fn expected_error(name: &str) -> DispatchErrorCode {
    match name {
        "invalid_request" => DispatchErrorCode::InvalidRequest,
        "not_found" => DispatchErrorCode::NotFound,
        "stale_revision" => DispatchErrorCode::StaleRevision,
        "wrong_document" => DispatchErrorCode::WrongDocument,
        "unavailable" => DispatchErrorCode::Unavailable,
        _ => panic!("unrecognized literal error {name}"),
    }
}
fn literal_case(index: usize) {
    let fixed = corpus();
    assert_eq!(fixed["cases"].as_array().unwrap().len(), 33);
    let case = &fixed["cases"][index];
    let name = case["name"].as_str().unwrap();
    let input = case["documentVariant"]
        .as_str()
        .map_or(&fixed["document"], |variant| {
            &fixed["documentVariants"][variant]
        });
    let pin = ObjectSnapshot::new("d35-fixture", document(input)).unwrap();
    let baseline = encode_storage_project(pin.document()).unwrap();
    let canonical = &fixed["cases"][0]["request"];
    let raw = if let Some(bytes) = case["rawRequest"].as_str() {
        bind_raw(bytes, canonical, &pin)
    } else {
        bind(case["request"].clone(), canonical, &pin).to_string()
    };
    let actual = pin.query_property_metadata_json(raw.as_bytes());
    if let Some(error) = case["expectedError"].as_str() {
        assert_eq!(actual, Err(expected_error(error)), "literal case: {name}");
    } else {
        let result = actual.unwrap_or_else(|e| panic!("literal case {name}: {e:?}"));
        assert_eq!(
            result,
            bind(case["expectedResult"].clone(), canonical, &pin),
            "literal case: {name}"
        );
        assert!(
            serde_json::to_vec(&result).unwrap().len() <= 3072,
            "literal case: {name}"
        );
    }
    assert_eq!(
        encode_storage_project(pin.document()).unwrap(),
        baseline,
        "read mutated {name}"
    );
    assert_eq!(pin.content_revision(), 0);
}
macro_rules! literal_cases {
    ($($name:ident => $index:expr),* $(,)?) => {$(
        #[test] fn $name() { literal_case($index); }
    )*};
}
literal_cases! {
    targets_small_first_page => 0,
    targets_deduplicate_authored_and_exclude_reference_only => 1,
    exact_terminal_page => 2,
    base_transform_control_order => 3,
    control_defaults_and_overrides => 4,
    absent_holder_stays_absent => 5,
    solid_fill_inherits_paint_default => 6,
    stored_optional_does_not_activate_three_d => 7,
    unknown_layer => 8,
    unknown_context => 9,
    reference_only_is_not_a_property_target => 10,
    missing_stored_property => 11,
    stale_snapshot => 12,
    wrong_document => 13,
    zero_limit => 14,
    over_limit => 15,
    past_end_offset => 16,
    frame_is_not_a_stable_target_member => 17,
    raw_duplicate_view_limit => 18,
    byte_budget_shrinks_page_and_advances_cursor => 19,
    single_unrepresentable_identity_refuses => 20,
    positional_targets_view_refuses => 21,
    positional_properties_view_refuses => 22,
    positional_stored_property_view_refuses => 23,
    single_unrepresentable_control_name_refuses => 24,
    missing_record_has_zero_authored_facts => 25,
    empty_track_has_zero_authored_keys => 26,
    one_authored_key_is_counted => 27,
    multiple_authored_keys_are_counted => 28,
    explicit_default_static_is_still_authored => 29,
    enabled_empty_code_is_still_expression_enabled => 30,
    disabled_nonempty_code_is_not_expression_enabled => 31,
    separate_order_owner_never_invents_false_facts => 32,
}
#[test]
fn identity_safe_integer_and_original_byte_guards() {
    let fixed = corpus();
    let pin = ObjectSnapshot::new("d35-fixture", document(&fixed["document"])).unwrap();
    let request = bind(
        fixed["cases"][0]["request"].clone(),
        &fixed["cases"][0]["request"],
        &pin,
    );
    let before = encode_storage_project(pin.document()).unwrap();
    let mut wrong = request.clone();
    wrong["instanceId"] = json!("foreign-instance");
    assert_eq!(
        pin.query_property_metadata_json(wrong.to_string().as_bytes()),
        Err(DispatchErrorCode::WrongInstance)
    );
    let mut unsafe_revision = request.clone();
    unsafe_revision["atRevision"] = json!(9_007_199_254_740_992_u64);
    assert_eq!(
        pin.query_property_metadata_json(unsafe_revision.to_string().as_bytes()),
        Err(DispatchErrorCode::InvalidRequest)
    );
    let mut wrong_revision = request.clone();
    wrong_revision["atRevision"] = json!(1);
    assert_eq!(
        pin.query_property_metadata_json(wrong_revision.to_string().as_bytes()),
        Err(DispatchErrorCode::StaleRevision)
    );
    let duplicate_tag = request.to_string().replace(
        "\"kind\":\"targets\"",
        "\"kind\":\"targets\",\"kind\":\"targets\"",
    );
    assert_eq!(
        pin.query_property_metadata_json(duplicate_tag.as_bytes()),
        Err(DispatchErrorCode::InvalidRequest)
    );
    let duplicate_identity = request.to_string().replacen("{", "{\"atRevision\":0,", 1);
    assert_eq!(
        pin.query_property_metadata_json(duplicate_identity.as_bytes()),
        Err(DispatchErrorCode::InvalidRequest)
    );
    let oversized = format!("{}{}", request, " ".repeat(4097));
    assert_eq!(
        pin.query_property_metadata_json(oversized.as_bytes()),
        Err(DispatchErrorCode::InvalidRequest)
    );
    assert_eq!(encode_storage_project(pin.document()).unwrap(), before);
}
#[cfg(feature = "history")]
#[test]
fn retained_metadata_pins_never_reacquire_head_or_change_real_history() {
    use crate::history::NativeObjectHistory;
    let fixed = corpus();
    let canonical = &fixed["cases"][0]["request"];
    let mut owner = NativeObjectHistory::new("d35-fixture", document(&fixed["document"])).unwrap();
    let zero = owner.acquire_snapshot(0).unwrap();
    let before = encode_storage_project(zero.document()).unwrap();
    let request = bind(fixed["cases"][4]["request"].clone(), canonical, &zero);
    let expected = bind(
        fixed["cases"][4]["expectedResult"].clone(),
        canonical,
        &zero,
    );
    assert_eq!(
        zero.query_property_metadata_json(request.to_string().as_bytes()),
        Ok(expected.clone())
    );
    assert_eq!(owner.history_depths(), (0, 0));
    let edit = json!({"command":"object.fill.set","stableTarget":fixed["document"]["objects"][0]["target"],
        "fill":{"kind":"solid","r":0.8,"g":0.1,"b":0.3,"a":1}});
    let prepared = owner
        .prepare_fill_json(edit.to_string().as_bytes())
        .unwrap();
    assert_eq!(owner.commit_fill(prepared), Ok(1));
    assert_eq!(owner.history_depths(), (1, 0));
    let one = owner.acquire_snapshot(1).unwrap();
    let after_edit = encode_storage_project(one.document()).unwrap();
    assert_ne!(after_edit, before);
    assert_eq!(
        one.query_property_metadata_json(request.to_string().as_bytes()),
        Err(DispatchErrorCode::StaleRevision)
    );
    let head_request = bind(fixed["cases"][4]["request"].clone(), canonical, &one);
    assert_eq!(
        one.query_property_metadata_json(head_request.to_string().as_bytes()),
        Ok(bind(
            fixed["cases"][4]["expectedResult"].clone(),
            canonical,
            &one
        ))
    );
    assert_eq!(owner.content_revision(), 1);
    assert_eq!(owner.history_depths(), (1, 0));
    assert_eq!(
        zero.query_property_metadata_json(request.to_string().as_bytes()),
        Ok(expected.clone())
    );
    assert_eq!(encode_storage_project(zero.document()).unwrap(), before);
    assert_eq!(owner.undo(1), Ok(2));
    assert_eq!(owner.history_depths(), (0, 1));
    let two = owner.acquire_snapshot(2).unwrap();
    assert_eq!(encode_storage_project(two.document()).unwrap(), before);
    assert_eq!(
        two.query_property_metadata_json(
            bind(fixed["cases"][4]["request"].clone(), canonical, &two)
                .to_string()
                .as_bytes()
        ),
        Ok(bind(
            fixed["cases"][4]["expectedResult"].clone(),
            canonical,
            &two
        ))
    );
    assert_eq!(owner.history_depths(), (0, 1));
    assert_eq!(owner.redo(2), Ok(3));
    let three = owner.acquire_snapshot(3).unwrap();
    assert_eq!(
        encode_storage_project(three.document()).unwrap(),
        after_edit
    );
    assert_eq!(owner.history_depths(), (1, 0));
    let final_request = bind(fixed["cases"][4]["request"].clone(), canonical, &three);
    assert_eq!(
        three.query_property_metadata_json(final_request.to_string().as_bytes()),
        Ok(bind(
            fixed["cases"][4]["expectedResult"].clone(),
            canonical,
            &three
        ))
    );
    assert_eq!(
        zero.query_property_metadata_json(request.to_string().as_bytes()),
        Ok(expected)
    );
    assert_eq!(encode_storage_project(zero.document()).unwrap(), before);
    assert_eq!(owner.content_revision(), 3);
    assert_eq!(owner.history_depths(), (1, 0));
}

#[test]
fn raw_control_name_survives_admission_and_descriptor_without_normalization() {
    let fixed = corpus();
    let mut source = fixed["document"].clone();
    let raw_name = "\u{FEFF}  Amount Ω\t ";
    source["propertyHolders"][0]["controls"][0]["name"] = json!(raw_name);
    let pin = ObjectSnapshot::new("d35-fixture", document(&source)).unwrap();
    let canonical = &fixed["cases"][0]["request"];
    let request = bind(
        json!({"instanceId":canonical["instanceId"],"documentId":canonical["documentId"],
        "atRevision":canonical["atRevision"],"documentSnapshotId":canonical["documentSnapshotId"],
        "view":{"kind":"properties","target":{"contextId":"scene-root","layerUid":"L0"},"offset":6,"limit":1}}),
        canonical,
        &pin,
    );
    let result = pin
        .query_property_metadata_json(request.to_string().as_bytes())
        .unwrap();
    assert_eq!(result["items"][0]["property"], "xc_number");
    assert_eq!(result["items"][0]["controlName"], raw_name);
    assert_eq!(result["items"][0]["defaultValue"], json!([0]));
    assert_eq!(serde_json::to_value(pin.document()).unwrap(), source);
}

#[test]
fn expanded_metadata_pages_keep_every_literal_row_once_with_advancing_cursors() {
    let fixed = corpus();
    let pin = ObjectSnapshot::new("d35-fixture", document(&fixed["document"])).unwrap();
    let before = encode_storage_project(pin.document()).unwrap();
    let canonical = &fixed["cases"][0]["request"];
    let mut rows = Vec::new();
    let mut offset = 0_u64;
    let mut terminal = false;
    for _ in 0..11 {
        let mut request = canonical.clone();
        request["view"] = json!({"kind":"properties","target":{"contextId":"scene-root","layerUid":"L0"},"offset":offset,"limit":8});
        let result = pin
            .query_property_metadata_json(bind(request, canonical, &pin).to_string().as_bytes())
            .unwrap();
        assert!(serde_json::to_vec(&result).unwrap().len() <= 3072);
        assert_eq!(
            result["unavailableCategories"],
            fixed["cases"][0]["expectedResult"]["unavailableCategories"]
        );
        let items = result["items"].as_array().unwrap();
        assert!(!items.is_empty());
        assert!(items.len() <= 8);
        rows.extend(items.iter().cloned());
        if result["nextOffset"].is_null() {
            terminal = true;
            break;
        }
        let next = result["nextOffset"].as_u64().unwrap();
        assert!(next > offset, "a page must advance or terminate");
        assert_eq!(
            next,
            offset + items.len() as u64,
            "no row may be skipped by a continuation"
        );
        offset = next;
    }
    assert!(
        terminal,
        "bounded structural row count must reach a terminal page"
    );
    assert_eq!(Value::Array(rows), fixed["propertyPageOracle"]);
    assert_eq!(encode_storage_project(pin.document()).unwrap(), before);
}

#[test]
fn separate_order_and_remap_storage_stay_refused_without_fabricated_generic_absence() {
    let fixed = corpus();
    let pin = ObjectSnapshot::new("d35-fixture", document(&fixed["document"])).unwrap();
    let canonical = &fixed["cases"][0]["request"];
    let mut request = canonical.clone();
    request["view"] = json!({"kind":"properties","target":{"contextId":"scene-root","layerUid":"L0"},"offset":5,"limit":1});
    let result = pin
        .query_property_metadata_json(bind(request, canonical, &pin).to_string().as_bytes())
        .unwrap();
    assert_eq!(
        result["items"][0]["authored"],
        json!({"availability":"unavailable","reason":"separate-property-owner"})
    );
    for field in ["keyCount", "hasStaticOverride", "expressionEnabled"] {
        assert!(result["items"][0]["authored"].get(field).is_none());
    }
    let before = encode_storage_project(pin.document()).unwrap();
    for property in ["order", "timeRemap"] {
        let mut source = fixed["document"].clone();
        source["propertyHolders"][0]["properties"]
            .as_array_mut()
            .unwrap()
            .push(json!({"property":property,"static":[0]}));
        assert!(decode_storage_project(source.to_string().as_bytes()).is_err());
    }
    assert_eq!(encode_storage_project(pin.document()).unwrap(), before);
}
