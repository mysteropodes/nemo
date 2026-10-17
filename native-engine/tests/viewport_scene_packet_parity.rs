//! One live scheduled frame: viewport, readback, PNG and export job all consume
//! the same native packet and produce the same bytes. Metal results are source
//! evidence for the shared appointment, not installed display acceptance.
use super::viewport_scene_support as support;
use super::ObjectViewportFrame;
use crate::compositor::{
    object_test_plan as plan, object_test_selector as selector, schedule_test_object as schedule,
    Compositor,
};
use crate::desktop_viewport::ViewportStatus;
use crate::export_job::{JobStatus, ObjectExportBegin, ObjectExportJob};
use crate::object_render_scene::{
    prepare_object_composition, prepare_scheduled_object_composition as prepare,
};
use crate::png_output::{encode_scheduled_object_png, ObjectPngEncoder};
use crate::render_scene::ScheduledFrameIdentity;
use crate::scheduler::{FrameScheduler, OutputSpec, PublicationDisposition};

#[test]
fn cpu_viewport_scene_oracle_pins_baseline_facts_and_both_scene_literals() {
    let facts = support::oracle_facts();
    let find = |name: &str| {
        let hits: Vec<_> = facts.iter().filter(|f| f[0] == name).collect();
        assert_eq!(hits.len(), 1, "{name}");
        hits[0].clone()
    };
    // Baseline engine.rs: one composite_scene body serves render and render_to_pixels.
    assert_eq!(
        find("shared-composite"),
        ["shared-composite", "composite_scene", "line", "3105"]
    );
    // The only divergence is the base colour: opaque pasteboard vs transparent.
    assert_eq!(
        find("render-base-colour"),
        ["render-base-colour", "0x20", "0x1f", "0x25", "0xff"]
    );
    assert_eq!(
        find("export-base-colour"),
        ["export-base-colour", "transparent"]
    );
    assert_eq!(
        find("export-bridge"),
        [
            "export-bridge",
            "forExport=true",
            "includeEditorOverlays=false",
            "entry=render_to_pixels"
        ]
    );
    let half = support::oracle_rows("a50");
    assert_eq!(half.len(), 6);
    assert_eq!((half[0].x, half[0].y), (8, 8));
    assert_eq!(half[0].baseline, [63.25, 95.25, 159.5, 255.0]);
    assert_eq!(half[0].native, [63.75, 95.63, 159.38, 255.0]);
    assert_eq!(half[4].baseline, [255.0, 255.0, 255.0, 255.0]);
    let full = support::oracle_rows("a100");
    assert_eq!(full[0].baseline, [0.0, 0.0, 255.0, 255.0]);
    assert_eq!(full[0].native, [0.0, 0.0, 255.0, 255.0]);
    assert!(support::oracle_rows("missing").is_empty());
}

#[test]
fn cpu_viewport_scene_artwork_witness_is_not_viewport_and_export_witness_is_the_same_packet() {
    let owner = support::scene_owner("viewport-scene-witness", 0.5);
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let input = plan(&snapshot, 7);
    let mut scheduler = FrameScheduler::new();
    let work = schedule(&mut scheduler, &snapshot, 7);
    let live = prepare(&scheduler, &work, &input, &selector(&snapshot, 7)).unwrap();
    assert_eq!(
        live.identity(),
        &ScheduledFrameIdentity::from_scheduled(&work)
    );
    assert_eq!(live.dimensions(), (64, 64));
    let viewport_side = live.prepared().composition();
    assert!(!viewport_side.is_viewport());
    assert_eq!(viewport_side.plan().commands().len(), 3);
    // The export job is handed a witness built from the same plan and pins.
    let export_side = prepare_object_composition(&input, &selector(&snapshot, 7)).unwrap();
    assert!(export_side.same_identity_as(viewport_side));
    assert_eq!(export_side.plan().commands().len(), 3);
    assert_eq!(export_side.frame(), 7);
    // Another instance is a different packet, so identity equality is not vacuous.
    let other = support::scene_owner("viewport-scene-witness-other", 0.5);
    let other_snapshot = other.acquire_snapshot(0).unwrap();
    let foreign =
        prepare_object_composition(&plan(&other_snapshot, 7), &selector(&other_snapshot, 7))
            .unwrap();
    assert!(!foreign.same_identity_as(viewport_side));
    drop(live);
    scheduler.cancel(work.work_id()).unwrap();
    assert_eq!(scheduler.lease_counters().live(), 0);
}

#[test]
fn gpu_viewport_scene_one_live_frame_gives_identical_readback_png_and_presented_pixels() {
    let mut gpu = Compositor::new().expect("L08-E requires real headless Metal");
    for (scene, alpha) in [("a50", 0.5), ("a100", 1.0)] {
        let owner = support::scene_owner("viewport-scene-parity", alpha);
        let snapshot = owner.acquire_snapshot(0).unwrap();
        let input = plan(&snapshot, 7);
        let mut scheduler = FrameScheduler::new();
        let work = schedule(&mut scheduler, &snapshot, 7);
        let expected_identity = ScheduledFrameIdentity::from_scheduled(&work);
        let live = prepare(&scheduler, &work, &input, &selector(&snapshot, 7)).unwrap();
        let frame = ObjectViewportFrame::compose(&mut gpu, &live).unwrap();

        // Readback through the single scheduled-object boundary, vs the oracle.
        let readback = gpu
            .readback_scheduled_object_rgba8(&frame.result, &live)
            .unwrap();
        assert_eq!(readback.dimensions(), (64, 64));
        let bytes = readback.bytes(&live).unwrap().to_vec();
        support::assert_matches_oracle(&bytes, scene);

        // PNG of the same composed result, and PNG through the export encoder
        // (which composes again from the same live permit).
        let same_result = encode_scheduled_object_png(&gpu, &frame.result, &live, 65536).unwrap();
        support::assert_same_bytes(
            "png of the same result",
            &support::decode_png(same_result.bytes(&live).unwrap()),
            &bytes,
        );
        let exported = gpu.encode_object_png(&live, 65536).unwrap();
        support::assert_same_bytes(
            "png through the export encoder",
            &support::decode_png(exported.bytes(&live).unwrap()),
            &bytes,
        );

        // The viewport double receives the same texture; read its bytes at acquire.
        let mut host = support::host_for(&gpu, &frame, &live, &[]);
        let receipt = frame.present(&mut host, &gpu, &live).unwrap();
        assert_eq!(receipt.status(), &ViewportStatus::Presented);
        assert_eq!(receipt.identity(), &expected_identity);
        let port = host.test_port();
        assert_eq!((port.calls, port.frames, port.settlements), (1, 1, 1));
        assert_eq!(port.captured.len(), 1);
        support::assert_same_bytes("presented texture", &port.captured[0], &bytes);
        assert_eq!(port.identities, vec![expected_identity.clone()]);

        // One identity across every consumer of this frame.
        for identity in [
            live.identity(),
            frame.identity(),
            readback.identity(),
            same_result.identity(),
            exported.identity(),
        ] {
            assert_eq!(identity, &expected_identity);
        }
        drop(live);
        let receipt = scheduler.succeed(work.work_id()).unwrap();
        assert_eq!(receipt.publication(), &PublicationDisposition::Published);
        let counters = scheduler.lease_counters();
        assert_eq!(
            (counters.acquired(), counters.released(), counters.live()),
            (1, 1, 0)
        );
    }
}

#[test]
fn gpu_viewport_scene_export_job_png_equals_viewport_readback_of_the_same_packet() {
    let mut gpu = Compositor::new().expect("L08-E requires real headless Metal");
    let owner = support::scene_owner("viewport-scene-export", 0.5);
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let input = plan(&snapshot, 7);
    let mut scheduler = FrameScheduler::new();

    // Phase 1: the viewport frame and its readback on the caller's scheduler.
    let work = schedule(&mut scheduler, &snapshot, 7);
    let live = prepare(&scheduler, &work, &input, &selector(&snapshot, 7)).unwrap();
    let witness = prepare_object_composition(&input, &selector(&snapshot, 7)).unwrap();
    assert!(witness.same_identity_as(live.prepared().composition()));
    let frame = ObjectViewportFrame::compose(&mut gpu, &live).unwrap();
    let readback = gpu
        .readback_scheduled_object_rgba8(&frame.result, &live)
        .unwrap();
    let bytes = readback.bytes(&live).unwrap().to_vec();
    support::assert_matches_oracle(&bytes, "a50");
    let mut host = support::host_for(&gpu, &frame, &live, &[]);
    assert_eq!(
        frame.present(&mut host, &gpu, &live).unwrap().status(),
        &ViewportStatus::Presented
    );
    support::assert_same_bytes("presented", &host.test_port().captured[0], &bytes);
    drop(live);
    scheduler.succeed(work.work_id()).unwrap();

    // Phase 2: the real export job schedules its own work for the same frame.
    let request = ObjectExportBegin {
        request_id: "viewport-scene-export".into(),
        target: "/memory/viewport-scene".into(),
        pins: support::pins(&snapshot, 7),
        output: OutputSpec::new("frame", "rgba8", 64, 64, "srgb", "straight").unwrap(),
        maximum_encoded_bytes: 65536,
    };
    let mut port = support::MemoryPort::default();
    {
        let mut job = ObjectExportJob::new(&mut port, &mut scheduler);
        let begun = job.begin(&owner, request, witness).unwrap();
        assert_eq!(begun.status, JobStatus::Running);
        assert_eq!(begun.pinned_revision, 0);
        assert_eq!(begun.document_snapshot_id, snapshot.snapshot_id());
        let pending = job.start_frame(&mut gpu).unwrap().expect("frame encoded");
        let done = job.finish_frame(pending).unwrap();
        assert_eq!(done.status, JobStatus::Succeeded);
        assert_eq!(done.progress, 1.0);
    }
    assert_eq!(port.names, ["frame_0001.png"]);
    assert!(port.published);
    support::assert_same_bytes("export job png", &support::decode_png(&port.bytes), &bytes);
    let counters = scheduler.lease_counters();
    assert_eq!(
        (counters.acquired(), counters.released(), counters.live()),
        (2, 2, 0)
    );
}
