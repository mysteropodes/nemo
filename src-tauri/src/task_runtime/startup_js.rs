//! Pure native metadata admission, not an IPC/JS/paint authority.
use super::LoadLog;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

const MAX_EVENTS: usize = 15;
const MAX_BYTES: usize = 1024;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Refusal {
    Native,
    Sender,
    Payload,
    Identity,
    Sequence,
    Stage,
    Limit,
    Fenced,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Fence {
    Navigation,
    Replacement,
    Closed,
    NativeMismatch,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Wire {
    schema: String,
    run_id: String,
    load_generation: u32,
    sequence: u32,
    stage: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ready_state: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    control: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    visibility: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ordinal: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    category: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
}

#[derive(Clone, Default)]
struct Progress {
    early: bool,
    ready_observed: bool,
    dom: bool,
    load: bool,
    control: bool,
    control_unknown: bool,
    raf: u32,
    refused: bool,
}

pub(super) struct JsStream {
    run: String,
    task_key: String,
    pid: u32,
    native_phase: usize,
    progress: Progress,
    records: Vec<Value>,
    rejected: u32,
    rejection_overflow: bool,
    fence: Option<Fence>,
}

impl JsStream {
    /// Only the existing private, owner-gated native log supplies boot pins.
    /// Reserve generation one before build: early IPC may precede Started.
    pub(super) fn from_native(native: &LoadLog) -> Result<Self, Refusal> {
        if native.refused
            || native.phase > 3
            || native.pid == 0
            || uuid::Uuid::parse_str(&native.run)
                .ok()
                .map(|u| u.to_string())
                .as_deref()
                != Some(&native.run)
            || native.task_key.len() != 64
            || !native
                .task_key
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(Refusal::Native);
        }
        Ok(Self {
            run: native.run.clone(),
            task_key: native.task_key.clone(),
            pid: native.pid,
            native_phase: native.phase,
            progress: Progress::default(),
            records: Vec::new(),
            rejected: 0,
            rejection_overflow: false,
            fence: None,
        })
    }
    pub(super) fn fence(&mut self, reason: Fence) {
        if self.fence.is_none() {
            self.fence = Some(reason);
        }
    }
    pub(super) fn reconcile_native(&mut self, native: &LoadLog) -> Result<(), Refusal> {
        if self.fence.is_some() {
            return Err(Refusal::Fenced);
        }
        if native.run != self.run
            || native.task_key != self.task_key
            || native.pid != self.pid
            || native.refused
            || native.phase < self.native_phase
            || native.phase > 3
        {
            self.fence(Fence::NativeMismatch);
            return Err(Refusal::Native);
        }
        self.native_phase = native.phase;
        Ok(())
    }
    /// Sender arguments belong to the future native WebviewWindow adapter,
    /// never fields read from this untrusted JSON envelope.
    pub(super) fn accept_json(
        &mut self,
        label: &str,
        url: &tauri::Url,
        pid: u32,
        bytes: &[u8],
    ) -> Result<(), Refusal> {
        let result = self.prepare(label, url, pid, bytes);
        match result {
            Ok((wire, progress)) => {
                self.records
                    .push(serde_json::to_value(wire).map_err(|_| Refusal::Payload)?);
                self.progress = progress;
                Ok(())
            }
            Err(reason) => {
                if self.rejected < MAX_EVENTS as u32 {
                    self.rejected += 1;
                } else {
                    self.rejection_overflow = true;
                }
                Err(reason)
            }
        }
    }
    fn prepare(
        &self,
        label: &str,
        url: &tauri::Url,
        pid: u32,
        bytes: &[u8],
    ) -> Result<(Wire, Progress), Refusal> {
        if self.fence.is_some() {
            return Err(Refusal::Fenced);
        }
        if pid != self.pid || !super::allowed_origin(label, url) {
            return Err(Refusal::Sender);
        }
        if bytes.is_empty() || bytes.len() > MAX_BYTES {
            return Err(Refusal::Payload);
        }
        if self.records.len() >= MAX_EVENTS {
            return Err(Refusal::Limit);
        }
        // Typed deserialization denies duplicate fields, extra keys and
        // wrong types; object cardinality also refuses irrelevant null keys.
        let wire: Wire = serde_json::from_slice(bytes).map_err(|_| Refusal::Payload)?;
        let raw: Value = serde_json::from_slice(bytes).map_err(|_| Refusal::Payload)?;
        let count = raw.as_object().ok_or(Refusal::Payload)?.len();
        if wire.schema != "nemo.startup-js/1"
            || wire.run_id != self.run
            || wire.load_generation != 1
        {
            return Err(Refusal::Identity);
        }
        if wire.sequence as usize != self.records.len() + 1 {
            return Err(Refusal::Sequence);
        }
        let mut p = self.progress.clone();
        if p.refused || (!p.early && wire.stage != "early-script") {
            return Err(Refusal::Stage);
        }
        let allowed = |value: &Option<String>, choices: &[&str]| {
            value.as_deref().is_some_and(|s| choices.contains(&s))
        };
        let expected = match wire.stage.as_str() {
            "early-script" if !p.early => {
                p.early = true;
                5
            }
            "ready-state-observed"
                if !p.ready_observed
                    && !p.dom
                    && !p.load
                    && !p.control
                    && allowed(&wire.ready_state, &["interactive", "complete"]) =>
            {
                p.ready_observed = true;
                6
            }
            "dom-ready" if !p.dom && !p.load => {
                p.dom = true;
                5
            }
            "startup-control"
                if (p.ready_observed || p.dom)
                    && !p.control
                    && wire.control.as_deref() == Some("start-open")
                    && allowed(
                        &wire.visibility,
                        &["missing", "hidden", "visible", "unknown"],
                    ) =>
            {
                p.control = true;
                p.control_unknown = wire.visibility.as_deref() == Some("unknown");
                7
            }
            "window-load" if (p.ready_observed || p.dom) && p.control && !p.load => {
                p.load = true;
                5
            }
            "raf-ack"
                if p.control
                    && !p.control_unknown
                    && p.raf < 2
                    && wire.ordinal == Some(p.raf + 1) =>
            {
                p.raf += 1;
                6
            }
            "js-error" | "unhandled-rejection"
                if allowed(
                    &wire.category,
                    &[
                        "SyntaxError",
                        "ReferenceError",
                        "TypeError",
                        "RangeError",
                        "unknown",
                    ],
                ) =>
            {
                6
            }
            "probe-refused"
                if allowed(
                    &wire.reason,
                    &[
                        "synthetic-event",
                        "stage-order",
                        "unsupported-document",
                        "unsupported-port",
                    ],
                ) =>
            {
                p.refused = true;
                6
            }
            _ => return Err(Refusal::Stage),
        };
        if count != expected {
            return Err(Refusal::Payload);
        }
        Ok((wire, p))
    }
    pub(super) fn projection(&self) -> Value {
        let fence = self.fence.map(|f| match f {
            Fence::Navigation => "navigation",
            Fence::Replacement => "replacement",
            Fence::Closed => "closed",
            Fence::NativeMismatch => "native-mismatch",
        });
        json!({"schema":"nemo.startup-js-native/1", "runId":self.run, "taskKey":self.task_key,
            "pid":self.pid, "window":"main", "loadGeneration":1, "nativePhase":self.native_phase,
            "status":if fence.is_some() { "fenced" } else if self.native_phase < 2 { "provisional" } else { "native-start-correlated" },
            "accepted":self.records.len(), "rejected":self.rejected, "rejectionOverflow":self.rejection_overflow,
            "fence":fence, "probeRefused":self.progress.refused, "records":self.records,
            "sealed":false, "visiblePaint":"not-observed"})
    }
}

#[cfg(test)]
#[path = "startup_js_tests.rs"]
mod tests;
