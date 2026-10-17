//! Tests of the scene item and the producer join of the image-mesh scene join (L03-S, acceptance
//! bullets 2 and 5): the real raster scene producer prepares the image (its D30 stage chain gives the
//! final rect and opacity), the mesh follows that rect, and the item is the one type both outputs use.
//!
//! The geometry itself is pinned against the executed baseline in `render_image_mesh_scene_source`;
//! here the join is compared with that geometry and with the oracle's final rect and opacity.

use super::image_helpers::{prepare_image_row, raster_image, SizedPort};
use super::{
    mesh_producer_output, prepare_mesh_scene_item, scene_refusal, MeshSceneError, PRODUCER,
};
use crate::evaluation::motion_parent_chain::ImageRect;
use crate::evaluation::object_image_mesh_kernels::render_image_mesh_scene_source::oracle_rows::{
    chain, mesh_of, mesh_rows, none_rows, plain_source, prepare_row, rect_close, row_named,
    store_of, IDENTITY_VIEW,
};
use crate::evaluation::object_scene_style_kernels::object_scene_producer_pin::{
    ProducerEntry, ScenePin,
};
use crate::object_document::{FrameScopeKind, ImageMeshStore};
use crate::object_render_scene::text_glyph_scene_kernels::object_raster_asset_scene::{
    prepare_raster_scene_image, RasterPixelPort, RasterPixels, RasterSceneSource,
};
use crate::object_render_scene::ObjectSceneError;
use std::rc::Rc;

const BASE_200: ImageRect = ImageRect {
    x: 320.0,
    y: 180.0,
    width: 200.0,
    height: 200.0,
    rotation: 0.0,
};

fn pin() -> ScenePin {
    ScenePin {
        instance_id: "instance".to_owned(),
        document_id: "document".to_owned(),
        content_revision: 7,
        document_snapshot_id: "snapshot".to_owned(),
        context_id: "scene-root".to_owned(),
        scope_kind: FrameScopeKind::Authored,
        frame: 3,
    }
}

#[test]
fn every_oracle_row_through_the_real_raster_producer_matches_the_baseline() {
    for row in mesh_rows() {
        let name = &row.name;
        let (image, item) = prepare_image_row(&row);
        assert!(
            rect_close(&image.rect(), &row.expect.rect),
            "{name}: final rect"
        );
        assert_eq!(
            image.opacity(),
            row.expect.opacity,
            "{name}: opacity product"
        );
        let (rect, op) = chain(row.base, &row.stages, row.opacity);
        assert_eq!(
            image.rect(),
            rect,
            "{name}: the producer and the chain agree"
        );
        assert_eq!(image.opacity(), op, "{name}");
        assert_eq!(item.payload().verts, row.expect.verts, "{name}: verts");
        assert_eq!(item.payload().uvs, row.expect.uvs, "{name}: uvs");
        let (_, _, geometry) = prepare_row(&row);
        assert_eq!(
            item.geometry(),
            &geometry,
            "{name}: item equals the geometry half"
        );
        let plan = item.plan().unwrap();
        assert_eq!(f64::from(plan.opacity), row.expect.opacity_f32, "{name}");
        assert_eq!(item.view(), row.view, "{name}");
        assert_eq!(item.mesh_id(), "im_1");
        assert_eq!(item.image(), &image);
    }
}

#[test]
fn a_mesh_without_triangles_is_left_to_the_plain_raster_producer() {
    for row in none_rows() {
        let image = raster_image(row.base, &[], 1.0, row.image);
        let store = store_of(&row.mesh);
        let outcome = prepare_mesh_scene_item(&image, &plain_source("im_1"), Some(&store));
        assert_eq!(outcome, Ok(None), "{}", row.name);
    }
}

#[test]
fn refusals_pass_through_typed_and_the_dangling_tag_is_not_a_flat_image() {
    let image = raster_image(BASE_200, &[], 1.0, (64, 64));
    let rows = mesh_rows();
    let store = store_of(&rows[row_named(&rows, "corpus_stored_offset")].mesh);
    let missing = prepare_mesh_scene_item(&image, &plain_source("im_9"), Some(&store));
    assert_eq!(
        missing,
        Err(MeshSceneError::MeshMissing {
            mesh_id: "im_9".to_owned()
        })
    );
    let no_store = prepare_mesh_scene_item(&image, &plain_source("im_1"), None);
    assert_eq!(
        no_store,
        Err(MeshSceneError::StoreUnavailable {
            mesh_id: "im_1".to_owned()
        })
    );
}

#[test]
fn another_output_replans_the_same_geometry_and_only_the_seam_dilation_follows_the_view() {
    let rows = mesh_rows();
    let (_, item) = prepare_image_row(&rows[row_named(&rows, "grid4_identity")]);
    assert_eq!(item.view(), IDENTITY_VIEW);
    let same = item.for_view(IDENTITY_VIEW);
    assert_eq!(same, item, "an identical view replans an identical item");
    let zoomed = item.for_view([2.0, 0.0, 0.0, 2.0, 100.0, 50.0]);
    assert_eq!(zoomed.payload(), item.payload());
    assert_eq!(zoomed.image(), item.image());
    assert_eq!(zoomed.view(), [2.0, 0.0, 0.0, 2.0, 100.0, 50.0]);
    let (a, b) = (item.plan().unwrap(), zoomed.plan().unwrap());
    assert_eq!((a.opacity, &a.outline), (b.opacity, &b.outline));
    assert!(a.triangles.iter().zip(b.triangles.iter()).all(|(x, y)| {
        (x.triangle, x.source, x.dest, x.place) == (y.triangle, y.source, y.dest, y.place)
    }));
    assert!(a
        .triangles
        .iter()
        .zip(b.triangles.iter())
        .any(|(x, y)| x.clip != y.clip));
}

#[test]
fn the_producer_reports_items_resources_nothing_and_refusals() {
    let rows = mesh_rows();
    let (_, item) = prepare_image_row(&rows[row_named(&rows, "grid4_scale_opacity")]);
    let reported = mesh_producer_output(pin(), 4, Ok(Some(item.clone())));
    assert_eq!(reported.producer, PRODUCER);
    assert_eq!(reported.pin, pin());
    assert_eq!(
        reported.resources,
        vec![("raster-asset:asset-1".to_owned(), "v1".to_owned())]
    );
    match reported.entries.as_slice() {
        [ProducerEntry::Item {
            source_object_index,
            payload,
        }] => {
            assert_eq!(*source_object_index, 4);
            assert_eq!(payload, &item);
        }
        other => panic!("one item expected, got {other:?}"),
    }
    let plain = mesh_producer_output::<()>(pin(), 5, Ok(None));
    assert!(plain.entries.is_empty() && plain.resources.is_empty());
    let error = MeshSceneError::MeshMissing {
        mesh_id: "im_9".to_owned(),
    };
    let refused = mesh_producer_output::<()>(pin(), 6, Err(error.clone()));
    assert!(refused.resources.is_empty());
    assert_eq!(
        refused.entries,
        vec![ProducerEntry::Unavailable {
            source_object_index: 6,
            reason: "meshed raster names a mesh the store does not hold".to_owned(),
        }]
    );
    assert_eq!(
        scene_refusal(&error, 6),
        ObjectSceneError::UnsupportedGeometry {
            source_object_index: 6,
            reason: "meshed raster names a mesh the store does not hold",
        }
    );
}

/// A pixel port whose handle is a reference-counted token, to observe who holds the pixels.
struct CountingPort {
    token: Rc<()>,
    inner: SizedPort,
}

impl RasterPixelPort for CountingPort {
    type Handle = Rc<()>;
    fn pixels(&self, _asset_id: &str) -> Option<RasterPixels<Rc<()>>> {
        Some(RasterPixels {
            handle: Rc::clone(&self.token),
            width: self.inner.width,
            height: self.inner.height,
            version: "v1".to_owned(),
        })
    }
}

#[test]
fn a_refusal_takes_no_pixel_handle_and_an_item_keeps_the_pixels_alive_past_the_image() {
    let rows = mesh_rows();
    let row = &rows[row_named(&rows, "grid4_identity")];
    let port = CountingPort {
        token: Rc::new(()),
        inner: SizedPort {
            width: 64,
            height: 64,
        },
    };
    let source = RasterSceneSource {
        asset_id: "asset-1".to_owned(),
        rect: row.base,
        opacity: 1.0,
    };
    let image = prepare_raster_scene_image(&source, &[], &port).unwrap();
    assert_eq!(
        Rc::strong_count(&port.token),
        2,
        "the port and the prepared image"
    );
    let store = store_of(&row.mesh);
    let refused = prepare_mesh_scene_item(&image, &plain_source("im_9"), Some(&store));
    assert!(refused.is_err());
    assert_eq!(
        Rc::strong_count(&port.token),
        2,
        "a refusal retains nothing"
    );
    let item = prepare_mesh_scene_item(&image, &plain_source("im_1"), Some(&store))
        .unwrap()
        .unwrap();
    assert_eq!(Rc::strong_count(&port.token), 3);
    drop(image);
    assert_eq!(
        Rc::strong_count(&port.token),
        2,
        "the item alone keeps the pixels alive"
    );
    assert!(Rc::ptr_eq(item.image().handle(), &port.token));
    drop(item);
    assert_eq!(Rc::strong_count(&port.token), 1);
}

#[test]
fn a_renderer_skip_is_an_item_that_draws_nothing_not_a_missing_item() {
    let tiny = r#"{"outline":[],"verts":[[0,0],[1,1]],"tris":[0,1,0],"offsets":[[0,0],[0,0]],"cols":1,"rows":1}"#;
    let store = ImageMeshStore::try_new(vec![("im_1".to_owned(), mesh_of(tiny))]).unwrap();
    let image = raster_image(BASE_200, &[], 1.0, (8, 8));
    let item = prepare_mesh_scene_item(&image, &plain_source("im_1"), Some(&store))
        .unwrap()
        .unwrap();
    assert!(item.plan().is_none());
    assert_eq!(item.triangle_count(), 0);
    let reported = mesh_producer_output(pin(), 0, Ok(Some(item)));
    assert_eq!(
        reported.entries.len(),
        1,
        "the raster is claimed, so the plain producer must not also draw it"
    );
    assert_eq!(reported.resources.len(), 1);
}
