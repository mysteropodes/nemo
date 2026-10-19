//! Detached Motion expression context views (D17/#2076, first provider).
//!
//! Binds the `comp`, `self` and compatibility-alias (`thisComp`, `thisLayer`)
//! identity views of `src/js/motion.js` (`COMP_VIEW`, `SELF_VIEW`,
//! `buildAliasComp`, `buildAliasLayer`, `resolveHolderLayer`) to one
//! retained immutable `ObjectSnapshot` and one D10 `PropertyHolderTarget`.
//! Every value is copied out at bind time: a view holds no borrow, handle or
//! reference into the snapshot, so dropping or superseding the snapshot never
//! changes it, and nothing reached through it can write the document.
//!
//! Motion semantics kept on purpose:
//! - an unresolved holder has no `self`: Motion answers `null`/`-1`, the
//!   alias layer is `null`;
//! - a layer target resolves by layer uid; an element target additionally
//!   needs a property holder for exactly that target (Motion's
//!   `elementMotion[strokeId]` entry);
//! - `hasParent` is the truthiness of the stored uid (`""` is false), while
//!   `parent` is resolved by that parent's NAME through Motion's
//!   `layerSnapshot(name)`, so it is the FIRST layer carrying that name and
//!   `null` when the parent is gone or unnamed;
//! - `frames` is the stored total, but the alias `duration` divides
//!   `totalFrames || 1`.
//!
//! Named dependencies, not provided here: sampled parent/self fields
//! (position, keys, controls; D11/D15), markers (D16/D26), in/out points,
//! mesh vertex identity, contentBox geometry, text views and component
//! playback/override/camera/marker evaluation beyond these identity views.
//! A bare drawing carries no canvas or fps,
//! so its comp view is unavailable rather than defaulted.

use crate::evaluation::object_component_kernels::object_context_resolver::ContextSource;
use crate::object_document::{ObjectDocument, ObjectLayer, ProjectMetadata, PropertyHolderTarget};
use crate::revision::ObjectSnapshot;

/// Why a view cannot be bound or answered. Each names a missing provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ViewUnavailable {
    /// The target's context is not declared by the retained document.
    ComponentContext,
    /// An order-plan context has no corresponding declared document context.
    ComponentLayers,
    /// No project metadata (canvas, fps) in this revision.
    ProjectMetadata,
}

/// Identity of the exact immutable revision a view was bound to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SnapshotIdentity {
    instance_id: String,
    document_id: String,
    snapshot_id: String,
    content_revision: u64,
}

impl SnapshotIdentity {
    fn of(snapshot: &ObjectSnapshot) -> Self {
        Self {
            instance_id: snapshot.instance_id().to_owned(),
            document_id: snapshot.document_id().to_owned(),
            snapshot_id: snapshot.snapshot_id().to_owned(),
            content_revision: snapshot.content_revision(),
        }
    }
}

/// `comp` and its `thisComp` alias, over one declared document context.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CompView {
    width: u32,
    height: u32,
    fps: f64,
    frames: u32,
    layers: usize,
    name: String,
}

impl CompView {
    pub(crate) fn width(&self) -> u32 {
        self.width
    }
    pub(crate) fn height(&self) -> u32 {
        self.height
    }
    pub(crate) fn fps(&self) -> f64 {
        self.fps
    }
    pub(crate) fn frames(&self) -> u32 {
        self.frames
    }
    pub(crate) fn layers(&self) -> usize {
        self.layers
    }
    /// Component name follows the baseline `name || "Component"` rule.
    pub(crate) fn name(&self) -> &str {
        &self.name
    }
    /// Alias `thisComp.frameDuration`.
    pub(crate) fn frame_duration(&self) -> f64 {
        1.0 / self.fps
    }
    /// Alias `thisComp.duration`: `(totalFrames || 1) / fps`.
    pub(crate) fn duration(&self) -> f64 {
        f64::from(self.frames.max(1)) / self.fps
    }
    /// Alias `thisComp.numLayers`.
    pub(crate) fn num_layers(&self) -> usize {
        self.layers
    }
}

/// `self.parent`: the first layer named like the stored parent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ParentRef {
    name: String,
    index: usize,
}

impl ParentRef {
    pub(crate) fn name(&self) -> &str {
        &self.name
    }
    pub(crate) fn index(&self) -> usize {
        self.index
    }
}

/// `self` and its `thisLayer` alias for a resolved holder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SelfView {
    layer_uid: String,
    stroke_id: Option<String>,
    name: Option<String>,
    index: usize,
    has_parent: bool,
    parent: Option<ParentRef>,
}

impl SelfView {
    pub(crate) fn layer_uid(&self) -> &str {
        &self.layer_uid
    }
    pub(crate) fn stroke_id(&self) -> Option<&str> {
        self.stroke_id.as_deref()
    }
    /// Stored layer name; `None` when the record has none (Motion `undefined`).
    pub(crate) fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }
    pub(crate) fn index(&self) -> usize {
        self.index
    }
    /// Element holder. Mesh holders are not representable in D10 targets yet.
    pub(crate) fn is_shape(&self) -> bool {
        self.stroke_id.is_some()
    }
    pub(crate) fn has_parent(&self) -> bool {
        self.has_parent
    }
    pub(crate) fn parent(&self) -> Option<&ParentRef> {
        self.parent.as_ref()
    }
}

/// The detached context one expression evaluation reads.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ExpressionContextView {
    identity: SnapshotIdentity,
    target: PropertyHolderTarget,
    property: String,
    comp: Result<CompView, ViewUnavailable>,
    self_view: Option<SelfView>,
}

impl ExpressionContextView {
    /// Bind to one retained snapshot. Fails only for contexts this provider
    /// cannot describe; an unresolved holder is a view without `self`.
    pub(crate) fn bind(
        snapshot: &ObjectSnapshot,
        target: &PropertyHolderTarget,
        property: &str,
    ) -> Result<Self, ViewUnavailable> {
        let metadata = snapshot.project().map(|p| p.project());
        let identity = SnapshotIdentity::of(snapshot);
        bind_parts(identity, snapshot.document(), metadata, target, property)
    }
    /// True only for the exact revision this view was bound to.
    pub(crate) fn is_bound_to(&self, snapshot: &ObjectSnapshot) -> bool {
        self.identity == SnapshotIdentity::of(snapshot)
    }
    pub(crate) fn identity(&self) -> &SnapshotIdentity {
        &self.identity
    }
    pub(crate) fn target(&self) -> &PropertyHolderTarget {
        &self.target
    }
    /// `self.property`: the evaluated property key, verbatim.
    pub(crate) fn property(&self) -> &str {
        &self.property
    }
    pub(crate) fn comp(&self) -> Result<&CompView, ViewUnavailable> {
        self.comp.as_ref().map_err(|e| *e)
    }
    /// `self` (and `thisLayer`); `None` is Motion's unresolved holder.
    pub(crate) fn self_view(&self) -> Option<&SelfView> {
        self.self_view.as_ref()
    }
}

fn bind_parts(
    identity: SnapshotIdentity,
    document: &ObjectDocument,
    metadata: Option<&ProjectMetadata>,
    target: &PropertyHolderTarget,
    property: &str,
) -> Result<ExpressionContextView, ViewUnavailable> {
    let source = ContextSource::of_document(document);
    let context = source
        .context(target.context_id())
        .map_err(|_| ViewUnavailable::ComponentContext)?;
    // Keep malformed/unresolved order declarations unavailable. A real declared
    // component does not obscure the root or another component's layer list.
    if document.composition_order.value().is_some_and(|plan| {
        plan.contexts
            .iter()
            .any(|entry| source.context(&entry.context_id).is_err())
    }) {
        return Err(ViewUnavailable::ComponentLayers);
    }
    let component = context.component();
    let comp = metadata
        .map(|m| {
            let (width, height) = m.canvas_dimensions();
            CompView {
                width,
                height,
                // enterSymbol inherits project dimensions and uses sym.fps || state.fps.
                fps: component
                    .and_then(|c| c.fps.value())
                    .and_then(|fps| fps.as_f64())
                    .unwrap_or(m.fps()),
                frames: context.total_frames(),
                layers: context.layers().len(),
                name: component
                    .map(|c| {
                        c.name
                            .value()
                            .filter(|name| !name.is_empty())
                            .map(String::as_str)
                            .unwrap_or("Component")
                    })
                    .unwrap_or("Scene")
                    .to_owned(),
            }
        })
        .ok_or(ViewUnavailable::ProjectMetadata);
    Ok(ExpressionContextView {
        identity,
        target: target.clone(),
        property: property.to_owned(),
        comp,
        self_view: resolve_self(document, context.layers(), target),
    })
}

fn resolve_self(
    document: &ObjectDocument,
    layers: &[ObjectLayer],
    target: &PropertyHolderTarget,
) -> Option<SelfView> {
    let index = layers
        .iter()
        .position(|l| l.layer_uid() == target.layer_uid())?;
    if target.stroke_id().is_some() {
        let holders = document.property_holders().unwrap_or(&[]);
        holders.iter().find(|h| h.target() == target)?;
    }
    let layer = &layers[index];
    let parent_uid = layer.parent_layer_uid.value().and_then(Option::as_deref);
    let has_parent = parent_uid.is_some_and(|uid| !uid.is_empty());
    let parent = parent_uid
        .filter(|uid| !uid.is_empty())
        .and_then(|uid| layers.iter().find(|l| l.layer_uid() == uid))
        .and_then(|p| p.name())
        .filter(|name| !name.is_empty())
        .and_then(|name| {
            let first = layers.iter().position(|l| l.name() == Some(name))?;
            Some(ParentRef {
                name: name.to_owned(),
                index: first,
            })
        });
    Some(SelfView {
        layer_uid: layer.layer_uid().to_owned(),
        stroke_id: target.stroke_id().map(str::to_owned),
        name: layer.name().map(str::to_owned),
        index,
        has_parent,
        parent,
    })
}

#[cfg(test)]
pub(crate) fn bind_for_test(
    document: &ObjectDocument,
    metadata: Option<&ProjectMetadata>,
    target: &PropertyHolderTarget,
    property: &str,
) -> Result<ExpressionContextView, ViewUnavailable> {
    let identity = SnapshotIdentity {
        instance_id: "test".into(),
        document_id: "test-document".into(),
        snapshot_id: "test-snapshot".into(),
        content_revision: 0,
    };
    bind_parts(identity, document, metadata, target, property)
}
