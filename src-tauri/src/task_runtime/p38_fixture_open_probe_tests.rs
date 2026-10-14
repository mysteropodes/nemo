use super::control::sha256_hex;
use super::*;

#[test]
fn fixture_bytes_match_reviewed_pins() {
    assert_eq!(sha256_hex(FIXTURE_A), HASH_A);
    assert_eq!(sha256_hex(FIXTURE_B), HASH_B);
}

#[test]
fn sha256_matches_standard_empty_and_abc_vectors() {
    assert_eq!(
        sha256_hex(b""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(
        sha256_hex(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[test]
fn a_scope_diagnostic_is_fixed_and_serializes_only_its_allowlisted_state() {
    use super::control::{a_fixture_scope_status, AOpenScopeDiagnostic};
    use serde_json::json;

    assert_eq!(a_fixture_scope_status(Some(true)), "allowed");
    assert_eq!(a_fixture_scope_status(Some(false)), "refused");
    assert_eq!(a_fixture_scope_status(None), "unavailable");
    let packet = AOpenScopeDiagnostic {
        a_fixture_scope: a_fixture_scope_status(Some(false)),
    };
    assert_eq!(
        serde_json::to_value(packet).unwrap(),
        json!({"aFixtureScope":"refused"})
    );
}

#[test]
fn categorical_open_failures_accept_only_the_fixed_fixture_allowlist() {
    for code in [
        "a-open-false",
        "a-pre-native-refusal",
        "a-native-status-rejected",
        "a-native-bootstrap-unsupported-format",
        "a-native-bootstrap-rejected",
        "a-native-bootstrap-resolved-open-refused",
        "a-fs-read-rejected",
        "a-native-status-empty-no-bootstrap",
        "a-native-status-available-no-bootstrap",
        "a-native-status-malformed-no-bootstrap",
        "a-observer-late-untrusted",
        "a-fs-read-resolved-open-refused",
    ] {
        assert!(super::control::allowed_a_failure(code), "{code}");
    }
    for code in [
        "b-open-false",
        "b-pre-native-refusal",
        "b-native-status-rejected",
        "b-native-bootstrap-unsupported-format",
        "b-native-bootstrap-rejected",
        "b-native-bootstrap-resolved-open-refused",
        "b-fs-read-rejected",
        "b-native-status-empty-no-bootstrap",
        "b-native-status-available-no-bootstrap",
        "b-native-status-malformed-no-bootstrap",
        "b-observer-late-untrusted",
        "b-fs-read-resolved-open-refused",
    ] {
        assert!(super::control::allowed_b_failure(code), "{code}");
    }
    for code in [
        "a-native-bootstrap-/Users/private/A.json",
        "a-native-bootstrap-secret error text",
        "b-fs-read-/private/path",
        "unsupported_format",
    ] {
        assert!(!super::control::allowed_a_failure(code), "{code}");
        assert!(!super::control::allowed_b_failure(code), "{code}");
    }
}

#[test]
fn pre_ready_failure_accepts_only_the_fixed_code_without_fixture_identity() {
    use super::control::{allowed_ready_failure, valid_failure_packet};
    assert!(allowed_ready_failure("ready-open-path-unavailable"));
    assert!(allowed_ready_failure("ready-observer-install-failed"));
    assert!(!allowed_ready_failure("ready-tauri-invoke-unavailable"));
    assert!(!allowed_ready_failure("private exception or path"));
    assert!(valid_failure_packet(
        0,
        None,
        None,
        Some("ready-open-path-unavailable")
    ));
    assert!(valid_failure_packet(
        0,
        None,
        None,
        Some("ready-observer-install-failed")
    ));
    assert!(!valid_failure_packet(
        0,
        Some("A"),
        Some(HASH_A),
        Some("ready-open-path-unavailable")
    ));
    assert!(!valid_failure_packet(
        0,
        None,
        Some(HASH_A),
        Some("ready-open-path-unavailable")
    ));
    assert!(!valid_failure_packet(
        0,
        None,
        None,
        Some("private exception or path")
    ));
    assert!(!valid_failure_packet(
        0,
        Some("A"),
        Some(HASH_A),
        Some("ready-observer-install-failed")
    ));
    assert!(!valid_failure_packet(
        1,
        None,
        None,
        Some("ready-open-path-unavailable")
    ));
    assert!(!valid_failure_packet(
        1,
        None,
        None,
        Some("ready-observer-install-failed")
    ));
}

#[test]
fn disabled_probe_is_inert_and_enabled_probe_requires_exact_release_identity() {
    assert_eq!(
        gate(false, false, false, false, false, false, "", ""),
        Ok(false)
    );
    assert!(gate(
        true,
        true,
        true,
        true,
        true,
        true,
        &"a".repeat(40),
        &"b".repeat(40)
    )
    .is_err());
    assert_eq!(
        gate(
            true,
            true,
            true,
            true,
            true,
            true,
            &"a".repeat(40),
            &"a".repeat(40)
        ),
        Ok(true)
    );
}

#[test]
fn non_release_or_non_isolated_profiles_are_refused() {
    let pin = "a".repeat(40);
    assert!(gate(true, false, true, true, true, true, &pin, &pin).is_err());
    assert!(gate(true, true, false, true, true, true, &pin, &pin).is_err());
    assert!(gate(true, true, true, false, true, true, &pin, &pin).is_err());
    assert!(gate(true, true, true, true, false, true, &pin, &pin).is_err());
    assert!(gate(true, true, true, true, true, false, &pin, &pin).is_err());
}

#[cfg(unix)]
struct TempProbeRoot(std::path::PathBuf);

#[cfg(unix)]
impl Drop for TempProbeRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[cfg(unix)]
fn fixture_session() -> (TempProbeRoot, Session) {
    use std::os::unix::fs::MetadataExt;
    let root = std::env::temp_dir().join(format!("p38-fixture-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    let root = std::fs::canonicalize(root).unwrap();
    let files = [root.join("A.json"), root.join("B.json")];
    write_new(&files[0], FIXTURE_A).unwrap();
    write_new(&files[1], FIXTURE_B).unwrap();
    let file_ids = [
        std::fs::symlink_metadata(&files[0]).unwrap().identity(),
        std::fs::symlink_metadata(&files[1]).unwrap().identity(),
    ];
    let root_meta = std::fs::symlink_metadata(&root).unwrap();
    let fixture = |id: &'static str, index: usize, hash: &'static str| FixturePin {
        id: id.into(),
        file: files[index].to_string_lossy().into_owned(),
        sha256: hash.into(),
        device: file_ids[index].0,
        inode: file_ids[index].1,
    };
    let session = Session {
        pins: Pins {
            schema: "nemo.p38-fixture-open/1",
            run_id: uuid::Uuid::new_v4().to_string(),
            task_key: "a".repeat(64),
            source_sha: "b".repeat(40),
            build_sha: "b".repeat(40),
            pid: std::process::id(),
            window: "main",
            origin: "tauri://localhost/index.html",
            fixtures: [fixture("A", 0, HASH_A), fixture("B", 1, HASH_B)],
        },
        pins_sha256: "c".repeat(64),
        owner: "d".repeat(32),
        root: root.clone(),
        root_dev: root_meta.dev(),
        root_ino: root_meta.ino(),
        files,
        file_ids,
        cursor: 0,
        waiting: false,
        failed: false,
        failure_code: None,
    };
    (TempProbeRoot(root), session)
}

#[cfg(unix)]
#[test]
fn fixture_verification_rejects_same_bytes_from_a_replacement_file() {
    let (root, session) = fixture_session();
    assert!(verify_fixtures(&session).is_ok());
    let replacement = root.0.join("replacement.json");
    write_new(&replacement, FIXTURE_A).unwrap();
    std::fs::remove_file(&session.files[0]).unwrap();
    std::fs::copy(&replacement, &session.files[0]).unwrap();
    assert_eq!(verify_fixtures(&session), Err("P38 fixture refused"));
}

#[cfg(unix)]
#[test]
fn fixture_verification_rejects_same_bytes_through_a_symlink() {
    use std::os::unix::fs::symlink;
    let (root, session) = fixture_session();
    let replacement = root.0.join("replacement.json");
    write_new(&replacement, FIXTURE_A).unwrap();
    std::fs::remove_file(&session.files[0]).unwrap();
    symlink(&replacement, &session.files[0]).unwrap();
    assert!(verify_fixtures(&session).is_err());
}

#[test]
fn main_window_origin_refuses_other_labels_and_url_components() {
    let main = tauri::Url::parse("tauri://localhost/index.html").unwrap();
    let query = tauri::Url::parse("tauri://localhost/index.html?foreign=1").unwrap();
    let foreign = tauri::Url::parse("tauri://localhost/other.html").unwrap();
    assert!(control::origin("main", &main));
    assert!(!control::origin("child", &main));
    assert!(!control::origin("main", &query));
    assert!(!control::origin("main", &foreign));
}
