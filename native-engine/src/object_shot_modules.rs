//! Shot module records and the non-chain StoryBoard edits (L10-B, lane 10 / #2116; IDs I17, I18, I42).
//!
//! Pure port of `splitSound`, the sound-menu guard, `addInstance`,
//! `addInstanceAuto` and `addMontage` of the agreed desktop baseline
//! (`src/js/storyboard.js` at fbe4b8a3). Nothing is read from or written to a
//! document: the caller (lane 4 `object_storyboard_*` records) supplies the
//! module facts and publishes the returned edit in its own transaction.
//!
//! Identity. The baseline mints module ids from a persisted counter
//! (`'sbm' + s.nextId++`). That counter is NOT ported. An edit that creates a
//! module returns a request (`request`, the creation index inside the edit,
//! `0` for the single module each edit here creates) and refers to the new
//! module through [`ModuleRef::New`]; `object_import_remap` (L10-G) binds the
//! request to a stable native id. Every new module is appended to the module
//! list after all existing ones, as `modules.push` does.
//!
//! Semantics kept on purpose: `Math.round` ties go toward `+Infinity`
//! ([`js_round`]); `x || 0` reads absent, `null`, `0` and `NaN` as `0`
//! ([`Field::or_zero`]); `null` and `undefined` stay distinct ([`Field`]).
//! Not ported on purpose: `logSb`, `render`, `_audioBuffers` (runtime state).

use crate::history::object_history::object_shot_kernels::object_shot_chain::ShotChainError;

/// Offset of the right half of a split sound module (`m.x + 250`).
pub(crate) const SOUND_SPLIT_X_OFFSET: f64 = 250.0;
/// Margin of the sound context-menu guard (`inSec + 0.05`, `outSec - 0.05`).
pub(crate) const SOUND_MENU_MARGIN: f64 = 0.05;

/// A JavaScript property that may be absent (`undefined`), `null` or a value.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Field<T> {
    Absent,
    Null,
    Value(T),
}

impl Field<f64> {
    /// `field || 0`: absent, null, `0`, `-0` and `NaN` read as `0`.
    pub(crate) fn or_zero(&self) -> f64 {
        match self {
            Field::Value(value) if *value != 0.0 && !value.is_nan() => *value,
            _ => 0.0,
        }
    }
}

/// `Math.round`: ties toward `+Infinity`, negative zero normalised to `0`.
pub(crate) fn js_round(value: f64) -> f64 {
    let floor = value.floor();
    let rounded = if value - floor >= 0.5 {
        floor + 1.0
    } else {
        floor
    };
    rounded + 0.0
}

/// A module of the edit: an existing id or a request for a new one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ModuleRef {
    Existing(String),
    New(u32),
}

/// `type` of a storyboard module, as the baseline counts them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ModuleKind {
    Instance,
    Montage,
    Sound,
    Other,
}

/// How many modules of `kind` exist (`modules.filter(type === ...).length`).
pub(crate) fn count_kind(modules: &[ModuleKind], kind: ModuleKind) -> usize {
    modules
        .iter()
        .filter(|candidate| **candidate == kind)
        .count()
}

/// A new `instance` module. The trim and duration are absent for a fresh
/// instance and set for the right half of a split clip.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NewInstance {
    pub request: u32,
    pub symbol_id: String,
    pub x: f64,
    pub y: f64,
    pub trim_in: Option<i64>,
    pub trim_out: Option<i64>,
    pub duration: Option<i64>,
}

/// One entry of a montage `audio` list (`{ moduleId, offsetFrames }`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AudioAttach {
    pub module: ModuleRef,
    pub offset_frames: i64,
}

/// A new `montage` module (`chain: [], audio: [], playhead: 0` at creation).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NewMontage {
    pub request: u32,
    pub name: String,
    pub x: f64,
    pub y: f64,
    pub chain: Vec<ModuleRef>,
    pub audio: Vec<AudioAttach>,
    pub playhead: i64,
}

/// `addMontage`: the new montage and `activeMontageId = m.id`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MontageAdded {
    pub montage: NewMontage,
    pub active_montage: ModuleRef,
}

/// The auto-placement grid of `addInstanceAuto`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AutoGrid {
    pub columns: usize,
    pub x0: i64,
    pub x_step: i64,
    pub y0: i64,
    pub y_step: i64,
}

/// `x = 40 + (count % 6) * 140`, `y = 40 + floor(count / 6) * 160`.
pub(crate) const AUTO_GRID: AutoGrid = AutoGrid {
    columns: 6,
    x0: 40,
    x_step: 140,
    y0: 40,
    y_step: 160,
};

/// Grid cell of the `count`-th instance module.
pub(crate) fn auto_instance_position_with(
    grid: &AutoGrid,
    count: usize,
) -> Result<(i64, i64), ShotChainError> {
    let column = i64::try_from(count % grid.columns).map_err(|_| ShotChainError::Overflow)?;
    let row = i64::try_from(count / grid.columns).map_err(|_| ShotChainError::Overflow)?;
    let x = column
        .checked_mul(grid.x_step)
        .and_then(|offset| offset.checked_add(grid.x0));
    let y = row
        .checked_mul(grid.y_step)
        .and_then(|offset| offset.checked_add(grid.y0));
    match (x, y) {
        (Some(x), Some(y)) => Ok((x, y)),
        _ => Err(ShotChainError::Overflow),
    }
}

/// `addInstance`: a bare instance at the rounded drop point.
pub(crate) fn new_instance(symbol_id: &str, x: f64, y: f64) -> Result<NewInstance, ShotChainError> {
    if !x.is_finite() || !y.is_finite() {
        return Err(ShotChainError::NonFinite);
    }
    Ok(NewInstance {
        request: 0,
        symbol_id: symbol_id.to_string(),
        x: js_round(x),
        y: js_round(y),
        trim_in: None,
        trim_out: None,
        duration: None,
    })
}

/// `addInstanceAuto` over an explicit grid; `modules` are the existing kinds.
pub(crate) fn add_instance_auto_with(
    grid: &AutoGrid,
    symbol_id: &str,
    modules: &[ModuleKind],
) -> Result<NewInstance, ShotChainError> {
    let count = count_kind(modules, ModuleKind::Instance);
    let (x, y) = auto_instance_position_with(grid, count)?;
    new_instance(symbol_id, x as f64, y as f64)
}

/// `addInstanceAuto` on the baseline grid.
pub(crate) fn add_instance_auto(
    symbol_id: &str,
    modules: &[ModuleKind],
) -> Result<NewInstance, ShotChainError> {
    add_instance_auto_with(&AUTO_GRID, symbol_id, modules)
}

/// `'Montage ' + (montage count + 1)`.
pub(crate) fn montage_name(existing_montages: usize) -> Result<String, ShotChainError> {
    let next = existing_montages
        .checked_add(1)
        .ok_or(ShotChainError::Overflow)?;
    Ok(format!("Montage {}", next))
}

/// `addMontage`: named after the montage count, empty, active.
pub(crate) fn new_montage(
    modules: &[ModuleKind],
    x: f64,
    y: f64,
) -> Result<MontageAdded, ShotChainError> {
    if !x.is_finite() || !y.is_finite() {
        return Err(ShotChainError::NonFinite);
    }
    let name = montage_name(count_kind(modules, ModuleKind::Montage))?;
    Ok(MontageAdded {
        montage: NewMontage {
            request: 0,
            name,
            x: js_round(x),
            y: js_round(y),
            chain: Vec::new(),
            audio: Vec::new(),
            playhead: 0,
        },
        active_montage: ModuleRef::New(0),
    })
}

/// A `sound` module as the baseline stores it; `source` is the opaque shared
/// audio payload (`dataB64`), never decoded here.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SoundModule {
    pub module_id: String,
    pub name: Field<String>,
    pub source: String,
    pub x: f64,
    pub y: f64,
    pub in_sec: Field<f64>,
    pub out_sec: Field<f64>,
    pub cursor_sec: Field<f64>,
}

/// The right half of a split sound: shares the source of the left half.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NewSound {
    pub request: u32,
    pub name: Field<String>,
    pub source: String,
    pub x: f64,
    pub y: f64,
    pub in_sec: Field<f64>,
    pub out_sec: Field<f64>,
    pub cursor_sec: Field<f64>,
}

/// `splitSound`: the left module's changed fields plus the new right module.
/// The left `inSec` is untouched.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SoundSplit {
    pub module_id: String,
    pub left_out_sec: f64,
    pub left_cursor_sec: Field<f64>,
    pub right: NewSound,
}

/// The context-menu guard: `cursorSec != null && cursorSec > (inSec || 0) +
/// 0.05 && cursorSec < (outSec || 0) - 0.05`. A caller that wants the
/// baseline gesture checks this before [`split_sound`].
pub(crate) fn sound_split_menu_enabled(sound: &SoundModule) -> bool {
    match &sound.cursor_sec {
        Field::Value(cursor) => {
            *cursor > sound.in_sec.or_zero() + SOUND_MENU_MARGIN
                && *cursor < sound.out_sec.or_zero() - SOUND_MENU_MARGIN
        }
        _ => false,
    }
}

/// `splitSound` at the module's cursor. The cursor must be set and strictly
/// inside `(inSec || 0, outSec || 0)`; the baseline function has no guard of
/// its own, so a cursor outside is a typed error here, never a half-valid pair.
pub(crate) fn split_sound(sound: &SoundModule) -> Result<SoundSplit, ShotChainError> {
    let cursor = match &sound.cursor_sec {
        Field::Value(cursor) => *cursor,
        _ => return Err(ShotChainError::CursorMissing),
    };
    if !(sound.in_sec.or_zero() < cursor && cursor < sound.out_sec.or_zero()) {
        return Err(ShotChainError::CursorNotInside);
    }
    if !sound.x.is_finite() || !sound.y.is_finite() {
        return Err(ShotChainError::NonFinite);
    }
    Ok(SoundSplit {
        module_id: sound.module_id.clone(),
        left_out_sec: cursor,
        left_cursor_sec: Field::Null,
        right: NewSound {
            request: 0,
            name: sound.name.clone(),
            source: sound.source.clone(),
            x: sound.x + SOUND_SPLIT_X_OFFSET,
            y: sound.y,
            in_sec: Field::Value(cursor),
            out_sec: sound.out_sec.clone(),
            cursor_sec: Field::Null,
        },
    })
}

#[cfg(test)]
#[path = "../tests/object_shot_modules_rows.rs"]
mod rows;

#[cfg(test)]
#[path = "../tests/object_shot_modules.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/object_shot_modules_controls.rs"]
mod controls;
