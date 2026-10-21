//! Baseline Drawing Fill lock guard; no universal content policy or writer.
use crate::object_document::{ObjectDocument, ObjectLayer};
impl ObjectLayer {
    /// Storage absence remains absent and reads with the baseline false default.
    pub(crate) fn locked(&self) -> bool {
        self.locked.value().copied().unwrap_or(false)
    }
}
impl ObjectDocument {
    /// Duplicator tables describe derived output, never an editable Drawing surface.
    /// Root Drawing operations address scene-root layers; component layers retain
    /// their own context and cannot lock a same-named root layer by coincidence.
    pub(crate) fn layer_is_duplicator(&self, uid: &str) -> bool {
        self.duplicator_set().is_some_and(|set| {
            set.duplicators
                .iter()
                .any(|record| record.context_id == "scene-root" && record.layer_uid == uid)
        })
    }

    /// Effective Drawing policy is derived from the sole authored document.
    /// It does not normalize persisted flags or block generic Motion edits.
    pub(crate) fn layer_is_locked(&self, uid: &str) -> bool {
        self.layers
            .iter()
            .find(|layer| layer.layer_uid() == uid)
            .is_some_and(|layer| layer.locked() || self.layer_is_duplicator(uid))
    }

    /// Fill calls this only after resolving its actual authored object target.
    /// Generic Motion property/track preparation deliberately does not call it.
    pub(crate) fn layer_allows_fill_edit(&self, uid: &str) -> bool {
        self.layers.iter().any(|layer| layer.layer_uid() == uid)
            && !self.layer_is_locked(uid)
    }
}
#[cfg(all(test, feature = "history"))]
#[path = "../tests/object_layer_lock_policy.rs"]
mod tests;
