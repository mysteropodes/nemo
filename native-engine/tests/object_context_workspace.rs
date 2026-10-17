//! Oracle tests for the workspace context-switching kernel (L04-H, D33 + F14).
//!
//! `fixtures/object-context-workspace/oracle.txt` (`ORACLE`) was produced by
//! `tools/gen_workspace_oracle.cjs` of the L04-H leaf, which cuts `enterSymbol`,
//! `exitToScene` and `closeSymbolTab` out of `src/js/app.js`, `renderSymbolTabs`
//! out of `src/js/timeline.js`, the project reset statements out of
//! `src/js/project.js` and the playhead jump of `enterComponentLayer` out of
//! `src/js/motion.js` at baseline `fbe4b8a3`, and runs them verbatim in a Node
//! `vm` over a fake DOM. Tab clicks go through the REAL click handlers that
//! `renderSymbolTabs` registers. The header carries the blob hashes and the row
//! grammar. Rows: `A` initial view, `C` catalog, `S` one executed step.

use super::{
    CloseOutcome, ContextInfo, ContextWriteback, EnterOutcome, ExitOutcome, SelectionTarget,
    TabClick, TabTarget, WorkspaceState, WorkspaceView,
};
use crate::evaluation::SUPPORTED_CONTEXT_ID;

pub(crate) const ORACLE: &str = include_str!("fixtures/object-context-workspace/oracle.txt");

pub(crate) struct Scenario {
    pub(crate) id: String,
    pub(crate) view: WorkspaceView,
    pub(crate) tabs: Vec<String>,
    pub(crate) sel: usize,
    pub(crate) catalog: Vec<ContextInfo>,
    pub(crate) steps: Vec<(String, String)>,
}

fn kv<'a>(tokens: &[&'a str], key: &str) -> &'a str {
    let prefix = format!("{key}=");
    tokens
        .iter()
        .find_map(|t| t.strip_prefix(prefix.as_str()))
        .unwrap_or_else(|| panic!("row has no {key}"))
}

fn int(tok: &str) -> i64 {
    tok.parse::<i64>().unwrap_or_else(|_| panic!("int {tok}"))
}

fn csv(tok: &str) -> Vec<String> {
    if tok == "-" {
        Vec::new()
    } else {
        tok.split(',').map(|s| s.to_string()).collect()
    }
}

pub(crate) fn scenarios() -> Vec<Scenario> {
    let mut out: Vec<Scenario> = Vec::new();
    for line in ORACLE.lines().filter(|l| !l.starts_with('#')) {
        let tokens: Vec<&str> = line.split(' ').collect();
        match tokens[0] {
            "A" => {
                let wa: Vec<&str> = kv(&tokens, "wa").split(',').collect();
                out.push(Scenario {
                    id: tokens[1].to_string(),
                    view: WorkspaceView {
                        current_frame: int(kv(&tokens, "f")),
                        wa_in: int(wa[0]),
                        wa_out: int(wa[1]),
                        active_layer_idx: int(kv(&tokens, "li")),
                        total_frames: int(kv(&tokens, "tot")),
                        fps: kv(&tokens, "fps").parse::<f64>().unwrap(),
                    },
                    tabs: csv(kv(&tokens, "tabs")),
                    sel: int(kv(&tokens, "sel")) as usize,
                    catalog: Vec::new(),
                    steps: Vec::new(),
                });
            }
            "C" => {
                let s = out.last_mut().unwrap();
                assert_eq!(s.id, tokens[1]);
                for item in tokens[2].split(',') {
                    let p: Vec<&str> = item.split(':').collect();
                    s.catalog.push(ContextInfo {
                        context_id: p[0].to_string(),
                        total_frames: int(p[1]),
                        fps: if p[2] == "-" {
                            None
                        } else {
                            Some(p[2].parse::<f64>().unwrap())
                        },
                    });
                }
            }
            "S" => {
                let (left, right) = line.split_once(" => ").expect("step row");
                let l: Vec<&str> = left.split(' ').collect();
                let s = out.last_mut().unwrap();
                assert_eq!(s.id, l[1]);
                s.steps.push((l[3].to_string(), right.to_string()));
            }
            other => panic!("row tag {other}"),
        }
    }
    out
}

/// The workspace, the document's context catalog and the selection of one run.
pub(crate) struct Env {
    pub(crate) ws: WorkspaceState,
    pub(crate) catalog: Vec<ContextInfo>,
    pub(crate) sel: Vec<SelectionTarget>,
}

pub(crate) fn make_sel(n: usize, context: &str) -> Vec<SelectionTarget> {
    (0..n)
        .map(|i| SelectionTarget {
            context_id: context.to_string(),
            layer_uid: format!("layer-{i}"),
            stroke_id: Some(format!("stroke-{i}")),
        })
        .collect()
}

pub(crate) fn new_env(s: &Scenario) -> Env {
    Env {
        ws: WorkspaceState::with_open_tabs(s.view.clone(), s.tabs.clone()).expect("scenario tabs"),
        catalog: s.catalog.clone(),
        sel: make_sel(s.sel, SUPPORTED_CONTEXT_ID),
    }
}

/// The operations a run drives. The real kernel implements them; the controls
/// implement the same trait with one deliberate deviation each.
pub(crate) trait Driver {
    fn enter(&self, env: &mut Env, id: &str, frame: Option<i64>) -> EnterOutcome;
    fn exit(&self, env: &mut Env) -> ExitOutcome;
    fn close(&self, env: &mut Env, id: &str) -> CloseOutcome;
    fn click(&self, env: &mut Env, target: TabTarget<'_>) -> TabClick {
        match target {
            TabTarget::Scene => TabClick::Exited(self.exit(env)),
            TabTarget::Context(id) => TabClick::Entered(self.enter(env, id, None)),
        }
    }
    fn reset(&self, env: &mut Env) -> ExitOutcome {
        let exit = self.exit(env);
        env.ws.open_tabs.clear();
        exit
    }
}

pub(crate) struct Real;

impl Driver for Real {
    fn enter(&self, env: &mut Env, id: &str, frame: Option<i64>) -> EnterOutcome {
        env.ws.enter_context(&env.catalog, &mut env.sel, id, frame)
    }
    fn exit(&self, env: &mut Env) -> ExitOutcome {
        env.ws.exit_context(&env.catalog, &mut env.sel)
    }
    fn close(&self, env: &mut Env, id: &str) -> CloseOutcome {
        env.ws.close_tab(&env.catalog, &mut env.sel, id)
    }
    fn click(&self, env: &mut Env, target: TabTarget<'_>) -> TabClick {
        env.ws.click_tab(&env.catalog, &mut env.sel, target)
    }
    fn reset(&self, env: &mut Env) -> ExitOutcome {
        env.ws.reset_for_project_replace(&env.catalog, &mut env.sel)
    }
}

fn enter_label(o: &EnterOutcome) -> &'static str {
    match o {
        EnterOutcome::Entered => "entered",
        EnterOutcome::AlreadyActive => "already",
        EnterOutcome::RefusedNested { .. } => "nested",
        EnterOutcome::UnknownContext => "unknown",
    }
}

fn exit_label(exited: bool) -> &'static str {
    if exited {
        "exited"
    } else {
        "idle"
    }
}

/// What a step reports besides the state: the thumbnail to invalidate and the
/// write-back of the exited context.
#[derive(Default)]
struct Effects {
    thumb: Option<String>,
    wb: Option<ContextWriteback>,
}

fn take_exit(env: &mut Env, exit: &ExitOutcome, fx: &mut Effects) -> bool {
    match exit {
        ExitOutcome::NotInContext => false,
        ExitOutcome::Exited {
            context_id,
            writeback,
        } => {
            fx.thumb = Some(context_id.clone());
            if let Some(wb) = writeback {
                fx.wb = Some(wb.clone());
                if let Some(info) = env.catalog.iter_mut().find(|c| &c.context_id == context_id) {
                    info.total_frames = wb.total_frames;
                    info.fps = Some(wb.fps);
                }
            }
            true
        }
    }
}

fn parts(op: &str) -> Vec<&str> {
    op.split(':').collect()
}

/// Runs one oracle op and returns the result text in the oracle grammar.
pub(crate) fn step(env: &mut Env, op: &str, d: &dyn Driver) -> String {
    let p = parts(op);
    let mut fx = Effects::default();
    let out: String = match p[0] {
        "E" => enter_label(&d.enter(env, p[1], None)).to_string(),
        "F" => enter_label(&d.enter(env, p[1], Some(int(p[2])))).to_string(),
        "X" => {
            let exit = d.exit(env);
            exit_label(take_exit(env, &exit, &mut fx)).to_string()
        }
        "S" => match d.click(env, TabTarget::Scene) {
            TabClick::Exited(exit) => exit_label(take_exit(env, &exit, &mut fx)).to_string(),
            TabClick::Entered(_) => panic!("scene tab entered a context"),
        },
        "T" | "K" => {
            let tabs = env.ws.visible_tabs(&env.catalog);
            match tabs.get(int(p[1]) as usize) {
                None => "skip".to_string(),
                Some(tab) if p[0] == "T" => match d.click(env, TabTarget::Context(&tab.context_id))
                {
                    TabClick::Entered(o) => enter_label(&o).to_string(),
                    TabClick::Exited(_) => panic!("context tab exited"),
                },
                Some(tab) => {
                    let closed = d.close(env, &tab.context_id);
                    close_label(env, closed, &mut fx)
                }
            }
        }
        "Z" => {
            let closed = d.close(env, p[1]);
            close_label(env, closed, &mut fx)
        }
        "R" => {
            let exit = d.reset(env);
            let exited = take_exit(env, &exit, &mut fx);
            env.catalog.clear();
            format!("reset:{}", exited as u8)
        }
        "V" => {
            let n: Vec<i64> = p[1].split(',').take(5).map(int).collect();
            let fps: f64 = p[1].split(',').nth(5).unwrap().parse().unwrap();
            env.ws.view = WorkspaceView {
                current_frame: n[0],
                wa_in: n[1],
                wa_out: n[2],
                active_layer_idx: n[3],
                total_frames: n[4],
                fps,
            };
            "set".to_string()
        }
        "L" => {
            let ctx = env
                .ws
                .active_context_id()
                .unwrap_or(SUPPORTED_CONTEXT_ID)
                .to_string();
            env.sel = make_sel(int(p[1]) as usize, &ctx);
            "sel".to_string()
        }
        "D" => {
            env.catalog.retain(|c| c.context_id != p[1]);
            "del".to_string()
        }
        other => panic!("op {other}"),
    };
    render(env, &out, &fx)
}

fn close_label(env: &mut Env, closed: CloseOutcome, fx: &mut Effects) -> String {
    let exited = take_exit(env, &closed.exit, fx);
    format!("close:{}:{}", exited as u8, closed.tab_removed as u8)
}

fn render(env: &Env, out: &str, fx: &Effects) -> String {
    let v = &env.ws.view;
    let mut vis = vec![format!(
        "scene{}",
        if env.ws.scene_tab_active() { "*" } else { "" }
    )];
    for t in env.ws.visible_tabs(&env.catalog) {
        vis.push(format!(
            "{}{}",
            t.context_id,
            if t.is_active { "*" } else { "" }
        ));
    }
    let tabs = if env.ws.open_tabs.is_empty() {
        "-".to_string()
    } else {
        env.ws.open_tabs.join(",")
    };
    format!(
        "out={out} act={} tabs={tabs} f={} wa={},{} li={} tot={} fps={} sel={} thumb={} wb={} vis={}",
        env.ws.active_context_id().unwrap_or("-"),
        v.current_frame,
        v.wa_in,
        v.wa_out,
        v.active_layer_idx,
        v.total_frames,
        v.fps,
        env.sel.len(),
        fx.thumb.as_deref().unwrap_or("-"),
        fx.wb.as_ref().map_or("-".to_string(), |w| format!("{}:{}", w.total_frames, w.fps)),
        vis.join(","),
    )
}

/// Replays every scenario with `d`; returns `(rows compared, rows that differ)`.
pub(crate) fn replay(d: &dyn Driver) -> (usize, usize) {
    let (mut total, mut wrong) = (0, 0);
    for s in scenarios() {
        let mut env = new_env(&s);
        for (op, expected) in &s.steps {
            total += 1;
            if &step(&mut env, op, d) != expected {
                wrong += 1;
            }
        }
    }
    (total, wrong)
}

#[test]
fn fixture_world_and_row_counts_are_pinned() {
    let all = scenarios();
    assert_eq!(all.len(), 83);
    assert_eq!(all.iter().map(|s| s.steps.len()).sum::<usize>(), 1292);
    assert_eq!(all[0].id, "basic");
    assert_eq!(all[0].steps.len(), 2);
    let outs = |needle: &str| {
        all.iter()
            .flat_map(|s| s.steps.iter())
            .filter(|(_, r)| r.starts_with(needle))
            .count()
    };
    // The world exercises every outcome class, so a replay cannot pass by never reaching one.
    for needle in [
        "out=entered",
        "out=already",
        "out=nested",
        "out=unknown",
        "out=exited",
        "out=idle",
    ] {
        assert!(outs(needle) > 0, "no {needle} row");
    }
    assert!(outs("out=close:1:1") > 0 && outs("out=close:0:1") > 0 && outs("out=close:0:0") > 0);
    assert!(outs("out=reset:1") > 0 && outs("out=reset:0") > 0);
}

#[test]
fn every_baseline_step_row_is_reproduced() {
    let (total, wrong) = replay(&Real);
    assert_eq!(total, 1292);
    assert_eq!(wrong, 0, "{wrong} of {total} rows differ from the baseline");
}

#[test]
fn invariants_hold_after_every_baseline_step() {
    for s in scenarios() {
        let mut env = new_env(&s);
        let mut last_tabs = env.ws.open_tabs.clone();
        for (op, _) in &s.steps {
            step(&mut env, op, &Real);
            env.ws
                .validate()
                .unwrap_or_else(|e| panic!("{} {op}: {e:?}", s.id));
            // The saved scene view exists exactly while a context is active.
            assert_eq!(env.ws.active.is_some(), env.ws.saved_scene_view().is_some());
            // Tabs only ever grow by entering and shrink by closing or reset: never reorder.
            let kept: Vec<&String> = last_tabs
                .iter()
                .filter(|t| env.ws.open_tabs.contains(t))
                .collect();
            let now: Vec<&String> = env
                .ws
                .open_tabs
                .iter()
                .filter(|t| last_tabs.contains(t))
                .collect();
            assert_eq!(kept, now, "{} {op} reordered tabs", s.id);
            last_tabs = env.ws.open_tabs.clone();
        }
    }
}
