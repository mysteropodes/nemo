//! Quarantine order, set-aside retention and first-generation adoption (#2360).
use crate::native_object_bootstrap::recovery::{
    list::quarantine_with, list_slots, quarantine, Limits,
};
use crate::native_object_bootstrap::recovery_tests::*;
use std::{fs, io, path::Path};

const AGE: u64 = 10_000;

fn limits(bytes: u64) -> Limits {
    Limits {
        slots: 5,
        bytes,
        age_ms: AGE,
    }
}

fn tree_bytes(path: &Path) -> u64 {
    let meta = fs::symlink_metadata(path).unwrap();
    if meta.is_file() {
        return meta.len();
    }
    fs::read_dir(path)
        .unwrap()
        .map(|e| tree_bytes(&e.unwrap().path()))
        .sum()
}

fn root_names(root: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(root)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    names
}

fn failing_move(_: &Path, _: &Path) -> io::Result<()> {
    Err(io::Error::from_raw_os_error(28))
}

#[test]
fn a_failed_quarantine_move_leaves_manifest_and_file_untouched() {
    let scratch = Scratch::new();
    let root = scratch.root();
    publish_ok(&root, "slot-a", 1, &project_with("1"), 1_000);
    publish_ok(&root, "slot-a", 2, &project_with("2"), 2_000);
    let manifest_path = root.join("slot-a").join("manifest.json");
    let before = fs::read(&manifest_path).unwrap();
    let m = manifest(&root, "slot-a");
    let files_before = slot_files(&root, "slot-a");
    let moved = quarantine_with(
        &root,
        "slot-a",
        &m.generations[1].file,
        9_000,
        &mut failing_move,
    );
    assert!(moved.is_err());
    assert_eq!(fs::read(&manifest_path).unwrap(), before);
    assert_eq!(slot_files(&root, "slot-a"), files_before);
    // A later publish must not find an unreferenced file to prune: both
    // generations the manifest named are still the ones on disk.
    let listing = list_slots(&root, 9_001, None, &Limits::DEFAULT).unwrap();
    assert_eq!(listing.entries[0].generations.len(), 2);
    // The last generation / whole-slot path fails the same way, slot intact.
    publish_ok(&root, "slot-b", 1, &project_with("b"), 3_000);
    let b = manifest(&root, "slot-b");
    assert!(quarantine_with(
        &root,
        "slot-b",
        &b.generations[0].file,
        9_000,
        &mut failing_move
    )
    .is_err());
    assert!(root.join("slot-b").join(&b.generations[0].file).is_file());
    assert_eq!(manifest(&root, "slot-b"), b);
    // And the real mover still works afterwards.
    assert_eq!(
        quarantine(&root, "slot-a", &m.generations[1].file, 9_002).unwrap(),
        (true, false)
    );
    assert!(slot_files(&root, "slot-a")
        .iter()
        .any(|f| f.contains(".corrupt-9002")));
}

#[test]
fn quarantine_moves_the_bytes_before_rewriting_the_manifest() {
    let scratch = Scratch::new();
    let root = scratch.root();
    publish_ok(&root, "slot-a", 1, &project_with("1"), 1_000);
    publish_ok(&root, "slot-a", 2, &project_with("2"), 2_000);
    let m = manifest(&root, "slot-a");
    let manifest_path = root.join("slot-a").join("manifest.json");
    let before = fs::read(&manifest_path).unwrap();
    let mut seen_manifest_at_move = None;
    let mut mover = |from: &Path, to: &Path| {
        seen_manifest_at_move = Some(fs::read(&manifest_path).unwrap());
        fs::rename(from, to)
    };
    quarantine_with(&root, "slot-a", &m.generations[1].file, 9_000, &mut mover).unwrap();
    assert_eq!(
        seen_manifest_at_move.unwrap(),
        before,
        "manifest untouched at move time"
    );
    assert_eq!(manifest(&root, "slot-a").generations.len(), 1);
}

#[test]
fn set_aside_names_are_evicted_by_age() {
    let scratch = Scratch::new();
    let root = scratch.root();
    publish_ok(&root, "slot-a", 1, &project_with("1"), 10_000);
    publish_ok(&root, "slot-a", 2, &project_with("2"), 11_000);
    let m = manifest(&root, "slot-a");
    // An old quarantined generation inside a live slot, an old and a young
    // quarantined slot at the root.
    quarantine(&root, "slot-a", &m.generations[1].file, 3_000).unwrap();
    publish_ok(&root, "slot-b", 1, &project_with("b"), 1_000);
    quarantine(
        &root,
        "slot-b",
        &manifest(&root, "slot-b").generations[0].file,
        3_500,
    )
    .unwrap();
    publish_ok(&root, "slot-c", 1, &project_with("c"), 20_000);
    quarantine(
        &root,
        "slot-c",
        &manifest(&root, "slot-c").generations[0].file,
        20_000,
    )
    .unwrap();
    assert!(slot_files(&root, "slot-a")
        .iter()
        .any(|f| f.contains(".corrupt-3000")));
    let kept = limits(u64::MAX);
    list_slots(&root, 12_000, None, &kept).unwrap();
    assert!(root_names(&root).iter().any(|n| n == "slot-b.corrupt-3500"));
    list_slots(&root, 20_500, None, &kept).unwrap();
    let names = root_names(&root);
    assert!(
        !names.iter().any(|n| n.starts_with("slot-b.corrupt-")),
        "{names:?}"
    );
    assert!(
        names.iter().any(|n| n == "slot-c.corrupt-20000"),
        "{names:?}"
    );
    assert!(
        !slot_files(&root, "slot-a")
            .iter()
            .any(|f| f.contains(".corrupt-")),
        "aged in-slot set-aside is gone"
    );
    assert_eq!(
        manifest(&root, "slot-a").generations.len(),
        1,
        "live data untouched"
    );
}

#[test]
fn set_aside_bytes_count_in_the_budget_and_go_before_live_slots() {
    let scratch = Scratch::new();
    let root = scratch.root();
    publish_ok(&root, "slot-a", 1, &project_with("live"), 9_000);
    for (slot, at) in [("slot-b", 1_000u64), ("slot-c", 2_000)] {
        publish_ok(&root, slot, 1, &project_with(slot), at);
        let file = manifest(&root, slot).generations[0].file.clone();
        quarantine(&root, slot, &file, at + 7_000).unwrap();
    }
    let live = manifest(&root, "slot-a").generations[0].byte_count;
    let newest_aside = tree_bytes(&root.join("slot-c.corrupt-9000"));
    // Room for the live slot plus one set-aside: the oldest set-aside goes.
    list_slots(&root, 9_500, None, &limits(live + newest_aside)).unwrap();
    let names = root_names(&root);
    assert!(
        !names.iter().any(|n| n.starts_with("slot-b.corrupt-")),
        "{names:?}"
    );
    assert!(
        names.iter().any(|n| n.starts_with("slot-c.corrupt-")),
        "{names:?}"
    );
    // Room for the live slot only: every set-aside goes, the live slot stays.
    let listing = list_slots(&root, 9_501, None, &limits(live)).unwrap();
    let names = root_names(&root);
    assert!(!names.iter().any(|n| n.contains(".corrupt-")), "{names:?}");
    assert_eq!(listing.entries.len(), 1);
    assert_eq!(listing.entries[0].slot_key, "slot-a");
}

#[test]
fn an_orphan_first_generation_is_adopted_not_quarantined() {
    let scratch = Scratch::new();
    let root = scratch.root();
    // Killed between the data rename and the manifest rename of a first write.
    fs::create_dir_all(root.join("slot-orphan")).unwrap();
    let good = root.join("slot-orphan").join("rec-5000-3-0a1b2c3d.json");
    fs::write(&good, project_with("first")).unwrap();
    let listing = list_slots(&root, 6_000, None, &Limits::DEFAULT).unwrap();
    assert!(listing.quarantined.is_empty(), "{:?}", listing.quarantined);
    assert_eq!(listing.entries.len(), 1);
    let entry = &listing.entries[0];
    assert_eq!(entry.slot_key, "slot-orphan");
    assert_eq!(entry.generations.len(), 1);
    assert_eq!(entry.generations[0].content_revision, 3);
    assert_eq!(entry.generations[0].saved_at_ms, 5_000);
    assert_eq!(entry.generations[0].path, good.to_string_lossy());
    assert_eq!(fs::read(&good).unwrap(), project_with("first"));
    // The manifest now exists, so the next list and a later publish keep it.
    assert_eq!(manifest(&root, "slot-orphan").generations.len(), 1);
    let again = list_slots(&root, 6_001, None, &Limits::DEFAULT).unwrap();
    assert_eq!(again.entries, listing.entries);
    assert!(!root_names(&root).iter().any(|n| n.contains(".corrupt-")));
}

#[test]
fn orphan_adoption_keeps_the_newest_parsing_files_and_sets_the_rest_aside() {
    let scratch = Scratch::new();
    let root = scratch.root();
    let slot = root.join("slot-orphan");
    fs::create_dir_all(&slot).unwrap();
    fs::write(slot.join("rec-1000-1-00000001.json"), project_with("a")).unwrap();
    fs::write(slot.join("rec-2000-2-00000002.json"), project_with("b")).unwrap();
    fs::write(slot.join("rec-3000-3-00000003.json"), project_with("c")).unwrap();
    fs::write(
        slot.join("rec-4000-4-00000004.json"),
        b"{\"format\":\"other\"}",
    )
    .unwrap();
    let listing = list_slots(&root, 5_000, None, &Limits::DEFAULT).unwrap();
    assert!(listing.quarantined.is_empty());
    let revisions: Vec<u64> = listing.entries[0]
        .generations
        .iter()
        .map(|g| g.content_revision)
        .collect();
    assert_eq!(revisions, [3, 2]);
    let files = slot_files(&root, "slot-orphan");
    assert!(
        files
            .iter()
            .any(|f| f.starts_with("rec-1000-1-00000001.json.corrupt-")),
        "{files:?}"
    );
    assert!(
        files
            .iter()
            .any(|f| f.starts_with("rec-4000-4-00000004.json.corrupt-")),
        "{files:?}"
    );
}
