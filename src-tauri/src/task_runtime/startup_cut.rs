//! Private contract only: no producer fence, IPC, disk flush or physical paint.
#![allow(dead_code)]
use super::{JsStream, LoadLog};
use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Phase {
    Requested,
    Draining,
    SealedPartial,
    SealedObserved,
    Refused,
}

/// Future native adapter must check current owned inode/bytes/store health before
/// constructing this input. It is NOT deserializable or supplied by JS. KN does
/// not implement that check and cannot itself attest actual retained evidence.
pub(super) struct RetentionEvidence {
    pub(super) run: String,
    pub(super) task: String,
    pub(super) pid: u32,
    pub(super) version: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Footer {
    run_id: String,
    load_generation: u32,
    cut_id: String,
    status: String,
    generated: u64,
    sent: u64,
    failed: u64,
    dropped: u64,
    late: u64,
    pending: u64,
    visible_paint: String,
}

pub(super) struct Cut {
    run: String,
    task: String,
    pid: u32,
    id: String,
    phase: Phase,
    native: Value,
    js: Value,
    version: u64,
    footer: Option<Footer>,
}

fn uuid(value: &str) -> bool {
    uuid::Uuid::parse_str(value).is_ok_and(|u| u.to_string() == value)
}
fn prefix(old: &Value, new: &Value, key: &str) -> bool {
    match (old[key].as_array(), new[key].as_array()) {
        (Some(a), Some(b)) => b.starts_with(a),
        _ => false,
    }
}

impl Cut {
    pub(super) fn request(
        native: &LoadLog,
        stream: &JsStream,
        retention: Option<&RetentionEvidence>,
    ) -> Result<Self, &'static str> {
        let js = stream.projection();
        let mut cut = Self {
            run: native.run.clone(),
            task: native.task_key.clone(),
            pid: native.pid,
            id: uuid::Uuid::new_v4().to_string(),
            phase: Phase::Requested,
            native: native.receipt(),
            js: js.clone(),
            version: 0,
            footer: None,
        };
        if !uuid(&cut.run)
            || cut.pid == 0
            || cut.task.len() != 64
            || !cut
                .task
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || !cut.correlated(native, &js)
            || !cut.healthy(retention)
        {
            return Err("startup cut owner unavailable");
        }
        cut.version = retention.unwrap().version;
        Ok(cut)
    }
    pub(super) fn id(&self) -> &str {
        &self.id
    }
    pub(super) fn phase(&self) -> Phase {
        self.phase
    }
    pub(super) fn begin_drain(&mut self) -> Result<(), &'static str> {
        match self.phase {
            Phase::Requested => {
                self.phase = Phase::Draining;
                Ok(())
            }
            Phase::Draining => Ok(()),
            _ => Err("startup cut terminal"),
        }
    }
    fn healthy(&self, proof: Option<&RetentionEvidence>) -> bool {
        proof.is_some_and(|p| {
            p.run == self.run
                && p.task == self.task
                && p.pid == self.pid
                && p.version > 0
                && p.version >= self.version
        })
    }
    fn correlated(&self, native: &LoadLog, js: &Value) -> bool {
        native.run == self.run
            && native.task_key == self.task
            && native.pid == self.pid
            && native.phase <= 3
            && js["runId"] == self.run
            && js["taskKey"] == self.task
            && js["pid"] == self.pid
            && js["window"] == "main"
            && js["loadGeneration"] == 1
            && js["nativePhase"] == native.phase
            && js["schema"] == "nemo.startup-js-native/1"
            && js["records"].as_array().is_some_and(|r| {
                r.len() <= 15
                    && js["accepted"].as_u64() == Some(r.len() as u64)
                    && r.iter().enumerate().all(|(i, e)| {
                        e["sequence"] == i + 1 && e["runId"] == self.run && e["loadGeneration"] == 1
                    })
            })
    }
    fn advances(&self, native: &Value, js: &Value) -> bool {
        prefix(&self.native, native, "stages")
            && prefix(&self.js, js, "records")
            && ["accepted", "rejected", "dropped"].iter().all(|k| {
                native[k]
                    .as_u64()
                    .zip(self.native[k].as_u64())
                    .is_some_and(|(n, o)| n >= o)
            })
            && js["rejected"]
                .as_u64()
                .zip(self.js["rejected"].as_u64())
                .is_some_and(|(n, o)| n >= o)
            && !(self.native["status"] == "refused" && native["status"] != "refused")
            && !(self.js["probeRefused"] == true && js["probeRefused"] != true)
            && !(self.js["rejectionOverflow"] == true && js["rejectionOverflow"] != true)
            && (self.js["fence"].is_null() || js["fence"] == self.js["fence"])
    }
    fn fail(&mut self) -> Result<Phase, &'static str> {
        self.phase = Phase::Refused;
        Err("startup cut refused")
    }
    fn parse_footer(
        &self,
        bytes: &[u8],
        native: &LoadLog,
        stream: &JsStream,
        retention: Option<&RetentionEvidence>,
    ) -> Result<Footer, &'static str> {
        let js = stream.projection();
        let current = native.receipt();
        if !self.healthy(retention)
            || !self.correlated(native, &js)
            || !self.advances(&current, &js)
            || bytes.is_empty()
            || bytes.len() > 1024
        {
            return Err("startup cut refused");
        }
        let Ok(footer) = serde_json::from_slice::<Footer>(bytes) else {
            return Err("startup cut refused");
        };
        let safe = 9_007_199_254_740_991;
        let accounted = footer
            .sent
            .checked_add(footer.failed)
            .and_then(|n| n.checked_add(footer.pending));
        let accepted = js["accepted"].as_u64().unwrap();
        if footer.run_id != self.run
            || footer.load_generation != 1
            || footer.cut_id != self.id
            || footer.visible_paint != "not-observed"
            || footer.generated > 15
            || footer.sent > accepted
            || accepted > footer.generated
            || !accounted.is_some_and(|n| n <= footer.generated)
            || footer.dropped > safe
            || footer.late > safe
            || !["draining", "drained", "partial"].contains(&footer.status.as_str())
            || (footer.status == "draining") != (footer.pending > 0)
            || (footer.status == "drained"
                && (footer.generated != footer.sent
                    || footer.failed != 0
                    || footer.dropped != 0
                    || footer.late != 0))
        {
            return Err("startup cut refused");
        }
        Ok(footer)
    }
    /// Provisional evidence only. Caller owns failure latching and final health
    /// reconciliation; successful validation never freezes late counters.
    pub(super) fn validate_provisional(
        &self,
        bytes: &[u8],
        native: &LoadLog,
        stream: &JsStream,
        retention: Option<&RetentionEvidence>,
    ) -> Result<(), &'static str> {
        if self.phase != Phase::Draining {
            return Err("startup cut is not draining");
        }
        self.parse_footer(bytes, native, stream, retention)
            .map(|_| ())
    }
    pub(super) fn finish(
        &mut self,
        bytes: &[u8],
        native: &LoadLog,
        stream: &JsStream,
        retention: Option<&RetentionEvidence>,
    ) -> Result<Phase, &'static str> {
        if self.phase == Phase::Refused {
            return Err("startup cut terminal");
        }
        let footer = match self.parse_footer(bytes, native, stream, retention) {
            Ok(footer) => footer,
            Err(_) => return self.fail(),
        };
        let js = stream.projection();
        let current = native.receipt();
        let accepted = js["accepted"].as_u64().unwrap();
        if matches!(self.phase, Phase::SealedObserved | Phase::SealedPartial) {
            return if self.footer.as_ref() == Some(&footer)
                && self.native == current
                && self.js == js
                && retention.unwrap().version == self.version
            {
                Ok(self.phase)
            } else {
                self.fail()
            };
        }
        if self.phase != Phase::Draining {
            return self.fail();
        }
        // Pending is honest partial terminal evidence; caller deadline/fencing is
        // outside this pure contract. No footer can reopen the same cut.
        let clean = footer.status == "drained"
            && footer.generated == accepted
            && footer.sent == accepted
            && footer.generated > 0
            && native.phase == 3
            && !native.refused
            && native.rejected == 0
            && native.dropped == 0
            && js["rejected"] == 0
            && js["rejectionOverflow"] == false
            && js["probeRefused"] == false
            && js["fence"].is_null()
            && !js["records"].as_array().unwrap().iter().any(|e| {
                ["js-error", "unhandled-rejection", "probe-refused"]
                    .contains(&e["stage"].as_str().unwrap_or(""))
            });
        self.phase = if clean {
            Phase::SealedObserved
        } else {
            Phase::SealedPartial
        };
        self.native = current;
        self.js = js;
        self.footer = Some(footer);
        self.version = retention.unwrap().version;
        Ok(self.phase)
    }
    pub(super) fn missing(
        &mut self,
        native: &LoadLog,
        stream: &JsStream,
        retention: Option<&RetentionEvidence>,
    ) -> Result<Phase, &'static str> {
        if matches!(
            self.phase,
            Phase::SealedPartial | Phase::SealedObserved | Phase::Refused
        ) {
            return Err("startup cut terminal");
        }
        let js = stream.projection();
        if !self.healthy(retention)
            || !self.correlated(native, &js)
            || !self.advances(&native.receipt(), &js)
        {
            return self.fail();
        }
        self.phase = Phase::SealedPartial;
        self.native = native.receipt();
        self.js = js;
        self.version = retention.unwrap().version;
        Ok(self.phase)
    }
}

#[cfg(test)]
#[path = "startup_cut_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "startup_cut_provisional_tests.rs"]
mod provisional_tests;
