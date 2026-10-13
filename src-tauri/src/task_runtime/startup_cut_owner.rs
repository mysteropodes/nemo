//! Native-only core. Request origin/Slot installation/dispatch are NOT proved.
#![allow(dead_code)]
use super::super::super::{valid_owner_token, valid_task_key, TaskRuntime};
use super::{
    cut::{Cut, Phase, RetentionEvidence},
    retention::Store,
    Bridge, LoadLog,
};
use serde::Deserialize;
use serde_json::Value;
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum State {
    Ready,
    Checking,
    Pending,
    Refused,
}
pub(super) struct Session {
    id: uuid::Uuid,
    token: String,
    task_id: String,
    data: PathBuf,
    run: String,
    task: String,
    pid: u32,
    store: Arc<Store>,
    native: Arc<Mutex<LoadLog>>,
    state: State,
    ticket: u64,
    snapshot: Option<Snapshot>,
    pending: Option<Cut>,
}
#[derive(Clone, PartialEq)]
struct Snapshot {
    native: Value,
    projection: Value,
    version: u64,
}
pub(super) struct Capture {
    session: uuid::Uuid,
    ticket: u64,
    token: String,
    store: Arc<Store>,
    snapshot: Snapshot,
}
pub(super) struct Authorization {
    capture: Capture,
    checked: Result<RetentionEvidence, &'static str>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Request {
    schema: String,
    owner_token: String,
    run_id: String,
    task_key: String,
    pid: u32,
    window: String,
    load_generation: u32,
}

impl Capture {
    /// Owned work carries NO metadata/native guard or borrowed locked data.
    /// Caller drops its guards before running this synchronous Store IO.
    pub(super) fn authorize(self, bytes: &[u8]) -> Authorization {
        let checked = (|| {
            if bytes.len() > 1024
                || bytes.iter().copied().find(|b| !b.is_ascii_whitespace()) != Some(b'{')
            {
                return Err("startup owner request refused");
            }
            let r: Request =
                serde_json::from_slice(bytes).map_err(|_| "startup owner request refused")?;
            let pins = &self.snapshot.projection["js"];
            if r.schema != "nemo.startup-owner-cut/1"
                || r.owner_token != self.token
                || r.run_id != pins["runId"]
                || r.task_key != pins["taskKey"]
                || Some(u64::from(r.pid)) != pins["pid"].as_u64()
                || r.window != "main"
                || r.load_generation != 1
            {
                return Err("startup owner request refused");
            }
            self.store
                .current_health(self.snapshot.version, &self.snapshot.projection)
        })();
        Authorization {
            capture: self,
            checked,
        }
    }
}

impl Session {
    pub(super) fn new(
        runtime: &TaskRuntime,
        native: &LoadLog,
        bridge: &Bridge,
        store: Arc<Store>,
    ) -> Result<Self, &'static str> {
        let token = runtime
            .owner_token
            .as_ref()
            .filter(|t| valid_owner_token(t))
            .ok_or("startup owner unavailable")?;
        if !runtime.data_dir.is_absolute()
            || !valid_task_key(&runtime.task_key)
            || native.task_key != runtime.task_key
            || native.pid != std::process::id()
            || uuid::Uuid::parse_str(&native.run).is_err()
        {
            return Err("startup owner unavailable");
        }
        let session = Self {
            id: uuid::Uuid::new_v4(),
            token: token.clone(),
            task_id: runtime.task_id.clone(),
            data: runtime.data_dir.clone(),
            run: native.run.clone(),
            task: native.task_key.clone(),
            pid: native.pid,
            store,
            native: bridge.native.clone(),
            state: State::Ready,
            ticket: 0,
            snapshot: None,
            pending: None,
        };
        if !session.identity(native, bridge) {
            return Err("startup owner unavailable");
        }
        Ok(session)
    }
    fn configured(&self, runtime: &TaskRuntime) -> bool {
        runtime.owner_token.as_deref() == Some(self.token.as_str())
            && runtime.task_key == self.task
            && runtime.task_id == self.task_id
            && runtime.data_dir == self.data
    }
    fn identity(&self, native: &LoadLog, bridge: &Bridge) -> bool {
        let p = bridge.stream.projection();
        native.run == self.run
            && native.task_key == self.task
            && native.pid == self.pid
            && p["runId"] == self.run
            && p["taskKey"] == self.task
            && p["pid"] == self.pid
            && p["window"] == "main"
            && p["loadGeneration"] == 1
            && p["nativePhase"] == native.phase
            && p["fence"].is_null()
            && Arc::ptr_eq(&self.native, &bridge.native)
    }
    fn fail<T>(&mut self) -> Result<T, &'static str> {
        self.state = State::Refused;
        Err("startup owner refused")
    }
    pub(super) fn state(&self) -> State {
        self.state
    }
    pub(super) fn capture(
        &mut self,
        runtime: &TaskRuntime,
        native: &LoadLog,
        bridge: &Bridge,
        version: u64,
    ) -> Result<Capture, &'static str> {
        if !matches!(self.state, State::Ready | State::Pending)
            || !self.configured(runtime)
            || !self.identity(native, bridge)
            || version == 0
            || self
                .pending
                .as_ref()
                .is_some_and(|p| !matches!(p.phase(), Phase::Requested | Phase::Draining))
        {
            return self.fail();
        }
        let Some(ticket) = self.ticket.checked_add(1) else {
            return self.fail();
        };
        let snapshot = Snapshot {
            native: native.receipt(),
            projection: bridge.projection(),
            version,
        };
        self.ticket = ticket;
        self.snapshot = Some(snapshot.clone());
        self.state = State::Checking;
        Ok(Capture {
            session: self.id,
            ticket,
            token: self.token.clone(),
            store: self.store.clone(),
            snapshot,
        })
    }
    pub(super) fn confirm(
        &mut self,
        auth: Authorization,
        runtime: &TaskRuntime,
        native: &LoadLog,
        bridge: &Bridge,
        version: u64,
    ) -> Result<&str, &'static str> {
        let Capture {
            session,
            ticket,
            store,
            snapshot,
            ..
        } = auth.capture;
        if self.state != State::Checking
            || session != self.id
            || ticket != self.ticket
            || !Arc::ptr_eq(&store, &self.store)
            || self.snapshot.as_ref() != Some(&snapshot)
            || version != snapshot.version
            || native.receipt() != snapshot.native
            || bridge.projection() != snapshot.projection
            || !self.configured(runtime)
            || !self.identity(native, bridge)
        {
            return self.fail();
        }
        let proof = match auth.checked {
            Ok(p) => p,
            Err(_) => return self.fail(),
        };
        if proof.version != version
            || proof.run != self.run
            || proof.task != self.task
            || proof.pid != self.pid
        {
            return self.fail();
        }
        if self.pending.is_none() {
            self.pending = match Cut::request(native, &bridge.stream, Some(&proof)) {
                Ok(cut) => Some(cut),
                Err(_) => return self.fail(),
            };
        }
        self.snapshot = None;
        self.state = State::Pending;
        Ok(self.pending.as_ref().unwrap().id())
    }
    pub(super) fn pending(&mut self) -> Result<&mut Cut, &'static str> {
        if self.state != State::Pending {
            return Err("startup owner unavailable");
        }
        self.pending.as_mut().ok_or("startup owner unavailable")
    }
    pub(super) fn retire(&mut self) {
        self.state = State::Refused;
    }
}

#[cfg(test)]
#[path = "startup_cut_owner_tests.rs"]
mod tests;
