//! L01-F (G14): the single composition-order element maintenance helper. A record created or removed in a
//! document that carries a `compositionOrder` plan must gain or lose exactly one scoped order element in the
//! record's own context, or the plan no longer covers the authored records and storage admission refuses it.
//! Pure functions over the plan; admission stays with the one native codec (`validate_storage_order`).
//! The same helper is intended for every lane that adds or removes a record (group, boolean, duplicator,
//! frame edit), so those lanes never grow a second copy.
use crate::object_document::{
    AuthoredOrder, FrameScopeKind, ObjectTarget, OrderElement, OrderPlan, OrderScope, Presence,
};

/// Why a plan could not be maintained; the caller maps these to typed dispatch errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OrderEditError {
    /// The target context is not declared by the plan.
    ContextAbsent,
    /// Only Authored scopes exist in a plan; Reference order is unavailable.
    ReferenceScope,
    /// A removal found no element for the target (the plan and the records disagree).
    ElementAbsent,
    /// The element position does not fit the plan's `u32` positions.
    PositionOverflow,
}

/// Append an element with empty authored order at the top of the target layer's natural stack inside the
/// target's own context and Authored frame scope. A missing frame scope is created before the first
/// existing scope of a later frame (appended when none follows); existing scopes are never reordered.
pub(crate) fn insert_element(
    plan: &mut OrderPlan,
    target: &ObjectTarget,
) -> Result<(), OrderEditError> {
    if target.frame_scope.kind != FrameScopeKind::Authored {
        return Err(OrderEditError::ReferenceScope);
    }
    let context = plan
        .contexts
        .iter_mut()
        .find(|context| context.context_id == target.context_id)
        .ok_or(OrderEditError::ContextAbsent)?;
    let index = match context
        .scopes
        .iter()
        .position(|scope| scope.frame_scope == target.frame_scope)
    {
        Some(index) => index,
        None => {
            let at = context
                .scopes
                .iter()
                .position(|scope| scope.frame_scope.frame > target.frame_scope.frame)
                .unwrap_or(context.scopes.len());
            context.scopes.insert(
                at,
                OrderScope {
                    frame_scope: target.frame_scope.clone(),
                    elements: Vec::new(),
                },
            );
            at
        }
    };
    let scope = &mut context.scopes[index];
    // Admitted plans number each layer's elements 0..n without gaps, so the count is the next position.
    let natural_position = u32::try_from(
        scope
            .elements
            .iter()
            .filter(|element| element.layer_uid == target.layer_uid)
            .count(),
    )
    .map_err(|_| OrderEditError::PositionOverflow)?;
    scope.elements.push(OrderElement {
        layer_uid: target.layer_uid.clone(),
        stroke_id: target.stroke_id.clone(),
        natural_position,
        order: AuthoredOrder {
            static_value: Presence::Missing,
            track: Presence::Missing,
            expression: Presence::Missing,
        },
    });
    Ok(())
}

/// Remove the target's element and close the gap: later elements of the same layer in that scope move down
/// one position. An emptied scope is kept: evaluation reads a missing scope as unavailable unless the lookup
/// is an exposure, and an explicit empty scope as an empty element list (object_order_evaluation.rs).
pub(crate) fn remove_element(
    plan: &mut OrderPlan,
    target: &ObjectTarget,
) -> Result<(), OrderEditError> {
    if target.frame_scope.kind != FrameScopeKind::Authored {
        return Err(OrderEditError::ReferenceScope);
    }
    let context = plan
        .contexts
        .iter_mut()
        .find(|context| context.context_id == target.context_id)
        .ok_or(OrderEditError::ContextAbsent)?;
    let scope = context
        .scopes
        .iter_mut()
        .find(|scope| scope.frame_scope == target.frame_scope)
        .ok_or(OrderEditError::ElementAbsent)?;
    let at = scope
        .elements
        .iter()
        .position(|element| {
            element.layer_uid == target.layer_uid && element.stroke_id == target.stroke_id
        })
        .ok_or(OrderEditError::ElementAbsent)?;
    let removed = scope.elements.remove(at).natural_position;
    for element in &mut scope.elements {
        if element.layer_uid == target.layer_uid && element.natural_position > removed {
            element.natural_position -= 1;
        }
    }
    Ok(())
}

/// `insert_element`, then move the new element down to `position` in its layer's natural stack, shifting
/// the layer's elements at or above it up by one (G15 behind placement).
pub(crate) fn insert_element_at(
    plan: &mut OrderPlan,
    target: &ObjectTarget,
    position: usize,
) -> Result<(), OrderEditError> {
    insert_element(plan, target)?;
    let position = u32::try_from(position).map_err(|_| OrderEditError::PositionOverflow)?;
    let scope = plan
        .contexts
        .iter_mut()
        .find(|context| context.context_id == target.context_id)
        .and_then(|context| context.scopes.iter_mut().find(|scope| scope.frame_scope == target.frame_scope))
        .ok_or(OrderEditError::ContextAbsent)?;
    for element in scope.elements.iter_mut().filter(|e| e.layer_uid == target.layer_uid) {
        if element.stroke_id == target.stroke_id {
            element.natural_position = position;
        } else if element.natural_position >= position {
            element.natural_position += 1;
        }
    }
    Ok(())
}
