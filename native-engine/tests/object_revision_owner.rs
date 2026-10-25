//! Existing revision-owner conservation, identity, bounds and resource controls.
use super::*;
fn quota_project(bytes: Vec<u8>) -> ProjectEnvelope {
    let drawing: ObjectDocument = serde_json::from_value(serde_json::json!({
        "format":"nemo.native-object-document","formatVersion":1,"totalFrames":1,
        "layers":[{"layerUid":"L"}],"objects":[]}))
    .unwrap();
    ObjectSnapshot::validate_drawing(&drawing).unwrap();
    let project = ProjectEnvelope::no_media(
        crate::object_document::ProjectMetadata::new(320, 180, 24.).unwrap(),
        drawing,
    );
    // Opaque already-admitted retention fixture; no PNG decoding claim.
    project.with_reference(
        ProjectReferenceState::admitted_still(
            "media-1".into(),
            "plate.png".into(),
            true,
            1.,
            0,
            bytes,
            1024,
        )
        .unwrap(),
    )
}
fn allocation(project: &ProjectEnvelope) -> Arc<Vec<u8>> {
    let ProjectReferenceState::StillImage(data) = project.reference_state() else {
        panic!()
    };
    Arc::clone(data.encoded_allocation())
}

#[test]
fn real_owner_entry_counts_aliases_pins_and_escaped_models_until_blob_dies() {
    let project = quota_project(vec![1, 2, 3]);
    let other = quota_project(vec![1, 2, 3]);
    let cost = allocation(&project).capacity();
    let other_cost = allocation(&other).capacity();
    let budget =
        encoded_retention_budget::EncodedRetentionBudget::new(cost + other_cost - 1).unwrap();
    let first = ObjectSnapshot::project_entry(
        "first".into(),
        project.clone(),
        &budget,
        ObjectSnapshot::from_value,
    )
    .unwrap();
    let second = ObjectSnapshot::project_entry(
        "second".into(),
        project.clone(),
        &budget,
        ObjectSnapshot::from_value,
    )
    .unwrap();
    assert_ne!(first.document_id(), second.document_id());
    assert_eq!(budget.used(), Ok(cost));
    let mut owner = ObjectRevisionOwner::from_admitted(first);
    let pinned = owner.acquire(0).unwrap();
    let escaped = pinned.project().unwrap().clone();
    let reference = escaped.reference_state();
    owner.publish(project.drawing().clone()).unwrap();
    let changed = project
        .reference_state()
        .with_settings("after.png".into(), false, 0.5, 2)
        .unwrap();
    owner.publish_reference(changed).unwrap();
    owner.restore(0).unwrap();
    assert_eq!(budget.used(), Ok(cost));
    owner
        .publish_reference(ProjectReferenceState::NoMedia)
        .unwrap();
    assert_eq!(budget.used(), Ok(cost));
    assert!(ObjectSnapshot::project_entry(
        "over".into(),
        other.clone(),
        &budget,
        ObjectSnapshot::from_value
    )
    .is_err());
    assert_eq!(owner.content_revision(), 4);
    assert!(owner.acquire(5).is_none());
    drop(owner);
    drop(project);
    drop(second);
    assert_eq!(budget.used(), Ok(cost));
    drop(pinned);
    assert_eq!(budget.used(), Ok(cost));
    drop(escaped);
    assert_eq!(budget.used(), Ok(cost));
    drop(reference);
    assert_eq!(budget.used(), Ok(0));
    let admitted =
        ObjectSnapshot::project_entry("reaped".into(), other, &budget, ObjectSnapshot::from_value)
            .unwrap();
    assert_eq!(budget.used(), Ok(other_cost));
    drop(admitted);
    assert_eq!(budget.used(), Ok(0));
}

#[test]
fn owner_identity_failure_rolls_back_only_uncommitted_adoption() {
    let project = quota_project(vec![1, 2, 3]);
    let cost = allocation(&project).capacity();
    let budget = encoded_retention_budget::EncodedRetentionBudget::new(cost).unwrap();
    let no_media = project.with_reference(ProjectReferenceState::NoMedia);
    let plain = ObjectSnapshot::project_entry(
        "no-media".into(),
        no_media,
        &budget,
        ObjectSnapshot::from_value,
    )
    .unwrap();
    assert!(plain.project().is_some());
    assert_eq!(budget.used(), Ok(0));
    let bare = ObjectSnapshot::from_admitted("bare".into(), project.drawing().clone()).unwrap();
    assert!(bare.project().is_none());
    assert_eq!(budget.used(), Ok(0));
    assert!(
        ObjectSnapshot::project_entry("fail".into(), project.clone(), &budget, |_, _| Err(
            "injected identity failure"
        ))
        .is_err()
    );
    assert_eq!(budget.used(), Ok(0));
    assert!(ObjectSnapshot::project_entry(
        "".into(),
        project.clone(),
        &budget,
        ObjectSnapshot::from_value
    )
    .is_err());
    assert_eq!(budget.used(), Ok(0));
    let admitted = ObjectSnapshot::project_entry(
        "live".into(),
        project.clone(),
        &budget,
        ObjectSnapshot::from_value,
    )
    .unwrap();
    assert!(
        ObjectSnapshot::project_entry("fail".into(), project.clone(), &budget, |_, _| Err(
            "injected identity failure"
        ))
        .is_err()
    );
    assert_eq!(budget.used(), Ok(cost));
    assert_eq!(admitted.content_revision(), 0);
    let mut owned = Vec::with_capacity(cost + 1);
    owned.extend_from_slice(&[1, 2, 3]);
    let oversized = quota_project(owned);
    assert!(
        ObjectSnapshot::project_entry("large".into(), oversized, &budget, |_, _| panic!(
            "quota refusal must precede identity allocation"
        ))
        .is_err()
    );
    assert_eq!(budget.used(), Ok(cost));
    drop(project);
    drop(admitted);
    assert_eq!(budget.used(), Ok(0));
}

#[test]
fn safe_integer_exhaustion_preserves_all_retained_snapshots_and_head() {
    let document: ObjectDocument = serde_json::from_value(serde_json::json!({
        "format":"nemo.native-object-document","formatVersion":1,"totalFrames":1,
        "layers":[{"layerUid":"limit-layer"}],"objects":[]}))
    .unwrap();
    let bare = ObjectSnapshot::from_admitted("limit-test".into(), document.clone()).unwrap();
    let project = ProjectEnvelope::no_media(
        crate::object_document::ProjectMetadata::new(320, 180, 24.).unwrap(),
        document.clone(),
    );
    let composite = ObjectSnapshot::from_admitted_project("project-limit".into(), project).unwrap();
    for snapshot in [bare, composite] {
        let mut owner = ObjectRevisionOwner::from_admitted(snapshot.clone());
        owner
            .snapshots
            .insert(MAX_CONTENT_REVISION - 1, Arc::clone(&snapshot.value));
        owner.content_revision = MAX_CONTENT_REVISION - 1;
        assert_eq!(owner.publish(document.clone()), Ok(MAX_CONTENT_REVISION));
        let before = owner.snapshots.clone();
        assert!(owner.publish(document.clone()).is_err());
        assert!(owner.restore(0).is_err());
        assert!(owner.restore(MAX_CONTENT_REVISION + 1).is_err());
        assert_eq!(owner.content_revision(), MAX_CONTENT_REVISION);
        assert_eq!(owner.snapshots, before);
        assert_eq!(owner.acquire(0).unwrap().document(), snapshot.document());
        assert_eq!(
            owner.acquire(MAX_CONTENT_REVISION).unwrap().project(),
            snapshot.project()
        );
        assert!(owner.acquire(MAX_CONTENT_REVISION + 1).is_none());
    }
}
#[test]
fn reference_storage_refuses_new_allocations_ids_and_attach_before_publication() {
    let document: ObjectDocument = serde_json::from_value(serde_json::json!({
        "format":"nemo.native-object-document","formatVersion":1,"totalFrames":1,
        "layers":[{"layerUid":"L"}],"objects":[]}))
    .unwrap();
    ObjectSnapshot::validate_drawing(&document).unwrap();
    let project = ProjectEnvelope::no_media(
        crate::object_document::ProjectMetadata::new(320, 180, 24.).unwrap(),
        document.clone(),
    );
    // Already-admitted bytes; this test checks allocation/identity barriers,
    // not PNG validity or resource-manager admission.
    let encoded=b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\0\x02\0\0\0\x02\x08\x06\0\0\0\x72\xb6\x0d\x24\0\0\0\x1aIDAT\x78\x9c\x63\x10\x54\x32\xfe\xef\x12\x9a\xd6\xc0\x50\xde\x31\x93\x61\xd5\xee\x33\xff\x01\x37\x8a\x07\xad\x4f\xf3\x86\x4d\0\0\0\0IEND\xae\x42\x60\x82";
    let state = ProjectReferenceState::admitted_still(
        "media-1".into(),
        "plate.png".into(),
        true,
        0.5,
        0,
        encoded.to_vec(),
        1024,
    )
    .unwrap();
    let mut owner = ObjectRevisionOwner::from_admitted(
        ObjectSnapshot::from_admitted_project(
            "guard".into(),
            project.with_reference(state.clone()),
        )
        .unwrap(),
    );
    let before = owner.snapshots.clone();
    for id in ["media-1", "foreign"] {
        let other = ProjectReferenceState::admitted_still(
            id.into(),
            "plate.png".into(),
            true,
            0.5,
            0,
            encoded.to_vec(),
            1024,
        )
        .unwrap();
        assert!(owner.publish_reference(other).is_err());
        assert_eq!(owner.content_revision(), 0);
        assert_eq!(owner.snapshots, before);
    }
    assert_eq!(
        owner.publish_reference(ProjectReferenceState::NoMedia),
        Ok(1)
    );
    assert!(owner.publish_reference(state.clone()).is_err());
    assert_eq!(owner.content_revision(), 1);
    let bare = ObjectSnapshot::from_admitted("bare".into(), document).unwrap();
    assert!(ObjectRevisionOwner::from_admitted(bare)
        .publish_reference(state.clone())
        .is_err());
    let full = Arc::clone(owner.snapshots.get(&0).unwrap());
    owner.snapshots.insert(MAX_CONTENT_REVISION, full);
    owner.content_revision = MAX_CONTENT_REVISION;
    let before = owner.snapshots.clone();
    let same_blob = state
        .with_settings("changed.png".into(), false, 0.25, 7)
        .unwrap();
    assert!(owner.publish_reference(same_blob).is_err());
    assert_eq!(owner.snapshots, before);
    assert_eq!(owner.content_revision(), MAX_CONTENT_REVISION);
}
