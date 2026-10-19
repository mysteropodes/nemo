//! Actual admitted path/item/mesh holders, through the existing storage,
//! history, raw query and expression providers. Pixel/font resources are not
//! fabricated: these are supported bare drawings, not render acceptance.
use crate::history::NativeObjectHistory;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::object_document::{Presence, PropertyHolderTarget};
use crate::request_receipts::DispatchErrorCode;
use serde_json::{json, Value};

fn object_target(layer: &str, id: &str) -> Value {
    json!({"contextId":"scene-root","frameScope":{"kind":"authored","frame":0},"layerUid":layer,"strokeId":id})
}
fn target(layer: &str, id: Option<&str>) -> PropertyHolderTarget {
    PropertyHolderTarget {
        context_id: "scene-root".into(),
        layer_uid: layer.into(),
        stroke_id: id.map_or(Presence::Missing, |id| Presence::Present(id.into())),
    }
}
fn fixture() -> Value {
    json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":3,
        "layers":[{"layerUid":"L0","name":"Mesh","locked":false},{"layerUid":"L1","name":"Text"}],
        "objects":[{"schemaVersion":1,"family":"closed-solid-fill-cubic-path","target":object_target("L0","s0"),
            "geometry":{"kind":"cubic-path","closed":true,"coordinateSpace":"document-world","handleSpace":"relative",
                "segments":[{"point":{"x":0,"y":0},"handleIn":{"x":0,"y":0},"handleOut":{"x":1,"y":0}},
                {"point":{"x":10,"y":10},"handleIn":{"x":0,"y":-1},"handleOut":{"x":0,"y":0}}]},
            "fill":{"kind":"solid","r":1,"g":0,"b":0,"a":1}}],
        "items":{"raster":[{"target":object_target("L0","r0"),
            "item":{"assetId":"img-1","x":0,"y":0,"width":64,"height":32,"meshId":"im_1"}}],
            "text":[{"target":object_target("L1","t0"),"block":{"text":"Hi","font":{"kind":"bundled","fontKey":"Roboto-Regular"},
                "size":48,"color":"#112233","align":"left","anchorTopLeft":{"x":10,"y":20}}}]},
        "imageMeshes":{"im_1":{"outline":[[0,0],[1,0],[0,1]],"verts":[[0,0],[1,0],[0,1]],"tris":[0,1,2],
            "offsets":[[0.1,0.2],[0.3,0.4],[0.5,0.6]]}},
        "propertyHolders":[
            {"target":{"contextId":"scene-root","layerUid":"L0","strokeId":"s0"},"properties":[{"property":"vtx0","static":[2,3]}]},
            {"target":{"contextId":"scene-root","layerUid":"L0","strokeId":"im_1"},"properties":[{"property":"vtx1","static":[10,-20]}]},
            {"target":{"contextId":"scene-root","layerUid":"L0","strokeId":"r0"},"properties":[{"property":"position","static":[4,5]}]},
            {"target":{"contextId":"scene-root","layerUid":"L1","strokeId":"t0"},"properties":[{"property":"position","static":[6,7]}]}]})
}
fn owner(value: &Value) -> NativeObjectHistory {
    NativeObjectHistory::new(
        "holder-targets",
        decode_storage_project(&serde_json::to_vec(value).unwrap()).unwrap(),
    )
    .unwrap()
}
fn metadata(pin: &crate::revision::ObjectSnapshot, view: Value) -> Value {
    pin.query_property_metadata_json(
        json!({"instanceId":pin.instance_id(),"documentId":pin.document_id(),
        "atRevision":pin.content_revision(),"documentSnapshotId":pin.snapshot_id(),"view":view})
        .to_string()
        .as_bytes(),
    )
    .unwrap()
}

#[test]
fn actual_item_and_mesh_holders_roundtrip_and_keep_vertex_units_separate() {
    let owner = owner(&fixture());
    let pin = owner.acquire_snapshot(0).unwrap();
    for (t, p, want) in [
        (target("L0", Some("s0")), "vtx0", vec![2., 3.]),
        (target("L0", Some("im_1")), "vtx0", vec![0., 0.]),
        (target("L0", Some("im_1")), "vtx1", vec![10., -20.]),
        (target("L0", Some("r0")), "position", vec![4., 5.]),
        (target("L1", Some("t0")), "position", vec![6., 7.]),
    ] {
        assert_eq!(pin.sample_property_raw(&t, p, 0.).unwrap(), want);
    }
    for (id, unit) in [("s0", "px"), ("im_1", "%")] {
        let r = metadata(
            &pin,
            json!({"kind":"properties","target":target("L0",Some(id)),"offset":6,"limit":8}),
        );
        assert!(r["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["property"] == "vtx0" && p["unit"] == unit));
    }
    let bytes = encode_storage_project(pin.document()).unwrap();
    let again = decode_storage_project(&bytes).unwrap();
    assert_eq!(encode_storage_project(&again).unwrap(), bytes);
    assert_eq!(
        again
            .image_meshes()
            .unwrap()
            .get("im_1")
            .unwrap()
            .effective_offsets(),
        vec![[0.1, 0.2], [0.3, 0.4], [0.5, 0.6]]
    );
}

#[test]
fn animated_vertex_tracks_read_through_the_same_pinned_query_without_render_fallback() {
    let mut v = fixture();
    v["propertyHolders"][1]["properties"][0]["track"] = json!({"keys":[
        {"frame":0,"v":[10,20],"curvePoints":[]},
        {"frame":2,"v":[30,60],"hold":false,"curvePoints":null}]});
    let owner = owner(&v);
    let pin = owner.acquire_snapshot(0).unwrap();
    let t = target("L0", Some("im_1"));
    assert_eq!(
        pin.sample_property_raw(&t, "vtx1", 1.).unwrap(),
        vec![20., 40.]
    );
    let mut q = json!({"instanceId":pin.instance_id(),"documentId":pin.document_id(),"atRevision":0,
        "documentSnapshotId":pin.snapshot_id(),"view":{"kind":"authoredKeys","target":t,"property":"vtx1","offset":0,"limit":8}});
    let read = pin
        .query_property_read_json(q.to_string().as_bytes())
        .unwrap();
    assert_eq!(
        read["items"],
        v["propertyHolders"][1]["properties"][0]["track"]["keys"]
    );
    q["documentSnapshotId"] = json!("foreign");
    assert_eq!(
        pin.query_property_read_json(q.to_string().as_bytes()),
        Err(DispatchErrorCode::StaleRevision)
    );
    use crate::object_frame_packet::{prepare_object_frame, ObjectFrameError, ObjectFrameSelector};
    let selector = ObjectFrameSelector {
        instance_id: pin.instance_id(),
        document_id: pin.document_id(),
        content_revision: 0,
        document_snapshot_id: pin.snapshot_id(),
        context_id: "scene-root",
        scope_kind: crate::object_document::FrameScopeKind::Authored,
        frame: 0,
    };
    assert_eq!(
        prepare_object_frame(&pin, &selector).unwrap_err(),
        ObjectFrameError::Unavailable,
        "unjoined mesh render consumer must refuse rather than silently discard animation"
    );
    assert_eq!(owner.history_depths(), (0, 0));
}

#[test]
fn target_pages_discover_only_real_frame_independent_item_and_mesh_keys() {
    let owner = owner(&fixture());
    let pin = owner.acquire_snapshot(0).unwrap();
    let mut offset = 0;
    let mut all = vec![];
    loop {
        let page = metadata(&pin, json!({"kind":"targets","offset":offset,"limit":2}));
        assert!(page["items"].as_array().unwrap().len() <= 2);
        all.extend(page["items"].as_array().unwrap().clone());
        let Some(next) = page["nextOffset"].as_u64() else {
            break;
        };
        assert!(next > offset);
        offset = next;
    }
    for id in ["s0", "r0", "im_1", "t0"] {
        assert_eq!(
            all.iter().filter(|v| v["target"]["strokeId"] == id).count(),
            1,
            "{id}"
        );
    }
    let mesh = target("L0", Some("im_1"));
    let mut names = vec![];
    for offset in [0, 8] {
        let page = metadata(
            &pin,
            json!({"kind":"properties","target":mesh,"offset":offset,"limit":8}),
        );
        names.extend(
            page["items"]
                .as_array()
                .unwrap()
                .iter()
                .map(|p| p["property"].as_str().unwrap().to_owned()),
        );
    }
    assert_eq!(&names[6..], ["vtx0", "vtx1", "vtx2"]);
}

#[test]
fn native_mesh_vertex_edits_keep_rest_pose_and_survive_history_and_reload() {
    let mut owner = owner(&fixture());
    let old = owner.acquire_snapshot(0).unwrap();
    let mesh = target("L0", Some("im_1"));
    let edit = json!({"command":"object.property.edit","target":mesh,"property":"vtx0",
        "action":{"kind":"setValue","frame":0,"values":[25,-50]}});
    let candidate = owner
        .prepare_property_edit_json(edit.to_string().as_bytes())
        .unwrap();
    owner.commit_property_edit(candidate).unwrap();
    let current = owner.acquire_snapshot(1).unwrap();
    assert_eq!(
        old.sample_property_raw(&mesh, "vtx0", 0.).unwrap(),
        vec![0., 0.]
    );
    assert_eq!(
        current.sample_property_raw(&mesh, "vtx0", 0.).unwrap(),
        vec![25., -50.]
    );
    assert_eq!(
        old.document().image_meshes(),
        current.document().image_meshes()
    );
    let bytes = encode_storage_project(current.document()).unwrap();
    let reopened =
        NativeObjectHistory::new("reopened", decode_storage_project(&bytes).unwrap()).unwrap();
    assert_eq!(
        reopened
            .acquire_snapshot(0)
            .unwrap()
            .sample_property_raw(&mesh, "vtx0", 0.)
            .unwrap(),
        vec![25., -50.]
    );
    owner.undo(1).unwrap();
    assert_eq!(
        owner
            .acquire_snapshot(2)
            .unwrap()
            .sample_property_raw(&mesh, "vtx0", 0.)
            .unwrap(),
        vec![0., 0.]
    );
    owner.redo(2).unwrap();
    assert_eq!(
        owner
            .acquire_snapshot(3)
            .unwrap()
            .sample_property_raw(&mesh, "vtx0", 0.)
            .unwrap(),
        vec![25., -50.]
    );
}

#[test]
fn foreign_missing_and_reference_only_mesh_bindings_do_not_invent_a_holder() {
    let owner = owner(&fixture());
    let pin = owner.acquire_snapshot(0).unwrap();
    assert_eq!(
        pin.sample_property_raw(&target("L1", Some("im_1")), "vtx0", 0.),
        Err(DispatchErrorCode::NotFound)
    );
    assert_eq!(
        pin.sample_property_raw(&target("L0", None), "vtx0", 0.),
        Err(DispatchErrorCode::Unavailable)
    );
    for mutate in [0, 1, 2] {
        let mut v = fixture();
        match mutate {
            0 => {
                v.as_object_mut().unwrap().remove("items");
            }
            1 => v["items"]["raster"][0]["target"]["frameScope"]["kind"] = json!("reference"),
            _ => v["items"]["raster"][0]["item"]["meshId"] = json!("foreign"),
        }
        assert!(decode_storage_project(v.to_string().as_bytes()).is_err());
    }
}

#[test]
fn old_authored_vertex_tracks_are_not_discarded_after_a_topology_shrink() {
    let mut v = fixture();
    v["propertyHolders"][1]["properties"]
        .as_array_mut()
        .unwrap()
        .push(json!({"property":"vtx99","static":[8,9]}));
    v["propertyHolders"][1]["properties"]
        .as_array_mut()
        .unwrap()
        .push(json!({"property":"vtx01","static":[11,12]}));
    let owner = owner(&v);
    let pin = owner.acquire_snapshot(0).unwrap();
    let t = target("L0", Some("im_1"));
    assert_eq!(
        pin.sample_property_raw(&t, "vtx99", 0.).unwrap(),
        vec![8., 9.]
    );
    assert_eq!(
        pin.sample_property_raw(&t, "vtx01", 0.).unwrap(),
        vec![11., 12.]
    );
    assert_eq!(
        pin.sample_property_raw(&t, "vtx98", 0.),
        Err(DispatchErrorCode::Unavailable)
    );
    assert_eq!(
        decode_storage_project(&encode_storage_project(pin.document()).unwrap()).unwrap(),
        *pin.document()
    );
}

#[cfg(feature = "object_expression_runtime")]
#[test]
fn mesh_context_reads_actual_rest_vertices_and_keeps_shape_and_mesh_distinct() {
    use crate::object_expression_runtime::{evaluate_property_for_owner, ExpressionLimits};
    let mut v = fixture();
    v["propertyHolders"][1]["properties"][0]["expression"] = json!({"enabled":true,
        "code":"self.isMesh && !self.isShape && self.isOutlineVertex ? [self.vertexIndex,self.vertexCount] : value"});
    let owner = owner(&v);
    let pin = owner.acquire_snapshot(0).unwrap();
    let t = target("L0", Some("im_1"));
    let result = evaluate_property_for_owner(
        &owner,
        &pin,
        &t,
        "vtx1",
        0.,
        0,
        ExpressionLimits::new(4096, 8_388_608, 262_144, 32).unwrap(),
    )
    .unwrap();
    assert_eq!(result.value, vec![1., 3.]);
    assert_eq!(result.diagnostic, None);
    let q = json!({"instanceId":pin.instance_id(),"documentId":pin.document_id(),"atRevision":0,
        "documentSnapshotId":pin.snapshot_id(),"view":{"kind":"expressionContext","target":t,"property":"vtx1","frame":0}});
    let r = pin
        .query_property_read_json(q.to_string().as_bytes())
        .unwrap();
    assert_eq!(r["mesh"]["vertexUV"], json!([1., 0.]));
    assert_eq!(r["mesh"]["vertexCount"], 3);
    assert_eq!(r["self"]["isShape"], false);
    assert!(r["comp"].is_null());
    assert_eq!(owner.history_depths(), (0, 0));
}

#[test]
fn codec_control_identity_uses_the_same_pinned_unicode_rules_as_lookup() {
    let mut v = fixture();
    let controls = json!([
        {"key":"k1","name":"ΟΣ","type":"number"},
        {"key":"k2","name":"ος","type":"number"}]);
    v["propertyHolders"][0]["controls"] = controls;
    assert!(decode_storage_project(v.to_string().as_bytes()).is_err());
    // Node22 Unicode16 leaves A7CE unmapped; Unicode17 Rust lowercases it.
    v["propertyHolders"][0]["controls"][0]["name"] = json!("\u{a7ce}");
    v["propertyHolders"][0]["controls"][1]["name"] = json!("\u{a7cf}");
    let decoded = decode_storage_project(v.to_string().as_bytes()).unwrap();
    assert_eq!(
        decoded.property_holders().unwrap()[0]
            .controls
            .value()
            .unwrap()
            .len(),
        2
    );
}
