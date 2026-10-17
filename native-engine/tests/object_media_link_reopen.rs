//! Pinned literals for the web reopen latch (G32, L09-C), hand-written from
//! `src/js/native-video-bridge.js` at fbe4b8a3 lines 826-880 and 806-812. The
//! scripted `W` oracle rows exercise the same latch through the real kernel.

use super::super::object_media_link_state::{FailureReason, LibraryEntry};
use super::{ReopenGate, WebReopenLatch, WEB_REOPEN_COOLDOWN_MS};

fn video(uid: &str) -> LibraryEntry {
    LibraryEntry {
        linked: true,
        layer_uid: Some(uid.to_owned()),
        ..LibraryEntry::default()
    }
}

#[test]
fn cooldown_constant_matches_the_baseline() {
    assert_eq!(WEB_REOPEN_COOLDOWN_MS, 4000);
}

#[test]
fn web_reopen_latch_warns_once_and_gates_on_its_own_clock() {
    // native-video-bridge.js:826-880.
    let mut l = WebReopenLatch::default();
    assert_eq!(l.gate(false, 5), ReopenGate::NoHandle { warn: true });
    assert_eq!(l.gate(false, 6), ReopenGate::NoHandle { warn: false });
    assert_eq!(l.gate(true, 7), ReopenGate::Attempt);
    let mut lib = vec![video("u1")];
    let f = l.attempt_failed(&mut lib, Some("u1"), false, 1000);
    assert_eq!(
        (f.reason, f.warn, f.reload_library),
        (FailureReason::Missing, false, true)
    );
    assert_eq!(l.gate(true, 4999), ReopenGate::CoolingDown);
    assert_eq!(l.gate(true, 5000), ReopenGate::Attempt);
    let f = l.attempt_failed(&mut lib, Some("u1"), true, 5000);
    assert_eq!(
        (f.reason, f.warn, f.reload_library),
        (FailureReason::PermissionNeeded, false, true)
    );
    l.retry_all();
    assert_eq!(l, WebReopenLatch::default());
    assert_eq!(
        l.gate(true, 5001),
        ReopenGate::Attempt,
        "a re-grant is never delayed"
    );
    assert!(l.attempt_succeeded(&mut lib, Some("u1")));
    assert!(!lib[0].broken);
}

#[test]
fn web_reopen_retry_stamp_zero_means_never_tried() {
    let mut l = WebReopenLatch::default();
    let mut lib = vec![video("u1")];
    l.attempt_failed(&mut lib, Some("u1"), true, 0);
    assert_eq!(
        l.gate(true, 1),
        ReopenGate::Attempt,
        "webRetryAt 0 is falsy in the baseline"
    );
}
