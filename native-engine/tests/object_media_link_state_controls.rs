//! Controls for the link-state kernel: each mutation below is one plausible
//! mistake spliced around the REAL kernel (an input changed before the call,
//! or an effect undone after it) and the oracle comparison must name rows that
//! catch it. `Mutation::None` is the unmutated pass-through, so a green control
//! proves the comparison can fail, not that it was skipped.

use super::super::object_media_link_reopen::{ReopenFailure, ReopenGate, WebReopenLatch};
use super::tests::run;
use super::{
    entries_for_key, key_for, mark_broken, mark_ok, mark_video_broken, video_entry_index,
    FailureReason, LibraryEntry, LinkKey, LinkedRef, PermissionAnswer, ResolveOutcome,
    ResolveStart, ResolveTable, Settled,
};

/// A deliberate mistake the comparison must catch.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Mutation {
    None,
    /// A web handle outranks a desktop path in the reference key.
    HandleOutranksPath,
    /// Entries are matched by the key's winner only, not path OR handle.
    EntryMatchByKeyWinner,
    /// The retry gate opens one millisecond early (`<=` for `<`).
    CooldownInclusive,
    /// A permission failure is recorded as a missing file.
    PermissionAsMissing,
    /// The video reload-storm guard is gone: re-marking reloads again.
    VideoGuardRemoved,
    /// A successful resolve leaves the old failure cached.
    SuccessKeepsFailure,
    /// A granted permission leaves the handle's cooldown in place.
    GrantKeepsCooldown,
}

impl Mutation {
    pub(crate) fn key_input<'a>(self, r: LinkedRef<'a>) -> LinkedRef<'a> {
        match self {
            Mutation::HandleOutranksPath if r.linked_handle_id.is_some_and(|h| !h.is_empty()) => {
                LinkedRef {
                    linked_path: None,
                    ..r
                }
            }
            _ => r,
        }
    }

    pub(crate) fn key(self, r: &LinkedRef<'_>) -> Option<LinkKey> {
        key_for(&self.key_input(*r))
    }

    fn matching<'a>(self, r: &LinkedRef<'a>) -> LinkedRef<'a> {
        match self {
            Mutation::EntryMatchByKeyWinner if r.linked_path.is_some_and(|p| !p.is_empty()) => {
                LinkedRef {
                    linked_handle_id: None,
                    ..*r
                }
            }
            _ => *r,
        }
    }

    pub(crate) fn entries_for_key(self, lib: &[LibraryEntry], r: &LinkedRef<'_>) -> Vec<usize> {
        entries_for_key(lib, &self.matching(r))
    }

    pub(crate) fn mark_broken(
        self,
        lib: &mut [LibraryEntry],
        r: &LinkedRef<'_>,
        reason: FailureReason,
    ) -> bool {
        mark_broken(lib, &self.matching(r), reason)
    }

    pub(crate) fn mark_ok(self, lib: &mut [LibraryEntry], r: &LinkedRef<'_>) -> bool {
        mark_ok(lib, &self.matching(r))
    }

    pub(crate) fn mark_video_broken(
        self,
        lib: &mut [LibraryEntry],
        uid: Option<&str>,
        reason: FailureReason,
    ) -> bool {
        let hit = video_entry_index(lib, uid).is_some();
        let got = mark_video_broken(lib, uid, reason);
        got || (self == Mutation::VideoGuardRemoved && hit)
    }

    pub(crate) fn begin(
        self,
        table: &mut ResolveTable,
        r: &LinkedRef<'_>,
        now_ms: i64,
    ) -> ResolveStart {
        let now = if self == Mutation::CooldownInclusive {
            now_ms - 1
        } else {
            now_ms
        };
        table.begin(&self.key_input(*r), now)
    }

    pub(crate) fn outcome(self, o: ResolveOutcome) -> ResolveOutcome {
        match (self, o) {
            (Mutation::PermissionAsMissing, ResolveOutcome::PermissionNeeded) => {
                ResolveOutcome::HandleMissing
            }
            _ => o,
        }
    }

    pub(crate) fn finish(
        self,
        table: &mut ResolveTable,
        lib: &mut [LibraryEntry],
        r: &LinkedRef<'_>,
        outcome: ResolveOutcome,
        now_ms: i64,
    ) -> Settled {
        let r = self.key_input(*r);
        let prior = key_for(&r).and_then(|k| table.failed.get(&k).copied().map(|f| (k, f)));
        let got = table.finish(lib, &r, self.outcome(outcome), now_ms);
        if let (Mutation::SuccessKeepsFailure, ResolveOutcome::Resolved, Some((k, f))) =
            (self, outcome, prior)
        {
            table.failed.insert(k, f);
        }
        got
    }

    pub(crate) fn prime(self, table: &mut ResolveTable, r: &LinkedRef<'_>, has: bool) {
        table.prime(&self.key_input(*r), has);
    }

    pub(crate) fn permission_round(
        self,
        table: &mut ResolveTable,
        answer: &dyn Fn(&str) -> PermissionAnswer,
    ) -> Vec<String> {
        let before: Vec<_> = table.failed.iter().map(|(k, f)| (k.clone(), *f)).collect();
        let granted = table.permission_round(answer);
        if self == Mutation::GrantKeepsCooldown {
            for (k, f) in before {
                if matches!(&k, LinkKey::Handle(id) if granted.contains(id)) {
                    table.failed.insert(k, f);
                }
            }
        }
        granted
    }

    pub(crate) fn gate(
        self,
        latch: &mut WebReopenLatch,
        has_handle: bool,
        now_ms: i64,
    ) -> ReopenGate {
        let now = if self == Mutation::CooldownInclusive {
            now_ms - 1
        } else {
            now_ms
        };
        latch.gate(has_handle, now)
    }

    pub(crate) fn attempt_failed(
        self,
        latch: &mut WebReopenLatch,
        lib: &mut [LibraryEntry],
        uid: Option<&str>,
        needs_permission: bool,
        now_ms: i64,
    ) -> ReopenFailure {
        let hit = video_entry_index(lib, uid).is_some();
        let needs = needs_permission && self != Mutation::PermissionAsMissing;
        let mut got = latch.attempt_failed(lib, uid, needs, now_ms);
        if self == Mutation::VideoGuardRemoved && hit {
            got.reload_library = true;
        }
        got
    }
}

fn assert_caught(m: Mutation, op: &str) {
    let bad = run(m).bad;
    assert!(!bad.is_empty(), "{m:?} slipped through the oracle");
    assert!(
        bad.iter().any(|b| b.contains(&format!(": {op} "))),
        "{m:?} was not caught by a {op} row:\n{}",
        bad.iter().take(3).cloned().collect::<Vec<_>>().join("\n")
    );
}

#[test]
fn unmutated_pass_through_is_clean() {
    let r = run(Mutation::None);
    assert!(r.bad.is_empty(), "{}", r.bad.join("\n"));
}

#[test]
fn mutation_handle_outranks_path_is_caught() {
    assert_caught(Mutation::HandleOutranksPath, "K");
    assert_caught(Mutation::HandleOutranksPath, "R");
}

#[test]
fn mutation_entry_match_by_key_winner_is_caught() {
    assert_caught(Mutation::EntryMatchByKeyWinner, "E");
}

#[test]
fn mutation_cooldown_inclusive_is_caught() {
    assert_caught(Mutation::CooldownInclusive, "R");
    assert_caught(Mutation::CooldownInclusive, "W");
}

#[test]
fn mutation_permission_as_missing_is_caught() {
    assert_caught(Mutation::PermissionAsMissing, "R");
    assert_caught(Mutation::PermissionAsMissing, "W");
}

#[test]
fn mutation_video_guard_removed_is_caught() {
    assert_caught(Mutation::VideoGuardRemoved, "E");
    assert_caught(Mutation::VideoGuardRemoved, "W");
}

#[test]
fn mutation_success_keeps_failure_is_caught() {
    assert_caught(Mutation::SuccessKeepsFailure, "R");
}

#[test]
fn mutation_grant_keeps_cooldown_is_caught() {
    assert_caught(Mutation::GrantKeepsCooldown, "R");
}
