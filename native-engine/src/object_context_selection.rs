//! Selection, hit addressing and id remap over the nested-context resolver
//! (L04-J, F14). It joins the L04-H workspace kernel (`SelectionTarget`,
//! `ContextInfo`, `ContextRemap`) to the L04-J resolver.
//!
//! Baseline facts kept (rows of `fixtures/object-context-resolver/oracle.txt`):
//! - a click on a placed component selects the INSTANCE as a whole; only a visible
//!   instance layer can be hit and `locked` is deliberately ignored
//!   (`hitTestComponentLayers`, `tools.js`). A double click enters the component
//!   (`enterSymbol`), where a click selects the component's own inner stroke;
//! - entering and leaving a context clears the live selection (`enterSymbol`,
//!   `exitToScene`), so a live selection is only ever valid in the ACTIVE context;
//!   the stable identity `(context, layer uid, stroke id)` is what survives;
//! - importing a component from another project mints ONE new symbol id and copies
//!   the inner layers verbatim (`importForeignSymbolTree`): layer uids and stroke
//!   ids are preserved, only the context id is replaced, and importing the same
//!   foreign id again returns the same new id. A remap table therefore rewrites
//!   CONTEXT ids and never layer or stroke ids.
//!
//! Nothing is applied here: the kernels answer, a sealed commit (L04-M) applies.
use crate::evaluation::object_component_kernels::object_context_resolver::{
    ContextKind, ContextSource, ElementIdentity, ResolveError, ResolvedTarget,
};
use crate::evaluation::object_component_kernels::object_context_workspace::{
    ContextInfo, ContextRemap, SelectionTarget,
};
use crate::object_document::object_component_record_kernels::object_component_records::SCENE_CONTEXT;
use std::collections::{BTreeMap, BTreeSet};

#[cfg(test)]
#[path = "../tests/object_context_selection_controls.rs"]
mod controls_tests;
#[cfg(test)]
#[path = "../tests/object_context_selection_remap.rs"]
mod remap_tests;
#[cfg(test)]
#[path = "../tests/object_context_selection.rs"]
mod tests;

/// A placed component as a whole: its scene layer and the component it plays.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct InstanceRef {
    pub(crate) layer_uid: String,
    pub(crate) symbol_id: String,
}

/// What a stored or hit selection addresses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SelectionRef {
    /// A component instance (a layer of the scene context).
    Instance(InstanceRef),
    /// A layer of the active context with no element.
    Layer {
        context_id: String,
        layer_uid: String,
    },
    /// One drawing element, whatever keyed drawing it is shown at.
    Element(ElementIdentity),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SelectionError {
    Resolve(ResolveError),
    /// The selection names a context other than the active one. A context switch
    /// clears the live selection, so such a selection is stale.
    NotInActiveContext {
        selected: String,
        active: String,
    },
    /// A stroke of an instance layer is a projection of the component. The element
    /// is addressed under the component's own context.
    InstanceStrokeNotAddressable {
        symbol_id: String,
    },
    /// A hidden instance layer cannot be hit.
    HiddenInstance,
}

impl From<ResolveError> for SelectionError {
    fn from(error: ResolveError) -> Self {
        SelectionError::Resolve(error)
    }
}

/// The context catalog the workspace kernel enters: the components, in table
/// order. `scene-root` is not in it (the baseline `state.symbols` holds symbols
/// only, so entering the scene by id is the workspace's unknown no-op).
pub(crate) fn workspace_catalog(source: &ContextSource<'_>) -> Vec<ContextInfo> {
    source
        .components()
        .iter()
        .map(|component| ContextInfo {
            context_id: component.symbol_id().to_string(),
            total_frames: i64::from(component.total_frames()),
            fps: component.fps.value().and_then(|number| number.as_f64()),
        })
        .collect()
}

/// A scene layer is hit as a component instance exactly when it is an instance
/// layer and visible. `locked` never matters (the baseline comment is explicit).
pub(crate) fn instance_selectable(source: &ContextSource<'_>, layer_uid: &str) -> bool {
    if source.instance(layer_uid).is_none() {
        return false;
    }
    source
        .context(SCENE_CONTEXT)
        .ok()
        .and_then(|scene| scene.layer(layer_uid))
        .is_some_and(|(_, layer)| layer.visible())
}

/// Resolves a stored selection against the ACTIVE context (`None` is the scene).
pub(crate) fn resolve_selection(
    source: &ContextSource<'_>,
    active: Option<&str>,
    selection: &SelectionTarget,
) -> Result<SelectionRef, SelectionError> {
    let active_id = active.unwrap_or(SCENE_CONTEXT);
    let context = source.context(&selection.context_id)?;
    if selection.context_id != active_id {
        return Err(SelectionError::NotInActiveContext {
            selected: selection.context_id.clone(),
            active: active_id.to_string(),
        });
    }
    if selection.layer_uid.is_empty() {
        return Err(ResolveError::EmptyId.into());
    }
    context
        .layer(&selection.layer_uid)
        .ok_or(ResolveError::UnknownLayer)?;
    let instance = match context.kind() {
        ContextKind::SceneRoot => source.instance(&selection.layer_uid),
        ContextKind::Component => None,
    };
    match (selection.stroke_id.as_deref(), instance) {
        (None, Some(instance)) => Ok(SelectionRef::Instance(InstanceRef {
            layer_uid: selection.layer_uid.clone(),
            symbol_id: instance.symbol_id.clone(),
        })),
        (None, None) => Ok(SelectionRef::Layer {
            context_id: selection.context_id.clone(),
            layer_uid: selection.layer_uid.clone(),
        }),
        (Some(_), Some(instance)) => Err(SelectionError::InstanceStrokeNotAddressable {
            symbol_id: instance.symbol_id.clone(),
        }),
        (Some(stroke_id), None) => {
            let identity =
                ElementIdentity::new(&selection.context_id, &selection.layer_uid, stroke_id)?;
            if context.authored_frames(&identity).is_empty() {
                return Err(ResolveError::UnknownStroke.into());
            }
            Ok(SelectionRef::Element(identity))
        }
    }
}

/// The result of a click: what is selected, the inner element a click on an
/// instance landed on (addressed under the component's context), and the
/// component a double click enters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HitAddress {
    pub(crate) selection: SelectionRef,
    pub(crate) inner: Option<ElementIdentity>,
    pub(crate) enter: Option<String>,
}

/// The unique element of a component carrying `stroke_id`, if there is exactly one.
fn inner_identity(
    source: &ContextSource<'_>,
    symbol_id: &str,
    stroke_id: &str,
) -> Option<ElementIdentity> {
    let context = source.context(symbol_id).ok()?;
    let mut found = context
        .identities()
        .into_iter()
        .filter(|identity| identity.stroke_id() == stroke_id);
    let first = found.next()?;
    if found.next().is_some() {
        return None;
    }
    Some(first)
}

/// Addresses a hit on `layer_uid` of the active context. In the scene, a hit on a
/// visible instance layer selects the instance whatever stroke of it was under the
/// pointer; the stroke is reported as the component's inner element. Inside a
/// component the hit is an ordinary layer or element selection.
pub(crate) fn address_hit(
    source: &ContextSource<'_>,
    active: Option<&str>,
    layer_uid: &str,
    stroke_id: Option<&str>,
) -> Result<HitAddress, SelectionError> {
    let active_id = active.unwrap_or(SCENE_CONTEXT);
    let context = source.context(active_id)?;
    if layer_uid.is_empty() {
        return Err(ResolveError::EmptyId.into());
    }
    let (_, layer) = context.layer(layer_uid).ok_or(ResolveError::UnknownLayer)?;
    if context.kind() == ContextKind::SceneRoot {
        if let Some(instance) = source.instance(layer_uid) {
            if !layer.visible() {
                return Err(SelectionError::HiddenInstance);
            }
            let inner = stroke_id.and_then(|id| inner_identity(source, &instance.symbol_id, id));
            return Ok(HitAddress {
                selection: SelectionRef::Instance(InstanceRef {
                    layer_uid: layer_uid.to_string(),
                    symbol_id: instance.symbol_id.clone(),
                }),
                inner,
                enter: Some(instance.symbol_id.clone()),
            });
        }
    }
    let selection = SelectionTarget {
        context_id: active_id.to_string(),
        layer_uid: layer_uid.to_string(),
        stroke_id: stroke_id.map(str::to_string),
    };
    Ok(HitAddress {
        selection: resolve_selection(source, active, &selection)?,
        inner: None,
        enter: None,
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RemapRefusal {
    EmptyId,
    /// `scene-root` is never renamed and never the target of a rename.
    SceneRoot,
    /// The same old id is mapped to two different new ids.
    ConflictingMapping {
        old_id: String,
    },
    /// Two distinct contexts would end up with the same id.
    Collision {
        new_id: String,
    },
}

/// A validated old -> new CONTEXT id table. Layer uids and stroke ids are never
/// rewritten (the baseline copies them verbatim into the new context).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RemapIndex {
    forward: BTreeMap<String, String>,
}

impl RemapIndex {
    /// `live` are the context ids that exist when the table is applied (besides
    /// `scene-root`). The table is refused whole: empty ids, `scene-root`, one old
    /// id with two targets, or two contexts, mapped or live and unmapped, ending up
    /// with one id. A swap (A -> B and B -> A) is valid: the table is simultaneous.
    pub(crate) fn new(table: &[ContextRemap], live: &[&str]) -> Result<Self, RemapRefusal> {
        let mut forward: BTreeMap<String, String> = BTreeMap::new();
        for entry in table {
            if entry.old_id.is_empty() || entry.new_id.is_empty() {
                return Err(RemapRefusal::EmptyId);
            }
            if entry.old_id == SCENE_CONTEXT || entry.new_id == SCENE_CONTEXT {
                return Err(RemapRefusal::SceneRoot);
            }
            if let Some(previous) = forward.insert(entry.old_id.clone(), entry.new_id.clone()) {
                if previous != entry.new_id {
                    return Err(RemapRefusal::ConflictingMapping {
                        old_id: entry.old_id.clone(),
                    });
                }
            }
        }
        let mut ids: BTreeSet<&str> = live.iter().copied().collect();
        ids.extend(forward.keys().map(String::as_str));
        let mut images: BTreeSet<&str> = BTreeSet::new();
        for id in ids {
            let image = forward.get(id).map_or(id, String::as_str);
            if !images.insert(image) {
                return Err(RemapRefusal::Collision {
                    new_id: image.to_string(),
                });
            }
        }
        Ok(Self { forward })
    }

    /// The new context id of `old_id`; an unmapped id is preserved.
    pub(crate) fn new_id<'a>(&'a self, old_id: &'a str) -> &'a str {
        self.forward.get(old_id).map_or(old_id, String::as_str)
    }

    /// A selection with its context rewritten; layer and stroke are untouched.
    pub(crate) fn remap_selection(&self, selection: &SelectionTarget) -> SelectionTarget {
        SelectionTarget {
            context_id: self.new_id(&selection.context_id).to_string(),
            layer_uid: selection.layer_uid.clone(),
            stroke_id: selection.stroke_id.clone(),
        }
    }

    pub(crate) fn remap_identity(&self, identity: &ElementIdentity) -> ElementIdentity {
        identity.in_context(self.new_id(identity.context_id()))
    }

    /// Resolves an identity that predates the remap through the table, in the
    /// post-remap source. A new id needs no table: resolve it in the source directly.
    pub(crate) fn resolve_old<'a>(
        &self,
        source: &ContextSource<'a>,
        identity: &ElementIdentity,
        frame: u32,
    ) -> Result<ResolvedTarget<'a>, ResolveError> {
        let remapped = self.remap_identity(identity);
        let context = source.context(remapped.context_id())?;
        context.element_at(&remapped, frame)
    }
}
