//! Internal source identity only. The public persisted Motion target stays unchanged.
use crate::object_document::{Presence, PropertyHolderTarget};
#[derive(Clone, PartialEq)]
pub(crate) enum ExpressionHolder {
    Property(PropertyHolderTarget),
    TextAnimator {
        context_id: String,
        layer_uid: String,
        animator_id: String,
    },
}
impl std::fmt::Debug for ExpressionHolder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Property(target) => std::fmt::Debug::fmt(target, f),
            Self::TextAnimator {
                context_id,
                layer_uid,
                animator_id,
            } => f
                .debug_struct("TextAnimator")
                .field("context_id", context_id)
                .field("layer_uid", layer_uid)
                .field("animator_id", animator_id)
                .finish(),
        }
    }
}
impl ExpressionHolder {
    pub(crate) fn context_id(&self) -> &str {
        match self {
            Self::Property(t) => t.context_id(),
            Self::TextAnimator { context_id, .. } => context_id,
        }
    }
    pub(crate) fn layer_uid(&self) -> &str {
        match self {
            Self::Property(t) => t.layer_uid(),
            Self::TextAnimator { layer_uid, .. } => layer_uid,
        }
    }
    pub(crate) fn property_target(&self) -> Option<&PropertyHolderTarget> {
        match self {
            Self::Property(t) => Some(t),
            Self::TextAnimator { .. } => None,
        }
    }
    /// Context metadata only; this does not impersonate the inline holder's self.
    pub(crate) fn context_target(&self) -> PropertyHolderTarget {
        self.property_target()
            .cloned()
            .unwrap_or_else(|| PropertyHolderTarget {
                context_id: self.context_id().to_owned(),
                layer_uid: self.layer_uid().to_owned(),
                stroke_id: Presence::Missing,
            })
    }
}
impl From<PropertyHolderTarget> for ExpressionHolder {
    fn from(target: PropertyHolderTarget) -> Self {
        Self::Property(target)
    }
}
// Existing ordinary Rust result comparisons retain their exact target meaning.
impl PartialEq<PropertyHolderTarget> for ExpressionHolder {
    fn eq(&self, target: &PropertyHolderTarget) -> bool {
        self.property_target() == Some(target)
    }
}
