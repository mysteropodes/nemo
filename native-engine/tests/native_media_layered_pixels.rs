//! #2257: a decoded video slot on the layered compositor. A Multiply layer over an empty backdrop is the source itself, so
//! the layered image of a project that draws a video must equal the flat Normal image of the same project, whichever
//! layer carries the blend. Real Metal; a missing adapter is a test FAILURE, never a skip.
use crate::native_media_frame_export::tests::{base, selector, wire, Fake, BOTH};
use crate::native_media_frame_export::*;
use crate::compositor::Compositor;
use crate::history::NativeObjectHistory;
use crate::object_render_scene::prepare_scheduled_object_composition_from_witness;
use crate::object_render_scene::scene_effect_kernels::scene_effect_order::CompositionPlan;
use crate::scheduler::{
    EvaluationKey, FrameScheduler, ObjectSchedulePins, ObjectScheduleScope, OutputSpec,
};

const FRAME: u32 = 3;

fn owner(blend_on: Option<usize>) -> NativeObjectHistory {
    let mut project = wire("v1", 0, true);
    if let Some(layer) = blend_on {
        project["drawing"]["layers"][layer]["blendMode"] = serde_json::json!("multiply");
    }
    NativeObjectHistory::from_project_file("media-layered", project.to_string().as_bytes()).unwrap()
}

/// `(layered, pixels)` of the scheduled door at `FRAME`, with the video frame decoded by the fake decoder.
fn image(owner: &NativeObjectHistory) -> (bool, Vec<u8>) {
    let snapshot = owner.acquire_snapshot(owner.content_revision()).unwrap();
    let mut decoder = Fake::with(BOTH);
    let mut port = NativeVideoPort::new(&mut decoder);
    let mut media = begin_native_media_frames(
        &mut port,
        &snapshot,
        &selector(&snapshot, FRAME),
        &[FRAME],
        true,
        &|_| true,
        0,
    )
    .unwrap();
    let witness = media.witness(FRAME, base(&snapshot, FRAME)).unwrap();
    assert!(
        witness.video_image(0).is_some(),
        "the frame draws its video"
    );
    let layered = matches!(
        witness.scene_effects().expect("production joins").plan(),
        CompositionPlan::Layered { .. }
    );
    let mut gpu = Compositor::new().expect("real Metal required");
    let mut scheduler = FrameScheduler::new();
    let key = EvaluationKey::new(
        snapshot.snapshot_id(),
        "scene-root",
        FRAME,
        "final",
        OutputSpec::new("frame", "rgba8", 320, 180, "srgb", "straight").unwrap(),
        declared_scene_resources(&witness),
    )
    .unwrap();
    let pins = ObjectSchedulePins {
        instance_id: snapshot.instance_id().into(),
        document_id: snapshot.document_id().into(),
        content_revision: snapshot.content_revision(),
        document_snapshot_id: snapshot.snapshot_id().into(),
        context_id: "scene-root".into(),
        frame: FRAME,
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
    drop((result, live, witness));
    assert!(media.finish().complete);
    scheduler.succeed(work.work_id()).unwrap();
    (layered, pixels)
}

fn at(bytes: &[u8], x: usize, y: usize) -> [u8; 4] {
    bytes[(y * 320 + x) * 4..][..4].try_into().unwrap()
}

#[test]
fn a_video_slot_draws_on_the_layered_stack_like_the_flat_sequence() {
    let (flat_layered, flat) = image(&owner(None));
    assert!(!flat_layered, "the control has no layered feature");
    let drawn = flat
        .chunks(4)
        .filter(|px| *px != [255, 255, 255, 255])
        .count();
    assert!(
        drawn > 10_000,
        "the video covers a large area of the frame: {drawn}"
    );
    // The fake decoder fills every pixel of a source frame with [tag, frame low, frame high, 255].
    let centre = at(&flat, 160, 90);
    assert_eq!(centre[3], 255);
    assert_eq!(
        &centre[1..3],
        &[FRAME as u8, 0],
        "the exact source frame is on screen"
    );
    for (blend_on, label) in [
        (Some(0), "blend on the drawn layer"),
        (Some(1), "blend on the video layer"),
    ] {
        let (layered, image) = image(&owner(blend_on));
        assert!(layered, "{label}: a blend selects the layer stack");
        assert_eq!(at(&image, 160, 90), centre, "{label}: the video pixel");
        // Outside the drawn layer's 10 x 10 corner the two images are one image (premultiplied rounding: one step).
        for y in (0..180).step_by(7) {
            for x in (12..320).step_by(5) {
                let worst = at(&image, x, y)
                    .iter()
                    .zip(at(&flat, x, y))
                    .map(|(a, b)| a.abs_diff(b))
                    .max()
                    .unwrap();
                assert!(
                    worst <= 1,
                    "{label}: layered vs flat at {x},{y}: {:?} vs {:?}",
                    at(&image, x, y),
                    at(&flat, x, y)
                );
            }
        }
    }
}
