//! Native immutable read controls. The token is a unit-query input; successful
//! desktop surface ownership is separately enforced by the host. No GPU is made.
#![cfg(feature = "application")]
use super::{ImageMeshReadFailure, ImageMeshReadProjection};
use crate::history::NativeObjectHistory;
use crate::object_document::FrameScopeKind;
use crate::object_frame_packet::{prepare_object_exposure_frame, ObjectFrameSelector};
use crate::object_render_scene::{
    prepare_object_paint_plan, prepare_object_render_scene, prepare_object_viewport_composition,
    ObjectCompositionWitness,
};
use crate::request_receipts::{DispatchErrorCode, OpacityRequest, ResponseEnvelope};
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};
#[path = "object_image_mesh_binding_fixtures.rs"]
mod fixtures;
#[path = "object_raster_asset_commands_support.rs"]
mod support;
use fixtures::pixel_consumer::{composition, fixture};

fn apply(owner: &mut NativeObjectHistory, payload: Value) {
    let candidate = owner
        .prepare_image_mesh_edit_json(payload.to_string().as_bytes())
        .unwrap();
    owner.commit_image_mesh_edit(candidate).unwrap();
}
fn attach(owner: &mut NativeObjectHistory, mesh: Value) {
    apply(
        owner,
        fixtures::payload(json!({"kind":"attach","target":fixtures::at(0),"mesh":mesh})),
    );
}
fn saved(owner: &NativeObjectHistory) -> Vec<u8> {
    let snapshot = support::head(owner);
    owner
        .serialize_project_file(owner.content_revision(), snapshot.snapshot_id())
        .unwrap()
}
fn drawing(owner: &NativeObjectHistory) -> Value {
    serde_json::from_slice(&support::bytes(owner)).unwrap()
}
fn presented_projection(
    witness: &ObjectCompositionWitness,
    layer: &str,
) -> ImageMeshReadProjection {
    witness
        .image_mesh_read_projection(layer)
        .unwrap()
        .with_presentation("native-work-9".into(), 3)
        .unwrap()
}
fn request(snapshot: &ObjectSnapshot, frame: u32, layer: &str, page: Value) -> OpacityRequest {
    serde_json::from_value(json!({"apiVersion":2,"requestId":"mesh-read","instanceId":snapshot.instance_id(),
        "documentId":snapshot.document_id(),"operation":"query.document.object","payload":{
            "atRevision":snapshot.content_revision(),"documentSnapshotId":snapshot.snapshot_id(),"view":{
                "kind":"imageMesh","contextId":"scene-root","layerUid":layer,"frame":frame,
                "workId":"native-work-9","viewGeneration":3,"page":page}}})).unwrap()
}
fn dispatch(
    snapshot: &ObjectSnapshot,
    projection: &ImageMeshReadProjection,
    request: OpacityRequest,
) -> ResponseEnvelope {
    assert!(serde_json::to_vec(&request).unwrap().len() <= 4096);
    let response = snapshot
        .dispatch_with_image_mesh_view(request, Ok(projection))
        .unwrap();
    assert!(serde_json::to_vec(&response).unwrap().len() <= 4096);
    response
}
fn read(
    snapshot: &ObjectSnapshot,
    projection: &ImageMeshReadProjection,
    frame: u32,
    page: Value,
) -> Value {
    let response = dispatch(snapshot, projection, request(snapshot, frame, "L0", page));
    assert!(response.ok, "{:?}", response.error);
    assert_eq!(response.content_revision, snapshot.content_revision());
    response.result.unwrap()
}
fn expect_error(response: ResponseEnvelope, code: DispatchErrorCode) {
    assert!(!response.ok);
    assert!(response.result.is_none());
    assert_eq!(response.error.unwrap().code(), code);
}
fn rect(value: &Value) -> [f64; 5] {
    ["x", "y", "width", "height", "rotation"].map(|key| value["rect"][key].as_f64().unwrap())
}
fn data_page(kind: &str, mesh: &str, cursor: u64) -> Value {
    json!({"kind":kind,"meshId":mesh,"cursor":cursor,"limit":16})
}

// These read fixtures need the explicit v2 order required by the real paint producer.
fn ordered_rasters(mut document: Value) -> Value {
    let mut scopes = std::collections::BTreeMap::<u32, Vec<Value>>::new();
    for entry in document["items"]["raster"].as_array().unwrap() {
        let target = &entry["target"];
        let elements = scopes
            .entry(target["frameScope"]["frame"].as_u64().unwrap() as u32)
            .or_default();
        elements.push(fixtures::element(
            target["layerUid"].as_str().unwrap(),
            target["strokeId"].as_str().unwrap(),
            elements.len() as u32,
        ));
    }
    let layers: Vec<_> = document["layers"].as_array().unwrap().iter().enumerate()
        .map(|(index, layer)| json!({"layerUid":layer["layerUid"],"naturalPosition":index,"order":{}})).collect();
    let scopes: Vec<_> = scopes
        .into_iter()
        .map(|(frame, elements)| fixtures::scope(frame, elements))
        .collect();
    document["formatVersion"] = json!(2);
    document["compositionOrder"] = json!({"format":"nemo.native-object-order-plan","formatVersion":1,
        "documentFormatVersion":2,"migration":"explicit-plan-required","referencePolicy":"unresolved",
        "contexts":[{"contextId":"scene-root","layers":layers,"scopes":scopes}]});
    document
}

#[test]
fn retained_frame12_rect_is_final_d30_once_and_independent_of_view_zoom() {
    let (mut owner, mesh) = fixture(false);
    attach(&mut owner, mesh);
    let before = support::state(&owner);
    let file = saved(&owner);
    let snapshot = support::head(&owner);
    let stored = drawing(&owner)["items"]["raster"][12]["item"].clone();
    assert_eq!(
        (stored["x"].as_f64(), stored["y"].as_f64()),
        (Some(320.), Some(180.))
    );
    let mut results = Vec::new();
    for zoom in [1., 0.75] {
        let witness = composition(&snapshot, 12, true, zoom);
        let projection = presented_projection(&witness, "L0");
        let value = read(&snapshot, &projection, 12, json!({"kind":"selection"}));
        // Stored (320,180) is the center; display_rect starts at (220,80), then Motion adds (40,20).
        assert_eq!(rect(&value), [260., 100., 200., 200., 0.]);
        assert_ne!(rect(&value)[..2], [220., 80.]);
        assert_ne!(
            rect(&value)[..2],
            [300., 120.],
            "the layer delta must not be applied twice"
        );
        assert_eq!(value["target"], fixtures::at(12));
        assert_eq!(value["mesh"]["meshId"], "im_1");
        assert_eq!(value["contextId"], "scene-root");
        assert_eq!(value["layerUid"], "L0");
        assert_eq!(value["frame"], 12);
        assert_eq!(value["workId"], "native-work-9");
        assert_eq!(value["viewGeneration"], 3);
        results.push(value);
    }
    assert_eq!(
        results[0], results[1],
        "navigation must not transform the retained document rectangle"
    );
    assert_eq!(support::state(&owner), before);
    assert_eq!(saved(&owner), file);
}

#[test]
fn authored_absence_and_an_unbound_actual_raster_are_preserved() {
    let mut document = fixtures::drawing(1, Some("im_1"));
    for key in ["cols", "rows", "offsets"] {
        document["imageMeshes"]["im_1"]
            .as_object_mut()
            .unwrap()
            .remove(key);
    }
    let document = ordered_rasters(document);
    let owner = support::owner(&document);
    let snapshot = support::head(&owner);
    let witness = composition(&snapshot, 0, true, 1.);
    let projection = presented_projection(&witness, "L0");
    let value = read(&snapshot, &projection, 0, json!({"kind":"selection"}));
    assert_eq!(
        value["mesh"],
        json!({"meshId":"im_1","outlineCount":8,"vertexCount":9,
        "triangleIndexCount":24,"offsetsPresent":false})
    );
    expect_error(
        dispatch(
            &snapshot,
            &projection,
            request(&snapshot, 0, "L0", data_page("offsets", "im_1", 0)),
        ),
        DispatchErrorCode::NotFound,
    );
    assert_eq!(drawing(&owner)["imageMeshes"], document["imageMeshes"]);
    let owner = support::owner(&ordered_rasters(fixtures::drawing(1, None)));
    let snapshot = support::head(&owner);
    let witness = composition(&snapshot, 0, true, 1.);
    let projection = presented_projection(&witness, "L0");
    let value = read(&snapshot, &projection, 0, json!({"kind":"selection"}));
    assert!(value["mesh"].is_null());
    assert_eq!(value["target"], fixtures::at(0));
    expect_error(
        dispatch(
            &snapshot,
            &projection,
            request(&snapshot, 0, "L0", data_page("verts", "im_1", 0)),
        ),
        DispatchErrorCode::NotFound,
    );
}

#[test]
fn full_dense_topology_is_assembled_in_bounded_pages_without_spending_state_or_ids() {
    let (mut owner, mesh) = fixture(false);
    attach(&mut owner, mesh.clone());
    apply(
        &mut owner,
        json!({"command":"object.image.mesh.edit","meshId":"im_1",
        "action":{"kind":"setDensity","cols":32,"rows":32}}),
    );
    let expected = drawing(&owner)["imageMeshes"]["im_1"].clone();
    assert!(serde_json::to_vec(&expected).unwrap().len() > 4096);
    assert_eq!(expected["cols"], 32);
    assert_eq!(expected["rows"], 32);
    let before = support::state(&owner);
    let file = saved(&owner);
    let snapshot = support::head(&owner);
    let witness = composition(&snapshot, 12, true, 0.75);
    let projection = presented_projection(&witness, "L0");
    let selected = read(&snapshot, &projection, 12, json!({"kind":"selection"}));
    let metadata = &selected["mesh"];
    for (kind, count) in [
        ("outline", "outlineCount"),
        ("verts", "vertexCount"),
        ("tris", "triangleIndexCount"),
        ("offsets", "vertexCount"),
    ] {
        if kind == "offsets" && metadata["offsetsPresent"] == false {
            continue;
        }
        let mut cursor = 0;
        let mut assembled = Vec::new();
        loop {
            let value = read(&snapshot, &projection, 12, data_page(kind, "im_1", cursor));
            assert_eq!(value["target"], selected["target"]);
            assert_eq!(value["rect"], selected["rect"]);
            assert_eq!(value["mesh"], selected["mesh"]);
            assert_eq!(value["page"]["total"], metadata[count]);
            assert_eq!(value["page"]["cursor"], cursor);
            let items = value["page"]["items"].as_array().unwrap();
            assert!(items.len() <= 16);
            assembled.extend_from_slice(items);
            if value["page"]["nextCursor"].is_null() {
                assert_eq!(assembled.len() as u64, metadata[count].as_u64().unwrap());
                break;
            }
            assert!(!items.is_empty());
            let next = value["page"]["nextCursor"].as_u64().unwrap();
            assert_eq!(next, cursor + items.len() as u64);
            assert!(next > cursor);
            cursor = next;
        }
        assert_eq!(
            Value::Array(assembled),
            expected[kind],
            "all original {kind} entries survive"
        );
    }
    assert_eq!(support::state(&owner), before);
    assert_eq!(saved(&owner), file);
    let mut control = NativeObjectHistory::from_project_file("mesh-read-control", &file).unwrap();
    attach(&mut owner, mesh.clone());
    attach(&mut control, mesh);
    assert_eq!(
        drawing(&owner)["items"]["raster"][0]["item"]["meshId"],
        "im_2"
    );
    assert_eq!(
        drawing(&owner)["imageMeshes"],
        drawing(&control)["imageMeshes"]
    );
}

#[test]
fn stale_snapshot_frame_and_presentation_tokens_refuse_the_retained_old_projection() {
    let (mut owner, mesh) = fixture(false);
    attach(&mut owner, mesh);
    let old = support::head(&owner);
    let witness = composition(&old, 12, true, 1.);
    let projection = presented_projection(&witness, "L0");
    let original = request(&old, 12, "L0", json!({"kind":"selection"}));
    for (path, replacement) in [
        ("/atRevision", json!(99)),
        ("/documentSnapshotId", json!("native-object:stale:0")),
        ("/view/frame", json!(13)),
        ("/view/workId", json!("another-work")),
        ("/view/viewGeneration", json!(4)),
    ] {
        let mut changed = original.clone();
        *changed.payload.pointer_mut(path).unwrap() = replacement;
        expect_error(
            dispatch(&old, &projection, changed),
            DispatchErrorCode::StaleRevision,
        );
    }
    apply(
        &mut owner,
        json!({"command":"object.image.mesh.edit","meshId":"im_1",
        "action":{"kind":"setOffset","index":8,"du":0.1,"dv":0}}),
    );
    let current = support::head(&owner);
    expect_error(
        dispatch(
            &current,
            &projection,
            request(&current, 12, "L0", json!({"kind":"selection"})),
        ),
        DispatchErrorCode::StaleRevision,
    );
    let before = support::state(&owner);
    expect_error(
        old.dispatch(original).unwrap(),
        DispatchErrorCode::Unavailable,
    );
    assert_eq!(support::state(&owner), before);
}

#[test]
fn closed_selectors_and_invalid_or_unknown_pages_are_typed_refusals() {
    let (mut owner, mesh) = fixture(false);
    attach(&mut owner, mesh);
    let before = support::state(&owner);
    let snapshot = support::head(&owner);
    let witness = composition(&snapshot, 12, true, 1.);
    let projection = presented_projection(&witness, "L0");
    for page in [
        json!({"kind":"unknown"}),
        json!({"kind":"selection","meshId":"im_1"}),
        json!({"kind":"verts","meshId":"im_1","cursor":0,"limit":0}),
        json!({"kind":"verts","meshId":"im_1","cursor":0,"limit":17}),
        json!({"kind":"verts","meshId":"im_1","cursor":0,"limit":16,"unknown":true}),
        json!({"kind":"verts","meshId":"im_1","cursor":99999,"limit":16}),
        json!({"kind":"verts","meshId":"im_1","cursor":0.5,"limit":16}),
        json!({"kind":"verts","meshId":"im_1","limit":16}),
    ] {
        expect_error(
            dispatch(&snapshot, &projection, request(&snapshot, 12, "L0", page)),
            DispatchErrorCode::InvalidRequest,
        );
    }
    for path in ["", "/view"] {
        let mut changed = request(&snapshot, 12, "L0", json!({"kind":"selection"}));
        changed.payload.pointer_mut(path).unwrap()["unknown"] = json!(true);
        expect_error(
            dispatch(&snapshot, &projection, changed),
            DispatchErrorCode::InvalidRequest,
        );
    }
    let mut mixed = request(&snapshot, 12, "L0", json!({"kind":"selection"}));
    mixed.payload["stableTarget"] = fixtures::at(12);
    expect_error(
        dispatch(&snapshot, &projection, mixed),
        DispatchErrorCode::InvalidRequest,
    );
    expect_error(
        dispatch(
            &snapshot,
            &projection,
            request(&snapshot, 12, "L0", data_page("verts", "unknown-mesh", 0)),
        ),
        DispatchErrorCode::NotFound,
    );
    let mut wrong_layer = request(&snapshot, 12, "L2", json!({"kind":"selection"}));
    expect_error(
        dispatch(&snapshot, &projection, wrong_layer.clone()),
        DispatchErrorCode::Unavailable,
    );
    wrong_layer.payload["view"]["contextId"] = json!("component-other");
    expect_error(
        dispatch(&snapshot, &projection, wrong_layer),
        DispatchErrorCode::Unavailable,
    );
    assert_eq!(support::state(&owner), before);
}

#[test]
fn real_witness_missing_ambiguous_and_artwork_sources_keep_distinct_refusals() {
    let mut document = fixtures::drawing(1, None);
    fixtures::push_raster(
        &mut document,
        fixtures::target("scene-root", "L0", 0, "second"),
        None,
    );
    let document = ordered_rasters(document);
    let owner = support::owner(&document);
    let snapshot = support::head(&owner);
    let witness = composition(&snapshot, 0, true, 1.);
    let ambiguous = witness.image_mesh_read_projection("L0").unwrap_err();
    assert!(matches!(ambiguous, ImageMeshReadFailure::Ambiguous));
    let missing = witness.image_mesh_read_projection("L2").unwrap_err();
    assert!(matches!(missing, ImageMeshReadFailure::Missing));
    for (layer, reason, code) in [
        ("L0", ambiguous, DispatchErrorCode::Unavailable),
        ("L2", missing, DispatchErrorCode::NotFound),
    ] {
        expect_error(
            snapshot
                .dispatch_with_image_mesh_view(
                    request(&snapshot, 0, layer, json!({"kind":"selection"})),
                    Err(reason),
                )
                .unwrap(),
            code,
        );
    }
    let artwork = composition(&snapshot, 0, false, 1.);
    assert!(matches!(
        artwork.image_mesh_read_projection("L0"),
        Err(ImageMeshReadFailure::Unsupported)
    ));
}

#[test]
fn real_sparse_exposure_keeps_held_source_target_and_the_presented_frame() {
    let mut document = fixtures::drawing(24, Some("im_1"));
    document["items"]["raster"] = json!([fixtures::entry(fixtures::at(0), Some("im_1"))]);
    document["layers"] = json!([{"layerUid":"L0","timing":{"format":"nemo.native-layer-timing",
        "formatVersion":1,"family":"drawing","contextId":"scene-root","keys":[{"frame":0}],"samples":[]}}]);
    let document = ordered_rasters(document);
    let owner = support::owner(&document);
    let snapshot = support::head(&owner);
    let selector = ObjectFrameSelector {
        instance_id: snapshot.instance_id(),
        document_id: snapshot.document_id(),
        content_revision: snapshot.content_revision(),
        document_snapshot_id: snapshot.snapshot_id(),
        context_id: "scene-root",
        scope_kind: FrameScopeKind::Authored,
        frame: 12,
    };
    let packet = prepare_object_exposure_frame(&snapshot, &selector).unwrap();
    assert_eq!(packet.source_frame_for_layer(0), Some(0));
    let scene = prepare_object_render_scene(&packet, &selector).unwrap();
    let plan = prepare_object_paint_plan(&scene, &selector).unwrap();
    let witness = prepare_object_viewport_composition(&plan, &selector, None).unwrap();
    let projection = presented_projection(&witness, "L0");
    let before = support::state(&owner);
    let value = read(&snapshot, &projection, 12, json!({"kind":"selection"}));
    assert_eq!(value["frame"], 12);
    assert_eq!(value["target"], fixtures::at(0));
    assert_eq!(value["mesh"]["meshId"], "im_1");
    assert_eq!(support::state(&owner), before);
}

#[test]
fn presentation_bootstrap_without_a_host_is_closed_unavailable_and_does_not_spend() {
    let (owner, _) = fixture(false);
    let before = support::state(&owner);
    let snapshot = support::head(&owner);
    let request = |payload| {
        serde_json::from_value(json!({"apiVersion":2,"requestId":"bootstrap-read",
        "instanceId":snapshot.instance_id(),"documentId":snapshot.document_id(),
        "operation":"query.document.object","payload":payload}))
        .unwrap()
    };
    let payload = json!({"atRevision":snapshot.content_revision(),"view":{"kind":"presentation"}});
    let response = snapshot.dispatch(request(payload.clone())).unwrap();
    assert_eq!(response.error.unwrap().code, DispatchErrorCode::Unavailable);
    let mut stale = payload.clone();
    stale["atRevision"] = json!(snapshot.content_revision() + 1);
    assert_eq!(
        snapshot
            .dispatch(request(stale))
            .unwrap()
            .error
            .unwrap()
            .code,
        DispatchErrorCode::StaleRevision
    );
    for path in ["", "/view"] {
        let mut extra = payload.clone();
        extra.pointer_mut(path).unwrap()["unknown"] = json!(true);
        assert_eq!(
            snapshot
                .dispatch(request(extra))
                .unwrap()
                .error
                .unwrap()
                .code,
            DispatchErrorCode::InvalidRequest
        );
    }
    assert_eq!(support::state(&owner), before);
}
