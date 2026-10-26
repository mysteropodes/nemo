//! Exercise the real first bake/admission, then fail the second bake with an unavailable tip.
use super::*;
use std::sync::Arc;

pub(crate) fn assert_second_bake_rollback(origin: &ObjectSnapshot) {
    let request = serde_json::from_value(serde_json::json!({
        "command":"object.tween.generate", "layerUid":"L0"
    }))
    .unwrap();
    let mut plan = crate::tween_kernels::plan_tween(origin.document(), &request, None)
        .unwrap()
        .unwrap();
    assert_eq!(plan.bakes.len(), 1);
    let mut second = plan.bakes[0].clone();
    second.fields.bitmap_brush_spec.tip = "unavailable-test-tip".into();
    plan.bakes.push(second);
    let before = origin.serialize_project_file().unwrap();
    let original = origin.project().unwrap().asset_inventory().unwrap();
    let mut created = vec![];
    let mut visits = 0;
    let result = prepare_with(origin, plan.document, plan.bakes, |index, project| {
        visits += 1;
        if index == 1 {
            for record in project.asset_inventory().unwrap().records() {
                if !original.records().iter().any(|r| r.id() == record.id()) {
                    created.push(Arc::downgrade(
                        record.source().encoded_allocation().unwrap(),
                    ));
                }
            }
        }
        Ok(())
    });
    assert!(
        matches!(result, Err(Error::Unavailable)),
        "refusal={:?}; visits={visits}",
        result.as_ref().err()
    );
    assert_eq!(visits, 2);
    assert!(
        !created.is_empty(),
        "first real bake admitted a new resource"
    );
    assert!(
        created
            .iter()
            .all(|allocation| allocation.upgrade().is_none()),
        "failed candidate retains no PNG allocations"
    );
    assert_eq!(origin.serialize_project_file().unwrap(), before);
}
