//! Negative and mutation controls of the raster scene join (G01 / L06-K): typed
//! unavailability with no partial frame and no leaked handle, degenerate input,
//! the opacity product and stage order the oracle must pin, and the report
//! through the pinned scene registry against a real `ObjectFramePacket`.

use super::oracle::{read, Handle, Pixels, ASSET};
use super::tests::prepare;
use super::{
    prepare_raster_scene, prepare_raster_scene_image, raster_producer_output, resource_id,
    scene_refusal, RasterSceneError, RasterSceneFailure, RasterSceneSource, PRODUCER,
};
use crate::evaluation::motion_parent_chain::{
    apply_chain_to_image_rect, ChainEntry, ImageRect, MotionMat,
};
use crate::evaluation::object_scene_style_kernels::object_scene_producer_pin::{
    join_scene, ProducerEntry, SceneOutcome, ScenePin, UnavailableCause,
};
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::object_document::FrameScopeKind;
use crate::object_frame_packet::{prepare_object_frame, ObjectFramePacket, ObjectFrameSelector};
use crate::object_render_scene::{paint_test_document, ObjectSceneError};
use crate::revision::ObjectSnapshot;
use std::sync::Arc;

fn source(asset_id: &str, rect: [f64; 4], rotation: f64, opacity: f64) -> RasterSceneSource {
    RasterSceneSource {
        asset_id: asset_id.to_owned(),
        rect: ImageRect {
            x: rect[0],
            y: rect[1],
            width: rect[2],
            height: rect[3],
            rotation,
        },
        opacity,
    }
}

fn unit() -> RasterSceneSource {
    source(ASSET, [10.0, 20.0, 40.0, 30.0], 0.0, 1.0)
}

fn stage(sx: f64, sy: f64, op: f64) -> ChainEntry {
    ChainEntry {
        mat: MotionMat {
            dx: 0.0,
            dy: 0.0,
            rot: 0.0,
            sx,
            sy,
            op,
            ax: 0.0,
            ay: 0.0,
        },
        pivot: [0.0, 0.0],
    }
}

#[test]
fn missing_pixels_are_typed_and_a_failed_frame_holds_no_handle() {
    let port = Pixels::new((8, 8));
    assert_eq!(Arc::strong_count(&port.handle), 1);
    let error =
        prepare_raster_scene_image(&source("gone", [0.0, 0.0, 4.0, 4.0], 0.0, 1.0), &[], &port)
            .unwrap_err();
    assert_eq!(
        error,
        RasterSceneError::PixelsUnavailable {
            asset_id: "gone".to_owned()
        }
    );
    let items = vec![
        (unit(), Vec::new()),
        (unit(), Vec::new()),
        (source("gone", [0.0, 0.0, 4.0, 4.0], 0.0, 1.0), Vec::new()),
    ];
    let failure = prepare_raster_scene(&items, &port).unwrap_err();
    assert_eq!(
        failure,
        RasterSceneFailure {
            index: 2,
            error: RasterSceneError::PixelsUnavailable {
                asset_id: "gone".to_owned()
            }
        }
    );
    // Two handles were taken for the first two items; the refusal released both.
    assert_eq!(Arc::strong_count(&port.handle), 1);
    let ok = prepare_raster_scene(&items[..2], &port).unwrap();
    assert_eq!(ok.len(), 2);
    assert_eq!(Arc::strong_count(&port.handle), 3);
    drop(ok);
    assert_eq!(Arc::strong_count(&port.handle), 1);
}

#[test]
fn the_retained_image_keeps_its_pixels_alive_after_the_port_is_gone() {
    let port = Pixels::new((8, 8));
    let image = prepare_raster_scene_image(&unit(), &[], &port).unwrap();
    let keep: Handle = Arc::clone(image.handle());
    drop(port);
    assert_eq!(&**image.handle(), &vec![0x7f, 0x20, 0x40, 0xff]);
    assert_eq!(Arc::strong_count(&keep), 2);
    assert_eq!(image.version(), "sha256:test-version");
}

#[test]
fn zero_extent_pixels_and_an_empty_version_are_typed_before_any_geometry() {
    let mut port = Pixels::new((0, 5));
    assert_eq!(
        prepare_raster_scene_image(&unit(), &[], &port).unwrap_err(),
        RasterSceneError::EmptyPixels {
            asset_id: ASSET.to_owned()
        }
    );
    port.size = (5, 0);
    assert!(matches!(
        prepare_raster_scene_image(&unit(), &[], &port).unwrap_err(),
        RasterSceneError::EmptyPixels { .. }
    ));
    port.size = (5, 5);
    port.version.clear();
    assert_eq!(
        prepare_raster_scene_image(&unit(), &[], &port).unwrap_err(),
        RasterSceneError::EmptyVersion {
            asset_id: ASSET.to_owned()
        }
    );
    assert_eq!(
        Arc::strong_count(&port.handle),
        1,
        "no handle survives a refusal"
    );
}

#[test]
fn degenerate_and_non_finite_input_is_refused_with_its_own_error() {
    let port = Pixels::new((4, 4));
    let degenerate = |rect: [f64; 4], stages: &[ChainEntry]| {
        prepare_raster_scene_image(&source(ASSET, rect, 0.0, 1.0), stages, &port).unwrap_err()
    };
    let want = RasterSceneError::DegenerateRect {
        asset_id: ASSET.to_owned(),
    };
    assert_eq!(degenerate([0.0, 0.0, 0.0, 5.0], &[]), want);
    assert_eq!(
        degenerate([0.0, 0.0, 5.0, 5.0], &[stage(0.0, 1.0, 1.0)]),
        want
    );
    assert_eq!(
        degenerate([0.0, 0.0, 5.0, 5.0], &[stage(1.0, 1.0, f64::INFINITY)]),
        want
    );
    assert_eq!(
        prepare_raster_scene_image(
            &source(ASSET, [f64::NAN, 0.0, 5.0, 5.0], 0.0, 1.0),
            &[],
            &port
        )
        .unwrap_err(),
        RasterSceneError::NonFiniteSource
    );
    assert_eq!(
        prepare_raster_scene_image(
            &source(ASSET, [0.0, 0.0, 5.0, 5.0], 0.0, f64::NAN),
            &[],
            &port
        )
        .unwrap_err(),
        RasterSceneError::NonFiniteSource
    );
    assert_eq!(
        prepare_raster_scene_image(&source("", [0.0, 0.0, 5.0, 5.0], 0.0, 1.0), &[], &port)
            .unwrap_err(),
        RasterSceneError::InvalidAssetId
    );
}

#[test]
fn a_falsy_rotation_reads_as_zero_like_the_baseline_and_adds_no_rotation_term() {
    let port = Pixels::new((8, 4));
    for rotation in [f64::NAN, -0.0, 0.0] {
        let image = prepare_raster_scene_image(
            &source(ASSET, [10.0, 20.0, 16.0, 8.0], rotation, 1.0),
            &[],
            &port,
        )
        .unwrap();
        assert_eq!(image.rect().rotation, 0.0);
        assert!(image.rect().rotation.is_sign_positive());
        assert_eq!(
            image.placement().as_coeffs(),
            [2.0, 0.0, 0.0, 2.0, 10.0, 20.0]
        );
    }
}

#[test]
fn the_sequential_opacity_product_is_pinned_and_the_aggregated_multiplier_differs() {
    let cases = read();
    let case = cases
        .iter()
        .find(|c| c.name == "opacity_association")
        .unwrap();
    let image = prepare(case);
    assert_eq!(image.opacity(), case.want_rect[5]);
    assert_eq!(image.opacity(), 0.015120000000000003);
    // apply_chain_to_image_rect multiplies the chain first and the item after.
    let (_, aggregated) = apply_chain_to_image_rect(&case.rect, &case.stages());
    assert_ne!(case.opacity.unwrap() * aggregated, image.opacity());
}

#[test]
fn reversing_the_stage_order_fails_the_oracle() {
    let cases = read();
    let case = cases
        .iter()
        .find(|c| c.name == "scale_all_stages_opacity")
        .unwrap();
    let port = Pixels::new(case.pixels);
    let mut reversed = case.stages();
    reversed.reverse();
    let image = prepare_raster_scene_image(&case.source(), &reversed, &port).unwrap();
    let rect = image.rect();
    let got = [rect.x, rect.y, rect.width, rect.height];
    assert_ne!(&got[..], &case.want_rect[..4]);
    // And production in the baseline order equals it exactly (the case is exact).
    let right = prepare(case).rect();
    assert_eq!(
        &[right.x, right.y, right.width, right.height][..],
        &case.want_rect[..4]
    );
}

#[test]
fn the_source_is_never_mutated() {
    let port = Pixels::new((8, 8));
    let original = unit();
    let before = original.clone();
    let stages = [stage(2.0, 2.0, 0.5)];
    let first = prepare_raster_scene_image(&original, &stages, &port).unwrap();
    let second = prepare_raster_scene_image(&original, &stages, &port).unwrap();
    assert_eq!(original, before);
    assert_eq!(first, second);
}

fn fixture() -> (ObjectSnapshot, ObjectFramePacket) {
    let value = paint_test_document(&[7]);
    let snapshot = ObjectSnapshot::new(
        "raster-pin",
        decode_storage_project(&serde_json::to_vec(&value).unwrap()).unwrap(),
    )
    .unwrap();
    let packet = prepare_object_frame(
        &snapshot,
        &ObjectFrameSelector {
            instance_id: snapshot.instance_id(),
            document_id: snapshot.document_id(),
            content_revision: snapshot.content_revision(),
            document_snapshot_id: snapshot.snapshot_id(),
            context_id: "scene-root",
            scope_kind: FrameScopeKind::Authored,
            frame: 7,
        },
    )
    .unwrap();
    (snapshot, packet)
}

#[test]
fn a_raster_item_declares_its_pixel_resource_to_the_pinned_scene() {
    let (snapshot, packet) = fixture();
    let before = encode_storage_project(snapshot.document()).unwrap();
    let pin = ScenePin::of_packet(&packet);
    let index = packet.records().next().unwrap().source_object_index();
    let port = Pixels::new((8, 8));
    let image = prepare_raster_scene_image(&unit(), &[], &port);
    let output = raster_producer_output(pin.clone(), index, image);
    assert_eq!(output.producer, PRODUCER);
    assert_eq!(output.pin, pin);
    assert_eq!(
        output.resources,
        vec![(
            "raster-asset:asset-1".to_owned(),
            "sha256:test-version".to_owned()
        )]
    );
    assert_eq!(resource_id(ASSET), "raster-asset:asset-1");
    let joined = join_scene(&packet, &pin, vec![output]).unwrap();
    assert_eq!(
        joined
            .declared_resource_versions()
            .get("raster-asset:asset-1"),
        Some(&"sha256:test-version".to_owned())
    );
    let claimed = joined
        .entries()
        .iter()
        .find(|e| e.source_object_index() == index)
        .unwrap();
    assert!(matches!(
        claimed.outcome(),
        SceneOutcome::Item { producer, .. } if producer == PRODUCER
    ));
    assert_eq!(encode_storage_project(snapshot.document()).unwrap(), before);
}

#[test]
fn unavailable_pixels_refuse_the_frame_and_stay_a_typed_entry_with_no_resource() {
    let (_snapshot, packet) = fixture();
    let pin = ScenePin::of_packet(&packet);
    let index = packet.records().next().unwrap().source_object_index();
    let port = Pixels::new((8, 8));
    let error =
        prepare_raster_scene_image(&source("gone", [0.0, 0.0, 4.0, 4.0], 0.0, 1.0), &[], &port)
            .map(|_| ())
            .unwrap_err();
    let output = raster_producer_output::<Handle>(pin.clone(), index, Err(error.clone()));
    assert!(output.resources.is_empty());
    assert!(matches!(
        output.entries.as_slice(),
        [ProducerEntry::Unavailable { reason, .. }] if reason == "raster asset pixels are unavailable"
    ));
    let joined = join_scene(&packet, &pin, vec![output]).unwrap();
    assert!(joined.declared_resource_versions().is_empty());
    let entry = joined
        .entries()
        .iter()
        .find(|e| e.source_object_index() == index)
        .unwrap();
    assert!(matches!(
        entry.outcome(),
        SceneOutcome::Unavailable(UnavailableCause::Reported { producer, .. }) if producer == PRODUCER
    ));
    assert_eq!(
        scene_refusal(&error, index),
        ObjectSceneError::UnsupportedGeometry {
            source_object_index: index,
            reason: "raster asset pixels are unavailable",
        }
    );
}
