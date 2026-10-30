//! Atomicity controls: a fault at any publication stage leaves the previous
//! generation, its manifest and the directory clean.
use crate::native_object_bootstrap::recovery::{list_slots, publish, Limits};
use crate::native_object_bootstrap::recovery_tests::*;
use std::{fs, io};

const BEFORE_MANIFEST_RENAME: [&str; 8] = [
    "data-create",
    "data-write",
    "data-sync",
    "rename",
    "dirsync",
    "manifest-create",
    "manifest-write",
    "manifest-sync",
];

fn fault(at: &'static str) -> impl FnMut(&str, &std::path::Path) -> io::Result<()> {
    move |stage, _| {
        if stage == at {
            Err(io::Error::from_raw_os_error(28))
        } else {
            Ok(())
        }
    }
}

fn files(dir: &std::path::Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    names
}

#[test]
fn a_fault_before_the_manifest_rename_keeps_the_previous_generation_exactly() {
    for stage in BEFORE_MANIFEST_RENAME
        .into_iter()
        .chain(["manifest-rename"])
    {
        let scratch = Scratch::new();
        let root = scratch.root();
        publish_ok(&root, "slot-a", 1, &project_with("durable"), 1_000);
        let slot = root.join("slot-a");
        let manifest_before = fs::read(slot.join("manifest.json")).unwrap();
        let tree_before = files(&slot);
        let failed = publish(
            &root,
            &entry("slot-a", 2),
            &project_with("lost"),
            2_000,
            &mut fault(stage),
        );
        assert!(failed.is_err(), "{stage}");
        assert_eq!(
            fs::read(slot.join("manifest.json")).unwrap(),
            manifest_before,
            "{stage}"
        );
        assert_eq!(
            files(&slot),
            tree_before,
            "{stage}: no partial final or temp file"
        );
        let listing = list_slots(&root, 3_000, None, &Limits::DEFAULT).unwrap();
        assert_eq!(listing.entries.len(), 1, "{stage}");
        assert_eq!(listing.entries[0].generations.len(), 1, "{stage}");
        assert_eq!(
            fs::read(&listing.entries[0].generations[0].path).unwrap(),
            project_with("durable"),
            "{stage}"
        );
        // The slot is fully usable on the next tick.
        publish_ok(&root, "slot-a", 3, &project_with("retry"), 4_000);
        assert_eq!(manifest(&root, "slot-a").generations.len(), 2, "{stage}");
    }
}

#[test]
fn a_fault_on_a_first_write_leaves_no_manifest_and_no_files() {
    for stage in BEFORE_MANIFEST_RENAME
        .into_iter()
        .chain(["manifest-rename"])
    {
        let scratch = Scratch::new();
        let root = scratch.root();
        assert!(publish(
            &root,
            &entry("slot-a", 1),
            &project_with("x"),
            1,
            &mut fault(stage)
        )
        .is_err());
        assert!(files(&root.join("slot-a")).is_empty(), "{stage}");
        let listing = list_slots(&root, 2, None, &Limits::DEFAULT).unwrap();
        assert!(
            listing.entries.is_empty() && listing.quarantined.is_empty(),
            "{stage}"
        );
    }
}

#[test]
fn a_fault_after_the_manifest_rename_is_durable_or_reported_but_never_corrupt() {
    let scratch = Scratch::new();
    let root = scratch.root();
    publish_ok(&root, "slot-a", 1, &project_with("one"), 1_000);
    // The directory sync after the rename failed: reported, but the new
    // generation is referenced and readable.
    assert!(publish(
        &root,
        &entry("slot-a", 2),
        &project_with("two"),
        2_000,
        &mut fault("manifest-dirsync")
    )
    .is_err());
    let listing = list_slots(&root, 3_000, None, &Limits::DEFAULT).unwrap();
    assert_eq!(listing.entries[0].generations.len(), 2);
    assert_eq!(
        fs::read(&listing.entries[0].generations[0].path).unwrap(),
        project_with("two")
    );
    // A prune failure is not a failed write: the generation is published.
    let published = publish(
        &root,
        &entry("slot-a", 3),
        &project_with("three"),
        4_000,
        &mut fault("prune"),
    )
    .unwrap();
    assert!(published.cleanup_pending);
    assert_eq!(manifest(&root, "slot-a").generations[0].content_revision, 3);
}

#[test]
fn a_kill_after_the_data_rename_leaves_an_unreferenced_file_that_is_pruned_later() {
    let scratch = Scratch::new();
    let root = scratch.root();
    publish_ok(&root, "slot-a", 1, &project_with("one"), 1_000);
    // Simulate the process dying between rename and manifest: a stray data file.
    let stray = root.join("slot-a").join("rec-9-9-deadbeef.json");
    fs::write(&stray, project_with("stray")).unwrap();
    let listing = list_slots(&root, 2_000, None, &Limits::DEFAULT).unwrap();
    assert_eq!(
        listing.entries[0].generations.len(),
        1,
        "stray is never offered"
    );
    assert_eq!(
        fs::read(&listing.entries[0].generations[0].path).unwrap(),
        project_with("one")
    );
    publish_ok(&root, "slot-a", 2, &project_with("two"), 3_000);
    assert!(!stray.exists(), "the next write prunes it");
}

#[test]
fn a_corrupt_manifest_is_quarantined_on_write_and_the_new_generation_starts_clean() {
    let scratch = Scratch::new();
    let root = scratch.root();
    publish_ok(&root, "slot-a", 1, &project_with("one"), 1_000);
    fs::write(root.join("slot-a").join("manifest.json"), b"garbage").unwrap();
    publish_ok(&root, "slot-a", 2, &project_with("two"), 2_000);
    assert_eq!(manifest(&root, "slot-a").generations.len(), 1);
    assert!(files(&root)
        .iter()
        .any(|n| n.starts_with("slot-a.corrupt-")));
}
