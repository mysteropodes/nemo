//! L01-E consumer join (evaluation harness only): the stored layer stack is read by the L01-A path-effect
//! kernel. Entries built from the native record must give the kernel's `active_stack` the same activity mask,
//! and `param_value` the same defaults, that the baseline `activeStack` / `paramValue` gave in the oracle rows.
use super::data::rows;
use super::{decode, doc};
use crate::evaluation::object_vector_kernels::object_path_effects_kernel::{
    active_stack, has_any, param_value, PathFxEntry as KernelEntry, PATH_FX_TYPES,
};
use crate::object_document::{PathFxRecord, PATH_FX_TYPE_IDS};
use serde_json::json;

fn kernel_entry(record: &PathFxRecord) -> KernelEntry {
    KernelEntry {
        fx_type: record.fx_type.clone(),
        enabled: record.enabled_flag(),
        params: [record.param(0), record.param(1), record.param(2)],
    }
}

#[test]
fn kernel_active_stack_matches_the_baseline_mask() {
    let (_, _, fxs) = rows();
    let mut checked = 0;
    for fx in &fxs {
        let Some(native) = &fx.native else { continue };
        let document = decode(&doc(json!([native]), vec![])).unwrap();
        let stack = document.layers()[0].path_fx().expect("pathFx");
        let entries: Vec<Option<KernelEntry>> =
            stack.iter().map(|r| Some(kernel_entry(r))).collect();
        let active = active_stack(Some(entries.as_slice()));
        let positions: Vec<usize> = active
            .iter()
            .flatten()
            .map(|hit| {
                entries
                    .iter()
                    .position(|e| std::ptr::eq(e.as_ref().unwrap(), *hit))
                    .expect("active entry comes from the stack")
            })
            .collect();
        let want: Vec<usize> = fx
            .mask
            .iter()
            .enumerate()
            .filter(|(_, on)| **on)
            .map(|(i, _)| i)
            .collect();
        assert_eq!(positions, want, "{}", fx.id);
        assert_eq!(has_any(Some(entries.as_slice())), fx.has_any, "{}", fx.id);
        checked += 1;
    }
    assert_eq!(checked, 8);
}

#[test]
fn kernel_param_values_match_the_baseline_param_value() {
    let (_, _, fxs) = rows();
    for fx in &fxs {
        let Some(native) = &fx.native else { continue };
        let document = decode(&doc(json!([native]), vec![])).unwrap();
        let stack = document.layers()[0].path_fx().expect("pathFx");
        for (i, record) in stack.iter().enumerate() {
            let entry = kernel_entry(record);
            for k in 0..3 {
                let want = fx.triples[i][k].as_f64().unwrap();
                assert_eq!(param_value(&entry, k), want, "{}[{i}].p{}", fx.id, k + 1);
            }
        }
    }
}

#[test]
fn record_type_table_equals_the_kernel_table_and_unknown_types_would_be_dropped_silently() {
    let kernel_ids: Vec<&str> = PATH_FX_TYPES.iter().map(|t| t.id).collect();
    assert_eq!(kernel_ids, PATH_FX_TYPE_IDS);
    // the kernel (like the baseline) drops an unknown type without a trace; the record admission refuses it
    let ghost = KernelEntry {
        fx_type: "wiggle".into(),
        enabled: None,
        params: [Some(1.0), None, None],
    };
    let known = KernelEntry {
        fx_type: "zigzag".into(),
        enabled: Some(true),
        params: [None, None, None],
    };
    let entries = [Some(ghost), Some(known)];
    let active = active_stack(Some(entries.as_slice())).expect("one entry survives");
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].fx_type, "zigzag");
}
