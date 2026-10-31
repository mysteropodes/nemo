//! Duplicator inspector edits (#2256 Q4): the panel's `wireNum` / checkbox / select writers of the baseline
//! (`timeline.js` at 1065e9d8, the Mograph duplicator panel) as ONE closed partial edit of the stored descriptor.
//!
//! Baseline per field (`d` is `ld.duplicator`): `mode` is the select value; `rows`/`cols` are `parseFloat` clamped to
//! 1..30 and `count` to 1..500; `spacingX`, `spacingY`, `radius`, `startAngle` and `offsetFrames` are unbounded numbers;
//! `seed` is the integer the reseed button draws (`Math.floor(Math.random()*1e6)`, 0..999999); `radialOrient` and
//! `pathAlignTangent` are checkboxes; `staggerRandom` is created on first write as `{}` and then takes one boolean per
//! channel; `timeOffset` is created on first write as `{enabled:false, offsetFrames:1, direction:'forward'}`.
//!
//! The edit names only the members it changes: every other stored member (including `endAngle`, `pathLayerUid`,
//! `sourceMode`, `sourceLayerUids`, `effectors`) keeps its bytes. A value equal to the stored one changes nothing, and an
//! edit that changes nothing publishes nothing (no revision, no history entry). A layer without a duplicator is refused.
//! The path layer keeps its own operation (`object.duplicator.path`). Declared as a private child of
//! `object_component_owner_kernels.rs`.
use crate::commands::object_fill_owner::component_owner::object_component_convert_support::{
    empty_duplicator_set, layer_index, require_v2, update, with_tables, SetupError, SetupPlan,
};
use crate::object_document::object_component_record_kernels::object_component_records::SCENE_CONTEXT;
use crate::object_document::object_component_record_kernels::object_duplicator_records::{
    DuplicatorDescriptor, StaggerRandom, TimeOffset,
};
use crate::object_document::{ObjectDocument, Presence};
use serde_json::Number;

pub(crate) const MODES: [&str; 3] = ["grid", "radial", "path"];
pub(crate) const DIRECTIONS: [&str; 4] = ["forward", "backward", "centerOut", "random"];
/// The baseline `wireNum` bounds.
const GRID_SIDE: (f64, f64) = (1.0, 30.0);
const COUNT: (f64, f64) = (1.0, 500.0);
pub(crate) const MAX_SEED: u64 = 999_999;

/// Per-channel random stagger booleans a request may name.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct StaggerEdit {
    pub(crate) position: Option<bool>,
    pub(crate) rotation: Option<bool>,
    pub(crate) scale: Option<bool>,
    pub(crate) opacity: Option<bool>,
    pub(crate) hue: Option<bool>,
}

/// The temporal stagger members a request may name.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct TimeEdit {
    pub(crate) enabled: Option<bool>,
    pub(crate) offset_frames: Option<f64>,
    pub(crate) direction: Option<String>,
}

/// The members of one `object.duplicator.config` request (every one optional).
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct ConfigEdit {
    pub(crate) mode: Option<String>,
    pub(crate) rows: Option<f64>,
    pub(crate) cols: Option<f64>,
    pub(crate) spacing_x: Option<f64>,
    pub(crate) spacing_y: Option<f64>,
    pub(crate) count: Option<f64>,
    pub(crate) radius: Option<f64>,
    pub(crate) start_angle: Option<f64>,
    pub(crate) radial_orient: Option<bool>,
    pub(crate) path_align_tangent: Option<bool>,
    pub(crate) seed: Option<u64>,
    pub(crate) stagger_random: Option<StaggerEdit>,
    pub(crate) time_offset: Option<TimeEdit>,
}

/// A JSON number: an integer when the value is one (so a stored `2` stays `2`), else the float.
fn number(value: f64) -> Number {
    if value.fract() == 0.0 && value.abs() < 9.0e15 {
        Number::from(value as i64)
    } else {
        Number::from_f64(value).unwrap_or_else(|| Number::from(0))
    }
}

fn set_number(slot: &mut Presence<Number>, wanted: Option<f64>, bounds: Option<(f64, f64)>) {
    let Some(raw) = wanted else { return };
    let value = bounds.map_or(raw, |(low, high)| raw.max(low).min(high));
    if slot.value().and_then(Number::as_f64) != Some(value) {
        *slot = Presence::Present(number(value));
    }
}

fn set_flag(slot: &mut Presence<bool>, wanted: Option<bool>) {
    if let Some(value) = wanted {
        *slot = Presence::Present(value);
    }
}

fn label(wanted: &Option<String>, allowed: &[&str]) -> Result<(), SetupError> {
    match wanted.as_deref() {
        Some(value) if !allowed.contains(&value) => Err(SetupError::ConfigInvalid),
        _ => Ok(()),
    }
}

fn apply(descriptor: &mut DuplicatorDescriptor, edit: &ConfigEdit) -> Result<(), SetupError> {
    label(&edit.mode, &MODES)?;
    if let Some(time) = &edit.time_offset {
        label(&time.direction, &DIRECTIONS)?;
    }
    if edit.seed.is_some_and(|seed| seed > MAX_SEED) {
        return Err(SetupError::SeedRange);
    }
    if let Some(mode) = &edit.mode {
        if descriptor.mode.value() != Some(mode) {
            descriptor.mode = Presence::Present(mode.clone());
        }
    }
    set_number(&mut descriptor.rows, edit.rows, Some(GRID_SIDE));
    set_number(&mut descriptor.cols, edit.cols, Some(GRID_SIDE));
    set_number(&mut descriptor.count, edit.count, Some(COUNT));
    set_number(&mut descriptor.spacing_x, edit.spacing_x, None);
    set_number(&mut descriptor.spacing_y, edit.spacing_y, None);
    set_number(&mut descriptor.radius, edit.radius, None);
    set_number(&mut descriptor.start_angle, edit.start_angle, None);
    set_number(&mut descriptor.seed, edit.seed.map(|s| s as f64), None);
    set_flag(&mut descriptor.radial_orient, edit.radial_orient);
    set_flag(&mut descriptor.path_align_tangent, edit.path_align_tangent);
    if let Some(stagger) = edit.stagger_random.as_ref().filter(|s| **s != StaggerEdit::default()) {
        let created = || StaggerRandom {
            position: Presence::Missing,
            rotation: Presence::Missing,
            scale: Presence::Missing,
            opacity: Presence::Missing,
            hue: Presence::Missing,
        };
        update(&mut descriptor.stagger_random, created, |slot| {
            set_flag(&mut slot.position, stagger.position);
            set_flag(&mut slot.rotation, stagger.rotation);
            set_flag(&mut slot.scale, stagger.scale);
            set_flag(&mut slot.opacity, stagger.opacity);
            set_flag(&mut slot.hue, stagger.hue);
        });
    }
    if let Some(time) = edit.time_offset.as_ref().filter(|t| **t != TimeEdit::default()) {
        let created = || TimeOffset {
            enabled: Presence::Present(false),
            offset_frames: Presence::Present(Number::from(1_u32)),
            direction: Presence::Present("forward".to_string()),
        };
        update(&mut descriptor.time_offset, created, |slot| {
            set_flag(&mut slot.enabled, time.enabled);
            set_number(&mut slot.offset_frames, time.offset_frames, None);
            if let Some(direction) = &time.direction {
                if slot.direction.value() != Some(direction) {
                    slot.direction = Presence::Present(direction.clone());
                }
            }
        });
    }
    Ok(())
}

/// The panel's field writers as one transaction on the layer's stored descriptor.
pub(crate) fn plan_duplicator_config(
    document: &ObjectDocument,
    layer_uid: &str,
    edit: &ConfigEdit,
) -> Result<SetupPlan, SetupError> {
    require_v2(document)?;
    layer_index(document, layer_uid)?;
    let record = document
        .duplicator_set()
        .and_then(|set| set.duplicator(SCENE_CONTEXT, layer_uid))
        .ok_or_else(|| SetupError::NotADuplicator(layer_uid.to_string()))?;
    let mut next = record.duplicator.clone();
    apply(&mut next, edit)?;
    if next == record.duplicator {
        return Ok(SetupPlan::unchanged(document, None));
    }
    let mut candidate = document.clone();
    with_tables(&mut candidate, |tables| {
        update(
            &mut tables.duplicators,
            empty_duplicator_set,
            |set| match set
                .duplicators
                .iter_mut()
                .find(|r| r.context_id == SCENE_CONTEXT && r.layer_uid == layer_uid)
            {
                Some(found) => {
                    found.duplicator = next;
                    Ok(())
                }
                None => Err(SetupError::NotADuplicator(layer_uid.to_string())),
            },
        )
    })?;
    Ok(SetupPlan {
        document: candidate,
        changed: true,
        symbol_id: None,
        layer_uid: Some(layer_uid.to_string()),
        created: None,
    })
}

#[cfg(test)]
#[path = "../tests/object_component_convert_fixture.rs"]
mod fixture;
#[cfg(test)]
#[path = "../tests/object_duplicator_config.rs"]
mod tests;
