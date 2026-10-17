//! Scripted adapter worlds for the link-state oracle (G32, L09-C): token
//! codecs, the platform-mode to typed-outcome mapping, and the `R`/`W` script
//! executors. The platform's one state change is mirrored here exactly as the
//! generator's fake does it: a granted permission prompt makes the handle
//! readable (`mode ok`). Everything else is the real kernel.

use super::super::object_media_link_reopen::{ReopenGate, WebReopenLatch};
use super::controls::Mutation;
use super::{
    FailureReason, LibraryEntry, LinkKey, LinkedRef, PermissionAnswer, ResolveOutcome,
    ResolveStart, ResolveTable,
};
use std::cell::RefCell;
use std::collections::HashMap;

pub(crate) const RLIB: &str =
    "1;/m/a.png;-;-;0;-,1;-;h%2Db;-;0;-,1;/m/a.png;-;-;0;-,1;/m/c.png;-;-;0;-,\
1;-;h%2Dc;-;0;-,0;/m/a.png;-;-;0;-,1;-;h%2De;-;0;-";
pub(crate) const WLIB: &str = "1;-;h%2Dv;u1;0;-,1;-;-;u2;0;-,1;-;h%2Dw;u1;0;-";
pub(crate) const WLIB_BROKEN: &str = "1;-;h%2Dv;u1;1;m,1;-;-;u2;0;-,1;-;h%2Dw;u1;0;-";

pub(crate) fn dec(t: &str) -> Option<String> {
    match t {
        "-" => None,
        "=" => Some(String::new()),
        _ => {
            let (b, mut out, mut i) = (t.as_bytes(), Vec::new(), 0);
            while i < b.len() {
                if b[i] == b'%' {
                    let hex = std::str::from_utf8(&b[i + 1..i + 3]).expect("hex");
                    out.push(u8::from_str_radix(hex, 16).expect("percent byte"));
                    i += 3;
                } else {
                    out.push(b[i]);
                    i += 1;
                }
            }
            Some(String::from_utf8(out).expect("utf8 locator"))
        }
    }
}

pub(crate) fn enc(s: Option<&str>) -> String {
    match s {
        None => "-".to_owned(),
        Some("") => "=".to_owned(),
        Some(text) => text
            .bytes()
            .map(|b| match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'.' | b'_' | b'/' => {
                    (b as char).to_string()
                }
                _ => format!("%{b:02X}"),
            })
            .collect(),
    }
}

pub(crate) fn reason_of(t: &str) -> Option<FailureReason> {
    match t {
        "-" => None,
        "m" => Some(FailureReason::Missing),
        "p" => Some(FailureReason::PermissionNeeded),
        other => panic!("reason token {other}"),
    }
}

pub(crate) fn reason_tok(r: Option<FailureReason>) -> &'static str {
    match r {
        None => "-",
        Some(FailureReason::Missing) => "m",
        Some(FailureReason::PermissionNeeded) => "p",
    }
}

pub(crate) fn entry_of(t: &str) -> LibraryEntry {
    let f: Vec<&str> = t.split(';').collect();
    LibraryEntry {
        linked: f[0] == "1",
        path: dec(f[1]),
        web_handle_id: dec(f[2]),
        layer_uid: dec(f[3]),
        broken: f[4] == "1",
        broken_reason: reason_of(f[5]),
    }
}

pub(crate) fn entries_of(t: &str) -> Vec<LibraryEntry> {
    t.split(',').map(entry_of).collect()
}

pub(crate) fn entries_tok(lib: &[LibraryEntry]) -> String {
    let one = |e: &LibraryEntry| {
        format!(
            "{};{};{};{};{};{}",
            u8::from(e.linked),
            enc(e.path.as_deref()),
            enc(e.web_handle_id.as_deref()),
            enc(e.layer_uid.as_deref()),
            u8::from(e.broken),
            reason_tok(e.broken_reason)
        )
    };
    lib.iter().map(one).collect::<Vec<_>>().join(",")
}

pub(crate) fn flags_tok(lib: &[LibraryEntry]) -> String {
    let one = |e: &LibraryEntry| format!("{}{}", u8::from(e.broken), reason_tok(e.broken_reason));
    lib.iter().map(one).collect::<Vec<_>>().join(",")
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Mode {
    Ok,
    Gone,
    Perm,
    ReadFail,
    Hold,
}

pub(crate) fn mode_of(t: &str) -> Mode {
    match t {
        "ok" => Mode::Ok,
        "gone" => Mode::Gone,
        "perm" => Mode::Perm,
        "readfail" => Mode::ReadFail,
        "hold" => Mode::Hold,
        other => panic!("mode {other}"),
    }
}

pub(crate) fn fields(m: Mutation, name: &str) -> LinkedRef<'static> {
    let r = match name {
        "a" => LinkedRef {
            linked_path: Some("/m/a.png"),
            linked_handle_id: None,
        },
        "b" => LinkedRef {
            linked_path: None,
            linked_handle_id: Some("h-b"),
        },
        "c" => LinkedRef {
            linked_path: Some("/m/c.png"),
            linked_handle_id: Some("h-c"),
        },
        "e" => LinkedRef {
            linked_path: None,
            linked_handle_id: Some("h-e"),
        },
        "d" => LinkedRef::default(),
        other => panic!("ref {other}"),
    };
    m.key_input(r)
}

/// The kernel's outcome for a platform mode, by the reference's key kind.
pub(crate) fn outcome_for(m: Mutation, name: &str, mode: Mode) -> ResolveOutcome {
    let handle = m
        .key(&fields(Mutation::None, name))
        .is_some_and(|k| matches!(k, LinkKey::Handle(_)));
    let o = match (handle, mode) {
        (_, Mode::Ok) => ResolveOutcome::Resolved,
        (true, Mode::Gone) => ResolveOutcome::HandleMissing,
        (true, Mode::Perm) => ResolveOutcome::PermissionNeeded,
        (_, Mode::ReadFail | Mode::Gone) => ResolveOutcome::ReadFailed,
        (_, Mode::Perm | Mode::Hold) => panic!("mode has no immediate outcome"),
    };
    m.outcome(o)
}

pub(crate) struct World {
    table: ResolveTable,
    lib: Vec<LibraryEntry>,
    latch: WebReopenLatch,
    clock: i64,
    env: HashMap<String, Mode>,
    reload: u32,
    rerender: u32,
    retry: u32,
    toasts: u32,
    opens: u32,
    has_handle: bool,
}

impl World {
    fn new(lib: &str) -> Self {
        let env = ["a", "b", "c", "e", "v"].map(|n| (n.to_owned(), Mode::Ok));
        World {
            table: ResolveTable::new(),
            lib: entries_of(lib),
            latch: WebReopenLatch::default(),
            clock: 0,
            env: env.into_iter().collect(),
            reload: 0,
            rerender: 0,
            retry: 0,
            toasts: 0,
            opens: 0,
            has_handle: true,
        }
    }

    fn settle(&mut self, m: Mutation, name: &str, mode: Mode) {
        let o = outcome_for(m, name, mode);
        let s = m.finish(
            &mut self.table,
            &mut self.lib,
            &fields(m, name),
            o,
            self.clock,
        );
        self.reload += u32::from(s.reload_library);
        self.rerender += u32::from(s.rerender);
    }

    fn r_snapshot(&self, m: Mutation) -> String {
        let mut parts = Vec::new();
        for name in ["a", "b", "c", "e"] {
            let r = fields(m, name);
            let failed = self.table.failure(&r).map_or("-".to_owned(), |f| {
                format!("{}@{}", reason_tok(Some(f.reason)), f.at_ms)
            });
            let (c, i) = (self.table.cached(&r), self.table.in_flight(&r));
            parts.push(format!("{name}={}{}{failed}", u8::from(c), u8::from(i)));
        }
        parts.push(format!("pend={}", pend_tok(&self.table)));
        parts.push(format!("lib={}", flags_tok(&self.lib)));
        parts.push(format!(
            "n={}.{}.{}",
            self.reload, self.rerender, self.retry
        ));
        parts.join("/")
    }

    fn w_snapshot(&self) -> String {
        format!(
            "warn={}/at={}/toast={}/open={}/lib={}/pend={}/n={}.{}",
            u8::from(self.latch.warned),
            self.latch.retry_at_ms,
            self.toasts,
            self.opens,
            flags_tok(&self.lib),
            pend_tok(&self.table),
            self.reload,
            self.retry
        )
    }
}

pub(crate) fn pend_tok(table: &ResolveTable) -> String {
    let p = table.pending_permission();
    if p.is_empty() {
        "-".to_owned()
    } else {
        p.join("+")
    }
}

pub(crate) fn r_step(w: &mut World, step: &str, m: Mutation) -> &'static str {
    if let Some(ms) = step.strip_prefix("t=") {
        w.clock = ms.parse().expect("clock");
    } else if let Some(kv) = step.strip_prefix("env.") {
        let (name, mode) = kv.split_once('=').expect("env");
        w.env.insert(name.to_owned(), mode_of(mode));
    } else if let Some(name) = step.strip_prefix("b.") {
        let start = m.begin(&mut w.table, &fields(m, name), w.clock);
        if start == ResolveStart::Started && w.env[name] != Mode::Hold {
            let mode = w.env[name];
            w.settle(m, name, mode);
        }
        return match start {
            ResolveStart::Started => "S",
            ResolveStart::AlreadyInFlight => "F",
            ResolveStart::CoolingDown => "C",
            ResolveStart::Unlinked => "U",
        };
    } else if let Some(rest) = step.strip_prefix("s.") {
        let (name, mode) = rest.split_once('.').expect("settle");
        w.env.insert(name.to_owned(), mode_of(mode));
        w.settle(m, name, mode_of(mode));
    } else if let Some(name) = step.strip_prefix("p0.") {
        m.prime(&mut w.table, &fields(m, name), false);
    } else if let Some(name) = step.strip_prefix("p.") {
        m.prime(&mut w.table, &fields(m, name), true);
    } else {
        panic!("R step {step}");
    }
    "."
}

pub(crate) fn g_step(w: &mut World, answers: &str, m: Mutation) -> String {
    let given: HashMap<&str, &str> = answers
        .split(',')
        .map(|kv| kv.split_once('=').expect("answer"))
        .collect();
    let asked = RefCell::new(Vec::new());
    let granted = m.permission_round(&mut w.table, &|id| {
        asked.borrow_mut().push(id.to_owned());
        match given.get(id) {
            Some(&"y") => PermissionAnswer::Granted,
            _ => PermissionAnswer::NotGranted,
        }
    });
    for id in granted {
        if let Some(name) = id.strip_prefix("h-") {
            w.env.insert(name.to_owned(), Mode::Ok);
        }
    }
    w.retry += 1;
    w.rerender += 1;
    let asked = asked.into_inner();
    if asked.is_empty() {
        "-".to_owned()
    } else {
        asked.join("+")
    }
}

pub(crate) fn run_r(script: &str, m: Mutation) -> String {
    let mut w = World::new(RLIB);
    let mut out = Vec::new();
    for step in script.split(';') {
        let code = match step.strip_prefix("g.") {
            Some(answers) => g_step(&mut w, answers, m),
            None => r_step(&mut w, step, m).to_owned(),
        };
        out.push(format!("{code}~{}", w.r_snapshot(m)));
    }
    out.join("|")
}

pub(crate) fn w_attempt(w: &mut World, m: Mutation) -> &'static str {
    match m.gate(&mut w.latch, w.has_handle, w.clock) {
        ReopenGate::NoHandle { warn } => {
            w.toasts += u32::from(warn);
            return "N";
        }
        ReopenGate::CoolingDown => return "C",
        ReopenGate::Attempt => {}
    }
    let (mode, uid, now) = (w.env["v"], Some("u1"), w.clock);
    let failure = match mode {
        Mode::Ok => {
            w.table.permission_observed("h-v", true);
            w.opens += 1;
            w.reload += u32::from(w.latch.attempt_succeeded(&mut w.lib, uid));
            return "A";
        }
        Mode::Gone => m.attempt_failed(&mut w.latch, &mut w.lib, uid, false, now),
        Mode::Perm => {
            w.table.permission_observed("h-v", false);
            m.attempt_failed(&mut w.latch, &mut w.lib, uid, true, now)
        }
        Mode::ReadFail => {
            w.table.permission_observed("h-v", true);
            m.attempt_failed(&mut w.latch, &mut w.lib, uid, false, now)
        }
        Mode::Hold => panic!("W has no hold mode"),
    };
    w.toasts += u32::from(failure.warn);
    w.reload += u32::from(failure.reload_library);
    "A"
}

pub(crate) fn run_w(script: &str, broken: bool, m: Mutation) -> String {
    let mut w = World::new(if broken { WLIB_BROKEN } else { WLIB });
    let mut out = Vec::new();
    for step in script.split(';') {
        let mut code = ".";
        if let Some(ms) = step.strip_prefix("t=") {
            w.clock = ms.parse().expect("clock");
        } else if let Some(mode) = step.strip_prefix("env.v=") {
            w.env.insert("v".to_owned(), mode_of(mode));
        } else if let Some(flag) = step.strip_prefix("h=") {
            w.has_handle = flag == "1";
        } else if step == "a" {
            code = w_attempt(&mut w, m);
        } else if step == "r" {
            w.latch.retry_all();
            w.retry += 1;
        } else {
            panic!("W step {step}");
        }
        out.push(format!("{code}~{}", w.w_snapshot()));
    }
    out.join("|")
}
