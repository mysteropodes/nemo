//! #2256 A-comp-edit-timeline: a declared component as the "scene" of the document-level frame, key and layer kernels.
//!
//! The timing kernels (`object_frame_edit_document`, the exposure, blank-key, retime, range, lifecycle and policy edits) are pure
//! functions of one `ObjectDocument` whose layers, records, order scopes, holders and items sit in the context `scene-root`. In the
//! baseline `enterSymbol` swaps `state.layers` for the component's layers and every timeline edit then works unchanged on them
//! (1065e9d8:src/js/app.js:4130). `project` is that swap: a detached document in which the component's own layers, records, order
//! scope, holders, items and frame count are the scene, with every context-keyed row retagged `scene-root`. The unchanged kernel runs
//! on it and `embed` writes the result back into the REAL document: the component's layers/records/total frames, its holders, items
//! and order scope retagged to the component id, everything else (the scene, the other components, the placement tables) exactly as
//! it was. A result that touched anything the view does not carry (folders, arcs, audio, component tables, image meshes) is refused
//! `Unavailable`, never half-embedded. The embedded document passes the same strict storage admission as every other edit.
use crate::object_document::object_component_record_kernels::object_component_records::{
    ComponentRecord, SCENE_CONTEXT,
};
use crate::object_document::tween::TweenStore;
use crate::object_document::{
    AuthoredOrder, FrameScope, FrameScopeKind, ItemRecords, ObjectDocument, ObjectRecord,
    OrderContext, OrderElement, OrderLayer, OrderPlan, OrderScope, Presence, PropertyHolder,
};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;

/// The context a payload names: absent is the scene root, anything else must be a declared component.
pub(crate) fn context_of(field: &Presence<String>) -> Result<&str, DispatchErrorCode> {
    match field {
        Presence::Missing => Ok(SCENE_CONTEXT),
        Presence::Present(id) if !id.is_empty() => Ok(id.as_str()),
        Presence::Present(_) => Err(DispatchErrorCode::InvalidRequest),
    }
}

fn component<'a>(
    document: &'a ObjectDocument,
    context: &str,
) -> Result<&'a ComponentRecord, DispatchErrorCode> {
    if document.format_version() != 2 {
        return Err(DispatchErrorCode::Unavailable);
    }
    document
        .component_set()
        .and_then(|set| {
            set.components
                .iter()
                .find(|record| record.symbol_id == context)
        })
        .ok_or(DispatchErrorCode::NotFound)
}

fn retag_record(record: &ObjectRecord, to: &str) -> ObjectRecord {
    let mut moved = record.clone();
    moved.target.context_id = to.to_owned();
    moved
}

fn retag_holder(holder: &PropertyHolder, to: &str) -> PropertyHolder {
    let mut moved = holder.clone();
    moved.target.context_id = to.to_owned();
    moved
}

/// The detached document whose scene IS `context`. A scene-root context is the document itself.
pub(crate) fn project(
    document: &ObjectDocument,
    context: &str,
) -> Result<ObjectDocument, DispatchErrorCode> {
    if context == SCENE_CONTEXT {
        return Ok(document.clone());
    }
    let record = component(document, context)?;
    let mut view = document.clone();
    view.total_frames = record.total_frames;
    view.layers = record.layers.clone();
    view.objects = record
        .objects
        .iter()
        .map(|item| retag_record(item, SCENE_CONTEXT))
        .collect();
    view.property_holders = match &document.property_holders {
        Presence::Missing => Presence::Missing,
        Presence::Present(all) => Presence::Present(
            all.iter()
                .filter(|holder| holder.target.context_id == context)
                .map(|holder| retag_holder(holder, SCENE_CONTEXT))
                .collect(),
        ),
    };
    view.items = project_items(&document.items, context);
    view.composition_order = match &document.composition_order {
        Presence::Missing => Presence::Missing,
        Presence::Present(plan) => {
            let mut narrowed = plan.clone();
            narrowed.contexts = vec![match plan
                .contexts
                .iter()
                .find(|scope| scope.context_id == context)
            {
                Some(explicit) => OrderContext {
                    context_id: SCENE_CONTEXT.to_owned(),
                    ..explicit.clone()
                },
                // A component whose owner defines natural order carries no inner plan: the view needs one to be a valid scene.
                None => natural_context(&view),
            }];
            Presence::Present(narrowed)
        }
    };
    view.tweens = project_tweens(document, &view);
    view.layer_folders = Presence::Missing;
    view.motion_arcs = Presence::Missing;
    view.audio_tracks = Presence::Missing;
    view.component_tables = Presence::Missing;
    Ok(view)
}

fn no_order() -> AuthoredOrder {
    AuthoredOrder {
        static_value: Presence::Missing,
        track: Presence::Missing,
        expression: Presence::Missing,
    }
}

/// The natural-order plan of a view: layers in stack order, every authored record and item once, in stored order per (frame, layer).
fn natural_context(view: &ObjectDocument) -> OrderContext {
    let layers = view
        .layers
        .iter()
        .zip(0u32..)
        .map(|(layer, natural_position)| OrderLayer {
            layer_uid: layer.layer_uid.clone(),
            natural_position,
            order: no_order(),
        })
        .collect();
    let targets = view.objects.iter().map(|record| &record.target).chain(
        view.items
            .value()
            .into_iter()
            .flat_map(|items| items.targets()),
    );
    let mut scopes: Vec<OrderScope> = Vec::new();
    for target in targets.filter(|t| t.frame_scope.kind == FrameScopeKind::Authored) {
        let frame = target.frame_scope.frame;
        let at = match scopes
            .iter()
            .position(|scope| scope.frame_scope.frame == frame)
        {
            Some(at) => at,
            None => {
                scopes.push(OrderScope {
                    frame_scope: FrameScope {
                        kind: FrameScopeKind::Authored,
                        frame,
                    },
                    elements: Vec::new(),
                });
                scopes.len() - 1
            }
        };
        let natural_position = scopes[at]
            .elements
            .iter()
            .filter(|element| element.layer_uid == target.layer_uid)
            .count();
        scopes[at].elements.push(OrderElement {
            layer_uid: target.layer_uid.clone(),
            stroke_id: target.stroke_id.clone(),
            natural_position: u32::try_from(natural_position).unwrap_or(u32::MAX),
            order: no_order(),
        });
    }
    scopes.sort_by_key(|scope| scope.frame_scope.frame);
    OrderContext {
        context_id: SCENE_CONTEXT.to_owned(),
        layers,
        scopes,
    }
}

fn project_items(items: &Presence<ItemRecords>, context: &str) -> Presence<ItemRecords> {
    let Some(all) = items.value() else {
        return Presence::Missing;
    };
    let mut kept = ItemRecords {
        raster: Presence::Missing,
        text: Presence::Missing,
    };
    if let Some(raster) = all.raster.value() {
        let rows: Vec<_> = raster
            .iter()
            .filter(|entry| entry.target.context_id == context)
            .cloned()
            .map(|mut entry| {
                entry.target.context_id = SCENE_CONTEXT.to_owned();
                entry
            })
            .collect();
        kept.raster = present_if_any(rows);
    }
    if let Some(text) = all.text.value() {
        let rows: Vec<_> = text
            .iter()
            .filter(|entry| entry.target.context_id == context)
            .cloned()
            .map(|mut entry| {
                entry.target.context_id = SCENE_CONTEXT.to_owned();
                entry
            })
            .collect();
        kept.text = present_if_any(rows);
    }
    if kept.raster.missing() && kept.text.missing() {
        Presence::Missing
    } else {
        Presence::Present(kept)
    }
}

fn present_if_any<T>(rows: Vec<T>) -> Presence<Vec<T>> {
    if rows.is_empty() {
        Presence::Missing
    } else {
        Presence::Present(rows)
    }
}

fn project_tweens(document: &ObjectDocument, view: &ObjectDocument) -> Presence<TweenStore> {
    let Some(store) = document.tweens.value() else {
        return Presence::Missing;
    };
    let mut kept = store.clone();
    kept.layers.retain(|entry| {
        view.layers
            .iter()
            .any(|layer| layer.layer_uid == entry.layer_uid)
    });
    if kept.layers.is_empty() {
        Presence::Missing
    } else {
        Presence::Present(kept)
    }
}

/// Write a kernel's result on `project(original, context)` back into `original`.
pub(crate) fn embed(
    original: &ObjectDocument,
    context: &str,
    view: &ObjectDocument,
    edited: ObjectDocument,
) -> Result<ObjectDocument, DispatchErrorCode> {
    if context == SCENE_CONTEXT {
        return Ok(edited);
    }
    component(original, context)?;
    // The view carried none of these: a kernel that wrote one has done something this embedding cannot place.
    if !edited.layer_folders.missing()
        || !edited.motion_arcs.missing()
        || !edited.audio_tracks.missing()
        || !edited.component_tables.missing()
        || edited.image_meshes != view.image_meshes
    {
        return Err(DispatchErrorCode::Unavailable);
    }
    let mut next = original.clone();
    let record = next
        .component_tables
        .value_mut()
        .and_then(|tables| tables.components.value_mut())
        .and_then(|set| {
            set.components
                .iter_mut()
                .find(|record| record.symbol_id == context)
        })
        .ok_or(DispatchErrorCode::NotFound)?;
    record.total_frames = edited.total_frames;
    record.layers = edited.layers.clone();
    record.objects = edited
        .objects
        .iter()
        .map(|item| retag_record(item, context))
        .collect();
    next.property_holders = embed_holders(&original.property_holders, context, &edited);
    next.composition_order = embed_order(&original.composition_order, context, &edited)?;
    next.items = embed_items(&original.items, context, &edited);
    next.tweens = embed_tweens(original, view, &edited);
    ObjectSnapshot::validate_drawing(&next).map_err(|_| DispatchErrorCode::InvalidRequest)?;
    Ok(next)
}

fn embed_holders(
    original: &Presence<Vec<PropertyHolder>>,
    context: &str,
    edited: &ObjectDocument,
) -> Presence<Vec<PropertyHolder>> {
    let mut rows: Vec<PropertyHolder> = original
        .value()
        .map(|all| {
            all.iter()
                .filter(|holder| holder.target.context_id != context)
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    if let Some(inner) = edited.property_holders.value() {
        rows.extend(inner.iter().map(|holder| retag_holder(holder, context)));
    }
    if original.missing() && rows.is_empty() {
        Presence::Missing
    } else {
        Presence::Present(rows)
    }
}

fn embed_order(
    original: &Presence<OrderPlan>,
    context: &str,
    edited: &ObjectDocument,
) -> Result<Presence<OrderPlan>, DispatchErrorCode> {
    let Some(plan) = original.value() else {
        return Ok(Presence::Missing);
    };
    // An implicit (natural-order) component stays implicit: the view's plan was synthesized, never authored.
    if !plan
        .contexts
        .iter()
        .any(|scope| scope.context_id == context)
    {
        return Ok(Presence::Present(plan.clone()));
    }
    let inner = edited
        .composition_order
        .value()
        .and_then(|inner| inner.contexts.first());
    let mut next = plan.clone();
    if let Some(inner) = inner {
        let rewritten = OrderContext {
            context_id: context.to_owned(),
            ..inner.clone()
        };
        if let Some(slot) = next
            .contexts
            .iter_mut()
            .find(|scope| scope.context_id == context)
        {
            *slot = rewritten;
        }
    }
    Ok(Presence::Present(next))
}

fn embed_items(
    original: &Presence<ItemRecords>,
    context: &str,
    edited: &ObjectDocument,
) -> Presence<ItemRecords> {
    let mut next = ItemRecords {
        raster: Presence::Missing,
        text: Presence::Missing,
    };
    if let Some(all) = original.value() {
        if let Some(raster) = all.raster.value() {
            let rows = raster
                .iter()
                .filter(|entry| entry.target.context_id != context)
                .cloned()
                .collect::<Vec<_>>();
            next.raster = present_if_any(rows);
        }
        if let Some(text) = all.text.value() {
            let rows = text
                .iter()
                .filter(|entry| entry.target.context_id != context)
                .cloned()
                .collect::<Vec<_>>();
            next.text = present_if_any(rows);
        }
    }
    if let Some(inner) = edited.items.value() {
        let mut raster = next.raster.value().cloned().unwrap_or_default();
        raster.extend(
            inner
                .raster
                .value()
                .into_iter()
                .flatten()
                .cloned()
                .map(|mut entry| {
                    entry.target.context_id = context.to_owned();
                    entry
                }),
        );
        next.raster = present_if_any(raster);
        let mut text = next.text.value().cloned().unwrap_or_default();
        text.extend(
            inner
                .text
                .value()
                .into_iter()
                .flatten()
                .cloned()
                .map(|mut entry| {
                    entry.target.context_id = context.to_owned();
                    entry
                }),
        );
        next.text = present_if_any(text);
    }
    if next.raster.missing() && next.text.missing() {
        Presence::Missing
    } else {
        Presence::Present(next)
    }
}

fn embed_tweens(
    original: &ObjectDocument,
    view: &ObjectDocument,
    edited: &ObjectDocument,
) -> Presence<TweenStore> {
    let Some(store) = original.tweens.value() else {
        return edited.tweens.clone();
    };
    let mut next = store.clone();
    next.layers.retain(|entry| {
        !view
            .layers
            .iter()
            .any(|layer| layer.layer_uid == entry.layer_uid)
    });
    if let Some(inner) = edited.tweens.value() {
        next.layers.extend(inner.layers.iter().cloned());
    }
    Presence::Present(next)
}

/// Run a kernel that maps a document to an optional replacement inside `context`: the scene root is the document itself, a component
/// is projected, edited and embedded. `None` is a semantic no-op and publishes nothing.
pub(crate) fn through(
    origin: &ObjectDocument,
    context: &str,
    edit: impl FnOnce(&ObjectDocument) -> Result<Option<ObjectDocument>, DispatchErrorCode>,
) -> Result<Option<ObjectDocument>, DispatchErrorCode> {
    let view = project(origin, context)?;
    match edit(&view)? {
        None => Ok(None),
        Some(edited) => embed(origin, context, &view, edited).map(Some),
    }
}

#[cfg(test)]
#[path = "../tests/object_context_projection.rs"]
mod tests;
