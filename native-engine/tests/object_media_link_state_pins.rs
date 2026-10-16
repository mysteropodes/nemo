//! Pinned literals for the link-state kernel (G32, L09-C). Every expected
//! value below is written out by hand from the baseline source named in the
//! test, independent of the oracle file, so a regenerated oracle cannot
//! silently move them.

use super::super::object_media_link_reopen::WEB_REOPEN_COOLDOWN_MS;
use super::{
    entries_for_key, key_for, mark_broken, mark_ok, mark_video_broken, mark_video_ok,
    video_entry_index, FailureReason, LibraryEntry, LinkDisposition, LinkKey, LinkedRef,
    PermissionAnswer, ResolveOutcome, ResolveStart, ResolveTable, Settled, RETRY_COOLDOWN_MS,
};

const A: LinkedRef<'static> = LinkedRef {
    linked_path: Some("/m/a.png"),
    linked_handle_id: None,
};
const H: LinkedRef<'static> = LinkedRef {
    linked_path: None,
    linked_handle_id: Some("h-b"),
};

fn entry(path: Option<&str>, handle: Option<&str>, uid: Option<&str>) -> LibraryEntry {
    LibraryEntry {
        linked: true,
        path: path.map(str::to_owned),
        web_handle_id: handle.map(str::to_owned),
        layer_uid: uid.map(str::to_owned),
        ..LibraryEntry::default()
    }
}

fn fail(t: &mut ResolveTable, r: &LinkedRef<'_>, o: ResolveOutcome, at: i64) {
    assert_eq!(t.begin(r, at), ResolveStart::Started);
    t.finish(&mut [], r, o, at);
}

#[test]
fn desktop_path_wins_over_web_handle_and_neither_is_unlinked() {
    // linked-media.js:88-92 keyFor.
    let both = LinkedRef {
        linked_path: Some("/m/c.png"),
        linked_handle_id: Some("h-c"),
    };
    assert_eq!(key_for(&both).unwrap().tag(), "p:/m/c.png");
    assert_eq!(key_for(&H).unwrap().tag(), "w:h-b");
    assert_eq!(key_for(&LinkedRef::default()), None);
    let empty = LinkedRef {
        linked_path: Some(""),
        linked_handle_id: Some(""),
    };
    assert_eq!(
        key_for(&empty),
        None,
        "empty strings are falsy in the baseline"
    );
    let empty_path = LinkedRef {
        linked_path: Some(""),
        linked_handle_id: Some("h-b"),
    };
    assert_eq!(
        key_for(&empty_path),
        Some(LinkKey::Handle("h-b".to_owned()))
    );
}

#[test]
fn locator_spelling_is_carried_byte_for_byte() {
    for raw in [
        "C:\\Users\\x y\\clip \u{e9}.mov",
        " lead",
        "trail ",
        "../up/./x//y/",
        "~/m.png",
        "%41",
        "p:x",
    ] {
        let r = LinkedRef {
            linked_path: Some(raw),
            linked_handle_id: None,
        };
        let key = key_for(&r).unwrap();
        assert_eq!(key.locator().as_bytes(), raw.as_bytes());
        assert_eq!(key.tag(), format!("p:{raw}"));
    }
}

#[test]
fn entries_match_by_path_or_handle_not_by_key_precedence() {
    // linked-media.js:114-118 _entriesForKey.
    let lib = vec![
        entry(Some("/m/c.png"), None, None),
        entry(None, Some("h-c"), None),
        entry(Some("/m/other.png"), Some("h-x"), None),
    ];
    let both = LinkedRef {
        linked_path: Some("/m/c.png"),
        linked_handle_id: Some("h-c"),
    };
    assert_eq!(entries_for_key(&lib, &both), vec![0, 1]);
    assert_eq!(key_for(&both).unwrap().tag(), "p:/m/c.png");
    let mut unlinked = lib.clone();
    unlinked[0].linked = false;
    assert_eq!(
        entries_for_key(&unlinked, &both),
        vec![1],
        "an embedded entry never matches"
    );
}

#[test]
fn cooldown_gate_is_4000_ms_exclusive_on_an_injected_clock() {
    assert_eq!((RETRY_COOLDOWN_MS, WEB_REOPEN_COOLDOWN_MS), (4000, 4000));
    for (dt, want) in [
        (3999, ResolveStart::CoolingDown),
        (4000, ResolveStart::Started),
        (4001, ResolveStart::Started),
    ] {
        let mut t = ResolveTable::new();
        fail(&mut t, &A, ResolveOutcome::ReadFailed, 1000);
        assert_eq!(t.begin(&A, 1000 + dt), want, "dt {dt}");
    }
    let mut t = ResolveTable::new();
    fail(&mut t, &A, ResolveOutcome::ReadFailed, 5000);
    assert_eq!(
        t.begin(&A, 1000),
        ResolveStart::CoolingDown,
        "clock went backwards"
    );
    assert_eq!(
        t.begin(&A, i64::MIN),
        ResolveStart::CoolingDown,
        "no arithmetic overflow"
    );
}

#[test]
fn permission_failure_is_distinct_from_missing() {
    // linked-media.js:174-177: needsPermission -> 'permission' else 'missing'.
    for (outcome, reason, tag) in [
        (
            ResolveOutcome::PermissionNeeded,
            FailureReason::PermissionNeeded,
            "permission",
        ),
        (
            ResolveOutcome::HandleMissing,
            FailureReason::Missing,
            "missing",
        ),
        (
            ResolveOutcome::ReadFailed,
            FailureReason::Missing,
            "missing",
        ),
    ] {
        let mut t = ResolveTable::new();
        fail(&mut t, &H, outcome, 7);
        let f = t.failure(&H).unwrap();
        assert_eq!((f.reason, f.at_ms, f.reason.tag()), (reason, 7, tag));
    }
}

#[test]
fn in_flight_is_deduplicated_and_unlinked_never_starts() {
    let mut t = ResolveTable::new();
    assert_eq!(t.begin(&LinkedRef::default(), 0), ResolveStart::Unlinked);
    assert_eq!(t.begin(&A, 0), ResolveStart::Started);
    assert_eq!(t.begin(&A, 0), ResolveStart::AlreadyInFlight);
    assert!(t.in_flight(&A));
    t.finish(&mut [], &A, ResolveOutcome::Resolved, 1);
    assert!(!t.in_flight(&A));
    assert_eq!(
        t.begin(&A, 2),
        ResolveStart::Started,
        "begin does not consult the pixel cache"
    );
}

#[test]
fn dispositions_follow_one_link_through_its_lifecycle() {
    let mut t = ResolveTable::new();
    assert_eq!(
        t.disposition(&LinkedRef::default(), 0),
        LinkDisposition::Unlinked
    );
    assert_eq!(t.disposition(&A, 0), LinkDisposition::Pending);
    t.begin(&A, 0);
    assert_eq!(
        t.disposition(&A, 0),
        LinkDisposition::Pending,
        "in flight is not a verdict"
    );
    t.finish(&mut [], &A, ResolveOutcome::HandleMissing, 100);
    assert_eq!(t.disposition(&A, 4099), LinkDisposition::CoolingDown);
    assert_eq!(t.disposition(&A, 4100), LinkDisposition::Missing);
    let mut p = ResolveTable::new();
    fail(&mut p, &H, ResolveOutcome::PermissionNeeded, 100);
    assert_eq!(p.disposition(&H, 4100), LinkDisposition::PermissionNeeded);
    t.begin(&A, 4100);
    t.finish(&mut [], &A, ResolveOutcome::Resolved, 4100);
    assert_eq!(t.disposition(&A, 4100), LinkDisposition::Resolved);
    assert_eq!(t.failure(&A), None, "success clears the failure");
}

#[test]
fn a_cached_resolve_outranks_a_later_failure_like_the_render_path() {
    let mut t = ResolveTable::new();
    t.prime(&A, true);
    assert!(t.cached(&A));
    fail(&mut t, &A, ResolveOutcome::HandleMissing, 10);
    assert_eq!(t.disposition(&A, 10), LinkDisposition::Resolved);
    assert!(
        t.failure(&A).is_some(),
        "the failure stays recorded, the cache is not dropped"
    );
    let mut u = ResolveTable::new();
    u.prime(&A, false);
    assert!(!u.cached(&A), "priming without data is a no-op");
}

#[test]
fn prime_forgets_a_cached_failure() {
    let mut t = ResolveTable::new();
    fail(&mut t, &A, ResolveOutcome::ReadFailed, 10);
    t.prime(&A, true);
    assert_eq!(t.failure(&A), None);
    assert_eq!(t.disposition(&A, 11), LinkDisposition::Resolved);
}

#[test]
fn video_marking_is_idempotent_and_images_reload_on_every_hit() {
    // native-video-bridge.js:791-796 versus linked-media.js:119-123.
    let mut lib = vec![entry(Some("/m/a.png"), None, Some("u1"))];
    assert!(mark_broken(&mut lib, &A, FailureReason::Missing));
    assert!(
        mark_broken(&mut lib, &A, FailureReason::Missing),
        "images reload every time"
    );
    assert!(
        !mark_broken(&mut lib, &H, FailureReason::Missing),
        "no hit, no reload"
    );
    let mut v = vec![entry(None, None, Some("u1"))];
    let (u, m, p) = (
        Some("u1"),
        FailureReason::Missing,
        FailureReason::PermissionNeeded,
    );
    assert!(mark_video_broken(&mut v, u, m));
    assert!(
        !mark_video_broken(&mut v, u, m),
        "same reason again is a no-op"
    );
    assert!(
        mark_video_broken(&mut v, u, p),
        "a different reason is a change"
    );
    assert_eq!((v[0].broken, v[0].broken_reason), (true, Some(p)));
    assert!(!mark_video_broken(&mut v, Some("nope"), m));
    assert!(
        !mark_video_broken(&mut v, Some(""), m),
        "an empty uid finds nothing"
    );
}

#[test]
fn ok_clears_the_flag_and_only_a_change_reloads() {
    let mut lib = vec![entry(Some("/m/a.png"), None, Some("u1"))];
    mark_broken(&mut lib, &A, FailureReason::PermissionNeeded);
    assert!(mark_ok(&mut lib, &A));
    assert_eq!((lib[0].broken, lib[0].broken_reason), (false, None));
    assert!(!mark_ok(&mut lib, &A), "already ok: no reload");
    // Inconsistent stored state: an image-side ok clears the stale reason
    // while a video-side ok leaves it (the early return of _markVideoEntryOk).
    lib[0].broken_reason = Some(FailureReason::Missing);
    assert!(!mark_ok(&mut lib, &A));
    assert_eq!(lib[0].broken_reason, None);
    lib[0].broken_reason = Some(FailureReason::Missing);
    assert!(!mark_video_ok(&mut lib, Some("u1")));
    assert_eq!(lib[0].broken_reason, Some(FailureReason::Missing));
    lib[0].broken = true;
    assert!(mark_video_ok(&mut lib, Some("u1")));
    assert_eq!(video_entry_index(&lib, Some("u1")), Some(0));
}

#[test]
fn embedded_and_linked_entries_stay_distinct_through_every_transition() {
    let mut embedded = entry(Some("/m/a.png"), None, None);
    embedded.linked = false;
    let mut lib = vec![embedded.clone()];
    assert!(!mark_broken(&mut lib, &A, FailureReason::Missing));
    assert!(!mark_ok(&mut lib, &A));
    assert_eq!(lib, vec![embedded]);
}

#[test]
fn failure_marks_every_matching_entry_and_success_marks_them_ok() {
    let mut t = ResolveTable::new();
    let mut lib = vec![
        entry(Some("/m/a.png"), None, None),
        entry(Some("/m/a.png"), None, None),
        entry(Some("/m/b.png"), None, None),
    ];
    t.begin(&A, 0);
    let s = t.finish(&mut lib, &A, ResolveOutcome::HandleMissing, 0);
    assert_eq!(
        s,
        Settled {
            rerender: false,
            reload_library: true
        }
    );
    assert_eq!(
        lib.iter().map(|m| m.broken).collect::<Vec<_>>(),
        [true, true, false]
    );
    t.begin(&A, 4000);
    let s = t.finish(&mut lib, &A, ResolveOutcome::Resolved, 4000);
    assert_eq!(
        s,
        Settled {
            rerender: true,
            reload_library: true
        }
    );
    assert!(lib.iter().all(|m| !m.broken && m.broken_reason.is_none()));
    t.begin(&A, 4001);
    let s = t.finish(&mut lib, &A, ResolveOutcome::Resolved, 4001);
    assert_eq!(
        s,
        Settled {
            rerender: true,
            reload_library: false
        },
        "already ok"
    );
}

#[test]
fn permission_round_asks_in_insertion_order_and_a_grant_forgets_the_cooldown() {
    // linked-media.js:173-190 requestPermissionForAll.
    let (b, e) = (
        H,
        LinkedRef {
            linked_path: None,
            linked_handle_id: Some("h-e"),
        },
    );
    let mut t = ResolveTable::new();
    fail(&mut t, &e, ResolveOutcome::PermissionNeeded, 1000);
    fail(&mut t, &b, ResolveOutcome::PermissionNeeded, 1000);
    assert_eq!(t.pending_permission(), ["h-e", "h-b"]);
    let asked = std::cell::RefCell::new(Vec::new());
    let granted = t.permission_round(&|id| {
        asked.borrow_mut().push(id.to_owned());
        if id == "h-b" {
            PermissionAnswer::Granted
        } else {
            PermissionAnswer::NotGranted
        }
    });
    assert_eq!(asked.into_inner(), ["h-e", "h-b"]);
    assert_eq!(granted, ["h-b"]);
    assert_eq!(t.pending_permission(), ["h-e"]);
    assert_eq!(
        t.begin(&b, 1001),
        ResolveStart::Started,
        "a grant is not delayed by the cooldown"
    );
    assert_eq!(
        t.begin(&e, 1001),
        ResolveStart::CoolingDown,
        "a denial keeps it"
    );
}

#[test]
fn pending_permission_follows_the_resolver_observations() {
    let mut t = ResolveTable::new();
    t.permission_observed("h-b", false);
    t.permission_observed("h-b", false);
    assert_eq!(t.pending_permission(), ["h-b"], "one entry per handle");
    t.begin(&H, 0);
    t.finish(&mut [], &H, ResolveOutcome::HandleMissing, 0);
    assert_eq!(
        t.pending_permission(),
        ["h-b"],
        "a vanished handle leaves the prompt pending"
    );
    t.begin(&H, 4000);
    t.finish(&mut [], &H, ResolveOutcome::ReadFailed, 4000);
    assert!(
        t.pending_permission().is_empty(),
        "a read after a grant means permission held"
    );
    t.permission_observed("never-pending", true);
    assert!(t.pending_permission().is_empty());
}

#[test]
fn kernel_source_reads_no_clock_or_io() {
    let src = include_str!("../src/object_media_link_state.rs");
    let code: String = src
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect();
    for banned in [
        "Instant",
        "SystemTime",
        "std::time",
        "std::fs",
        "std::io",
        "std::thread",
        "std::env",
    ] {
        assert!(!code.contains(banned), "kernel must not use {banned}");
    }
}
