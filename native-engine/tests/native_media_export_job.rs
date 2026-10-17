//! The decode gate without a job: resource ids, versions, charges, the
//! exactly-once release rule, debt on a refusing port, and typed acquisition
//! failures. Every expectation is a literal (a 4x2 RGBA8 frame is 32 bytes).
use super::support::{exactly_once, isolated, spec, used, Decoder};
use super::*;
use crate::export_job::native_media_export::native_media_export_lease::LeaseTally;
use crate::object_render_scene::scene_effect_kernels::object_scene_resource_set::SceneResourceKind;

fn gate<'p>(
    port: &'p mut Decoder,
    budget: &crate::revision::ImageResourceBudget,
) -> MediaExportGate<'p, Decoder> {
    MediaExportGate::new(port, budget.clone(), "snap-1".into())
}

#[test]
fn resource_ids_versions_and_charges_are_literal_and_one_session_serves_every_frame() {
    let decoder = Decoder::sized(4, 2);
    let budget = isolated(1 << 20);
    let mut port = decoder.clone();
    let mut gate = gate(&mut port, &budget);
    // Repeats decode once; the set is sorted bytewise by resource id.
    let set = gate
        .acquire(&[spec("vid", &[9, 3, 4]), spec("vid", &[4])])
        .unwrap();
    let ids: Vec<&str> = set.entries().iter().map(|e| e.resource_id()).collect();
    assert_eq!(ids, ["media:vid:3", "media:vid:4", "media:vid:9"]);
    for entry in set.entries() {
        assert_eq!(entry.version(), "snap-1");
        assert_eq!(entry.kind(), SceneResourceKind::Media);
        assert_eq!(entry.charged_bytes(), 32);
    }
    assert_eq!(set.total_charged_bytes(), 96);
    assert_eq!(
        decoder.st().decoded,
        [
            ("vid".to_string(), 9),
            ("vid".to_string(), 3),
            ("vid".to_string(), 4)
        ]
    );
    assert_eq!(decoder.st().events[0], "open:vid");
    assert_eq!(
        decoder
            .st()
            .events
            .iter()
            .filter(|e| e.starts_with("open:"))
            .count(),
        1
    );
    assert_eq!(used(&budget), 96);
    assert_eq!(
        gate.tally(),
        LeaseTally {
            acquired: 3,
            released: 0,
            tokens: 3,
            pending: 0
        }
    );
    // The id grammar: the last colon separates the decimal frame from the asset id.
    assert_eq!(media_resource_id("a:1", 2), "media:a:1:2");
}

#[test]
fn a_frame_is_released_exactly_once_after_its_last_holder_drops() {
    let decoder = Decoder::sized(4, 2);
    let budget = isolated(1 << 20);
    let mut port = decoder.clone();
    let mut gate = gate(&mut port, &budget);
    let set = gate.acquire(&[spec("vid", &[1, 2])]).unwrap();
    let holder = Arc::clone(&set);
    drop(set);
    // A clone still holds the set (a witness, a pending frame): nothing may be released.
    let held = gate.finish();
    assert!(!held.complete());
    assert_eq!((held.frames.live(), held.sessions_open), (2, 1));
    assert!(decoder.st().released.is_empty() && decoder.st().closed.is_empty());
    assert_eq!(used(&budget), 64);
    drop(holder);
    let done = gate.finish();
    assert!(done.complete());
    assert_eq!(
        done.frames,
        LeaseTally {
            acquired: 2,
            released: 2,
            tokens: 0,
            pending: 0
        }
    );
    assert_eq!((done.sessions_opened, done.sessions_closed), (1, 1));
    assert_eq!(decoder.st().released, [1, 2]);
    assert_eq!(decoder.st().closed, [1]);
    assert!(exactly_once(&decoder.st().released));
    assert_eq!(used(&budget), 0);
    // Finishing again repeats nothing.
    assert_eq!(gate.finish(), done);
    assert_eq!(decoder.st().released.len(), 2);
    assert_eq!(decoder.st().closed.len(), 1);
}

#[test]
fn a_refusing_port_leaves_debt_and_the_quota_stays_charged_until_release_succeeds() {
    let decoder = Decoder::sized(4, 2);
    let budget = isolated(1 << 20);
    let mut port = decoder.clone();
    let mut gate = gate(&mut port, &budget);
    decoder.set().fail_release = true;
    drop(gate.acquire(&[spec("vid", &[5])]).unwrap());
    let stuck = gate.finish();
    assert!(!stuck.complete());
    assert_eq!(
        stuck.frames,
        LeaseTally {
            acquired: 1,
            released: 0,
            tokens: 0,
            pending: 1
        }
    );
    // The decoded bytes still exist, so the quota is still charged and the session open.
    assert_eq!(used(&budget), 32);
    assert!(decoder.st().closed.is_empty());
    assert_eq!(
        gate.last_release_error(),
        Some(&DecodeFailure("release refused".into()))
    );
    decoder.set().fail_release = false;
    let done = gate.finish();
    assert!(done.complete());
    assert_eq!(decoder.st().released, [1]);
    assert_eq!(decoder.st().closed, [1]);
    assert_eq!(used(&budget), 0);
}

#[test]
fn a_refused_close_is_retried_and_never_forgotten() {
    let decoder = Decoder::sized(4, 2);
    let budget = isolated(1 << 20);
    let mut port = decoder.clone();
    let mut gate = gate(&mut port, &budget);
    drop(gate.acquire(&[spec("vid", &[0])]).unwrap());
    decoder.set().fail_close = true;
    let stuck = gate.finish();
    assert!(!stuck.complete());
    assert_eq!(
        (
            stuck.frames.live(),
            stuck.sessions_open,
            stuck.sessions_closed
        ),
        (0, 1, 0)
    );
    decoder.set().fail_close = false;
    let done = gate.finish();
    assert!(done.complete());
    assert_eq!((done.sessions_opened, done.sessions_closed), (1, 1));
    assert_eq!(decoder.st().closed, [1]);
    let attempts = |e: &&String| e.starts_with("close:");
    assert_eq!(decoder.st().events.iter().filter(attempts).count(), 2);
}

#[test]
fn a_decode_failure_releases_the_earlier_frames_and_is_typed() {
    let decoder = Decoder::sized(4, 2);
    let budget = isolated(1 << 20);
    let mut port = decoder.clone();
    let mut gate = gate(&mut port, &budget);
    decoder.set().fail_decode_at = Some(2);
    let error = gate.acquire(&[spec("vid", &[1, 2, 3])]).unwrap_err();
    assert_eq!(
        error,
        AcquireError::Decode {
            resource_id: "media:vid:2".into(),
            failure: DecodeFailure("decode refused".into()),
        }
    );
    // Frame 3 was never asked for; frame 1 was released by the failed call itself.
    assert_eq!(decoder.st().decoded.len(), 2);
    assert_eq!(decoder.st().released, [1]);
    assert_eq!(used(&budget), 0);
    assert_eq!(gate.take_error(), Some(error));
    assert_eq!(gate.take_error(), None);
}

#[test]
fn the_budget_refuses_before_any_decode_and_the_earlier_frames_are_released() {
    let decoder = Decoder::sized(4, 2);
    let budget = isolated(64); // two 32-byte frames fit, a third does not
    let mut port = decoder.clone();
    let mut gate = gate(&mut port, &budget);
    let error = gate.acquire(&[spec("vid", &[1, 2, 3])]).unwrap_err();
    assert_eq!(
        error,
        AcquireError::Budget {
            resource_id: "media:vid:3".into(),
            error: ImageResourceError::Capacity,
        }
    );
    assert_eq!(
        decoder.st().decoded.len(),
        2,
        "no decode for the refused frame"
    );
    assert_eq!(decoder.st().released, [1, 2]);
    assert_eq!(used(&budget), 0);
}

#[test]
fn open_failures_and_zero_sized_sessions_are_typed() {
    let decoder = Decoder::sized(4, 2);
    let budget = isolated(1 << 20);
    let mut port = decoder.clone();
    let mut refused = gate(&mut port, &budget);
    decoder.set().fail_open = true;
    assert_eq!(
        refused.acquire(&[spec("vid", &[1])]).unwrap_err(),
        AcquireError::Open {
            asset_id: "vid".into(),
            failure: DecodeFailure("open refused".into()),
        }
    );
    assert_eq!(refused.finish().sessions_opened, 0);
    drop(refused);

    let empty = Decoder::sized(0, 5);
    let mut port = empty.clone();
    let mut zero = gate(&mut port, &budget);
    assert_eq!(
        zero.acquire(&[spec("vid", &[1])]).unwrap_err(),
        AcquireError::EmptyFrame {
            asset_id: "vid".into()
        }
    );
    assert!(empty.st().decoded.is_empty());
    let done = zero.finish();
    assert!(done.complete());
    assert_eq!((done.sessions_opened, done.sessions_closed), (1, 1));
    assert_eq!(used(&budget), 0);
}

#[test]
fn frames_of_an_earlier_exported_frame_are_released_before_the_next_decode() {
    let decoder = Decoder::sized(4, 2);
    let budget = isolated(1 << 20);
    let mut port = decoder.clone();
    let mut gate = gate(&mut port, &budget);
    drop(gate.acquire(&[spec("vid", &[1])]).unwrap());
    drop(gate.acquire(&[spec("vid", &[2])]).unwrap());
    assert_eq!(
        decoder.st().events,
        ["open:vid", "decode:vid:1", "release:1", "decode:vid:2"]
    );
    assert_eq!(gate.finish().frames.live(), 0);
    assert_eq!(decoder.st().released, [1, 2]);
}

#[test]
fn sessions_open_once_per_asset_and_close_newest_first() {
    let decoder = Decoder::sized(4, 2);
    let budget = isolated(1 << 20);
    let mut port = decoder.clone();
    let mut gate = gate(&mut port, &budget);
    drop(gate.acquire(&[spec("a", &[1]), spec("b", &[1])]).unwrap());
    drop(gate.acquire(&[spec("a", &[2])]).unwrap());
    let done = gate.finish();
    assert!(done.complete());
    assert_eq!(done.sessions_opened, 2);
    assert_eq!(decoder.st().closed, [2, 1]);
    assert!(exactly_once(&decoder.st().released));
    assert_eq!(decoder.st().released.len(), 3);
}

#[test]
fn dropping_the_gate_without_finish_still_releases_everything() {
    let decoder = Decoder::sized(4, 2);
    let budget = isolated(1 << 20);
    {
        let mut port = decoder.clone();
        let mut gate = gate(&mut port, &budget);
        drop(gate.acquire(&[spec("vid", &[7])]).unwrap());
    }
    assert_eq!(decoder.st().released, [1]);
    assert_eq!(decoder.st().closed, [1]);
    assert_eq!(used(&budget), 0);
}

#[test]
fn an_empty_version_is_refused_by_the_set_and_the_decoded_frame_is_returned() {
    let decoder = Decoder::sized(4, 2);
    let budget = isolated(1 << 20);
    let mut port = decoder.clone();
    let mut gate = MediaExportGate::new(&mut port, budget.clone(), String::new());
    // The scene resource set refuses an empty version; the frame it was built from is
    // not leaked: no token outlives the refused set.
    assert_eq!(
        gate.acquire(&[spec("vid", &[1])]).unwrap_err(),
        AcquireError::Set(SceneResourceSetError::EmptyVersion {
            resource_id: "media:vid:1".into()
        })
    );
    assert_eq!(decoder.st().released, [1]);
    assert_eq!(used(&budget), 0);
    assert!(gate.finish().complete());
}

#[test]
fn the_production_constructor_charges_the_shared_image_budget() {
    // The shared budget is process-wide and other tests run beside this one, so only the
    // outcome is asserted: one 32-byte frame is decoded, released once and the session closed.
    let decoder = Decoder::sized(4, 2);
    let mut port = decoder.clone();
    let mut gate = MediaExportGate::with_shared_budget(&mut port, "snap-1".into());
    let set = gate.acquire(&[spec("vid", &[3])]).unwrap();
    assert_eq!(set.total_charged_bytes(), 32);
    drop(set);
    assert!(gate.finish().complete());
    assert_eq!(decoder.st().released, [1]);
    assert_eq!(decoder.st().closed, [1]);
}
