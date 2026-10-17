//! The sparse retime plan against the oracle, with no document, history or request layer in the way: every row of
//! retime.txt (the verbatim baseline `moveKeyframe` with its tween-span retime at fbe4b8a3) is turned into the subject
//! layer's `LayerTiming` and run through `LayerTiming::retime_key`. The plan must reproduce the baseline's key and sample
//! flags (with the manual-edit presence), the landing frame, the key pairs handed to `rekeyTweenPairData`, and which old
//! cell's strokes every written cell carries; refusals and no-ops must be typed as the baseline's outcome class says.
//! Rows whose subject layer is locked are not the kernel's business (the lock policy is the transaction's).
use super::*;

const RETIME: &str = include_str!("fixtures/object-exposure-edit-retime/retime.txt");

struct Row {
    id: usize,
    from: i64,
    to: i64,
    outcome: String,
    total: u32,
    applied: Option<u32>,
    input: Vec<String>,
    output: Vec<String>,
    pairs: String,
}
fn rows() -> Vec<Row> {
    RETIME
        .lines()
        .filter(|line| line.starts_with("row "))
        .map(|line| {
            let part: Vec<&str> = line.split(' ').collect();
            let args: Vec<&str> = part[2].split(':').collect();
            let layer: usize = args[0].parse().unwrap();
            let cells = |text: &str| -> Vec<String> {
                text.split('|')
                    .nth(layer)
                    .unwrap()
                    .split(';')
                    .map(str::to_owned)
                    .collect()
            };
            Row {
                id: part[1].parse().unwrap(),
                from: args[1].parse().unwrap(),
                to: args[2].parse().unwrap(),
                outcome: part[5].to_owned(),
                total: part[6].parse().unwrap(),
                applied: part[7].parse().ok(),
                input: cells(part[8]),
                output: cells(part[9]),
                pairs: part[12].to_owned(),
            }
        })
        .collect()
}
fn timing_from(cells: &[String]) -> LayerTiming {
    let mut timing = LayerTiming::blank_drawing();
    timing.keys.clear();
    for (frame, token) in cells.iter().enumerate() {
        let frame = frame as u32;
        match token.chars().next().unwrap() {
            'K' => timing.keys.push(TimingKey { frame }),
            'I' => timing.samples.push(TimingSample {
                frame,
                manual_edit: Presence::Missing,
            }),
            'M' => timing.samples.push(TimingSample {
                frame,
                manual_edit: Presence::Present(true),
            }),
            'N' => timing.samples.push(TimingSample {
                frame,
                manual_edit: Presence::Present(false),
            }),
            _ => {}
        }
    }
    timing
}
fn flags(timing: &LayerTiming, total: u32) -> String {
    let mut out = vec!['.'; total as usize];
    for key in &timing.keys {
        out[key.frame as usize] = 'K';
    }
    for sample in &timing.samples {
        out[sample.frame as usize] = match sample.manual_edit {
            Presence::Missing => 'I',
            Presence::Present(true) => 'M',
            Presence::Present(false) => 'N',
        };
    }
    out.into_iter().collect()
}
fn strokes(token: &str) -> &str {
    token.split_once('=').map_or("", |(_, list)| list)
}
fn pair_text(plan: &RetimePlan) -> String {
    let list: Vec<String> = plan
        .pairs
        .iter()
        .map(|p| format!("{}-{}>{}-{}", p.from_a, p.from_b, p.to_a, p.to_b))
        .collect();
    list.join(";")
}

fn run(row: &Row) -> Result<RetimeEdit, LayerTimingError> {
    timing_from(&row.input).retime_key(row.from as u32, row.to as u32, row.total)
}
/// The rows the kernel decides: every class but the lock policy, and no negative frame (a payload refusal).
fn kernel_rows() -> Vec<Row> {
    rows()
        .into_iter()
        .filter(|row| row.outcome != "locked" && row.to >= 0 && row.from >= 0)
        .collect()
}

#[test]
fn every_oracle_row_matches_the_sparse_retime_plan() {
    let all = kernel_rows();
    assert_eq!(all.len(), 599);
    let mut changed = 0;
    for row in &all {
        let got = run(row);
        match row.outcome.as_str() {
            "changed" => {
                let plan = match got {
                    Ok(RetimeEdit::Changed(plan)) => plan,
                    other => panic!("row {}: expected a changed plan, got {other:?}", row.id),
                };
                changed += 1;
                assert_eq!(
                    flags(&plan.timing, row.total),
                    row.output
                        .iter()
                        .map(|token| token.chars().next().unwrap())
                        .collect::<String>(),
                    "row {} flags",
                    row.id
                );
                assert_eq!(Some(plan.applied_to), row.applied, "row {}", row.id);
                assert_eq!(pair_text(&plan), row.pairs, "row {} pairs", row.id);
                assert!(plan.timing.validate(row.total).is_ok(), "row {}", row.id);
                // which old cell's strokes every written cell carries, and that unwritten cells keep their own
                for frame in 0..row.total as usize {
                    let written = plan.writes.iter().find(|(at, _)| *at as usize == frame);
                    let expected = match written {
                        Some((_, Some(source))) => strokes(&row.input[*source as usize]),
                        Some((_, None)) => "",
                        None => strokes(&row.input[frame]),
                    };
                    assert_eq!(
                        strokes(&row.output[frame]),
                        expected,
                        "row {} frame {frame} strokes",
                        row.id
                    );
                }
                assert!(
                    plan.writes.windows(2).all(|pair| pair[0].0 < pair[1].0),
                    "row {}: writes ascend with one entry per frame",
                    row.id
                );
            }
            "noop" | "same" => assert_eq!(got, Ok(RetimeEdit::NoChange), "row {}", row.id),
            "range" | "notkey" => {
                assert_eq!(got, Err(LayerTimingError::Invalid), "row {}", row.id)
            }
            other => panic!("unexpected outcome {other}"),
        }
    }
    assert_eq!(changed, 345);
}

#[test]
fn the_plan_never_mutates_its_input_and_a_refusal_leaves_it_alone() {
    for row in &kernel_rows() {
        let timing = timing_from(&row.input);
        let before = timing.clone();
        let _ = timing.retime_key(row.from as u32, row.to as u32, row.total);
        assert_eq!(timing, before, "row {}", row.id);
    }
}

#[test]
fn a_timing_of_another_family_is_unsupported_and_an_overfull_one_is_invalid() {
    let mut foreign = timing_from(&["K".to_owned(), ".".to_owned(), "K".to_owned()]);
    foreign.family = "component".into();
    assert_eq!(
        foreign.retime_key(0, 1, 3),
        Err(LayerTimingError::Unsupported)
    );
    // validate runs against the frame count first: a key beyond it is not a timing this kernel edits
    let wide = timing_from(&["K".to_owned(), ".".to_owned(), "K".to_owned()]);
    assert_eq!(wide.retime_key(0, 1, 2), Err(LayerTimingError::Invalid));
}

#[test]
fn the_oracle_keeps_reaching_the_plans_that_matter() {
    let changed: Vec<Row> = kernel_rows()
        .into_iter()
        .filter(|row| row.outcome == "changed")
        .collect();
    let plans: Vec<RetimePlan> = changed
        .iter()
        .map(|row| match run(row) {
            Ok(RetimeEdit::Changed(plan)) => plan,
            other => panic!("row {}: {other:?}", row.id),
        })
        .collect();
    let count = |test: &dyn Fn(&Row, &RetimePlan) -> bool| {
        changed
            .iter()
            .zip(plans.iter())
            .filter(|(row, plan)| test(row, plan))
            .count()
    };
    let reach = [
        count(&|_, plan| plan.writes.iter().any(|(_, source)| source.is_none())),
        count(&|row, plan| plan.applied_to as i64 != row.to),
        count(&|_, plan| {
            plan.writes.iter().any(|(at, source)| {
                source.is_some_and(|s| s != *at) && !plan.timing.keys.iter().any(|k| k.frame == *at)
            })
        }),
        count(&|_, plan| {
            plan.timing
                .samples
                .iter()
                .any(|sample| sample.manual_edit == Presence::Present(true))
        }),
    ];
    assert_eq!(reach, [332, 62, 142, 183]);
}
