use super::*;

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("nemo-startup-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        Self(std::fs::canonicalize(root).unwrap())
    }
    fn log(&self) -> LoadLog {
        LoadLog::new(
            "test-run".into(),
            "a".repeat(64),
            42,
            open_sink(&self.0, &self.0).unwrap(),
        )
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
fn origin() -> tauri::Url {
    tauri::Url::parse("tauri://localhost/index.html").unwrap()
}
fn event(log: &mut LoadLog, stage: &str) {
    if stage == "window-created" {
        log.confirm_creation("main", 42, "test-run");
    } else {
        log.observe(stage, "main", &origin(), 42, "test-run");
    }
}
fn window() -> tauri::utils::config::WindowConfig {
    let mut window = tauri::utils::config::WindowConfig::default();
    window.label = "main".into();
    window.url = tauri::WebviewUrl::App("index.html".into());
    window
}

#[test]
fn gates_require_opt_in_owned_release_single_main_without_environment_mutation() {
    let main = window();
    assert_eq!(gate(false, true, true, &[main.clone()]), Ok(false));
    assert_eq!(gate(true, false, true, &[main.clone()]), Ok(false));
    assert_eq!(gate(true, true, false, &[main.clone()]).is_err(), true);
    assert!(gate(true, true, true, &[]).is_err());
    assert!(gate(true, true, true, &[main.clone(), main.clone()]).is_err());
    let mut foreign = main.clone();
    foreign.label = "foreign".into();
    assert!(gate(true, true, true, &[foreign]).is_err());
    let mut external = main.clone();
    external.url = tauri::WebviewUrl::External(tauri::Url::parse("https://example.com").unwrap());
    assert!(gate(true, true, true, &[external]).is_err());
    assert_eq!(gate(true, true, true, &[main]), Ok(true));
}

#[test]
fn exact_owned_origin_refuses_foreign_credentials_query_fragment_and_port() {
    assert!(allowed_origin("main", &origin()));
    assert!(!allowed_origin("foreign", &origin()));
    for raw in [
        "https://localhost/index.html",
        "tauri://foreign/index.html",
        "tauri://localhost:99/index.html",
        "tauri://user@localhost/index.html",
        "tauri://localhost/private",
        "tauri://localhost/index.html?private=secret",
        "tauri://localhost/index.html#secret",
    ] {
        assert!(
            !allowed_origin("main", &tauri::Url::parse(raw).unwrap()),
            "{raw}"
        );
    }
}

#[test]
fn native_sequence_remains_explicitly_unsealed_and_never_claims_js_or_paint() {
    let fixture = Fixture::new();
    let mut log = fixture.log();
    assert_eq!(log.receipt()["status"], "partial");
    event(&mut log, "window-created");
    event(&mut log, "native-page-started");
    assert_eq!(log.receipt()["status"], "partial");
    event(&mut log, "native-page-finished");
    log.persist().unwrap();
    let value: Value =
        serde_json::from_slice(&std::fs::read(fixture.0.join(FILE)).unwrap()).unwrap();
    assert_eq!(value["status"], "native-load-observed");
    assert_eq!(value["accepted"], 3);
    assert_eq!(value["sealed"], false);
    assert_eq!(value["jsReadiness"], "not-observed");
    assert_eq!(value["visiblePaint"], "not-observed");
    assert_eq!(value["stages"].as_array().unwrap().len(), 3);
    assert!(std::fs::read(fixture.0.join(FILE)).unwrap().len() < MAX_BYTES);
}

#[test]
fn wrong_identity_unknown_stage_and_out_of_order_never_become_success() {
    for variant in 0..6 {
        let fixture = Fixture::new();
        let mut log = fixture.log();
        match variant {
            0 => log.confirm_creation("main", 43, "test-run"),
            1 => log.confirm_creation("main", 42, "foreign-run"),
            2 => log.confirm_creation("foreign", 42, "test-run"),
            3 => event(&mut log, "secret-console-payload"),
            4 => event(&mut log, "native-page-finished"),
            _ => log.observe(
                "native-page-started",
                "main",
                &tauri::Url::parse("https://private.example/secret").unwrap(),
                42,
                "test-run",
            ),
        }
        for stage in [
            "window-created",
            "native-page-started",
            "native-page-finished",
        ] {
            event(&mut log, stage);
        }
        assert_eq!(log.receipt()["status"], "refused");
        assert_eq!(log.receipt()["accepted"], 0);
        log.persist().unwrap();
        let bytes = String::from_utf8(std::fs::read(fixture.0.join(FILE)).unwrap()).unwrap();
        for private in ["secret-console", "private.example", "foreign-run", "secret"] {
            assert!(!bytes.contains(private));
        }
    }
}

#[test]
fn duplicate_and_event_overflow_are_bounded_and_refuse_even_after_finished() {
    let fixture = Fixture::new();
    let mut log = fixture.log();
    for stage in [
        "window-created",
        "native-page-started",
        "native-page-finished",
    ] {
        event(&mut log, stage);
    }
    for _ in 0..100 {
        event(&mut log, "native-page-finished");
    }
    assert_eq!(log.receipt()["status"], "refused");
    assert_eq!(log.stages.len(), 3);
    assert_eq!(log.attempts, LIMIT);
    assert!(log.dropped > 0);
    assert!(log.rejected <= LIMIT);
    log.persist().unwrap();
    assert!(std::fs::read(fixture.0.join(FILE)).unwrap().len() < MAX_BYTES);
}

#[test]
fn actual_window_confirmation_during_build_does_not_fabricate_duplicate_creation() {
    let fixture = Fixture::new();
    let mut log = fixture.log();
    log.confirm_creation("main", 42, "test-run");
    event(&mut log, "native-page-started");
    log.confirm_creation("main", 42, "test-run");
    event(&mut log, "native-page-finished");
    assert_eq!(log.receipt()["accepted"], 3);
    assert_eq!(log.receipt()["rejected"], 0);
    assert_eq!(log.receipt()["status"], "native-load-observed");
}

#[test]
fn owned_creation_without_document_is_partial_and_late_native_load_can_complete() {
    let fixture = Fixture::new();
    let mut log = fixture.log();
    log.confirm_creation("main", 42, "test-run"); // No URL/readiness input exists.
    log.persist().unwrap();
    assert_eq!(log.receipt()["status"], "partial");
    assert_eq!(log.receipt()["accepted"], 1);
    assert_eq!(log.receipt()["stages"][0]["stage"], "window-created");
    event(&mut log, "native-page-started");
    event(&mut log, "native-page-finished");
    assert_eq!(log.receipt()["status"], "native-load-observed");
}

#[test]
fn actual_nonapp_callback_origin_refuses_after_valid_owned_creation() {
    let fixture = Fixture::new();
    let mut log = fixture.log();
    log.confirm_creation("main", 42, "test-run");
    log.observe(
        "native-page-started",
        "main",
        &tauri::Url::parse("about:blank").unwrap(),
        42,
        "test-run",
    );
    event(&mut log, "native-page-started");
    event(&mut log, "native-page-finished");
    assert_eq!(log.receipt()["status"], "refused");
    assert_eq!(log.receipt()["accepted"], 1);
}

#[test]
#[cfg(target_os = "macos")]
fn standard_var_alias_accepts_the_same_owned_root() {
    let fixture = Fixture::new();
    let canonical = fixture.0.to_str().unwrap();
    assert!(canonical.starts_with("/private/var/"));
    let alias = PathBuf::from(canonical.replacen("/private/var/", "/var/", 1));
    assert_ne!(alias, fixture.0);
    let _file = open_sink(&alias, &fixture.0).unwrap();
    assert!(fixture.0.join(FILE).is_file());
}

#[test]
fn sink_refuses_escape_duplicate_foreign_file_and_symlinks_without_overwriting() {
    let fixture = Fixture::new();
    let foreign = Fixture::new();
    assert!(open_sink(&foreign.0, &fixture.0).is_err());
    assert!(open_sink(&fixture.0.join("missing"), &fixture.0.join("missing")).is_err());
    std::fs::write(fixture.0.join(FILE), b"foreign-private-bytes").unwrap();
    assert!(open_sink(&fixture.0, &fixture.0).is_err());
    assert_eq!(
        std::fs::read(fixture.0.join(FILE)).unwrap(),
        b"foreign-private-bytes"
    );
    #[cfg(unix)]
    {
        std::fs::remove_file(fixture.0.join(FILE)).unwrap();
        std::os::unix::fs::symlink(foreign.0.join(FILE), fixture.0.join(FILE)).unwrap();
        assert!(open_sink(&fixture.0, &fixture.0).is_err());
        std::os::unix::fs::symlink(&foreign.0, fixture.0.join("alias")).unwrap();
        assert!(open_sink(&fixture.0.join("alias"), &fixture.0.join("alias")).is_err());
        assert!(!foreign.0.join(FILE).exists());
    }
}
