//! Stable nested-context resolver (L04-J, F14).
//!
//! A Component is a CONTEXT of the one document (a tab, `enterSymbol` of the
//! baseline), not a second document. This kernel answers, over a borrowed view of
//! that document, the questions every selection, property holder and frame query
//! of a nested context needs:
//!
//! - which contexts exist (`scene-root` plus every declared component, in table
//!   order) and what each one holds (layers, authored records, frame count);
//! - which record a native target `(context, frame scope, layer uid, stroke id)`
//!   names. Resolution is by those four exact identities only: an array position,
//!   a Motion element label ("Shape 1", a per-frame positional label of
//!   `layerElements`/`elementLabel`) or any other frame-local presentation id is
//!   not an input of any function here and never matches a stroke id;
//! - the frame-less element identity `(context, layer uid, stroke id)` that a
//!   selection keeps across context switches, edits, layer reorders and keyed
//!   drawings (one element spans every keyed drawing that carries its stroke id);
//! - which element(s) an exposed component property targets (`targetStrokeId` is
//!   scoped to its own component: the same stroke id in two components is two
//!   different elements);
//! The snapshot pins and the frame-query admission of a context are the sibling
//! `object_context_admission`.
//!
//! Inner layers and strokes stay inside their component record (`contextId ==
//! symbolId`, leaf L04-I), so the same layer or stroke id in two components, or
//! in a component and the scene, is never a collision. Nothing here mutates a
//! document, mints an id or owns a copy of document content: the views borrow.
//! Absent providers are explicit: frame EVALUATION of a component (playback,
//! overrides, camera) is the evaluation join (L04-K) and the frame packet keeps
//! answering `Unavailable` for every non-root context until that join routes it.
use crate::object_document::object_component_record_kernels::object_component_records::{
    ComponentInstance, ComponentRecord, ComponentSet, ExposedProp, SCENE_CONTEXT,
};
use crate::object_document::{
    FrameScope, FrameScopeKind, ObjectDocument, ObjectLayer, ObjectRecord, ObjectTarget,
};

#[cfg(test)]
#[path = "../tests/object_context_resolver_controls.rs"]
mod controls_tests;
#[cfg(test)]
#[path = "../tests/object_context_resolver_elements.rs"]
mod element_tests;
#[cfg(test)]
#[path = "../tests/object_context_resolver_sequence.rs"]
mod sequence_tests;
#[cfg(test)]
#[path = "../tests/object_context_resolver.rs"]
mod tests;
#[cfg(test)]
#[path = "../tests/object_context_resolver_world.rs"]
pub(crate) mod world;

/// Why a context, layer or target did not resolve. Nothing is partially resolved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ResolveError {
    /// A context, layer or stroke id is empty.
    EmptyId,
    /// The context is neither `scene-root` nor a declared component.
    UnknownContext,
    /// The target names another context than the one asked to resolve it.
    WrongContext,
    /// The layer is not a layer of the context.
    UnknownLayer,
    /// Component records are authored-frame records; a Reference scope has no record there.
    ReferenceUnavailable,
    /// The frame is not below the context's `totalFrames`.
    FrameOutOfRange,
    /// No record of that layer and context carries the stroke id at any frame.
    UnknownStroke,
    /// The element exists in the layer but has no authored record at this frame.
    NotAuthoredAtFrame,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ContextKind {
    SceneRoot,
    Component,
}

/// A borrowed view of one document: the scene layer list and records plus the
/// optional component table. Constructing it copies nothing.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ContextSource<'a> {
    total_frames: u32,
    layers: &'a [ObjectLayer],
    objects: &'a [ObjectRecord],
    components: Option<&'a ComponentSet>,
}

impl<'a> ContextSource<'a> {
    pub(crate) fn new(
        total_frames: u32,
        layers: &'a [ObjectLayer],
        objects: &'a [ObjectRecord],
        components: Option<&'a ComponentSet>,
    ) -> Self {
        Self {
            total_frames,
            layers,
            objects,
            components,
        }
    }

    /// A document without the component member declares only `scene-root`.
    pub(crate) fn of_document(document: &'a ObjectDocument) -> Self {
        Self::new(
            document.total_frames(),
            document.layers(),
            document.objects(),
            document.component_set(),
        )
    }

    pub(crate) fn components(&self) -> &'a [ComponentRecord] {
        match self.components {
            Some(set) => set.components.as_slice(),
            None => &[],
        }
    }

    /// `scene-root` first, then every component symbol id in table order.
    pub(crate) fn declared_contexts(&self) -> Vec<&'a str> {
        let mut ids: Vec<&'a str> = vec![SCENE_CONTEXT];
        ids.extend(self.components().iter().map(ComponentRecord::symbol_id));
        ids
    }

    pub(crate) fn context(&self, context_id: &str) -> Result<ResolvedContext<'a>, ResolveError> {
        if context_id.is_empty() {
            return Err(ResolveError::EmptyId);
        }
        if context_id == SCENE_CONTEXT {
            return Ok(ResolvedContext {
                id: SCENE_CONTEXT,
                kind: ContextKind::SceneRoot,
                total_frames: self.total_frames,
                layers: self.layers,
                objects: self.objects,
                component: None,
            });
        }
        let component = self
            .components()
            .iter()
            .find(|component| component.symbol_id() == context_id)
            .ok_or(ResolveError::UnknownContext)?;
        Ok(ResolvedContext {
            id: component.symbol_id(),
            kind: ContextKind::Component,
            total_frames: component.total_frames(),
            layers: component.layers(),
            objects: component.objects(),
            component: Some(component),
        })
    }

    /// The placement record when `layer_uid` is a component-instance layer of the scene.
    pub(crate) fn instance(&self, layer_uid: &str) -> Option<&'a ComponentInstance> {
        self.components
            .and_then(|set| set.instance(SCENE_CONTEXT, layer_uid))
    }
}

/// One context with the layers and records it holds.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ResolvedContext<'a> {
    id: &'a str,
    kind: ContextKind,
    total_frames: u32,
    layers: &'a [ObjectLayer],
    objects: &'a [ObjectRecord],
    component: Option<&'a ComponentRecord>,
}

/// The record a target names. The indices are provenance inside the context's own
/// arrays and change with a reorder; the identities never do.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ResolvedTarget<'a> {
    pub(crate) layer_index: usize,
    pub(crate) object_index: usize,
    pub(crate) layer: &'a ObjectLayer,
    pub(crate) record: &'a ObjectRecord,
}

/// The frame-less identity of one drawing element: what a selection keeps.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct ElementIdentity {
    context_id: String,
    layer_uid: String,
    stroke_id: String,
}

impl ElementIdentity {
    pub(crate) fn new(
        context_id: &str,
        layer_uid: &str,
        stroke_id: &str,
    ) -> Result<Self, ResolveError> {
        if context_id.is_empty() || layer_uid.is_empty() || stroke_id.is_empty() {
            return Err(ResolveError::EmptyId);
        }
        Ok(Self {
            context_id: context_id.to_string(),
            layer_uid: layer_uid.to_string(),
            stroke_id: stroke_id.to_string(),
        })
    }
    pub(crate) fn of_target(target: &ObjectTarget) -> Result<Self, ResolveError> {
        Self::new(&target.context_id, &target.layer_uid, &target.stroke_id)
    }
    pub(crate) fn context_id(&self) -> &str {
        &self.context_id
    }
    pub(crate) fn layer_uid(&self) -> &str {
        &self.layer_uid
    }
    pub(crate) fn stroke_id(&self) -> &str {
        &self.stroke_id
    }
    /// The same layer and stroke in another context. Only an id remap uses this.
    pub(crate) fn in_context(&self, context_id: &str) -> Self {
        Self {
            context_id: context_id.to_string(),
            layer_uid: self.layer_uid.clone(),
            stroke_id: self.stroke_id.clone(),
        }
    }
    /// The authored-scope target of this element at `frame`.
    pub(crate) fn target_at(&self, frame: u32) -> ObjectTarget {
        ObjectTarget {
            context_id: self.context_id.clone(),
            frame_scope: FrameScope {
                kind: FrameScopeKind::Authored,
                frame,
            },
            layer_uid: self.layer_uid.clone(),
            stroke_id: self.stroke_id.clone(),
        }
    }
}

impl<'a> ResolvedContext<'a> {
    pub(crate) fn id(&self) -> &'a str {
        self.id
    }
    pub(crate) fn kind(&self) -> ContextKind {
        self.kind
    }
    pub(crate) fn total_frames(&self) -> u32 {
        self.total_frames
    }
    pub(crate) fn layers(&self) -> &'a [ObjectLayer] {
        self.layers
    }
    pub(crate) fn objects(&self) -> &'a [ObjectRecord] {
        self.objects
    }
    pub(crate) fn component(&self) -> Option<&'a ComponentRecord> {
        self.component
    }

    /// A layer by its stable uid, with its position in this context (provenance only).
    pub(crate) fn layer(&self, layer_uid: &str) -> Option<(usize, &'a ObjectLayer)> {
        self.layers
            .iter()
            .enumerate()
            .find(|&(_, layer)| layer.layer_uid() == layer_uid)
    }

    /// Resolves a full native target. The checks run in a fixed order: ids, context,
    /// layer, scope, frame range, then the record itself.
    pub(crate) fn resolve_target(
        &self,
        target: &ObjectTarget,
    ) -> Result<ResolvedTarget<'a>, ResolveError> {
        if target.context_id.is_empty()
            || target.layer_uid.is_empty()
            || target.stroke_id.is_empty()
        {
            return Err(ResolveError::EmptyId);
        }
        if target.context_id != self.id {
            return Err(ResolveError::WrongContext);
        }
        let (layer_index, layer) = self
            .layer(&target.layer_uid)
            .ok_or(ResolveError::UnknownLayer)?;
        if self.kind == ContextKind::Component
            && target.frame_scope.kind != FrameScopeKind::Authored
        {
            return Err(ResolveError::ReferenceUnavailable);
        }
        if target.frame_scope.frame >= self.total_frames {
            return Err(ResolveError::FrameOutOfRange);
        }
        let mut elsewhere = false;
        for (object_index, record) in self.objects.iter().enumerate() {
            let other = &record.target;
            if other.context_id != target.context_id
                || other.layer_uid != target.layer_uid
                || other.stroke_id != target.stroke_id
            {
                continue;
            }
            if other.frame_scope == target.frame_scope {
                return Ok(ResolvedTarget {
                    layer_index,
                    object_index,
                    layer,
                    record,
                });
            }
            elsewhere = true;
        }
        Err(if elsewhere {
            ResolveError::NotAuthoredAtFrame
        } else {
            ResolveError::UnknownStroke
        })
    }

    /// The record of an element at an authored frame.
    pub(crate) fn element_at(
        &self,
        identity: &ElementIdentity,
        frame: u32,
    ) -> Result<ResolvedTarget<'a>, ResolveError> {
        if identity.context_id != self.id {
            return Err(ResolveError::WrongContext);
        }
        self.resolve_target(&identity.target_at(frame))
    }

    /// Every distinct element of this context, in first-record order.
    pub(crate) fn identities(&self) -> Vec<ElementIdentity> {
        let mut found: Vec<ElementIdentity> = Vec::new();
        for record in self.objects {
            if record.target.frame_scope.kind != FrameScopeKind::Authored {
                continue;
            }
            if let Ok(identity) = ElementIdentity::of_target(&record.target) {
                if !found.contains(&identity) {
                    found.push(identity);
                }
            }
        }
        found
    }

    /// Ascending authored frames that carry a record of the element.
    pub(crate) fn authored_frames(&self, identity: &ElementIdentity) -> Vec<u32> {
        let mut frames: Vec<u32> = self
            .objects
            .iter()
            .filter(|record| {
                let target = &record.target;
                target.frame_scope.kind == FrameScopeKind::Authored
                    && target.context_id == identity.context_id
                    && target.layer_uid == identity.layer_uid
                    && target.stroke_id == identity.stroke_id
            })
            .map(|record| record.target.frame_scope.frame)
            .collect();
        frames.sort_unstable();
        frames.dedup();
        frames
    }

    /// The holder-target check of `validate_property_holders`, per context: the layer is a layer
    /// of THIS context and, when a stroke id is given, it is an authored element of this context.
    /// (A holder spans every keyed drawing of its element, so no frame is part of the check.)
    pub(crate) fn resolve_holder(
        &self,
        layer_uid: &str,
        stroke_id: Option<&str>,
    ) -> Result<(), ResolveError> {
        if layer_uid.is_empty() {
            return Err(ResolveError::EmptyId);
        }
        self.layer(layer_uid).ok_or(ResolveError::UnknownLayer)?;
        if let Some(stroke_id) = stroke_id {
            let identity = ElementIdentity::new(self.id, layer_uid, stroke_id)?;
            if self.authored_frames(&identity).is_empty() {
                return Err(ResolveError::UnknownStroke);
            }
        }
        Ok(())
    }

    /// The elements an exposed property targets: every element of THIS context whose
    /// stroke id is the property's `targetStrokeId` (the baseline `epByStroke` match).
    pub(crate) fn exposed_targets(&self, prop: &ExposedProp) -> Vec<ElementIdentity> {
        if self.kind != ContextKind::Component {
            return Vec::new();
        }
        self.identities()
            .into_iter()
            .filter(|identity| identity.stroke_id == prop.target_stroke_id)
            .collect()
    }
}
