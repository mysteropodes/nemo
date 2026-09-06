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

#[test]
fn unsupported_isolated_startup_fails_before_config_or_window_creation() {
    let result = TaskRuntime::resolve_for_startup(env(&full()), || false);
    let err = result.expect_err("macOS below 14 must not reach Tauri with an isolated request");
    assert!(err.contains("macOS 14"), "{err}");
    assert!(err.contains("WebKit"), "{err}");
}

#[test]
fn ordinary_startup_does_not_probe_isolated_store_support() {
    for pairs in [
        vec![],
        vec![(ENV_TASK_ID, "task-a"), (ENV_TASK_KEY, KEY)],
        vec![(ENV_DATA_DIR, "   ")],
    ] {
        assert_eq!(
            TaskRuntime::resolve_for_startup(env(&pairs), || panic!(
                "ordinary startup must not probe"
            )),
            Ok(None)
        );
    }
}

#[test]
fn invalid_request_fails_before_platform_probe() {
    let err = TaskRuntime::resolve_for_startup(env(&[(ENV_DATA_DIR, "/tmp/x")]), || {
        panic!("invalid request must not probe")
    })
    .unwrap_err();
    assert!(err.contains(ENV_TASK_ID), "{err}");
}

#[test]
fn supported_startup_preserves_distinct_persistent_store_configuration() {
    let a = TaskRuntime::resolve_for_startup(env(&full()), || true)
        .unwrap()
        .unwrap();
    let mut pairs = full();
    pairs[1] = (ENV_TASK_ID, "task-b");
    pairs[2] = (
        ENV_TASK_KEY,
        "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210",
    );
    let b = TaskRuntime::resolve_for_startup(env(&pairs), || true)
        .unwrap()
        .unwrap();
    assert_eq!(a.data_store_identifier(), data_store_identifier(KEY));
    assert_ne!(a.data_store_identifier(), b.data_store_identifier());

    let mut config = tauri::utils::config::Config::default();
    config.identifier = "com.strokemotion.app".into();
    config.app.windows = vec![tauri::utils::config::WindowConfig::default()];
    let label = config.app.windows[0].label.clone();
    a.apply_to_config(&mut config);
    assert_eq!(
        config.identifier,
        "com.strokemotion.app.nemo-task-0123456789abcdef"
    );
    // Tauri must leave window creation to start(), which attaches the store ID.
    assert!(!config.app.windows[0].create);
    assert!(!config.app.windows[0].incognito);
    assert_eq!(config.app.windows[0].label, label);
}

#[cfg(target_os = "macos")]
#[test]
fn native_store_support_matches_host_macos_version() {
    // Exercise the real Foundation/objc ABI without creating an app or store.
    // sw_vers supplies an independent host-version observation for this check.
    let output = std::process::Command::new("/usr/bin/sw_vers")
        .arg("-productVersion")
        .output()
        .unwrap();
    assert!(output.status.success());
    let version = String::from_utf8(output.stdout).unwrap();
    let major: u32 = version.trim().split('.').next().unwrap().parse().unwrap();
    assert_eq!(supports_isolated_webkit_store(), major >= 14, "{version}");
}
