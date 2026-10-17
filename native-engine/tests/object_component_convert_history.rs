//! Typed transactions of the lane-4 setup / convert operations through the one
//! history owner (L04-M): one revision and one undo entry per real change, exact
//! undo / redo bytes, no-ops that publish nothing, sealed candidates and refusals
//! that cannot partially publish.
use super::fixture::{decode, document, layer, s1_document, s2_document};
use super::*;
use crate::commands::{SetupError, SourceRefusal};
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::request_receipts::DispatchErrorCode;
use serde_json::{json, Value};

fn owner(value: &Value) -> NativeObjectHistory {
    NativeObjectHistory::new("component-setup", decode(value)).unwrap()
}
fn bytes(owner: &NativeObjectHistory) -> Vec<u8> {
    let snapshot = owner.acquire_snapshot(owner.content_revision()).unwrap();
    encode_storage_project(snapshot.document()).unwrap()
}
fn value(owner: &NativeObjectHistory) -> Value {
    serde_json::from_slice(&bytes(owner)).unwrap()
}
fn convert(layer_uids: &[&str]) -> Value {
    json!({"command":"object.component.convert","contextId":"scene-root","layerUids":layer_uids})
}
fn prepare(
    owner: &NativeObjectHistory,
    kind: SetupKind,
    payload: &Value,
) -> Result<PreparedComponentSetup, SetupRefusal> {
    owner.prepare_component_setup_json(kind, payload.to_string().as_bytes())
}
fn commit(owner: &mut NativeObjectHistory, kind: SetupKind, payload: &Value) -> u64 {
    let candidate = prepare(owner, kind, payload).unwrap();
    owner.commit_component_setup(candidate).unwrap()
}

#[test]
fn convert_is_one_revision_and_one_undo_entry_with_exact_undo_and_redo_bytes() {
    let cases = [
        (
            s1_document(json!({})),
            vec!["ly_hand"],
            Some("native-component:1:0"),
            None,
        ),
        (
            s2_document(),
            vec!["ly_a", "ly_b", "ly_c"],
            Some("native-component:1:0"),
            Some("native-layer:1:0"),
        ),
    ];
    for (doc, selection, symbol, created) in cases {
        let mut owner = owner(&doc);
        let original = bytes(&owner);
        let before = owner.acquire_snapshot(0).unwrap();
        let candidate = prepare(&owner, SetupKind::Convert, &convert(&selection)).unwrap();
        assert_eq!(candidate.symbol_id(), symbol);
        assert_eq!(candidate.created(), created);
        assert_eq!(bytes(&owner), original, "preparation publishes nothing");
        assert_eq!(owner.history_depths(), (0, 0));
        let planned = encode_storage_project(candidate.document()).unwrap();
        assert_eq!(owner.commit_component_setup(candidate).unwrap(), 1);
        assert_eq!(owner.history_depths(), (1, 0));
        let after = bytes(&owner);
        assert_eq!(after, planned);
        assert_eq!(encode_storage_project(before.document()).unwrap(), original);
        let reopened = decode_storage_project(&after).unwrap();
        assert!(!reopened.component_set().unwrap().components.is_empty());
        owner.undo(1).unwrap();
        assert_eq!(
            bytes(&owner),
            original,
            "the exact prior bytes, member absent again"
        );
        assert_eq!(owner.history_depths(), (0, 1));
        owner.redo(2).unwrap();
        assert_eq!(bytes(&owner), after);
        assert_eq!(owner.history_depths(), (1, 0));
    }
}

#[test]
fn a_chain_of_setups_is_one_undo_entry_each_and_unwinds_in_order() {
    let mut owner = owner(&s1_document(json!({})));
    let mut states = vec![bytes(&owner)];
    let symbol = "native-component:1:0";
    let steps: [(SetupKind, Value); 4] = [
        (SetupKind::Convert, convert(&["ly_hand"])),
        (
            SetupKind::Expose,
            json!({"command":"object.component.expose","symbolId":symbol,"key":"ep_mte91adt_cx4d",
                "targetStrokeId":"sa","targetField":"fillColor","label":"Fill"}),
        ),
        (
            SetupKind::Place,
            json!({"command":"object.component.place","symbolId":symbol,"name":"Second"}),
        ),
        (
            SetupKind::DuplicatorSet,
            json!({"command":"object.duplicator.set","layerUid":"ly_other","enabled":true,"seed":42}),
        ),
    ];
    for (index, (kind, payload)) in steps.iter().enumerate() {
        assert_eq!(commit(&mut owner, *kind, payload), index as u64 + 1);
        states.push(bytes(&owner));
    }
    assert_eq!(owner.history_depths(), (4, 0));
    let wire = value(&owner);
    assert_eq!(wire["layers"][2]["layerUid"], "native-layer:3:0");
    assert_eq!(
        wire["componentTables"]["components"]["components"][0]["exposedProps"][0]["key"],
        "ep_mte91adt_cx4d"
    );
    assert_eq!(
        wire["componentTables"]["duplicators"]["duplicators"][0]["duplicator"]["seed"],
        42
    );
    for step in (0..4).rev() {
        let current = owner.content_revision();
        owner.undo(current).unwrap();
        assert_eq!(bytes(&owner), states[step]);
    }
    for step in 1..=4 {
        let current = owner.content_revision();
        owner.redo(current).unwrap();
        assert_eq!(bytes(&owner), states[step]);
    }
}

#[test]
fn a_noop_setup_keeps_presence_bytes_history_depth_and_redo() {
    let mut owner = owner(&s1_document(json!({})));
    let enable =
        json!({"command":"object.duplicator.set","layerUid":"ly_other","enabled":true,"seed":9});
    let disable = json!({"command":"object.duplicator.set","layerUid":"ly_other","enabled":false});
    commit(&mut owner, SetupKind::DuplicatorSet, &enable);
    owner.undo(1).unwrap();
    assert_eq!(owner.history_depths(), (0, 1));
    let (revision, saved) = (owner.content_revision(), bytes(&owner));
    let candidate = prepare(&owner, SetupKind::DuplicatorSet, &disable).unwrap();
    assert!(!candidate.changed());
    assert_eq!(owner.commit_component_setup(candidate).unwrap(), revision);
    assert_eq!(bytes(&owner), saved);
    assert_eq!(owner.history_depths(), (0, 1), "redo survives a no-op");
    assert!(
        value(&owner).get("componentTables").is_none(),
        "absent stays absent"
    );
    // The same exposed entry twice, and the same path value twice, are no-ops too.
    commit(&mut owner, SetupKind::Convert, &convert(&["ly_hand"]));
    let expose = json!({"command":"object.component.expose","symbolId":"native-component:3:0","key":"ep_a",
        "targetStrokeId":"sa","targetField":"opacity","label":"Alpha","default":100});
    commit(&mut owner, SetupKind::Expose, &expose);
    let depths = owner.history_depths();
    let revision = owner.content_revision();
    assert_eq!(commit(&mut owner, SetupKind::Expose, &expose), revision);
    assert_eq!(owner.history_depths(), depths);
}

#[test]
fn a_candidate_is_sealed_to_its_origin_and_commits_once() {
    let doc = s1_document(json!({}));
    let mut source = owner(&doc);
    let candidate = prepare(&source, SetupKind::Convert, &convert(&["ly_hand"])).unwrap();
    let mut other = owner(&doc);
    let before = bytes(&other);
    assert_eq!(
        other.commit_component_setup(candidate.clone()).unwrap_err(),
        DispatchErrorCode::WrongDocument
    );
    let mut foreign = NativeObjectHistory::new("foreign", decode(&doc)).unwrap();
    assert_eq!(
        foreign
            .commit_component_setup(candidate.clone())
            .unwrap_err(),
        DispatchErrorCode::WrongInstance
    );
    assert_eq!((bytes(&other), other.history_depths()), (before, (0, 0)));
    source.commit_component_setup(candidate.clone()).unwrap();
    let after = bytes(&source);
    assert_eq!(
        source.commit_component_setup(candidate).unwrap_err(),
        DispatchErrorCode::StaleRevision
    );
    assert_eq!((bytes(&source), source.history_depths()), (after, (1, 0)));
}

#[test]
fn a_dropped_candidate_leaves_revision_depths_bytes_and_redo_untouched() {
    let mut owner = owner(&s1_document(json!({})));
    let saved = bytes(&owner);
    drop(prepare(&owner, SetupKind::Convert, &convert(&["ly_hand"])).unwrap());
    assert_eq!(
        (
            bytes(&owner),
            owner.content_revision(),
            owner.history_depths()
        ),
        (saved, 0, (0, 0))
    );
    assert_eq!(
        commit(&mut owner, SetupKind::Convert, &convert(&["ly_hand"])),
        1
    );
}

#[test]
fn bad_sources_return_the_typed_reason_and_the_baseline_text() {
    let mut owner = owner(&s1_document(json!({})));
    commit(&mut owner, SetupKind::Convert, &convert(&["ly_hand"]));
    let (saved, depths) = (bytes(&owner), owner.history_depths());
    let refusal = prepare(&owner, SetupKind::Convert, &convert(&["ly_hand"])).unwrap_err();
    assert_eq!(refusal.code, DispatchErrorCode::InvalidRequest);
    assert_eq!(refusal.message, "Impossible de convertir : un composant");
    assert_eq!(
        refusal.error,
        Some(SetupError::BadSource {
            layer_uid: "ly_hand".into(),
            refusal: SourceRefusal::Component
        })
    );
    assert_eq!((bytes(&owner), owner.history_depths()), (saved, depths));
}

#[test]
fn malformed_stale_and_unresolvable_input_cannot_partially_publish() {
    let owner = owner(&s1_document(json!({})));
    let saved = bytes(&owner);
    let ok = r#"{"command":"object.component.convert","contextId":"scene-root","layerUids":["ly_hand"]}"#;
    let cases = [
        (
            r#"{"command":"object.component.convert","contextId":"scene-root","layerUids":["ly_hand"],"layerUids":["ly_other"]}"#,
            DispatchErrorCode::InvalidRequest,
        ),
        (
            r#"{"command":"object.component.convert","contextId":"scene-root","layerUids":["ly_hand"],"symbolId":"chosen"}"#,
            DispatchErrorCode::InvalidRequest,
        ),
        (
            r#"{"command":"object.component.convert","contextId":"scene-root","layerUids":[]}"#,
            DispatchErrorCode::InvalidRequest,
        ),
        (
            r#"{"command":"other","contextId":"scene-root","layerUids":["ly_hand"]}"#,
            DispatchErrorCode::InvalidRequest,
        ),
        (
            r#"{"command":"object.component.convert","contextId":"scene-root","layerUids":[""]}"#,
            DispatchErrorCode::InvalidRequest,
        ),
        (
            r#"["object.component.convert"]"#,
            DispatchErrorCode::InvalidRequest,
        ),
        (
            r#"{"command":"object.component.convert","contextId":"sym_open","layerUids":["ly_hand"]}"#,
            DispatchErrorCode::Unavailable,
        ),
        (
            r#"{"command":"object.component.convert","contextId":"scene-root","layerUids":["nope"]}"#,
            DispatchErrorCode::NotFound,
        ),
    ];
    for (raw, code) in cases {
        let refusal = owner
            .prepare_component_setup_json(SetupKind::Convert, raw.as_bytes())
            .unwrap_err();
        assert_eq!(refusal.code, code, "{raw}");
        assert_eq!(
            (bytes(&owner), owner.history_depths()),
            (saved.clone(), (0, 0)),
            "{raw}"
        );
    }
    let mut padded = ok.to_string();
    padded.push_str(&" ".repeat(4096 - padded.len()));
    assert!(owner
        .prepare_component_setup_json(SetupKind::Convert, padded.as_bytes())
        .is_ok());
    padded.push(' ');
    assert_eq!(
        owner
            .prepare_component_setup_json(SetupKind::Convert, padded.as_bytes())
            .unwrap_err()
            .code,
        DispatchErrorCode::InvalidRequest
    );
    let place = r#"{"command":"object.component.place","symbolId":"nope","name":"x"}"#;
    assert_eq!(
        owner
            .prepare_component_setup_json(SetupKind::Place, place.as_bytes())
            .unwrap_err()
            .code,
        DispatchErrorCode::NotFound
    );
    assert_eq!(bytes(&owner), saved);
}

#[test]
fn an_exposed_key_the_codec_does_not_admit_is_refused_before_publication() {
    let mut owner = owner(&s1_document(json!({})));
    commit(&mut owner, SetupKind::Convert, &convert(&["ly_hand"]));
    let saved = bytes(&owner);
    let symbol = "native-component:1:0";
    let bad = [
        json!({"command":"object.component.expose","symbolId":symbol,"key":"nokey","targetStrokeId":"sa","targetField":"opacity","label":"x"}),
        json!({"command":"object.component.expose","symbolId":symbol,"key":"ep_k","targetStrokeId":"missing","targetField":"opacity","label":"x"}),
    ];
    for payload in bad {
        let refusal = prepare(&owner, SetupKind::Expose, &payload).unwrap_err();
        assert_eq!(refusal.code, DispatchErrorCode::InvalidRequest);
        assert_eq!(
            (bytes(&owner), owner.history_depths()),
            (saved.clone(), (1, 0))
        );
    }
}

#[test]
fn animating_a_flat_layer_never_converts_it() {
    let mut owner = owner(&s1_document(json!({})));
    let key = json!({"command":"object.property.edit","target":{"contextId":"scene-root","layerUid":"ly_hand"},
        "property":"position","action":{"kind":"setKey","frame":4.0,"values":[10.0,20.0],"curvePoints":[]}});
    let candidate = owner
        .prepare_property_edit_json(key.to_string().as_bytes())
        .unwrap();
    assert_eq!(owner.commit_property_edit(candidate).unwrap(), 1);
    let animated = value(&owner);
    assert_eq!(
        animated["propertyHolders"][0]["target"]["layerUid"],
        "ly_hand"
    );
    assert!(
        animated.get("componentTables").is_none(),
        "no component record appears"
    );
    assert_eq!(animated["layers"].as_array().unwrap().len(), 2);
    // Conversion only through the explicit request; the instance keeps its own motion.
    commit(&mut owner, SetupKind::Convert, &convert(&["ly_hand"]));
    let converted = value(&owner);
    assert_eq!(
        converted["componentTables"]["components"]["instances"][0]["layerUid"],
        "ly_hand"
    );
    assert_eq!(converted["propertyHolders"], animated["propertyHolders"]);
}

#[test]
fn a_layer_removed_by_the_merge_comes_back_with_the_undo() {
    let mut owner = owner(&document(
        &[
            layer("a", "A", &[(0, "a1")]),
            layer("b", "B", &[(0, "b1")]),
            layer("c", "C", &[(0, "c1")]),
        ],
        json!({}),
    ));
    let original = value(&owner);
    commit(&mut owner, SetupKind::Convert, &convert(&["a", "b"]));
    let merged = value(&owner);
    let uids: Vec<&str> = merged["layers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["layerUid"].as_str().unwrap())
        .collect();
    assert_eq!(uids, ["native-layer:1:0", "c"]);
    owner.undo(1).unwrap();
    assert_eq!(value(&owner), original);
}

fn pool_value() -> Value {
    document(
        &[
            layer("ly_a", "A", &[(0, "a1")]),
            layer("ly_b", "B", &[(0, "b1")]),
            layer("ly_p", "Path", &[(0, "p1")]),
        ],
        json!({}),
    )
}

#[test]
fn the_duplicator_wire_payloads_are_strict_bounded_and_one_undo_entry_each() {
    let mut owner = owner(&pool_value());
    let original = bytes(&owner);
    let objects =
        json!({"command":"object.duplicator.objects","layerUids":["ly_a","ly_b"],"seed":7});
    let candidate = prepare(&owner, SetupKind::DuplicatorObjects, &objects).unwrap();
    assert!(candidate.changed());
    assert_eq!(candidate.created(), Some("native-layer:1:0"));
    assert_eq!(bytes(&owner), original, "preparation publishes nothing");
    assert_eq!(owner.commit_component_setup(candidate).unwrap(), 1);
    let host = value(&owner)["layers"][3]["layerUid"].clone();
    assert_eq!(host, "native-layer:1:0");
    // The host that was just created takes a path layer through the second wire kind.
    let path = json!({"command":"object.duplicator.path","layerUid":"native-layer:1:0","pathLayerUid":"ly_p"});
    assert_eq!(commit(&mut owner, SetupKind::DuplicatorPath, &path), 2);
    assert_eq!(owner.history_depths(), (2, 0));
    for _ in 0..2 {
        let current = owner.content_revision();
        owner.undo(current).unwrap();
    }
    assert_eq!(bytes(&owner), original);
    // Malformed variants of both kinds publish nothing and name the vocabulary code.
    let long = "x".repeat(129);
    let bad = [
        (
            SetupKind::DuplicatorObjects,
            json!({"command":"object.duplicator.objects","layerUids":["ly_a","ly_b"]}),
            DispatchErrorCode::InvalidRequest,
        ),
        (
            SetupKind::DuplicatorObjects,
            json!({"command":"object.duplicator.objects","layerUids":[],"seed":7}),
            DispatchErrorCode::InvalidRequest,
        ),
        (
            SetupKind::DuplicatorObjects,
            json!({"command":"object.duplicator.objects","layerUids":["ly_a","ly_b"],"seed":-1}),
            DispatchErrorCode::InvalidRequest,
        ),
        (
            SetupKind::DuplicatorObjects,
            json!({"command":"object.duplicator.objects","layerUids":["ly_a"],"seed":7}),
            DispatchErrorCode::InvalidRequest,
        ),
        (
            SetupKind::DuplicatorPath,
            json!({"command":"object.duplicator.path","layerUid":"ly_a","pathLayerUid":long}),
            DispatchErrorCode::InvalidRequest,
        ),
        (
            SetupKind::DuplicatorPath,
            json!({"command":"object.duplicator.path","layerUid":"ly_a","pathLayerUid":null,"extra":1}),
            DispatchErrorCode::InvalidRequest,
        ),
        (
            SetupKind::DuplicatorPath,
            json!({"command":"object.duplicator.path","layerUid":"ly_a","pathLayerUid":null}),
            DispatchErrorCode::NotFound,
        ),
    ];
    for (kind, payload, code) in bad {
        let refusal = prepare(&owner, kind, &payload).unwrap_err();
        assert_eq!(refusal.code, code, "{payload}");
        assert_eq!(
            (bytes(&owner), owner.history_depths()),
            (original.clone(), (0, 2))
        );
    }
}

#[test]
fn a_path_host_takes_and_clears_its_path_layer_through_the_wire() {
    let mut doc = pool_value();
    doc["componentTables"] = json!({"duplicators":{"format":"nemo.native-duplicator-set",
        "formatVersion":1,"effectorLayers":[],"duplicators":[
            {"contextId":"scene-root","layerUid":"ly_a","duplicator":{"mode":"path"}}]}});
    let mut owner = owner(&doc);
    let set = json!({"command":"object.duplicator.path","layerUid":"ly_a","pathLayerUid":"ly_p"});
    assert_eq!(commit(&mut owner, SetupKind::DuplicatorPath, &set), 1);
    assert_eq!(
        value(&owner)["componentTables"]["duplicators"]["duplicators"][0]["duplicator"]
            ["pathLayerUid"],
        "ly_p"
    );
    let again = prepare(&owner, SetupKind::DuplicatorPath, &set).unwrap();
    assert!(!again.changed(), "the same path value twice is a no-op");
    let clear = json!({"command":"object.duplicator.path","layerUid":"ly_a","pathLayerUid":null});
    assert_eq!(commit(&mut owner, SetupKind::DuplicatorPath, &clear), 2);
    assert_eq!(owner.history_depths(), (2, 0));
    for _ in 0..2 {
        let current = owner.content_revision();
        owner.undo(current).unwrap();
    }
    assert!(
        value(&owner)["componentTables"]["duplicators"]["duplicators"][0]["duplicator"]
            .get("pathLayerUid")
            .is_none()
    );
}
