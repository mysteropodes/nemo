//! The kernel against the real layer-create owner: same ids in the same states (L10-G).
//! The owner (`NativeObjectHistory` over `ObjectFillOwner`) is the actual single authority; nothing
//! here mocks it. A kernel id must equal the id the owner prepares for the same origin.
use super::*;
use crate::history::NativeObjectHistory;
use crate::object_codec::decode_storage_project;
use serde_json::{json, Value};

fn drawing() -> Value {
    let cases: Value = serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    ))
    .unwrap();
    let mut record = cases["records"][0].clone();
    record["target"] = json!({"contextId":"scene-root","frameScope":{"kind":"authored","frame":0},"layerUid":"L0","strokeId":"a"});
    json!({"format":"nemo.native-object-document","formatVersion":1,
        "totalFrames":120,"layers":[{"layerUid":"L0","name":"Original","visible":false},
        {"layerUid":"native-layer:1:0"}],"objects":[record]})
}
fn owner() -> NativeObjectHistory {
    NativeObjectHistory::new(
        "remap-owner",
        decode_storage_project(drawing().to_string().as_bytes()).unwrap(),
    )
    .unwrap()
}
fn payload() -> Vec<u8> {
    json!({"command":"object.layer.create","name":"Imported"})
        .to_string()
        .into_bytes()
}
/// The origin the kernel sees: the owner's current revision and every layer uid it holds.
fn origin_of(owner: &NativeObjectHistory) -> OriginIds {
    let snapshot = owner.acquire_snapshot(owner.content_revision()).unwrap();
    let uids: Vec<String> = snapshot
        .document()
        .layers
        .iter()
        .map(|layer| layer.layer_uid().to_owned())
        .collect();
    OriginIds::new(owner.content_revision())
        .with_ids(RemapKind::Layer, uids.iter().map(String::as_str))
}
fn kernel_id(owner: &NativeObjectHistory) -> String {
    allocate_ids(&origin_of(owner), RemapKind::Layer, 1)
        .unwrap()
        .remove(0)
}
fn owner_id(owner: &NativeObjectHistory) -> String {
    owner
        .prepare_layer_create_json(&payload())
        .unwrap()
        .layer_uid()
        .to_owned()
}

#[test]
fn the_kernel_allocates_the_id_the_owner_prepares_in_every_state() {
    let mut owner = owner();
    // Initial state: revision 0, "native-layer:1:0" already imported, so the owner probes to suffix 1.
    assert_eq!(owner_id(&owner), "native-layer:1:1");
    assert_eq!(kernel_id(&owner), "native-layer:1:1");
    // Preparing consumed nothing: asking twice gives the same id.
    assert_eq!(owner_id(&owner), "native-layer:1:1");
    let candidate = owner.prepare_layer_create_json(&payload()).unwrap();
    assert_eq!(owner.commit_layer_create(candidate).unwrap(), 1);
    assert_eq!(owner_id(&owner), "native-layer:2:0");
    assert_eq!(kernel_id(&owner), "native-layer:2:0");
    // Undo creates a new monotonic revision; ids never go back.
    assert_eq!(owner.undo(1).unwrap(), 2);
    assert_eq!(owner_id(&owner), "native-layer:3:0");
    assert_eq!(kernel_id(&owner), "native-layer:3:0");
    assert_eq!(owner.redo(2).unwrap(), 3);
    assert_eq!(owner_id(&owner), kernel_id(&owner));
    assert_eq!(kernel_id(&owner), "native-layer:4:0");
}

#[test]
fn a_refused_stale_preparation_consumes_no_id() {
    let mut owner = owner();
    let first = owner.prepare_layer_create_json(&payload()).unwrap();
    let second = owner.prepare_layer_create_json(&payload()).unwrap();
    assert_eq!(first.layer_uid(), second.layer_uid());
    owner.commit_layer_create(first).unwrap();
    assert!(owner.commit_layer_create(second).is_err(), "stale origin");
    assert_eq!(owner.content_revision(), 1);
    // The refusal changed nothing: the next id is still the pure function of the origin.
    assert_eq!(owner_id(&owner), kernel_id(&owner));
    assert_eq!(kernel_id(&owner), "native-layer:2:0");
}

#[test]
fn a_remapped_batch_ids_never_collide_with_the_origin_the_owner_holds() {
    let owner = owner();
    let origin = origin_of(&owner);
    assert!(origin.contains(RemapKind::Layer, "native-layer:1:0"));
    let batch: Vec<SourceEntry> = ["ly_a", "ly_b", "ly_c"]
        .iter()
        .map(|id| SourceEntry::mapped(RemapKind::Layer, id))
        .collect();
    let table = build_remap_table(&origin, &batch).unwrap();
    assert_eq!(
        table.native_ids(RemapKind::Layer),
        ["native-layer:1:1", "native-layer:1:2", "native-layer:1:3"]
    );
    // The first batch id is exactly what the owner would have created alone.
    assert_eq!(table.native_ids(RemapKind::Layer)[0], owner_id(&owner));
    // An import whose source id is already a native id of the origin is ambiguous and refused.
    let clash = vec![SourceEntry::mapped(RemapKind::Layer, "native-layer:1:0")];
    assert_eq!(
        build_remap_table(&origin, &clash),
        Err(RemapError::CollidesWithOrigin { index: 0 })
    );
    let parent = table
        .resolve_optional(&origin, RemapKind::Layer, Some("L0"))
        .unwrap();
    assert_eq!(parent, Some(Resolved::Outside("L0".to_owned())));
}
