//! Controls for the temporal offset and the copy loop: every deviation fed
//! through the real kernel must be caught by the oracle scenarios.
//!
//! A control changes ONE thing, either a [`TemporalRules`] switch the real
//! loop honors or one input of a scenario, and requires that named scenarios
//! then mismatch; the unchanged drive is the baseline test of
//! `duplicator_placement_temporal.rs`.

use super::test_codec::{mismatch, run, scenarios, Scenario};
use super::tests::ORACLE;
use super::{TemporalRules, BASELINE_RULES};
use crate::evaluation::object_component_kernels::duplicator_placement_modes::JsNumber;

/// Names of the scenarios the edited scenario or the rules no longer reproduce.
fn failing(rules: TemporalRules, edit: &dyn Fn(&mut Scenario)) -> Vec<String> {
    scenarios(ORACLE)
        .into_iter()
        .filter_map(|mut sc| {
            let original = sc.clone();
            edit(&mut sc);
            // the expectation is the baseline's, the input is the edited one
            let ran = run(&sc, rules);
            mismatch(&original, &ran).map(|_| original.name)
        })
        .collect()
}

fn has(names: &[String], name: &str) -> bool {
    names.iter().any(|n| n == name)
}

#[test]
fn the_undeviated_drive_matches_every_scenario() {
    assert!(failing(BASELINE_RULES, &|_| {}).is_empty());
}

#[test]
fn a_shifted_index_that_is_not_wrapped_into_the_span_is_detected() {
    let rules = TemporalRules {
        wrap_into_span: false,
        ..BASELINE_RULES
    };
    let names = failing(rules, &|_| {});
    for expected in [
        "wraps_below_the_span_start",
        "wraps_above_the_span_end",
        "frame_before_the_span",
        "span_length_zero_is_one",
        "span_length_fractional",
    ] {
        assert!(has(&names, expected), "{expected} in {names:?}");
    }
    assert_eq!(names.len(), 9, "{names:?}");
    // a scenario without a temporal object is unaffected
    assert!(!has(&names, "no_temporal_object"));
}

#[test]
fn a_pivot_that_ignores_the_shifted_content_is_detected() {
    let rules = TemporalRules {
        recompute_pivot: false,
        ..BASELINE_RULES
    };
    let names = failing(rules, &|_| {});
    for expected in [
        "pivot_follows_the_shifted_frame",
        "forward_in_span",
        "center_out",
    ] {
        assert!(has(&names, expected), "{expected} in {names:?}");
    }
    assert_eq!(names.len(), 28, "{names:?}");
}

#[test]
fn a_swapped_direction_is_detected() {
    let names = failing(BASELINE_RULES, &|sc| {
        if let Some((_, _, dir)) = sc.temporal.as_mut() {
            *dir = match dir.as_deref() {
                Some("backward") => Some("forward".to_string()),
                _ => Some("backward".to_string()),
            };
        }
    });
    for expected in ["forward_in_span", "backward_in_span", "center_out"] {
        assert!(has(&names, expected), "{expected} in {names:?}");
    }
    assert_eq!(names.len(), 27, "{names:?}");
}

#[test]
fn an_offset_truncated_to_an_integer_is_detected() {
    let names = failing(BASELINE_RULES, &|sc| {
        if let Some((_, off, _)) = sc.temporal.as_mut() {
            *off = off.trunc();
        }
    });
    assert!(has(&names, "center_out_half_steps"), "{names:?}");
    assert!(has(&names, "offset_one_and_a_half_is_on"), "{names:?}");
    assert_eq!(names.len(), 3, "{names:?}");
}

#[test]
fn a_random_stream_seeded_differently_is_detected() {
    let names = failing(BASELINE_RULES, &|sc| {
        sc.seed = match sc.seed {
            JsNumber::Value(v) => JsNumber::Value(v + 1.0),
            other => other,
        };
    });
    assert!(has(&names, "random_seed_42"), "{names:?}");
    // every copy carries `dup<seed>_<k>`, so any seed change is visible to every scenario
    assert!(has(&names, "forward_in_span"), "{names:?}");
    // only the scenario without a seed value and the one with count 1 build no id from it
    assert_eq!(names.len(), 36, "{names:?}");
}

#[test]
fn a_span_that_is_one_frame_too_long_or_too_early_is_detected() {
    let longer = failing(BASELINE_RULES, &|sc| sc.span.len += 1.0);
    assert!(has(&longer, "wraps_below_the_span_start"), "{longer:?}");
    let later = failing(BASELINE_RULES, &|sc| sc.span.start += 1.0);
    assert!(has(&later, "span_with_nonzero_start"), "{later:?}");
}

#[test]
fn a_count_one_too_high_or_a_missing_resample_port_is_detected() {
    let more = failing(BASELINE_RULES, &|sc| sc.count += 1.0);
    assert!(has(&more, "forward_in_span"), "{more:?}");
    assert_eq!(
        more.len(),
        38,
        "a different clone count changes every scenario"
    );
    let no_port = failing(BASELINE_RULES, &|sc| sc.can_resample = false);
    assert!(has(&no_port, "forward_in_span"), "{no_port:?}");
    assert!(has(&no_port, "hold_blank_falls_back"), "{no_port:?}");
    // a scenario that never used the port is unaffected
    assert!(!has(&no_port, "no_resample_port_is_off"));
    assert_eq!(no_port.len(), 29, "{no_port:?}");
}

#[test]
fn a_frame_off_by_one_is_detected() {
    let names = failing(BASELINE_RULES, &|sc| sc.frame += 1.0);
    assert!(has(&names, "forward_in_span"), "{names:?}");
    assert!(has(&names, "span_with_nonzero_start"), "{names:?}");
}
