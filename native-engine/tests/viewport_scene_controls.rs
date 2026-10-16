//! Negative and mutation controls for the viewport_scene_* tests: each control
//! proves a guard used elsewhere can actually fail.
use super::viewport_scene_support as support;
use super::ObjectViewportFrame;
use crate::compositor::{
    object_test_plan as plan, object_test_selector as selector, schedule_test_object as schedule,
    Compositor,
};
use crate::desktop_viewport::ViewportStatus;
use crate::object_render_scene::prepare_scheduled_object_composition as prepare;
use crate::scheduler::FrameScheduler;
use std::panic::catch_unwind;

fn painted(pixel: [u8; 4]) -> Vec<u8> {
    // White canvas with the oracle rectangle [4,28) x [4,28) painted `pixel`.
    let mut bytes = vec![255u8; (support::SIZE * support::SIZE * 4) as usize];
    for y in 4..28usize {
        for x in 4..28usize {
            let at = (y * support::SIZE as usize + x) * 4;
            bytes[at..at + 4].copy_from_slice(&pixel);
        }
    }
    bytes
}

#[test]
fn cpu_viewport_scene_control_byte_comparator_sees_one_channel_and_length_skew() {
    let base = painted([64, 96, 159, 255]);
    assert_eq!(support::first_diff(&base, &base.clone()), None);
    let mut one = base.clone();
    one[(8 * 64 + 8) * 4 + 2] ^= 1;
    assert_eq!(support::first_diff(&base, &one), Some((8 * 64 + 8) * 4 + 2));
    assert_eq!(
        support::first_diff(&base, &base[..base.len() - 1]),
        Some(base.len() - 1)
    );
    assert!(catch_unwind(|| support::assert_same_bytes("mutant", &base, &one)).is_err());
}

#[test]
fn cpu_viewport_scene_control_oracle_check_accepts_both_roundings_and_rejects_wrong_pixels() {
    // The native-alpha and the baseline-alpha roundings of the same layers both pass.
    support::assert_matches_oracle(&painted([64, 96, 159, 255]), "a50");
    support::assert_matches_oracle(&painted([63, 95, 160, 255]), "a50");
    support::assert_matches_oracle(&painted([0, 0, 255, 255]), "a100");
    for wrong in [
        painted([66, 96, 159, 255]),
        painted([64, 96, 159, 200]),
        painted([255, 255, 255, 255]),
        painted([0, 0, 255, 255]),
    ] {
        assert!(catch_unwind(|| support::assert_matches_oracle(&wrong, "a50")).is_err());
    }
    // A fully opaque scene is not the half-alpha scene.
    assert!(
        catch_unwind(|| support::assert_matches_oracle(&painted([64, 96, 159, 255]), "a100"))
            .is_err()
    );
}

#[test]
fn cpu_viewport_scene_control_surface_guard_trips_on_any_call() {
    support::assert_no_surface_call(0, 0, "idle");
    assert!(catch_unwind(|| support::assert_no_surface_call(0, 1, "mutant")).is_err());
    assert!(catch_unwind(|| support::assert_no_surface_call(1, 2, "mutant")).is_err());
}

#[test]
fn gpu_viewport_scene_control_current_frame_reaches_the_surface_so_zero_call_checks_are_not_vacuous(
) {
    let mut gpu = Compositor::new().expect("L08-E requires real headless Metal");
    let snapshot = support::scene_owner("viewport-scene-positive", 0.5)
        .acquire_snapshot(0)
        .unwrap();
    let input = plan(&snapshot, 7);
    let mut scheduler = FrameScheduler::new();
    let work = schedule(&mut scheduler, &snapshot, 7);
    let live = prepare(&scheduler, &work, &input, &selector(&snapshot, 7)).unwrap();
    let frame = ObjectViewportFrame::compose(&mut gpu, &live).unwrap();
    let mut host = support::host_for(&gpu, &frame, &live, &[]);
    assert_eq!(
        frame.present(&mut host, &gpu, &live).unwrap().status(),
        &ViewportStatus::Presented
    );
    let port = host.test_port();
    assert_eq!((port.calls, port.frames, port.settlements), (1, 1, 1));
    assert_eq!(port.captured.len(), 1);
    assert!(catch_unwind(|| support::assert_no_surface_call(0, 1, "current frame")).is_err());
    drop(live);
    scheduler.succeed(work.work_id()).unwrap();
}

#[test]
fn gpu_viewport_scene_control_different_scenes_differ_and_one_scene_repeats_exactly() {
    let mut gpu = Compositor::new().expect("L08-E requires real headless Metal");
    let mut read = |instance: &str, alpha: f64| {
        let snapshot = support::scene_owner(instance, alpha)
            .acquire_snapshot(0)
            .unwrap();
        let input = plan(&snapshot, 7);
        let mut scheduler = FrameScheduler::new();
        let work = schedule(&mut scheduler, &snapshot, 7);
        let live = prepare(&scheduler, &work, &input, &selector(&snapshot, 7)).unwrap();
        let frame = ObjectViewportFrame::compose(&mut gpu, &live).unwrap();
        let bytes = gpu
            .readback_scheduled_object_rgba8(&frame.result, &live)
            .unwrap()
            .bytes(&live)
            .unwrap()
            .to_vec();
        drop(live);
        scheduler.succeed(work.work_id()).unwrap();
        bytes
    };
    let half = read("viewport-scene-differ-a", 0.5);
    let again = read("viewport-scene-differ-b", 0.5);
    let full = read("viewport-scene-differ-c", 1.0);
    support::assert_same_bytes("same scene, new owner instance", &half, &again);
    assert_eq!(support::first_diff(&half, &full), Some((4 * 64 + 4) * 4));
    assert!(catch_unwind(|| support::assert_same_bytes("mutant", &half, &full)).is_err());
}

#[test]
fn gpu_viewport_scene_control_permit_bypass_mutant_presents_a_cancelled_frame_and_trips_the_guard()
{
    let mut gpu = Compositor::new().expect("L08-E requires real headless Metal");
    let snapshot = support::scene_owner("viewport-scene-mutant", 0.5)
        .acquire_snapshot(0)
        .unwrap();
    let input = plan(&snapshot, 7);
    let mut scheduler = FrameScheduler::new();
    let work = schedule(&mut scheduler, &snapshot, 7);
    let (frame, mut host) = {
        let live = prepare(&scheduler, &work, &input, &selector(&snapshot, 7)).unwrap();
        let frame = ObjectViewportFrame::compose(&mut gpu, &live).unwrap();
        let host = support::host_for(&gpu, &frame, &live, &[]);
        (frame, host)
    };
    scheduler.cancel(work.work_id()).unwrap();
    let next = schedule(&mut scheduler, &snapshot, 0);
    let next_plan = plan(&snapshot, 0);
    let live = prepare(&scheduler, &next, &next_plan, &selector(&snapshot, 0)).unwrap();
    // Production path: refused before the surface.
    assert!(frame.present(&mut host, &gpu, &live).is_err());
    assert_eq!(host.test_port().calls, 0);
    // Mutant: skip the live permit via the test-only entry. The cancelled frame
    // is "presented", and the zero-call guard used by the matrix trips.
    let mutant = host
        .simulate_port_completion(frame.identity().clone())
        .unwrap();
    assert_eq!(mutant.status(), &ViewportStatus::Presented);
    let calls = host.test_port().calls;
    assert_eq!(calls, 1);
    assert!(catch_unwind(|| support::assert_no_surface_call(0, calls, "stale present")).is_err());
    drop(live);
    scheduler.cancel(next.work_id()).unwrap();
    let counters = scheduler.lease_counters();
    assert_eq!(
        (counters.acquired(), counters.released(), counters.live()),
        (2, 2, 0)
    );
}
