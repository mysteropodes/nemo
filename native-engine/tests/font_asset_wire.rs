//! PRIVATE paired font/PNG owner tests. Authored, UNRUN; no installed ports.
use super::*;
use base64::Engine;
use object_codec::{FontAssetLimits, ProjectWireError, ProjectWireLimits};
use serde_json::{json, Value};
use std::{
    cell::{Cell, RefCell},
    sync::{Arc, Weak},
};

const FONT: &[u8] = include_bytes!("../../src/fonts/Roboto-Regular.ttf");
fn limits() -> ProjectWireLimits {
    ProjectWireLimits::new(2 * 1024 * 1024, 8 * 1024)
        .unwrap()
        .with_png_limits(1024, 4096, 4096, 16, 16)
        .unwrap()
        .with_font_limits(FontAssetLimits::new(1024 * 1024, 256).unwrap())
}
fn wire() -> Value {
    json!({"format":"nemo.native-project-document","formatVersion":2,"project":{"canvasW":320,"canvasH":180,"fps":24},
    "drawing":{"format":"nemo.native-object-document","formatVersion":1,"totalFrames":1,"layers":[{"layerUid":"L"}],"objects":[]},"viewportReference":null,
    "assets":[{"id":"native-font-1","name":"Roboto-Regular","kind":"font","source":{"kind":"embedded","mediaType":"font/ttf","encoding":"base64","data":base64::engine::general_purpose::STANDARD.encode(FONT)}}],
    "assetInventory":{"assetIds":["native-font-1"],"placements":[]}})
}
fn mixed() -> Value {
    let mut value = wire();
    let png = object_codec::encoded_wire_test::PNG;
    value["assets"].as_array_mut().unwrap().push(json!({"id":"plate","name":"Plate","kind":"image","source":{"kind":"embedded","mediaType":"image/png","encoding":"base64","data":png}}));
    value["assetInventory"]["assetIds"] = json!(["plate", "native-font-1"]);
    value["viewportReference"] =
        object_codec::encoded_wire_test::wire()["viewportReference"].clone();
    value["viewportReference"]["assetId"] = "plate".into();
    value
}
fn validate(source: &[u8], png: object_codec::ImageAssetLimits) -> Result<(), ProjectWireError> {
    crate::reference_image_asset::validate_png(source, png)
        .map(|_| ())
        .map_err(ProjectWireError::Png)
}

#[test]
fn chosen_native_font_id_and_exact_bytes_survive_real_project_save_reopen() {
    let mut input = wire();
    input["project"]["canvasBg"] = "#aBcD1234".into();
    input["project"]["waIn"] = 1.25.into();
    input["project"]["waOut"] = 0.5.into();
    let original = ObjectSnapshot::from_managed_project_wire(
        "font-open".into(),
        &serde_json::to_vec(&input).unwrap(),
        limits(),
    )
    .unwrap();
    let project = original.project().unwrap();
    let record = &project.asset_inventory().unwrap().records()[0];
    assert_eq!(record.id().as_str(), "native-font-1");
    assert_eq!(record.name(), "Roboto-Regular");
    assert_eq!(
        record.source().encoded_allocation().unwrap().as_slice(),
        FONT
    );
    assert!(record.source().has_font_admission());
    let saved = object_codec::encode_project_envelope(project, limits()).unwrap();
    let output: Value = serde_json::from_slice(&saved).unwrap();
    for key in ["canvasBg", "waIn", "waOut"] {
        assert_eq!(output["project"][key], input["project"][key]);
    }
    assert_eq!(output["assets"][0]["source"]["kind"], "embedded");
    assert!(output["assets"][0]["source"].get("admitted").is_none());
    let reopened =
        ObjectSnapshot::from_managed_project_wire("font-reopen".into(), &saved, limits()).unwrap();
    assert_eq!(reopened.project(), original.project());
    assert_eq!(
        object_codec::encode_project_envelope(reopened.project().unwrap(), limits()).unwrap(),
        saved
    );
}

#[test]
fn font_limits_refuse_before_copy_and_unconfigured_writer_cannot_drop_the_font() {
    let bounded = limits().with_font_limits(FontAssetLimits::new(FONT.len() - 1, 256).unwrap());
    let result = ObjectSnapshot::managed_project_wire_using::<()>(
        "font-limit".into(),
        &serde_json::to_vec(&wire()).unwrap(),
        bounded,
        |_| panic!("no reserve"),
        |_| panic!("no copy"),
        |_, _| panic!("no bind"),
        |_, _| panic!("no identity"),
        validate,
    );
    assert!(matches!(result, Err(ProjectWireError::Budget)));
    let original = ObjectSnapshot::from_managed_project_wire(
        "writer".into(),
        &serde_json::to_vec(&wire()).unwrap(),
        limits(),
    )
    .unwrap();
    let absent = ProjectWireLimits::new(2 * 1024 * 1024, 8 * 1024).unwrap();
    assert!(matches!(
        object_codec::encode_project_envelope(original.project().unwrap(), absent),
        Err(ProjectWireError::Unsupported)
    ));
    let saved =
        object_codec::encode_project_envelope(original.project().unwrap(), limits()).unwrap();
    let value: Value = serde_json::from_slice(&saved).unwrap();
    for key in ["canvasBg", "waIn", "waOut"] {
        assert!(value["project"].get(key).is_none());
    }
}

#[test]
fn generic_mime_png_for_font_and_font_for_viewport_never_publish() {
    let mut png = wire();
    png["assets"][0]["source"]["mediaType"] = "image/png".into();
    let mut image = wire();
    image["assets"][0]["kind"] = "image".into();
    let mut viewport = wire();
    viewport["viewportReference"] = json!({"kind":"still-image","assetId":"native-font-1","name":"font.ttf","visible":true,"opacity":1,"offsetFrames":0});
    for value in [png, image, viewport] {
        assert!(ObjectSnapshot::from_managed_project_wire(
            "refused".into(),
            &serde_json::to_vec(&value).unwrap(),
            limits()
        )
        .is_err());
    }
    let mut malformed = wire();
    malformed["assets"][0]["source"]["data"] = object_codec::encoded_wire_test::PNG.into();
    assert!(ObjectSnapshot::from_managed_project_wire(
        "wrong-bytes".into(),
        &serde_json::to_vec(&malformed).unwrap(),
        limits()
    )
    .is_err());
}

#[test]
fn unconfigured_font_limit_keeps_the_original_unavailable_guard() {
    let result = ObjectSnapshot::from_managed_project_wire(
        "unsupported".into(),
        &serde_json::to_vec(&wire()).unwrap(),
        ProjectWireLimits::new(2 * 1024 * 1024, 8 * 1024).unwrap(),
    );
    assert!(matches!(result, Err(ProjectWireError::Unsupported)));
}

#[test]
fn mixed_batch_reserves_all_before_copy_and_binds_the_original_allocations() {
    let budget = ObjectSnapshot::encoded_test_budget(1024 * 1024);
    let trace = RefCell::new(Vec::new());
    let result = ObjectSnapshot::managed_project_wire_using(
        "font-png".into(),
        &serde_json::to_vec(&mixed()).unwrap(),
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
    let inventory = result.project().unwrap().asset_inventory().unwrap();
    assert_eq!(
        inventory
            .records()
            .iter()
            .map(|r| r.id().as_str())
            .collect::<Vec<_>>(),
        ["plate", "native-font-1"]
    );
    let font = inventory.records()[1]
        .source()
        .encoded_allocation()
        .unwrap();
    assert_eq!(font.as_slice(), FONT);
    assert!(inventory.records()[1].source().has_font_admission());
    let object_codec::ProjectReferenceState::StillImage(reference) =
        result.project().unwrap().reference_state()
    else {
        panic!()
    };
    let png = inventory.records()[0]
        .source()
        .encoded_allocation()
        .unwrap();
    assert!(Arc::ptr_eq(png, reference.encoded_allocation()));
    assert_eq!(budget.used(), Ok(font.capacity() + png.capacity()));
    drop(reference);
    drop(result);
    assert_eq!(budget.used(), Ok(0));
}

#[test]
fn aggregate_mixed_budget_refusal_precedes_every_copy() {
    let budget = ObjectSnapshot::encoded_test_budget(FONT.len());
    let copies = Cell::new(0);
    let result = ObjectSnapshot::managed_project_wire_using(
        "aggregate".into(),
        &serde_json::to_vec(&mixed()).unwrap(),
        limits(),
        |n| budget.reserve(n).map_err(|_| "reserve"),
        |_| {
            copies.set(copies.get() + 1);
            panic!("no copy")
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
fn malformed_font_rolls_back_every_reservation_before_binding_or_identity() {
    let mut corrupt = FONT.to_vec();
    let count = u16::from_be_bytes(corrupt[4..6].try_into().unwrap()) as usize;
    let table = (0..count)
        .map(|i| 12 + i * 16)
        .find(|i| &corrupt[*i..*i + 4] == b"hmtx")
        .unwrap();
    corrupt[table + 12..table + 16].copy_from_slice(&0u32.to_be_bytes());
    let mut value = mixed();
    value["assets"][0]["source"]["data"] = base64::engine::general_purpose::STANDARD
        .encode(corrupt)
        .into();
    let budget = ObjectSnapshot::encoded_test_budget(1024 * 1024);
    let copies = Cell::new(0);
    let result = ObjectSnapshot::managed_project_wire_using(
        "invalid-font".into(),
        &serde_json::to_vec(&value).unwrap(),
        limits(),
        |n| budget.reserve(n).map_err(|_| "reserve"),
        |p| {
            copies.set(copies.get() + 1);
            p.allocate().map_err(|_| "allocate")
        },
        |_, _| panic!("no bind"),
        |_, _| panic!("no identity"),
        validate,
    );
    assert!(matches!(result, Err(ProjectWireError::Invalid)));
    assert_eq!(copies.get(), 1);
    assert_eq!(budget.used(), Ok(0));
}

#[test]
fn mixed_failure_and_unwind_release_both_resources_without_touching_old_pins() {
    let old = ObjectSnapshot::from_managed_project_wire(
        "old".into(),
        &serde_json::to_vec(&wire()).unwrap(),
        limits(),
    )
    .unwrap();
    let before = object_codec::encode_project_envelope(old.project().unwrap(), limits()).unwrap();
    for unwind in [false, true] {
        for stage in ["allocate", "validate", "bind", "identity"] {
            let budget = ObjectSnapshot::encoded_test_budget(1024 * 1024);
            let copies = Cell::new(0);
            let bindings = Cell::new(0);
            let weak = RefCell::new(Vec::<Weak<Vec<u8>>>::new());
            let fail = || {
                if unwind {
                    panic!("injected {stage}")
                }
            };
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                ObjectSnapshot::managed_project_wire_using(
                    "failure".into(),
                    &serde_json::to_vec(&mixed()).unwrap(),
                    limits(),
                    |n| budget.reserve(n).map_err(|_| "reserve"),
                    |p| {
                        copies.set(copies.get() + 1);
                        if stage == "allocate" && copies.get() == 2 {
                            fail();
                            return Err("allocate");
                        }
                        p.allocate().map_err(|_| "allocate")
                    },
                    |p, a| {
                        bindings.set(bindings.get() + 1);
                        weak.borrow_mut().push(Arc::downgrade(a));
                        if stage == "bind" && bindings.get() == 2 {
                            fail();
                            return Err("bind");
                        }
                        p.bind(a).map_err(|_| "bind")
                    },
                    |_, _| {
                        assert_eq!(stage, "identity");
                        fail();
                        Err("identity")
                    },
                    |source, png| {
                        if stage == "validate" {
                            fail();
                            return Err(ProjectWireError::Invalid);
                        }
                        validate(source, png)
                    },
                )
            }));
            if unwind {
                assert!(result.is_err(), "{stage}");
            } else {
                assert!(result.unwrap().is_err(), "{stage}");
            }
            assert_eq!(budget.used(), Ok(0), "{stage}");
            assert!(
                weak.borrow().iter().all(|w| w.upgrade().is_none()),
                "{stage}"
            );
            assert_eq!(
                object_codec::encode_project_envelope(old.project().unwrap(), limits()).unwrap(),
                before
            );
        }
    }
}

#[test]
fn real_fill_undo_redo_and_save_reopen_preserve_font_bytes_ids_and_old_pins() {
    let cases: Value = serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    ))
    .unwrap();
    let mut value = mixed();
    value["drawing"] = json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":21,"layers":[{"layerUid":"legacy layer:alpha"},{"layerUid":"0"}],"objects":cases["records"]});
    let managed = ObjectSnapshot::from_managed_project_wire(
        "font-history".into(),
        &serde_json::to_vec(&value).unwrap(),
        limits(),
    )
    .unwrap();
    let mut owner = crate::history::NativeObjectHistory::from_admitted_project(
        "font-history",
        managed.project().unwrap().clone(),
    )
    .unwrap();
    let old = owner.acquire_snapshot(0).unwrap();
    let before = object_codec::encode_project_envelope(old.project().unwrap(), limits()).unwrap();
    let assets = old.project().unwrap().asset_inventory().unwrap();
    let payload = serde_json::to_vec(&cases["command"]["request"]["payload"]).unwrap();
    let prepared = owner.prepare_fill_json(&payload).unwrap();
    assert_eq!(owner.commit_fill(prepared), Ok(1));
    let after = owner.acquire_snapshot(1).unwrap();
    assert_eq!(
        serde_json::to_value(after.document()).unwrap()["objects"][0],
        cases["command"]["afterRecord"]
    );
    for (a, b) in after
        .project()
        .unwrap()
        .asset_inventory()
        .unwrap()
        .records()
        .iter()
        .zip(assets.records())
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
    let reopened = crate::history::NativeObjectHistory::from_project_file("font-reopen", &saved)
        .unwrap()
        .acquire_snapshot(0)
        .unwrap();
    assert_eq!(reopened.project(), redone.project());
    let font = &reopened
        .project()
        .unwrap()
        .asset_inventory()
        .unwrap()
        .records()[1];
    assert_eq!(font.id().as_str(), "native-font-1");
    assert_eq!(font.source().encoded_allocation().unwrap().as_slice(), FONT);
    assert!(font.source().has_font_admission());
    drop((owner, managed, after, redone, undone, reopened));
    assert_eq!(
        object_codec::encode_project_envelope(old.project().unwrap(), limits()).unwrap(),
        before
    );
}
