//! actual prepared codec and batch-owner failure controls.
use super::*;
use object_codec::{ProjectReferenceState, ProjectWireError};
use serde_json::{json, Value};
use std::{
    cell::{Cell, RefCell},
    sync::{Arc, Weak},
};

fn wire() -> Value {
    let mut wire = object_codec::encoded_wire_test::wire();
    let png = object_codec::encoded_wire_test::PNG;
    wire["formatVersion"] = 2.into();
    wire["assets"] = json!([
        {"id":"media-1","name":"Original plate","kind":"image","source":{"kind":"embedded","mediaType":"image/png","encoding":"base64","data":png}},
        {"id":"media-2","name":"Second plate","kind":"image","source":{"kind":"embedded","mediaType":"image/png","encoding":"base64","data":png}},
        {"id":"font-1","name":"Original font","kind":"font","source":{"kind":"linked","locator":"../Fonts/Mixed CASE.otf"}}
    ]);
    wire["assetInventory"] = json!({"assetIds":["font-1","media-2","media-1"],"placements":[]});
    wire
}
fn limits() -> object_codec::ProjectWireLimits {
    object_codec::encoded_wire_test::limits()
}
fn validate(source: &[u8], png: object_codec::ImageAssetLimits) -> Result<(), ProjectWireError> {
    crate::reference_image_asset::validate_png(source, png)
        .map(|_| ())
        .map_err(ProjectWireError::Png)
}

#[test]
fn batch_reserves_all_then_copies_binds_and_issues_one_identity() {
    let bytes = serde_json::to_vec(&wire()).unwrap();
    let trace = RefCell::new(Vec::new());
    let budget = ObjectSnapshot::encoded_test_budget(4096);
    let result = ObjectSnapshot::managed_project_wire_using(
        "mixed".into(),
        &bytes,
        limits(),
        |n| {
            trace.borrow_mut().push("reserve");
            budget.reserve(n).map_err(|_| "reserve")
        },
        |p| {
            trace.borrow_mut().push("copy");
            p.allocate().map_err(|_| "allocate")
        },
        |p, a| {
            trace.borrow_mut().push("bind");
            p.bind(a).map_err(|_| "bind")
        },
        |instance, project| {
            trace.borrow_mut().push("identity");
            ObjectSnapshot::from_admitted_project(instance, project)
        },
        validate,
    )
    .unwrap();
    assert_eq!(
        *trace.borrow(),
        ["reserve", "reserve", "copy", "copy", "bind", "bind", "identity"]
    );
    let project = result.project().unwrap();
    let inventory = project.asset_inventory().unwrap();
    assert_eq!(
        inventory
            .records()
            .iter()
            .map(|r| r.id().as_str())
            .collect::<Vec<_>>(),
        ["font-1", "media-2", "media-1"]
    );
    assert_eq!(
        inventory.records()[0].source().locator(),
        Some("../Fonts/Mixed CASE.otf")
    );
    let ProjectReferenceState::StillImage(reference) = project.reference_state() else {
        panic!()
    };
    assert!(Arc::ptr_eq(
        reference.encoded_allocation(),
        inventory.records()[2]
            .source()
            .encoded_allocation()
            .unwrap()
    ));
    let cost = reference.encoded_allocation().capacity();
    assert_eq!(budget.used(), Ok(cost * 2));
    let saved = object_codec::encode_project_envelope(project, limits()).unwrap();
    let reopened =
        ObjectSnapshot::from_managed_project_wire("reopen".into(), &saved, limits()).unwrap();
    assert_eq!(reopened.project(), result.project());
    drop((reference, reopened, result));
    assert_eq!(budget.used(), Ok(0));
}

#[test]
fn aggregate_reservation_refusal_precedes_every_copy_and_identity() {
    let cost = base64::decoded_len_estimate(object_codec::encoded_wire_test::PNG.len());
    let budget = ObjectSnapshot::encoded_test_budget(cost);
    let copies = Cell::new(0);
    let result = ObjectSnapshot::managed_project_wire_using(
        "aggregate".into(),
        &serde_json::to_vec(&wire()).unwrap(),
        limits(),
        |n| budget.reserve(n).map_err(|_| "reserve"),
        |_| {
            copies.set(copies.get() + 1);
            unreachable!()
        },
        |_, _| panic!("no bind"),
        |_, _| panic!("no identity"),
        validate,
    );
    assert!(matches!(result, Err(ProjectWireError::Budget)));
    assert_eq!(copies.get(), 0);
    assert_eq!(budget.used(), Ok(0));
}

#[test]
fn allocation_validation_binding_and_identity_failures_release_every_allocation() {
    for stage in ["allocate", "validate", "bind", "identity"] {
        let budget = ObjectSnapshot::encoded_test_budget(4096);
        let copies = Cell::new(0);
        let validations = Cell::new(0);
        let bindings = Cell::new(0);
        let weak = RefCell::new(Vec::<Weak<Vec<u8>>>::new());
        let result = ObjectSnapshot::managed_project_wire_using(
            "failure".into(),
            &serde_json::to_vec(&wire()).unwrap(),
            limits(),
            |n| budget.reserve(n).map_err(|_| "reserve"),
            |p| {
                copies.set(copies.get() + 1);
                if stage == "allocate" && copies.get() == 2 {
                    Err("injected allocation")
                } else {
                    p.allocate().map_err(|_| "allocate")
                }
            },
            |p, a| {
                bindings.set(bindings.get() + 1);
                weak.borrow_mut().push(Arc::downgrade(a));
                if stage == "bind" && bindings.get() == 2 {
                    Err("injected binding")
                } else {
                    p.bind(a).map_err(|_| "bind")
                }
            },
            |_, _| {
                assert_eq!(stage, "identity");
                Err("injected identity")
            },
            |source, png| {
                validations.set(validations.get() + 1);
                if stage == "validate" && validations.get() == 2 {
                    Err(ProjectWireError::Invalid)
                } else {
                    validate(source, png)
                }
            },
        );
        assert!(result.is_err(), "{stage}");
        assert_eq!(budget.used(), Ok(0), "{stage}");
        assert!(
            weak.borrow().iter().all(|w| w.upgrade().is_none()),
            "{stage}"
        );
    }
}

#[test]
fn linked_only_catalog_preserves_exact_values_without_binary_callbacks() {
    let mut value = wire();
    value["viewportReference"] = Value::Null;
    value["assets"] = json!([value["assets"][2].clone()]);
    value["assetInventory"]["assetIds"] = json!(["font-1"]);
    let result = ObjectSnapshot::managed_project_wire_using::<()>(
        "linked".into(),
        &serde_json::to_vec(&value).unwrap(),
        limits(),
        |_| panic!("no reserve"),
        |_| panic!("no copy"),
        |_, _| panic!("no bind"),
        ObjectSnapshot::from_admitted_project,
        |_, _| panic!("no decoder"),
    )
    .unwrap();
    let project = result.project().unwrap();
    let saved = object_codec::encode_project_envelope(project, limits()).unwrap();
    let reopened =
        ObjectSnapshot::from_managed_project_wire("linked-reopen".into(), &saved, limits())
            .unwrap();
    assert_eq!(result.project(), reopened.project());
    assert_eq!(
        project.asset_inventory().unwrap().records()[0]
            .source()
            .locator(),
        Some("../Fonts/Mixed CASE.otf")
    );
}

#[test]
fn duplicate_unknown_missing_unsupported_or_dangling_values_refuse_before_copy() {
    let original = wire();
    let mut cases = Vec::new();
    let mut v = original.clone();
    v["assets"][1]["id"] = "media-1".into();
    cases.push(v);
    let mut v = original.clone();
    v["assetInventory"]["assetIds"] = json!(["media-1", "media-1"]);
    cases.push(v);
    let mut v = original.clone();
    v["assetInventory"]["assetIds"][0] = "missing".into();
    cases.push(v);
    let mut v = original.clone();
    v["assetInventory"]["placements"] = json!([{"id":"active"}]);
    cases.push(v);
    let mut v = original.clone();
    v["assets"][0]["kind"] = "video".into();
    cases.push(v);
    let mut v = original.clone();
    v["assets"][0]["source"]["extra"] = true.into();
    cases.push(v);
    let mut v = original.clone();
    v["assetInventory"] = Value::Null;
    cases.push(v);
    let mut v = original.clone();
    v.as_object_mut().unwrap().remove("assetInventory");
    cases.push(v);
    let mut v = original.clone();
    v["formatVersion"] = 99.into();
    cases.push(v);
    let mut v = original.clone();
    v["assetInventory"]["assetIds"] = json!(["media-1"]);
    cases.push(v);
    for value in cases {
        let result = ObjectSnapshot::managed_project_wire_using::<()>(
            "refused".into(),
            &serde_json::to_vec(&value).unwrap(),
            limits(),
            |_| panic!("no reserve"),
            |_| panic!("no copy"),
            |_, _| panic!("no bind"),
            |_, _| panic!("no identity"),
            |_, _| panic!("no decoder"),
        );
        assert!(result.is_err());
    }
    // Raw duplication must be rejected before JSON values could collapse it.
    let data = serde_json::to_string(&original).unwrap();
    let duplicate = data.replacen(
        "\"formatVersion\":2",
        "\"formatVersion\":2,\"formatVersion\":2",
        1,
    );
    assert!(object_codec::prepare_project_wire(duplicate.as_bytes(), limits()).is_err());
}

#[test]
fn legacy_output_is_literal_and_catalog_is_never_silently_discarded() {
    let data = br#"{"format":"nemo.native-project-document","formatVersion":1,"project":{"canvasW":320,"canvasH":180,"fps":24},"drawing":{"format":"nemo.native-object-document","formatVersion":1,"totalFrames":1,"layers":[{"layerUid":"L"}],"objects":[]},"viewportReference":null,"assets":[]}"#;
    let snapshot =
        ObjectSnapshot::from_managed_project_wire("legacy".into(), data, limits()).unwrap();
    let output =
        object_codec::encode_project_envelope(snapshot.project().unwrap(), limits()).unwrap();
    let expected = std::str::from_utf8(data)
        .unwrap()
        .replace("\"fps\":24", "\"fps\":24.0");
    assert_eq!(output, expected.as_bytes());
    assert!(snapshot.project().unwrap().asset_inventory().is_none());
}

#[test]
fn real_fill_history_undo_redo_and_reopen_preserve_catalog_and_old_pins() {
    let cases: Value = serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    ))
    .unwrap();
    let mut value = wire();
    value["drawing"] = json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":21,"layers":[{"layerUid":"legacy layer:alpha"},{"layerUid":"0"}],"objects":cases["records"]});
    let bytes = serde_json::to_vec(&value).unwrap();
    let managed =
        ObjectSnapshot::from_managed_project_wire("catalog-history".into(), &bytes, limits())
            .unwrap();
    let mut owner = crate::history::NativeObjectHistory::from_admitted_project(
        "catalog-history",
        managed.project().unwrap().clone(),
    )
    .unwrap();
    let old = owner.acquire_snapshot(0).unwrap();
    let before = object_codec::encode_project_envelope(old.project().unwrap(), limits()).unwrap();
    let original_assets = old.project().unwrap().asset_inventory().unwrap().clone();
    let payload = serde_json::to_vec(&cases["command"]["request"]["payload"]).unwrap();
    let prepared = owner.prepare_fill_json(&payload).unwrap();
    assert_eq!(owner.commit_fill(prepared), Ok(1));
    let after = owner.acquire_snapshot(1).unwrap();
    assert_eq!(
        serde_json::to_value(after.document()).unwrap()["objects"][0],
        cases["command"]["afterRecord"]
    );
    assert_eq!(
        after.project().unwrap().asset_inventory(),
        Some(&original_assets)
    );
    for (a, b) in after
        .project()
        .unwrap()
        .asset_inventory()
        .unwrap()
        .records()
        .iter()
        .zip(original_assets.records())
    {
        assert!(Arc::ptr_eq(a, b));
    }
    assert_eq!(owner.undo(1), Ok(2));
    let undone = owner.acquire_snapshot(2).unwrap();
    assert_eq!(
        object_codec::encode_project_envelope(undone.project().unwrap(), limits()).unwrap(),
        before
    );
    assert_eq!(owner.redo(2), Ok(3));
    let redone = owner.acquire_snapshot(3).unwrap();
    let saved = owner
        .serialize_project_file(3, redone.snapshot_id())
        .unwrap();
    let reopened =
        crate::history::NativeObjectHistory::from_project_file("reopen-catalog", &saved).unwrap();
    let reopened = reopened.acquire_snapshot(0).unwrap();
    assert_eq!(reopened.project(), redone.project());
    drop((owner, managed, after, redone, undone));
    assert_eq!(
        object_codec::encode_project_envelope(old.project().unwrap(), limits()).unwrap(),
        before
    );
}

#[test]
fn unwinding_partial_decode_bind_or_identity_releases_the_same_ledger() {
    for stage in ["allocate", "validate", "bind", "identity"] {
        let budget = ObjectSnapshot::encoded_test_budget(4096);
        let copies = Cell::new(0);
        let validations = Cell::new(0);
        let bindings = Cell::new(0);
        let weak = RefCell::new(Vec::<Weak<Vec<u8>>>::new());
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            ObjectSnapshot::managed_project_wire_using(
                "unwind".into(),
                &serde_json::to_vec(&wire()).unwrap(),
                limits(),
                |n| budget.reserve(n).map_err(|_| "reserve"),
                |p| {
                    copies.set(copies.get() + 1);
                    if stage == "allocate" && copies.get() == 2 {
                        panic!("injected allocator unwind");
                    }
                    p.allocate().map_err(|_| "allocate")
                },
                |p, a| {
                    bindings.set(bindings.get() + 1);
                    weak.borrow_mut().push(Arc::downgrade(a));
                    if stage == "bind" && bindings.get() == 2 {
                        panic!("injected binding unwind");
                    }
                    p.bind(a).map_err(|_| "bind")
                },
                |_, _| {
                    assert_eq!(stage, "identity");
                    panic!("injected identity unwind");
                },
                |source, png| {
                    validations.set(validations.get() + 1);
                    if stage == "validate" && validations.get() == 2 {
                        panic!("injected validator unwind");
                    }
                    validate(source, png)
                },
            )
        }));
        assert!(result.is_err(), "{stage}");
        assert_eq!(budget.used(), Ok(0), "{stage}");
        assert!(
            weak.borrow().iter().all(|w| w.upgrade().is_none()),
            "{stage}"
        );
    }
}
