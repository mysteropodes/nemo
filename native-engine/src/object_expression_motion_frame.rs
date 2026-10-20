//! Original owner-held Motion -> duplicator frame; all guest values retire first.
//! Non-clean outcomes remain at the actual host and refuse publication. C02's
//! diagnostic/timeout-disable committer is still required; a getter never edits history.
use crate::evaluation::motion_parent_chain::motion_mat_from_values;
use crate::evaluation::object_component_kernels::duplicator_placement_modes::JsNumber;
use crate::evaluation::object_component_kernels::object_duplicator_evaluate::snapshot::SnapshotMotion;
use crate::evaluation::object_component_kernels::object_effector_layer::{MotionDelta, Sampled};
use crate::evaluation::object_component_kernels::object_effector_layers::{
    MotionPort, Unavailable,
};
use crate::history::NativeObjectHistory;
use crate::object_document::{Presence, PropertyHolderTarget};
use crate::object_expression_pin::{
    verify_owner_snapshot, PropertyDependencyEffect, PropertyEvaluation,
};
use crate::object_expression_runtime::property::evaluate_property_for_owner_with_effects;
use crate::object_expression_runtime::{ExpressionLimits, ExpressionRefusal};
use crate::object_frame_packet::{ObjectFrameError, ObjectFramePacket, ObjectFrameSelector};
use crate::revision::ObjectSnapshot;
use std::cell::RefCell;

/// Rust-only receipts. A success already owns its dependencies; a refused
/// finalization carries the dependencies separately, never twice.
#[derive(Debug)]
enum MotionEffect {
    Evaluated {
        target: PropertyHolderTarget,
        property: String,
        result: PropertyEvaluation,
    },
    Refused {
        target: PropertyHolderTarget,
        property: String,
        refusal: ExpressionRefusal,
        dependencies: Vec<PropertyDependencyEffect>,
    },
}

/// The real preview/export host retains this exact failure, not just its text.
/// Its immutable origin keeps stale effects separate from the current document.
#[derive(Debug)]
pub struct ObjectMotionFrameFailure {
    snapshot: ObjectSnapshot,
    context_id: String,
    frame: u32,
    cause: ExpressionRefusal,
    effects: Vec<MotionEffect>,
}
impl ObjectMotionFrameFailure {
    fn same_source(&self, snapshot: &ObjectSnapshot) -> bool {
        self.snapshot.instance_id() == snapshot.instance_id()
            && self.snapshot.document_id() == snapshot.document_id()
            && self.snapshot.content_revision() == snapshot.content_revision()
            && self.snapshot.snapshot_id() == snapshot.snapshot_id()
            && std::ptr::eq(self.snapshot.document(), snapshot.document())
    }
}
impl std::fmt::Display for ObjectMotionFrameFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "evaluated Motion frame refused: {:?}; {}/{}; effects={:?}",
            self.cause, self.context_id, self.frame, self.effects
        )
    }
}

struct EvaluatedMotion<'a> {
    owner: &'a NativeObjectHistory,
    snapshot: &'a ObjectSnapshot,
    context: &'a str,
    frame: f64,
    limits: ExpressionLimits,
    raw: SnapshotMotion<'a>,
    effects: RefCell<Vec<MotionEffect>>,
    refusal: RefCell<Option<ExpressionRefusal>>,
}
impl EvaluatedMotion<'_> {
    fn target(&self, uid: &str) -> PropertyHolderTarget {
        PropertyHolderTarget {
            context_id: self.context.to_owned(),
            layer_uid: uid.to_owned(),
            stroke_id: Presence::Missing,
        }
    }
    fn numbers(&self, uid: &str, key: &'static str) -> Result<Vec<f64>, Unavailable> {
        self.value(uid, key)?
            .ok_or(Unavailable::LayerMotion)?
            .into_iter()
            .map(|n| match n {
                JsNumber::Value(v) if v.is_finite() => Ok(v),
                _ => Err(Unavailable::LayerMotion),
            })
            .collect()
    }
}
impl MotionPort for EvaluatedMotion<'_> {
    fn value(&self, uid: &str, key: &'static str) -> Result<Sampled, Unavailable> {
        let target = self.target(uid);
        let enabled = self
            .snapshot
            .document()
            .property_holders()
            .unwrap_or(&[])
            .iter()
            .filter(|h| h.target() == &target)
            .flat_map(|h| h.properties())
            .any(|p| p.property() == key && p.expression.value().is_some_and(|e| e.enabled));
        if !enabled {
            return self.raw.value(uid, key);
        }
        let (result, dependencies) = evaluate_property_for_owner_with_effects(
            self.owner,
            self.snapshot,
            &target,
            key,
            self.frame,
            0,
            self.limits,
        );
        match result {
            Ok(result) => {
                if result.diagnostic.is_some()
                    || result.disable.is_some()
                    || !result.dependencies.is_empty()
                {
                    self.effects.borrow_mut().push(MotionEffect::Evaluated {
                        target,
                        property: key.to_owned(),
                        result,
                    });
                    return Err(Unavailable::Property(key));
                }
                Ok(Some(
                    result.value.into_iter().map(JsNumber::Value).collect(),
                ))
            }
            Err(refusal) => {
                self.refusal.borrow_mut().get_or_insert(refusal);
                self.effects.borrow_mut().push(MotionEffect::Refused {
                    target,
                    property: key.to_owned(),
                    refusal,
                    dependencies,
                });
                Err(Unavailable::Property(key))
            }
        }
    }
    fn motion_delta(&self, uid: &str) -> Result<Option<MotionDelta>, Unavailable> {
        // Same local computeMotionMat order as the original. No parent pose is
        // added here, and all five consumed fields use the evaluated provider.
        let read = |key| self.numbers(uid, key).map_err(|_| Unavailable::LayerMotion);
        let p = read("position")?;
        let a = read("anchor")?;
        let r = read("rotation")?;
        let s = read("scale")?;
        let o = read("opacity")?;
        let mat = match (
            p.as_slice(),
            a.as_slice(),
            r.as_slice(),
            s.as_slice(),
            o.as_slice(),
        ) {
            ([x, y], [ax, ay], [rot], [sx, sy], [op]) => {
                motion_mat_from_values([*x, *y], [*ax, *ay], *rot, [*sx, *sy], *op)
            }
            _ => return Err(Unavailable::LayerMotion),
        };
        // Shared original FollowPath hook is supplied by the sibling packet;
        // its path parameters/target pose retain their explicit strict checks.
        self.raw.motion_delta_from_local(uid, mat)
    }
}

fn verify(owner: &NativeObjectHistory, snapshot: &ObjectSnapshot) -> Result<(), ExpressionRefusal> {
    let current = owner
        .acquire_snapshot(owner.content_revision())
        .ok_or(ObjectFrameError::Unavailable)?;
    verify_owner_snapshot(owner, &current, snapshot).map_err(Into::into)
}

/// Used by BOTH actual owner-held preview and artifact export. The effect sink
/// is mandatory. Only clean values publish; a pending effect refuses every new
/// frame of that same source until the original C02 owner committer exists.
/// A later owner revision explicitly retires the obsolete receipt; no stale
/// disable is applied to the new document. This function never mutates history.
pub fn prepare_object_motion_exposure_frame_for_owner(
    owner: &NativeObjectHistory,
    snapshot: &ObjectSnapshot,
    selector: &ObjectFrameSelector<'_>,
    retained: &mut Option<ObjectMotionFrameFailure>,
) -> Result<ObjectFramePacket, String> {
    // Same already-admitted ORIGINAL39 per-evaluation envelope as property bake.
    let limits = ExpressionLimits::new(4096, 8_388_608, 262_144, 32)
        .map_err(|e| format!("Motion expression limits: {e:?}"))?;
    prepare_with_limits(owner, snapshot, selector, retained, limits)
}

fn prepare_with_limits(
    owner: &NativeObjectHistory,
    snapshot: &ObjectSnapshot,
    selector: &ObjectFrameSelector<'_>,
    retained: &mut Option<ObjectMotionFrameFailure>,
    limits: ExpressionLimits,
) -> Result<ObjectFramePacket, String> {
    verify(owner, snapshot).map_err(|e| format!("Motion owner pin: {e:?}"))?;
    if let Some(previous) = retained.as_ref() {
        if previous.same_source(snapshot) && !previous.effects.is_empty() {
            return Err(previous.to_string());
        }
    }
    // Only a verified owner/source can retire an obsolete receipt.
    *retained = None;
    let motion = EvaluatedMotion {
        owner,
        snapshot,
        context: selector.context_id,
        frame: f64::from(selector.frame),
        limits,
        raw: SnapshotMotion::new(snapshot, selector.context_id, f64::from(selector.frame)),
        effects: RefCell::default(),
        refusal: RefCell::default(),
    };
    let prepared =
        crate::object_frame_packet::duplicators::prepare_with_motion(snapshot, selector, &motion);
    let current = verify(owner, snapshot);
    let effects = motion.effects.into_inner();
    let refused = motion.refusal.into_inner();
    let cause = current
        .err()
        .or(refused)
        .or_else(|| prepared.as_ref().err().copied().map(Into::into));
    if let Some(cause) = cause {
        let failure = ObjectMotionFrameFailure {
            snapshot: snapshot.clone(),
            context_id: selector.context_id.to_owned(),
            frame: selector.frame,
            cause,
            effects,
        };
        let message = failure.to_string();
        *retained = Some(failure);
        return Err(message);
    }
    // A provider cannot publish a clean-looking packet while hiding an effect.
    if !effects.is_empty() {
        let failure = ObjectMotionFrameFailure {
            snapshot: snapshot.clone(),
            context_id: selector.context_id.to_owned(),
            frame: selector.frame,
            cause: ObjectFrameError::Unavailable.into(),
            effects,
        };
        let message = failure.to_string();
        *retained = Some(failure);
        return Err(message);
    }
    prepared.map_err(|e| format!("Motion frame: {e:?}"))
}

#[cfg(all(
    test,
    feature = "test-object_expression_runtime",
    feature = "compositor"
))]
#[path = "../tests/object_expression_motion_frame.rs"]
mod tests;
