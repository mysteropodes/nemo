use super::NativeObjectHistory;
use crate::object_document::ProjectReferenceState;
use crate::request_receipts::DispatchErrorCode;
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};
use std::cell::Cell;
use std::sync::Arc;

// Independently frozen PNGs: blue/white then red/green, each 2x1 RGBA8.
const BLUE: &str = "89504e470d0a1a0a0000000d4948445200000002000000010806000000f4227f8a0000000d49444154789c636060f80f060014f405fbb956037d0000000049454e44ae426082";
const RED: &str = "89504e470d0a1a0a0000000d4948445200000002000000010806000000f4227f8a0000000e49444154789c63f8cfc0f01f040110f803fd4e95c16f0000000049454e44ae426082";
fn png(hex: &str) -> Vec<u8> {
    hex.as_bytes()
        .chunks_exact(2)
        .map(|v| u8::from_str_radix(std::str::from_utf8(v).unwrap(), 16).unwrap())
        .collect()
}
fn cases() -> Value {
    serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    ))
    .unwrap()
}
fn wire() -> Value {
    json!({"format":"nemo.native-project-document","formatVersion":1,
        "project":{"canvasW":320,"canvasH":180,"fps":29.97},
        "drawing":{"format":"nemo.native-object-document","formatVersion":1,"totalFrames":21,
            "layers":[{"layerUid":"legacy layer:alpha"},{"layerUid":"0"}],"objects":cases()["records"]},
        "viewportReference":null,"assets":[]})
}
fn owner(instance: &str) -> NativeObjectHistory {
    NativeObjectHistory::from_project_file(instance, &serde_json::to_vec(&wire()).unwrap()).unwrap()
}
fn current(owner: &NativeObjectHistory) -> ObjectSnapshot {
    owner.acquire_snapshot(owner.content_revision()).unwrap()
}
fn save(owner: &NativeObjectHistory, revision: u64) -> Vec<u8> {
    let snapshot = owner.acquire_snapshot(revision).unwrap();
    owner
        .serialize_project_file(revision, snapshot.snapshot_id())
        .unwrap()
}
fn attach(owner: &mut NativeObjectHistory, id: &str, name: &str, hex: &str) -> u64 {
    owner
        .attach_reference_png(owner.content_revision(), id.into(), name.into(), &png(hex))
        .unwrap()
}
fn media(snapshot: &ObjectSnapshot, id: &str, name: &str, hex: &str) {
    let project = snapshot.project().unwrap();
    let ProjectReferenceState::StillImage(image) = project.reference_state() else {
        panic!()
    };
    assert_eq!((image.id(), image.name()), (id, name));
    assert_eq!(image.encoded_png(), png(hex));
    assert_eq!(project.project().canvas_dimensions(), (320, 180));
    assert_eq!(project.project().fps(), 29.97);
}

#[test]
fn real_project_attach_replace_history_save_and_fresh_reimport_preserve_all_consumers() {
    let mut original = owner("workflow");
    let zero = current(&original);
    let empty = save(&original, 0);
    assert_eq!(attach(&mut original, "blue", "blue.png", BLUE), 1);
    let one = current(&original);
    media(&one, "blue", "blue.png", BLUE);
    assert!(std::ptr::eq(zero.document(), one.document()));
    let blue = save(&original, 1);
    assert_eq!(attach(&mut original, "red", "red.png", RED), 2);
    let two = current(&original);
    media(&two, "red", "red.png", RED);
    assert!(std::ptr::eq(one.document(), two.document()));
    let red = save(&original, 2);
    assert_eq!(original.history_depths(), (2, 0));
    assert_eq!(original.undo(2), Ok(3));
    assert_eq!(save(&original, 3), blue);
    assert_eq!(original.undo(3), Ok(4));
    assert_eq!(save(&original, 4), empty);
    assert_eq!(original.redo(4), Ok(5));
    assert_eq!(save(&original, 5), blue);
    assert_eq!(original.redo(5), Ok(6));
    assert_eq!(save(&original, 6), red);
    assert_eq!(
        save(&original, 1),
        blue,
        "retained revision remains savable"
    );
    for bytes in [&empty, &blue, &red] {
        let reopened = NativeObjectHistory::from_project_file("workflow", bytes).unwrap();
        assert_ne!(reopened.document_id(), original.document_id());
        assert_eq!(
            (reopened.content_revision(), reopened.history_depths()),
            (0, (0, 0))
        );
        assert_eq!(
            save(&reopened, 0),
            *bytes,
            "strict codec has stable exact bytes"
        );
        let roundtrip: Value = serde_json::from_slice(bytes).unwrap();
        assert_eq!(roundtrip["drawing"], wire()["drawing"]);
        assert_eq!(roundtrip["project"], wire()["project"]);
    }
    drop(original);
    media(&one, "blue", "blue.png", BLUE);
    media(&two, "red", "red.png", RED);
}

#[test]
fn native_layer_names_survive_project_info_fill_history_save_and_fresh_reopen() {
    let baseline: Value = serde_json::from_str(include_str!(
        "../../tests/animation/fixtures/curve-workflow.json"
    ))
    .unwrap();
    assert_eq!(baseline["layers"][0]["layerUid"], "r08_curve_layer");
    assert_eq!(baseline["layers"][0]["name"], "R08 rectangle");
    for version in [1, 2] {
        for name in [
            None,
            Some("R08 rectangle"),
            Some(""),
            Some(" Calque 雪 🟦\n"),
        ] {
            let mut input = wire();
            let drawing = &mut input["drawing"];
            drawing["formatVersion"] = json!(version);
            drawing["layers"][0]["layerUid"] = json!("r08_curve_layer");
            if let Some(name) = name {
                drawing["layers"][0]["name"] = json!(name);
            }
            drawing["objects"][0]["target"]["layerUid"] = json!("r08_curve_layer");
            if version == 2 {
                drawing["compositionOrder"] = json!({
                    "format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
                    "contexts":[{"contextId":"scene-root","layers":[
                        {"layerUid":"r08_curve_layer","naturalPosition":0,"order":{}},
                        {"layerUid":"0","naturalPosition":1,"order":{}}],
                    "scopes":[{"frameScope":{"kind":"authored","frame":7},"elements":[
                        {"layerUid":"r08_curve_layer","strokeId":"ink/path_A","naturalPosition":0,"order":{}}]}]}],
                    "migration":"explicit-plan-required","referencePolicy":"unresolved"});
            }
            let mut owner = NativeObjectHistory::from_project_file(
                "layer-name",
                &serde_json::to_vec(&input).unwrap(),
            )
            .unwrap();
            let zero = current(&owner);
            let before = save(&owner, 0);
            assert_eq!(serde_json::from_slice::<Value>(&before).unwrap(), input);
            let check_name = |snapshot: &ObjectSnapshot| {
                assert_eq!(
                    snapshot.document().layers()[0].layer_uid(),
                    "r08_curve_layer"
                );
                assert_eq!(snapshot.document().layers()[0].name(), name);
                assert_eq!(snapshot.document().layers()[1].name(), None);
            };
            let info = owner.project_file_info().unwrap();
            assert_eq!(info["layers"][0]["layerUid"], "r08_curve_layer");
            assert_eq!(info["layers"][0].get("name").and_then(Value::as_str), name);
            let mut payload = cases()["command"]["request"]["payload"].clone();
            payload["stableTarget"]["layerUid"] = json!("r08_curve_layer");
            let prepared = owner
                .prepare_fill_json(&serde_json::to_vec(&payload).unwrap())
                .unwrap();
            let stale = prepared.clone();
            assert_eq!(owner.commit_fill(prepared), Ok(1));
            assert_eq!(
                owner.commit_fill(stale),
                Err(DispatchErrorCode::StaleRevision)
            );
            let one = current(&owner);
            let after = save(&owner, 1);
            assert_ne!(after, before);
            assert_eq!(owner.undo(1), Ok(2));
            assert_eq!(save(&owner, 2), before);
            let mut noop = payload.clone();
            noop["fill"] = input["drawing"]["objects"][0]["fill"].clone();
            let prepared = owner
                .prepare_fill_json(&serde_json::to_vec(&noop).unwrap())
                .unwrap();
            assert_eq!(owner.commit_fill(prepared), Ok(2));
            assert_eq!(owner.history_depths(), (0, 1));
            assert_eq!(owner.redo(2), Ok(3));
            assert_eq!(save(&owner, 3), after);
            assert_eq!(
                save(&owner, 0),
                before,
                "retained revision remains immutable"
            );
            for snapshot in [&zero, &one, &current(&owner)] {
                check_name(snapshot);
            }
            for bytes in [&before, &after] {
                let reopened =
                    NativeObjectHistory::from_project_file("layer-name-reopened", bytes).unwrap();
                check_name(&current(&reopened));
                assert_eq!(save(&reopened, 0), *bytes);
                assert_eq!(
                    reopened.project_file_info().unwrap()["layers"],
                    info["layers"]
                );
            }
        }
    }
}

#[test]
fn native_visibility_command_keeps_project_save_reopen_and_history_on_the_same_owner() {
    let mut owner = owner("visible-project");
    let before = save(&owner, 0);
    let request = |owner: &NativeObjectHistory, id: &str, visible: bool| {
        serde_json::from_value::<crate::commands::OpacityRequest>(json!({
        "apiVersion":2,"requestId":id,"instanceId":owner.instance_id(),"documentId":owner.document_id(),
        "expectedRevision":owner.content_revision(),"operation":"command.document.object.layer.visibility.set",
        "payload":{"command":"object.layer.visibility.set","layerUid":"legacy layer:alpha","visible":visible}})).unwrap()
    };
    let hide = request(&owner, "project-hide", false);
    assert!(
        owner
            .try_dispatch_object_command(&hide)
            .unwrap()
            .unwrap()
            .ok
    );
    let hidden = save(&owner, 1);
    assert_ne!(hidden, before);
    assert_eq!(
        owner.project_file_info().unwrap()["layers"][0]["visible"],
        false
    );
    let reopened = NativeObjectHistory::from_project_file("reopened", &hidden).unwrap();
    assert!(!current(&reopened).document().layers()[0].visible());
    assert_eq!(save(&reopened, 0), hidden);
    assert_eq!(owner.undo(1), Ok(2));
    assert_eq!(save(&owner, 2), before);
    let noop = request(&owner, "project-show-noop", true);
    assert!(
        owner
            .try_dispatch_object_command(&noop)
            .unwrap()
            .unwrap()
            .ok
    );
    assert_eq!(owner.history_depths(), (0, 1));
    assert_eq!(owner.redo(2), Ok(3));
    assert_eq!(save(&owner, 3), hidden);
    assert_eq!(
        save(&owner, 0),
        before,
        "retained project bytes cannot be mutated by visibility"
    );
}

#[test]
fn settings_fill_remove_and_attachment_share_one_history_and_preserve_replace_settings() {
    use crate::commands::ReferenceEdit;
    let mut owner = owner("interleaved");
    attach(&mut owner, "blue", "blue.png", BLUE);
    let edit = owner
        .prepare_reference_edit(ReferenceEdit::Settings {
            name: "old.png".into(),
            visible: false,
            opacity: 0.25,
            offset_frames: -7,
        })
        .unwrap();
    assert_eq!(owner.commit_reference_edit(edit), Ok(2));
    assert_eq!(attach(&mut owner, "red", "new.png", RED), 3);
    let ProjectReferenceState::StillImage(image) =
        current(&owner).project().unwrap().reference_state()
    else {
        panic!()
    };
    assert_eq!(
        (image.visible(), image.opacity(), image.offset_frames()),
        (false, 0.25, -7)
    );
    let payload = serde_json::to_vec(&cases()["command"]["request"]["payload"]).unwrap();
    let fill = owner.prepare_fill_json(&payload).unwrap();
    assert_eq!(owner.commit_fill(fill), Ok(4));
    let after = current(&owner);
    media(&after, "red", "new.png", RED);
    assert_eq!(
        serde_json::to_value(after.document()).unwrap()["objects"][0],
        cases()["command"]["afterRecord"]
    );
    let remove = owner.prepare_reference_edit(ReferenceEdit::Remove).unwrap();
    assert_eq!(owner.commit_reference_edit(remove), Ok(5));
    assert_eq!(owner.undo(5), Ok(6));
    media(&current(&owner), "red", "new.png", RED);
    let before_noop = save(&owner, 6);
    assert_eq!(attach(&mut owner, "red", "new.png", RED), 6);
    assert_eq!(owner.history_depths(), (4, 1));
    assert_eq!(save(&owner, 6), before_noop);
    assert_eq!(owner.redo(6), Ok(7));
    assert_eq!(
        current(&owner).project().unwrap().reference_state(),
        ProjectReferenceState::NoMedia
    );
}

#[test]
fn stale_bad_png_settings_and_snapshot_pins_refuse_without_history_or_byte_changes() {
    let mut owner = owner("refusals");
    attach(&mut owner, "blue", "blue.png", BLUE);
    owner.undo(1).unwrap();
    let before = save(&owner, 2);
    assert_eq!(
        owner.attach_reference_png(0, "bad".into(), "bad".into(), &[]),
        Err(DispatchErrorCode::StaleRevision)
    );
    let mut corrupt = png(RED);
    let last = corrupt.len() - 1;
    corrupt[last] ^= 1;
    for (id, name, bytes) in [
        ("good", "good.png", vec![]),
        ("good", "good.png", corrupt),
        ("", "good.png", png(RED)),
        ("good", "../bad.png", png(RED)),
    ] {
        assert!(owner
            .attach_reference_png(2, id.into(), name.into(), &bytes)
            .is_err());
        assert_eq!(
            (owner.content_revision(), owner.history_depths()),
            (2, (0, 1))
        );
        assert_eq!(save(&owner, 2), before);
    }
    assert_eq!(
        owner.serialize_project_file(2, "wrong"),
        Err(DispatchErrorCode::StaleRevision)
    );
    assert_eq!(
        owner.serialize_project_file(99, current(&owner).snapshot_id()),
        Err(DispatchErrorCode::StaleRevision)
    );
    owner.redo(2).unwrap();
    media(&current(&owner), "blue", "blue.png", BLUE);
}

#[test]
fn sealed_candidate_refuses_other_owner_and_intervening_revision_even_after_undo() {
    let mut owner = owner("same");
    let source = png(BLUE);
    let foreign = NativeObjectHistory::from_project_file("same", &save(&owner, 0)).unwrap();
    let candidate = current(&foreign)
        .prepare_reference_attachment("blue".into(), "blue.png".into(), &source)
        .unwrap();
    assert_eq!(
        owner.owner.commit_reference_attachment(candidate),
        Err(DispatchErrorCode::WrongDocument)
    );
    let foreign = NativeObjectHistory::from_project_file("other", &save(&owner, 0)).unwrap();
    let candidate = current(&foreign)
        .prepare_reference_attachment("blue".into(), "blue.png".into(), &source)
        .unwrap();
    assert_eq!(
        owner.owner.commit_reference_attachment(candidate),
        Err(DispatchErrorCode::WrongInstance)
    );
    let candidate = current(&owner)
        .prepare_reference_attachment("blue".into(), "blue.png".into(), &source)
        .unwrap();
    attach(&mut owner, "red", "red.png", RED);
    owner.undo(1).unwrap();
    assert_eq!(
        owner.owner.commit_reference_attachment(candidate),
        Err(DispatchErrorCode::StaleRevision)
    );
    assert_eq!(owner.content_revision(), 2);
    assert_eq!(owner.history_depths(), (0, 1));
}

#[test]
fn quota_precedes_persistent_copy_and_allocator_binding_failures_release_the_charge() {
    let owner = owner("quota");
    let snapshot = current(&owner);
    let source = png(BLUE);
    let small = ObjectSnapshot::encoded_test_budget(source.len() - 1);
    let allocations = Cell::new(0);
    let refused = snapshot.prepare_reference_attachment_using(
        "blue".into(),
        "blue.png".into(),
        &source,
        |n| small.reserve(n).map_err(|_| "quota"),
        |p| {
            allocations.set(allocations.get() + 1);
            p.allocate().map_err(|_| "alloc")
        },
        |p, a| p.bind(a).map_err(|_| "bind"),
    );
    assert!(matches!(refused, Err(DispatchErrorCode::Unavailable)));
    assert_eq!((allocations.get(), small.used()), (0, Ok(0)));
    let budget = ObjectSnapshot::encoded_test_budget(source.len());
    for length in [0, source.len() - 1, source.len() + 1] {
        let refused = snapshot.prepare_reference_attachment_using(
            "blue".into(),
            "blue.png".into(),
            &source,
            |n| budget.reserve(n).map_err(|_| "quota"),
            |_| Ok(vec![0; length]),
            |_, _| panic!("incorrect buffer cannot be bound"),
        );
        assert!(matches!(refused, Err(DispatchErrorCode::Unavailable)));
        assert_eq!(budget.used(), Ok(0));
    }
    let refused = snapshot.prepare_reference_attachment_using(
        "blue".into(),
        "blue.png".into(),
        &source,
        |n| budget.reserve(n).map_err(|_| "quota"),
        |p| p.allocate().map_err(|_| "alloc"),
        |_, _| Err("binding unavailable"),
    );
    assert!(matches!(refused, Err(DispatchErrorCode::Unavailable)));
    assert_eq!(budget.used(), Ok(0));
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        snapshot.prepare_reference_attachment_using(
            "blue".into(),
            "blue.png".into(),
            &source,
            |n| budget.reserve(n).map_err(|_| "quota"),
            |_| panic!("allocation interrupted"),
            |_, _| Ok(()),
        )
    }));
    assert!(panic.is_err());
    assert_eq!(budget.used(), Ok(0));
    assert_eq!(owner.content_revision(), 0);
    assert_eq!(owner.history_depths(), (0, 0));
}

#[test]
fn attached_allocation_charge_survives_owner_drop_until_last_old_snapshot() {
    let mut owner = owner("retention");
    let source = png(BLUE);
    let budget = ObjectSnapshot::encoded_test_budget(source.len());
    let candidate = current(&owner)
        .prepare_reference_attachment_using(
            "blue".into(),
            "blue.png".into(),
            &source,
            |n| budget.reserve(n).map_err(|_| "quota"),
            |p| p.allocate().map_err(|_| "alloc"),
            |p, a| p.bind(a).map_err(|_| "bind"),
        )
        .unwrap();
    assert_eq!(owner.owner.commit_reference_attachment(candidate), Ok(1));
    let old = current(&owner);
    let ProjectReferenceState::StillImage(image) = old.project().unwrap().reference_state() else {
        panic!()
    };
    let weak = Arc::downgrade(image.encoded_allocation());
    drop(image);
    attach(&mut owner, "red", "red.png", RED);
    drop(owner);
    assert_eq!(budget.used(), Ok(source.len()));
    assert!(weak.upgrade().is_some());
    media(&old, "blue", "blue.png", BLUE);
    drop(old);
    assert!(weak.upgrade().is_none());
    assert_eq!(budget.used(), Ok(0));
}

#[test]
fn strict_reimport_and_bare_drawing_refusals_preserve_the_existing_owner() {
    let original = owner("wire");
    let before = save(&original, 0);
    let mut extra = wire();
    extra["unexpected"] = true.into();
    let duplicate = String::from_utf8(before.clone()).unwrap().replacen(
        "\"formatVersion\":1",
        "\"formatVersion\":1,\"formatVersion\":1",
        1,
    );
    for bytes in [
        vec![],
        b"null".to_vec(),
        duplicate.into_bytes(),
        serde_json::to_vec(&extra).unwrap(),
        serde_json::to_vec(&wire()["drawing"]).unwrap(),
    ] {
        assert!(NativeObjectHistory::from_project_file("wire", &bytes).is_err());
    }
    assert_eq!(save(&original, 0), before);
    assert!(NativeObjectHistory::from_project_file("", &before).is_err());
    let drawing = current(&original).document().clone();
    let mut bare = NativeObjectHistory::new("bare", drawing).unwrap();
    assert_eq!(
        bare.attach_reference_png(0, "blue".into(), "blue.png".into(), &png(BLUE)),
        Err(DispatchErrorCode::Unavailable)
    );
    assert_eq!(
        bare.serialize_project_file(0, current(&bare).snapshot_id()),
        Err(DispatchErrorCode::Unavailable)
    );
    assert_eq!(bare.content_revision(), 0);
    assert_eq!(bare.history_depths(), (0, 0));
}
