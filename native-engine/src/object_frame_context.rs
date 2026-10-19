//! Context-local immutable source provenance. A component stays in the one
//! retained document; these indices never own or copy its authored content.
use crate::evaluation::object_component_kernels::object_context_admission::{
    admit_frame_context, ContextPins,
};
use crate::evaluation::object_component_kernels::object_context_resolver::ContextKind;
use crate::object_document::{ObjectDocument, ObjectLayer, ObjectRecord};
use crate::object_frame_packet::{ObjectFrameError, ObjectFrameSelector};
use crate::revision::ObjectSnapshot;

#[derive(Debug, Clone, Copy)]
pub(super) enum FrameContext {
    Scene,
    Component(usize),
}
impl FrameContext {
    pub(super) fn admit(
        snapshot: &ObjectSnapshot,
        selector: &ObjectFrameSelector<'_>,
        exposure: bool,
    ) -> Result<Self, ObjectFrameError> {
        ContextPins {
            instance_id: selector.instance_id,
            document_id: selector.document_id,
            content_revision: selector.content_revision,
            document_snapshot_id: selector.document_snapshot_id,
        }
        .check(snapshot)?;
        if exposure && selector.scope_kind != crate::object_document::FrameScopeKind::Authored {
            return Err(ObjectFrameError::Unavailable);
        }
        let admitted = admit_frame_context(snapshot, selector).map_err(|error| {
            // Preserve the established public packet refusal. The separate
            // resolver still reports InvalidContext for undeclared contexts.
            if error == ObjectFrameError::InvalidContext
                && (exposure || snapshot.document().format_version() == 2)
            {
                ObjectFrameError::Unavailable
            } else {
                error
            }
        })?;
        if admitted.context.kind() == ContextKind::SceneRoot {
            return Ok(Self::Scene);
        }
        if snapshot.document().item_records().is_some_and(|items| {
            items
                .targets()
                .any(|target| target.context_id() == selector.context_id)
        }) {
            // Inner component records currently own drawing objects, not the
            // scene's separate raster/text item table. Never paint a subset.
            return Err(ObjectFrameError::Unavailable);
        }
        snapshot
            .document()
            .component_set()
            .and_then(|set| {
                set.components
                    .iter()
                    .position(|component| component.symbol_id() == admitted.context.id())
            })
            .map(Self::Component)
            .ok_or(ObjectFrameError::InvalidContext)
    }
    pub(super) fn is_component(self) -> bool {
        matches!(self, Self::Component(_))
    }
    pub(super) fn layers<'a>(&self, document: &'a ObjectDocument) -> &'a [ObjectLayer] {
        match *self {
            Self::Scene => document.layers(),
            Self::Component(index) => document
                .component_set()
                .expect("admitted retained component table")
                .components[index]
                .layers(),
        }
    }
    pub(super) fn objects<'a>(&self, document: &'a ObjectDocument) -> &'a [ObjectRecord] {
        match *self {
            Self::Scene => document.objects(),
            Self::Component(index) => document
                .component_set()
                .expect("admitted retained component table")
                .components[index]
                .objects(),
        }
    }
}

impl crate::object_frame_packet::ObjectFramePacket {
    /// Index equality alone is not provenance: a foreign snapshot may carry the
    /// same target. The record must be borrowed from this retained source.
    pub(crate) fn owns_record(
        &self,
        source: crate::object_frame_packet::ObjectFrameRecord<'_>,
    ) -> bool {
        self.source_objects()
            .get(source.source_object_index())
            .is_some_and(|record| std::ptr::eq(record, source.record()))
            && self
                .source_layers()
                .get(source.source_layer_index())
                .is_some_and(|layer| layer.layer_uid() == source.record().target().layer_uid())
    }
}
