//! L02-F: the kernel-refusal to request-code table, and the lock / transform
//! parent facts the stand-in policy cannot decide (Root F06 owns that decision).
use super::*;
use crate::object_document::object_group_kernels::object_layer_lifecycle_plan::{
    plan_delete, plan_duplicate, plan_rename, plan_reorder, LayerOp, LayerSelection, PolicyRefusal,
};
use serde_json::json;

#[test]
fn every_kernel_refusal_maps_to_one_stable_request_code() {
    let policy = |op, refusal| LifecycleError::Policy {
        op,
        layer_uid: "l".to_string(),
        refusal,
    };
    let table = [
        (
            LifecycleError::UnknownLayer("x".into()),
            DispatchErrorCode::NotFound,
        ),
        (LifecycleError::LastLayer, DispatchErrorCode::InvalidRequest),
        (
            LifecycleError::NonFiniteGap,
            DispatchErrorCode::InvalidRequest,
        ),
        (
            LifecycleError::InvalidColor("zz".into()),
            DispatchErrorCode::InvalidRequest,
        ),
        (
            LifecycleError::SourceUnnamed("x".into()),
            DispatchErrorCode::InvalidRequest,
        ),
        (
            LifecycleError::OrderPlanIncomplete {
                context_id: "c".into(),
                layer_uid: "l".into(),
            },
            DispatchErrorCode::Internal,
        ),
        (
            LifecycleError::RevisionOutOfRange,
            DispatchErrorCode::Unavailable,
        ),
        (
            LifecycleError::IdUnavailable,
            DispatchErrorCode::Unavailable,
        ),
        (
            LifecycleError::TooManyLayers,
            DispatchErrorCode::Unavailable,
        ),
        (
            policy(LayerOp::Delete, PolicyRefusal::Unavailable),
            DispatchErrorCode::Unavailable,
        ),
        (
            policy(LayerOp::Reorder, PolicyRefusal::Denied),
            DispatchErrorCode::Unavailable,
        ),
        (
            policy(LayerOp::DetachFromDeletedParent, PolicyRefusal::Unavailable),
            DispatchErrorCode::Unavailable,
        ),
    ];
    for (error, code) in table {
        assert_eq!(refusal_code(&error), code, "{error:?}");
    }
}

fn document(locked: bool) -> ObjectDocument {
    serde_json::from_value(
        json!({"format":"nemo.native-object-document","formatVersion":1,
        "totalFrames":1,"objects":[],
        "layers":[{"layerUid":"A","name":"a"},{"layerUid":"B","name":"b","locked":locked}]}),
    )
    .unwrap()
}

#[test]
fn a_locked_layer_is_refused_as_unavailable_until_a_provider_decides() {
    // Since #2147 (62c8c83b, the layer-policy owner) storage admits a locked layer, but the lifecycle
    // kernels still ask no provider about it: planning over the locked document and sealing the plan
    // to an origin must turn the typed policy refusal into Unavailable and keep the candidate away
    // from every owner.
    let origin = ObjectSnapshot::new("lock-origin", document(false)).unwrap();
    assert!(ObjectSnapshot::new("lock-admitted", document(true)).is_ok());
    let locked = document(true);
    let policy = UnavailableLayerEditPolicy;
    let pick = |active: &str| LayerSelection {
        active: active.to_string(),
        selected: vec![],
        anchor: None,
    };
    let plans = [
        (
            LayerLifecycleKind::Delete,
            plan_delete(&locked, &pick("B"), &policy),
        ),
        (
            LayerLifecycleKind::Duplicate,
            plan_duplicate(&locked, "B", 1, &policy),
        ),
        (
            LayerLifecycleKind::Rename,
            plan_rename(&locked, "B", "renamed", &policy),
        ),
        (
            LayerLifecycleKind::Reorder,
            plan_reorder(&locked, &[1], 0.0, &pick("A"), &policy),
        ),
    ];
    for (kind, plan) in plans {
        assert!(plan.is_err(), "{kind:?}");
        let sealed = PreparedLayerLifecycle::seal(origin.clone(), kind, plan);
        assert_eq!(
            sealed.unwrap_err(),
            DispatchErrorCode::Unavailable,
            "{kind:?}"
        );
    }
    // The same operations on the unlocked layer are not asked and succeed.
    let free = document(false);
    let plan = plan_rename(&free, "B", "renamed", &policy);
    assert!(PreparedLayerLifecycle::seal(origin, LayerLifecycleKind::Rename, plan).is_ok());
}
