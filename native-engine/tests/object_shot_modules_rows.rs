//! Oracle rows and comparison harness for the shot module edits (L10-B, I17/I42).
//!
//! Reads the `K`, `W`, `X`, `G`, `A` and `M` rows of
//! `fixtures/object-shot-chain/oracle.txt` (see `object_shot_chain_rows.rs` for
//! the generator and the baseline commit): `K in out cursor menu`,
//! `W name in out cursor x y result`, `X name in out cursor x y reason baseline`,
//! `G kinds x y`, `A x y x' y' sym` and `M kinds x y name x' y' chainLen
//! audioLen playhead active added where`. `~` is `undefined`, `null` is `null`;
//! a kinds string is `-` (none) or letters `i`nstance / `m`ontage / `s`ound /
//! `o`ther, one module each. Every sound has name `snd`, source `B64`, x 12.5
//! and y 7 unless the row names its own.

use super::super::rows::oracle_rows;
use super::super::ShotChainError;
use super::{
    add_instance_auto_with, new_instance, new_montage, sound_split_menu_enabled, split_sound,
    AutoGrid, Field, ModuleKind, ModuleRef, MontageAdded, NewInstance, SoundModule, SoundSplit,
    AUTO_GRID,
};

pub(crate) type MenuFn = fn(&SoundModule) -> bool;
pub(crate) type SoundFn = fn(&SoundModule) -> Result<SoundSplit, ShotChainError>;
pub(crate) type InstanceFn = fn(&str, f64, f64) -> Result<NewInstance, ShotChainError>;
pub(crate) type MontageFn = fn(&[ModuleKind], f64, f64) -> Result<MontageAdded, ShotChainError>;

/// The pieces a control may replace; `PRODUCTION` is the shipped kernel.
#[derive(Clone, Copy)]
pub(crate) struct Impls {
    pub menu: MenuFn,
    pub split: SoundFn,
    pub grid: AutoGrid,
    pub instance: InstanceFn,
    pub montage: MontageFn,
}

pub(crate) const PRODUCTION: Impls = Impls {
    menu: sound_split_menu_enabled,
    split: split_sound,
    grid: AUTO_GRID,
    instance: new_instance,
    montage: new_montage,
};

/// Rows compared and the rows that disagreed (kind, text).
pub(crate) struct Report {
    pub checked: usize,
    pub bad: Vec<(char, String)>,
}

fn row_text(kind: char, fields: &[&str]) -> String {
    format!("{} {}", kind, fields.join(" "))
}

fn number(token: &str) -> f64 {
    token.parse().expect("number token")
}

/// JavaScript `String(number)` for the magnitudes the oracle holds.
pub(crate) fn js_num(value: f64) -> String {
    if value.is_nan() {
        "NaN".to_string()
    } else if value == f64::INFINITY {
        "Infinity".to_string()
    } else if value == f64::NEG_INFINITY {
        "-Infinity".to_string()
    } else {
        format!("{}", value)
    }
}

pub(crate) fn num_field(token: &str) -> Field<f64> {
    match token {
        "~" => Field::Absent,
        "null" => Field::Null,
        _ => Field::Value(number(token)),
    }
}

fn name_field(token: &str) -> Field<String> {
    match token {
        "~" => Field::Absent,
        "null" => Field::Null,
        _ => Field::Value(token.to_string()),
    }
}

fn num_text(field: &Field<f64>) -> String {
    match field {
        Field::Absent => "~".to_string(),
        Field::Null => "null".to_string(),
        Field::Value(value) => js_num(*value),
    }
}

fn name_text(field: &Field<String>) -> String {
    match field {
        Field::Absent => "~".to_string(),
        Field::Null => "null".to_string(),
        Field::Value(value) => value.clone(),
    }
}

/// A sound module as the rows build it.
pub(crate) fn sound_of(
    name: &str,
    in_sec: &str,
    out_sec: &str,
    cursor: &str,
    x: f64,
    y: f64,
) -> SoundModule {
    SoundModule {
        module_id: "snd0".to_string(),
        name: name_field(name),
        source: "B64".to_string(),
        x,
        y,
        in_sec: num_field(in_sec),
        out_sec: num_field(out_sec),
        cursor_sec: num_field(cursor),
    }
}

pub(crate) fn kinds_of(token: &str) -> Vec<ModuleKind> {
    if token == "-" {
        return Vec::new();
    }
    token
        .chars()
        .map(|letter| match letter {
            'i' => ModuleKind::Instance,
            'm' => ModuleKind::Montage,
            's' => ModuleKind::Sound,
            'o' => ModuleKind::Other,
            other => panic!("unknown kind {}", other),
        })
        .collect()
}

/// The `result` text of a `W` row, rebuilt from the split edit.
pub(crate) fn split_text(split: &SoundSplit) -> String {
    let right = &split.right;
    format!(
        "lo={};lc={};r={},{},{},{},{},{},{},N",
        js_num(split.left_out_sec),
        num_text(&split.left_cursor_sec),
        name_text(&right.name),
        num_text(&right.in_sec),
        num_text(&right.out_sec),
        num_text(&right.cursor_sec),
        js_num(right.x),
        js_num(right.y),
        right.source
    )
}

fn same_number(text: &str, value: f64) -> bool {
    number(text).to_bits() == value.to_bits()
}

/// Run every module row through `impls`.
pub(crate) fn run(impls: &Impls) -> Report {
    let mut report = Report {
        checked: 0,
        bad: Vec::new(),
    };
    for fields in oracle_rows('K') {
        let sound = sound_of("snd", fields[0], fields[1], fields[2], 12.5, 7.0);
        report.checked += 1;
        if (impls.menu)(&sound) != (fields[3] == "1") {
            report.bad.push(('K', row_text('K', &fields)));
        }
    }
    for fields in oracle_rows('W') {
        let sound = sound_of(
            fields[0],
            fields[1],
            fields[2],
            fields[3],
            number(fields[4]),
            number(fields[5]),
        );
        let ok = match (impls.split)(&sound) {
            Ok(split) => split.module_id == "snd0" && split_text(&split) == fields[6],
            Err(_) => false,
        };
        report.checked += 1;
        if !ok {
            report.bad.push(('W', row_text('W', &fields)));
        }
    }
    for fields in oracle_rows('X') {
        let sound = sound_of(
            fields[0],
            fields[1],
            fields[2],
            fields[3],
            number(fields[4]),
            number(fields[5]),
        );
        let want = match fields[6] {
            "nocursor" => ShotChainError::CursorMissing,
            "outside" => ShotChainError::CursorNotInside,
            other => panic!("unknown reason {}", other),
        };
        report.checked += 1;
        if (impls.split)(&sound) != Err(want) {
            report.bad.push(('X', row_text('X', &fields)));
        }
    }
    for fields in oracle_rows('G') {
        let got = add_instance_auto_with(&impls.grid, "sym", &kinds_of(fields[0]));
        report.checked += 1;
        let ok = match got {
            Ok(added) => {
                same_number(fields[1], added.x)
                    && same_number(fields[2], added.y)
                    && added.symbol_id == "sym"
                    && added.request == 0
                    && added.trim_in.is_none()
                    && added.trim_out.is_none()
                    && added.duration.is_none()
            }
            Err(_) => false,
        };
        if !ok {
            report.bad.push(('G', row_text('G', &fields)));
        }
    }
    for fields in oracle_rows('A') {
        let got = (impls.instance)(fields[4], number(fields[0]), number(fields[1]));
        report.checked += 1;
        let ok = match got {
            Ok(added) => {
                same_number(fields[2], added.x)
                    && same_number(fields[3], added.y)
                    && added.symbol_id == fields[4]
            }
            Err(_) => false,
        };
        if !ok {
            report.bad.push(('A', row_text('A', &fields)));
        }
    }
    for fields in oracle_rows('M') {
        let got = (impls.montage)(&kinds_of(fields[0]), number(fields[1]), number(fields[2]));
        report.checked += 1;
        let ok = match got {
            Ok(added) => {
                let montage = &added.montage;
                montage.name.replace(' ', "_") == fields[3]
                    && same_number(fields[4], montage.x)
                    && same_number(fields[5], montage.y)
                    && montage.chain.len().to_string() == fields[6]
                    && montage.audio.len().to_string() == fields[7]
                    && montage.playhead.to_string() == fields[8]
                    && (added.active_montage == ModuleRef::New(montage.request))
                        == (fields[9] == "N")
                    && fields[10] == "1"
                    && fields[11] == "last"
            }
            Err(_) => false,
        };
        if !ok {
            report.bad.push(('M', row_text('M', &fields)));
        }
    }
    report
}
