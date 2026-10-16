//! Controls for the shot module edits: each deliberate mistake, run through the
//! same oracle comparison as the shipped kernel, must be rejected by the rows
//! that exercise it. Counts are what `tools/gen.mjs` measured on the JS baseline
//! with the equivalent source edit (the Rust side checks the same rows).

use super::super::rows::ORACLE;
use super::super::ShotChainError;
use super::rows::{run, Impls, Report, PRODUCTION};
use super::{
    count_kind, new_montage, split_sound, AutoGrid, Field, ModuleKind, MontageAdded, NewInstance,
    SoundModule, SoundSplit, AUTO_GRID, SOUND_MENU_MARGIN,
};

/// `m.inSec + 0.05` with no `|| 0`: undefined adds as NaN, null as 0.
fn raw(field: &Field<f64>) -> f64 {
    match field {
        Field::Absent => f64::NAN,
        Field::Null => 0.0,
        Field::Value(value) => *value,
    }
}

fn menu_with(sound: &SoundModule, low: f64, high: f64, or_zero: bool) -> bool {
    match &sound.cursor_sec {
        Field::Value(cursor) => {
            let start = if or_zero {
                sound.in_sec.or_zero()
            } else {
                raw(&sound.in_sec)
            };
            *cursor > start + low && *cursor < sound.out_sec.or_zero() - high
        }
        _ => false,
    }
}

fn no_low_margin(sound: &SoundModule) -> bool {
    menu_with(sound, 0.0, SOUND_MENU_MARGIN, true)
}

fn no_high_margin(sound: &SoundModule) -> bool {
    menu_with(sound, SOUND_MENU_MARGIN, 0.0, true)
}

fn no_or_zero(sound: &SoundModule) -> bool {
    menu_with(sound, SOUND_MENU_MARGIN, SOUND_MENU_MARGIN, false)
}

/// The right half starts where the left one started instead of at the cursor.
fn right_keeps_in(sound: &SoundModule) -> Result<SoundSplit, ShotChainError> {
    let mut split = split_sound(sound)?;
    split.right.in_sec = sound.in_sec.clone();
    Ok(split)
}

/// The left half keeps its cursor.
fn left_keeps_cursor(sound: &SoundModule) -> Result<SoundSplit, ShotChainError> {
    let mut split = split_sound(sound)?;
    split.left_cursor_sec = sound.cursor_sec.clone();
    Ok(split)
}

/// The right half is offset by 25 instead of 250.
fn short_offset(sound: &SoundModule) -> Result<SoundSplit, ShotChainError> {
    let mut split = split_sound(sound)?;
    split.right.x = sound.x + 25.0;
    Ok(split)
}

/// `Math.floor` instead of `Math.round` for the x coordinate.
fn floor_x(symbol_id: &str, x: f64, y: f64) -> Result<NewInstance, ShotChainError> {
    let mut added = super::new_instance(symbol_id, x, y)?;
    added.x = x.floor() + 0.0;
    Ok(added)
}

/// The montage count without the `+ 1`.
fn name_without_plus_one(
    modules: &[ModuleKind],
    x: f64,
    y: f64,
) -> Result<MontageAdded, ShotChainError> {
    let mut added = new_montage(modules, x, y)?;
    added.montage.name = format!("Montage {}", count_kind(modules, ModuleKind::Montage));
    Ok(added)
}

fn rejected(report: Report, at_least: usize, kinds: &str) -> Vec<String> {
    assert!(
        report.bad.len() >= at_least,
        "slipped through: {} of {} rows",
        report.bad.len(),
        report.checked
    );
    assert!(
        report.bad.iter().all(|(k, _)| kinds.contains(*k)),
        "unexpected row kind rejected"
    );
    report.bad.into_iter().map(|(_, text)| text).collect()
}

#[test]
fn a_menu_without_the_low_margin_is_rejected() {
    let impls = Impls {
        menu: no_low_margin,
        ..PRODUCTION
    };
    let bad = rejected(run(&impls), 12, "K");
    assert!(bad.iter().any(|b| b == "K ~ 10 0.05 0"));
}

#[test]
fn a_menu_without_the_high_margin_is_rejected() {
    let impls = Impls {
        menu: no_high_margin,
        ..PRODUCTION
    };
    let bad = rejected(run(&impls), 10, "K");
    assert!(bad.iter().any(|b| b == "K ~ 10 9.95 0"));
}

#[test]
fn a_menu_that_forgets_the_zero_default_is_rejected() {
    let impls = Impls {
        menu: no_or_zero,
        ..PRODUCTION
    };
    let bad = rejected(run(&impls), 15, "K");
    assert!(bad.iter().any(|b| b == "K ~ 10 0.06 1"));
}

#[test]
fn a_right_half_that_keeps_the_old_start_is_rejected() {
    let impls = Impls {
        split: right_keeps_in,
        ..PRODUCTION
    };
    rejected(run(&impls), 118, "W");
}

#[test]
fn a_left_half_that_keeps_its_cursor_is_rejected() {
    let impls = Impls {
        split: left_keeps_cursor,
        ..PRODUCTION
    };
    rejected(run(&impls), 118, "W");
}

#[test]
fn a_short_right_offset_is_rejected() {
    let impls = Impls {
        split: short_offset,
        ..PRODUCTION
    };
    rejected(run(&impls), 118, "W");
}

#[test]
fn five_columns_are_rejected() {
    let impls = Impls {
        grid: AutoGrid {
            columns: 5,
            ..AUTO_GRID
        },
        ..PRODUCTION
    };
    let bad = rejected(run(&impls), 36, "G");
    assert!(bad.iter().any(|b| b == "G iiiiii 40 200"));
}

#[test]
fn a_row_step_of_140_is_rejected() {
    let impls = Impls {
        grid: AutoGrid {
            y_step: 140,
            ..AUTO_GRID
        },
        ..PRODUCTION
    };
    let bad = rejected(run(&impls), 40, "G");
    assert!(bad.iter().any(|b| b == "G iiiiii 40 200"));
}

#[test]
fn floor_instead_of_round_for_the_drop_point_is_rejected() {
    let impls = Impls {
        instance: floor_x,
        ..PRODUCTION
    };
    let bad = rejected(run(&impls), 28, "A");
    assert!(bad.iter().any(|b| b == "A 0.5 0 1 0 sym"));
}

#[test]
fn a_montage_name_without_the_plus_one_is_rejected() {
    let impls = Impls {
        montage: name_without_plus_one,
        ..PRODUCTION
    };
    let bad = rejected(run(&impls), 18, "M");
    assert!(bad.iter().any(|b| b.starts_with("M - 0 0 Montage_1 ")));
}

/// The production kernel passes the same comparison the mutants fail, and the
/// grid contains the classes the controls above reject.
#[test]
fn the_unmutated_comparison_is_clean_and_the_classes_are_present() {
    assert!(run(&PRODUCTION).bad.is_empty());
    let has = |line: &str| ORACLE.lines().any(|l| l == line);
    assert!(has("K ~ 10 0.05 0"), "low margin edge");
    assert!(has("K ~ 10 9.95 0"), "high margin edge");
    assert!(has("A 0.5 0 1 0 sym"), "rounding tie");
    assert!(has("G iiiiii 40 200"), "first wrapped row");
}
