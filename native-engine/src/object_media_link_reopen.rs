//! Web video reopen latch (G32): the `webRetryAt` / `webReopenWarned` pair of
//! one video layer's sync state in `src/js/native-video-bridge.js` at fbe4b8a3
//! (`_layerFrameSyncAsync` lines 826-880 and `retryWebLinkedSessions`).
//!
//! The clock is an injected `now_ms`. Library marking goes through the video
//! variants of the link-state kernel, so a repeated failure of the same kind
//! does not ask for another library reload. A retry stamp of `0` means "never
//! tried", exactly as the baseline's truthiness test reads it.

use crate::object_document::media_timing::object_media_link_state::{
    mark_video_broken, mark_video_ok, FailureReason, LibraryEntry,
};

/// `WEB_REOPEN_COOLDOWN_MS` of `native-video-bridge.js`.
pub(crate) const WEB_REOPEN_COOLDOWN_MS: i64 = 4000;

/// `st.webReopenWarned` and `st.webRetryAt` of one video layer's sync state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct WebReopenLatch {
    pub(crate) warned: bool,
    /// `0` is "never tried", as the baseline's truthiness test reads it.
    pub(crate) retry_at_ms: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReopenGate {
    /// No usable handle: tell the user once (`warn`), then stay quiet.
    NoHandle {
        warn: bool,
    },
    CoolingDown,
    Attempt,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ReopenFailure {
    pub(crate) reason: FailureReason,
    pub(crate) warn: bool,
    pub(crate) reload_library: bool,
}

impl WebReopenLatch {
    /// The checks before `SMLinkedMedia.getHandleFile` in `_layerFrameSyncAsync`.
    pub(crate) fn gate(&mut self, has_handle: bool, now_ms: i64) -> ReopenGate {
        if !has_handle {
            let warn = !self.warned;
            self.warned = true;
            return ReopenGate::NoHandle { warn };
        }
        if self.retry_at_ms != 0 && now_ms.saturating_sub(self.retry_at_ms) < WEB_REOPEN_COOLDOWN_MS
        {
            return ReopenGate::CoolingDown;
        }
        ReopenGate::Attempt
    }

    /// The `catch` of that call: stamp the retry clock, classify, mark.
    pub(crate) fn attempt_failed(
        &mut self,
        entries: &mut [LibraryEntry],
        layer_uid: Option<&str>,
        needs_permission: bool,
        now_ms: i64,
    ) -> ReopenFailure {
        self.retry_at_ms = now_ms;
        if needs_permission {
            let reason = FailureReason::PermissionNeeded;
            let reload_library = mark_video_broken(entries, layer_uid, reason);
            return ReopenFailure {
                reason,
                warn: false,
                reload_library,
            };
        }
        let reason = FailureReason::Missing;
        let reload_library = mark_video_broken(entries, layer_uid, reason);
        let warn = !self.warned;
        self.warned = true;
        ReopenFailure {
            reason,
            warn,
            reload_library,
        }
    }

    /// After the session opened: clear the latch, mark the entry ok.
    pub(crate) fn attempt_succeeded(
        &mut self,
        entries: &mut [LibraryEntry],
        layer_uid: Option<&str>,
    ) -> bool {
        self.warned = false;
        self.retry_at_ms = 0;
        mark_video_ok(entries, layer_uid)
    }

    /// `retryWebLinkedSessions`, once per layer.
    pub(crate) fn retry_all(&mut self) {
        self.warned = false;
        self.retry_at_ms = 0;
    }
}

#[cfg(test)]
#[path = "../tests/object_media_link_reopen.rs"]
mod tests;
