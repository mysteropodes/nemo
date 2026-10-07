//! Independent object data and real shared host installation/lifecycle controls.
use crate::application_mcp::ApplicationMcp;
use native_engine::object_codec::decode_project;
use serde_json::{json, Value};

fn fixture() -> Value {
    let cases: Value = serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    )).unwrap();
    json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":21,
        "layers":[{"layerUid":"legacy layer:alpha"},{"layerUid":"0"}],"objects":cases["records"]})
}

#[test]
fn production_installer_owns_the_host_slot_at_fresh_revision_zero() {
    let state = ApplicationMcp::default();
    let document = decode_project(&serde_json::to_vec(&fixture()).unwrap()).unwrap();
    state.install_native_object(document).unwrap();
    let native = state.native_state();
    let authority = native.lock().unwrap();
    let (_, owner) = authority.active().unwrap();
    assert_eq!(owner.instance_id(), state.instance_id());
    assert!(owner.document_id().starts_with("native-object-document-"));
    assert_eq!(owner.content_revision(), 0);
}
