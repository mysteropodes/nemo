//! Cross-context frame clipboard (F13): the typed consumer port a paste, cut or duplicate goes through when the clipboard
//! was copied in one context and is written into another. The dense editor pastes by RELATIVE layer index into whatever
//! context is open (`state.layers` is swapped by `enterSymbol`), so a clipboard can cross from the scene into a component
//! and back. Natively only the scene-root context has a cell model: component layers, their `blankOverride` /
//! `componentFrame` records and their contexts belong to the component provider (I14/I15, lane 4) and are absent, and
//! L05-I writes scene-root cells only.
//!
//! So the port admits exactly one pair, scene-root to scene-root, and answers every other pair with a stable typed code
//! BEFORE anything is prepared, read or written: a context the document declares but that has no native cell model
//! yet is `Unavailable`, a context the document does not know is `NotFound`, an empty id is `InvalidRequest`. No old
//! JavaScript or Paper.js writer is reachable from here, and no default is invented for a missing provider. When the
//! component provider lands, `admit_clipboard_contexts` is the one function that learns the new pair.
use crate::commands::object_fill_owner::object_frame_clipboard_paste::FrameClipboardOperation;
use crate::commands::object_fill_owner::object_frame_clipboard_paste_plan::PreparedFrameClipboard;
use crate::commands::ObjectFillOwner;
use crate::object_document::{ObjectDocument, Presence};
use crate::request_receipts::DispatchErrorCode;

const ROOT: &str = "scene-root";

/// Where a clipboard was copied from and where it is being written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ClipboardContexts {
    pub(crate) source: String,
    pub(crate) target: String,
}
impl ClipboardContexts {
    /// The only pair native documents can hold today.
    pub(crate) fn scene_root() -> Self {
        Self {
            source: ROOT.to_owned(),
            target: ROOT.to_owned(),
        }
    }
}

/// True when the document declares the context: a native document holds a context other than scene-root only through its
/// composition-order plan, which every record and property holder of that context must resolve against.
fn declared(document: &ObjectDocument, context: &str) -> bool {
    match &document.composition_order {
        Presence::Present(plan) => plan.contexts.iter().any(|c| c.context_id == context),
        Presence::Missing => false,
    }
}

/// Admission of a clipboard context pair against the document: scene-root to scene-root is admitted; an empty id is
/// `InvalidRequest`; an unknown context is `NotFound`; a declared non-root context is `Unavailable` (component provider
/// I14/I15). The checks run in that order, so an identity failure wins over an unavailable provider.
pub(crate) fn admit_clipboard_contexts(
    document: &ObjectDocument,
    contexts: &ClipboardContexts,
) -> Result<(), DispatchErrorCode> {
    let pair = [contexts.source.as_str(), contexts.target.as_str()];
    if pair.iter().any(|id| id.is_empty()) {
        return Err(DispatchErrorCode::InvalidRequest);
    }
    let others: Vec<&str> = pair.iter().copied().filter(|id| *id != ROOT).collect();
    if others.iter().any(|id| !declared(document, id)) {
        return Err(DispatchErrorCode::NotFound);
    }
    if !others.is_empty() {
        return Err(DispatchErrorCode::Unavailable);
    }
    Ok(())
}

impl ObjectFillOwner {
    /// Prepare a clipboard operation for a context pair. The pair is admitted against the current snapshot first; only
    /// an admitted pair reaches the L05-I planners, which then run exactly as for a plain clipboard operation.
    pub(crate) fn prepare_contextual_frame_clipboard(
        &self,
        contexts: &ClipboardContexts,
        operation: &FrameClipboardOperation,
    ) -> Result<PreparedFrameClipboard, DispatchErrorCode> {
        admit_clipboard_contexts(self.current_snapshot().document(), contexts)?;
        self.prepare_frame_clipboard(operation)
    }
}

#[cfg(test)]
#[path = "../tests/object_frame_clipboard_tracks_context.rs"]
mod tests;
