//! The media gate driving the REAL object export job (through the SVG host driver,
//! no GPU): source-frame mapping, remap, blend pairs, missing links refused before
//! staging, decode failures, media-free frames, and the lease counters of the real
//! scheduler. Published bytes are verified by the L09-E verifier, not by the encoder.
use super::support::{
    exactly_once, host_request, isolated, media_plan, owner, spec, staged_files, used, witness,
    Decoder, Scene, Stage,
};
use super::*;
use crate::export_job::native_media_export::native_media_export_lease::LeaseTally;
use crate::export_job::native_media_export::native_media_export_plan::MediaExportPlan;
use crate::export_job::native_media_export::native_media_export_verify::verify_output;
use crate::export_job::native_vector_export::native_vector_export_job::export_object_svg_frames;
use crate::export_job::JobStatus;
use crate::history::NativeObjectHistory;
use crate::object_document::media_timing::object_media_link_join::{
    MissingLink, MissingLinkReason,
};
use crate::object_document::media_timing::object_media_link_needs::tests as docs;
use crate::object_document::media_timing::object_media_link_needs::NeedsRefusal;
use crate::object_document::media_timing::object_media_link_plan::VideoFacts;
use crate::object_document::media_timing::object_media_link_state::{
    LinkedRef, ResolveOutcome, ResolveTable,
};
use crate::object_document::AssetKind;
use crate::object_render_scene::ObjectSceneResourceSet;
use crate::revision::{ImageResourceBudget, ObjectSnapshot};
use crate::scheduler::FrameScheduler;
use std::cell::RefCell;

type Seen = RefCell<Vec<Vec<String>>>;

/// One SVG batch (the plan's first) through the real host driver and the gate.
#[allow(clippy::too_many_arguments)]
fn run_svg(
    scene: &Scene,
    history: &NativeObjectHistory,
    snapshot: &ObjectSnapshot,
    plan: &MediaExportPlan,
    decoder: &Decoder,
    budget: &ImageResourceBudget,
    scheduler: &mut FrameScheduler,
    stage: &Stage,
    seen: &Seen,
) -> Result<MediaBatchOutcome, NeedsRefusal> {
    let batch = plan.batch(0).expect("first batch");
    let mut port = decoder.clone();
    let mut gate = MediaExportGate::new(&mut port, budget.clone(), snapshot.snapshot_id().into());
    let mut sink = stage.clone();
    run_gated_batch(
        &mut gate,
        &batch,
        |frame| scene.specs(frame),
        |frame, set| {
            let ids = set.entries().iter().map(|e| e.resource_id().to_owned());
            seen.borrow_mut().push(ids.collect());
            Ok(witness(snapshot, frame))
        },
        |witness_for| {
            export_object_svg_frames(
                history,
                scheduler,
                &mut sink,
                host_request(snapshot, batch.frames[0]),
                &batch.frames,
                witness_for,
            )
        },
    )
}

struct World {
    history: NativeObjectHistory,
    snapshot: ObjectSnapshot,
    decoder: Decoder,
    budget: ImageResourceBudget,
    stage: Stage,
    scheduler: FrameScheduler,
    seen: Seen,
}

fn world(name: &str, frames: &[u32]) -> World {
    let history = owner(name, frames);
    let snapshot = history.acquire_snapshot(0).unwrap();
    World {
        history,
        snapshot,
        decoder: Decoder::sized(4, 2),
        budget: isolated(1 << 20),
        stage: Stage::default(),
        scheduler: FrameScheduler::new(),
        seen: RefCell::new(Vec::new()),
    }
}

impl World {
    fn run(
        &mut self,
        scene: &Scene,
        first: u32,
        last: u32,
    ) -> Result<MediaBatchOutcome, NeedsRefusal> {
        let plan = media_plan(&self.snapshot, first, last);
        run_svg(
            scene,
            &self.history,
            &self.snapshot,
            &plan,
            &self.decoder,
            &self.budget,
            &mut self.scheduler,
            &self.stage,
            &self.seen,
        )
    }
    /// (acquired, released, live) of the real scheduler's lease manager.
    fn leases(&self) -> (u64, u64, u64) {
        let counters = self.scheduler.lease_counters();
        (counters.acquired(), counters.released(), counters.live())
    }
}

fn dims_and_names(w: &World, first: u32, last: u32) -> Vec<(String, u32, u32)> {
    let plan = media_plan(&w.snapshot, first, last);
    let published = w.stage.st().published.clone();
    let verified = verify_output(&plan, &staged_files(&published)).expect("independent verifier");
    verified
        .files
        .iter()
        .map(|f| (f.name.clone(), f.width, f.height))
        .collect()
}

#[test]
fn exported_frames_decode_the_kernel_source_frames_and_every_lease_returns_to_zero() {
    let mut w = world("media-host-map", &[4, 5, 6]);
    let scene = Scene::video(3, 20.0);
    let outcome = w.run(&scene, 4, 6).unwrap();
    // Timeline frames 4, 5, 6 at offset 3 show source frames 1, 2, 3 (clamp(N - 3, 0, 19)).
    assert_eq!(
        w.decoder.st().decoded,
        [
            ("vid".to_string(), 1),
            ("vid".to_string(), 2),
            ("vid".to_string(), 3)
        ]
    );
    // The scene producer was handed exactly those resources, per frame.
    assert_eq!(
        *w.seen.borrow(),
        [["media:vid:1"], ["media:vid:2"], ["media:vid:3"]]
    );
    let receipt = outcome.host.receipt.clone().unwrap();
    assert_eq!(receipt.status, JobStatus::Succeeded);
    assert_eq!(
        receipt.artifact.unwrap().files,
        ["frame_0001.svg", "frame_0002.svg", "frame_0003.svg"]
    );
    assert!(outcome.acquire_error.is_none());
    // Release: every decoded frame once, the session once, quota and scheduler back to zero.
    assert!(outcome.release.complete());
    assert_eq!(
        outcome.release.frames,
        LeaseTally {
            acquired: 3,
            released: 3,
            tokens: 0,
            pending: 0
        }
    );
    assert_eq!(w.decoder.st().released.len(), 3);
    assert!(exactly_once(&w.decoder.st().released));
    assert_eq!(w.decoder.st().closed, [1]);
    assert_eq!(used(&w.budget), 0);
    // One snapshot lease plus one media resource lease per exported frame.
    assert_eq!(w.leases(), (6, 6, 0));
    // Bytes, dimensions and frame count come from the verifier, not from the encoder.
    assert_eq!(
        dims_and_names(&w, 4, 6),
        [
            ("frame_0001.svg".to_string(), 64, 64),
            ("frame_0002.svg".to_string(), 64, 64),
            ("frame_0003.svg".to_string(), 64, 64)
        ]
    );
}

#[test]
fn a_time_remap_value_from_the_provider_port_drives_every_exported_frame() {
    let mut w = world("media-host-remap", &[4, 5, 6]);
    let mut scene = Scene::video(3, 20.0);
    scene
        .facts
        .video
        .insert("pv".into(), docs::video_facts(20.0, true));
    scene
        .facts
        .remap
        .insert("pv".into(), Box::new(docs::Remap(Some(7.0))));
    let outcome = w.run(&scene, 4, 6).unwrap();
    assert_eq!(
        w.decoder.st().decoded,
        [
            ("vid".to_string(), 7),
            ("vid".to_string(), 7),
            ("vid".to_string(), 7)
        ]
    );
    assert_eq!(outcome.host.receipt.unwrap().status, JobStatus::Succeeded);
    assert!(outcome.release.complete());
    assert!(exactly_once(&w.decoder.st().released));
    assert_eq!(w.leases(), (6, 6, 0));
}

#[test]
fn a_blend_pair_declares_and_releases_both_frames_of_each_exported_frame() {
    let mut w = world("media-host-blend", &[4, 5, 6]);
    let mut scene = Scene::video(0, 20.0);
    scene.facts.video.insert(
        "pv".into(),
        VideoFacts {
            frame_count: 20.0,
            time_remap: true,
            interp_mode: Some("blend".into()),
            frame_blend: false,
        },
    );
    scene
        .facts
        .remap
        .insert("pv".into(), Box::new(docs::Remap(Some(3.5))));
    let outcome = w.run(&scene, 4, 6).unwrap();
    let pair = [("vid".to_string(), 3), ("vid".to_string(), 4)];
    let expected: Vec<(String, u32)> = pair.iter().cloned().cycle().take(6).collect();
    assert_eq!(w.decoder.st().decoded, expected);
    assert!(outcome.release.complete());
    assert_eq!(outcome.release.frames.acquired, 6);
    // Three snapshot leases and six media resource leases.
    assert_eq!(w.leases(), (9, 9, 0));
    assert_eq!(used(&w.budget), 0);
}

fn resolved(path: &'static str) -> ResolveTable {
    let reference = LinkedRef {
        linked_path: Some(path),
        linked_handle_id: None,
    };
    let mut table = ResolveTable::new();
    table.begin(&reference, 0);
    table.finish(&mut [], &reference, ResolveOutcome::Resolved, 0);
    table
}

#[test]
fn a_missing_link_refuses_before_any_staging_decode_or_lease() {
    let mut w = world("media-host-missing", &[4, 5, 6]);
    let mut scene = Scene::video(0, 20.0);
    scene.inventory = docs::inventory(
        vec![docs::linked(AssetKind::Video, "vid", "/media/clip.mp4")],
        vec![docs::place("pv", "vid", docs::drawing("lv"), 0)],
    );
    let refusal = w.run(&scene, 4, 6).unwrap_err();
    assert_eq!(
        refusal,
        NeedsRefusal::MissingLink(MissingLink {
            asset_id: docs::aid("vid"),
            placement_id: "pv".into(),
            reason: MissingLinkReason::NotResolved,
        })
    );
    // Nothing happened: no staging, no session, no decode, no scheduler lease, no quota.
    assert!(w.stage.st().events.is_empty());
    assert!(w.decoder.st().events.is_empty());
    assert_eq!(w.leases(), (0, 0, 0));
    assert_eq!(used(&w.budget), 0);
    // The same export proceeds once the link has a verdict of "resolved".
    scene.table = resolved("/media/clip.mp4");
    let outcome = w.run(&scene, 4, 6).unwrap();
    assert_eq!(outcome.host.receipt.unwrap().status, JobStatus::Succeeded);
    assert_eq!(w.decoder.st().events[0], "open:vid");
}

#[test]
fn a_refusal_at_the_last_frame_of_the_batch_refuses_before_any_earlier_frame_is_staged() {
    // Frames 4 and 5 are exportable; the remap answers NaN only at frame 6. A gate that
    // preflighted frame by frame would already have opened a session and decoded 4 and 5.
    let mut w = world("media-host-late-refusal", &[4, 5, 6]);
    let mut scene = Scene::video(0, 20.0);
    scene
        .facts
        .video
        .insert("pv".into(), docs::video_facts(20.0, true));
    scene
        .facts
        .remap
        .insert("pv".into(), Box::new(docs::NanAt(6.0)));
    let refusal = w.run(&scene, 4, 6).unwrap_err();
    assert!(matches!(refusal, NeedsRefusal::Refused(_)));
    assert!(w.stage.st().events.is_empty());
    assert!(w.decoder.st().events.is_empty());
    assert!(w.decoder.st().decoded.is_empty());
    assert_eq!(w.leases(), (0, 0, 0));
    assert_eq!(used(&w.budget), 0);
}

#[test]
fn a_decode_failure_mid_sequence_fails_the_job_publishes_nothing_and_releases_all() {
    let mut w = world("media-host-decode", &[4, 5, 6]);
    let scene = Scene::video(3, 20.0);
    w.decoder.set().fail_decode_at = Some(2);
    let outcome = w.run(&scene, 4, 6).unwrap();
    let receipt = outcome.host.receipt.clone().unwrap();
    assert_eq!(receipt.status, JobStatus::Failed);
    assert_eq!(receipt.error.unwrap().code, "frame_failed");
    assert_eq!(
        outcome.acquire_error,
        Some(AcquireError::Decode {
            resource_id: "media:vid:2".into(),
            failure:
                crate::export_job::native_media_export::native_media_export_lease::DecodeFailure(
                    "decode refused".into()
                ),
        })
    );
    // Frame 1 was staged, then removed by the job's own cleanup; nothing was published.
    assert!(w.stage.st().published.is_empty());
    assert_eq!(w.stage.st().events, ["begin", "write", "cleanup"]);
    assert_eq!(w.stage.st().cleanups, [vec!["frame_0001.svg".to_string()]]);
    assert!(outcome.host.release.cleanup_complete);
    assert!(outcome.release.complete());
    assert_eq!(
        outcome.release.frames.acquired,
        outcome.release.frames.released
    );
    assert_eq!(w.decoder.st().released, [1]);
    assert_eq!(used(&w.budget), 0);
    assert_eq!(w.leases().2, 0);
}

#[test]
fn frames_without_media_resources_keep_an_empty_key_declaration() {
    let mut w = world("media-host-stills", &[4, 5]);
    let mut scene = Scene::video(0, 20.0);
    scene.inventory = docs::inventory(
        vec![docs::embedded(AssetKind::Image, "img")],
        vec![docs::place("pi", "img", docs::drawing("li"), 0)],
    );
    let outcome = w.run(&scene, 4, 5).unwrap();
    assert_eq!(outcome.host.receipt.unwrap().status, JobStatus::Succeeded);
    // A still needs no decode: no session, one snapshot lease per frame, as before media.
    assert!(w.decoder.st().events.is_empty());
    assert_eq!(w.leases(), (2, 2, 0));
    assert_eq!(
        *w.seen.borrow(),
        [Vec::<String>::new(), Vec::<String>::new()]
    );
}

#[test]
fn the_gate_attaches_the_media_set_and_the_key_declaration_matches_it() {
    let w = world("media-host-declare", &[4]);
    let plain = witness(&w.snapshot, 4);
    assert!(declared_resources(&plain).is_empty());
    let decoder = Decoder::sized(4, 2);
    let budget = isolated(1 << 20);
    let mut port = decoder.clone();
    let mut gate = MediaExportGate::new(&mut port, budget.clone(), w.snapshot.snapshot_id().into());
    let bound = gate
        .frame_witness(&[spec("vid", &[2])], |_| Ok(witness(&w.snapshot, 4)))
        .unwrap();
    assert_eq!(
        declared_resources(&bound),
        [(
            "media:vid:2".to_string(),
            w.snapshot.snapshot_id().to_string()
        )]
    );
    // A frame with nothing to decode leaves the witness exactly as produced.
    let untouched = gate
        .frame_witness(&[], |_| Ok(witness(&w.snapshot, 4)))
        .unwrap();
    assert!(untouched.scene_resources().is_none());
    // A producer that already declared resources is refused, never silently overwritten.
    let own = witness(&w.snapshot, 4)
        .with_scene_resources(std::sync::Arc::new(ObjectSceneResourceSet::empty()));
    let refused = gate.frame_witness(&[spec("vid", &[3])], |_| Ok(own));
    assert_eq!(
        refused.err(),
        Some("the produced witness already declares scene resources".to_string())
    );
    // A producer failure releases what was decoded for it.
    let failed = gate.frame_witness(&[spec("vid", &[4])], |_| Err("no scene".into()));
    assert_eq!(failed.err(), Some("no scene".to_string()));
    drop(bound);
    let done = gate.finish();
    assert!(done.complete());
    assert_eq!(done.frames.acquired, 3);
    assert_eq!(done.frames.released, 3);
    assert!(exactly_once(&decoder.st().released));
    assert_eq!(used(&budget), 0);
}
