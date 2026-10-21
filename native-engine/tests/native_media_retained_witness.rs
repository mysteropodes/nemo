//! T12 review B1: a frame that draws a raster AND a video makes the compositor
//! retain its witness as the raster/reference image cache. That retained copy
//! must hold no decoded-video lease, or the media gate never finishes complete
//! and the next preview is blocked and every later export refused.
//! The CPU tests run anywhere; the GPU test composes through the real
//! scheduled compositor and needs the coordinated Metal slot.
use super::tests::{base, selector, wire, Fake, BOTH};
use super::*;
use crate::history::NativeObjectHistory;
use crate::object_render_scene::scene_effect_kernels::object_scene_resource_set::SceneResourceKind;
use serde_json::json;
include!("object_raster_asset_attach_support.rs");

/// The media fixture (drawn layer D, video layer V on `v1`) plus a real
/// imported PNG on its own new layer, so one frame holds a raster and a video.
fn raster_and_video() -> NativeObjectHistory {
    let mut owner = NativeObjectHistory::from_project_file(
        "media-retained",
        wire("v1", 0, true).to_string().as_bytes(),
    )
    .unwrap();
    let input = asset_request(
        &owner,
        "retained-png",
        "attach",
        json!({"documentSnapshotId":current_snapshot_id(&owner),"lifecycleGeneration":1,
            "name":"still.png","mediaType":"image/png",
            "source":{"kind":"file","path":"/admitted/still.png"},"placement":{"kind":"newLayer"}}),
    );
    let reply = owner
        .dispatch_image_import(&input, &oracle_bytes("blue-2x1"))
        .unwrap();
    assert!(reply.is_ok(), "{reply:?}");
    owner
}

fn gate<'p, 'd>(
    port: &'p mut NativeVideoPort<'d>,
    snapshot: &ObjectSnapshot,
    frames: &[u32],
) -> NativeMediaFrames<'p, 'd> {
    begin_native_media_frames(
        port,
        snapshot,
        &selector(snapshot, frames[0]),
        frames,
        true,
        &|_| true,
        0,
    )
    .unwrap()
}

#[test]
fn the_retained_image_cache_copy_holds_no_video_lease_and_keeps_raster_identity() {
    let owner = raster_and_video();
    let snapshot = owner.acquire_snapshot(owner.content_revision()).unwrap();
    let mut decoder = Fake::with(BOTH);
    let mut port = NativeVideoPort::new(&mut decoder);
    let mut media = gate(&mut port, &snapshot, &[3]);
    let witness = media.witness(3, base(&snapshot, 3)).unwrap();
    assert!(witness.has_raster_images() && witness.has_scene_images());
    assert!(
        witness.video_image(0).is_some(),
        "the frame draws its video"
    );
    let kinds = |w: &ObjectCompositionWitness| -> Vec<SceneResourceKind> {
        w.scene_resources()
            .map(|set| set.entries().iter().map(|entry| entry.kind()).collect())
            .unwrap_or_default()
    };
    assert!(kinds(&witness).contains(&SceneResourceKind::Media));
    assert!(kinds(&witness).contains(&SceneResourceKind::Raster));

    // Exactly what the compositor now retains past the frame.
    let retained = witness.without_media();
    assert!(retained.video_image(0).is_none());
    assert_eq!(kinds(&retained), [SceneResourceKind::Raster]);
    assert!(retained.has_raster_images());
    assert!(retained.reference_matches_plan(witness.plan()));
    assert_eq!(
        retained.raster_images().next().unwrap().data.id(),
        witness.raster_images().next().unwrap().data.id(),
        "the retained raster identity is the drawn one"
    );
    drop(witness);
    let release = media.finish();
    assert!(release.complete, "{release:?}");
    assert_eq!((release.frames_acquired, release.frames_released), (1, 1));

    // A later frame is accepted while the cache copy is still alive.
    let mut decoder = Fake::with(BOTH);
    let mut port = NativeVideoPort::new(&mut decoder);
    let mut media = gate(&mut port, &snapshot, &[4]);
    let witness = media.witness(4, base(&snapshot, 4)).unwrap();
    assert!(retained.reference_matches_plan(witness.plan()));
    drop(witness);
    assert!(media.finish().complete);
    drop(retained);
}

#[test]
fn a_full_clone_kept_past_the_frame_is_what_blocks_the_gate() {
    // Control for the test above: the pre-fix retain (`prepared.clone()`).
    let owner = raster_and_video();
    let snapshot = owner.acquire_snapshot(owner.content_revision()).unwrap();
    let mut decoder = Fake::with(BOTH);
    let mut port = NativeVideoPort::new(&mut decoder);
    let mut media = gate(&mut port, &snapshot, &[3]);
    let witness = media.witness(3, base(&snapshot, 3)).unwrap();
    let leaked = witness.clone();
    drop(witness);
    assert!(!media.finish().complete);
    drop(leaked);
}

#[test]
fn a_video_only_witness_retains_no_resource_set_at_all() {
    let history = NativeObjectHistory::from_project_file(
        "media-only",
        wire("v1", 0, true).to_string().as_bytes(),
    )
    .unwrap();
    let snapshot = history.acquire_snapshot(0).unwrap();
    let mut decoder = Fake::with(BOTH);
    let mut port = NativeVideoPort::new(&mut decoder);
    let mut media = gate(&mut port, &snapshot, &[2]);
    let witness = media.witness(2, base(&snapshot, 2)).unwrap();
    let retained = witness.without_media();
    assert!(retained.scene_resources().is_none());
    assert!(retained.video_image(0).is_none());
    drop(witness);
    assert!(media.finish().complete);
}

#[test]
#[ignore = "requires the explicitly coordinated real Metal/GPU validation slot"]
fn gpu_composing_a_raster_and_video_frame_releases_every_lease_for_the_next_frame() {
    use crate::compositor::Compositor;
    use crate::object_render_scene::prepare_scheduled_object_composition_from_witness;
    use crate::scheduler::{
        EvaluationKey, FrameScheduler, ObjectSchedulePins, ObjectScheduleScope, OutputSpec,
    };
    let owner = raster_and_video();
    let snapshot = owner.acquire_snapshot(owner.content_revision()).unwrap();
    let mut gpu = Compositor::new().expect("real Metal required");
    let mut scheduler = FrameScheduler::new();
    // Two successive frames through the same compositor: the first leaves its
    // retained image cache behind, the second must still be admitted and finish.
    for frame in [3, 4] {
        let mut decoder = Fake::with(BOTH);
        let mut port = NativeVideoPort::new(&mut decoder);
        let mut media = gate(&mut port, &snapshot, &[frame]);
        let witness = media.witness(frame, base(&snapshot, frame)).unwrap();
        let output = OutputSpec::new("frame", "rgba8", 320, 180, "srgb", "straight").unwrap();
        let key = EvaluationKey::new(
            snapshot.snapshot_id(),
            "scene-root",
            frame,
            "final",
            output,
            declared_scene_resources(&witness),
        )
        .unwrap();
        let pins = ObjectSchedulePins {
            instance_id: snapshot.instance_id().into(),
            document_id: snapshot.document_id().into(),
            content_revision: snapshot.content_revision(),
            document_snapshot_id: snapshot.snapshot_id().into(),
            context_id: "scene-root".into(),
            frame,
            scope: ObjectScheduleScope::Authored,
        };
        let work = scheduler
            .schedule_object(snapshot.clone(), pins, key)
            .unwrap();
        let live =
            prepare_scheduled_object_composition_from_witness(&scheduler, &work, &witness).unwrap();
        let result = gpu.compose_scheduled_object(&live).unwrap();
        let pixels = gpu
            .readback_scheduled_object_rgba8(&result, &live)
            .unwrap()
            .bytes(&live)
            .unwrap()
            .to_vec();
        assert_eq!(pixels.len(), 320 * 180 * 4);
        drop((result, live, witness));
        let release = media.finish();
        assert!(release.complete, "frame {frame}: {release:?}");
        assert_eq!((release.frames_acquired, release.frames_released), (1, 1));
        scheduler.succeed(work.work_id()).unwrap();
    }
}
