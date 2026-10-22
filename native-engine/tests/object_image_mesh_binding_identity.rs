//! A1 durable identity regressions through the existing codec, owner and history.
use crate::history::NativeObjectHistory;
use crate::object_document::{PropertyHolder, PropertyHolderTarget};
use crate::request_receipts::DispatchErrorCode;
use serde_json::{json, Value};

#[path = "object_image_mesh_binding_fixtures.rs"]
mod fixtures;
#[path = "object_raster_asset_commands_support.rs"]
mod support;
use fixtures::{at, drawing, mesh};

fn apply(owner: &mut NativeObjectHistory, action: Value) {
    let payload = json!({"command":"object.image.mesh.edit","action":action}).to_string();
    let candidate = owner
        .prepare_image_mesh_edit_json(payload.as_bytes())
        .unwrap();
    owner.commit_image_mesh_edit(candidate).unwrap();
}
fn attach(owner: &mut NativeObjectHistory) {
    apply(owner, json!({"kind":"attach","target":at(0),"mesh":mesh()}));
}
fn bytes(owner: &NativeObjectHistory) -> Vec<u8> {
    let snapshot = support::head(owner);
    owner
        .serialize_project_file(owner.content_revision(), snapshot.snapshot_id())
        .unwrap()
}
fn replace(owner: &mut NativeObjectHistory, file: &[u8]) {
    let snapshot = support::head(owner);
    let candidate = owner
        .prepare_project_replacement(snapshot.snapshot_id(), file)
        .unwrap();
    owner.commit_project_replacement(candidate).unwrap();
}
fn wire(owner: &NativeObjectHistory) -> Value {
    serde_json::from_slice(&support::bytes(owner)).unwrap()
}
fn mesh_id(owner: &NativeObjectHistory) -> String {
    wire(owner)["items"]["raster"][0]["item"]["meshId"]
        .as_str()
        .unwrap()
        .into()
}

#[test]
fn final_detach_save_reopen_reserves_the_old_mesh_identity_without_rebinding_tracks() {
    let mut value = drawing(24, Some("im_1"));
    value["propertyHolders"] = json!([{"target":{"contextId":"scene-root","layerUid":"L0","strokeId":"im_1"},
        "properties":[{"property":"vtx8","static":[0.1,0.2],"track":{"keys":[
            {"frame":0,"v":[0.1,0.2]},{"frame":23,"v":[0.3,0.4]}]}}]}]);
    let mut owner = support::owner(&value);
    apply(&mut owner, json!({"kind":"detach","target":at(0)}));
    let detached = wire(&owner);
    assert!(detached["imageMeshes"].get("im_1").is_none());
    assert_eq!(
        detached["propertyHolders"][0]["meshSource"],
        json!({"meshId":"im_1"})
    );
    let old: PropertyHolderTarget =
        serde_json::from_value(value["propertyHolders"][0]["target"].clone()).unwrap();
    let mut reopened =
        NativeObjectHistory::from_project_file("identity-reopen", &bytes(&owner)).unwrap();
    attach(&mut reopened);
    assert_eq!(mesh_id(&reopened), "im_2");
    let snapshot = support::head(&reopened);
    assert!(PropertyHolder::mesh_is_unbound(snapshot.document(), &old));
    assert_eq!(
        snapshot.sample_property_raw(&old, "vtx8", 1.),
        Err(DispatchErrorCode::Unavailable)
    );
    assert_eq!(
        wire(&reopened)["propertyHolders"],
        detached["propertyHolders"]
    );
    assert_eq!(
        wire(&reopened)["propertyHolders"][0]["properties"],
        value["propertyHolders"][0]["properties"]
    );
    assert!(wire(&reopened)["imageMeshes"].get("im_1").is_none());
}

#[test]
fn admitted_high_store_then_low_replacement_before_any_binding_command_keeps_the_high_water() {
    let low = bytes(&support::owner(&drawing(24, None)));
    let mut admitted = support::owner(&drawing(24, Some("im_900")));
    let high = bytes(&admitted);
    replace(&mut admitted, &low);
    attach(&mut admitted);
    assert_eq!(mesh_id(&admitted), "im_901");
    let mut reopened = NativeObjectHistory::from_project_file("high-reopen", &high).unwrap();
    replace(&mut reopened, &low);
    attach(&mut reopened);
    assert_eq!(mesh_id(&reopened), "im_901");
}

#[test]
fn discarded_or_refused_preparation_does_not_observe_an_incoming_high_identity_or_spend_an_id() {
    let high = bytes(&support::owner(&drawing(24, Some("im_900"))));
    let mut owner = support::owner(&drawing(24, None));
    let initial = support::state(&owner);
    let snapshot = support::head(&owner);
    drop(
        owner
            .prepare_project_replacement(snapshot.snapshot_id(), &high)
            .unwrap(),
    );
    let payload = json!({"command":"object.image.mesh.edit","action":{
        "kind":"attach","target":at(0),"mesh":mesh()}})
    .to_string();
    drop(
        owner
            .prepare_image_mesh_edit_json(payload.as_bytes())
            .unwrap(),
    );
    let invalid = json!({"command":"object.image.mesh.edit","action":{
        "kind":"attach","target":at(99),"mesh":mesh()}})
    .to_string();
    assert_eq!(
        owner
            .prepare_image_mesh_edit_json(invalid.as_bytes())
            .unwrap_err(),
        DispatchErrorCode::InvalidRequest
    );
    assert_eq!(support::state(&owner), initial);
    attach(&mut owner);
    assert_eq!(mesh_id(&owner), "im_1");
    let snapshot = support::head(&owner);
    let stale = owner
        .prepare_project_replacement(snapshot.snapshot_id(), &high)
        .unwrap();
    attach(&mut owner);
    assert_eq!(mesh_id(&owner), "im_2");
    let before = support::state(&owner);
    assert_eq!(
        owner.commit_project_replacement(stale),
        Err(DispatchErrorCode::StaleRevision)
    );
    assert_eq!(support::state(&owner), before);
    attach(&mut owner);
    assert_eq!(mesh_id(&owner), "im_3");
}
