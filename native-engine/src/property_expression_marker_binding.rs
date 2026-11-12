//! D16-K2 (#2072): the D16-K1 marker API over actual immutable native marker records.
//!
//! Mirrors where `src/js/motion.js` takes each list from, per expression context.
//! `thisComp.marker` and the alias `comp.marker` read `state.markers`: the root
//! composition's list in the scene, and `sym.markers || []` inside a component, because
//! `enterSymbol` swaps the list (`app.js`). Native component records refuse a `markers`
//! field, so a component context reads an empty list. A layer's `marker` reads that
//! layer's stored `ld.markers`, natively its `range.markers`. A missing list reads
//! empty, as `_markerApi(list || [])` does. `_exprFps` is `state.fps`, which
//! `enterSymbol` sets to `sym.fps || state.fps`: the component fps, else the project fps.
//!
//! Nothing is copied into a second store: every list is rebuilt from the pinned
//! snapshot it is asked about. A bare drawing has no composition metadata, so its
//! composition list and fps are refused instead of inventing Motion's defaults.

use crate::evaluation::object_component_kernels::object_context_resolver::{
    ContextSource, ResolvedContext,
};
use crate::evaluation::property_expression_lookup::{Fps, LookupRefusal, MarkerList, MarkerRecord};
use crate::object_document::ObjectLayer;
use crate::revision::ObjectSnapshot;

/// Why a native marker list cannot be bound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MarkerBindingError {
    Refused(LookupRefusal),
    /// The snapshot is a bare drawing: no composition markers or fps exist.
    NoProject,
    /// The context id names no context of the pinned document.
    UnknownContext,
    /// No layer of that context has this uid.
    UnknownLayer,
}

impl From<LookupRefusal> for MarkerBindingError {
    fn from(r: LookupRefusal) -> Self {
        Self::Refused(r)
    }
}

/// `_exprFps()` in context `context_id`: the component fps when the context is a
/// component that declares one, else the project fps. The `|| 24` fallback of Motion is
/// unreachable here, because the project codec admits only a finite fps above zero.
pub(crate) fn snapshot_fps(
    snapshot: &ObjectSnapshot,
    context_id: &str,
) -> Result<Fps, MarkerBindingError> {
    let project = snapshot.project().ok_or(MarkerBindingError::NoProject)?;
    let context = resolve(snapshot, context_id)?;
    let fps = context
        .component()
        .and_then(|c| c.fps.value())
        .and_then(|fps| fps.as_f64())
        .unwrap_or_else(|| project.project().fps());
    Ok(Fps::new(fps)?)
}

/// `_markerApi(state.markers)` in context `context_id`: the root composition's stored
/// markers in stored order for the scene, and an empty list inside a component.
pub(crate) fn composition_markers(
    snapshot: &ObjectSnapshot,
    context_id: &str,
) -> Result<MarkerList, MarkerBindingError> {
    let project = snapshot.project().ok_or(MarkerBindingError::NoProject)?;
    if resolve(snapshot, context_id)?.component().is_some() {
        return Ok(MarkerList::new(Vec::new())?);
    }
    let records = project
        .project()
        .composition_markers()
        .unwrap_or(&[])
        .iter()
        .map(|(frame, name, color)| MarkerRecord {
            frame: *frame,
            name: name.clone(),
            color: color.clone(),
        })
        .collect();
    Ok(MarkerList::new(records)?)
}

fn resolve<'a>(
    snapshot: &'a ObjectSnapshot,
    context_id: &str,
) -> Result<ResolvedContext<'a>, MarkerBindingError> {
    ContextSource::of_document(snapshot.document())
        .context(context_id)
        .map_err(|_| MarkerBindingError::UnknownContext)
}

/// `_markerApi(ld.markers)` for one stored layer.
pub(crate) fn layer_markers(layer: &ObjectLayer) -> Result<MarkerList, LookupRefusal> {
    let records = layer
        .range
        .value()
        .and_then(|range| range.markers.value())
        .map_or(&[][..], Vec::as_slice)
        .iter()
        .map(|m| MarkerRecord {
            frame: m.frame as f64,
            name: m.name.value().cloned(),
            color: m.color.value().cloned(),
        })
        .collect();
    MarkerList::new(records)
}

/// [`layer_markers`] for the layer `layer_uid` of context `context_id` in the pin.
pub(crate) fn snapshot_layer_markers(
    snapshot: &ObjectSnapshot,
    context_id: &str,
    layer_uid: &str,
) -> Result<MarkerList, MarkerBindingError> {
    let layer = resolve(snapshot, context_id)?
        .layers()
        .iter()
        .find(|l| l.layer_uid() == layer_uid)
        .ok_or(MarkerBindingError::UnknownLayer)?;
    Ok(layer_markers(layer)?)
}

/// D16-K2: the marker API over actual pinned native records, not supplied tables.
#[cfg(all(test, feature = "history"))]
#[path = "../tests/property_expression_marker_binding.rs"]
mod tests;
