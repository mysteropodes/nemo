//! Oracle rows and replay harness for the montage view context (L10-I, I18/I19/I20/I36/I42).
//!
//! `fixtures/object-shot-context/oracle.txt` (`ORACLE`) was produced by `tools/gen.mjs` of the L10-I
//! leaf (kept outside the repo) from baseline fbe4b8a33e6d5a229f014154df2ba21988a9fabc: it cuts
//! `enterMontageView`, `exitMontageView`, `createUserLayer` (`src/js/app.js`) and `montageById`,
//! `chainMods`, `montageTotal`, `chainModsForView`, `ensureRetime`, `symbolDuration`
//! (`src/js/storyboard.js`) verbatim and runs them in a Node vm (presentation calls stubbed, `SM.t`
//! returns its key). Rows are TAB separated: `W world valid storyboard-json components-json` and
//! `X scenario world step op result state montage-owned`. Step `0` is the initial state; `state` is
//! `act;cur;tot;wa;lay;fps;sel;mk;ma;to;te;ck` (`sel`: `clearSel` calls so far; the five owned fields
//! are the live values); `montage-owned` is the montage module's stored fields (`~` undefined, `-` no
//! module, `*` not compared mid-view because the baseline persists the defaults at once).
use super::{
    enter_from_records, EnterOutcome, ExitOutcome, OwnedFields, ShotWorkspace, StoredOwned,
    ViewScalars,
};
use crate::object_document::object_component_record_kernels::object_component_records::ComponentSet;
use crate::object_document::object_component_record_kernels::object_storyboard_records::StoryboardRecord;
use serde_json::Value;
use std::collections::HashMap;

pub(crate) const ORACLE: &str = include_str!("fixtures/object-shot-context/oracle.txt");

pub(crate) struct World {
    pub(crate) id: &'static str,
    pub(crate) storyboard: StoryboardRecord,
    pub(crate) components: ComponentSet,
}

pub(crate) fn rows(kind: &str) -> Vec<Vec<&'static str>> {
    ORACLE
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| l.split('\t').collect::<Vec<_>>())
        .filter(|f| f[0] == kind)
        .collect()
}

pub(crate) fn worlds() -> Vec<World> {
    rows("W")
        .into_iter()
        .map(|f| World {
            id: f[1],
            storyboard: serde_json::from_str(f[3]).expect("world storyboard"),
            components: serde_json::from_str(f[4]).expect("world components"),
        })
        .collect()
}

/// JavaScript `String(number)` for the values this fixture produces.
pub(crate) fn js_number(value: f64) -> String {
    if value.is_nan() {
        "NaN".to_string()
    } else if value == f64::INFINITY {
        "Infinity".to_string()
    } else if value == f64::NEG_INFINITY {
        "-Infinity".to_string()
    } else {
        format!("{value}")
    }
}

fn hex(text: &str) -> String {
    text.bytes().map(|b| format!("{b:02x}")).collect()
}

pub(crate) fn segments_of(entered: &super::EnteredView) -> String {
    let frames = entered.spec.total_frames.max(0);
    entered
        .spec
        .segments
        .iter()
        .map(|s| {
            format!(
                "{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
                hex(&s.name),
                s.symbol_id,
                s.sym_play_mode,
                js_number(s.sym_speed),
                s.sym_placed_at,
                s.sym_trim_in,
                s.sym_trim_out,
                s.sym_single_frame,
                s.in_point,
                s.out_point,
                u8::from(s.locked),
                u8::from(s.visible),
                frames,
                u8::from(frames > 0)
            )
        })
        .collect::<Vec<_>>()
        .join(";")
}

fn field<'a>(state: &'a str, key: &str) -> &'a str {
    let prefix = format!("{key}=");
    state
        .split(';')
        .find_map(|p| p.strip_prefix(prefix.as_str()))
        .unwrap_or_else(|| panic!("field {key} in {state}"))
}

/// The initial scene view of an `init` row.
pub(crate) fn initial_of(state: &str) -> (ViewScalars, OwnedFields) {
    let wa = field(state, "wa");
    let (wa_in, wa_out) = wa.split_once("..").expect("wa");
    let json = |key: &str| -> Value { serde_json::from_str(field(state, key)).expect("json") };
    (
        ViewScalars {
            current_frame: field(state, "cur").parse().expect("cur"),
            total_frames: field(state, "tot").parse().expect("tot"),
            wa_in: wa_in.parse().expect("wa in"),
            wa_out: wa_out.parse().expect("wa out"),
            active_layer: field(state, "lay").parse().expect("lay"),
            fps: field(state, "fps").parse().expect("fps"),
        },
        OwnedFields {
            markers: json("mk"),
            motion_arcs: json("ma"),
            tween_overrides: json("to"),
            tween_easing: json("te"),
            camera_keys: json("ck"),
        },
    )
}

/// The montage module's stored fields of an `init` row (`~` is undefined).
pub(crate) fn stored_of(text: &str) -> StoredOwned {
    let get = |key: &str| -> Option<Value> {
        match field(text, key) {
            "~" => None,
            json => Some(serde_json::from_str(json).expect("stored json")),
        }
    };
    StoredOwned {
        markers: get("mk"),
        motion_arcs: get("ma"),
        tween_overrides: get("to"),
        tween_easing: get("te"),
        camera_keys: get("ck"),
    }
}

fn stored_text(stored: &StoredOwned) -> String {
    let one = |value: &Option<Value>| match value {
        None => "~".to_string(),
        Some(v) => serde_json::to_string(v).expect("json"),
    };
    format!(
        "mk={};ma={};to={};te={};ck={}",
        one(&stored.markers),
        one(&stored.motion_arcs),
        one(&stored.tween_overrides),
        one(&stored.tween_easing),
        one(&stored.camera_keys)
    )
}

/// How the replay deviates from the consumer contract (the controls turn these on one at a time).
#[derive(Clone, Copy, Default)]
pub(crate) struct Deviation {
    pub(crate) ignore_write_back: bool,
    pub(crate) drop_camera_write_back: bool,
    pub(crate) skip_exit_clear: bool,
    pub(crate) forget_component_flag: bool,
}

/// One scenario replayed on the production kernel plus the consumer duties the oracle implies:
/// per-montage stored fields, the selection clear counter, the open-component flag.
pub(crate) struct Replay<'a> {
    pub(crate) ws: ShotWorkspace,
    world: &'a World,
    storyboard: StoryboardRecord,
    stored: HashMap<String, StoredOwned>,
    component_open: bool,
    sel: usize,
    deviation: Deviation,
}

impl<'a> Replay<'a> {
    pub(crate) fn new(world: &'a World, init: &[&str], deviation: Deviation) -> Self {
        let (view, owned) = initial_of(init[6]);
        let mut stored = HashMap::new();
        if init[7] != "-" {
            stored.insert("mt".to_string(), stored_of(init[7]));
        }
        Self {
            ws: ShotWorkspace::new(view, owned),
            world,
            storyboard: world.storyboard.clone(),
            stored,
            component_open: false,
            sel: 0,
            deviation,
        }
    }

    fn module_exists(&self, id: &str) -> bool {
        self.storyboard
            .modules()
            .iter()
            .any(|m| m.kind() == "montage" && m.id() == id)
    }

    /// Runs one `op` (JSON text of the row) and returns the result column.
    pub(crate) fn step(&mut self, op: &str) -> String {
        let op: Vec<Value> = serde_json::from_str(op).expect("op");
        let name = op[0].as_str().expect("op name");
        match name {
            "enter" => self.enter(op[1].as_str().expect("id")),
            "exit" => self.exit(),
            "comp" => {
                self.component_open = op[1] == 1;
                "ok".to_string()
            }
            "edit" => {
                let live = self.ws.owned_mut();
                let (slot, val) = (op[1].as_str().expect("field"), op[2].as_str().expect("val"));
                match slot {
                    "mk" => live.markers.as_array_mut().expect("array").push(val.into()),
                    "ck" => live
                        .camera_keys
                        .as_array_mut()
                        .expect("array")
                        .push(val.into()),
                    "ma" => {
                        let _ = live
                            .motion_arcs
                            .as_object_mut()
                            .expect("object")
                            .insert(val.into(), 1.into());
                    }
                    "to" => {
                        let _ = live
                            .tween_overrides
                            .as_object_mut()
                            .expect("object")
                            .insert(val.into(), 1.into());
                    }
                    "te" => {
                        let _ = live
                            .tween_easing
                            .as_object_mut()
                            .expect("object")
                            .insert(val.into(), 1.into());
                    }
                    other => panic!("edit field {other}"),
                }
                "ok".to_string()
            }
            "replace" => {
                let live = self.ws.owned_mut();
                let value = op[2].clone();
                match op[1].as_str().expect("field") {
                    "mk" => live.markers = value,
                    "ck" => live.camera_keys = value,
                    "ma" => live.motion_arcs = value,
                    "to" => live.tween_overrides = value,
                    "te" => live.tween_easing = value,
                    other => panic!("replace field {other}"),
                }
                "ok".to_string()
            }
            "set" => {
                let view = self.ws.view_mut();
                match op[1].as_str().expect("scalar") {
                    "cur" => view.current_frame = op[2].as_i64().expect("n"),
                    "fps" => view.fps = op[2].as_f64().expect("n"),
                    "tot" => view.total_frames = op[2].as_i64().expect("n"),
                    "lay" => view.active_layer = op[2].as_u64().expect("n") as usize,
                    "wa" => {
                        view.wa_in = op[2][0].as_i64().expect("n");
                        view.wa_out = op[2][1].as_i64().expect("n");
                    }
                    other => panic!("set {other}"),
                }
                "ok".to_string()
            }
            "del" => {
                let mut value = serde_json::to_value(&self.storyboard).expect("encode");
                value["modules"]
                    .as_array_mut()
                    .expect("modules")
                    .retain(|m| m["id"] != "mt");
                self.storyboard = serde_json::from_value(value).expect("decode");
                "ok".to_string()
            }
            other => panic!("op {other}"),
        }
    }

    fn open_flag(&self) -> bool {
        self.component_open && !self.deviation.forget_component_flag
    }

    fn enter(&mut self, id: &str) -> String {
        let stored = self.stored.get(id).cloned().unwrap_or_default();
        let open = self.open_flag();
        let outcome = enter_from_records(
            &mut self.ws,
            open,
            &self.storyboard,
            &self.world.components,
            &stored,
            id,
        )
        .expect("baseline scenarios are admissible");
        match outcome {
            EnterOutcome::NoOp => "noop".to_string(),
            EnterOutcome::Refused(why) => {
                format!(
                    "toast:{}",
                    why.toast_key().expect("enter refusals have a key")
                )
            }
            EnterOutcome::Entered(entered) => {
                self.sel += 1;
                format!("entered:{}", segments_of(&entered))
            }
        }
    }

    fn exit(&mut self) -> String {
        let active = self.ws.active_montage_view().map(str::to_string);
        let exists = active.as_deref().is_some_and(|id| self.module_exists(id));
        let open = self.open_flag();
        match self.ws.exit(open, exists) {
            ExitOutcome::NoOp => "noop".to_string(),
            ExitOutcome::Refused(why) => {
                format!(
                    "toast:{}",
                    why.toast_key().expect("exit refusals have a key")
                )
            }
            ExitOutcome::Exited { write_back } => {
                if !self.deviation.skip_exit_clear {
                    self.sel += 1;
                }
                if let (Some(wb), false) = (write_back, self.deviation.ignore_write_back) {
                    let camera = if self.deviation.drop_camera_write_back {
                        self.stored
                            .get(&wb.montage_id)
                            .and_then(|s| s.camera_keys.clone())
                    } else {
                        Some(wb.owned.camera_keys)
                    };
                    self.stored.insert(
                        wb.montage_id,
                        StoredOwned {
                            markers: Some(wb.owned.markers),
                            motion_arcs: Some(wb.owned.motion_arcs),
                            tween_overrides: Some(wb.owned.tween_overrides),
                            tween_easing: Some(wb.owned.tween_easing),
                            camera_keys: camera,
                        },
                    );
                }
                "exited".to_string()
            }
        }
    }

    /// The `state` column.
    pub(crate) fn state_text(&self) -> String {
        let view = self.ws.view();
        let live = self.ws.owned();
        let j = |v: &Value| serde_json::to_string(v).expect("json");
        format!(
            "act={};cur={};tot={};wa={}..{};lay={};fps={};sel={};mk={};ma={};to={};te={};ck={}",
            self.ws.active_montage_view().unwrap_or("-"),
            view.current_frame,
            view.total_frames,
            view.wa_in,
            view.wa_out,
            view.active_layer,
            js_number(view.fps),
            self.sel,
            j(&live.markers),
            j(&live.motion_arcs),
            j(&live.tween_overrides),
            j(&live.tween_easing),
            j(&live.camera_keys)
        )
    }

    /// The `montage-owned` column for a step: only after an exit, else `*`.
    pub(crate) fn owned_text(&self, op: &str, result: &str) -> String {
        if !op.starts_with("[\"exit\"") || result != "exited" {
            return "*".to_string();
        }
        if !self.module_exists("mt") {
            return "-".to_string();
        }
        stored_text(&self.stored.get("mt").cloned().unwrap_or_default())
    }
}

/// Every scenario of the oracle replayed under `deviation`: the rows that no longer match.
pub(crate) fn mismatches(deviation: Deviation) -> Vec<String> {
    let all = worlds();
    let mut by_scenario: Vec<Vec<Vec<&str>>> = Vec::new();
    for row in rows("X") {
        let same = by_scenario.last().is_some_and(|list| list[0][1] == row[1]);
        if same {
            by_scenario.last_mut().expect("scenario").push(row);
        } else {
            by_scenario.push(vec![row]);
        }
    }
    let mut bad = Vec::new();
    for list in &by_scenario {
        let world = all.iter().find(|w| w.id == list[0][2]).expect("world");
        let mut replay = Replay::new(world, &list[0], deviation);
        for row in &list[1..] {
            let result = replay.step(row[4]);
            let state = replay.state_text();
            let owned = replay.owned_text(row[4], &result);
            if result != row[5] || state != row[6] || owned != row[7] {
                bad.push(format!("{} step {}", row[1], row[3]));
            }
        }
    }
    bad
}
