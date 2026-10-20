//! Raw drawing inputs of a duplicator, retained in the admitted frame context.
//! Hidden pool/path layers remain readable, as getEffectiveStrokes does; paint
//! visibility and path effects belong to the separate rendered-source consumer.
use super::{context::FrameContext, sources, ObjectFrameError, ObjectFrameSelector};
use crate::evaluation::object_component_kernels::object_duplicator_evaluate::{
    evaluate_duplicator, Evaluation, LayerSources, Ports, Request,
};
use crate::evaluation::object_component_kernels::object_effector_layers::MotionPort;
use crate::object_document::{FrameScopeKind, ObjectLayer, ObjectRecord, RangeEvaluator};
use crate::revision::ObjectSnapshot;
use std::cell::Cell;

pub(crate) struct SnapshotSources<'a> {
    snapshot: &'a ObjectSnapshot,
    context_id: String,
    scope_kind: FrameScopeKind,
    context: FrameContext,
    exposure: bool,
    failure: Cell<Option<ObjectFrameError>>,
}

impl<'a> SnapshotSources<'a> {
    pub(crate) fn new(
        snapshot: &'a ObjectSnapshot,
        selector: &ObjectFrameSelector<'_>,
        exposure: bool,
    ) -> Result<Self, ObjectFrameError> {
        let context = FrameContext::admit(snapshot, selector, exposure)?;
        Ok(Self {
            snapshot,
            context_id: selector.context_id.to_owned(),
            scope_kind: selector.scope_kind.clone(),
            context,
            exposure: exposure || context.is_component(),
            failure: Cell::new(None),
        })
    }

    fn layers(&self) -> &[ObjectLayer] {
        self.context.layers(self.snapshot.document())
    }

    /// The old array lookup has no entry at a fractional/out-of-range frame.
    /// In particular, never round a temporal stagger to a different drawing.
    pub(crate) fn read(
        &self,
        uid: &str,
        frame: f64,
    ) -> Result<Vec<ObjectRecord>, ObjectFrameError> {
        if !frame.is_finite() {
            return Err(ObjectFrameError::Unavailable);
        }
        let Some(layer) = self.layers().iter().find(|layer| layer.layer_uid() == uid) else {
            return Ok(Vec::new());
        };
        if frame < 0.0 || frame.fract() != 0.0 || frame > f64::from(u32::MAX) {
            return Ok(Vec::new());
        }
        let selector = ObjectFrameSelector {
            instance_id: self.snapshot.instance_id(),
            document_id: self.snapshot.document_id(),
            content_revision: self.snapshot.content_revision(),
            document_snapshot_id: self.snapshot.snapshot_id(),
            context_id: &self.context_id,
            scope_kind: self.scope_kind.clone(),
            frame: frame as u32,
        };
        match FrameContext::admit(self.snapshot, &selector, self.exposure) {
            Err(ObjectFrameError::FrameOutOfRange) => return Ok(Vec::new()),
            other => {
                other?;
            }
        }
        let document = self.snapshot.document();
        if layer.widget.value().is_some()
            || layer.is_effect_layer.value() == Some(&true)
            || document.duplicator_set().is_some_and(|set| {
                set.effector_layers.iter().any(|effector| {
                    effector.context_id == self.context_id && effector.layer_uid == uid
                })
            })
        {
            // getEffectiveStrokes returns before its drawing lookup for these
            // non-content roles, but AFTER its shared range gate. Preserve
            // unavailable linked offsets, without requiring drawing timing
            // when this zero-content layer has no range gate at all.
            let ranges = RangeEvaluator::new(self.snapshot, &self.context_id, selector.frame)
                .map_err(|_| ObjectFrameError::Unavailable)?;
            if self.scope_kind == FrameScopeKind::Authored
                && ranges
                    .gates(uid)
                    .map_err(|_| ObjectFrameError::Unavailable)?
            {
                ranges
                    .resolve(uid, true)
                    .map_err(|_| ObjectFrameError::Unavailable)?;
            }
            return Ok(Vec::new());
        }
        // These families need their real item/component consumer. An empty
        // vector response would hide their content behind the kernel's blank
        // temporal fallback, so preserve an explicit unavailable disposition.
        if document
            .component_set()
            .is_some_and(|set| set.instance(&self.context_id, uid).is_some())
            || document.item_records().is_some_and(|items| {
                items.targets().any(|target| {
                    target.context_id() == self.context_id
                        && target.layer_uid() == uid
                        && target.scope_kind() == &self.scope_kind
                })
            })
        {
            return Err(ObjectFrameError::Unavailable);
        }
        // Reuse the canonical range/exposure reader for this requested layer.
        // Its final flag is PAINT admission, intentionally false for raw source
        // lookup: hidden Object-mode pool/path content is still source content.
        let source = sources::select(
            self.snapshot,
            &selector,
            std::slice::from_ref(layer),
            self.exposure,
            false,
        )?[0];
        Ok(self
            .context
            .objects(document)
            .iter()
            .filter(|record| {
                let target = record.target();
                target.context_id() == self.context_id
                    && target.layer_uid() == uid
                    && target.scope_kind() == &self.scope_kind
                    && source == Some(target.frame())
            })
            .cloned()
            .collect())
    }

    /// The historical port cannot return Result. Always finish the kernel here
    /// so its legitimate empty-source fallback cannot swallow a provider error.
    pub(crate) fn evaluate(
        &self,
        request: &Request<'_>,
        motion: &dyn MotionPort,
    ) -> Result<Evaluation, ObjectFrameError> {
        if request.context_id != self.context_id {
            return Err(ObjectFrameError::InvalidContext);
        }
        if !self
            .snapshot
            .document()
            .duplicator_set()
            .is_some_and(|set| std::ptr::eq(set, request.set))
            || !std::ptr::eq(self.layers(), request.layers)
        {
            return Err(ObjectFrameError::InvalidSource);
        }
        let result = evaluate_duplicator(
            request,
            &Ports {
                motion,
                sources: self,
            },
        );
        if let Some(error) = self.failure.get() {
            return Err(error);
        }
        result.map_err(|_| ObjectFrameError::Unavailable)
    }
}

impl LayerSources for SnapshotSources<'_> {
    fn has_layer(&self, uid: &str) -> bool {
        self.layers().iter().any(|layer| layer.layer_uid() == uid)
    }

    fn strokes_at(&self, uid: &str, frame: f64) -> Vec<ObjectRecord> {
        match self.read(uid, frame) {
            Ok(records) => records,
            Err(error) => {
                if self.failure.get().is_none() {
                    self.failure.set(Some(error));
                }
                Vec::new()
            }
        }
    }
}

#[cfg(test)]
#[path = "../tests/object_duplicator_sources.rs"]
mod tests;
