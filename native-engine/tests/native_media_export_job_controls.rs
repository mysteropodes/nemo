//! Negative controls of the media export join, run against the real scheduler,
//! the real permit check and the real job: why the job's key must declare the
//! witness's media resources, what each wrong declaration is refused as, that the
//! ledger invariant holds after every step, and that a witness of the wrong frame
//! still returns its decoded frames.
use super::support::{
    exactly_once, host_request, isolated, output, owner, pins, spec, used, witness, Decoder, Scene,
};
use super::*;
use crate::export_job::native_media_export::native_media_export_lease::LeaseTally;
use crate::export_job::native_vector_export::native_vector_export_job::export_object_svg_frames;
use crate::export_job::ExportJobErrorKind;
use crate::object_render_scene::{
    prepare_scheduled_object_composition_from_witness, SceneResourceMismatchKind,
    ScheduledObjectSceneError,
};
use crate::scheduler::{EvaluationKey, FrameScheduler};

/// Schedule frame 4 of the pinned snapshot under a key declaring `declared`, then
/// ask the real permit check whether `witness` may be consumed under it.
fn permit_error(
    declared: Vec<(String, String)>,
    media: &ObjectCompositionWitness,
    snapshot: &crate::revision::ObjectSnapshot,
    scheduler: &mut FrameScheduler,
) -> (Option<ScheduledObjectSceneError>, u64) {
    let key = EvaluationKey::new(
        snapshot.snapshot_id(),
        "scene-root",
        4,
        "final",
        output(),
        declared,
    )
    .unwrap();
    let work = scheduler
        .schedule_object(snapshot.clone(), pins(snapshot, 4), key)
        .unwrap();
    let error = prepare_scheduled_object_composition_from_witness(scheduler, &work, media).err();
    let live_during = scheduler.lease_counters().live();
    scheduler.cancel(work.work_id()).unwrap();
    (error, live_during)
}

#[test]
fn the_job_key_must_declare_exactly_the_witness_resources() {
    let history = owner("media-ctl-key", &[4]);
    let snapshot = history.acquire_snapshot(0).unwrap();
    let decoder = Decoder::sized(4, 2);
    let budget = isolated(1 << 20);
    let mut port = decoder.clone();
    let mut gate = MediaExportGate::new(&mut port, budget.clone(), snapshot.snapshot_id().into());
    let bound = gate
        .frame_witness(&[spec("vid", &[2])], |_| Ok(witness(&snapshot, 4)))
        .unwrap();
    let snap = snapshot.snapshot_id().to_string();
    let mut scheduler = FrameScheduler::new();
    let kind = |e: Option<ScheduledObjectSceneError>| match e {
        Some(ScheduledObjectSceneError::ResourceDeclaration(kind)) => Some(kind),
        _ => None,
    };
    // Before this leaf the job declared `[]`: a witness holding media is refused (Extra).
    let (error, live) = permit_error(vec![], &bound, &snapshot, &mut scheduler);
    assert_eq!(kind(error), Some(SceneResourceMismatchKind::Extra));
    assert_eq!(live, 1, "only the snapshot lease");
    // Declaring a different version of the same resource is version skew.
    let skew = vec![("media:vid:2".to_string(), "another-snapshot".to_string())];
    let (error, _) = permit_error(skew, &bound, &snapshot, &mut scheduler);
    assert_eq!(kind(error), Some(SceneResourceMismatchKind::VersionSkew));
    // Declaring a resource the witness does not hold is Missing.
    let mut missing = declared_resources(&bound);
    missing.push(("media:vid:9".to_string(), snap.clone()));
    let (error, _) = permit_error(missing, &bound, &snapshot, &mut scheduler);
    assert_eq!(kind(error), Some(SceneResourceMismatchKind::Missing));
    // The declaration the patched job derives is accepted, and the scheduler then
    // leases the snapshot AND the media resource together.
    let declared = declared_resources(&bound);
    assert_eq!(declared, [("media:vid:2".to_string(), snap)]);
    let (error, live) = permit_error(declared, &bound, &snapshot, &mut scheduler);
    assert_eq!(error, None);
    assert_eq!(live, 2);
    assert_eq!(scheduler.lease_counters().live(), 0);
}

#[test]
fn the_ledger_invariant_holds_after_every_step() {
    let decoder = Decoder::sized(4, 2);
    let budget = isolated(1 << 20);
    let mut port = decoder.clone();
    let mut gate = MediaExportGate::new(&mut port, budget.clone(), "snap".into());
    let balanced = |tally: LeaseTally| {
        tally.live() == tally.tokens + tally.pending && tally.released <= tally.acquired
    };
    let first = gate.acquire(&[spec("vid", &[1, 2])]).unwrap();
    assert!(balanced(gate.tally()));
    decoder.set().fail_release = true;
    drop(first);
    assert!(balanced(gate.tally()));
    assert!(!gate.finish().complete());
    assert!(balanced(gate.tally()));
    decoder.set().fail_release = false;
    decoder.set().fail_decode_at = Some(4); // the second acquisition fails on its second frame
    let error = gate.acquire(&[spec("vid", &[5, 6])]).unwrap_err();
    assert!(matches!(error, AcquireError::Decode { .. }));
    assert!(balanced(gate.tally()));
    let done = gate.finish();
    assert!(balanced(gate.tally()) && done.complete());
    assert_eq!(done.frames.acquired, 3);
    assert_eq!(done.frames.released, 3);
    assert!(exactly_once(&decoder.st().released));
    assert_eq!(used(&budget), 0);
}

#[test]
fn the_exactly_once_checker_itself_detects_a_double_release() {
    assert!(exactly_once(&[2, 1, 3]));
    assert!(!exactly_once(&[1, 2, 2]));
    assert!(exactly_once(&[]));
}

#[test]
fn a_witness_of_another_frame_is_refused_by_the_job_and_still_returns_its_frames() {
    let history = owner("media-ctl-wrong-frame", &[4, 5]);
    let snapshot = history.acquire_snapshot(0).unwrap();
    let scene = Scene::video(0, 20.0);
    let decoder = Decoder::sized(4, 2);
    let budget = isolated(1 << 20);
    let stage = super::support::Stage::default();
    let mut scheduler = FrameScheduler::new();
    let (mut port, mut sink) = (decoder.clone(), stage.clone());
    let mut gate = MediaExportGate::new(&mut port, budget.clone(), snapshot.snapshot_id().into());
    let host = export_object_svg_frames(
        &history,
        &mut scheduler,
        &mut sink,
        host_request(&snapshot, 4),
        &[4],
        // The producer answers frame 5's scene for the requested frame 4.
        |frame| {
            let specs = scene.specs(frame).unwrap();
            gate.frame_witness(&specs, |_| Ok(witness(&snapshot, 5)))
        },
    );
    let report = gate.finish();
    assert_eq!(
        host.receipt.unwrap_err().kind,
        ExportJobErrorKind::InvalidRequest
    );
    assert!(stage.st().published.is_empty());
    assert!(report.complete());
    assert_eq!(decoder.st().released, [1]);
    assert_eq!(used(&budget), 0);
    // The job leased the snapshot and the media resource, then returned both.
    let counters = scheduler.lease_counters();
    assert_eq!(
        (counters.acquired(), counters.released(), counters.live()),
        (2, 2, 0)
    );
}

// ---- decoded pixels travel with their lease (T12) ------------------------------

fn token_pixels(set: &ObjectSceneResourceSet, id: &str) -> Vec<u8> {
    use crate::export_job::native_media_export::native_media_export_lease::FrameToken;
    let entry = set.get(id).expect("declared media frame");
    let token = entry
        .retention()
        .downcast_ref::<FrameToken>()
        .expect("a media entry retains its frame token");
    token.as_ref().to_vec()
}

#[test]
fn each_media_entry_owns_the_exact_pixels_its_source_frame_decoded_to() {
    use super::support::frame_pixels;
    let (decoder, budget) = (Decoder::sized(3, 2), isolated(1 << 20));
    let mut port = decoder.clone();
    let mut gate = MediaExportGate::new(&mut port, budget.clone(), "snap".into());
    let set = gate
        .acquire(&[spec("clip-a", &[7, 8]), spec("clip-b", &[7])])
        .unwrap();
    for (asset, frame) in [("clip-a", 7), ("clip-a", 8), ("clip-b", 7)] {
        let pixels = token_pixels(&set, &media_resource_id(asset, frame));
        assert_eq!(pixels, frame_pixels(asset, frame, 3 * 2 * 4));
    }
    // Independently known values: two sources at the same frame differ, and so do
    // two frames of the same source.
    assert_ne!(
        token_pixels(&set, "media:clip-a:7"),
        token_pixels(&set, "media:clip-b:7")
    );
    assert_ne!(
        token_pixels(&set, "media:clip-a:7"),
        token_pixels(&set, "media:clip-a:8")
    );
    assert_eq!(used(&budget), 3 * 3 * 2 * 4);
    drop(set);
    let done = gate.finish();
    assert!(done.complete());
    assert_eq!((done.frames.acquired, done.frames.released), (3, 3));
    assert!(exactly_once(&decoder.st().released));
    assert_eq!(used(&budget), 0);
}

#[test]
fn a_short_decoded_buffer_is_a_typed_decode_error_and_still_released_once() {
    let (decoder, budget) = (Decoder::sized(4, 2), isolated(1 << 20));
    decoder.set().short_by = 4;
    let mut port = decoder.clone();
    let mut gate = MediaExportGate::new(&mut port, budget.clone(), "snap".into());
    let error = gate.acquire(&[spec("vid", &[1])]).unwrap_err();
    assert!(matches!(
        &error,
        AcquireError::Decode { resource_id, failure }
            if resource_id == "media:vid:1" && failure.0 == "decoded 28 bytes, expected 32"
    ));
    assert_eq!(gate.take_error(), Some(error));
    // The frame the port handed over was released at once; no charge survives it.
    assert_eq!(decoder.st().released, [1]);
    assert_eq!(used(&budget), 0);
    let done = gate.finish();
    assert!(done.complete());
    assert_eq!((done.frames.acquired, done.frames.released), (1, 1));
}

#[test]
fn a_scene_join_that_merged_the_media_set_is_accepted_unchanged() {
    use std::sync::Arc;
    let history = owner("media-merged", &[4]);
    let snapshot = history.acquire_snapshot(0).unwrap();
    let decoder = Decoder::sized(2, 2);
    let mut port = decoder.clone();
    let mut gate =
        MediaExportGate::new(&mut port, isolated(1 << 20), snapshot.snapshot_id().into());
    let merged = gate
        .frame_witness(&[spec("vid", &[2])], |media| {
            // What a scene join does: its own declarations plus the very media entries.
            Ok(witness(&snapshot, 4).with_scene_resources(Arc::clone(media)))
        })
        .unwrap();
    assert_eq!(
        declared_resources(&merged),
        [(
            "media:vid:2".to_string(),
            snapshot.snapshot_id().to_string()
        )]
    );
    // A set that only names the id, with a different retention, is not the same set.
    let forged = gate.frame_witness(&[spec("vid", &[3])], |_| {
        let entry = crate::object_render_scene::scene_effect_kernels::object_scene_resource_set::SceneResourceEntry::new(
            "media:vid:3",
            snapshot.snapshot_id(),
            crate::object_render_scene::scene_effect_kernels::object_scene_resource_set::SceneResourceKind::Media,
            16,
            Arc::new(()),
        );
        Ok(witness(&snapshot, 4)
            .with_scene_resources(Arc::new(ObjectSceneResourceSet::new(vec![entry]).unwrap())))
    });
    assert_eq!(
        forged.err(),
        Some("the produced witness already declares scene resources".to_string())
    );
    drop(merged);
    let done = gate.finish();
    assert!(done.complete());
    assert!(exactly_once(&decoder.st().released));
}
