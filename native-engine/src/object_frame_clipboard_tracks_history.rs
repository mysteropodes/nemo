//! History entry point of the cross-context frame clipboard port: the same prepare step the plain clipboard has
//! (`NativeObjectHistory::prepare_frame_clipboard`, L05-I), preceded by the context admission of
//! `object_frame_clipboard_tracks_context.rs`. The commit is L05-I's unchanged `commit_frame_clipboard`, so one changed
//! operation is one history entry and one Undo, a no-op keeps redo, and the request fingerprint / replay store that the
//! dispatcher already runs applies as for any object command. Nothing here publishes.
use crate::commands::{ClipboardContexts, FrameClipboardOperation, PreparedFrameClipboard};
use crate::history::object_history::NativeObjectHistory;
use crate::request_receipts::DispatchErrorCode;

impl NativeObjectHistory {
    /// Prepare a paste, cut or duplicate between `contexts`. Any pair other than scene-root to scene-root is a typed
    /// refusal (`InvalidRequest`, `NotFound` or `Unavailable`) raised before the document is read or a candidate exists.
    pub(crate) fn prepare_contextual_frame_clipboard(
        &self,
        contexts: &ClipboardContexts,
        operation: &FrameClipboardOperation,
    ) -> Result<PreparedFrameClipboard, DispatchErrorCode> {
        self.owner
            .prepare_contextual_frame_clipboard(contexts, operation)
    }
}

#[cfg(test)]
#[path = "../tests/object_frame_clipboard_tracks_history.rs"]
mod tests;
