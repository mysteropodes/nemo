//! Parent A set / clear plan (#2372 U12): `setLayerParent` of `src/js/motion.js` at 1065e9d8.
//!
//! The baseline refuses a layer as its own parent and any cycle, then reassigns `parentLayerUid` and folds the
//! displacement of the child's pivot at the playhead into its static Position, so the world pose does not jump.
//! This kernel admits the same graph (existing parent, same context, no folder, no cycle) and keeps the pose at `frame`
//! by the one case it can prove exactly: every ancestor in the old and the new chain is a pure translation there, so the
//! displacement is the difference of the two translation sums whatever the pivots are. A rotated or scaled ancestor
//! needs an evaluated pivot; an expression-driven ancestor or a keyed child Position cannot be folded into one static
//! value. Each of those is a typed refusal, never a silent jump. The kernel owns no history or id state.
use crate::object_document::object_group_kernels::object_layer_lifecycle_plan::{
    index_of, LifecycleError, LifecyclePlan,
};
use crate::object_document::{ObjectDocument, Presence};
use crate::object_snapshot::ObjectSnapshot;

/// Largest ancestor count the chain walks visit (the D30 bound the render chain shares).
const CHAIN_LIMIT: usize = 64;

/// Ancestor identities of `start` (itself first), nearest first; `None` when the walk loops or is too deep.
fn chain_of<'a>(document: &'a ObjectDocument, start: Option<&'a str>) -> Option<Vec<&'a str>> {
    let mut chain: Vec<&str> = Vec::new();
    let mut next = start;
    while let Some(uid) = next {
        if chain.contains(&uid) || chain.len() >= CHAIN_LIMIT {
            return None;
        }
        chain.push(uid);
        next = document
            .layers
            .iter()
            .find(|layer| layer.layer_uid == uid)
            .and_then(|layer| layer.parent_layer_uid.value().and_then(Option::as_deref));
    }
    Some(chain)
}

/// The edited context must list both layers when the document has an order plan; without one only the root exists.
fn in_context(document: &ObjectDocument, context_id: &str, uids: [&str; 2]) -> bool {
    match document.composition_order.value() {
        None => context_id == "scene-root",
        Some(plan) => plan
            .contexts
            .iter()
            .find(|context| context.context_id == context_id)
            .is_some_and(|context| {
                uids.iter()
                    .all(|uid| context.layers.iter().any(|l| l.layer_uid == *uid))
            }),
    }
}

mod pose {
    use super::{LifecycleError, ObjectSnapshot};
    use crate::evaluation::motion_parent_chain::motion_mat_from_values;
    use crate::object_document::{Presence, PropertyHolderTarget};

    pub(super) fn target(context_id: &str, layer_uid: &str) -> PropertyHolderTarget {
        PropertyHolderTarget {
            context_id: context_id.to_owned(),
            layer_uid: layer_uid.to_owned(),
            stroke_id: Presence::Missing,
        }
    }

    /// Sum of the translations of `chain` at `frame`; a rotation, a scale or an unsampleable pose refuses.
    pub(super) fn translation(
        snapshot: &ObjectSnapshot,
        context_id: &str,
        chain: &[&str],
        frame: u32,
    ) -> Result<[f64; 2], LifecycleError> {
        let mut sum = [0.0, 0.0];
        for uid in chain {
            let v = snapshot
                .sampled_motion_values(&target(context_id, uid), f64::from(frame))
                .map_err(|_| LifecycleError::PoseUnavailable("parent pose is not sampleable"))?;
            if let Some(m) =
                motion_mat_from_values(v.position, v.anchor, v.rotation, v.scale, v.opacity)
            {
                if m.rot != 0.0 || m.sx != 1.0 || m.sy != 1.0 {
                    return Err(LifecycleError::PoseUnavailable(
                        "a rotated or scaled parent needs its evaluated pivot",
                    ));
                }
                sum[0] += m.dx;
                sum[1] += m.dy;
            }
        }
        Ok(sum)
    }

    pub(super) fn position(
        snapshot: &ObjectSnapshot,
        context_id: &str,
        layer_uid: &str,
        frame: u32,
    ) -> Result<[f64; 2], LifecycleError> {
        snapshot
            .sampled_motion_values(&target(context_id, layer_uid), f64::from(frame))
            .map(|v| v.position)
            .map_err(|_| LifecycleError::PoseUnavailable("child position is not sampleable"))
    }
}

/// Writes the child's static Position; a keyed or expression-driven Position refuses (one value cannot hold the shift).
fn shift_position(
    snapshot: &ObjectSnapshot,
    candidate: &mut ObjectDocument,
    context_id: &str,
    layer_uid: &str,
    frame: u32,
    shift: [f64; 2],
) -> Result<(), LifecycleError> {
    use crate::object_document::{PropertyHolder, PropertyHolderTarget, PropertyRecord};
    use serde_json::Number;
    let animated = snapshot
        .document()
        .property_holders()
        .is_some_and(|holders| {
            holders.iter().any(|h| {
                h.target().context_id() == context_id
                    && h.target().layer_uid() == layer_uid
                    && h.target().stroke_id().is_none()
                    && h.properties().iter().any(|p| {
                        p.property() == "position"
                            && (p.track.value().is_some_and(|t| !t.keys.is_empty())
                                || p.expression.value().is_some())
                    })
            })
        });
    if animated {
        return Err(LifecycleError::PoseUnavailable(
            "an animated child Position cannot absorb the shift",
        ));
    }
    let now = pose::position(snapshot, context_id, layer_uid, frame)?;
    // Whole values stay integers, like every stored position the editor wrote.
    let number = |v: f64| {
        if v.fract() == 0.0 && v.abs() < 9e15 {
            return Ok(Number::from(v as i64));
        }
        Number::from_f64(v).ok_or(LifecycleError::PoseUnavailable("position is not finite"))
    };
    let value = vec![number(now[0] + shift[0])?, number(now[1] + shift[1])?];
    let mut holders = candidate
        .property_holders
        .value()
        .cloned()
        .unwrap_or_default();
    let at = holders.iter().position(|h| {
        h.target().context_id() == context_id
            && h.target().layer_uid() == layer_uid
            && h.target().stroke_id().is_none()
    });
    let holder = match at {
        Some(index) => &mut holders[index],
        None => {
            holders.push(PropertyHolder {
                mesh_source: Presence::Missing,
                target: PropertyHolderTarget {
                    context_id: context_id.to_owned(),
                    layer_uid: layer_uid.to_owned(),
                    stroke_id: Presence::Missing,
                },
                controls: Presence::Missing,
                properties: Vec::new(),
            });
            let last = holders.len() - 1;
            &mut holders[last]
        }
    };
    match holder
        .properties
        .iter_mut()
        .find(|p| p.property() == "position")
    {
        Some(record) => record.static_value = Presence::Present(value),
        None => holder.properties.push(PropertyRecord {
            property: "position".to_owned(),
            static_value: Presence::Present(value),
            track: Presence::Missing,
            expression: Presence::Missing,
        }),
    }
    candidate.property_holders = Presence::Present(holders);
    Ok(())
}

/// `setLayerParent(li, parentUid)`: `parent` `None` is the menu's "Parent A : Aucun (parentage libre)".
pub(crate) fn plan_parent(
    snapshot: &ObjectSnapshot,
    context_id: &str,
    layer_uid: &str,
    parent: Option<&str>,
    frame: u32,
) -> Result<LifecyclePlan, LifecycleError> {
    let document = snapshot.document();
    let index = index_of(document, layer_uid)?;
    if let Some(uid) = parent {
        let target = index_of(document, uid)?;
        if uid == layer_uid {
            return Err(LifecycleError::ParentSelf(layer_uid.to_owned()));
        }
        if document.layers[target].is_folder_layer.value() == Some(&true) {
            return Err(LifecycleError::ParentRefused(uid.to_owned()));
        }
        if !in_context(document, context_id, [layer_uid, uid]) {
            return Err(LifecycleError::ParentRefused(uid.to_owned()));
        }
        let cycle = |uid: &str| LifecycleError::ParentCycle {
            layer_uid: layer_uid.to_owned(),
            parent_uid: uid.to_owned(),
        };
        let ancestry = chain_of(document, Some(uid)).ok_or_else(|| cycle(uid))?;
        if ancestry.contains(&layer_uid) {
            return Err(cycle(uid));
        }
    }
    let current = document.layers[index]
        .parent_layer_uid
        .value()
        .and_then(Option::as_deref);
    if current == parent {
        return Ok(LifecyclePlan::unchanged(document, None));
    }
    let old = chain_of(document, current).ok_or(LifecycleError::PoseUnavailable(
        "the stored parent chain is not a chain",
    ))?;
    let new = chain_of(document, parent).unwrap_or_default();
    let mut candidate = document.clone();
    if old != new {
        let before = pose::translation(snapshot, context_id, &old, frame)?;
        let after = pose::translation(snapshot, context_id, &new, frame)?;
        let shift = [before[0] - after[0], before[1] - after[1]];
        if shift[0].abs() > 1e-9 || shift[1].abs() > 1e-9 {
            shift_position(
                snapshot,
                &mut candidate,
                context_id,
                layer_uid,
                frame,
                shift,
            )?;
        }
    }
    candidate.layers[index].parent_layer_uid = Presence::Present(parent.map(str::to_owned));
    Ok(LifecyclePlan {
        document: candidate,
        changed: true,
        selection: None,
        removed: Vec::new(),
        touched: vec![layer_uid.to_owned()],
        spared: None,
        created: None,
    })
}
