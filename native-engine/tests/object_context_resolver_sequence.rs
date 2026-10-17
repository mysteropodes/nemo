//! Replays the oracle's enter -> edit -> exit -> enter scripts (L04-J, F14): the context switches
//! go through the L04-H workspace kernel, the edits mutate the native component records, and
//! after every settled step the resolver must agree with the state the VERBATIM baseline
//! `enterSymbol`/`exitToScene` left in `state.symbols` (rows `V`/`VL`). Stable identities
//! captured before the edits are resolved again after every switch.
use super::world::{
    fill_hex, inventory, layer_orders, rows, settled_steps, step_rows, world, World,
};
use super::*;
use crate::evaluation::object_component_kernels::object_context_selection::{
    resolve_selection, workspace_catalog, SelectionError, SelectionRef,
};
use crate::evaluation::object_component_kernels::object_context_workspace::{
    EnterOutcome, ExitOutcome, SelectionTarget, WorkspaceState, WorkspaceView,
};
use crate::object_document::object_component_record_kernels::object_component_records::ComponentRecord;
use crate::object_document::{Presence, SolidFill};
use serde_json::json;

fn solid(hex: &str) -> SolidFill {
    let channel = |i: usize| {
        f64::from(u8::from_str_radix(&hex[1 + 2 * i..3 + 2 * i], 16).expect("hex digits")) / 255.0
    };
    serde_json::from_value(json!({
        "kind": "solid", "r": channel(0), "g": channel(1), "b": channel(2), "a": 1
    }))
    .expect("solid fill")
}

/// One editing session: the native world, the workspace kernel and its live selection.
pub(crate) struct Session {
    pub(crate) world: World,
    pub(crate) ws: WorkspaceState,
    pub(crate) live: Vec<SelectionTarget>,
}

impl Session {
    pub(crate) fn new() -> Self {
        let view = WorkspaceView {
            current_frame: 0,
            wa_in: 0,
            wa_out: 23,
            active_layer_idx: 0,
            total_frames: 24,
            fps: 24.0,
        };
        Self {
            world: world("D"),
            ws: WorkspaceState::new(view),
            live: Vec::new(),
        }
    }

    fn component_mut(&mut self, symbol_id: &str) -> &mut ComponentRecord {
        self.world
            .set
            .components
            .iter_mut()
            .find(|component| component.symbol_id == symbol_id)
            .expect("component")
    }

    /// One edit-script op, exactly as the `T` rows spell it.
    pub(crate) fn apply(&mut self, op: &[&str]) {
        match op[0] {
            "enter" => {
                let catalog = workspace_catalog(&self.world.source());
                let outcome = self.ws.enter_context(&catalog, &mut self.live, op[1], None);
                assert_eq!(outcome, EnterOutcome::Entered);
                assert_eq!(self.ws.active_context_id(), Some(op[1]));
            }
            "exit" => {
                let catalog = workspace_catalog(&self.world.source());
                let active = self.ws.active_context_id().map(str::to_string);
                match self.ws.exit_context(&catalog, &mut self.live) {
                    ExitOutcome::Exited { context_id, .. } => assert_eq!(Some(context_id), active),
                    ExitOutcome::NotInContext => panic!("exit outside a context"),
                }
                assert!(self.live.is_empty(), "the switch clears the live selection");
            }
            "setFill" => {
                let (layer, frame, stroke) = (op[2], op[3].parse::<u32>().unwrap(), op[4]);
                let fill = solid(op[5]);
                let mut hit = 0;
                for record in self.component_mut(op[1]).objects.iter_mut() {
                    let target = &record.target;
                    let wanted = target.layer_uid == layer
                        && target.frame_scope.frame == frame
                        && target.stroke_id == stroke;
                    if wanted {
                        record.fill = Presence::Present(Some(fill.clone()));
                        hit += 1;
                    }
                }
                assert_eq!(hit, 1);
            }
            "addStroke" => {
                let component = self.component_mut(op[1]);
                let mut record = component
                    .objects
                    .iter()
                    .find(|record| record.target.stroke_id == op[6])
                    .expect("source stroke")
                    .clone();
                record.target.layer_uid = op[2].to_string();
                record.target.frame_scope.frame = op[3].parse().unwrap();
                record.target.stroke_id = op[4].to_string();
                record.fill = Presence::Present(Some(solid(op[5])));
                component.objects.push(record);
            }
            "delStroke" => {
                let (layer, frame, stroke) = (op[2], op[3].parse::<u32>().unwrap(), op[4]);
                let component = self.component_mut(op[1]);
                let before = component.objects.len();
                component.objects.retain(|record| {
                    let target = &record.target;
                    !(target.layer_uid == layer
                        && target.frame_scope.frame == frame
                        && target.stroke_id == stroke)
                });
                assert_eq!(component.objects.len(), before - 1);
            }
            "reorder" => self.component_mut(op[1]).layers.swap(0, 1),
            "replace" => {
                let component = self.component_mut(op[1]);
                let layers = component.layers.clone();
                let objects = component.objects.clone();
                component.layers = layers;
                component.objects = objects;
            }
            other => panic!("unknown op {other}"),
        }
    }

    /// The resolver agrees with the baseline state of this settled step, row by row.
    pub(crate) fn check(&self, scenario: &str, step: usize) {
        let expected = step_rows(scenario, step);
        assert!(!expected.is_empty());
        assert_eq!(
            inventory(&self.world.set),
            expected,
            "{scenario} step {step}"
        );
        let source = self.world.source();
        for row in &expected {
            let frame: u32 = row[2].parse().unwrap();
            let context = source.context(&row[0]).unwrap();
            let identity = ElementIdentity::new(&row[0], &row[1], &row[3]).unwrap();
            let hit = context
                .element_at(&identity, frame)
                .unwrap_or_else(|error| panic!("{row:?}: {error:?}"));
            assert_eq!(fill_hex(hit.record), row[4], "{row:?}");
        }
        for (symbol, csv) in layer_orders(scenario, step) {
            let context = source.context(&symbol).unwrap();
            let uids: Vec<&str> = context
                .layers()
                .iter()
                .map(ObjectLayer::layer_uid)
                .collect();
            assert_eq!(uids.join(","), csv);
            for (position, uid) in csv.split(',').enumerate() {
                assert_eq!(context.layer(uid).unwrap().0, position);
            }
            let wanted = expected.iter().filter(|row| row[0] == symbol).count();
            assert_eq!(context.objects().len(), wanted);
        }
    }
}

/// Replays a scenario; `visit` runs at the initial state (step 0) and after every settled step.
pub(crate) fn replay(scenario: &str, mut visit: impl FnMut(usize, &Session)) {
    let mut session = Session::new();
    visit(0, &session);
    for row in rows("T").iter().filter(|row| row[1] == scenario) {
        session.apply(&row[4..]);
        if row[3] == "1" {
            visit(row[2].parse().unwrap(), &session);
        }
    }
}

#[test]
fn the_edit_script_replays_every_settled_baseline_step() {
    assert_eq!(settled_steps("edits"), vec![0, 7, 8, 10]);
    let mut visited = Vec::new();
    replay("edits", |step, session| {
        session.check("edits", step);
        visited.push(step);
    });
    assert_eq!(visited, vec![0, 7, 8, 10]);
}

#[test]
fn the_two_context_script_replays_every_settled_baseline_step() {
    assert_eq!(settled_steps("twoContexts"), vec![0, 3, 4, 6]);
    let mut visited = Vec::new();
    replay("twoContexts", |step, session| {
        session.check("twoContexts", step);
        visited.push(step);
    });
    assert_eq!(visited, vec![0, 3, 4, 6]);
}

#[test]
fn an_edit_in_one_context_never_reaches_the_look_alike_in_the_other() {
    let fills = |session: &Session| {
        let source = session.world.source();
        let read = |sym: &str| {
            let id = ElementIdentity::new(sym, "ly_comp_ia", "s_comp_a_1").unwrap();
            fill_hex(
                source
                    .context(sym)
                    .unwrap()
                    .element_at(&id, 1)
                    .unwrap()
                    .record,
            )
        };
        (read("sym_comp"), read("sym_comp_b"))
    };
    let mut seen = Vec::new();
    replay("twoContexts", |step, session| {
        seen.push((step, fills(session)))
    });
    let orange = "#ff9500".to_string();
    assert_eq!(seen[0], (0, (orange.clone(), orange.clone())));
    assert_eq!(seen[1], (3, ("#ff0000".to_string(), orange.clone())));
    assert_eq!(seen[2], (4, ("#ff0000".to_string(), orange)));
    assert_eq!(seen[3], (6, ("#ff0000".to_string(), "#0000ff".to_string())));
}

#[test]
fn selected_identities_survive_enter_edit_exit_enter() {
    let mut session = Session::new();
    let mut saved: Vec<ElementIdentity> = Vec::new();
    for row in rows("T").iter().filter(|row| row[1] == "edits") {
        session.apply(&row[4..]);
        if row[2] == "1" {
            // Selected inside the entered component, before any edit.
            for (layer, stroke) in [
                ("ly_comp_ia", "s_comp_a_0"),
                ("ly_comp_ia", "s_comp_a_3"),
                ("ly_comp_ib", "s_comp_b_0"),
                ("ly_comp_ia", "s_comp_a_4"),
            ] {
                let target = SelectionTarget {
                    context_id: "sym_comp".to_string(),
                    layer_uid: layer.to_string(),
                    stroke_id: Some(stroke.to_string()),
                };
                let source = session.world.source();
                match resolve_selection(&source, session.ws.active_context_id(), &target) {
                    Ok(SelectionRef::Element(identity)) => saved.push(identity),
                    other => panic!("not an element selection: {other:?}"),
                }
                session.live.push(target);
            }
            assert_eq!(session.live.len(), 4);
        }
        if row[3] == "1" {
            let step: usize = row[2].parse().unwrap();
            session.check("edits", step);
            let source = session.world.source();
            let context = source.context("sym_comp").unwrap();
            let rows_now = step_rows("edits", step);
            for identity in &saved {
                let mut expected: Vec<(u32, String)> = rows_now
                    .iter()
                    .filter(|r| {
                        r[0] == identity.context_id()
                            && r[1] == identity.layer_uid()
                            && r[3] == identity.stroke_id()
                    })
                    .map(|r| (r[2].parse().unwrap(), r[4].clone()))
                    .collect();
                expected.sort();
                let frames = context.authored_frames(identity);
                assert_eq!(frames, expected.iter().map(|(f, _)| *f).collect::<Vec<_>>());
                for (frame, fill) in &expected {
                    let hit = context.element_at(identity, *frame).unwrap();
                    assert_eq!(&fill_hex(hit.record), fill);
                }
            }
        }
    }
    // After the whole script (step 10): the element keyed at 0 and 5 spans both drawings, the
    // deleted element is gone with a typed error, and the others kept their baseline fill.
    let source = session.world.source();
    let context = source.context("sym_comp").unwrap();
    assert_eq!(saved.len(), 4);
    assert_eq!(context.authored_frames(&saved[0]), vec![0, 5]);
    assert_eq!(
        fill_hex(context.element_at(&saved[0], 0).unwrap().record),
        "#ff9500"
    );
    assert_eq!(
        fill_hex(context.element_at(&saved[0], 5).unwrap().record),
        "#abcdef"
    );
    assert_eq!(context.authored_frames(&saved[1]), vec![3]);
    assert_eq!(context.authored_frames(&saved[2]), vec![0]);
    assert!(context.authored_frames(&saved[3]).is_empty());
    assert_eq!(
        context.element_at(&saved[3], 4).err(),
        Some(ResolveError::UnknownStroke)
    );
}

#[test]
fn a_live_selection_is_cleared_by_the_switch_and_stale_after_it() {
    let mut session = Session::new();
    session.apply(&["enter", "sym_comp"]);
    let target = SelectionTarget {
        context_id: "sym_comp".to_string(),
        layer_uid: "ly_comp_ia".to_string(),
        stroke_id: Some("s_comp_a_2".to_string()),
    };
    let first = resolve_selection(&session.world.source(), Some("sym_comp"), &target).unwrap();
    session.live.push(target.clone());
    session.apply(&["exit"]);
    assert!(session.live.is_empty());
    let stale = resolve_selection(
        &session.world.source(),
        session.ws.active_context_id(),
        &target,
    );
    assert_eq!(
        stale,
        Err(SelectionError::NotInActiveContext {
            selected: "sym_comp".to_string(),
            active: "scene-root".to_string()
        })
    );
    session.apply(&["enter", "sym_comp"]);
    let again = resolve_selection(
        &session.world.source(),
        session.ws.active_context_id(),
        &target,
    )
    .unwrap();
    assert_eq!(again, first);
    // Entering the other component makes the same stroke id a different element.
    session.apply(&["exit"]);
    session.apply(&["enter", "sym_comp_b"]);
    let other = SelectionTarget {
        context_id: "sym_comp_b".to_string(),
        ..target
    };
    let b = resolve_selection(
        &session.world.source(),
        session.ws.active_context_id(),
        &other,
    )
    .unwrap();
    assert_ne!(b, first);
    assert!(
        matches!(b, SelectionRef::Element(ref id) if id.context_id() == "sym_comp_b" && id.stroke_id() == "s_comp_a_2")
    );
}

#[test]
fn layer_positions_move_but_identities_do_not() {
    let mut before = None;
    let mut after = None;
    replay("edits", |step, session| {
        let source = session.world.source();
        let context = source.context("sym_comp").unwrap();
        let id = ElementIdentity::new("sym_comp", "ly_comp_ia", "s_comp_a_6").unwrap();
        let hit = context.element_at(&id, 6).unwrap();
        let summary = (
            hit.layer_index,
            hit.object_index,
            hit.record.target.clone(),
            fill_hex(hit.record),
        );
        if step == 0 {
            before = Some(summary);
        } else if step == 7 {
            after = Some(summary);
        }
    });
    let (before, after) = (before.unwrap(), after.unwrap());
    assert_eq!((before.0, before.1), (0, 6));
    // The inner layers were swapped and one earlier record deleted: both positions moved.
    assert_eq!((after.0, after.1), (1, 5));
    // The identity, the record's target and its fill did not.
    assert_eq!(before.2, after.2);
    assert_eq!(before.3, after.3);
}
