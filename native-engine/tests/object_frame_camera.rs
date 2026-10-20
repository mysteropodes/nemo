//! Actual project storage -> retained native camera/3D inputs, never supplied poses.
use crate::evaluation::property_projection_3d::{make_projector, Bounds};
use crate::history::NativeObjectHistory;
use crate::object_document::PropertyHolderTarget;
use crate::object_snapshot::ObjectSnapshot;
use serde_json::{json, Value};

fn source() -> Value {
    json!({"format":"nemo.native-project-document","formatVersion":1,
        "project":{"canvasW":320,"canvasH":180,"fps":24,"cameraLayerOn":true,
            "cameraKeys":[{"frame":0,"x":160,"y":90,"w":320},{"frame":4,"x":160,"y":90,"w":160}]},
        "drawing":{"format":"nemo.native-object-document","formatVersion":1,"totalFrames":8,
            "layers":[{"layerUid":"L0","threeD":true}],"objects":[],
            "propertyHolders":[{"target":{"contextId":"scene-root","layerUid":"L0"},"properties":[
                {"property":"position","static":[0,0]}, {"property":"scale","static":[100,100]},
                {"property":"positionZ","track":{"keys":[{"frame":0,"v":[0]},{"frame":4,"v":[2000]}]}},
                {"property":"rotationX","static":[0]}, {"property":"rotationY","static":[0]}]}]},
        "viewportReference":null,"assets":[]})
}
fn owner(source: &Value) -> NativeObjectHistory {
    NativeObjectHistory::from_project_file("stored-camera", &serde_json::to_vec(source).unwrap())
        .unwrap()
}
fn snapshot(o: &NativeObjectHistory) -> ObjectSnapshot {
    o.acquire_snapshot(o.content_revision()).unwrap()
}
fn saved(o: &NativeObjectHistory) -> Vec<u8> {
    o.serialize_project_file(o.content_revision(), snapshot(o).snapshot_id())
        .unwrap()
}
fn target() -> PropertyHolderTarget {
    serde_json::from_value(json!({"contextId":"scene-root","layerUid":"L0"})).unwrap()
}
fn near(a: f64, b: f64) {
    assert!((a - b).abs() <= 1e-10, "{a} != {b}");
}

#[test]
fn stored_keyframes_feed_the_original_camera_and_3d_kernels_on_one_snapshot() {
    let o = owner(&source());
    let before = saved(&o);
    let s = snapshot(&o);
    assert_eq!(
        s.sampled_camera_matrix(0.).unwrap().unwrap(),
        [1., 0., 0., 1., 0., 0.]
    );
    let matrix = s.sampled_camera_matrix(2.).unwrap().unwrap();
    for (a, b) in matrix
        .into_iter()
        .zip([4. / 3., 0., 0., 4. / 3., -160. / 3., -30.])
    {
        near(a, b);
    }
    let pose = s.sampled_pose_3d(&target(), 2.).unwrap().unwrap();
    assert_eq!(pose.position_z, 1000.);
    let projector = make_projector(
        &pose,
        &Bounds {
            x: 0.,
            y: 0.,
            width: 320.,
            height: 180.,
        },
        320.,
        180.,
        None,
    );
    let p = projector.project(320., 180.);
    near(p[0], 160. + 160. / 1.5);
    near(p[1], 90. + 90. / 1.5);
    assert_eq!(saved(&o), before);
    assert_eq!(o.history_depths(), (0, 0));
    let reopened = NativeObjectHistory::from_project_file("fresh-camera", &before).unwrap();
    assert_eq!(saved(&reopened), before);
    assert_eq!(
        snapshot(&reopened).sampled_camera_matrix(2.).unwrap(),
        Some(matrix)
    );
    assert_eq!(
        snapshot(&reopened).sampled_pose_3d(&target(), 2.).unwrap(),
        Some(pose)
    );
}

#[test]
fn disabled_flags_do_not_activate_old_keys_and_absence_does_not_author_defaults() {
    let mut value = source();
    value["project"]["cameraLayerOn"] = json!(false);
    value["drawing"]["layers"][0]["threeD"] = json!(false);
    let o = owner(&value);
    let s = snapshot(&o);
    assert_eq!(s.sampled_camera_matrix(2.).unwrap(), None);
    assert_eq!(s.sampled_pose_3d(&target(), 2.).unwrap(), None);
    value["project"]
        .as_object_mut()
        .unwrap()
        .remove("cameraKeys");
    value["project"]
        .as_object_mut()
        .unwrap()
        .remove("cameraLayerOn");
    value["drawing"]["layers"][0]
        .as_object_mut()
        .unwrap()
        .remove("threeD");
    let absent: Value = serde_json::from_slice(&saved(&owner(&value))).unwrap();
    assert!(absent["project"].get("cameraKeys").is_none());
    assert!(absent["project"].get("cameraLayerOn").is_none());
    assert!(absent["drawing"]["layers"][0].get("threeD").is_none());
    value["project"]["cameraKeys"] = json!([]);
    let empty: Value = serde_json::from_slice(&saved(&owner(&value))).unwrap();
    assert_eq!(empty["project"]["cameraKeys"], json!([]));
}

#[test]
fn strict_camera_and_3d_wire_shapes_and_duplicate_keys_remain_closed() {
    for (path, bad) in [
        ("cameraKeys", json!([[0, 160, 90, 320]])),
        (
            "cameraKeys",
            json!([{"frame":0,"x":160,"y":90,"w":320,"foreign":1}]),
        ),
        ("cameraLayerOn", json!(1)),
        (
            "cameraKeys",
            json!([{"frame":0,"x":160,"y":90,"w":320,"hOut":[0,0,0]}]),
        ),
    ] {
        let mut value = source();
        value["project"][path] = bad;
        assert!(NativeObjectHistory::from_project_file(
            "bad",
            &serde_json::to_vec(&value).unwrap()
        )
        .is_err());
    }
    let mut value = source();
    value["drawing"]["layers"][0]["threeD"] = json!("true");
    assert!(
        NativeObjectHistory::from_project_file("bad", &serde_json::to_vec(&value).unwrap())
            .is_err()
    );
    let duplicate = source().to_string().replace(
        "\"cameraLayerOn\":true",
        "\"cameraLayerOn\":true,\"cameraLayerOn\":false",
    );
    assert!(NativeObjectHistory::from_project_file("duplicate", duplicate.as_bytes()).is_err());
}

#[test]
fn expressions_and_nonfinite_projection_do_not_fall_back_to_raw_or_identity() {
    let mut value = source();
    value["drawing"]["propertyHolders"][0]["properties"][2]["expression"] =
        json!({"enabled":true,"code":"2000"});
    let o = owner(&value);
    assert!(snapshot(&o).sampled_pose_3d(&target(), 2.).is_err());
    value = source();
    value["project"]["cameraKeys"][0]["w"] = json!(0);
    let o = owner(&value);
    assert!(snapshot(&o).sampled_camera_matrix(0.).is_err());
    assert!(snapshot(&o).sampled_camera_matrix(f64::NAN).is_err());
}

#[test]
fn component_targets_cannot_borrow_an_opposite_root_layer_3d_flag() {
    // The native component fixture is an actually admitted context, not an
    // unknown context that would fail target validation for another reason.
    let line = include_str!("fixtures/object-component-document/oracle.txt")
        .lines()
        .find(|line| line.starts_with("G\t"))
        .unwrap();
    let drawing: Value = serde_json::from_str(line.split('\t').nth(1).unwrap()).unwrap();
    let component = &drawing["componentTables"]["components"]["components"][0];
    let uid = component["layers"][0]["layerUid"].clone();
    let context = component["symbolId"].clone();
    for enabled in [false, true] {
        let mut value = source();
        value["drawing"] = drawing.clone();
        value["drawing"]["componentTables"]["components"]["components"][0]["layers"][0]["threeD"] =
            json!(!enabled);
        if let Some(index) = value["drawing"]["layers"]
            .as_array()
            .unwrap()
            .iter()
            .position(|l| l["layerUid"] == uid)
        {
            value["drawing"]["layers"][index]["threeD"] = json!(enabled);
        } else {
            // Add a real colliding root layer with explicit order membership.
            let index = value["drawing"]["layers"].as_array().unwrap().len();
            value["drawing"]["layers"]
                .as_array_mut()
                .unwrap()
                .push(json!({"layerUid":uid,"threeD":enabled}));
            let root_context = value["drawing"]["compositionOrder"]["contexts"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|c| c["contextId"] == "scene-root")
                .unwrap();
            root_context["layers"]
                .as_array_mut()
                .unwrap()
                .push(json!({"layerUid":uid,"naturalPosition":index,"order":{}}));
        }
        let o = owner(&value);
        let target = serde_json::from_value(json!({"contextId":context,"layerUid":uid})).unwrap();
        assert_eq!(
            snapshot(&o).sampled_pose_3d(&target, 0.),
            Err(crate::object_frame_packet::PropertySampleError::UnsupportedProperty)
        );
    }
}
