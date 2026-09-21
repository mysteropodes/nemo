use crate::native_application_ports::*;

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        let path =
            std::env::temp_dir().join(format!("nemo-host-ports-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&path).unwrap();
        Self(path.canonicalize().unwrap())
    }

    fn port(&self) -> DesktopArtifactPort {
        DesktopArtifactPort::new(
            self.0.join("staging"),
            [
                ("opaque:output".into(), self.0.join("output")),
                ("opaque:other".into(), self.0.join("other")),
            ],
        )
        .unwrap()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        // The exclusively created fixture tree contains no user files.
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn resource(id: &str, version: &str) -> GeometryPaintInput {
    GeometryPaintInput::new(id, version, vec![]).unwrap()
}

fn handle(id: &str, version: &str) -> OpaqueResourceHandle {
    serde_json::from_value(serde_json::json!({"resourceId": id, "resourceVersion": version}))
        .unwrap()
}

fn names() -> Vec<String> {
    vec!["frame_0000.png".into()]
}

#[test]
fn resources_resolve_only_exact_admitted_identity_and_clone_immutably() {
    let mut resolver =
        DesktopResourceResolver::new(vec![resource("shape", "v1"), resource("shape", "v2")])
            .unwrap();
    assert_eq!(
        resolver.resolve_geometry(&handle("shape", "v1")).unwrap(),
        resource("shape", "v1")
    );
    assert_eq!(
        resolver.clone().resolve_identity("shape", "v2").unwrap(),
        resource("shape", "v2")
    );
    assert!(resolver.resolve_geometry(&handle("shape", "v3")).is_err());
    assert!(resolver.resolve_geometry(&handle("other", "v1")).is_err());
    assert!(
        DesktopResourceResolver::new(vec![resource("shape", "v1"), resource("shape", "v1")])
            .is_err()
    );
}

#[test]
fn shared_compositor_clones_retain_one_handle() {
    let shared = SharedCompositor::new(Compositor::new().expect("native GPU context"));
    assert!(Arc::ptr_eq(&shared.inner(), &shared.clone().inner()));
}

#[test]
fn bindings_reject_aliases_and_invalid_handles_without_changing_existing_binding() {
    let scratch = Scratch::new();
    let mut port = scratch.port();
    let bindings = port.bindings();
    assert!(bindings
        .bind("opaque:output".into(), scratch.0.join("changed"))
        .is_err());
    assert_eq!(
        bindings.0.lock().unwrap()["opaque:output"],
        scratch.0.join("output")
    );
    assert!(bindings
        .bind("different".into(), scratch.0.join("output"))
        .is_err());
    assert!(bindings
        .bind("../path".into(), scratch.0.join("new"))
        .is_err());
    assert!(bindings
        .bind("relative".into(), PathBuf::from("relative/output"))
        .is_err());
    assert!(bindings.bind("root".into(), PathBuf::from("/")).is_err());
    bindings
        .clone()
        .bind("later".into(), scratch.0.join("later"))
        .unwrap();
    port.begin_staging("late-job", "later").unwrap();
    port.cleanup("late-job").unwrap();
}

#[test]
fn begin_rejects_unknown_targets_duplicate_jobs_and_existing_destinations() {
    let scratch = Scratch::new();
    let mut port = scratch.port();
    assert!(port.begin_staging("job", "unknown").is_err());
    port.begin_staging("job", "opaque:output").unwrap();
    let path = port.stages["job"].path.clone();
    assert!(port.begin_staging("job", "opaque:other").is_err());
    assert_eq!(port.stages["job"].path, path);
    fs::create_dir(scratch.0.join("other")).unwrap();
    assert!(port.begin_staging("job2", "opaque:other").is_err());
    port.cleanup("job").unwrap();
}

#[test]
fn stages_are_private_unique_and_job_ids_never_form_paths() {
    let scratch = Scratch::new();
    let mut first = scratch.port();
    let mut second = scratch.port();
    first
        .begin_staging("../../outside", "opaque:output")
        .unwrap();
    second
        .begin_staging("../../outside", "opaque:output")
        .unwrap();
    let a = &first.stages["../../outside"].path;
    let b = &second.stages["../../outside"].path;
    assert_ne!(a, b);
    assert_eq!(a.parent().unwrap(), first.root);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(fs::metadata(a).unwrap().permissions().mode() & 0o777, 0o700);
    }
    first.cleanup("../../outside").unwrap();
    assert!(b.exists());
    second.cleanup("../../outside").unwrap();
}

#[test]
fn frame_names_are_exact_and_writes_never_overwrite() {
    let scratch = Scratch::new();
    let mut port = scratch.port();
    port.begin_staging("job", "opaque:output").unwrap();
    for name in [
        "../escape.png",
        "/absolute.png",
        "frame_0.png",
        "frame_00000.png",
        "frame_00a0.png",
        "frame_0000.png/x",
        "frame_0000.png\0",
    ] {
        assert!(port.write_frame("job", name, b"invalid").is_err(), "{name}");
    }
    port.write_frame("job", "frame_0000.png", b"original")
        .unwrap();
    assert!(port
        .write_frame("job", "frame_0000.png", b"overwrite")
        .is_err());
    assert_eq!(
        fs::read(port.stages["job"].path.join("frame_0000.png")).unwrap(),
        b"original"
    );
    port.cleanup("job").unwrap();
    port.cleanup("job").unwrap();
}

#[test]
fn publication_requires_exact_target_and_complete_manifest() {
    let scratch = Scratch::new();
    let mut port = scratch.port();
    port.begin_staging("job", "opaque:output").unwrap();
    port.write_frame("job", "frame_0000.png", b"png").unwrap();
    assert!(port.publish("job", "opaque:other", &names()).is_err());
    assert!(port.publish("job", "opaque:output", &[]).is_err());
    assert!(port
        .publish("job", "opaque:output", &["frame_0001.png".into()])
        .is_err());
    assert!(port
        .publish(
            "job",
            "opaque:output",
            &[names()[0].clone(), names()[0].clone()]
        )
        .is_err());
    let stage = port.stages["job"].path.clone();
    fs::write(stage.join("unexpected"), b"preserve").unwrap();
    assert!(port.publish("job", "opaque:output", &names()).is_err());
    assert!(port.cleanup("job").is_err());
    assert_eq!(fs::read(stage.join("unexpected")).unwrap(), b"preserve");
    fs::remove_file(stage.join("unexpected")).unwrap();
    port.cleanup("job").unwrap();
}

#[test]
fn output_collision_after_staging_preserves_destination_and_allows_cleanup() {
    let scratch = Scratch::new();
    let mut port = scratch.port();
    port.begin_staging("job", "opaque:output").unwrap();
    port.write_frame("job", "frame_0000.png", b"new").unwrap();
    fs::create_dir(scratch.0.join("output")).unwrap();
    let original_identity = directory_identity(&scratch.0.join("output")).unwrap();
    let error = port.publish("job", "opaque:output", &names()).unwrap_err();
    assert!(!error.contains(&scratch.0.to_string_lossy().to_string()));
    assert_eq!(
        directory_identity(&scratch.0.join("output")).unwrap(),
        original_identity
    );
    port.cleanup("job").unwrap();
    assert!(scratch.0.join("output").exists());
}

#[cfg(target_os = "macos")]
#[test]
fn successful_publication_is_atomic_and_receipt_has_only_opaque_identity() {
    let scratch = Scratch::new();
    let mut port = scratch.port();
    port.begin_staging("job", "opaque:output").unwrap();
    let stage = port.stages["job"].path.clone();
    port.write_frame("job", "frame_0000.png", b"png").unwrap();
    let receipt = port.publish("job", "opaque:output", &names()).unwrap();
    assert_eq!(receipt.target, "opaque:output");
    assert_eq!(receipt.files, names());
    assert!(!stage.exists());
    assert_eq!(
        fs::read(scratch.0.join("output/frame_0000.png")).unwrap(),
        b"png"
    );
    port.cleanup("job").unwrap();
    assert!(scratch.0.join("output/frame_0000.png").exists());
}

#[cfg(unix)]
#[test]
fn replaced_stage_symlink_and_frame_symlink_are_never_published_or_traversed() {
    use std::os::unix::fs::symlink;
    let scratch = Scratch::new();
    let mut port = scratch.port();
    port.begin_staging("job", "opaque:output").unwrap();
    port.write_frame("job", "frame_0000.png", b"owned").unwrap();
    let stage = port.stages["job"].path.clone();
    let saved = scratch.0.join("saved");
    fs::rename(&stage, &saved).unwrap();
    symlink(&saved, &stage).unwrap();
    assert!(port.cleanup("job").is_err());
    assert!(port.publish("job", "opaque:output", &names()).is_err());
    assert!(saved.join("frame_0000.png").exists());
    fs::remove_file(&stage).unwrap();
    fs::rename(&saved, &stage).unwrap();
    let external = scratch.0.join("external");
    fs::write(&external, b"preserve").unwrap();
    fs::remove_file(stage.join("frame_0000.png")).unwrap();
    symlink(&external, stage.join("frame_0000.png")).unwrap();
    assert!(port.publish("job", "opaque:output", &names()).is_err());
    port.cleanup("job").unwrap();
    assert_eq!(fs::read(external).unwrap(), b"preserve");
}

#[test]
fn unavailable_destination_parent_keeps_stage_until_explicit_cleanup() {
    let scratch = Scratch::new();
    let parent = scratch.0.join("parent");
    fs::create_dir(&parent).unwrap();
    let mut port = DesktopArtifactPort::new(
        scratch.0.join("staging"),
        [("target".into(), parent.join("output"))],
    )
    .unwrap();
    port.begin_staging("job", "target").unwrap();
    port.write_frame("job", "frame_0000.png", b"png").unwrap();
    fs::remove_dir(&parent).unwrap();
    assert!(port.publish("job", "target", &names()).is_err());
    assert!(port.stages["job"].path.join("frame_0000.png").exists());
    port.cleanup("job").unwrap();
}
