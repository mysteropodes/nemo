//! Failure/atomicity controls split without removing assertions.
use crate::native_dispatch::{NativeDispatch, NativeReleaseProgress};
use crate::native_object_bootstrap::save_tests::*;
use native_engine::{
    commands::{OpacityRequest, ResponseEnvelope},
    document::OpacityDocument,
    export_job::{JobReceipt, PendingFrame},
};
use std::sync::{atomic::Ordering, Arc};

struct NonObjectFixture {
    instance: String,
    pump: TerminalPump,
}

#[test]
#[cfg(unix)]
fn normal_recursive_scope_publishes_without_allowing_hidden_siblings() {
    let scratch = Scratch::new();
    let state = ApplicationMcp::default();
    let pins = install(&state, ORIGINAL);
    let destination = scratch.0.join("scoped.json");
    let pattern = tauri::fs::Pattern::new(&format!(
        "{}/**",
        tauri::fs::Pattern::escape(&scratch.0.to_string_lossy())
    ))
    .unwrap();
    let hidden = scratch.0.join(".unrelated-secret");
    // The real pinned Tauri glob type: plain glob defaults allow a dotfile,
    // while Tauri's Unix scope explicitly requires a literal leading dot.
    let mut options = Default::default();
    assert!(pattern.matches_path_with(&hidden, options));
    options.require_literal_separator = true;
    options.require_literal_leading_dot = true;
    assert!(pattern.matches_path_with(&destination, options));
    assert!(!pattern.matches_path_with(&hidden, options));

    let receipt = save_request(
        &state,
        &request(&pins, 0, &destination).to_string(),
        |path| pattern.matches_path_with(path, options),
        |_, _| Ok(()),
    )
    .expect("an authorized ordinary destination must publish without a hidden-file grant");
    let receipt = serde_json::to_value(receipt).unwrap();
    assert_eq!(receipt["published"], true);
    assert_eq!(receipt["cleanupPending"], false);
    assert_eq!(fs::read_to_string(&destination).unwrap(), ORIGINAL);
    assert_eq!(scratch.files(), 1, "no staging sibling remains");
    assert!(!pattern.matches_path_with(&hidden, options));

    // A picker-only destination grant still cannot authorize an arbitrary
    // staging sibling; the fix must retain both existing scope checks.
    let second = scratch.0.join("picker-only.json");
    assert!(save_request(
        &state,
        &request(&pins, 0, &second).to_string(),
        |path| path == second,
        |_, _| panic!("destination-only permission reached a filesystem effect"),
    )
    .is_err());
    assert_eq!(scratch.files(), 1);
}
impl NativeDispatch for NonObjectFixture {
    fn instance_id(&self) -> &str {
        &self.instance
    }
    fn document_id(&self) -> &str {
        self.pump.document_id()
    }
    fn content_revision(&self) -> u64 {
        self.pump.content_revision()
    }
    fn dispatch(&mut self, request: OpacityRequest) -> Result<ResponseEnvelope, String> {
        self.pump.dispatch(request)
    }
    fn replace_document(&mut self, document: OpacityDocument) -> Result<Vec<JobReceipt>, String> {
        self.pump.replace_document(document)
    }
    fn start_next_export_frame(&mut self, job: &str) -> Result<Option<PendingFrame>, String> {
        self.pump.start_next_export_frame(job)
    }
    fn finish_export_frame(&mut self, frame: PendingFrame) -> Result<JobReceipt, String> {
        self.pump.finish_export_frame(frame)
    }
    fn release_project(&mut self) -> Result<NativeReleaseProgress, String> {
        self.pump.release_project()
    }
    fn release_progress(&self) -> Option<NativeReleaseProgress> {
        self.pump.release_progress()
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

#[test]
fn cancelled_denied_scope_missing_parent_and_stale_pins_never_stage() {
    let scratch = Scratch::new();
    let state = ApplicationMcp::default();
    let pins = install(&state, ORIGINAL);
    let valid = request(&pins, 0, &scratch.0.join("new.json"));
    let mut cancelled = valid.clone();
    cancelled["cancelledBeforeDispatch"] = json!(true);
    assert_eq!(
        save_request(
            &state,
            &cancelled.to_string(),
            |_| panic!("cancel reached scope"),
            |_, _| panic!("cancel effect")
        )
        .unwrap_err()
        .code,
        "cancelled_before_dispatch"
    );
    for scope in [0, 1] {
        assert!(save_request(
            &state,
            &valid.to_string(),
            |p| scope == 1 && p == scratch.0.join("new.json"),
            |_, _| panic!("denied effect")
        )
        .is_err());
    }
    let absent = request(&pins, 0, &scratch.0.join("absent/new.json"));
    assert!(save_request(
        &state,
        &absent.to_string(),
        |_| true,
        |_, _| panic!("parent effect")
    )
    .is_err());
    advance(&state, &pins, 0, "fill.set");
    assert!(save_request(
        &state,
        &valid.to_string(),
        |_| true,
        |_, _| panic!("stale effect")
    )
    .is_err());
    assert_eq!(scratch.files(), 0);
    advance(&state, &pins, 1, "undo");
    assert_eq!(
        serialized(&state, &pins, 2)["result"]["documentJson"],
        ORIGINAL
    );
}

#[test]
fn absent_nonobject_released_and_oversized_serialization_have_zero_disk_effect() {
    let scratch = Scratch::new();
    let state = ApplicationMcp::default();
    let fake = json!({"instanceId":state.instance_id(),"documentId":"document-fixture"});
    let attempt = |state: &ApplicationMcp, pins: &Value| {
        assert!(save_request(
            state,
            &request(pins, 0, &scratch.0.join("new.json")).to_string(),
            |_| true,
            |_, _| panic!("unavailable effect")
        )
        .is_err())
    };
    attempt(&state, &fake);
    let reservation = state.reserve_native_install().unwrap();
    attempt(&state, &fake);
    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    state
        .install_dispatch(
            reservation.generation(),
            Box::new(NonObjectFixture {
                instance: state.instance_id().to_owned(),
                pump: TerminalPump(Arc::clone(&calls)),
            }),
        )
        .unwrap();
    attempt(&state, &fake);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let active = ApplicationMcp::default();
    let pins = install(&active, ORIGINAL);
    release(&active, &pins, 0);
    attempt(&active, &pins);
    let mut large: Value = serde_json::from_str(ORIGINAL).unwrap();
    let segment = large["objects"][0]["geometry"]["segments"][0].clone();
    large["objects"][0]["geometry"]["segments"] = json!(vec![segment; 256]);
    let state = ApplicationMcp::default();
    let pins = install(&state, &large.to_string());
    attempt(&state, &pins);
    assert_eq!(scratch.files(), 0);
}

#[test]
fn all_existing_targets_and_racing_destination_remain_untouched() {
    let scratch = Scratch::new();
    let state = ApplicationMcp::default();
    let pins = install(&state, ORIGINAL);
    let file = scratch.0.join("existing");
    fs::write(&file, b"foreign").unwrap();
    let empty = scratch.0.join("empty");
    fs::write(&empty, b"").unwrap();
    let dir = scratch.0.join("dir");
    fs::create_dir(&dir).unwrap();
    let mut targets = vec![file.clone(), empty, dir];
    #[cfg(unix)]
    {
        for (name, to) in [
            ("link", file.clone()),
            ("dangling", scratch.0.join("absent")),
        ] {
            let path = scratch.0.join(name);
            std::os::unix::fs::symlink(&to, &path).unwrap();
            targets.push(path);
        }
    }
    let count = scratch.files();
    for path in targets {
        assert!(save_request(
            &state,
            &request(&pins, 0, &path).to_string(),
            |_| true,
            |_, _| Ok(())
        )
        .is_err());
        assert_eq!(scratch.files(), count);
        assert_eq!(fs::read(&file).unwrap(), b"foreign");
        assert!(fs::metadata(scratch.0.join("dir")).unwrap().is_dir());
    }
    #[cfg(unix)]
    {
        assert_eq!(
            fs::read_link(scratch.0.join("dangling")).unwrap(),
            scratch.0.join("absent")
        );
    }
    let destination = scratch.0.join("racing");
    assert!(save_request(
        &state,
        &request(&pins, 0, &destination).to_string(),
        |_| true,
        |phase, _| {
            if phase == "link" {
                fs::write(&destination, b"racing foreign")?;
            }
            Ok(())
        }
    )
    .is_err());
    assert_eq!(fs::read(&destination).unwrap(), b"racing foreign");
    assert_eq!(scratch.files(), count + 1);
}

#[test]
fn real_staging_write_sync_link_failures_and_post_commit_cleanup_are_distinct() {
    for phase in ["create", "write", "sync", "link", "cleanup"] {
        let scratch = Scratch::new();
        let state = ApplicationMcp::default();
        let pins = install(&state, ORIGINAL);
        let destination = scratch.0.join("new.json");
        let result = save_request(
            &state,
            &request(&pins, 0, &destination).to_string(),
            |_| true,
            |current, stage| {
                if phase == current {
                    if phase == "write" {
                        fs::write(stage, b"partial")?;
                    }
                    return Err(fail());
                }
                Ok(())
            },
        );
        if phase == "cleanup" {
            let receipt = serde_json::to_value(result.unwrap()).unwrap();
            assert_eq!(receipt["published"], true);
            assert_eq!(receipt["cleanupPending"], true);
            assert_eq!(fs::read(&destination).unwrap(), ORIGINAL.as_bytes());
            assert_eq!(scratch.files(), 2);
        } else {
            assert!(result.is_err());
            assert!(!destination.exists());
            assert_eq!(scratch.files(), 0);
        }
        assert_eq!(
            serialized(&state, &pins, 0)["result"]["documentJson"],
            ORIGINAL
        );
    }
}

#[test]
fn collision_and_detected_staging_substitution_preserve_foreign_files() {
    for phase in ["create", "link"] {
        let scratch = Scratch::new();
        let state = ApplicationMcp::default();
        let pins = install(&state, ORIGINAL);
        let destination = scratch.0.join("new.json");
        let mut foreign = None;
        let result = save_request(
            &state,
            &request(&pins, 0, &destination).to_string(),
            |_| true,
            |current, path| {
                if phase == current {
                    if phase == "link" {
                        fs::remove_file(path)?;
                    }
                    fs::write(path, b"foreign replacement")?;
                    foreign = Some(path.to_owned());
                }
                Ok(())
            },
        );
        assert!(result.is_err());
        assert!(!destination.exists());
        assert_eq!(fs::read(foreign.unwrap()).unwrap(), b"foreign replacement");
        assert_eq!(scratch.files(), 1);
    }
}

#[test]
fn immutable_saved_snapshot_survives_actual_advance_and_release_after_acquisition() {
    for should_release in [false, true] {
        let scratch = Scratch::new();
        let state = ApplicationMcp::default();
        let pins = install(&state, ORIGINAL);
        let destination = scratch.0.join("new.json");
        let result = save_request(
            &state,
            &request(&pins, 0, &destination).to_string(),
            |_| true,
            |phase, _| {
                if phase == "snapshot" {
                    advance(&state, &pins, 0, "fill.set");
                    if should_release {
                        release(&state, &pins, 1);
                    }
                }
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(serde_json::to_value(result).unwrap()["atRevision"], 0);
        assert_eq!(fs::read(destination).unwrap(), ORIGINAL.as_bytes());
        let current = serialized(&state, &pins, 1);
        if should_release {
            assert_eq!(current["ok"], false);
        } else {
            assert_eq!(current["result"]["documentJson"], EDITED);
        }
    }
}
