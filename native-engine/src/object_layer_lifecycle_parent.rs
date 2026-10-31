//! Parent A set / clear plan (#2372 U12): `setLayerParent` of `src/js/motion.js` at 1065e9d8.
//!
//! The baseline refuses a layer as its own parent and any cycle, then reassigns `parentLayerUid` and folds the
//! displacement of the child's pivot at the playhead into its static Position, so the world pose does not jump.
//! This kernel admits the same graph (existing parent, same context, no folder, no cycle) and keeps the pose at `frame`
//! by the one case it can prove exactly: every ancestor in the old and the new chain is a pure translation there, so the
//! displacement is the difference of the two translation sums whatever the pivots are. A rotated or scaled ancestor
//! needs its evaluated pivot, which the commands layer cannot reach: the read-queries facade
//! (`object_parent_pivot_read.rs`) answers it, and `object_layer_lifecycle_parent_keep.rs` solves the child's whole
//! Position, Rotation and Scale from it, exactly. An expression-driven ancestor, a keyed child Position, Rotation or
//! Scale that has to change, a layer without an evaluated pivot, a collapsed or skewing chain are typed refusals,
//! never a silent jump. The kernel owns no history or id state.
use crate::object_document::object_group_kernels::object_layer_lifecycle_plan::{
    index_of, LifecycleError, LifecyclePlan,
};
use crate::object_document::{ObjectDocument, Presence};
use crate::object_snapshot::object_parent_pivot_read::layer_pivot_bases;
use crate::object_snapshot::ObjectSnapshot;

#[path = "object_layer_lifecycle_parent_keep.rs"]
mod keep;

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
pub(crate) fn in_context(document: &ObjectDocument, context_id: &str, uids: [&str; 2]) -> bool {
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
    use crate::commands::object_fill_owner::layer_lifecycle::parent::keep::Pose;
    use crate::object_document::{ObjectDocument, Presence, PropertyHolderTarget};

    const MOTION: [&str; 4] = ["position", "anchor", "rotation", "scale"];

    fn target(context_id: &str, layer_uid: &str) -> PropertyHolderTarget {
        PropertyHolderTarget {
            context_id: context_id.to_owned(),
            layer_uid: layer_uid.to_owned(),
            stroke_id: Presence::Missing,
        }
    }

    /// Raw values of one Motion property at `frame` (the read-query facade; expressions are not evaluated).
    fn sample(
        snapshot: &ObjectSnapshot,
        target: &PropertyHolderTarget,
        key: &str,
        frame: u32,
        what: &'static str,
    ) -> Result<Vec<f64>, LifecycleError> {
        snapshot
            .sample_property_raw(target, key, f64::from(frame))
            .map_err(|_| LifecycleError::PoseUnavailable(what))
    }

    /// True when an enabled expression drives a Motion property of the target: raw tracks are then not its value.
    fn expression_driven(document: &ObjectDocument, target: &PropertyHolderTarget) -> bool {
        document.property_holders().is_some_and(|holders| {
            holders.iter().any(|h| {
                h.target() == target
                    && h.properties().iter().any(|p| {
                        MOTION.contains(&p.property())
                            && p.expression.value().is_some_and(|code| code.enabled)
                    })
            })
        })
    }

    /// Sum of the translations of `chain` at `frame`; `None` when a rotation or a scale makes the sum insufficient
    /// (the evaluated pivots are then needed); an expression or an unsampleable pose refuses.
    pub(super) fn translation(
        snapshot: &ObjectSnapshot,
        context_id: &str,
        chain: &[&str],
        frame: u32,
    ) -> Result<Option<[f64; 2]>, LifecycleError> {
        const NOT_SAMPLEABLE: &str = "a parent pose is not sampleable";
        let mut sum = [0.0, 0.0];
        let mut plain = true;
        for uid in chain {
            let t = target(context_id, uid);
            if expression_driven(snapshot.document(), &t) {
                return Err(LifecycleError::PoseUnavailable(
                    "an expression-driven parent cannot be kept in place",
                ));
            }
            let position = sample(snapshot, &t, "position", frame, NOT_SAMPLEABLE)?;
            let rotation = sample(snapshot, &t, "rotation", frame, NOT_SAMPLEABLE)?;
            let scale = sample(snapshot, &t, "scale", frame, NOT_SAMPLEABLE)?;
            let (px, py, rot, sx, sy) =
                match (position.as_slice(), rotation.as_slice(), scale.as_slice()) {
                    (&[px, py], &[rot], &[sx, sy]) => (px, py, rot, sx, sy),
                    _ => return Err(LifecycleError::PoseUnavailable(NOT_SAMPLEABLE)),
                };
            if [px, py, rot, sx, sy].iter().any(|v| !v.is_finite()) {
                return Err(LifecycleError::PoseUnavailable(NOT_SAMPLEABLE));
            }
            plain &= rot == 0.0 && sx == 100.0 && sy == 100.0;
            sum[0] += px;
            sum[1] += py;
        }
        Ok(plain.then_some(sum))
    }

    /// The full stored Motion of one layer at `frame` with its evaluated pivot base; an expression refuses.
    pub(super) fn full(
        snapshot: &ObjectSnapshot,
        context_id: &str,
        layer_uid: &str,
        frame: u32,
        base: [f64; 2],
    ) -> Result<Pose, LifecycleError> {
        const WHAT: &str = "a layer pose in the parent chain is not sampleable";
        let t = target(context_id, layer_uid);
        if expression_driven(snapshot.document(), &t) {
            return Err(LifecycleError::PoseUnavailable(
                "an expression-driven layer cannot be kept in place",
            ));
        }
        let err = || LifecycleError::PoseUnavailable(WHAT);
        let pair = |key: &str| -> Result<[f64; 2], LifecycleError> {
            sample(snapshot, &t, key, frame, WHAT)?
                .try_into()
                .map_err(|_| err())
        };
        let (position, anchor, scale) = (pair("position")?, pair("anchor")?, pair("scale")?);
        let rotation = match sample(snapshot, &t, "rotation", frame, WHAT)?[..] {
            [v] => v,
            _ => return Err(err()),
        };
        if ![position, anchor, scale]
            .iter()
            .flatten()
            .chain([&rotation])
            .all(|v| v.is_finite())
        {
            return Err(err());
        }
        Ok(Pose { position, anchor, rotation, scale, base })
    }

    pub(super) fn position(
        snapshot: &ObjectSnapshot,
        context_id: &str,
        layer_uid: &str,
        frame: u32,
    ) -> Result<[f64; 2], LifecycleError> {
        let what = "the child position is not sampleable";
        match sample(
            snapshot,
            &target(context_id, layer_uid),
            "position",
            frame,
            what,
        )?[..]
        {
            [x, y] => Ok([x, y]),
            _ => Err(LifecycleError::PoseUnavailable(what)),
        }
    }
}

/// True when `property` of the layer-level holder carries keys or an expression (one static value cannot absorb a change).
fn animated(snapshot: &ObjectSnapshot, context_id: &str, layer_uid: &str, property: &str) -> bool {
    snapshot
        .document()
        .property_holders()
        .is_some_and(|holders| {
            holders.iter().any(|h| {
                h.target().context_id() == context_id
                    && h.target().layer_uid() == layer_uid
                    && h.target().stroke_id().is_none()
                    && h.properties().iter().any(|p| {
                        p.property() == property
                            && (p.track.value().is_some_and(|t| !t.keys.is_empty())
                                || p.expression.value().is_some())
                    })
            })
        })
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
    if animated(snapshot, context_id, layer_uid, "position") {
        return Err(LifecycleError::PoseUnavailable(
            "an animated child Position cannot absorb the shift",
        ));
    }
    let now = pose::position(snapshot, context_id, layer_uid, frame)?;
    let value = vec![
        static_number(now[0] + shift[0])?,
        static_number(now[1] + shift[1])?,
    ];
    set_static_value(candidate, context_id, layer_uid, "position", value);
    Ok(())
}

/// A rotated or scaled ancestor: the evaluated pivots (read-queries facade) let the whole chain be solved, so the
/// child keeps every world point through Position, Rotation and Scale. A property that is keyed and has to change refuses.
fn keep_affine(
    snapshot: &ObjectSnapshot,
    candidate: &mut ObjectDocument,
    at: (&str, &str, u32),
    chains: [&[&str]; 2],
) -> Result<(), LifecycleError> {
    let (context_id, layer_uid, frame) = at;
    let mut uids: Vec<&str> = vec![layer_uid];
    for uid in chains.iter().flat_map(|c| c.iter()) {
        if !uids.contains(uid) {
            uids.push(uid);
        }
    }
    let bases = layer_pivot_bases(snapshot, context_id, &uids, frame)
        .map_err(LifecycleError::PoseUnavailable)?;
    let poses = uids
        .iter()
        .zip(bases)
        .map(|(uid, base)| pose::full(snapshot, context_id, uid, frame, base))
        .collect::<Result<Vec<_>, _>>()?;
    let chain = |uids_of: &[&str]| -> Vec<keep::Pose> {
        uids_of
            .iter()
            .filter_map(|u| uids.iter().position(|x| x == u).map(|i| poses[i]))
            .collect()
    };
    let child = poses[0];
    let kept = keep::keep_world_pose(&child, &chain(chains[0]), &chain(chains[1]))
        .map_err(|r| LifecycleError::PoseUnavailable(r.reason()))?;
    let changed = |was: &[f64], now: &[f64]| was.iter().zip(now).any(|(a, b)| (a - b).abs() > 1e-9);
    let writes = [
        ("position", &child.position[..], &kept.position[..]),
        ("rotation", &[child.rotation][..], &[kept.rotation][..]),
        ("scale", &child.scale[..], &kept.scale[..]),
    ];
    for (property, was, now) in writes {
        if !changed(was, now) {
            continue;
        }
        if animated(snapshot, context_id, layer_uid, property) {
            return Err(LifecycleError::PoseUnavailable(
                "an animated child Position, Rotation or Scale cannot absorb the shift",
            ));
        }
        let value = now
            .iter()
            .map(|v| static_number(*v))
            .collect::<Result<Vec<_>, _>>()?;
        set_static_value(candidate, context_id, layer_uid, property, value);
    }
    Ok(())
}

/// Whole values stay integers, like every stored value the editor wrote.
pub(crate) fn static_number(v: f64) -> Result<serde_json::Number, LifecycleError> {
    if v.fract() == 0.0 && v.abs() < 9e15 {
        return Ok(serde_json::Number::from(v as i64));
    }
    serde_json::Number::from_f64(v).ok_or(LifecycleError::PoseUnavailable("a value is not finite"))
}

/// Writes the static value of one layer-level property record, creating the holder and the record when absent.
pub(crate) fn set_static_value(
    candidate: &mut ObjectDocument,
    context_id: &str,
    layer_uid: &str,
    property: &str,
    value: Vec<serde_json::Number>,
) {
    use crate::object_document::{PropertyHolder, PropertyHolderTarget, PropertyRecord};
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
        .find(|p| p.property() == property)
    {
        Some(record) => record.static_value = Presence::Present(value),
        None => holder.properties.push(PropertyRecord {
            property: property.to_owned(),
            static_value: Presence::Present(value),
            track: Presence::Missing,
            expression: Presence::Missing,
        }),
    }
    candidate.property_holders = Presence::Present(holders);
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
        if let (Some(before), Some(after)) = (before, after) {
            let shift = [before[0] - after[0], before[1] - after[1]];
            if shift[0].abs() > 1e-9 || shift[1].abs() > 1e-9 {
                shift_position(snapshot, &mut candidate, context_id, layer_uid, frame, shift)?;
            }
        } else {
            keep_affine(snapshot, &mut candidate, (context_id, layer_uid, frame), [&old, &new])?;
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
