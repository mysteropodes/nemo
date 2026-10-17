//! Oracle checks of the PSD import plan (L10-F): every baseline run of `importChildren` / `importFile`
//! (fixture rows generated from `fbe4b8a3`) must be reproduced by the production plan.
use super::rows::{parse, Case, Outcome, Toast};
use super::*;

pub(crate) fn plan_of(case: &Case) -> Result<PsdImportPlan, PsdPlanError> {
    plan_psd_import(
        &case.psd,
        &CanvasSize {
            width: case.canvas.0,
            height: case.canvas.1,
        },
    )
}

/// Skipped children as the layer rows alone say they must be (independent of production).
fn expected_skips(case: &Case) -> Vec<(usize, SkipReason)> {
    case.psd
        .children
        .iter()
        .enumerate()
        .filter_map(|(i, l)| {
            if !l.has_canvas {
                Some((i, SkipReason::NoCanvas { hidden: l.hidden }))
            } else if l.hidden {
                Some((i, SkipReason::Hidden))
            } else {
                None
            }
        })
        .collect()
}

fn skips_of(skipped: &[SkippedLayer]) -> Vec<(usize, SkipReason)> {
    skipped.iter().map(|s| (s.source_index, s.reason)).collect()
}

/// Compare a production result with the baseline run; the error names the first difference.
pub(crate) fn compare(
    case: &Case,
    result: &Result<PsdImportPlan, PsdPlanError>,
) -> Result<(), String> {
    let want_skips = expected_skips(case);
    match (&case.outcome, result) {
        (Outcome::Count(n), Ok(plan)) => {
            if plan.rows.len() != *n || plan.rows.len() != case.placed.len() {
                return Err(format!("count {} want {n}", plan.rows.len()));
            }
            let want_resize = case.resizes.last().map(|r| (r.0, r.1));
            let got_resize = plan.resize.map(|r| (r.width, r.height));
            if want_resize != got_resize || case.resizes.len() > 1 {
                return Err(format!("resize {got_resize:?} want {want_resize:?}"));
            }
            for (row, want) in plan.rows.iter().zip(&case.placed) {
                let same = row.source_index == want.source
                    && row.layer_name == want.name
                    && row.rect.center_x == want.x
                    && row.rect.center_y == want.y
                    && row.rect.width == want.w
                    && row.rect.height == want.h
                    && row.opacity == want.opacity
                    && row.visible == want.visible
                    && row.raster.key == format!("psd-layer:{}", want.source)
                    && row.raster.width == want.w
                    && row.raster.height == want.h
                    && row.raster.mime == "image/png";
                if !same {
                    return Err(format!(
                        "row {} differs: {row:?} want {want:?}",
                        want.source
                    ));
                }
            }
            for (m, row) in case.media.iter().zip(&plan.rows) {
                if m.0 != row.source_index || m.1 != row.layer_name {
                    return Err(format!("media entry {m:?} vs row {row:?}"));
                }
            }
            if case.media.len() != plan.rows.len() {
                return Err("media entry count".to_string());
            }
            if skips_of(&plan.skipped) != want_skips {
                return Err(format!("skipped {:?} want {want_skips:?}", plan.skipped));
            }
            if plan.child_count() != case.psd.children.len() {
                return Err("rows + skipped != children".to_string());
            }
            if plan.rows.len() + want_skips.len() != case.psd.children.len() {
                return Err("fixture rows disagree with the layer rows".to_string());
            }
            Ok(())
        }
        (Outcome::Throw(message), Err(err)) => {
            let (child_count, skipped) = match err {
                PsdPlanError::NoPixels {
                    child_count,
                    skipped,
                } => (child_count, skipped),
                other => return Err(format!("error {other:?}")),
            };
            if &err.message() != message {
                return Err(format!("message {:?} want {message:?}", err.message()));
            }
            if *child_count != case.psd.children.len() || skips_of(skipped) != want_skips {
                return Err("error report".to_string());
            }
            if want_skips.len() != case.psd.children.len() || !case.placed.is_empty() {
                return Err("fixture: a throw must follow all-skipped children".to_string());
            }
            Ok(())
        }
        (want, got) => Err(format!("outcome {got:?} want {want:?}")),
    }
}

fn is_guard(case: &Case) -> bool {
    case.id.starts_with("g_")
}

#[test]
fn the_fixture_has_the_pinned_shape() {
    let (cases, toasts) = parse();
    assert_eq!(cases.len(), 150);
    assert_eq!(toasts.len(), 20);
    assert_eq!(
        cases.iter().map(|c| c.psd.children.len()).sum::<usize>(),
        660
    );
    assert!(cases.iter().all(|c| c.undo == 1), "one pushUndo per import");
    assert_eq!(cases.iter().filter(|c| is_guard(c)).count(), 4);
    let throws = cases
        .iter()
        .filter(|c| matches!(c.outcome, Outcome::Throw(_)))
        .count();
    let resized = cases.iter().filter(|c| !c.resizes.is_empty()).count();
    let defaulted = cases
        .iter()
        .flat_map(|c| c.placed.iter())
        .filter(|p| p.name.starts_with("Calque PSD "))
        .count();
    let zero_opacity = cases
        .iter()
        .flat_map(|c| c.placed.iter())
        .filter(|p| p.opacity == 0.0)
        .count();
    assert_eq!(
        (throws, resized, defaulted, zero_opacity),
        (23, 43, 131, 48)
    );
}

#[test]
fn every_baseline_run_is_reproduced_by_the_plan() {
    let (cases, _) = parse();
    let mut checked = 0;
    for case in cases.iter().filter(|c| !is_guard(c)) {
        let got = plan_of(case);
        compare(case, &got).unwrap_or_else(|e| panic!("{}: {e}", case.id));
        checked += 1;
    }
    assert_eq!(checked, 146);
}

#[test]
fn a_baseline_run_that_resized_then_threw_publishes_nothing() {
    let (cases, _) = parse();
    let resized_throw: Vec<&Case> = cases
        .iter()
        .filter(|c| matches!(c.outcome, Outcome::Throw(_)) && !c.resizes.is_empty())
        .collect();
    let ids: Vec<&str> = resized_throw.iter().map(|c| c.id.as_str()).collect();
    for id in [
        "empty_children",
        "children_undefined",
        "only_groups",
        "only_hidden",
    ] {
        assert!(
            ids.contains(&id),
            "{id} resized before it threw in the baseline"
        );
    }
    for case in resized_throw {
        assert_eq!(case.resizes.len(), 1);
        // The plan has no resize to publish: the error is all there is.
        assert!(
            matches!(plan_of(case), Err(PsdPlanError::NoPixels { .. })),
            "{}",
            case.id
        );
    }
}

#[test]
fn a_non_finite_placement_is_refused_where_the_baseline_placed_it() {
    let (cases, _) = parse();
    let want = [
        ("g_left_infinity", 0, "left"),
        ("g_top_neg_infinity", 1, "top"),
        ("g_opacity_nan", 0, "opacity"),
        ("g_opacity_infinity", 0, "opacity"),
    ];
    for (id, index, field) in want {
        let case = cases.iter().find(|c| c.id == id).unwrap();
        let baseline = case.placed.iter().find(|p| p.source == index).unwrap();
        assert!(
            !(baseline.x.is_finite() && baseline.y.is_finite() && baseline.opacity.is_finite()),
            "{id}: the baseline put a non-finite number on the Raster"
        );
        assert_eq!(
            plan_of(case),
            Err(PsdPlanError::NonFinite {
                source_index: index,
                field
            }),
            "{id}"
        );
    }
}

#[test]
fn the_toast_texts_match_the_baseline_expressions() {
    let (_, toasts) = parse();
    let mut seen = (0, 0);
    for toast in &toasts {
        match toast {
            Toast::Success { count, suffix, out } => {
                assert_eq!(&success_toast(*count, suffix), out);
                seen.0 += 1;
            }
            Toast::Failure {
                prefix,
                message,
                out,
            } => {
                assert_eq!(&failure_toast(prefix, message.as_deref()), out);
                seen.1 += 1;
            }
        }
    }
    assert_eq!(seen, (8, 12));
}
