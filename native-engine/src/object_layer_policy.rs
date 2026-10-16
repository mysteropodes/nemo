//! Baseline Drawing Fill lock guard; no universal content policy or writer.
use crate::object_document::{ObjectDocument, ObjectLayer};
impl ObjectLayer {
    /// Storage absence remains absent and reads with the baseline false default.
    pub(crate) fn locked(&self) -> bool {
        self.locked.value().copied().unwrap_or(false)
    }
}
impl ObjectDocument {
    /// Fill calls this only after resolving its actual authored object target.
    /// Generic Motion property/track preparation deliberately does not call it.
    pub(crate) fn layer_allows_fill_edit(&self, uid: &str) -> bool {
        self.layers
            .iter()
            .find(|layer| layer.layer_uid() == uid)
            .is_some_and(|layer| !layer.locked())
    }
}
#[cfg(all(test, feature = "history"))]
#[path = "../tests/object_layer_lock_policy.rs"]
mod tests;
