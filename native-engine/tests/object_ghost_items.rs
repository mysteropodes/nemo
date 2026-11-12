//! #2358 GHOST-UNDER: text glyph and raster ghosts of the ghosted layer are composed with the vector ones (real Metal).
use super::gpu::metal;
use super::{drawing, owner, plan, selector};
use crate::application::prepare_ghost_underlay;
use crate::object_render_scene::{
    prepare_object_composition, prepare_object_viewport_composition, GhostMode,
    GhostUnderlayRequest,
};

fn request(layer: &str, mode: GhostMode) -> GhostUnderlayRequest {
    GhostUnderlayRequest {
        layer_uid: layer.into(),
        onion: true,
        ghost_all: false,
        from: 0,
        to: 7,
        previous_opacity: 60.,
        next_opacity: 30.,
        mode,
        previous_tint: None,
        next_tint: None,
        shift: None,
    }
}

#[test]
fn gpu_text_and_raster_ghosts_join_the_vector_ghosts_under_the_live_frame_and_stay_out_of_exports()
{
    let owner = owner(&drawing());
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let request = request("L0", GhostMode::Tinted);
    let underlay = prepare_ghost_underlay(&snapshot, &request, 7);
    let receipt = underlay.receipt();
    assert_eq!(
        (receipt.status, receipt.frames),
        ("composited", 7),
        "{receipt:?}"
    );
    let (shapes, images) = underlay.counts();
    assert!(
        images >= 7,
        "one raster ghost per ghost frame, got {images}"
    );
    // 7 frames x (the vector square + the glyph contours of 'A'): text adds contours beyond the vector paths alone.
    assert!(shapes > 7, "vector and text ghosts, got {shapes}");
    let outline = prepare_ghost_underlay(&snapshot, &self::request("L0", GhostMode::Outline), 7);
    assert!(outline.counts().0 > 0 && outline.counts().1 == images);
    // The live frame is drawn over the ghosts: with identical content in every frame, live pixels are unchanged.
    let mut gpu = metal();
    let p = plan(&snapshot, 7);
    let pins = selector(&snapshot, 7);
    let live = prepare_object_viewport_composition(&p, &pins, None).unwrap();
    let plain = render_with_quality(&mut gpu, &live);
    let with = live.clone().with_ghost_underlay(underlay.clone()).unwrap();
    let ghosted = render_with_quality(&mut gpu, &with);
    assert_eq!(plain.len(), ghosted.len());
    let wide = |bytes: &[u8], x: usize, y: usize| -> [u8; 4] {
        bytes[(y * 320 + x) * 4..(y * 320 + x) * 4 + 4]
            .try_into()
            .unwrap()
    };
    for (x, y) in [(300, 20), (41, 41), (60, 60)] {
        let (a, b) = (wide(&plain, x, y), wide(&ghosted, x, y));
        assert!(
            a.iter().zip(b).all(|(l, r)| l.abs_diff(r) <= 3),
            "{x},{y}: {a:?} vs {b:?}"
        );
    }
    // Never in an export: the artwork witness refuses the underlay.
    let artwork = prepare_object_composition(&p, &pins).unwrap();
    assert!(artwork.with_ghost_underlay(underlay).is_err());
    // A layer with nothing to ghost is typed, not blank-by-accident.
    assert_eq!(
        prepare_ghost_underlay(&snapshot, &self::request("nope", GhostMode::Tinted), 7)
            .receipt()
            .status,
        "unavailable"
    );
}
#[test]
fn gpu_repeated_presents_of_an_unchanged_revision_do_not_reprepare_ghost_frames_or_reset_the_image_cache(
) {
    use crate::application::{prepare_ghost_underlay, prepared_ghost_frames};
    let owner = owner(&drawing());
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let request = request("L0", GhostMode::Tinted);
    let before = prepared_ghost_frames();
    let first = prepare_ghost_underlay(&snapshot, &request, 7);
    let evaluated = prepared_ghost_frames() - before;
    assert_eq!(evaluated, 7, "one evaluated ghost frame per ghost frame");
    assert!(first.has_images(), "the fixture ghosts raster images");
    // Pan / zoom / repeat presents: the same prepared underlay, nothing evaluated again (the old build skipped the
    // memo whenever the underlay held images and re-ran the whole query on every present).
    let again = prepare_ghost_underlay(&snapshot, &request, 7);
    assert!(std::sync::Arc::ptr_eq(&first, &again));
    assert_eq!(prepared_ghost_frames() - before, evaluated);
    // Another frame of the same revision re-uses the frames already evaluated: only frame 7 is new.
    let scrubbed = prepare_ghost_underlay(&snapshot, &self::request("L0", GhostMode::Tinted), 6);
    assert_eq!(scrubbed.receipt().status, "composited");
    assert_eq!(prepared_ghost_frames() - before, evaluated + 1);
    // The compositor keeps the resident ghost set: presenting the same underlay again resets nothing.
    let mut gpu = metal();
    let p = plan(&snapshot, 7);
    let live = prepare_object_viewport_composition(&p, &selector(&snapshot, 7), None).unwrap();
    let ghosted = live.with_ghost_underlay(first).unwrap();
    render_with_quality(&mut gpu, &ghosted);
    let resets = gpu.ghost_cache_resets();
    assert!(resets <= 1, "a first set may reset once, got {resets}");
    for _ in 0..3 {
        render_with_quality(&mut gpu, &ghosted);
    }
    assert_eq!(gpu.ghost_cache_resets(), resets, "an unchanged ghost set never resets the image cache again");
}
fn render_with_quality(
    gpu: &mut crate::compositor::Compositor,
    witness: &crate::object_render_scene::ObjectCompositionWitness,
) -> Vec<u8> {
    use crate::object_render_scene::prepare_scheduled_object_composition_from_witness as prepare_live;
    use crate::scheduler::{
        EvaluationKey, FrameScheduler, ObjectSchedulePins, ObjectScheduleScope, OutputSpec,
    };
    let snapshot = witness.plan().packet().snapshot();
    let pins = ObjectSchedulePins {
        instance_id: snapshot.instance_id().into(),
        document_id: snapshot.document_id().into(),
        content_revision: snapshot.content_revision(),
        document_snapshot_id: snapshot.snapshot_id().into(),
        context_id: "scene-root".into(),
        frame: witness.frame(),
        scope: ObjectScheduleScope::Authored,
    };
    let key = EvaluationKey::new(
        snapshot.snapshot_id(),
        "scene-root",
        witness.frame(),
        witness.expected_quality(),
        OutputSpec::new("frame", "rgba8", 320, 180, "srgb", "straight").unwrap(),
        witness.plan().scene_resource_versions().unwrap(),
    )
    .unwrap();
    let mut scheduler = FrameScheduler::new();
    let work = scheduler
        .schedule_object(snapshot.clone(), pins, key)
        .unwrap();
    let live = prepare_live(&scheduler, &work, witness).unwrap();
    let result = gpu.compose_scheduled_object(&live).unwrap();
    let bytes = gpu
        .readback_scheduled_object_rgba8(&result, &live)
        .unwrap()
        .bytes(&live)
        .unwrap()
        .to_vec();
    drop(live);
    scheduler.succeed(work.work_id()).unwrap();
    bytes
}
