//! #2256 (component writes): the vector edit owner edits a declared component context exactly as it edits the scene.
//!
//! Baseline `enterSymbol` (app.js @1065e9d8) swaps `state.layers` for the component's layers, so Draw, Pen, shapes,
//! Select and Delete edit the component's own layers with the scene's tools and one undo step per gesture. Natively
//! the component's layers and records live inside its record of the component table, not in `ObjectDocument.layers`
//! and `objects`. Instead of a second copy of every edit kernel, the kernels run unchanged over a VIEW of the document:
//! a clone whose `layers`, `objects` and `total_frames` are the component's. The kernels' own rules (target and frame
//! bounds, locked layers, path effects, order plan, holders, fill regeneration) therefore apply to the component as
//! they do to the scene, and the edited view is written back into the component record of the ORIGINAL document, which
//! is then admitted whole by the one storage validator before it can publish.
//!
//! The context is never invented: a target names a context, and only `scene-root` or a component the document declares
//! resolves. Anything else (an id the document does not declare, a component deleted since the caller read it) is
//! an invalid target and nothing is prepared. Nothing in the scene root is touched by a component edit.
use crate::object_document::{ObjectDocument, ObjectTarget, Presence};
use crate::request_receipts::DispatchErrorCode;

pub(super) const SCENE: &str = "scene-root";

/// The context a payload edits: its first addressed target's, the scene when it addresses none. A request spanning
/// contexts is refused by the kernels, which compare every target with the first.
pub(super) fn context_of<'a>(targets: impl IntoIterator<Item = &'a ObjectTarget>) -> String {
    targets
        .into_iter()
        .next()
        .map_or_else(|| SCENE.to_string(), |target| target.context_id.clone())
}

/// A component's view of `origin`, or `None` for the scene (the origin is the base itself).
pub(super) fn view(
    origin: &ObjectDocument,
    context: &str,
) -> Result<Option<ObjectDocument>, DispatchErrorCode> {
    if context == SCENE {
        return Ok(None);
    }
    let component = origin
        .component_set()
        .and_then(|set| set.component(context))
        // An undeclared context was always an invalid target of the vector owner (storage admission); that code stays.
        .ok_or(DispatchErrorCode::InvalidRequest)?;
    let mut view = origin.clone();
    view.layers = component.layers.clone();
    view.objects = component.objects.clone();
    view.total_frames = component.total_frames;
    Ok(Some(view))
}

/// Write an edited view back: the component record takes the view's layers and records, every other member of the
/// result is what the kernels left in the view (order plan, holders, items), and the scene's own layers, records and
/// frame count are the origin's.
pub(super) fn restore(
    mut edited: ObjectDocument,
    origin: &ObjectDocument,
    context: &str,
) -> Result<ObjectDocument, DispatchErrorCode> {
    let layers = std::mem::replace(&mut edited.layers, origin.layers.clone());
    let objects = std::mem::replace(&mut edited.objects, origin.objects.clone());
    edited.total_frames = origin.total_frames;
    let component = match &mut edited.component_tables {
        Presence::Present(tables) => match &mut tables.components {
            Presence::Present(set) => set
                .components
                .iter_mut()
                .find(|component| component.symbol_id == context),
            _ => None,
        },
        _ => None,
    }
    .ok_or(DispatchErrorCode::NotFound)?;
    component.layers = layers;
    component.objects = objects;
    Ok(edited)
}
