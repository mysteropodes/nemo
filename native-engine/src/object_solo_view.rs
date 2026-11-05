//! #2371 C-solo-2: the Solo view's visibility rule, applied while a frame packet is built. Baseline 1065e9d8
//! `layerIsEffectivelyVisible` (app.js): while ANY layer of the shown context is soloed a layer is visible exactly when it, or
//! its folder parent, is soloed, whatever its eye says; with no solo the eye decides. Everything the baseline derives from that
//! one answer (the layer's items, its text and video, a folder's children, an effect, matte or mask layer that stops applying
//! once it is not soloed) follows it here, because the packet and the joins read the layer's visibility through `layer_visible`.
//! Without a solo the same function gives the baseline's other half: a hidden folder layer hides its children.
//!
//! The set is a session view fact, never document data. It is held for the duration of one scope on the calling thread (the
//! preview and export preparation run synchronously), so the packet owner needs no new parameter and the stored `visible` flag
//! of the document is never touched. Outside a scope, or in another context than the shown one, the stored flag decides.
use crate::object_document::ObjectLayer;
use std::cell::RefCell;

struct View {
    context_id: String,
    soloed: Vec<String>,
}

thread_local! {
    static VIEW: RefCell<Option<View>> = const { RefCell::new(None) };
}

/// Restores the view that was current before `enter` when dropped (also on unwind).
pub struct SoloViewScope {
    previous: Option<View>,
}

impl Drop for SoloViewScope {
    fn drop(&mut self) {
        let previous = self.previous.take();
        VIEW.with(|view| *view.borrow_mut() = previous);
    }
}

/// Open the Solo view of the shown context `context_id` for this thread. An empty set opens an inert scope.
pub fn enter(context_id: &str, soloed: &[String]) -> SoloViewScope {
    let next = (!soloed.is_empty()).then(|| View {
        context_id: context_id.to_owned(),
        soloed: soloed.to_vec(),
    });
    SoloViewScope {
        previous: VIEW.with(|view| view.replace(next)),
    }
}

/// The folder layer `layer` hangs from (one level, baseline `_layerFolderParent`); `None` for a layer under no folder layer.
fn folder_parent<'a>(layers: &'a [ObjectLayer], layer: &ObjectLayer) -> Option<&'a ObjectLayer> {
    layer
        .parent_layer_uid
        .value()
        .and_then(Option::as_deref)
        .filter(|parent| *parent != layer.layer_uid())
        .and_then(|parent| layers.iter().find(|l| l.layer_uid() == parent))
        .filter(|parent| parent.is_folder_layer())
}

/// Which of `layers` stay visible under the soloed uids; `None` when no soloed layer belongs to `layers` (baseline
/// `anyLayerSoloed` reads the shown context only, so the flag then changes nothing). The children of a soloed FOLDER layer
/// count as soloed.
pub(crate) fn solo_visibility(layers: &[ObjectLayer], soloed: &[String]) -> Option<Vec<bool>> {
    let on = |uid: &str| soloed.iter().any(|s| s == uid);
    if !layers.iter().any(|l| on(l.layer_uid())) {
        return None;
    }
    Some(
        layers
            .iter()
            .map(|layer| {
                on(layer.layer_uid())
                    || folder_parent(layers, layer).is_some_and(|f| on(f.layer_uid()))
            })
            .collect(),
    )
}

/// The soloed uids when a Solo view is open for `context_id` and one of them belongs to `layers`.
fn soloed_in(context_id: &str, layers: &[ObjectLayer]) -> Option<Vec<String>> {
    VIEW.with(|view| {
        view.borrow()
            .as_ref()
            .filter(|view| view.context_id == context_id)
            .filter(|view| {
                layers
                    .iter()
                    .any(|l| view.soloed.iter().any(|s| s == l.layer_uid()))
            })
            .map(|view| view.soloed.clone())
    })
}

/// The eye with the folder rule (baseline: a hidden folder layer hides its children), no Solo involved.
fn eye_visible(layers: &[ObjectLayer], layer: &ObjectLayer) -> bool {
    layer.visible() && folder_parent(layers, layer).is_none_or(ObjectLayer::visible)
}

/// The effective visibility of every layer of `layers` (the layers of context `context_id`), in order.
pub(crate) fn layer_visibility(context_id: &str, layers: &[ObjectLayer]) -> Vec<bool> {
    soloed_in(context_id, layers)
        .and_then(|soloed| solo_visibility(layers, &soloed))
        .unwrap_or_else(|| {
            layers
                .iter()
                .map(|layer| eye_visible(layers, layer))
                .collect()
        })
}

/// The effective visibility of one layer of `layers` (the layers of context `context_id`).
pub(crate) fn layer_visible(context_id: &str, layers: &[ObjectLayer], layer: &ObjectLayer) -> bool {
    match soloed_in(context_id, layers) {
        Some(soloed) => {
            let on = |uid: &str| soloed.iter().any(|s| s == uid);
            on(layer.layer_uid()) || folder_parent(layers, layer).is_some_and(|f| on(f.layer_uid()))
        }
        None => eye_visible(layers, layer),
    }
}
