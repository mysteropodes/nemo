//! Binding transaction, refusal, lifetime and retained-resource history evidence.
//! The private parent supplies the existing real owner, dispatcher and fixture adapters.
use super::*;

#[test]
fn all_24_frames_and_store_publish_as_one_history_entry_and_scrub_identity_survives() {
    let mut owner = support::owner(&drawing(24, None));
    let initial = support::bytes(&owner);
    let targets = raster_targets(&wire(&owner));
    assert_applied(&run(&mut owner, "attach", attach(at(13))), true);
    assert_eq!(owner.history_depths(), (1, 0));
    assert_tags(&owner, targets.clone(), Some("im_1"));
    let attached = support::bytes(&owner);
    assert_eq!(wire(&owner)["imageMeshes"]["im_1"], mesh());
    assert_applied(&run(&mut owner, "detach", detach(at(7))), true);
    assert_eq!(owner.history_depths(), (2, 0));
    assert_tags(&owner, targets, None);
    assert!(wire(&owner)["imageMeshes"].get("im_1").is_none());
    let detached = support::bytes(&owner);
    undo_to(&mut owner, &attached);
    undo_to(&mut owner, &initial);
    redo_to(&mut owner, &attached);
    redo_to(&mut owner, &detached);
}

#[test]
fn lifetime_ids_never_rewind_with_detach_undo_redo_or_project_replacement() {
    let mut owner = support::owner(&drawing(24, None));
    let original_file = project_file(&owner);
    run(&mut owner, "first", attach(at(0)));
    assert_tag(&owner, at(0), Some("im_1"));
    run(&mut owner, "detach", detach(at(0)));
    owner.undo(owner.content_revision()).unwrap();
    owner.redo(owner.content_revision()).unwrap();
    run(&mut owner, "second", attach(at(0)));
    assert_tag(&owner, at(0), Some("im_2"));
    owner.undo(owner.content_revision()).unwrap();
    run(&mut owner, "branch", attach(at(0)));
    assert_tag(&owner, at(0), Some("im_3"));
    replace_file(&mut owner, &original_file);
    assert_eq!(owner.history_depths(), (0, 0));
    run(&mut owner, "after-replacement", attach(at(0)));
    assert_tag(&owner, at(0), Some("im_4"));
}

#[test]
fn preparation_is_detached_and_discarded_stale_or_foreign_candidates_publish_nothing() {
    let mut owner = support::owner(&drawing(24, None));
    let body = attach(at(0)).to_string();
    let initial = support::state(&owner);
    drop(owner.prepare_image_mesh_edit_json(body.as_bytes()).unwrap());
    assert_eq!(support::state(&owner), initial);
    let stale = owner.prepare_image_mesh_edit_json(body.as_bytes()).unwrap();
    let candidate = owner.prepare_image_mesh_edit_json(body.as_bytes()).unwrap();
    let mut foreign = support::owner(&drawing(24, None));
    let foreign_before = support::state(&foreign);
    assert_eq!(
        foreign.commit_image_mesh_edit(candidate).unwrap_err(),
        DispatchErrorCode::WrongDocument
    );
    assert_eq!(support::state(&foreign), foreign_before);
    run(&mut owner, "commit", attach(at(0)));
    assert_tag(&owner, at(0), Some("im_1")); // discarded preparation spends no id
    let committed = support::state(&owner);
    assert_eq!(
        owner.commit_image_mesh_edit(stale).unwrap_err(),
        DispatchErrorCode::StaleRevision
    );
    assert_eq!(support::state(&owner), committed);
}

#[test]
fn replay_cancellation_stale_and_malformed_payloads_cannot_publish_or_erase_redo() {
    let mut owner = support::owner(&drawing(24, None));
    let initial = support::state(&owner);
    for (id, field, value, code) in refused_envelopes() {
        let mut request = request(&owner, id, attach(at(0)));
        request[field] = value;
        assert_error(&mut owner, &request, code);
        assert_eq!(support::state(&owner), initial);
    }
    let mut invalid = attach(at(0));
    invalid["action"]["unexpected"] = json!(true);
    let bad = request(&owner, "malformed", invalid);
    assert_error(&mut owner, &bad, "invalid_request");
    assert_eq!(support::state(&owner), initial);
    let original = request(&owner, "once", attach(at(0)));
    let reply = dispatch(&mut owner, &original);
    assert_applied(&reply, true);
    let applied = support::state(&owner);
    assert_eq!(dispatch(&mut owner, &original), reply);
    assert_eq!(support::state(&owner), applied);
    let mut conflict = original.clone();
    conflict["payload"]["action"]["mesh"]["offsets"][0] = json!([0.2, 0]);
    assert_error(&mut owner, &conflict, "invalid_request");
    assert_eq!(support::state(&owner), applied);
    owner.undo(owner.content_revision()).unwrap();
    let undone = support::state(&owner);
    assert_eq!(dispatch(&mut owner, &original), reply);
    assert_eq!(support::state(&owner), undone);
    assert_applied(&run(&mut owner, "noop", detach(at(0))), false);
    assert_eq!(
        support::state(&owner),
        undone,
        "a no-op keeps the pending redo"
    );
    run(&mut owner, "fresh", attach(at(0)));
    assert_tag(&owner, at(0), Some("im_2"));
}

#[test]
fn real_save_reload_keeps_all_frame_tags_and_the_mesh_store() {
    let mut owner = support::owner(&drawing(24, None));
    run(&mut owner, "attach", attach(at(9)));
    let saved_drawing = support::bytes(&owner);
    let reopened =
        NativeObjectHistory::from_project_file("reopened", &project_file(&owner)).unwrap();
    assert_eq!(support::bytes(&reopened), saved_drawing);
    assert_eq!(reopened.history_depths(), (0, 0));
    assert_tags(&reopened, (0..24).map(at), Some("im_1"));
}

#[test]
fn v2_mixed_order_save_reload_preserves_every_target_slot_and_authored_order() {
    let mut owner = support::owner(&mixed());
    run(&mut owner, "mixed-save", attach(at(0)));
    let expected = support::bytes(&owner);
    let mut reopened =
        NativeObjectHistory::from_project_file("v2-reopened", &project_file(&owner)).unwrap();
    assert_eq!(support::bytes(&reopened), expected);
    run(&mut reopened, "mixed-detach", detach(at(1)));
    assert_tag(&reopened, at(0), None);
    assert_tag(&reopened, at(1), None);
    assert_eq!(
        wire(&reopened)["compositionOrder"],
        mixed()["compositionOrder"]
    );
}

#[test]
fn importing_a_high_mesh_id_raises_the_existing_owner_lifetime_sequence() {
    let mut owner = support::owner(&drawing(24, None));
    run(&mut owner, "first-before-import", attach(at(0)));
    let imported = support::owner(&drawing(24, Some("im_900")));
    replace_file(&mut owner, &project_file(&imported));
    run(&mut owner, "after-high-import", attach(at(0)));
    assert_tag(&owner, at(0), Some("im_901"));
    run(&mut owner, "detach-high", detach(at(0)));
    run(&mut owner, "after-high-release", attach(at(0)));
    assert_tag(&owner, at(0), Some("im_902"));
}

#[test]
fn binding_history_and_replacement_conserve_retained_image_records_and_encoded_payloads() {
    use crate::object_document::{AssetId, AssetRecord};
    use std::sync::Arc;

    let images = |owner: &NativeObjectHistory| {
        let snapshot = support::head(owner);
        let inventory = snapshot.project().unwrap().asset_inventory().unwrap();
        ["img-1", "img-2"]
            .map(|id| Arc::clone(inventory.get(&AssetId::new(id.into()).unwrap()).unwrap()))
    };
    let saved_assets = |owner: &NativeObjectHistory| {
        let file: Value = serde_json::from_slice(&project_file(owner)).unwrap();
        serde_json::to_vec(&file["assets"]).unwrap()
    };
    let value = tagged_copy_drawing();
    let mut owner = support::owner(&value);
    let retained = support::head(&owner);
    let original = images(&owner);
    let asset_bytes = saved_assets(&owner);
    let assert_retained = |owner: &NativeObjectHistory, expected: &[Arc<AssetRecord>]| {
        for ((id, before), after) in ["img-1", "img-2"].iter().zip(expected).zip(images(owner)) {
            assert_eq!(after.id().as_str(), *id);
            assert!(Arc::ptr_eq(before, &after), "{id}: retained record");
            let before_bytes = before.source().encoded_allocation().unwrap();
            let after_bytes = after.source().encoded_allocation().unwrap();
            assert!(
                Arc::ptr_eq(before_bytes, after_bytes),
                "{id}: encoded allocation"
            );
            assert_eq!(after_bytes.as_slice(), support::png());
        }
        assert_eq!(saved_assets(owner), asset_bytes);
    };
    for (id, action) in [
        ("resource-attach", attach(at(0))),
        ("resource-duplicate", duplicate("L2")),
        ("resource-detach", detach(at(0))),
    ] {
        run(&mut owner, id, action);
        assert_retained(&owner, &original);
    }
    for redo in [false, false, false, true, true, true] {
        if redo {
            owner.redo(owner.content_revision()).unwrap();
        } else {
            owner.undo(owner.content_revision()).unwrap();
        }
        assert_retained(&owner, &original);
    }
    let saved = project_file(&owner);
    replace_file(&mut owner, &saved);
    assert_eq!(project_file(&owner), saved);
    let replacement = images(&owner);
    for (before, after) in original.iter().zip(&replacement) {
        // File admission may create new allocations; exact IDs and encoded bytes must survive it.
        assert_eq!(after.id(), before.id());
        assert_eq!(
            after.source().encoded_allocation(),
            before.source().encoded_allocation()
        );
    }
    assert_retained(&owner, &replacement);
    run(&mut owner, "replacement-resource-attach", attach(at(0)));
    owner.undo(owner.content_revision()).unwrap();
    owner.redo(owner.content_revision()).unwrap();
    assert_retained(&owner, &replacement);
    let old_inventory = retained.project().unwrap().asset_inventory().unwrap();
    for before in &original {
        assert!(Arc::ptr_eq(old_inventory.get(before.id()).unwrap(), before));
    }
}
