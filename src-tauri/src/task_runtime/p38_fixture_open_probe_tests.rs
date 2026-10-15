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
fn ready_result_is_fixed_and_serializes_only_owner_acknowledgment() {
    use super::control::accepted_checkpoint_result;
    use serde_json::json;

    assert_eq!(
        serde_json::to_value(accepted_checkpoint_result("ready")).unwrap(),
        json!({"ownerAcknowledged":true})
    );
    for phase in ["a-opened", "b-opened", "b-closed", "private-unknown"] {
        assert!(accepted_checkpoint_result(phase).is_none());
    }
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
    for code in [
        "ready-open-path-unavailable",
        "ready-observer-install-failed",
    ] {
        assert!(allowed_ready_failure(code));
        for (cursor, fixture, hash, accepted) in [
            (0, None, None, true),
            (0, Some("A"), Some(HASH_A), false),
            (0, None, Some(HASH_A), false),
            (0, Some("A"), None, false),
            (1, None, None, false),
        ] {
            assert_eq!(
                valid_failure_packet(cursor, fixture, hash, Some(code)),
                accepted
            );
        }
    }
    for code in [
        "ready-tauri-invoke-unavailable",
        "private exception or path",
    ] {
        assert!(!allowed_ready_failure(code));
        assert!(!valid_failure_packet(0, None, None, Some(code)));
    }
}

#[test]
fn post_bootstrap_failure_accepts_only_finite_components_at_the_exact_fixture_cursor() {
    use super::control::valid_failure_packet;
    for stage in "receipt|installed-status|metadata|first-preview|first-owner|reveal|second-preview|second-owner|ui-commit|publication|subscriber".split('|') {
        for kind in ["invoke-rejected", "local-refusal"] {
            for native in "invalid_request|unavailable|unsupported_format|replacement_refused|stale_revision|stale_owner|cancelled|wrong_instance|wrong_document|cancelled_before_dispatch|unknown".split('|') {
                for disposition in ["notDispatched", "other"] {
                    for (cursor, fixture, hash, prefix) in [(1, "A", HASH_A, "a"), (2, "B", HASH_B, "b")] {
                        let code = format!("{prefix}-post-bootstrap-{stage}:{kind}:{native}:{disposition}");
                        assert!(valid_failure_packet(cursor, Some(fixture), Some(hash), Some(&code)));
                        assert!(!valid_failure_packet(3, Some(fixture), Some(hash), Some(&code)));
                    }
                }
            }
        }
    }
    for suffix in [
        "PRIVATE:invoke-rejected:unknown:other",
        "receipt:PRIVATE:unknown:other",
        "receipt:local-refusal:PRIVATE:other",
        "receipt:local-refusal:unknown:PRIVATE",
        "receipt:local-refusal:unknown:other:PRIVATE",
        "receipt:local-refusal:unknown",
    ] {
        assert!(!valid_failure_packet(
            1,
            Some("A"),
            Some(HASH_A),
            Some(&format!("a-post-bootstrap-{suffix}"))
        ));
    }
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
    let base = std::env::temp_dir().join(format!("p38-fixture-test-{}", uuid::Uuid::new_v4()));
    let temp = base.join("tmp");
    let run = uuid::Uuid::new_v4().to_string();
    let root = temp.join(format!("p38-fixture-open-{run}"));
    std::fs::create_dir_all(&root).unwrap();
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
            run_id: run,
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
    (TempProbeRoot(base), session)
}

#[cfg(unix)]
#[derive(Default)]
struct FixtureScope {
    grants: std::cell::RefCell<std::collections::HashSet<PathBuf>>,
    denies: std::cell::RefCell<std::collections::HashSet<PathBuf>>,
    calls: std::cell::RefCell<Vec<PathBuf>>,
}

#[cfg(unix)]
impl FixtureScope {
    fn allowed(&self, path: &Path) -> bool {
        self.grants.borrow().contains(path) && !self.denies.borrow().contains(path)
    }
    fn grant(&self, path: &Path) -> Result<(), ()> {
        self.calls.borrow_mut().push(path.to_path_buf());
        self.grants.borrow_mut().insert(path.to_path_buf());
        Ok(())
    }
    fn forbid(&self, path: &Path) -> Result<(), ()> {
        self.denies.borrow_mut().insert(path.to_path_buf());
        Ok(())
    }
    fn bind(&self, session: &Session, temp: &Path) -> Result<(), &'static str> {
        bind_fixture_scope(
            session,
            temp,
            |p| self.allowed(p),
            |p| self.grant(p),
            |p| self.forbid(p),
        )
    }
}

#[cfg(unix)]
#[test]
fn fixture_scope_grants_only_exact_verified_a_and_b() {
    let (_root, session) = fixture_session();
    let temp = session.root.parent().unwrap();
    let scope = FixtureScope::default();
    scope.bind(&session, temp).unwrap();
    assert_eq!(*scope.calls.borrow(), session.files);
    assert_eq!(scope.grants.borrow().len(), 2);
    assert!(scope.denies.borrow().is_empty());
    assert!(session.files.iter().all(|p| scope.allowed(p)));
    for excluded in [
        temp.to_path_buf(),
        session.root.clone(),
        temp.join("foreign.json"),
        session.root.join("C.json"),
        session.files[0].join("child"),
        session.files[1].join("child"),
    ] {
        assert!(!scope.allowed(&excluded));
    }
}

#[cfg(unix)]
#[test]
fn changed_or_foreign_fixture_identity_refuses_before_any_grant() {
    use std::os::unix::fs::symlink;
    for case in [
        "bytes",
        "missing",
        "symlink",
        "inode",
        "root",
        "run",
        "pin-id",
        "pin-hash",
        "pin-path",
        "pin-inode",
        "foreign",
        "temp",
    ] {
        let (base, mut session) = fixture_session();
        let temp = session.root.parent().unwrap().to_path_buf();
        match case {
            "bytes" => fs::write(&session.files[1], b"changed").unwrap(),
            "missing" => fs::remove_file(&session.files[1]).unwrap(),
            "symlink" | "inode" => {
                let replacement = base.0.join("replacement.json");
                write_new(&replacement, FIXTURE_A).unwrap();
                if case == "symlink" {
                    fs::remove_file(&session.files[0]).unwrap();
                    symlink(replacement, &session.files[0]).unwrap();
                } else {
                    fs::rename(replacement, &session.files[0]).unwrap();
                }
            }
            "root" => session.root_ino ^= 1,
            "run" => session.pins.run_id = uuid::Uuid::new_v4().to_string(),
            "pin-id" => session.pins.fixtures[0].id = "foreign".into(),
            "pin-hash" => session.pins.fixtures[0].sha256 = HASH_B.into(),
            "pin-path" => session.pins.fixtures[0].file.push_str("-foreign"),
            "pin-inode" => session.pins.fixtures[0].inode ^= 1,
            "foreign" => {
                session.files[0] = base.0.join("foreign.json");
                write_new(&session.files[0], FIXTURE_A).unwrap();
                session.file_ids[0] = fs::symlink_metadata(&session.files[0]).unwrap().identity();
                let pin = &mut session.pins.fixtures[0];
                pin.file = session.files[0].to_str().unwrap().into();
                (pin.device, pin.inode) = session.file_ids[0];
            }
            _ => {}
        }
        let scope = FixtureScope::default();
        let expected_temp = if case == "temp" {
            base.0.as_path()
        } else {
            &temp
        };
        assert!(scope.bind(&session, expected_temp).is_err(), "{case}");
        assert!(scope.calls.borrow().is_empty(), "{case}");
        assert!(scope.grants.borrow().is_empty(), "{case}");
        assert!(scope.denies.borrow().is_empty(), "{case}");
    }
}

#[cfg(unix)]
#[test]
fn preexisting_parent_sibling_descendant_or_file_scope_refuses_without_mutation() {
    let (_root, session) = fixture_session();
    let temp = session.root.parent().unwrap();
    for prior in [
        temp.to_path_buf(),
        session.root.clone(),
        temp.join("p38-scope-other.json"),
        session.root.join("C.json"),
        session.files[0].join("child"),
        session.files[1].clone(),
    ] {
        let scope = FixtureScope::default();
        scope.grants.borrow_mut().insert(prior.clone());
        assert!(scope.bind(&session, temp).is_err());
        assert!(scope.calls.borrow().is_empty());
        assert_eq!(
            *scope.grants.borrow(),
            std::collections::HashSet::from([prior])
        );
        assert!(scope.denies.borrow().is_empty());
    }
}

#[cfg(unix)]
#[test]
fn callback_failure_revokes_both_fixtures_even_after_partial_effect() {
    let (_root, session) = fixture_session();
    let temp = session.root.parent().unwrap();
    for failure_index in [0, 1] {
        let scope = FixtureScope::default();
        let result = bind_fixture_scope(
            &session,
            temp,
            |p| scope.allowed(p),
            |p| {
                scope.grant(p)?;
                if p == session.files[failure_index] {
                    Err(())
                } else {
                    Ok(())
                }
            },
            |p| scope.forbid(p),
        );
        assert_eq!(result, Err("P38 fixture scope binding refused"));
        assert!(session.files.iter().all(|p| !scope.allowed(p)));
        assert_eq!(scope.denies.borrow().len(), 2);
    }
}

#[cfg(unix)]
#[test]
fn ineffective_or_broad_binding_refuses_and_revokes_fixture_access() {
    let (_root, session) = fixture_session();
    let temp = session.root.parent().unwrap();
    for broad in [false, true] {
        let scope = FixtureScope::default();
        let result = bind_fixture_scope(
            &session,
            temp,
            |p| scope.allowed(p),
            |p| {
                if broad {
                    scope.grant(p)?;
                    scope.grants.borrow_mut().insert(session.root.clone());
                }
                Ok(())
            },
            |p| scope.forbid(p),
        );
        assert_eq!(result, Err("P38 fixture scope not exact"));
        assert!(session.files.iter().all(|p| !scope.allowed(p)));
    }
}

#[cfg(unix)]
#[test]
fn fixture_change_during_binding_and_rollback_error_refuse_initialization() {
    for rollback_error in [false, true] {
        let (_root, session) = fixture_session();
        let scope = FixtureScope::default();
        let result = bind_fixture_scope(
            &session,
            session.root.parent().unwrap(),
            |p| scope.allowed(p),
            |p| {
                scope.grant(p)?;
                fs::write(&session.files[1], b"changed during binding").unwrap();
                Ok(())
            },
            |p| {
                scope.forbid(p)?;
                if rollback_error {
                    Err(())
                } else {
                    Ok(())
                }
            },
        );
        assert!(result.is_err());
        if rollback_error {
            assert_eq!(result, Err("P38 fixture scope rollback refused"));
        }
        assert!(session.files.iter().all(|p| !scope.allowed(p)));
    }
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

#[cfg(unix)]
#[test]
fn inactive_b_checkpoints_reject_skips_replays_wrong_fixture_and_closed_failure_phase() {
    let (_root, mut session) = fixture_session();
    session.pins.build_sha = BUILD_SHA.into();
    super::control::assert_inactive_checkpoint_contract(&mut session);
}
