//! The sparse frame-edit kernel against the verbatim dense baseline editor (fbe4b8a3).
//!
//! Every oracle row is one dense editor run (src/js/app.js insertFrame, insertKeyframeAt,
//! convertToKeyframes, clearKeyframe, removeFrame, removeFrameSpan, removeTweenSpan and
//! src/js/timeline.js deleteSelectedFrames, executed in a Node vm) converted cell by cell to the
//! sparse model: isKeyframe -> key, isInterpolated -> sample, isManualEdit -> manualEdit. The
//! expected exposure of every output frame comes from the baseline `heldKeyframeAt`, never from
//! the array length. Mutation controls live in object_frame_edit_sparse_controls.rs.
use super::super::{LayerTiming, LayerTimingError, TimingKey, TimingSample};
use super::TimingEdit;
use crate::object_document::Presence;

pub(crate) const ORACLE: &str = include_str!("fixtures/object-frame-edit-baseline/oracle.txt");

pub(crate) type Applier = dyn Fn(&Case, &LayerTiming) -> Result<TimingEdit, LayerTimingError>;

/// One oracle row. `args` are gap `[cf]`, remove `[min, max]`, promote/clear cells,
/// clear-key `[cell]` or demote `[from, to]`; `role` is the gap role (G targeted, P padded) or
/// the baseline entry point that produced the row.
pub(crate) struct Case {
    pub(crate) op: &'static str,
    pub(crate) role: &'static str,
    pub(crate) args: Vec<u32>,
    pub(crate) chg: bool,
    pub(crate) undo: bool,
    pub(crate) input: &'static str,
    pub(crate) output: &'static str,
    pub(crate) exposure: &'static str,
}

fn num(text: &str) -> u32 {
    text.parse().unwrap()
}
fn list(text: &str) -> Vec<u32> {
    text.split(',').map(num).collect()
}
fn parse(line: &'static str) -> Case {
    let t: Vec<&'static str> = line.split(' ').collect();
    let flag = |text: &str| text == "1";
    let (op, role) = (t[0], "");
    match op {
        "gap" => Case {
            op,
            role: t[1],
            args: vec![num(t[2])],
            chg: true,
            undo: true,
            input: t[3],
            output: t[4],
            exposure: t[5],
        },
        "rm" => Case {
            op,
            role: t[1],
            args: vec![num(t[2]), num(t[3])],
            chg: flag(t[4]),
            undo: flag(t[5]),
            input: t[6],
            output: t[7],
            exposure: t[8],
        },
        "promote" => Case {
            op,
            role: t[1],
            args: list(t[2]),
            chg: flag(t[3]),
            undo: flag(t[4]),
            input: t[5],
            output: t[6],
            exposure: t[7],
        },
        "clearkey" | "clear" => Case {
            op,
            role,
            args: list(t[1]),
            chg: flag(t[2]),
            undo: flag(t[3]),
            input: t[4],
            output: t[5],
            exposure: t[6],
        },
        "demote" => Case {
            op,
            role,
            args: vec![num(t[1]), num(t[2])],
            chg: flag(t[3]),
            undo: flag(t[4]),
            input: t[5],
            output: t[6],
            exposure: t[7],
        },
        other => panic!("unknown oracle op {other}"),
    }
}
pub(crate) fn cases() -> Vec<Case> {
    ORACLE
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(parse)
        .collect()
}

/// Dense cell string -> sparse timing: K key, I/M/N sample with manualEdit absent/true/false.
pub(crate) fn from_cells(cells: &str) -> LayerTiming {
    let mut timing = LayerTiming::blank_drawing();
    timing.keys = Vec::new();
    for (index, cell) in cells.chars().enumerate() {
        let frame = index as u32;
        let sample = |manual_edit| TimingSample { frame, manual_edit };
        match cell {
            'K' => timing.keys.push(TimingKey { frame }),
            'I' => timing.samples.push(sample(Presence::Missing)),
            'M' => timing.samples.push(sample(Presence::Present(true))),
            'N' => timing.samples.push(sample(Presence::Present(false))),
            '.' => {}
            other => panic!("unknown cell {other}"),
        }
    }
    timing
}

/// One character per frame through `LayerTiming::exposure`: `*` key, `~` sample, `!` manual
/// sample, `-` unexposed, otherwise the base-36 frame of the held key.
pub(crate) fn exposure_string(timing: &LayerTiming, total: u32) -> String {
    (0..total)
        .map(|frame| match timing.exposure(frame) {
            ("key", Some(key), false) if key == frame => '*',
            ("interpolated", Some(sample), manual) if sample == frame => {
                if manual {
                    '!'
                } else {
                    '~'
                }
            }
            ("held", Some(key), false) if key < frame => char::from_digit(key, 36).unwrap(),
            ("unexposed", None, false) => '-',
            other => panic!("unexpected exposure {other:?} at frame {frame}"),
        })
        .collect()
}

pub(crate) fn real_apply(
    case: &Case,
    timing: &LayerTiming,
) -> Result<TimingEdit, LayerTimingError> {
    let total = case.input.len() as u32;
    match (case.op, case.role) {
        ("gap", "G") => timing.insert_gap(case.args[0], total),
        ("gap", _) => timing.insert_pad(total),
        ("rm", _) => timing.remove_span(case.args[0], case.args[1], total),
        ("promote", _) => timing.promote_cells(&case.args, total),
        ("clearkey", _) => timing.clear_key(case.args[0], total),
        ("clear", _) => timing.clear_cells(&case.args, total),
        ("demote", _) => timing.demote_span(case.args[0], case.args[1], total),
        other => panic!("unknown case {other:?}"),
    }
}

/// Compare one applier result with the baseline row: the edit must publish exactly when the dense
/// editor changed anything, the sparse result must equal the converted dense output, and the
/// exposure of EVERY output frame must equal the baseline's held/inbetween/blank reading.
pub(crate) fn check(case: &Case, apply: &Applier) -> Result<(), String> {
    let before = from_cells(case.input);
    let (n_in, n_out) = (case.input.len() as u32, case.output.len() as u32);
    let tag = || {
        format!(
            "{} {} {:?} {} -> {}",
            case.op, case.role, case.args, case.input, case.output
        )
    };
    before
        .validate(n_in)
        .map_err(|error| format!("converted input invalid {error:?}: {}", tag()))?;
    match (apply(case, &before), case.chg) {
        (Ok(TimingEdit::NoChange), false) => {
            if exposure_string(&before, n_in) == case.exposure {
                Ok(())
            } else {
                Err(format!("unchanged exposure differs: {}", tag()))
            }
        }
        (Ok(TimingEdit::Changed(after)), true) => {
            if after != from_cells(case.output) {
                return Err(format!("sparse result differs: {}", tag()));
            }
            after
                .validate(n_out)
                .map_err(|error| format!("result invalid {error:?}: {}", tag()))?;
            if exposure_string(&after, n_out) != case.exposure {
                return Err(format!("exposure differs: {}", tag()));
            }
            Ok(())
        }
        (other, _) => Err(format!(
            "{other:?} but baseline changed={}: {}",
            case.chg,
            tag()
        )),
    }
}

/// Failure descriptions of `apply` over every row of one operation.
pub(crate) fn failures(op: &str, apply: &Applier) -> Vec<String> {
    cases()
        .iter()
        .filter(|case| case.op == op)
        .filter_map(|case| check(case, apply).err())
        .collect()
}

#[test]
fn every_oracle_row_matches_the_sparse_kernel() {
    let all = cases();
    assert_eq!(all.len(), 6428);
    let bad: Vec<String> = all
        .iter()
        .filter_map(|case| check(case, &real_apply).err())
        .collect();
    assert!(
        bad.is_empty(),
        "{} mismatches, first: {:?}",
        bad.len(),
        bad.first()
    );
}

#[test]
fn each_operation_matches_on_its_own_rows() {
    for (op, rows) in [
        ("gap", 1029),
        ("rm", 1478),
        ("promote", 1271),
        ("clearkey", 781),
        ("clear", 956),
        ("demote", 913),
    ] {
        assert_eq!(
            cases().iter().filter(|case| case.op == op).count(),
            rows,
            "{op} rows"
        );
        let bad = failures(op, &real_apply);
        assert!(
            bad.is_empty(),
            "{op}: {} mismatches, first {:?}",
            bad.len(),
            bad.first()
        );
    }
}

#[test]
fn oracle_rows_are_internally_consistent() {
    for case in cases() {
        assert_eq!(
            case.output.len(),
            case.exposure.len(),
            "{} exposure covers every frame",
            case.input
        );
        assert_eq!(
            case.chg,
            case.input != case.output,
            "{} {} {:?}",
            case.op,
            case.input,
            case.args
        );
        match (case.op, case.role) {
            ("gap", _) => assert_eq!(case.output.len(), case.input.len() + 1),
            ("rm", _) if case.chg => assert!(case.output.len() < case.input.len()),
            ("rm", _) => assert_eq!(case.output.len(), case.input.len()),
            _ => assert_eq!(
                case.output.len(),
                case.input.len(),
                "{} keeps the frame count",
                case.op
            ),
        }
        assert!(case.input.chars().all(|cell| "KIMN.".contains(cell)));
    }
}

fn is_sample(cell: Option<u8>) -> bool {
    matches!(cell, Some(b'I' | b'M' | b'N'))
}
fn at(case: &Case, frame: u32) -> Option<u8> {
    case.input.as_bytes().get(frame as usize).copied()
}

/// Reach check: each case branch the baseline can take occurs this many times in the oracle, so a
/// regenerated oracle that silently drops a branch (or a vacuous generator) fails here.
#[test]
fn oracle_reaches_every_branch() {
    type Reach = (&'static str, fn(&Case) -> bool, usize);
    let table: [Reach; 26] = [
        ("gap targeted", |c| c.op == "gap" && c.role == "G", 781),
        ("gap padded", |c| c.op == "gap" && c.role == "P", 248),
        (
            "gap straddling a tween",
            |c| {
                c.op == "gap"
                    && c.role == "G"
                    && is_sample(at(c, c.args[0]))
                    && is_sample(at(c, c.args[0] + 1))
            },
            99,
        ),
        (
            "gap appended after the last frame",
            |c| c.op == "gap" && c.role == "G" && c.args[0] as usize + 1 == c.input.len(),
            202,
        ),
        (
            "gap on a layer unexposed at frame 0",
            |c| c.op == "gap" && c.role == "G" && c.exposure.starts_with('-'),
            224,
        ),
        ("remove one frame", |c| c.op == "rm" && c.role == "f", 711),
        (
            "remove a selected span",
            |c| c.op == "rm" && c.role == "s",
            764,
        ),
        (
            "remove with no selection",
            |c| c.op == "rm" && c.role == "n",
            3,
        ),
        ("remove refused", |c| c.op == "rm" && !c.chg, 118),
        (
            "remove leaves no key",
            |c| c.op == "rm" && c.chg && c.input.contains('K') && !c.output.contains('K'),
            194,
        ),
        (
            "remove several frames",
            |c| c.op == "rm" && c.chg && c.args[1] > c.args[0],
            297,
        ),
        (
            "remove drops samples",
            |c| {
                c.op == "rm"
                    && c.chg
                    && c.input[c.args[0] as usize..=c.args[1] as usize]
                        .chars()
                        .any(|x| "IMN".contains(x))
            },
            599,
        ),
        (
            "promote a hold",
            |c| c.op == "promote" && c.role == "a" && c.chg && at(c, c.args[0]) == Some(b'.'),
            230,
        ),
        (
            "promote a sample",
            |c| c.op == "promote" && c.role == "a" && c.chg && is_sample(at(c, c.args[0])),
            316,
        ),
        (
            "promote an existing key",
            |c| c.op == "promote" && c.role == "a" && !c.chg,
            235,
        ),
        (
            "promote several cells",
            |c| c.op == "promote" && c.role == "c" && c.args.len() > 1 && c.chg,
            301,
        ),
        (
            "promote that pushed undo yet changed nothing",
            |c| c.op == "promote" && !c.chg && c.undo,
            59,
        ),
        ("clear key", |c| c.op == "clearkey" && c.chg, 237),
        ("clear key refused", |c| c.op == "clearkey" && !c.chg, 544),
        (
            "clear key leaves no key",
            |c| c.op == "clearkey" && c.chg && !c.output.contains('K'),
            71,
        ),
        (
            "clear a sample",
            |c| c.op == "clear" && c.chg && c.args.iter().any(|a| is_sample(at(c, *a))),
            530,
        ),
        (
            "clear a key",
            |c| c.op == "clear" && c.chg && c.args.iter().any(|a| at(c, *a) == Some(b'K')),
            428,
        ),
        ("clear holds only", |c| c.op == "clear" && !c.chg, 196),
        (
            "demote drops a key inside",
            |c| {
                c.op == "demote"
                    && c.chg
                    && c.input[c.args[0] as usize + 1..c.args[1] as usize].contains('K')
            },
            186,
        ),
        ("demote an empty span", |c| c.op == "demote" && !c.chg, 576),
        (
            "some sample carries explicit manualEdit=false",
            |c| c.input.contains('N'),
            1140,
        ),
    ];
    let all = cases();
    for (name, reached, expected) in table {
        assert_eq!(
            all.iter().filter(|case| reached(case)).count(),
            expected,
            "{name}"
        );
    }
}

/// The dense editor pushes an undo step for some edits that change nothing; the native edit
/// must report `NoChange` for exactly those, so history never publishes them.
#[test]
fn baseline_undo_without_change_is_a_native_no_change() {
    let mut seen = 0;
    for case in cases().iter().filter(|case| case.undo && !case.chg) {
        let got = real_apply(case, &from_cells(case.input));
        assert_eq!(
            got,
            Ok(TimingEdit::NoChange),
            "{} {:?} {}",
            case.op,
            case.args,
            case.input
        );
        seen += 1;
    }
    assert_eq!(seen, 831);
}
