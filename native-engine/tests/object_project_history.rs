use super::*;
use crate::object_document::{ProjectMetadata, ProjectReferenceState};
use serde_json::{json, Value};

fn cases() -> Value {
    serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    ))
    .unwrap()
}
fn drawing(version: u32) -> Value {
    let mut value = json!({"format":"nemo.native-object-document","formatVersion":version,"totalFrames":21,
        "layers":[{"layerUid":"legacy layer:alpha"},{"layerUid":"0"}],"objects":cases()["records"]});
    if version == 2 {
        value["compositionOrder"] = json!({
        "format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
        "contexts":[{"contextId":"scene-root","layers":[
            {"layerUid":"legacy layer:alpha","naturalPosition":0,"order":{"static":[0]}},
            {"layerUid":"0","naturalPosition":1,"order":{}}],
        "scopes":[{"frameScope":{"kind":"authored","frame":7},"elements":[
            {"layerUid":"legacy layer:alpha","strokeId":"ink/path_A","naturalPosition":0,"order":{}}]}]}],
        "migration":"explicit-plan-required","referencePolicy":"unresolved"});
    }
    value
}
fn project(version: u32, attached: bool) -> ProjectEnvelope {
    let document: ObjectDocument = serde_json::from_value(drawing(version)).unwrap();
    ObjectSnapshot::validate_drawing(&document).unwrap();
    let project =
        ProjectEnvelope::no_media(ProjectMetadata::new(320, 180, 29.97).unwrap(), document);
    if !attached {
        return project;
    }
    // Already-admitted encoded fixture: these are owner-retention controls,
    // not a claim that this private constructor performs PNG decoding.
    let hex="89504e470d0a1a0a0000000d494844520000000200000002080600000072b60d240000001a49444154789c63105432feef129ad6c050de319361d5ee33ff01378a07ad4ff3864d0000000049454e44ae426082";
    let bytes = hex
        .as_bytes()
        .chunks_exact(2)
        .map(|v| u8::from_str_radix(std::str::from_utf8(v).unwrap(), 16).unwrap())
        .collect();
    project.with_reference(
        ProjectReferenceState::admitted_still(
            "media-1".into(),
            ".plate.png".into(),
            false,
            0.5,
            -7,
            bytes,
            1024,
        )
        .unwrap(),
    )
}
fn current(owner: &NativeObjectHistory) -> ObjectSnapshot {
    owner.acquire_snapshot(owner.content_revision()).unwrap()
}
fn settings(name: &str, visible: bool, opacity: f64, offset_frames: i64) -> ReferenceEdit {
    ReferenceEdit::Settings {
        name: name.into(),
        visible,
        opacity,
        offset_frames,
    }
}
fn media(snapshot: &ObjectSnapshot, original: &ProjectEnvelope, expected: (&str, bool, f64, i64)) {
    let project = snapshot.project().unwrap();
    assert_eq!(project.project(), original.project());
    let ProjectReferenceState::StillImage(data) = project.reference_state() else {
        panic!()
    };
    let ProjectReferenceState::StillImage(before) = original.reference_state() else {
        panic!()
    };
    assert_eq!(data.id(), before.id());
    assert_eq!(data.name(), expected.0);
    assert_eq!(data.visible(), expected.1);
    assert_eq!(data.opacity(), expected.2);
    assert_eq!(data.offset_frames(), expected.3);
    assert_eq!(data.encoded_png(), before.encoded_png());
    assert_eq!(data.encoded_png().as_ptr(), before.encoded_png().as_ptr());
}

#[test]
fn existing_reference_workflow_interleaves_settings_fill_remove_in_one_history() {
    for version in [1, 2] {
        let original = project(version, true);
        let mut owner =
            NativeObjectHistory::from_admitted_project("reference-history", original.clone())
                .unwrap();
        let zero = current(&owner);
        let prepared = owner
            .prepare_reference_edit(settings("renamed.png", true, 0.25, 12))
            .unwrap();
        let stale = prepared.clone();
        assert_eq!(owner.commit_reference_edit(prepared), Ok(1));
        let one = current(&owner);
        media(&one, &original, ("renamed.png", true, 0.25, 12));
        assert!(std::ptr::eq(zero.document(), one.document()));
        assert_eq!(
            owner.commit_reference_edit(stale),
            Err(DispatchErrorCode::StaleRevision)
        );
        let payload = serde_json::to_vec(&cases()["command"]["request"]["payload"]).unwrap();
        assert_eq!(
            owner.commit_fill(owner.prepare_fill_json(&payload).unwrap()),
            Ok(2)
        );
        let two = current(&owner);
        media(&two, &original, ("renamed.png", true, 0.25, 12));
        assert_eq!(
            owner.commit_reference_edit(
                owner.prepare_reference_edit(ReferenceEdit::Remove).unwrap()
            ),
            Ok(3)
        );
        let three = current(&owner);
        assert_eq!(
            three.project().unwrap().reference_state(),
            ProjectReferenceState::NoMedia
        );
        assert!(std::ptr::eq(two.document(), three.document()));
        assert_eq!(owner.history_depths(), (3, 0));
        assert_eq!(owner.undo(3), Ok(4));
        media(&current(&owner), &original, ("renamed.png", true, 0.25, 12));
        let noop = owner
            .prepare_reference_edit(settings("renamed.png", true, 0.25, 12))
            .unwrap();
        assert_eq!(owner.commit_reference_edit(noop), Ok(4));
        assert_eq!(owner.history_depths(), (2, 1));
        assert_eq!(owner.undo(4), Ok(5));
        assert_eq!(owner.undo(5), Ok(6));
        media(&current(&owner), &original, (".plate.png", false, 0.5, -7));
        assert_eq!(owner.redo(6), Ok(7));
        assert_eq!(owner.redo(7), Ok(8));
        assert_eq!(owner.redo(8), Ok(9));
        assert_eq!(
            current(&owner).project().unwrap().reference_state(),
            ProjectReferenceState::NoMedia
        );
        assert_eq!(
            owner.commit_reference_edit(
                owner.prepare_reference_edit(ReferenceEdit::Remove).unwrap()
            ),
            Ok(9)
        );
        assert_eq!(owner.history_depths(), (3, 0));
        assert_eq!(
            serde_json::to_value(two.document()).unwrap()["objects"][0],
            cases()["command"]["afterRecord"]
        );
        drop(owner);
        media(&zero, &original, (".plate.png", false, 0.5, -7));
        media(&one, &original, ("renamed.png", true, 0.25, 12));
    }
}

#[test]
fn reference_refusals_and_remove_noop_preserve_pins_head_and_redo() {
    let original = project(1, true);
    let mut owner = NativeObjectHistory::from_admitted_project("same", original.clone()).unwrap();
    for edit in [
        settings("../bad", true, 0.5, 0),
        settings("ok", true, f64::NAN, 0),
        settings("ok", true, 0.5, i64::MAX),
    ] {
        assert!(owner.prepare_reference_edit(edit).is_err());
        assert_eq!(
            (owner.content_revision(), owner.history_depths()),
            (0, (0, 0))
        );
    }
    for instance in ["same", "other"] {
        let foreign =
            NativeObjectHistory::from_admitted_project(instance, original.clone()).unwrap();
        let expected = if instance == "same" {
            DispatchErrorCode::WrongDocument
        } else {
            DispatchErrorCode::WrongInstance
        };
        assert_eq!(
            owner.commit_reference_edit(
                foreign
                    .prepare_reference_edit(ReferenceEdit::Remove)
                    .unwrap()
            ),
            Err(expected)
        );
    }
    assert_eq!(
        owner.commit_reference_edit(owner.prepare_reference_edit(ReferenceEdit::Remove).unwrap()),
        Ok(1)
    );
    assert!(owner
        .prepare_reference_edit(settings("ok", true, 1., 0))
        .is_err());
    let payload = serde_json::to_vec(&cases()["command"]["request"]["payload"]).unwrap();
    assert_eq!(
        owner.commit_fill(owner.prepare_fill_json(&payload).unwrap()),
        Ok(2)
    );
    assert_eq!(owner.undo(2), Ok(3));
    assert_eq!(
        owner.commit_reference_edit(owner.prepare_reference_edit(ReferenceEdit::Remove).unwrap()),
        Ok(3)
    );
    assert_eq!(owner.history_depths(), (1, 1));
    let bare = NativeObjectHistory::new("bare", original.drawing().clone()).unwrap();
    assert!(bare.prepare_reference_edit(ReferenceEdit::Remove).is_err());
    assert!(bare
        .prepare_reference_edit(settings("ok", true, 1., 0))
        .is_err());
}
fn unchanged_project(snapshot: &ObjectSnapshot, original: &ProjectEnvelope, expected: &Value) {
    let project = snapshot.project().unwrap();
    assert_eq!(project.project(), original.project());
    assert_eq!(project.reference_state(), original.reference_state());
    if let (ProjectReferenceState::StillImage(a), ProjectReferenceState::StillImage(b)) =
        (project.reference_state(), original.reference_state())
    {
        assert!(std::sync::Arc::ptr_eq(&a, &b));
        assert_eq!(a.encoded_png().as_ptr(), b.encoded_png().as_ptr());
    }
    assert!(std::ptr::eq(snapshot.document(), project.drawing()));
    assert_eq!(
        serde_json::to_value(snapshot.document()).unwrap(),
        *expected
    );
}

#[test]
fn real_fill_noop_undo_redo_and_pins_preserve_composite_v1_v2_values() {
    for version in [1, 2] {
        for attached in [false, true] {
            let original = project(version, attached);
            let before = drawing(version);
            let mut after = before.clone();
            after["objects"][0] = cases()["command"]["afterRecord"].clone();
            let mut owner =
                NativeObjectHistory::from_admitted_project("project-history", original.clone())
                    .unwrap();
            let zero = current(&owner);
            assert_eq!(owner.history_depths(), (0, 0));
            let payload = cases()["command"]["request"]["payload"].clone();
            let prepared = owner
                .prepare_fill_json(&serde_json::to_vec(&payload).unwrap())
                .unwrap();
            let stale = prepared.clone();
            assert_eq!(owner.commit_fill(prepared), Ok(1));
            assert_eq!(owner.history_depths(), (1, 0));
            let one = current(&owner);
            unchanged_project(&one, &original, &after);
            assert_eq!(
                owner.commit_fill(stale),
                Err(DispatchErrorCode::StaleRevision)
            );
            assert_eq!(owner.undo(1), Ok(2));
            assert_eq!(owner.history_depths(), (0, 1));
            let two = current(&owner);
            unchanged_project(&two, &original, &before);
            let mut noop = payload.clone();
            noop["fill"] = before["objects"][0]["fill"].clone();
            let prepared = owner
                .prepare_fill_json(&serde_json::to_vec(&noop).unwrap())
                .unwrap();
            assert_eq!(owner.commit_fill(prepared), Ok(2));
            assert_eq!(owner.history_depths(), (0, 1));
            assert_eq!(owner.redo(2), Ok(3));
            assert_eq!(owner.history_depths(), (1, 0));
            unchanged_project(&current(&owner), &original, &after);
            assert_eq!(owner.undo(2), Err(DispatchErrorCode::StaleRevision));
            unchanged_project(&zero, &original, &before);
            unchanged_project(&one, &original, &after);
            drop(owner);
            unchanged_project(&zero, &original, &before);
            unchanged_project(&two, &original, &before);
        }
    }
}

#[test]
fn fresh_incarnations_and_failed_origins_do_not_publish_project_revisions() {
    let original = project(1, true);
    let mut a = NativeObjectHistory::from_admitted_project("same", original.clone()).unwrap();
    let b = NativeObjectHistory::from_admitted_project("same", original.clone()).unwrap();
    let c = NativeObjectHistory::from_admitted_project("other", original.clone()).unwrap();
    assert_ne!(a.document_id(), b.document_id());
    let payload = serde_json::to_vec(&cases()["command"]["request"]["payload"]).unwrap();
    assert_eq!(
        a.commit_fill(b.prepare_fill_json(&payload).unwrap()),
        Err(DispatchErrorCode::WrongDocument)
    );
    assert_eq!(
        a.commit_fill(c.prepare_fill_json(&payload).unwrap()),
        Err(DispatchErrorCode::WrongInstance)
    );
    assert_eq!(a.content_revision(), 0);
    assert_eq!(a.history_depths(), (0, 0));
    assert!(a.acquire_snapshot(1).is_none());
    unchanged_project(&current(&a), &original, &drawing(1));
}

#[test]
fn arbitrary_project_drawings_are_revalidated_before_owner_construction() {
    for version in [1, 2] {
        let good = project(version, true);
        for path in ["/totalFrames", "/objects/1/target/frameScope/frame"] {
            let mut raw = drawing(version);
            *raw.pointer_mut(path).unwrap() = json!(if path == "/totalFrames" { 0 } else { 21 });
            let invalid: ObjectDocument = serde_json::from_value(raw).unwrap();
            assert!(NativeObjectHistory::from_admitted_project(
                "invalid",
                good.with_drawing(invalid)
            )
            .is_err());
        }
        if version == 2 {
            let mut invalid = good.drawing().clone();
            invalid.composition_order = Default::default();
            assert!(NativeObjectHistory::from_admitted_project(
                "invalid",
                good.with_drawing(invalid)
            )
            .is_err());
        }
        assert!(NativeObjectHistory::from_admitted_project("", good.clone()).is_err());
        let owner = NativeObjectHistory::from_admitted_project("good", good.clone()).unwrap();
        unchanged_project(&current(&owner), &good, &drawing(version));
    }
}

#[test]
fn drawing_only_serialize_refuses_projects_and_bare_compatibility_stays() {
    for attached in [false, true] {
        let owner =
            NativeObjectHistory::from_admitted_project("serialize", project(1, attached)).unwrap();
        let snapshot = current(&owner);
        let request = serde_json::from_value(
            json!({"apiVersion":2,"requestId":"serialize", "instanceId":snapshot.instance_id(),
            "documentId":snapshot.document_id(),"operation":"query.document.object.serialize",
            "payload":{"atRevision":0}}),
        )
        .unwrap();
        let reply = snapshot.dispatch(request).unwrap();
        assert!(!reply.ok);
        assert!(reply.result.is_none());
        assert_eq!(reply.error.unwrap().code, DispatchErrorCode::Unavailable);
    }
    let bare = NativeObjectHistory::new("bare", project(1, false).drawing().clone()).unwrap();
    let snapshot = current(&bare);
    assert!(snapshot.project().is_none());
    let request = serde_json::from_value(
        json!({"apiVersion":2,"requestId":"bare", "instanceId":snapshot.instance_id(),
        "documentId":snapshot.document_id(),"operation":"query.document.object.serialize",
        "payload":{"atRevision":0}}),
    )
    .unwrap();
    assert!(snapshot.dispatch(request).unwrap().ok);
}

#[cfg(feature = "project_media_codec")]
mod inactive {
    use super::*;

    fn state(owner: &NativeObjectHistory) -> (String, Vec<u8>, Value, (usize, usize)) {
        let pin = current(owner);
        (
            pin.snapshot_id().into(),
            owner
                .serialize_project_file(owner.content_revision(), pin.snapshot_id())
                .unwrap(),
            owner.project_file_info().unwrap(),
            owner.history_depths(),
        )
    }

    #[test]
    fn inactive_admission_preserves_dirty_owner_and_retains_one_actual_charge() {
        // Exercise the real shared domain alone, even under parallel libtest.
        const KEY: &str = "NEMO_INACTIVE_PROJECT_TEST";
        let full_name = concat!(
            module_path!(),
            "::inactive_admission_preserves_dirty_owner_and_retains_one_actual_charge"
        );
        let name = full_name.split_once("::").unwrap().1;
        if std::env::var(KEY).as_deref() != Ok(name) {
            let result = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", name, "--nocapture", "--test-threads=1"])
                .env(KEY, name)
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&result.stdout),
                String::from_utf8_lossy(&result.stderr)
            );
            assert!(String::from_utf8_lossy(&result.stdout).contains("1 passed"));
            return;
        }
        const FULL: usize = 128 * 1024 * 1024;
        for version in [1, 2] {
            let (bytes, expected_b, cost) = {
                let seed =
                    NativeObjectHistory::from_admitted_project("seed", project(version, true))
                        .unwrap();
                let (_, bytes, info, _) = state(&seed);
                let cost = match current(&seed).project().unwrap().reference_state() {
                    ProjectReferenceState::StillImage(image) => image.encoded_png().len(),
                    _ => unreachable!(),
                };
                (bytes, info, cost)
            };
            let mut a =
                NativeObjectHistory::from_admitted_project("inactive", project(version, false))
                    .unwrap();
            let initial = state(&a);
            let mut payload = cases()["command"]["request"]["payload"].clone();
            for red in [0.8, 0.2] {
                payload["fill"]["r"] = json!(red);
                let fill = a
                    .prepare_fill_json(&serde_json::to_vec(&payload).unwrap())
                    .unwrap();
                a.commit_fill(fill).unwrap();
            }
            a.undo(2).unwrap();
            let before = state(&a);
            assert_eq!(before.3, (1, 1));
            assert_ne!(before.1, initial.1, "A is actually dirty");
            let bare = NativeObjectHistory::new("other", project(version, false).drawing().clone())
                .unwrap();
            for stale in [&initial.0, current(&bare).snapshot_id()] {
                assert_eq!(
                    a.prepare_inactive_project(stale, b"invalid").err(),
                    Some(DispatchErrorCode::StaleRevision)
                );
                assert_eq!(state(&a), before);
            }
            assert_eq!(
                bare.prepare_inactive_project(current(&bare).snapshot_id(), &bytes)
                    .err(),
                Some(DispatchErrorCode::Unavailable)
            );
            drop(bare);
            let mut invalid_png: Value = serde_json::from_slice(&bytes).unwrap();
            invalid_png["assets"][0]["data"] = json!("AAAA");
            for rejected in [
                b"invalid".to_vec(),
                serde_json::to_vec(&drawing(version)).unwrap(),
                serde_json::to_vec(&invalid_png).unwrap(),
            ] {
                assert!(a.prepare_inactive_project(&before.0, &rejected).is_err());
                assert_eq!(state(&a), before);
            }
            // Scalar permit: no 128 MiB test allocation. B must use the real quota.
            let full = ObjectSnapshot::reserve_encoded(FULL).unwrap();
            assert_eq!(
                a.prepare_inactive_project(&before.0, &bytes).err(),
                Some(DispatchErrorCode::Unavailable)
            );
            assert_eq!(state(&a), before);
            drop(full);
            let b: crate::history::InactiveProject =
                a.prepare_inactive_project(&before.0, &bytes).unwrap();
            assert_eq!(state(&a), before);
            let info = b.metadata().unwrap();
            assert_eq!(info["instanceId"], a.instance_id());
            assert_ne!(info["documentId"], a.document_id());
            assert_ne!(info["documentSnapshotId"], before.0);
            assert_eq!(info["contentRevision"], 0);
            assert_eq!(
                info["origin"],
                json!({"documentId":a.document_id(), "documentSnapshotId":before.0, "contentRevision":3})
            );
            for key in [
                "documentDigest",
                "byteCount",
                "canvasW",
                "canvasH",
                "fps",
                "totalFrames",
                "layers",
                "reference",
            ] {
                assert_eq!(info[key], expected_b[key], "{key}");
            }
            assert!(info.get("assets").is_none() && info.get("undoDepth").is_none());
            let mut detached = b.metadata().unwrap();
            detached["reference"]["name"] = json!("changed");
            assert_eq!(b.metadata().unwrap(), info);
            let retained = b.clone();
            let remaining = ObjectSnapshot::reserve_encoded(FULL - cost).unwrap();
            drop(remaining);
            assert!(ObjectSnapshot::reserve_encoded(FULL).is_err());
            drop(b);
            assert!(ObjectSnapshot::reserve_encoded(FULL).is_err());
            assert_eq!(a.redo(3), Ok(4));
            assert_eq!(a.history_depths(), (2, 0));
            assert_eq!(retained.metadata().unwrap(), info);
            drop(a);
            assert_eq!(retained.metadata().unwrap(), info);
            assert!(ObjectSnapshot::reserve_encoded(FULL).is_err());
            drop(retained);
            assert!(ObjectSnapshot::reserve_encoded(FULL).is_ok());
        }
    }
}
