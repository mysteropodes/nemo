//! Linked-media reference and disposition state (G32), pure and I/O free.
//!
//! Mirrors the state half of `src/js/linked-media.js` at fbe4b8a3 (`keyFor`,
//! `primeCache`/`getCachedSrc`, `_entriesForKey`, `_markEntryBroken`,
//! `_markEntryOk`, `resolveAsync` and its cooldown, the `_pendingPermission`
//! set and `requestPermissionForAll`) and the video-entry marking of
//! `src/js/native-video-bridge.js` (`_videoEntryFor`, `_markVideoEntryBroken`
//! and `_markVideoEntryOk`). The web-reopen latch of the same file lives in the
//! sibling `object_media_link_reopen`.
//!
//! Nothing here reads a file, a handle, a clock or a document. Every effect
//! the baseline performs is an injected fact: the clock is a plain `now_ms`
//! argument, the resolver's verdict is a [`ResolveOutcome`], the library is a
//! slice of [`LibraryEntry`]. The result of a transition names the follow-up
//! the adapter owes (re-render, reload the library, toast) and does it never.
//!
//! Semantics kept on purpose:
//! - the reference key is `'p:' + linkedPath` when a non-empty path exists,
//!   else `'w:' + linkedHandleId`, else none. `||`-style falsiness: an empty
//!   string is absent. A locator is carried as given, never normalized;
//! - entries match by `linked && (path equal || handle equal)`, which is NOT
//!   the key precedence: a reference holding both fields keys on the path yet
//!   also marks the entry that only matches its handle;
//! - a failed resolve is cached for `RETRY_COOLDOWN_MS` (4000) and retried
//!   only after `now - at >= 4000` (a clock that went backwards stays cooling);
//! - a failure carrying the permission flag is `PermissionNeeded`, any other
//!   failure `Missing`; the verdict never expires, only the retry gate does;
//! - image-side `mark_broken` requests a library reload on EVERY call that hits
//!   an entry; the video-side variant is a no-op when the same reason is set.
//!
//! Deliberately not provided: reading bytes or handles, the banner, relink,
//! bulk conversion, and the namespace of a native locator (see the join).

use std::collections::{HashMap, HashSet};

/// `RETRY_COOLDOWN_MS` of `linked-media.js`.
pub(crate) const RETRY_COOLDOWN_MS: i64 = 4000;

/// Why a link did not resolve: the baseline's `'missing' | 'permission'`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FailureReason {
    Missing,
    PermissionNeeded,
}

impl FailureReason {
    /// The persisted `linkedBrokenReason` spelling.
    pub(crate) fn tag(self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::PermissionNeeded => "permission",
        }
    }
}

/// The two persisted reference fields of a `d`-shaped dict.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct LinkedRef<'a> {
    pub(crate) linked_path: Option<&'a str>,
    pub(crate) linked_handle_id: Option<&'a str>,
}

/// `keyFor`: a desktop path wins over a web handle.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum LinkKey {
    Path(String),
    Handle(String),
}

impl LinkKey {
    /// The baseline key string: `'p:' + path` or `'w:' + id`.
    pub(crate) fn tag(&self) -> String {
        match self {
            Self::Path(path) => format!("p:{path}"),
            Self::Handle(id) => format!("w:{id}"),
        }
    }

    /// The locator exactly as persisted.
    pub(crate) fn locator(&self) -> &str {
        match self {
            Self::Path(value) | Self::Handle(value) => value,
        }
    }
}

fn truthy(value: Option<&str>) -> Option<&str> {
    value.filter(|text| !text.is_empty())
}

pub(crate) fn key_for(r: &LinkedRef<'_>) -> Option<LinkKey> {
    if let Some(path) = truthy(r.linked_path) {
        return Some(LinkKey::Path(path.to_owned()));
    }
    truthy(r.linked_handle_id).map(|id| LinkKey::Handle(id.to_owned()))
}

/// One media-library entry, reduced to the fields the transitions read.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct LibraryEntry {
    pub(crate) linked: bool,
    pub(crate) path: Option<String>,
    pub(crate) web_handle_id: Option<String>,
    pub(crate) layer_uid: Option<String>,
    pub(crate) broken: bool,
    pub(crate) broken_reason: Option<FailureReason>,
}

/// `_entriesForKey`: indices of the linked entries the reference reaches.
pub(crate) fn entries_for_key(entries: &[LibraryEntry], r: &LinkedRef<'_>) -> Vec<usize> {
    let (path, handle) = (truthy(r.linked_path), truthy(r.linked_handle_id));
    let hit = |m: &LibraryEntry| {
        m.linked
            && ((path.is_some() && m.path.as_deref() == path)
                || (handle.is_some() && m.web_handle_id.as_deref() == handle))
    };
    (0..entries.len()).filter(|&i| hit(&entries[i])).collect()
}

/// `_markEntryBroken`; true when the library must be reloaded (any hit).
pub(crate) fn mark_broken(
    entries: &mut [LibraryEntry],
    r: &LinkedRef<'_>,
    reason: FailureReason,
) -> bool {
    let hit = entries_for_key(entries, r);
    for &i in &hit {
        entries[i].broken = true;
        entries[i].broken_reason = Some(reason);
    }
    !hit.is_empty()
}

/// `_markEntryOk`; true when an entry was broken (reload only then). The
/// reason is cleared on every hit, as the baseline does.
pub(crate) fn mark_ok(entries: &mut [LibraryEntry], r: &LinkedRef<'_>) -> bool {
    let mut changed = false;
    for i in entries_for_key(entries, r) {
        changed |= entries[i].broken;
        entries[i].broken = false;
        entries[i].broken_reason = None;
    }
    changed
}

/// `_videoEntryFor`: the first entry carrying the layer uid.
pub(crate) fn video_entry_index(
    entries: &[LibraryEntry],
    layer_uid: Option<&str>,
) -> Option<usize> {
    let uid = truthy(layer_uid)?;
    entries
        .iter()
        .position(|m| m.layer_uid.as_deref() == Some(uid))
}

/// `_markVideoEntryBroken`: re-marking the same reason is a no-op (the
/// reload-storm guard). True when the library must be reloaded.
pub(crate) fn mark_video_broken(
    entries: &mut [LibraryEntry],
    layer_uid: Option<&str>,
    reason: FailureReason,
) -> bool {
    let Some(i) = video_entry_index(entries, layer_uid) else {
        return false;
    };
    let m = &mut entries[i];
    if m.broken && m.broken_reason == Some(reason) {
        return false;
    }
    m.broken = true;
    m.broken_reason = Some(reason);
    true
}

/// `_markVideoEntryOk`: untouched (reason included) unless broken.
pub(crate) fn mark_video_ok(entries: &mut [LibraryEntry], layer_uid: Option<&str>) -> bool {
    let Some(i) = video_entry_index(entries, layer_uid) else {
        return false;
    };
    let m = &mut entries[i];
    if !m.broken {
        return false;
    }
    m.broken = false;
    m.broken_reason = None;
    true
}

/// What a caller learns about one link; never a blank success.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LinkDisposition {
    Unlinked,
    Resolved,
    /// No verdict yet: never attempted or in flight (the baseline paints a
    /// placeholder meanwhile). Not a success.
    Pending,
    /// A failure is cached and the retry gate is still closed.
    CoolingDown,
    Missing,
    PermissionNeeded,
}

/// `resolveAsync`'s entry gate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ResolveStart {
    Unlinked,
    AlreadyInFlight,
    CoolingDown,
    Started,
}

/// What the resolver reported, in the baseline's own cases.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ResolveOutcome {
    Resolved,
    /// `getHandle` found nothing, or the stored handle is gone.
    HandleMissing,
    /// `queryPermission` was not `granted` (the `needsPermission` error).
    PermissionNeeded,
    /// Permission was granted (web) or not involved (path) and reading failed.
    ReadFailed,
}

/// Follow-ups a settled resolve owes its adapter.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Settled {
    pub(crate) rerender: bool,
    pub(crate) reload_library: bool,
}

/// `requestPermission` answer; a throw is `NotGranted`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PermissionAnswer {
    Granted,
    NotGranted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Failure {
    pub(crate) reason: FailureReason,
    pub(crate) at_ms: i64,
}

/// `_resolved`, `_inFlight`, `_failed` and `_pendingPermission`.
#[derive(Debug, Default)]
pub(crate) struct ResolveTable {
    resolved: HashSet<LinkKey>,
    in_flight: HashSet<LinkKey>,
    failed: HashMap<LinkKey, Failure>,
    pending_permission: Vec<String>,
}

impl ResolveTable {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// `getCachedSrc`: true when pixels for this reference are cached.
    pub(crate) fn cached(&self, r: &LinkedRef<'_>) -> bool {
        key_for(r).is_some_and(|key| self.resolved.contains(&key))
    }

    /// `primeCache`: only with data; a primed key forgets its failure.
    pub(crate) fn prime(&mut self, r: &LinkedRef<'_>, has_data: bool) {
        if let (Some(key), true) = (key_for(r), has_data) {
            self.failed.remove(&key);
            self.resolved.insert(key);
        }
    }

    pub(crate) fn in_flight(&self, r: &LinkedRef<'_>) -> bool {
        key_for(r).is_some_and(|key| self.in_flight.contains(&key))
    }

    pub(crate) fn failure(&self, r: &LinkedRef<'_>) -> Option<Failure> {
        self.failed.get(&key_for(r)?).copied()
    }

    /// Handle ids awaiting a user gesture, in insertion order.
    pub(crate) fn pending_permission(&self) -> &[String] {
        &self.pending_permission
    }

    fn cooling(&self, key: &LinkKey, now_ms: i64) -> bool {
        self.failed
            .get(key)
            .is_some_and(|f| now_ms.saturating_sub(f.at_ms) < RETRY_COOLDOWN_MS)
    }

    /// The first half of `resolveAsync`: no key, in flight, cooling, or start.
    pub(crate) fn begin(&mut self, r: &LinkedRef<'_>, now_ms: i64) -> ResolveStart {
        let Some(key) = key_for(r) else {
            return ResolveStart::Unlinked;
        };
        if self.in_flight.contains(&key) {
            return ResolveStart::AlreadyInFlight;
        }
        if self.cooling(&key, now_ms) {
            return ResolveStart::CoolingDown;
        }
        self.in_flight.insert(key);
        ResolveStart::Started
    }

    /// `_pendingPermission[id] = handle` or `delete _pendingPermission[id]`.
    pub(crate) fn permission_observed(&mut self, handle_id: &str, granted: bool) {
        let at = self
            .pending_permission
            .iter()
            .position(|id| id == handle_id);
        match (granted, at) {
            (true, Some(i)) => {
                self.pending_permission.remove(i);
            }
            (false, None) => self.pending_permission.push(handle_id.to_owned()),
            _ => {}
        }
    }

    /// The `.then`/`.catch` half of `resolveAsync`. `now_ms` is the clock when
    /// the failure landed (`Date.now()` inside the catch).
    pub(crate) fn finish(
        &mut self,
        entries: &mut [LibraryEntry],
        r: &LinkedRef<'_>,
        outcome: ResolveOutcome,
        now_ms: i64,
    ) -> Settled {
        let Some(key) = key_for(r) else {
            return Settled::default();
        };
        self.in_flight.remove(&key);
        if let LinkKey::Handle(id) = &key {
            match outcome {
                ResolveOutcome::Resolved | ResolveOutcome::ReadFailed => {
                    self.permission_observed(id, true)
                }
                ResolveOutcome::PermissionNeeded => self.permission_observed(id, false),
                ResolveOutcome::HandleMissing => {}
            }
        }
        let reason = match outcome {
            ResolveOutcome::Resolved => {
                self.failed.remove(&key);
                self.resolved.insert(key);
                return Settled {
                    rerender: true,
                    reload_library: mark_ok(entries, r),
                };
            }
            ResolveOutcome::PermissionNeeded => FailureReason::PermissionNeeded,
            ResolveOutcome::HandleMissing | ResolveOutcome::ReadFailed => FailureReason::Missing,
        };
        self.failed.insert(
            key,
            Failure {
                reason,
                at_ms: now_ms,
            },
        );
        Settled {
            rerender: false,
            reload_library: mark_broken(entries, r, reason),
        }
    }

    /// `requestPermissionForAll`: ask each pending handle in order; a granted
    /// one leaves the set and forgets its cooldown. Returns the granted ids.
    /// The caller always owes the video-session retry and a re-render after.
    pub(crate) fn permission_round(
        &mut self,
        answer: &dyn Fn(&str) -> PermissionAnswer,
    ) -> Vec<String> {
        let asked: Vec<String> = self.pending_permission.clone();
        let mut granted = Vec::new();
        for id in asked {
            if answer(&id) == PermissionAnswer::Granted {
                self.permission_observed(&id, true);
                self.failed.remove(&LinkKey::Handle(id.clone()));
                granted.push(id);
            }
        }
        granted
    }

    /// One typed verdict for a reference at `now_ms`. A cached resolve wins,
    /// as it does in `desR`; a cached failure reads `CoolingDown` until its
    /// retry gate opens, then its reason.
    pub(crate) fn disposition(&self, r: &LinkedRef<'_>, now_ms: i64) -> LinkDisposition {
        let Some(key) = key_for(r) else {
            return LinkDisposition::Unlinked;
        };
        if self.resolved.contains(&key) {
            return LinkDisposition::Resolved;
        }
        match self.failed.get(&key) {
            None => LinkDisposition::Pending,
            Some(_) if self.cooling(&key, now_ms) => LinkDisposition::CoolingDown,
            Some(f) if f.reason == FailureReason::PermissionNeeded => {
                LinkDisposition::PermissionNeeded
            }
            Some(_) => LinkDisposition::Missing,
        }
    }
}

#[cfg(test)]
#[path = "../tests/object_media_link_state_controls.rs"]
mod controls;
#[cfg(test)]
#[path = "../tests/object_media_link_state_pins.rs"]
mod pins;
#[cfg(test)]
#[path = "../tests/object_media_link_state.rs"]
mod tests;
#[cfg(test)]
#[path = "../tests/object_media_link_state_world.rs"]
mod world;
