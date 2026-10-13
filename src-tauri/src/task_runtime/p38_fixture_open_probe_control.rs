//! Data-only checkpoints and owner-file acknowledgments for the fixed P38 probe.
use super::{ProbeState, Session, BUILD_SHA, HASH_A, HASH_B, MAX_PACKET, PHASES};
use crate::task_runtime::TaskRuntimeState;
use serde::Deserialize;
use std::{
    fs::{self, OpenOptions},
    io::Read,
    path::Path,
    time::{Duration, Instant},
};
use tauri::Manager;

#[cfg(target_os = "linux")]
pub(super) const NOFOLLOW_FLAG: i32 = 0x20000;
#[cfg(all(unix, not(target_os = "linux")))]
pub(super) const NOFOLLOW_FLAG: i32 = 0x100;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Packet {
    run_id: String,
    task_key: String,
    source_sha: String,
    build_sha: String,
    manifest_sha256: String,
    phase: String,
    status: String,
    fixture_id: Option<String>,
    fixture_sha256: Option<String>,
    failure_code: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Ack {
    run_id: String,
    task_key: String,
    source_sha: String,
    build_sha: String,
    manifest_sha256: String,
    phase: String,
    owner_token: String,
    decision: String,
}

fn read_ack(session: &Session, phase: &str) -> Result<Option<Ack>, &'static str> {
    let path = session.root.join("ack.json");
    let meta = match fs::symlink_metadata(&path) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("P38 owner acknowledgment unavailable"),
    };
    if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > MAX_PACKET as u64 {
        return Err("P38 owner acknowledgment refused");
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(NOFOLLOW_FLAG); // O_NOFOLLOW
    }
    let mut bytes = Vec::new();
    options
        .open(path)
        .map_err(|_| "P38 owner acknowledgment unavailable")?
        .take((MAX_PACKET + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| "P38 owner acknowledgment unavailable")?;
    if bytes.len() > MAX_PACKET {
        return Err("P38 owner acknowledgment bound exceeded");
    }
    let ack: Ack =
        serde_json::from_slice(&bytes).map_err(|_| "P38 owner acknowledgment invalid")?;
    if ack.run_id != session.pins.run_id
        || ack.task_key != session.pins.task_key
        || ack.source_sha != session.pins.source_sha
        || ack.build_sha != session.pins.build_sha
        || ack.manifest_sha256 != session.pins_sha256
        || ack.phase != phase
        || ack.owner_token != session.owner
        || !["accepted", "refused"].contains(&ack.decision.as_str())
    {
        return Err("P38 owner acknowledgment identity refused");
    }
    Ok(Some(ack))
}

fn runtime_matches(app: &tauri::AppHandle, session: &Session) -> Result<(), &'static str> {
    let state = app
        .try_state::<TaskRuntimeState>()
        .ok_or("P38 runtime unavailable")?;
    let manifest = state
        .manifest
        .lock()
        .map_err(|_| "P38 runtime unavailable")?;
    if manifest["isolated"] != true
        || manifest["state"] != "active"
        || manifest["taskKey"] != session.pins.task_key
        || manifest["pid"] != session.pins.pid
    {
        return Err("P38 runtime identity refused");
    }
    Ok(())
}

fn accept_packet(session: &Session, packet: &Packet) -> Result<(), &'static str> {
    if session.failed
        || session.waiting
        || session.cursor >= PHASES.len()
        || packet.run_id != session.pins.run_id
        || packet.task_key != session.pins.task_key
        || packet.source_sha != session.pins.source_sha
        || packet.build_sha != BUILD_SHA
        || packet.manifest_sha256 != session.pins_sha256
        || packet.phase != PHASES[session.cursor]
    {
        return Err("P38 checkpoint identity or order refused");
    }
    let (status, fixture, hash) = match session.cursor {
        0 => ("ready", None, None),
        1 => ("returned", Some("A"), Some(HASH_A)),
        2 => ("returned", Some("B"), Some(HASH_B)),
        _ => return Err("P38 checkpoint refused"),
    };
    if packet.status == "failed" {
        if !valid_failure_packet(
            session.cursor,
            packet.fixture_id.as_deref(),
            packet.fixture_sha256.as_deref(),
            packet.failure_code.as_deref(),
        ) {
            return Err("P38 failure checkpoint refused");
        }
        return Err("P38 operation reported a fixed failure");
    }
    if packet.status != status
        || packet.fixture_id.as_deref() != fixture
        || packet.fixture_sha256.as_deref() != hash
        || packet.failure_code.is_some()
    {
        return Err("P38 checkpoint payload refused");
    }
    Ok(())
}

pub(super) fn valid_failure_packet(
    cursor: usize,
    fixture_id: Option<&str>,
    fixture_sha256: Option<&str>,
    failure_code: Option<&str>,
) -> bool {
    match (cursor, fixture_id, fixture_sha256, failure_code) {
        (0, None, None, Some(code)) => allowed_ready_failure(code),
        (1, Some("A"), Some(HASH_A), Some(code)) => allowed_a_failure(code),
        (2, Some("B"), Some(HASH_B), Some(code)) => allowed_b_failure(code),
        _ => false,
    }
}

pub(super) fn allowed_ready_failure(code: &str) -> bool {
    matches!(
        code,
        "ready-open-path-unavailable" | "ready-observer-install-failed"
    )
}

pub(super) fn allowed_a_failure(code: &str) -> bool {
    matches!(
        code,
        "a-open-threw"
            | "a-open-false"
            | "a-open-unaccepted"
            | "a-pre-native-refusal"
            | "a-native-status-rejected"
            | "a-native-bootstrap-unsupported-format"
            | "a-native-bootstrap-rejected"
            | "a-native-bootstrap-resolved-open-refused"
            | "a-native-status-empty-no-bootstrap"
            | "a-native-status-available-no-bootstrap"
            | "a-native-status-malformed-no-bootstrap"
            | "a-observer-late-untrusted"
            | "a-fs-read-rejected"
            | "a-fs-read-resolved-open-refused"
    )
}

pub(super) fn allowed_b_failure(code: &str) -> bool {
    matches!(
        code,
        "b-open-threw"
            | "b-open-false"
            | "b-open-unaccepted"
            | "b-pre-native-refusal"
            | "b-native-status-rejected"
            | "b-native-bootstrap-unsupported-format"
            | "b-native-bootstrap-rejected"
            | "b-native-bootstrap-resolved-open-refused"
            | "b-native-status-empty-no-bootstrap"
            | "b-native-status-available-no-bootstrap"
            | "b-native-status-malformed-no-bootstrap"
            | "b-observer-late-untrusted"
            | "b-fs-read-rejected"
            | "b-fs-read-resolved-open-refused"
    )
}

#[tauri::command]
pub(crate) async fn nemo_test_p38_fixture_open_probe(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    payload_json: String,
) -> Result<(), String> {
    if payload_json.len() > MAX_PACKET {
        return Err("P38 checkpoint too large".into());
    }
    let packet: Packet =
        serde_json::from_str(&payload_json).map_err(|_| "P38 checkpoint invalid")?;
    let url = window.url().map_err(|_| "P38 window unavailable")?;
    if !super::control::origin(window.label(), &url) {
        return Err("P38 original main window refused".into());
    }
    let state = app
        .try_state::<ProbeState>()
        .ok_or("P38 probe unavailable")?;
    let (root, phase) = {
        let mut guard = state.0.lock().map_err(|_| "P38 probe unavailable")?;
        let session = guard.as_mut().ok_or("P38 probe inactive")?;
        if session.pins.pid != std::process::id() {
            return Err("P38 process identity refused".into());
        }
        runtime_matches(&app, session).map_err(str::to_owned)?;
        super::verify_fixtures(session).map_err(str::to_owned)?;
        if let Err(reason) = accept_packet(session, &packet) {
            if reason == "P38 operation reported a fixed failure" {
                session.failed = true;
                session.failure_code = packet.failure_code.clone();
                session.persist("failed").map_err(str::to_owned)?;
            }
            return Err(reason.to_owned());
        }
        session.waiting = true;
        session.persist("waiting").map_err(str::to_owned)?;
        (session.root.clone(), session.pins.run_id.to_owned())
    };
    let expected_phase = packet.phase.clone();
    let deadline = Instant::now() + Duration::from_secs(90);
    let decision = loop {
        {
            let guard = state.0.lock().map_err(|_| "P38 probe unavailable")?;
            let session = guard.as_ref().ok_or("P38 probe unavailable")?;
            if session.root != root || session.pins.run_id != phase {
                break Err("P38 session replaced");
            }
            match read_ack(session, &expected_phase) {
                Ok(Some(ack)) => {
                    let _ = fs::remove_file(root.join("ack.json"));
                    break if ack.decision == "accepted" {
                        Ok(())
                    } else {
                        Err("P38 visible checkpoint refused")
                    };
                }
                Ok(None) => {}
                Err(error) => break Err(error),
            }
        }
        if Instant::now() >= deadline {
            break Err("P38 visible checkpoint timed out");
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    let mut guard = state.0.lock().map_err(|_| "P38 probe unavailable")?;
    let session = guard.as_mut().ok_or("P38 probe unavailable")?;
    if session.root != root || session.pins.run_id != phase || !session.waiting {
        return Err("P38 session changed during checkpoint".into());
    }
    if decision.is_ok() {
        let post_wait = (|| {
            if session.pins.pid != std::process::id() {
                return Err("P38 process identity refused");
            }
            runtime_matches(&app, session)?;
            super::verify_fixtures(session)?;
            let url = window.url().map_err(|_| "P38 window unavailable")?;
            if !super::control::origin(window.label(), &url) {
                return Err("P38 original main window refused");
            }
            Ok(())
        })();
        if let Err(reason) = post_wait {
            session.waiting = false;
            session.failed = true;
            session.failure_code = Some("post-wait-validation-refused".into());
            session.persist("failed").map_err(str::to_owned)?;
            return Err(reason.into());
        }
    }
    session.waiting = false;
    if decision.is_ok() {
        session.cursor += 1;
    } else {
        session.failed = true;
    }
    session
        .persist(if decision.is_ok() {
            "accepted"
        } else {
            "refused"
        })
        .map_err(str::to_owned)?;
    decision.map_err(str::to_owned)
}

pub(super) fn root_is_safe(path: &Path, dev: u64, ino: u64) -> bool {
    super::check_root(path, dev, ino).is_ok()
}

// Minimal SHA-256 used only to pin the two compile-time fixture byte strings.
pub(super) fn sha256_hex(bytes: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h = [
        0x6a09e667u32,
        0xbb67ae85,
        0x3c6ef372,
        0xa54ff53a,
        0x510e527f,
        0x9b05688c,
        0x1f83d9ab,
        0x5be0cd19,
    ];
    let mut data = bytes.to_vec();
    let bits = (data.len() as u64).wrapping_mul(8);
    data.push(0x80);
    while data.len() % 64 != 56 {
        data.push(0);
    }
    data.extend_from_slice(&bits.to_be_bytes());
    for block in data.chunks_exact(64) {
        let mut w = [0u32; 64];
        for (i, chunk) in block.chunks_exact(4).enumerate() {
            w[i] = u32::from_be_bytes(chunk.try_into().unwrap());
        }
        for i in 16..64 {
            let a = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let b = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(a)
                .wrapping_add(w[i - 7])
                .wrapping_add(b);
        }
        let mut v = h;
        for i in 0..64 {
            let s1 = v[4].rotate_right(6) ^ v[4].rotate_right(11) ^ v[4].rotate_right(25);
            let ch = (v[4] & v[5]) ^ (!v[4] & v[6]);
            let t1 = v[7]
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = v[0].rotate_right(2) ^ v[0].rotate_right(13) ^ v[0].rotate_right(22);
            let maj = (v[0] & v[1]) ^ (v[0] & v[2]) ^ (v[1] & v[2]);
            let t2 = s0.wrapping_add(maj);
            v = [
                t1.wrapping_add(t2),
                v[0],
                v[1],
                v[2],
                v[3].wrapping_add(t1),
                v[4],
                v[5],
                v[6],
            ];
        }
        for i in 0..8 {
            h[i] = h[i].wrapping_add(v[i]);
        }
    }
    h.iter().map(|v| format!("{v:08x}")).collect()
}

pub(super) fn origin(label: &str, url: &tauri::Url) -> bool {
    label == "main"
        && url.scheme() == "tauri"
        && url.host_str() == Some("localhost")
        && url.port().is_none()
        && url.username().is_empty()
        && url.password().is_none()
        && ["", "/", "/index.html"].contains(&url.path())
        && url.query().is_none()
        && url.fragment().is_none()
}
