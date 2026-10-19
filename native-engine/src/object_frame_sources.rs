//! Immutable source-frame selection for the scene packet. The existing sparse exposure
//! rules and shared D27 range reader run before any content/order is published.
use crate::object_document::{FrameScopeKind, ObjectLayer, RangeError, RangeEvaluator};
use crate::object_frame_packet::{ObjectFrameError, ObjectFrameSelector};
use crate::revision::ObjectSnapshot;

impl<'a> RangeEvaluator<'a> {
    /// Snapshot route for readers. The document core also serves the sealed edit owner,
    /// without the document layer depending on revision/evaluation or owning mutable state.
    pub(crate) fn new(
        snapshot: &'a ObjectSnapshot,
        context: &'a str,
        frame: u32,
    ) -> Result<Self, RangeError> {
        Self::for_document(snapshot.document(), context, frame)
    }
}

pub(super) fn select(
    snapshot: &ObjectSnapshot,
    selector: &ObjectFrameSelector<'_>,
    layers: &[ObjectLayer],
    exposure: bool,
    component: bool,
) -> Result<Vec<Option<u32>>, ObjectFrameError> {
    let ranges = RangeEvaluator::new(snapshot, selector.context_id, selector.frame)
        .map_err(|_| ObjectFrameError::Unavailable)?;
    layers
        .iter()
        .map(|layer| {
            if component && !layer.visible() {
                return Ok(None);
            }
            if component
                && layer.path_fx().is_some_and(|stack| {
                    stack
                        .iter()
                        .any(|effect| effect.enabled_flag() != Some(false))
                })
            {
                return Err(ObjectFrameError::Unavailable);
            }
            // Keep the timing provider requirement even outside the visible window.
            let source = if exposure {
                layer
                    .timing
                    .value()
                    .map(|timing| timing.exposure(selector.frame).1)
                    .ok_or(ObjectFrameError::Unavailable)?
            } else {
                Some(selector.frame)
            };
            let gated = selector.scope_kind == FrameScopeKind::Authored
                && ranges
                    .gates(layer.layer_uid())
                    .map_err(|_| ObjectFrameError::Unavailable)?;
            if gated
                && !ranges
                    .resolve(layer.layer_uid(), true)
                    .map_err(|_| ObjectFrameError::Unavailable)?
                    .contains(selector.frame)
            {
                Ok(None)
            } else {
                Ok(source)
            }
        })
        .collect()
}
