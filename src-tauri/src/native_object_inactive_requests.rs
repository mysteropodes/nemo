//! Bounded, non-evicting request identities owned by the single native host.
use crate::native_dispatch::{NativeDispatch, NativeObjectHost};

pub(super) struct InactiveRequest {
    fingerprint: Vec<u8>,
    snapshot: String,
    prepares: bool,
    retained_token: Option<String>,
    outcome: Option<serde_json::Value>,
}
fn inactive_failure(code: &str, disposition: &str) -> serde_json::Value {
    serde_json::json!({"ok":false,"error":{"code":code,
        "message":"Inactive project request cannot be executed again.",
        "details":{"disposition":disposition,"retryExecution":false}}})
}

impl NativeObjectHost {
    /// Retain terminal identities instead of evicting them and risking reexecution.
    pub(crate) fn replay_inactive_request(
        &mut self,
        id: &str,
        fingerprint: &[u8],
    ) -> Result<Option<serde_json::Value>, String> {
        if self.released.is_some() {
            return Err("unavailable:inactive owner was released".into());
        }
        let current = self
            .history
            .acquire_snapshot(self.content_revision())
            .ok_or("unavailable:inactive owner snapshot unavailable")?;
        let has_retained_project = self.has_inactive_project();
        for record in self
            .inactive_requests
            .values_mut()
            .filter(|r| r.outcome.is_none())
        {
            if !record.prepares || has_retained_project {
                record.outcome = Some(inactive_failure("inactive_indeterminate", "indeterminate"));
            } else if record.snapshot != current.snapshot_id() {
                record.outcome = Some(inactive_failure("stale_revision", "notDispatched"));
            }
        }
        let Some(record) = self.inactive_requests.get(id) else {
            return Ok(None);
        };
        if record.fingerprint != fingerprint {
            return Err("request_conflict:inactive requestId body changed".into());
        }
        Ok(Some(record.outcome.clone().unwrap_or_else(|| {
            inactive_failure("inactive_pending", "pending")
        })))
    }

    pub(crate) fn begin_inactive_request(
        &mut self,
        id: &str,
        fingerprint: &[u8],
        prepares: bool,
        close_token: Option<&str>,
    ) -> Result<(), String> {
        if id.is_empty() || id.len() > 128 || fingerprint.len() > 4096 {
            return Err("invalid_request:inactive request exceeds limits".into());
        }
        if self.inactive_requests.contains_key(id) {
            return Err("request_conflict:inactive request already admitted".into());
        }
        let closes_current = self.retained_project_token() == close_token && close_token.is_some();
        // Preserve one receipt for closing B, including after a lost prepare reply.
        let needs_close_reserve = prepares || (self.has_inactive_project() && !closes_current);
        if self.inactive_requests.len() >= 16
            || (needs_close_reserve && self.inactive_requests.len() >= 15)
        {
            return Err("unavailable:inactive request receipt capacity exhausted".into());
        }
        if self.inactive_requests.values().any(|r| r.outcome.is_none()) {
            return Err("inactive_pending:another inactive request is pending".into());
        }
        if prepares && self.has_inactive_project() {
            return Err("preparation_busy:one inactive project is already retained".into());
        }
        let snapshot = self
            .history
            .acquire_snapshot(self.content_revision())
            .ok_or("unavailable:inactive owner snapshot unavailable")?;
        self.inactive_requests.insert(
            id.into(),
            InactiveRequest {
                fingerprint: fingerprint.to_vec(),
                snapshot: snapshot.snapshot_id().into(),
                prepares,
                retained_token: None,
                outcome: None,
            },
        );
        Ok(())
    }

    pub(crate) fn pending_inactive_request(&self, id: &str, fingerprint: &[u8]) -> bool {
        self.inactive_requests
            .get(id)
            .is_some_and(|r| r.fingerprint == fingerprint && r.outcome.is_none())
    }
    pub(crate) fn finish_inactive_request(
        &mut self,
        id: &str,
        fingerprint: &[u8],
        outcome: serde_json::Value,
    ) -> Result<(), String> {
        if serde_json::to_vec(&outcome).map_or(true, |v| v.len() > 68 * 1024) {
            return Err("inactive_indeterminate:inactive receipt exceeds limit".into());
        }
        let record = self
            .inactive_requests
            .get_mut(id)
            .filter(|r| r.fingerprint == fingerprint && r.outcome.is_none())
            .ok_or("inactive_indeterminate:inactive request no longer pending")?;
        record.outcome = Some(outcome);
        Ok(())
    }
    pub(crate) fn current_inactive_project(&self) -> Result<Option<serde_json::Value>, String> {
        self.retained_project_metadata()
    }

    pub(super) fn inactive_preparation_request_for_token(&self, token: &str) -> Option<&str> {
        self.inactive_requests
            .iter()
            .find(|(_, r)| r.retained_token.as_deref() == Some(token))
            .map(|(id, _)| id.as_str())
    }

    pub(crate) fn pending_inactive_request_id(&self) -> Option<&str> {
        self.inactive_requests
            .iter()
            .find(|(_, r)| r.outcome.is_none())
            .map(|(id, _)| id.as_str())
    }
    pub(crate) fn inactive_request_outcome(&self, id: &str) -> Option<serde_json::Value> {
        self.inactive_requests.get(id).map(|record| {
            let reply = record.outcome.clone().unwrap_or_else(|| inactive_failure("inactive_pending", "pending"));
            if reply["ok"] == true {
                serde_json::json!({"ok":true,"value":{
                    "state":reply["value"]["state"],"preparationToken":reply["value"]["preparationToken"]}})
            } else { reply }
        })
    }
    pub(super) fn record_inactive_prepared(&mut self, token: &str) {
        if let Some(record) = self
            .inactive_requests
            .values_mut()
            .find(|r| r.prepares && r.outcome.is_none())
        {
            record.retained_token = Some(token.to_owned());
        }
    }
    pub(super) fn record_inactive_closed(&mut self, token: &str) {
        for record in self.inactive_requests.values_mut() {
            if record.retained_token.as_deref() == Some(token) {
                record.outcome = Some(inactive_failure("preparation_closed", "closed"));
            }
        }
    }
}
