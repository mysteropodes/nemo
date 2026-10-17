//! Mutation controls for the SVG import plan (L10-D, G35).
//!
//! Each deliberate mistake is modelled here and run through the same flow comparison as the shipped
//! plan. It must be rejected by exactly as many flow rows as the equivalent source edit of the
//! baseline `importSVGString` changes in the JS oracle (the `# control` lines of `oracle.txt`).
//! The unmutated model must agree with production on every row, so a mutant can only be rejected
//! for its own defect.

use super::super::rows as geo;
use super::rows::{check_flows, production, FlowImpl};
use super::*;

#[derive(Clone, Copy)]
struct FlowMut {
    ignore_symbol: bool,
    ignore_locked: bool,
    no_fallback: bool,
    tight_join: bool,
    count_leaves: bool,
    drop_suffix: bool,
    layer_first: bool,
}

const BASE: FlowMut = FlowMut {
    ignore_symbol: false,
    ignore_locked: false,
    no_fallback: false,
    tight_join: false,
    count_leaves: false,
    drop_suffix: false,
    layer_first: false,
};

fn plan_mut(request: &SvgImportRequest, m: FlowMut) -> SvgImportOutcome {
    let mut req = request.clone();
    if let Some(layer) = req.layer.as_mut() {
        if m.ignore_symbol {
            layer.has_symbol = false;
        }
        if m.ignore_locked {
            layer.locked = false;
        }
    }
    let mut out = plan_svg_import(&req);
    if m.layer_first {
        if let (SvgImportOutcome::Empty { skipped }, Some(reason)) =
            (&out, layer_refusal(&req.layer))
        {
            return SvgImportOutcome::Refused {
                reason,
                skipped: skipped.clone(),
            };
        }
    }
    if m.no_fallback {
        if let SvgImportOutcome::Ready(plan) = &mut out {
            for s in plan.skipped.iter_mut() {
                if s.class_name == UNNAMED_CLASS {
                    s.class_name.clear();
                }
            }
        }
    }
    out
}

fn render_mut(
    outcome: &SvgImportOutcome,
    lookup: &dyn Fn(&str) -> String,
    m: FlowMut,
) -> Vec<String> {
    let plan = match outcome {
        SvgImportOutcome::Ready(plan) if m.tight_join || m.count_leaves || m.drop_suffix => plan,
        _ => {
            return outcome
                .toast()
                .map(|t| t.message(lookup))
                .into_iter()
                .collect()
        }
    };
    let count = if m.count_leaves {
        plan.leaves.len()
    } else {
        plan.paths.len()
    };
    let mut msg = lookup("toastSvgImported").replacen("{n}", &count.to_string(), 1);
    if !plan.skipped.is_empty() && !m.drop_suffix {
        msg.push_str(SKIPPED_SEPARATOR);
        msg.push_str(&lookup("toastSvgSkipped").replacen(
            "{n}",
            &plan.skipped.len().to_string(),
            1,
        ));
        let names: Vec<String> = plan.skipped.iter().map(|s| s.class_name.clone()).collect();
        msg.push_str(" (");
        msg.push_str(&names.join(if m.tight_join { "," } else { ", " }));
        msg.push(')');
    }
    vec![msg]
}

fn rejected(m: FlowMut) -> usize {
    let plan = |r: &SvgImportRequest| plan_mut(r, m);
    let render = |o: &SvgImportOutcome, l: &dyn Fn(&str) -> String| render_mut(o, l, m);
    let count = |o: &SvgImportOutcome| o.imported_count();
    let imp = FlowImpl {
        plan: &plan,
        render: &render,
        count: &count,
    };
    check_flows(&imp).1
}

/// The mutant must be rejected by as many flow rows as the JS source edit named `name`.
fn assert_matches_js(name: &str, m: FlowMut) -> usize {
    let got = rejected(m);
    assert_eq!(
        got,
        geo::control_count(name, "F"),
        "{}: rejected flow rows",
        name
    );
    assert!(got > 0, "{} must not be vacuous", name);
    got
}

#[test]
fn the_unmutated_model_agrees_with_production_on_every_flow() {
    assert_eq!(rejected(BASE), 0);
    assert_eq!(check_flows(&production()).1, 0);
}

#[test]
fn a_layer_check_that_ignores_symbol_layers_is_rejected() {
    assert_matches_js(
        "layer-ignores-symbol",
        FlowMut {
            ignore_symbol: true,
            ..BASE
        },
    );
}

#[test]
fn a_layer_check_that_ignores_locked_layers_is_rejected() {
    assert_matches_js(
        "layer-ignores-locked",
        FlowMut {
            ignore_locked: true,
            ..BASE
        },
    );
}

#[test]
fn skipped_names_without_the_inconnu_fallback_are_rejected() {
    assert_matches_js(
        "skipped-without-fallback",
        FlowMut {
            no_fallback: true,
            ..BASE
        },
    );
}

#[test]
fn a_skipped_list_joined_without_the_space_is_rejected() {
    assert_matches_js(
        "skipped-joined-tight",
        FlowMut {
            tight_join: true,
            ..BASE
        },
    );
}

#[test]
fn a_message_without_the_skipped_suffix_is_rejected() {
    let dropped = assert_matches_js(
        "skipped-suffix-dropped",
        FlowMut {
            drop_suffix: true,
            ..BASE
        },
    );
    assert_eq!(dropped, 12, "every flow that skipped something");
}

#[test]
fn counting_leaves_instead_of_inserted_paths_is_rejected() {
    assert_matches_js(
        "count-leaves-not-paths",
        FlowMut {
            count_leaves: true,
            ..BASE
        },
    );
}

#[test]
fn checking_the_layer_before_emptiness_is_rejected() {
    // No JS counterpart: moving the layer check ahead of the emptiness check is a block move, not a
    // one-line source edit. The typed order is pinned by exactly the two flows whose SVG is empty
    // while the layer is locked or missing.
    assert_eq!(
        rejected(FlowMut {
            layer_first: true,
            ..BASE
        }),
        2
    );
}
