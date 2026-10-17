//! Mutation and negative controls of the PSD import plan (L10-F). An independent model of each rule is
//! written here; the unmutated model must agree with the baseline oracle on every non-guard case and
//! every mutation must disagree somewhere, so the oracle rows are known to discriminate. A second
//! group tampers with the expected side and requires the production comparison to notice.
use super::oracle::{compare, plan_of};
use super::rows::{parse, Case, Outcome, Placed};
use super::*;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Mutation {
    Reference,
    ImportHidden,
    ImportGroups,
    NameFromSourceIndex,
    KeepEmptyName,
    CentreIsTopLeft,
    ZeroOpacityBecomesOne,
    NanOffsetKept,
    ResizeIgnoresHeight,
    ResizeOnZeroSize,
    ResizeAlways,
    EmptyIsNotAnError,
    SwapWidthHeight,
}

const MUTANTS: [Mutation; 12] = [
    Mutation::ImportHidden,
    Mutation::ImportGroups,
    Mutation::NameFromSourceIndex,
    Mutation::KeepEmptyName,
    Mutation::CentreIsTopLeft,
    Mutation::ZeroOpacityBecomesOne,
    Mutation::NanOffsetKept,
    Mutation::ResizeIgnoresHeight,
    Mutation::ResizeOnZeroSize,
    Mutation::ResizeAlways,
    Mutation::EmptyIsNotAnError,
    Mutation::SwapWidthHeight,
];

/// (source index, name, centre x, centre y, width, height, opacity)
type ModelRow = (usize, String, f64, f64, u32, u32, f64);

#[derive(Debug, PartialEq)]
struct ModelOut {
    resize: Option<(u32, u32)>,
    rows: Vec<ModelRow>,
    error: bool,
}

fn falsy_zero(v: Option<f64>, keep_nan: bool) -> f64 {
    match v {
        Some(x) if x.is_nan() && keep_nan => x,
        Some(x) if x != 0.0 && !x.is_nan() => x,
        _ => 0.0,
    }
}

fn model(case: &Case, m: Mutation) -> ModelOut {
    let mut rows: Vec<ModelRow> = Vec::new();
    for (i, l) in case.psd.children.iter().enumerate() {
        let skip_group = !l.has_canvas && m != Mutation::ImportGroups;
        let skip_hidden = l.hidden && m != Mutation::ImportHidden;
        if skip_group || skip_hidden {
            continue;
        }
        let counter = if m == Mutation::NameFromSourceIndex {
            i
        } else {
            rows.len()
        };
        let name = match &l.name {
            Some(n) if !n.is_empty() || m == Mutation::KeepEmptyName => n.clone(),
            _ => format!("Calque PSD {}", counter + 1),
        };
        let keep_nan = m == Mutation::NanOffsetKept;
        let left = falsy_zero(l.left, keep_nan);
        let top = falsy_zero(l.top, keep_nan);
        let (w, h) = if m == Mutation::SwapWidthHeight {
            (l.height, l.width)
        } else {
            (l.width, l.height)
        };
        let (cx, cy) = if m == Mutation::CentreIsTopLeft {
            (left, top)
        } else {
            (left + f64::from(w) / 2.0, top + f64::from(h) / 2.0)
        };
        let opacity = match l.opacity {
            None => 1.0,
            Some(o) if o == 0.0 && m == Mutation::ZeroOpacityBecomesOne => 1.0,
            Some(o) => o,
        };
        rows.push((i, name, cx, cy, w, h, opacity));
    }
    let (pw, ph) = (case.psd.width, case.psd.height);
    let truthy = |v: Option<u32>| v.filter(|x| *x != 0 || m == Mutation::ResizeOnZeroSize);
    let resize = match (truthy(pw), truthy(ph)) {
        (Some(w), Some(h)) => {
            let wide = f64::from(w) != case.canvas.0;
            let high = f64::from(h) != case.canvas.1;
            let differs = if m == Mutation::ResizeIgnoresHeight {
                wide
            } else {
                wide || high
            };
            if differs || m == Mutation::ResizeAlways {
                Some((w, h))
            } else {
                None
            }
        }
        _ => None,
    };
    let error = rows.is_empty() && m != Mutation::EmptyIsNotAnError;
    ModelOut {
        resize: if error { None } else { resize },
        rows,
        error,
    }
}

/// What the baseline run says, in the model's shape (a throw leaves no resize in the plan).
fn expected(case: &Case) -> ModelOut {
    ModelOut {
        resize: match case.outcome {
            Outcome::Throw(_) => None,
            _ => case.resizes.last().copied(),
        },
        rows: case
            .placed
            .iter()
            .map(|p| (p.source, p.name.clone(), p.x, p.y, p.w, p.h, p.opacity))
            .collect(),
        error: matches!(case.outcome, Outcome::Throw(_)),
    }
}

fn generic_cases() -> Vec<Case> {
    parse()
        .0
        .into_iter()
        .filter(|c| !c.id.starts_with("g_"))
        .collect()
}

#[test]
fn the_unmutated_model_agrees_with_the_baseline_oracle() {
    for case in generic_cases() {
        assert_eq!(
            model(&case, Mutation::Reference),
            expected(&case),
            "{}",
            case.id
        );
    }
}

#[test]
fn every_mutation_disagrees_with_the_oracle_somewhere() {
    let cases = generic_cases();
    for m in MUTANTS {
        let first = cases.iter().find(|c| model(c, m) != expected(c));
        assert!(first.is_some(), "{m:?} is not detected by any oracle case");
    }
}

#[test]
fn the_hand_written_cases_alone_catch_every_mutation() {
    let cases = generic_cases();
    for m in MUTANTS {
        let caught = cases
            .iter()
            .filter(|c| !c.id.starts_with("rnd"))
            .any(|c| model(c, m) != expected(c));
        assert!(caught, "{m:?} slips past the hand-written cases");
    }
}

#[test]
fn production_agrees_with_the_reference_model_on_the_rows_it_plans() {
    for case in generic_cases() {
        let want = model(&case, Mutation::Reference);
        match plan_of(&case) {
            Ok(plan) => {
                assert!(!want.error, "{}", case.id);
                let rows: Vec<ModelRow> = plan
                    .rows
                    .iter()
                    .map(|r| {
                        (
                            r.source_index,
                            r.layer_name.clone(),
                            r.rect.center_x,
                            r.rect.center_y,
                            r.rect.width,
                            r.rect.height,
                            r.opacity,
                        )
                    })
                    .collect();
                assert_eq!(rows, want.rows, "{}", case.id);
                assert_eq!(
                    plan.resize.map(|r| (r.width, r.height)),
                    want.resize,
                    "{}",
                    case.id
                );
            }
            Err(_) => assert!(want.error, "{}", case.id),
        }
    }
}

fn first_with_rows(cases: &[Case]) -> usize {
    cases
        .iter()
        .position(|c| !c.placed.is_empty() && c.resizes.is_empty())
        .unwrap()
}

#[test]
fn tampering_with_the_expected_side_is_noticed_by_the_comparison() {
    let cases = generic_cases();
    let at = first_with_rows(&cases);
    assert!(compare(&cases[at], &plan_of(&cases[at])).is_ok());
    let tampers: [(&str, fn(&mut Case)); 7] = [
        ("opacity", |c: &mut Case| c.placed[0].opacity += 0.125),
        ("name", |c: &mut Case| c.placed[0].name.push('!')),
        ("centre x", |c: &mut Case| c.placed[0].x += 0.5),
        ("size", |c: &mut Case| c.placed[0].w += 1),
        ("visible", |c: &mut Case| c.placed[0].visible = false),
        ("fake resize", |c: &mut Case| c.resizes.push((7, 7))),
        ("missing row", |c: &mut Case| {
            c.placed.pop();
        }),
    ];
    for (what, tamper) in tampers {
        let mut c = cases[at].clone();
        tamper(&mut c);
        assert!(compare(&c, &plan_of(&c)).is_err(), "{what} slipped through");
    }
}

#[test]
fn tampering_with_a_throw_case_is_noticed_by_the_comparison() {
    let cases = generic_cases();
    let at = cases.iter().position(|c| c.id == "only_groups").unwrap();
    assert!(compare(&cases[at], &plan_of(&cases[at])).is_ok());
    let mut wrong_text = cases[at].clone();
    wrong_text.outcome = Outcome::Throw("autre message".to_string());
    assert!(compare(&wrong_text, &plan_of(&wrong_text)).is_err());
    let mut pretend_ok = cases[at].clone();
    pretend_ok.outcome = Outcome::Count(2);
    assert!(compare(&pretend_ok, &plan_of(&pretend_ok)).is_err());
    let mut pretend_placed = cases[at].clone();
    pretend_placed.placed.push(Placed {
        source: 0,
        name: "x".to_string(),
        x: 0.0,
        y: 0.0,
        w: 1,
        h: 1,
        opacity: 1.0,
        visible: true,
    });
    assert!(compare(&pretend_placed, &plan_of(&pretend_placed)).is_err());
}

#[test]
fn an_eligibility_flip_in_the_input_changes_the_plan() {
    let cases = generic_cases();
    let case = cases
        .iter()
        .find(|c| c.id == "groups_and_hidden_mixed")
        .unwrap();
    let base = plan_of(case).unwrap();
    for i in 0..case.psd.children.len() {
        let mut flipped = case.clone();
        let l = &mut flipped.psd.children[i];
        l.hidden = !l.hidden;
        let after = plan_of(&flipped);
        assert_ne!(
            after.ok(),
            Some(base.clone()),
            "toggling hidden of child {i} must matter"
        );
    }
}
