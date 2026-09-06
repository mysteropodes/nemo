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
