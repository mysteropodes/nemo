//! The items of converted layers (`component-convert-items`). The baseline carries a raster (and a
//! vector-text glyph path) with the frame's `strokes` array, so it keeps its stack position among
//! the strokes inside the component. Native images and text blocks are re-keyed to the component
//! context and their inner layer; a block's holder id follows its target. When any item moves and
//! the document carries an order plan, the component declares its own order context: the scene's
//! elements of the converted layers, re-keyed. Natural component order would lift every item above
//! the layer's contours. Still refused: text on a layer with text animators (a placed component
//! paints none, and a one-layer convert leaves the layer's animators on the instance),
//! bitmap-brush or brush-texture rasters, and mesh-bound images.
use crate::commands::object_fill_owner::component_owner::object_component_convert_support::SetupError;
use crate::commands::remove_order_element;
use crate::object_document::object_component_record_kernels::object_component_records::SCENE_CONTEXT;
use crate::object_document::{
    AuthoredOrder, FrameScopeKind, ItemRecords, ObjectDocument, OrderContext, OrderLayer,
    OrderScope, Presence,
};

fn scene_on(target: &crate::object_document::ObjectTarget, layer_uid: &str) -> bool {
    target.context_id == SCENE_CONTEXT && target.layer_uid == layer_uid
}

/// Whether the layer holds an item no component can paint yet.
pub(crate) fn refused_on(document: &ObjectDocument, layer_uid: &str) -> bool {
    let animated = document.layers.iter().any(|layer| {
        layer.layer_uid == layer_uid
            && layer
                .text_animators
                .value()
                .is_some_and(|animators| !animators.0.is_empty())
    });
    document.item_records().is_some_and(|items| {
        (animated
            && items
                .text_entries()
                .iter()
                .any(|entry| scene_on(&entry.target, layer_uid)))
            || items.raster_entries().iter().any(|entry| {
                let item = &entry.item;
                scene_on(&entry.target, layer_uid)
                    && (item.is_bitmap_brush() == Some(true)
                        || item.brush_group_id().is_some()
                        || item.is_brush_texture_copy() == Some(true)
                        || item.mesh_id().is_some())
            })
    })
}

/// Move the items of each `(scene layer, inner layer)` into `symbol`, in `candidate` (a clone of
/// `document`, whose scene order is read before any of the converted layers' elements are removed).
/// Returns the component's order context for [`declare`] once the layer lifecycle edits are done:
/// they match layers by uid in every context, and a merge keeps the source uids inside.
pub(crate) fn move_images(
    candidate: &mut ObjectDocument,
    document: &ObjectDocument,
    symbol: &str,
    layers: &[(String, String)],
) -> Result<Option<OrderContext>, SetupError> {
    let Some(items) = document.item_records() else {
        return Ok(None);
    };
    let inner_of = |uid: &str| {
        layers
            .iter()
            .find(|(scene, _)| scene == uid)
            .map(|(_, inner)| inner.clone())
    };
    let mut moved = Vec::new();
    let mut retarget = |target: &mut crate::object_document::ObjectTarget| {
        if target.context_id != SCENE_CONTEXT {
            return Ok(());
        }
        if let Some(inner) = inner_of(&target.layer_uid) {
            // As `retarget_objects`: only authored frames can enter a component.
            if target.frame_scope.kind != FrameScopeKind::Authored {
                return Err(SetupError::ObjectScope {
                    layer_uid: target.layer_uid.clone(),
                });
            }
            moved.push(target.clone());
            target.context_id = symbol.to_string();
            target.layer_uid = inner;
        }
        Ok(())
    };
    let mut rasters = items.raster_entries().to_vec();
    for entry in &mut rasters {
        retarget(&mut entry.target)?;
    }
    let mut texts = items.text_entries().to_vec();
    for entry in &mut texts {
        retarget(&mut entry.target)?;
    }
    if moved.is_empty() {
        return Ok(None);
    }
    candidate.items = ItemRecords::normalized(rasters, texts);
    let Presence::Present(plan) = &document.composition_order else {
        return Ok(None);
    };
    let scene = plan
        .contexts
        .iter()
        .find(|context| context.context_id == SCENE_CONTEXT)
        .ok_or_else(|| SetupError::Order(crate::commands::OrderEditError::ContextAbsent))?;
    let empty = || AuthoredOrder {
        static_value: Presence::Missing,
        track: Presence::Missing,
        expression: Presence::Missing,
    };
    // `natural_component`'s layer order: component layer index is its natural position.
    let mut context = OrderContext {
        context_id: symbol.to_string(),
        layers: Vec::with_capacity(layers.len()),
        scopes: Vec::new(),
    };
    for (position, (_, inner)) in layers.iter().enumerate() {
        context.layers.push(OrderLayer {
            layer_uid: inner.clone(),
            natural_position: u32::try_from(position).map_err(|_| SetupError::TooMany)?,
            order: empty(),
        });
    }
    for scope in &scene.scopes {
        let elements: Vec<_> = scope
            .elements
            .iter()
            .filter_map(|element| {
                let inner = inner_of(&element.layer_uid)?;
                let mut element = element.clone();
                element.layer_uid = inner;
                Some(element)
            })
            .collect();
        if !elements.is_empty() {
            context.scopes.push(OrderScope {
                frame_scope: scope.frame_scope.clone(),
                elements,
            });
        }
    }
    if let Presence::Present(plan) = &mut candidate.composition_order {
        for target in &moved {
            remove_order_element(plan, target).map_err(SetupError::Order)?;
        }
    }
    Ok(Some(context))
}

/// Add the component's order context returned by [`move_images`].
pub(crate) fn declare(candidate: &mut ObjectDocument, context: Option<OrderContext>) {
    if let (Some(context), Presence::Present(plan)) = (context, &mut candidate.composition_order) {
        plan.contexts.push(context);
    }
}
