//! Hand-written literal controls for the lock / transform-parent policy port
//! (L02-E). The kernel asks the port only about layers that carry lock or parent
//! facts, never fabricates a decision, and forwards a refusal with its operation
//! and layer. The codec admits Boolean lock metadata; the policy port still
//! decides edits to locked layers. Transform-parent metadata remains unavailable,
//! so a permissive provider cannot retain a parented layer in an admitted plan.

use super::fixture::{admitted, build, layer_uids, plain, text_of};
use super::{
    plan_delete, plan_duplicate, plan_recolor, plan_rename, plan_reorder, LayerEditPolicy, LayerOp,
    LayerSelection, LifecycleError, PolicyRefusal, UnavailableLayerEditPolicy as Nobody,
};
use crate::object_document::{ObjectDocument, ObjectLayer, Presence};
use std::cell::RefCell;

struct Allow;
impl LayerEditPolicy for Allow {
    fn permit(&self, _op: LayerOp, _layer: &ObjectLayer) -> Result<(), PolicyRefusal> {
        Ok(())
    }
}
struct Deny;
impl LayerEditPolicy for Deny {
    fn permit(&self, _op: LayerOp, _layer: &ObjectLayer) -> Result<(), PolicyRefusal> {
        Err(PolicyRefusal::Denied)
    }
}
/// Allows everything and records each decision it was asked for.
#[derive(Default)]
struct Log(RefCell<Vec<(LayerOp, String)>>);
impl LayerEditPolicy for Log {
    fn permit(&self, op: LayerOp, layer: &ObjectLayer) -> Result<(), PolicyRefusal> {
        self.0.borrow_mut().push((op, layer.layer_uid.clone()));
        Ok(())
    }
}

fn pick(active: &str, selected: &[&str]) -> LayerSelection {
    LayerSelection {
        active: active.to_string(),
        selected: selected.iter().map(|s| s.to_string()).collect(),
        anchor: None,
    }
}

fn locked(count: usize, index: usize) -> ObjectDocument {
    let mut document = build(&plain(count), 2);
    document.layers[index].locked = Presence::Present(true);
    document
}

#[test]
fn a_locked_layer_needs_a_provider_and_ordinary_layers_do_not() {
    let document = locked(3, 1);
    let bytes = text_of(&document);
    assert!(admitted(&document), "Boolean lock metadata is admitted");
    let refused = |op: LayerOp, got: Result<super::LifecyclePlan, LifecycleError>, refusal| {
        let want = LifecycleError::Policy {
            op,
            layer_uid: "L1".to_string(),
            refusal,
        };
        assert_eq!(got.expect_err("refusal"), want);
    };
    let providers: [(&dyn LayerEditPolicy, PolicyRefusal); 2] = [
        (&Nobody, PolicyRefusal::Unavailable),
        (&Deny, PolicyRefusal::Denied),
    ];
    for (policy, refusal) in providers {
        let me = pick("L1", &[]);
        refused(
            LayerOp::Rename,
            plan_rename(&document, "L1", "q", policy),
            refusal,
        );
        refused(
            LayerOp::Recolor,
            plan_recolor(&document, "L1", &[], "#112233", policy),
            refusal,
        );
        refused(
            LayerOp::Duplicate,
            plan_duplicate(&document, "L1", 4, policy),
            refusal,
        );
        refused(
            LayerOp::Delete,
            plan_delete(&document, &me, policy),
            refusal,
        );
        refused(
            LayerOp::Reorder,
            plan_reorder(&document, &[1], 3.0, &me, policy),
            refusal,
        );
    }
    assert_eq!(text_of(&document), bytes);
    let log = Log::default();
    let free = plan_reorder(&document, &[0], 2.0, &pick("L0", &[]), &log).expect("reorder");
    assert_eq!(layer_uids(&free.document), ["L1", "L0", "L2"]);
    assert!(
        log.0.borrow().is_empty(),
        "moving an unlocked layer asks nobody"
    );
    assert!(
        admitted(&free.document),
        "Boolean lock metadata is admitted"
    );
    assert_eq!(free.document.layers[0].locked, Presence::Present(true));
    assert_eq!(free.document.layers[0].shy, Presence::Missing);
    let noop = plan_rename(&document, "L1", "L1", &log).expect("rename");
    assert!(
        !noop.changed && log.0.borrow().is_empty(),
        "a no-op needs no permission"
    );
    assert_eq!(text_of(&noop.document), bytes);
    let removed = plan_delete(&document, &pick("L1", &[]), &log).expect("delete with a provider");
    assert_eq!(layer_uids(&removed.document), ["L0", "L2"]);
    assert_eq!(*log.0.borrow(), [(LayerOp::Delete, "L1".to_string())]);
    assert!(admitted(&removed.document), "no locked layer is left");
    let kept = plan_rename(&document, "L1", "q", &Allow).expect("rename with a provider");
    assert!(
        kept.changed && admitted(&kept.document),
        "Boolean lock metadata is admitted after a permitted edit"
    );
    assert_eq!(kept.document.layers[1].locked, Presence::Present(true));
    assert_eq!(kept.document.layers[1].shy, Presence::Missing);
    assert_eq!(text_of(&document), bytes);
}

#[test]
fn deleting_a_transform_parent_asks_about_each_orphaned_child() {
    let mut document = build(&plain(3), 2);
    document.layers[2].parent_layer_uid = Presence::Present(Some("L0".to_string()));
    let bytes = text_of(&document);
    let only_parent = pick("L0", &[]);
    let got = plan_delete(&document, &only_parent, &Nobody).expect_err("refusal");
    let want = LifecycleError::Policy {
        op: LayerOp::DetachFromDeletedParent,
        layer_uid: "L2".to_string(),
        refusal: PolicyRefusal::Unavailable,
    };
    assert_eq!(got, want);
    let log = Log::default();
    let stale = plan_delete(&document, &only_parent, &log).expect("delete with a provider");
    assert!(
        !admitted(&stale.document),
        "the codec still refuses the dangling parent"
    );
    assert_eq!(
        *log.0.borrow(),
        [(LayerOp::DetachFromDeletedParent, "L2".to_string())]
    );
    let log = Log::default();
    let both = plan_delete(&document, &pick("L0", &["L0", "L2"]), &log).expect("child goes too");
    assert_eq!(layer_uids(&both.document), ["L1"]);
    assert!(admitted(&both.document));
    assert_eq!(*log.0.borrow(), [(LayerOp::Delete, "L2".to_string())]);
    assert_eq!(text_of(&document), bytes);
}
