//! L01-G: the edit guard predicate (`layer_has_active_path_fx`) is the baseline `SMPathFx.hasAny`: an entry counts
//! unless it is explicitly disabled. The expectation is written out by hand over a grid of stacks; with the
//! evaluation layer the same grid must also agree with the kernel's `has_any`, so the guard and the evaluator can
//! never disagree about which layers are deformed.
use super::layer_has_active_path_fx;
use crate::object_document::{ObjectLayer, PathFxRecord, Presence};
use serde_json::{json, Value};

fn layer_with(stack: Option<&Value>) -> ObjectLayer {
    let mut layer = ObjectLayer::identified("L1".to_string());
    if let Some(stack) = stack {
        let entries: Vec<PathFxRecord> =
            serde_json::from_value(stack.clone()).expect("path effect entries");
        layer.path_fx = Presence::Present(entries);
    }
    layer
}

/// Every stack of up to two entries over five entry shapes, and the layer that never had a stack.
fn grid() -> Vec<(Option<Value>, bool)> {
    let entries = [
        (json!({"type": "zigzag"}), true),
        (json!({"type": "zigzag", "enabled": true}), true),
        (json!({"type": "zigzag", "enabled": false}), false),
        (json!({"type": "roughen", "enabled": false, "p1": 3}), false),
        (json!({"type": "roughen", "p1": null}), true),
    ];
    let mut stacks: Vec<(Option<Value>, bool)> = vec![(None, false), (Some(json!([])), false)];
    for (first, first_active) in &entries {
        stacks.push((Some(json!([first])), *first_active));
        for (second, second_active) in &entries {
            stacks.push((
                Some(json!([first, second])),
                *first_active || *second_active,
            ));
        }
    }
    stacks
}

#[test]
fn the_guard_counts_every_entry_that_is_not_explicitly_disabled() {
    let stacks = grid();
    assert_eq!(stacks.len(), 2 + 5 + 25);
    let mut active = 0;
    for (stack, expected) in &stacks {
        let layer = layer_with(stack.as_ref());
        assert_eq!(layer_has_active_path_fx(&layer), *expected, "{stack:?}");
        if *expected {
            active += 1;
        }
    }
    assert!(active > 0 && active < stacks.len());
}

#[cfg(feature = "evaluation")]
#[test]
fn the_guard_agrees_with_the_evaluation_kernel_on_the_whole_grid() {
    use crate::evaluation::object_vector_kernels::object_path_effects_kernel::{
        has_any, PathFxEntry,
    };
    for (stack, _) in grid() {
        let layer = layer_with(stack.as_ref());
        let kernel: Option<Vec<Option<PathFxEntry>>> = layer.path_fx().map(|stored| {
            stored
                .iter()
                .map(|entry| {
                    Some(PathFxEntry {
                        fx_type: entry.fx_type.clone(),
                        enabled: entry.enabled_flag(),
                        params: [entry.param(0), entry.param(1), entry.param(2)],
                    })
                })
                .collect()
        });
        assert_eq!(
            has_any(kernel.as_deref()),
            layer_has_active_path_fx(&layer),
            "{stack:?}"
        );
    }
}
