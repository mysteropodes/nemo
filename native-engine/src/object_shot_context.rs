//! Montage view context (L10-I, lane 10 / #2116; IDs I18, I19, I20, I36, I42).
//!
//! `enterMontageView` / `exitMontageView` (`src/js/app.js`, baseline `fbe4b8a3`) reduced to data, over
//! the L10-A segment specs and the admitted StoryBoard / component records. The view is DERIVED from
//! the montage module (one locked Component-instance segment per chain member); it is never stored, so
//! there is no second document. What the swap isolates is the montage-owned state, and that is the
//! only state this kernel holds besides the scalars of the scene view:
//! - [`ViewScalars`]: current frame, total frames, work area, active layer, fps (restored exactly);
//! - [`OwnedFields`]: `markers`, `motionArcs`, `tweenOverrides`, `tweenEasing`, `cameraKeys`, swapped
//!   for the montage module's own values on entry (`m.x || (m.x = default)`) and written back to the
//!   montage on exit (`mWB.x = state.x`) only while that montage still exists.
//!
//! The kernel writes no document: exit RETURNS the [`WriteBack`] the consumer commits through the
//! StoryBoard owner, exactly like `object_context_workspace` reports a component write-back. Guard
//! order, silent no-ops and refusals follow the baseline; a refusal or a timing error changes nothing.
//! Whether a component context is open is the caller's fact (`state.activeSymbolId`).
//!
//! [`admit_shot_source`] is the StoryBoard "Components only" rule (CLAUDE.md section 8): a shot is a
//! component, and a flat layer is refused here; turning it into a component is the explicit lane 4
//! convert request, never an implicit side effect of placing a shot.
//!
//! Not here (remaining obligations, see HANDOFF): the native storage of the montage-owned fields (the
//! L04-G StoryboardModule has no `markers` / `cameraKeys` member, so they enter as [`StoredOwned`]),
//! hosting the segment instances in a native context (the L04-G instance record admits only the scene
//! root), the tab strip, and the Paper presentation (layer opacity 0.25, redraws).
use crate::evaluation::object_montage_layer::{MontageLayerError, ResolvedMontage};
use crate::evaluation::object_montage_timing::{
    montage_view_segments, ChainMember, MontageTimingError, MontageViewSpec,
};
use crate::object_document::object_component_record_kernels::object_component_records::ComponentSet;
use crate::object_document::object_component_record_kernels::object_storyboard_records::StoryboardRecord;
use serde_json::{json, Value};

pub(crate) const KEY_CLOSE_COMPONENT_FIRST: &str = "toastCloseComponentFirst";
pub(crate) const KEY_CLOSE_MONTAGE_FIRST: &str = "toastCloseMontageFirst";
pub(crate) const KEY_MONTAGE_EMPTY: &str = "hsMontageEmpty";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ShotRefusal {
    CloseComponentFirst,
    CloseMontageFirst,
    EmptyMontage,
    /// A shot names a symbol that is no component.
    NotAComponent,
    /// A shot names a layer that is not a component instance (flat layer).
    FlatLayer,
}

impl ShotRefusal {
    /// The baseline toast key, `None` for the refusals the baseline never printed.
    pub(crate) fn toast_key(self) -> Option<&'static str> {
        match self {
            Self::CloseComponentFirst => Some(KEY_CLOSE_COMPONENT_FIRST),
            Self::CloseMontageFirst => Some(KEY_CLOSE_MONTAGE_FIRST),
            Self::EmptyMontage => Some(KEY_MONTAGE_EMPTY),
            Self::NotAComponent | Self::FlatLayer => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ShotContextError {
    Montage(MontageLayerError),
    Timing(MontageTimingError),
}

/// The scalars `_montageViewSnapshot` keeps (`activeLayerIdx` is an index of the live layer list).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ViewScalars {
    pub(crate) current_frame: i64,
    pub(crate) total_frames: i64,
    pub(crate) wa_in: i64,
    pub(crate) wa_out: i64,
    pub(crate) active_layer: usize,
    pub(crate) fps: f64,
}

/// The five fields the montage owns, as opaque values (no owner of their semantics lives here).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct OwnedFields {
    pub(crate) markers: Value,
    pub(crate) motion_arcs: Value,
    pub(crate) tween_overrides: Value,
    pub(crate) tween_easing: Value,
    pub(crate) camera_keys: Value,
}

impl OwnedFields {
    /// The defaults `enterMontageView` assigns to an absent field: `[]` / `{}`.
    pub(crate) fn empty() -> Self {
        Self {
            markers: json!([]),
            motion_arcs: json!({}),
            tween_overrides: json!({}),
            tween_easing: json!({}),
            camera_keys: json!([]),
        }
    }
}

/// What the montage module stores (`None` is JavaScript `undefined`).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct StoredOwned {
    pub(crate) markers: Option<Value>,
    pub(crate) motion_arcs: Option<Value>,
    pub(crate) tween_overrides: Option<Value>,
    pub(crate) tween_easing: Option<Value>,
    pub(crate) camera_keys: Option<Value>,
}

/// JavaScript truthiness of a stored value: `m.x || (m.x = default)` keeps only truthy values.
fn js_truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(flag) => *flag,
        Value::Number(number) => number.as_f64().is_some_and(|n| n != 0.0 && !n.is_nan()),
        Value::String(text) => !text.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}

fn kept(stored: &Option<Value>, default: Value) -> Value {
    match stored {
        Some(value) if js_truthy(value) => value.clone(),
        _ => default,
    }
}

impl StoredOwned {
    /// The five fields as `enterMontageView` binds them.
    pub(crate) fn resolved(&self) -> OwnedFields {
        let empty = OwnedFields::empty();
        OwnedFields {
            markers: kept(&self.markers, empty.markers),
            motion_arcs: kept(&self.motion_arcs, empty.motion_arcs),
            tween_overrides: kept(&self.tween_overrides, empty.tween_overrides),
            tween_easing: kept(&self.tween_easing, empty.tween_easing),
            camera_keys: kept(&self.camera_keys, empty.camera_keys),
        }
    }
}

struct Inside {
    montage_id: String,
    saved_view: ViewScalars,
    saved_owned: OwnedFields,
}

/// The live scene view plus, while a montage view is open, what to restore on exit.
pub(crate) struct ShotWorkspace {
    view: ViewScalars,
    owned: OwnedFields,
    inside: Option<Inside>,
}

/// What entering built; the consumer instantiates the segments.
#[derive(Clone, Debug)]
pub(crate) struct EnteredView {
    pub(crate) montage_id: String,
    pub(crate) spec: MontageViewSpec,
}

#[derive(Clone, Debug)]
pub(crate) enum EnterOutcome {
    /// A silent no-op: re-entering the open view, or an unknown montage.
    NoOp,
    Refused(ShotRefusal),
    Entered(EnteredView),
}

/// The values to store back on the montage module (the consumer commits them).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct WriteBack {
    pub(crate) montage_id: String,
    pub(crate) owned: OwnedFields,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ExitOutcome {
    NoOp,
    Refused(ShotRefusal),
    /// `write_back` is `None` when the montage no longer exists (`if (mWB)`).
    Exited {
        write_back: Option<WriteBack>,
    },
}

impl ShotWorkspace {
    pub(crate) fn new(view: ViewScalars, owned: OwnedFields) -> Self {
        Self {
            view,
            owned,
            inside: None,
        }
    }
    pub(crate) fn view(&self) -> &ViewScalars {
        &self.view
    }
    pub(crate) fn view_mut(&mut self) -> &mut ViewScalars {
        &mut self.view
    }
    pub(crate) fn owned(&self) -> &OwnedFields {
        &self.owned
    }
    pub(crate) fn owned_mut(&mut self) -> &mut OwnedFields {
        &mut self.owned
    }
    /// `state.activeMontageViewId`.
    pub(crate) fn active_montage_view(&self) -> Option<&str> {
        self.inside
            .as_ref()
            .map(|inside| inside.montage_id.as_str())
    }

    /// The guards that precede any montage lookup, in the baseline's order.
    pub(crate) fn precheck(&self, component_open: bool, montage_id: &str) -> Option<EnterOutcome> {
        if component_open {
            return Some(EnterOutcome::Refused(ShotRefusal::CloseComponentFirst));
        }
        match &self.inside {
            Some(inside) if inside.montage_id == montage_id => Some(EnterOutcome::NoOp),
            Some(_) => Some(EnterOutcome::Refused(ShotRefusal::CloseMontageFirst)),
            None => None,
        }
    }

    /// `enterMontageView`. `montage` is `None` when the id names no montage. The segment specs are
    /// built before anything changes, so a timing error leaves the workspace untouched.
    pub(crate) fn enter(
        &mut self,
        component_open: bool,
        montage_id: &str,
        montage: Option<(&[ChainMember], &StoredOwned)>,
    ) -> Result<EnterOutcome, MontageTimingError> {
        if let Some(outcome) = self.precheck(component_open, montage_id) {
            return Ok(outcome);
        }
        let (members, stored) = match montage {
            Some(found) => found,
            None => return Ok(EnterOutcome::NoOp),
        };
        if members.is_empty() {
            return Ok(EnterOutcome::Refused(ShotRefusal::EmptyMontage));
        }
        let spec = montage_view_segments(members)?;
        let last = spec
            .total_frames
            .checked_sub(1)
            .ok_or(MontageTimingError::Overflow)?;
        let saved_view = self.view.clone();
        let saved_owned = std::mem::replace(&mut self.owned, stored.resolved());
        self.view = ViewScalars {
            current_frame: 0,
            total_frames: spec.total_frames,
            wa_in: 0,
            wa_out: last,
            active_layer: 0,
            fps: saved_view.fps,
        };
        self.inside = Some(Inside {
            montage_id: montage_id.to_string(),
            saved_view,
            saved_owned,
        });
        Ok(EnterOutcome::Entered(EnteredView {
            montage_id: montage_id.to_string(),
            spec,
        }))
    }

    /// `exitMontageView`. `montage_exists` is whether the montage module is still there.
    pub(crate) fn exit(&mut self, component_open: bool, montage_exists: bool) -> ExitOutcome {
        if self.inside.is_none() {
            return ExitOutcome::NoOp;
        }
        if component_open {
            return ExitOutcome::Refused(ShotRefusal::CloseComponentFirst);
        }
        let inside = match self.inside.take() {
            Some(inside) => inside,
            None => return ExitOutcome::NoOp,
        };
        let live = std::mem::replace(&mut self.owned, inside.saved_owned);
        self.view = inside.saved_view;
        ExitOutcome::Exited {
            write_back: montage_exists.then(|| WriteBack {
                montage_id: inside.montage_id,
                owned: live,
            }),
        }
    }
}

/// `enterMontageView` over the real records: guards first, then the montage and its chain.
pub(crate) fn enter_from_records(
    workspace: &mut ShotWorkspace,
    component_open: bool,
    storyboard: &StoryboardRecord,
    components: &ComponentSet,
    stored: &StoredOwned,
    montage_id: &str,
) -> Result<EnterOutcome, ShotContextError> {
    if let Some(outcome) = workspace.precheck(component_open, montage_id) {
        return Ok(outcome);
    }
    let montage = ResolvedMontage::resolve(storyboard, components, montage_id)
        .map_err(ShotContextError::Montage)?;
    match montage {
        None => Ok(EnterOutcome::NoOp),
        Some(montage) => {
            let members = montage.members().map_err(ShotContextError::Montage)?;
            workspace
                .enter(
                    component_open,
                    montage_id,
                    Some((members.as_slice(), stored)),
                )
                .map_err(ShotContextError::Timing)
        }
    }
}

/// What a shot is made from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ShotSource<'a> {
    Symbol(&'a str),
    InstanceLayer {
        context_id: &'a str,
        layer_uid: &'a str,
    },
}

/// The symbol a shot would play, or why it cannot be a shot.
pub(crate) fn admit_shot_source(
    components: &ComponentSet,
    source: ShotSource<'_>,
) -> Result<String, ShotRefusal> {
    match source {
        ShotSource::Symbol(symbol_id) => components
            .component(symbol_id)
            .map(|component| component.symbol_id().to_string())
            .ok_or(ShotRefusal::NotAComponent),
        ShotSource::InstanceLayer {
            context_id,
            layer_uid,
        } => components
            .instance(context_id, layer_uid)
            .map(|instance| instance.symbol_id().to_string())
            .ok_or(ShotRefusal::FlatLayer),
    }
}

#[cfg(test)]
#[path = "../tests/object_shot_context_rows.rs"]
mod rows;

#[cfg(test)]
#[path = "../tests/object_shot_context.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/object_shot_context_controls.rs"]
mod controls;
