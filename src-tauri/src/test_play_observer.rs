// O1/#1708: private shell metadata sink, absent from normal packages.
// Bounded test-only control outside every product frame/scheduler path.
#[cfg(feature = "test-play-publication-observer")]
pub(crate) fn watch_play_seal<R: tauri::Runtime>(
    handle: tauri::AppHandle<R>,
    reports: std::path::PathBuf,
    run: String,
) {
    std::thread::spawn(move || {
        use tauri::{Emitter, Manager};
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(600);
        let marker = reports.join("play-observer-seal.json");
        let mut sent = false;
        while std::time::Instant::now() < deadline {
            let observation = handle
                .state::<TestPlayState>()
                .0
                .lock()
                .ok()
                .and_then(|s| s.as_ref().map(|s| (s.sealed, s.accepted > 0)));
            let Some((sealed, ready)) = observation else {
                return;
            };
            if sealed {
                return;
            }
            if ready && !sent && seal_marker_valid(&marker, &run) {
                if let Some(window) = handle.get_webview_window("main") {
                    let _ = window.emit("nemo-test-play-seal", serde_json::json!({"runId":run}));
                }
                sent = true;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    });
}

pub(crate) fn seal_marker_valid(marker: &std::path::Path, run: &str) -> bool {
    use std::io::Read;
    if !std::fs::symlink_metadata(marker).is_ok_and(|m| m.is_file() && !m.file_type().is_symlink())
    {
        return false;
    }
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(target_os = "macos")]
    {
        // Darwin O_NOFOLLOW, independently read back from the host's os constant.
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(0x100);
    }
    let Ok(file) = options.open(marker) else {
        return false;
    };
    if !file.metadata().is_ok_and(|m| m.is_file() && m.len() <= 128) {
        return false;
    }
    let mut bytes = Vec::new();
    file.take(129).read_to_end(&mut bytes).is_ok()
        && bytes.len() <= 128
        && serde_json::from_slice::<serde_json::Value>(&bytes).ok()
            == Some(serde_json::json!({"runId":run}))
}

#[cfg(feature = "test-play-publication-observer")]
pub(crate) struct TestPlayState(pub(crate) std::sync::Mutex<Option<TestPlayLog>>);
#[cfg(feature = "test-play-publication-observer")]
#[derive(Default)]
pub(crate) struct TestPlayLog {
    pub(crate) run: String,
    pub(crate) accepted: u64,
    pub(crate) rejected: u64,
    pub(crate) sealed: bool,
    finished: bool,
    cut_id: Option<String>,
    cut_counts: [u64; 3],
    late: bool,
}
#[cfg(feature = "test-play-publication-observer")]
#[derive(serde::Deserialize)]
#[serde(
    tag = "action",
    content = "payload",
    rename_all = "lowercase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
enum TestPlayInput {
    Record {
        run_id: String,
        sequence: u64,
        event: serde_json::Value,
    },
    Seal {
        run_id: String,
        generated: u64,
        dropped: u64,
        failed: u64,
    },
    Finish {
        run_id: String,
        cut_id: String,
        generated: u64,
        dropped: u64,
        failed: u64,
        pending: u64,
    },
    Late {
        run_id: String,
    },
}
#[cfg(feature = "test-play-publication-observer")]
impl TestPlayLog {
    pub(crate) fn allowed_origin(label: &str, url: &tauri::Url) -> bool {
        label == "main"
            && url.scheme() == "tauri"
            && url.host_str() == Some("localhost")
            && url.port().is_none()
            && url.username().is_empty()
            && url.password().is_none()
            && ["", "/", "/index.html"].contains(&url.path())
    }
    pub(crate) fn new(run: &str) -> Self {
        Self {
            run: run.into(),
            ..Self::default()
        }
    }
    pub(crate) fn valid_event(event: &serde_json::Value) -> bool {
        let events = [
            "ready",
            "play-start",
            "play-step",
            "play-stop",
            "play-state",
            "navigate-request",
            "navigate-superseded",
            "host-request",
            "host-receipt",
            "receipt-rejected",
            "receipt-deferred",
            "receipt-stale",
            "receipt-retry",
            "presented-return",
            "commit-enter",
            "commit-return",
            "paint-enter",
            "paint-return",
            "sync-enter",
            "sync-return",
            "navigate-return",
            "ui-publication",
            "rollback",
            "failure",
        ];
        let Some(fields) = event.as_object() else {
            return false;
        };
        if !event["event"].as_str().is_some_and(|v| events.contains(&v)) {
            return false;
        }
        fields.iter().all(|(key, value)| match key.as_str() {
            "event" => true,
            "contentRevision"
            | "frame"
            | "lifecycleGeneration"
            | "jsLifecycleGeneration"
            | "hostLifecycleGeneration"
            | "viewGeneration"
            | "sessionOrdinal"
            | "intent"
            | "runGeneration"
            | "hostGeneration"
            | "currentFrame"
            | "priorFrame" => value.as_u64().is_some_and(|v| v <= 9_007_199_254_740_991),
            "hostCallOrdinal" => value
                .as_u64()
                .is_some_and(|v| v > 0 && v <= 9_007_199_254_740_991),
            "result" | "playing" => value.is_boolean(),
            "documentSnapshotId" | "contextId" => value.as_str().is_some_and(|s| {
                !s.is_empty()
                    && s.len() <= 128
                    && s.as_bytes()[0].is_ascii_alphanumeric()
                    && s.bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"._:/-".contains(&b))
            }),
            "owner" => value == "native",
            "instanceId" => value
                .as_str()
                .is_some_and(|s| uuid::Uuid::parse_str(s).is_ok_and(|v| v.to_string() == s)),
            "documentId" | "workId" => value.as_str().is_some_and(|s| {
                // Opacity projects publish `native-document-N`, native object projects `native-object-document-N`.
                let tail = if key == "documentId" {
                    s.strip_prefix("native-object-document-")
                        .or_else(|| s.strip_prefix("native-document-"))
                } else {
                    s.strip_prefix("native-work-")
                };
                tail.is_some_and(|tail| {
                    tail.parse::<u64>()
                        .is_ok_and(|n| n > 0 && n.to_string() == tail)
                })
            }),
            "status" => value.as_str().is_some_and(|s| {
                [
                    "presented",
                    "deferred-occluded",
                    "deferred-timeout",
                    "stale-discarded",
                    "failed-validation",
                    "failed-device-lost",
                    "failed-retry-lost",
                    "failed-retry-outdated",
                    "failed-recreate",
                    "failed-reconfigure",
                ]
                .contains(&s)
            }),
            "phase" => value.as_str().is_some_and(|s| {
                [
                    "native",
                    "blocked",
                    "preparing",
                    "closed",
                    "legacy",
                    "installing",
                    "release-requested",
                    "releasing",
                    "replacing",
                    "indeterminate",
                ]
                .contains(&s)
            }),
            "code" => value.as_str().is_some_and(|s| {
                [
                    "native_preview_deferred",
                    "native_preview_superseded",
                    "observer_refused",
                    "unknown",
                ]
                .contains(&s)
            }),
            _ => false,
        }) && serde_json::to_vec(event).is_ok_and(|v| v.len() <= 1024)
    }
    pub(crate) fn record(
        &mut self,
        run: &str,
        sequence: u64,
        event: serde_json::Value,
    ) -> Result<serde_json::Value, &'static str> {
        if run != self.run
            || self.sealed
            || sequence != self.accepted + 1
            || sequence > 4096
            || !Self::valid_event(&event)
            || (sequence == 1) != (event["event"] == "ready")
        {
            self.rejected = self.rejected.saturating_add(1);
            return Err("test observer record refused");
        }
        self.accepted = sequence;
        Ok(
            serde_json::json!({"kind":"event", "runId":self.run, "sequence":sequence, "event":event}),
        )
    }
    pub(crate) fn seal(
        &mut self,
        run: &str,
        generated: u64,
        dropped: u64,
        failed: u64,
    ) -> serde_json::Value {
        if run != self.run || self.sealed {
            self.rejected = self.rejected.saturating_add(1);
        }
        self.sealed = true;
        self.cut_counts = [generated, dropped, failed];
        let cut = uuid::Uuid::new_v4().to_string();
        self.cut_id = Some(cut.clone());
        serde_json::json!({"kind":"seal", "runId":self.run, "cutId":cut, "status":"provisional",
            "generated":generated, "accepted":self.accepted, "dropped":dropped, "failed":failed, "rejected":self.rejected})
    }
    pub(crate) fn finish(
        &mut self,
        run: &str,
        cut: &str,
        counts: [u64; 4],
    ) -> Result<serde_json::Value, &'static str> {
        if run != self.run || !self.sealed || self.finished || self.cut_id.as_deref() != Some(cut) {
            return self.refuse("test observer terminal fence refused");
        }
        let [generated, dropped, failed, pending] = counts;
        let complete = self.accepted > 0
            && generated == self.accepted
            && pending == 0
            && dropped == 0
            && failed == 0
            && self.rejected == 0
            && !self.late
            && self.cut_counts[0] == self.accepted
            && self.cut_counts[1] == 0
            && self.cut_counts[2] == 0;
        self.finished = true;
        Ok(
            serde_json::json!({"kind":"footer", "runId":self.run, "cutId":cut, "terminalFence":complete,
            "status":if complete {"complete"} else {"partial"}, "generated":generated, "accepted":self.accepted,
            "dropped":dropped, "failed":failed, "pending":pending, "rejected":self.rejected}),
        )
    }
    pub(crate) fn output(value: &serde_json::Value) -> Result<(), &'static str> {
        use std::io::Write;
        let line = format!("NEMO_PLAY_OBSERVER {value}\n");
        if line.len() > 2048 {
            return Err("test observer line too large");
        }
        let mut out = std::io::stderr().lock();
        out.write_all(line.as_bytes())
            .and_then(|_| out.flush())
            .map_err(|_| "test observer output failed")
    }
    pub(crate) fn dispatch(
        &mut self,
        action: &str,
        payload: serde_json::Value,
    ) -> Result<serde_json::Value, &'static str> {
        if !payload.is_object() {
            return self.refuse("test observer object required");
        }
        let input: Result<TestPlayInput, _> =
            serde_json::from_value(serde_json::json!({"action":action,"payload":payload}));
        let input = match input {
            Ok(v) => v,
            Err(_) => return self.refuse("test observer envelope refused"),
        };
        let run = match &input {
            TestPlayInput::Record { run_id, .. }
            | TestPlayInput::Seal { run_id, .. }
            | TestPlayInput::Finish { run_id, .. }
            | TestPlayInput::Late { run_id } => run_id,
        };
        if run != &self.run {
            return self.refuse("test observer envelope refused");
        }
        if self.finished
            || (self.sealed && !matches!(input, TestPlayInput::Finish { .. }))
            || matches!(input, TestPlayInput::Late { .. })
        {
            self.rejected = self.rejected.saturating_add(1);
            if !self.late {
                self.late = true;
                Self::output(&serde_json::json!({"kind":"late", "runId":self.run}))?;
            }
            return Err("test observer terminal cut");
        }
        let frame = match input {
            TestPlayInput::Record {
                run_id,
                sequence,
                event,
            } => self.record(&run_id, sequence, event)?,
            TestPlayInput::Seal {
                run_id,
                generated,
                dropped,
                failed,
            } => {
                if [generated, dropped, failed]
                    .iter()
                    .any(|n| *n > 9_007_199_254_740_991)
                {
                    return self.refuse("test observer counter refused");
                }
                self.seal(&run_id, generated, dropped, failed)
            }
            TestPlayInput::Finish {
                run_id,
                cut_id,
                generated,
                dropped,
                failed,
                pending,
            } => {
                let counts = [generated, dropped, failed, pending];
                if counts.iter().any(|n| *n > 9_007_199_254_740_991) {
                    return self.refuse("test observer counter refused");
                }
                self.finish(&run_id, &cut_id, counts)?
            }
            TestPlayInput::Late { .. } => unreachable!(),
        };
        if let Err(error) = Self::output(&frame) {
            self.rejected = self.rejected.saturating_add(1);
            return Err(error);
        }
        Ok(frame)
    }
    fn refuse(&mut self, reason: &'static str) -> Result<serde_json::Value, &'static str> {
        self.rejected = self.rejected.saturating_add(1);
        if self.sealed && !self.late {
            self.late = true;
            Self::output(&serde_json::json!({"kind":"late", "runId":self.run}))?;
        }
        Err(reason)
    }
}
