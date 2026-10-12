use super::*;
use std::collections::HashMap;

fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
    let map: HashMap<String, String> = pairs
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect();
    move |key: &str| map.get(key).cloned()
}

const KEY: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn full() -> Vec<(&'static str, &'static str)> {
    vec![
        (ENV_DATA_DIR, "/tmp/nemo-runtime/tasks/abc/tauri-data"),
        (ENV_TASK_ID, "task-a"),
        (ENV_TASK_KEY, KEY),
    ]
}

#[test]
fn absent_activation_variable_is_not_isolated() {
    // The production path: NEMO_TASK_ID alone (which the R06 build launcher
    // already exports) must NOT be enough to move the app's data roots.
    let resolved = TaskRuntime::resolve(env(&[(ENV_TASK_ID, "task-a"), (ENV_TASK_KEY, KEY)]));
    assert_eq!(resolved, Ok(None));
}

#[test]
fn empty_activation_variable_is_not_isolated() {
    assert_eq!(
        TaskRuntime::resolve(env(&[(ENV_DATA_DIR, "   ")])),
        Ok(None)
    );
}

#[test]
fn complete_request_resolves() {
    let runtime = TaskRuntime::resolve(env(&full())).unwrap().unwrap();
    assert_eq!(runtime.task_id, "task-a");
    assert_eq!(runtime.task_key, KEY);
    assert_eq!(
        runtime.data_dir,
        PathBuf::from("/tmp/nemo-runtime/tasks/abc/tauri-data")
    );
    assert_eq!(runtime.owner_token, None);
}

#[test]
fn incomplete_or_invalid_requests_fail_closed() {
    // Each of these once meant "fall back to com.strokemotion.app", i.e.
    // write an isolated run's state into the user's real app data.
    for (pairs, needle) in [
        (vec![(ENV_DATA_DIR, "/tmp/x")], ENV_TASK_ID),
        (
            vec![(ENV_DATA_DIR, "/tmp/x"), (ENV_TASK_ID, "task-a")],
            ENV_TASK_KEY,
        ),
        (
            vec![
                (ENV_DATA_DIR, "relative/path"),
                (ENV_TASK_ID, "task-a"),
                (ENV_TASK_KEY, KEY),
            ],
            ENV_DATA_DIR,
        ),
        (
            vec![
                (ENV_DATA_DIR, "/tmp/x"),
                (ENV_TASK_ID, "-leading-hyphen"),
                (ENV_TASK_KEY, KEY),
            ],
            ENV_TASK_ID,
        ),
        (
            vec![
                (ENV_DATA_DIR, "/tmp/x"),
                (ENV_TASK_ID, "task a"),
                (ENV_TASK_KEY, KEY),
            ],
            ENV_TASK_ID,
        ),
        (
            vec![
                (ENV_DATA_DIR, "/tmp/x"),
                (ENV_TASK_ID, "task-a"),
                (ENV_TASK_KEY, "ABCDEF"),
            ],
            ENV_TASK_KEY,
        ),
        (
            vec![
                (ENV_DATA_DIR, "/tmp/x"),
                (ENV_TASK_ID, "task-a"),
                (ENV_TASK_KEY, KEY),
                (ENV_OWNER_TOKEN, "no"),
            ],
            ENV_OWNER_TOKEN,
        ),
    ] {
        let err = TaskRuntime::resolve(env(&pairs)).expect_err("must refuse");
        assert!(err.contains(needle), "{err} should name {needle}");
    }
}

#[test]
fn uppercase_task_key_is_refused_rather_than_lowercased() {
    // isolation.cjs emits lowercase hex; accepting a normalized variant
    // would let two spellings of one task address two different stores.
    let mut pairs = full();
    pairs[2] = (
        ENV_TASK_KEY,
        "0123456789ABCDEF0123456789abcdef0123456789abcdef0123456789abcdef",
    );
    assert!(TaskRuntime::resolve(env(&pairs)).is_err());
}

#[test]
fn owner_token_is_optional_and_kept_out_of_the_identifier() {
    let mut pairs = full();
    pairs.push((ENV_OWNER_TOKEN, "0123456789abcdef0123"));
    let runtime = TaskRuntime::resolve(env(&pairs)).unwrap().unwrap();
    assert_eq!(runtime.owner_token.as_deref(), Some("0123456789abcdef0123"));
    assert!(!runtime
        .identifier("com.strokemotion.app")
        .contains("0123456789abcdef0123"));
}

#[test]
fn identifier_is_per_task_and_bundle_safe() {
    let a = isolated_identifier("com.strokemotion.app", KEY);
    let b = isolated_identifier(
        "com.strokemotion.app",
        "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210",
    );
    assert_eq!(a, "com.strokemotion.app.nemo-task-0123456789abcdef");
    assert_ne!(a, b);
    assert!(a
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-'));
}

#[test]
fn data_store_identifier_is_the_first_sixteen_key_bytes() {
    assert_eq!(
        data_store_identifier(KEY),
        [
            0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0x01, 0x23, 0x45, 0x67, 0x89, 0xab,
            0xcd, 0xef
        ]
    );
    // Distinct tasks must not share a WebKit store.
    assert_ne!(
        data_store_identifier(KEY),
        data_store_identifier("fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210")
    );
}

#[test]
fn task_id_rule_matches_the_node_library() {
    for ok in ["a", "task-a", "Task.A_1", "0", &"a".repeat(120)] {
        assert!(valid_task_id(ok), "{ok} should be accepted");
    }
    for bad in [
        "",
        "-a",
        ".a",
        "_a",
        "a b",
        "a/b",
        "a\u{e9}",
        &"a".repeat(121),
    ] {
        assert!(!valid_task_id(bad), "{bad} should be rejected");
    }
}

#[test]
fn manifest_file_lives_inside_the_task_data_root() {
    let runtime = TaskRuntime::resolve(env(&full())).unwrap().unwrap();
    assert_eq!(
        runtime.manifest_file(),
        PathBuf::from("/tmp/nemo-runtime/tasks/abc/tauri-data/native-runtime.json")
    );
}

#[cfg(feature = "test-play-publication-observer")]
#[test]
fn play_metadata_has_no_free_text_payload_or_coerced_identity() {
    use crate::test_play_observer::TestPlayLog;
    for bad in [
        json!({"event":"play-stop", "path":"/private/secret"}),
        json!({"event":"host-receipt", "workId":159}),
        json!({"event":"host-receipt", "documentId":"/private/file"}),
        json!({"event":"host-receipt", "contentRevision":"2"}),
        json!({"event":"commit-return", "frame":-1}),
        json!({"event":"host-receipt", "status":"secret"}),
        json!({"event":"host-receipt", "owner":"paper"}),
        json!({"event":"play-stop", "result":{}}),
    ] {
        assert!(!TestPlayLog::valid_event(&bad));
    }
    let event = json!({"event":"host-receipt", "documentId":"native-document-3",
        "workId":"native-work-159", "frame":10, "contentRevision":2, "status":"presented"});
    assert!(TestPlayLog::valid_event(&event));
}

#[cfg(feature = "test-play-publication-observer")]
#[test]
fn play_collection_seal_refuses_missing_ready_gaps_duplicates_and_overflow() {
    use crate::test_play_observer::TestPlayLog;
    let run = "da5f47f9-5681-4e71-a0b6-aa7578b6b5fa";
    let mut clean = TestPlayLog::new(run);
    assert!(clean.record(run, 1, json!({"event":"ready"})).is_ok());
    assert!(clean.record(run, 2, json!({"event":"play-stop"})).is_ok());
    assert_eq!(clean.seal(run, 2, 0, 0)["status"], "complete");
    for mode in [
        "missing-ready",
        "gap",
        "duplicate",
        "wrong-run",
        "drop",
        "transport",
    ] {
        let mut log = TestPlayLog::new(run);
        if mode != "missing-ready" {
            log.record(run, 1, json!({"event":"ready"})).unwrap();
        }
        if mode == "gap" {
            assert!(log.record(run, 3, json!({"event":"play-stop"})).is_err());
        }
        if mode == "duplicate" {
            assert!(log.record(run, 1, json!({"event":"ready"})).is_err());
        }
        if mode == "wrong-run" {
            assert!(log
                .record("other", 2, json!({"event":"play-stop"}))
                .is_err());
        }
        let footer = log.seal(
            run,
            log.accepted,
            u64::from(mode == "drop"),
            u64::from(mode == "transport"),
        );
        assert_eq!(footer["status"], "partial", "{mode}");
    }
}

#[cfg(feature = "test-play-publication-observer")]
#[test]
fn play_record_cap_and_terminal_cut_never_ack_more_records() {
    let run = "da5f47f9-5681-4e71-a0b6-aa7578b6b5fa";
    let mut log = crate::test_play_observer::TestPlayLog::new(run);
    log.record(run, 1, json!({"event":"ready"})).unwrap();
    for sequence in 2..=4096 {
        log.record(run, sequence, json!({"event":"play-step"}))
            .unwrap();
    }
    assert!(log.record(run, 4097, json!({"event":"play-step"})).is_err());
    assert_eq!(log.seal(run, 4097, 0, 0)["status"], "partial");
    assert!(log.record(run, 4098, json!({"event":"play-stop"})).is_err());
    assert_eq!(log.accepted, 4096);
}

#[cfg(feature = "test-play-publication-observer")]
#[test]
fn malformed_typed_play_envelopes_are_counted_before_any_footer() {
    let run = "da5f47f9-5681-4e71-a0b6-aa7578b6b5fa";
    for (action, payload) in [
        (
            "record",
            json!({"runId":run,"sequence":2,"event":{"event":"play-stop"},"secret":"private"}),
        ),
        ("record", json!([run, 2, {"event":"play-stop"}])),
        (
            "record",
            json!({"runId":run,"sequence":"2","event":{"event":"play-stop"}}),
        ),
        (
            "record",
            json!({"runId":run,"sequence":2.0,"event":{"event":"play-stop"}}),
        ),
        (
            "seal",
            json!({"runId":run,"generated":1,"dropped":0,"failed":0,"extra":true}),
        ),
        (
            "seal",
            json!({"runId":run,"generated":1,"dropped":0,"failed":9_007_199_254_740_992_u64}),
        ),
        ("other", json!({"runId":run})),
    ] {
        let mut log = crate::test_play_observer::TestPlayLog::new(run);
        log.record(run, 1, json!({"event":"ready"})).unwrap();
        assert!(log.dispatch(action, payload).is_err());
        assert_eq!(log.rejected, 1);
        assert_eq!(log.seal(run, 1, 0, 0)["status"], "partial");
    }
}

#[cfg(feature = "test-play-publication-observer")]
#[test]
fn a_malformed_envelope_after_seal_is_an_explicit_late_tail() {
    let run = "da5f47f9-5681-4e71-a0b6-aa7578b6b5fa";
    let mut log = crate::test_play_observer::TestPlayLog::new(run);
    log.record(run, 1, json!({"event":"ready"})).unwrap();
    assert_eq!(log.seal(run, 1, 0, 0)["status"], "complete");
    assert!(log.dispatch("other", json!({"secret":"private"})).is_err());
    assert_eq!(log.rejected, 1);
    assert_eq!(log.seal(run, 1, 0, 0)["status"], "partial");
}

#[cfg(feature = "test-play-publication-observer")]
#[test]
fn seal_marker_is_bounded_exact_run_and_not_a_symlink_or_directory() {
    use crate::test_play_observer::seal_marker_valid;
    let root = std::env::temp_dir().join(format!("nemo-play-marker-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    let marker = root.join("marker");
    let run = "da5f47f9-5681-4e71-a0b6-aa7578b6b5fa";
    assert!(!seal_marker_valid(&marker, run));
    std::fs::write(&marker, json!({"runId":run}).to_string()).unwrap();
    assert!(seal_marker_valid(&marker, run));
    assert!(!seal_marker_valid(&marker, "other"));
    std::fs::write(&marker, "x".repeat(129)).unwrap();
    assert!(!seal_marker_valid(&marker, run));
    std::fs::write(&marker, json!({"runId":run,"secret":"private"}).to_string()).unwrap();
    assert!(!seal_marker_valid(&marker, run));
    assert!(!seal_marker_valid(&root, run));
    #[cfg(unix)]
    {
        let link = root.join("link");
        std::os::unix::fs::symlink(&marker, &link).unwrap();
        assert!(!seal_marker_valid(&link, run));
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[cfg(feature = "test-play-publication-observer")]
#[test]
fn only_pinned_main_packaged_root_urls_can_send_play_metadata() {
    use crate::test_play_observer::TestPlayLog;
    for address in [
        "tauri://localhost",
        "tauri://localhost/",
        "tauri://localhost/index.html",
    ] {
        let url = tauri::Url::parse(address).unwrap();
        assert!(TestPlayLog::allowed_origin("main", &url));
        assert!(!TestPlayLog::allowed_origin("aux", &url));
    }
    for address in [
        "https://localhost/",
        "http://localhost:1420/",
        "tauri://other/",
        "tauri://localhost/other.html",
        "tauri://user:secret@localhost/",
        "tauri://localhost:3000/",
    ] {
        assert!(!TestPlayLog::allowed_origin(
            "main",
            &tauri::Url::parse(address).unwrap()
        ));
    }
}
