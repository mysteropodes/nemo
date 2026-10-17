//! Export-routing, tag and widget-slot policy tests of the image-mesh scene join (L03-S, acceptance
//! bullets 3 and 4).
//!
//! `exportHasImageMesh` rows are the executed baseline (`export.js` at fbe4b8a3); the slot rules are
//! checked on decoded documents with the widget layer first, last and absent.

use super::{
    check_export_route, export_has_image_mesh, scene_slot, scene_slots, tagged_mesh_id,
    widget_layers_with_content, ExportRoute, ImageOnlyExportRefusal, LayerSceneSlot,
};
use crate::evaluation::motion_parent_chain::ImageRect;
use crate::evaluation::object_image_mesh_kernels::render_image_mesh_scene_source::oracle_rows::{
    export_rows, geometry_for, mesh_of, mesh_rows, plain_source, row_named, widget_document,
};
use crate::evaluation::object_image_mesh_kernels::render_image_mesh_scene_source::prepare_mesh_geometry;
use crate::object_codec::decode_project;
use crate::object_document::object_raster_asset_kernels::object_raster_asset_item::RasterItem;
use crate::object_document::ImageMeshStore;
use crate::revision::ObjectSnapshot;
use serde_json::Value;

fn export_store(keys: &Option<Vec<String>>, mesh: &str) -> Option<ImageMeshStore> {
    keys.as_ref().map(|keys| {
        ImageMeshStore::try_new(
            keys.iter()
                .map(|key| (key.clone(), mesh_of(mesh)))
                .collect(),
        )
        .unwrap()
    })
}

fn one_pixel_mesh() -> String {
    let rows = mesh_rows();
    rows[row_named(&rows, "grid1_one_pixel_image")].mesh.clone()
}

#[test]
fn export_routing_equals_the_executed_export_has_image_mesh() {
    let mesh = one_pixel_mesh();
    for row in export_rows() {
        let store = export_store(&row.keys, &mesh);
        assert_eq!(
            export_has_image_mesh(store.as_ref()),
            row.has,
            "{}",
            row.name
        );
    }
}

#[test]
fn an_image_only_export_of_a_meshed_project_is_refused_not_flattened() {
    let mesh = one_pixel_mesh();
    let two = Some(vec!["im_1".to_owned(), "im_2".to_owned()]);
    let store = export_store(&two, &mesh);
    assert_eq!(
        check_export_route(store.as_ref(), ExportRoute::NativeScene),
        Ok(())
    );
    let refusal = check_export_route(store.as_ref(), ExportRoute::ImageOnly).unwrap_err();
    assert_eq!(
        refusal,
        ImageOnlyExportRefusal {
            mesh_ids: vec!["im_1".to_owned(), "im_2".to_owned()]
        }
    );
    assert_eq!(ImageOnlyExportRefusal::CODE, "image-only-export-refused");
    let one = export_store(&Some(vec!["orphan".to_owned()]), &mesh);
    assert!(
        check_export_route(one.as_ref(), ExportRoute::ImageOnly).is_err(),
        "an orphan mesh still routes the export through the engine, like the baseline"
    );
    let empty = export_store(&Some(vec![]), &mesh);
    assert_eq!(
        check_export_route(empty.as_ref(), ExportRoute::ImageOnly),
        Ok(())
    );
    assert_eq!(check_export_route(None, ExportRoute::ImageOnly), Ok(()));
}

#[test]
fn a_raster_without_a_tag_has_no_mesh_id() {
    let tagged = RasterItem::decode(
        r#"{"assetId":"asset-1","x":0,"y":0,"width":4,"height":4,"meshId":"im_1"}"#,
    )
    .unwrap();
    let plain =
        RasterItem::decode(r#"{"assetId":"asset-1","x":0,"y":0,"width":4,"height":4}"#).unwrap();
    assert_eq!(tagged_mesh_id(&tagged), Some("im_1"));
    assert_eq!(tagged_mesh_id(&plain), None);
}

fn snapshot(value: &Value) -> ObjectSnapshot {
    ObjectSnapshot::new(
        "widget-slots",
        decode_project(&serde_json::to_vec(value).unwrap()).unwrap(),
    )
    .unwrap()
}

#[test]
fn the_empty_widget_slot_keeps_every_stack_index() {
    let first = snapshot(&widget_document(Some(0), true));
    let last = snapshot(&widget_document(Some(2), false));
    let plain = snapshot(&widget_document(None, false));
    assert_eq!(
        scene_slots(first.document().layers()),
        vec![
            LayerSceneSlot::EmptyWidget,
            LayerSceneSlot::Content,
            LayerSceneSlot::Content
        ]
    );
    assert_eq!(
        scene_slots(last.document().layers()),
        vec![
            LayerSceneSlot::Content,
            LayerSceneSlot::Content,
            LayerSceneSlot::EmptyWidget
        ]
    );
    assert_eq!(
        scene_slots(plain.document().layers()),
        vec![LayerSceneSlot::Content; 2]
    );
    assert_eq!(
        scene_slot(&last.document().layers()[2]),
        LayerSceneSlot::EmptyWidget
    );
    assert_eq!(
        scene_slot(&last.document().layers()[0]),
        LayerSceneSlot::Content
    );
}

#[test]
fn a_widget_layer_that_owned_scene_content_would_be_reported() {
    for widget_at in [0, 2] {
        let snapshot = snapshot(&widget_document(Some(widget_at), true));
        let layers = snapshot.document().layers();
        let ordinary: Vec<usize> = (0..3).filter(|i| *i != widget_at).collect();
        assert_eq!(
            widget_layers_with_content(layers, ordinary),
            Vec::<usize>::new(),
            "the two ordinary layers own content, the widget layer does not"
        );
        assert_eq!(
            widget_layers_with_content(layers, [widget_at, widget_at, 1]),
            vec![widget_at]
        );
        assert_eq!(
            widget_layers_with_content(layers, [99]),
            Vec::<usize>::new()
        );
    }
}

#[test]
fn a_drawing_object_cannot_be_put_on_a_widget_layer() {
    use crate::evaluation::object_image_mesh_kernels::render_image_mesh_scene_source::oracle_rows::cases;
    use serde_json::json;
    let mut value = widget_document(Some(2), false);
    let mut record = cases()["records"][0].clone();
    record["target"] = json!({"contextId": "scene-root", "frameScope": {"kind": "authored", "frame": 0},
        "layerUid": "w", "strokeId": "s1"});
    value["objects"] = json!([record]);
    assert!(decode_project(&serde_json::to_vec(&value).unwrap()).is_err());
    value["layers"][2] = json!({"layerUid": "w"});
    assert!(
        decode_project(&serde_json::to_vec(&value).unwrap()).is_ok(),
        "the same record is accepted once the layer carries no widget"
    );
}

/// The shipped two-layer document with the mesh store member `imageMeshes` holding `meshes`.
fn document_with_store(meshes: &[(&str, &str)]) -> Value {
    let mut value = widget_document(None, false);
    let mut store = serde_json::Map::new();
    for (id, text) in meshes {
        store.insert((*id).to_owned(), serde_json::from_str(text).unwrap());
    }
    value["imageMeshes"] = Value::Object(store);
    value
}

#[test]
fn the_document_store_member_feeds_the_export_gate() {
    let mesh = one_pixel_mesh();
    let meshed = snapshot(&document_with_store(&[
        ("im_1", mesh.as_str()),
        ("im_2", mesh.as_str()),
    ]));
    let store = meshed.document().image_meshes();
    assert!(export_has_image_mesh(store));
    assert_eq!(
        check_export_route(store, ExportRoute::ImageOnly),
        Err(ImageOnlyExportRefusal {
            mesh_ids: vec!["im_1".to_owned(), "im_2".to_owned()]
        })
    );
    assert_eq!(check_export_route(store, ExportRoute::NativeScene), Ok(()));
    let plain = snapshot(&widget_document(None, false));
    assert!(plain.document().image_meshes().is_none());
    assert_eq!(
        check_export_route(plain.document().image_meshes(), ExportRoute::ImageOnly),
        Ok(())
    );
}

#[test]
fn the_document_store_member_feeds_the_geometry_join() {
    let rows = mesh_rows();
    let corpus = &rows[row_named(&rows, "corpus_stored_offset")].mesh;
    let doc = snapshot(&document_with_store(&[("im_1", corpus.as_str())]));
    let base = ImageRect {
        x: 320.0,
        y: 180.0,
        width: 200.0,
        height: 200.0,
        rotation: 0.0,
    };
    let from_document = prepare_mesh_geometry(
        &base,
        (64, 64),
        1.0,
        &plain_source("im_1"),
        doc.document().image_meshes(),
    )
    .unwrap()
    .unwrap();
    let from_text = geometry_for(base, (64, 64), 1.0, &plain_source("im_1"), corpus)
        .unwrap()
        .unwrap();
    assert_eq!(from_document, from_text);
    assert_eq!(from_document.payload.verts[8], [440.0, 280.0]);
}
