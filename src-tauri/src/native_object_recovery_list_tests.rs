//! Generation retention, listing validation, quarantine and symlink controls.
use crate::native_object_bootstrap::recovery::{
    discard, list_slots, publish, quarantine, Entry, Limits,
};
use crate::native_object_bootstrap::recovery_tests::*;
use std::fs;

#[test]
fn two_generations_per_slot_newest_first_and_strays_pruned() {
    let scratch = Scratch::new();
    let root = scratch.root();
    for (n, revision) in [(1u64, 1u64), (2, 5), (3, 9)] {
        publish_ok(
            &root,
            "slot-a",
            revision,
            &project_with(&format!("g{n}")),
            n * 1_000,
        );
    }
    let m = manifest(&root, "slot-a");
    assert_eq!(m.generations.len(), 2);
    assert_eq!(
        m.generations
            .iter()
            .map(|g| g.content_revision)
            .collect::<Vec<_>>(),
        [9, 5]
    );
    let files = slot_files(&root, "slot-a");
    assert_eq!(files.len(), 3, "{files:?}"); // two generations + manifest
    assert!(files.iter().all(|f| !f.ends_with(".tmp")));
    assert_eq!(
        fs::read(root.join("slot-a").join(&m.generations[0].file)).unwrap(),
        project_with("g3")
    );
    assert_eq!(
        fs::read(root.join("slot-a").join(&m.generations[1].file)).unwrap(),
        project_with("g2")
    );
    // A same-millisecond, same-revision write never clobbers: names are unique.
    publish_ok(&root, "slot-a", 9, &project_with("g4"), 3_000);
    let m = manifest(&root, "slot-a");
    assert_ne!(m.generations[0].file, m.generations[1].file);
    assert_eq!(slot_files(&root, "slot-a").len(), 3);
}

#[test]
fn list_is_newest_first_and_enforces_slot_age_and_byte_retention() {
    let scratch = Scratch::new();
    let root = scratch.root();
    for (n, slot) in [
        "slot-a", "slot-b", "slot-c", "slot-d", "slot-e", "slot-f", "slot-g",
    ]
    .iter()
    .enumerate()
    {
        publish_ok(
            &root,
            slot,
            1,
            &project_with(slot),
            10_000 + n as u64 * 1_000,
        );
    }
    let listing = list_slots(&root, 20_000, None, &Limits::DEFAULT).unwrap();
    let keys: Vec<_> = listing
        .entries
        .iter()
        .map(|e| e.slot_key.as_str())
        .collect();
    assert_eq!(keys, ["slot-g", "slot-f", "slot-e", "slot-d", "slot-c"]);
    assert!(!root.join("slot-a").exists() && !root.join("slot-b").exists());
    // Age: everything older than 14 days from `now` is removed.
    let listing = list_slots(&root, 16_000 + 14 * DAY + 1, None, &Limits::DEFAULT).unwrap();
    assert!(listing.entries.is_empty(), "{listing:?}");
    assert!(fs::read_dir(&root).unwrap().next().is_none());
    // Bytes: shed oldest first; the protected slot is never shed.
    let small = Limits {
        slots: 5,
        bytes: 2 * project_with("x").len() as u64 + 8,
        age_ms: 14 * DAY,
    };
    for (n, slot) in ["slot-a", "slot-b", "slot-c"].iter().enumerate() {
        publish_ok(&root, slot, 1, &project_with("x"), 1_000 + n as u64);
    }
    let listing = list_slots(&root, 2_000, Some("slot-a"), &small).unwrap();
    let keys: Vec<_> = listing
        .entries
        .iter()
        .map(|e| e.slot_key.as_str())
        .collect();
    assert_eq!(keys, ["slot-c", "slot-a"]);
    assert!(!root.join("slot-b").exists());
}

#[test]
fn list_validates_cheaply_and_quarantines_instead_of_deleting() {
    let scratch = Scratch::new();
    let root = scratch.root();
    publish_ok(&root, "slot-ok", 1, &project_with("ok"), 1_000);
    // Corrupt manifest.
    publish_ok(&root, "slot-bad-manifest", 1, &project_with("bm"), 2_000);
    fs::write(
        root.join("slot-bad-manifest").join("manifest.json"),
        b"{not json",
    )
    .unwrap();
    // Data file truncated after the manifest recorded its size.
    publish_ok(&root, "slot-truncated", 1, &project_with("tr"), 3_000);
    let m = manifest(&root, "slot-truncated");
    fs::write(
        root.join("slot-truncated").join(&m.generations[0].file),
        b"short",
    )
    .unwrap();
    // One good generation and one that is not a native project file.
    publish_ok(&root, "slot-half", 1, &project_with("old"), 4_000);
    publish_ok(&root, "slot-half", 2, &project_with("new"), 5_000);
    let m = manifest(&root, "slot-half");
    let wrong = b"{\"format\":\"something-else\",\"padding\":\"xxxxxxxxxxxxxxxxxxxxxxx\"}";
    let mut edited = m.clone();
    edited.generations[1].byte_count = wrong.len() as u64;
    fs::write(root.join("slot-half").join(&m.generations[1].file), wrong).unwrap();
    fs::write(
        root.join("slot-half").join("manifest.json"),
        serde_json::to_vec(&edited).unwrap(),
    )
    .unwrap();
    // Data file without any manifest (killed during a first write).
    fs::create_dir(root.join("slot-orphan")).unwrap();
    fs::write(
        root.join("slot-orphan").join("rec-1-1-0a1b2c3d.json"),
        project_with("o"),
    )
    .unwrap();
    // Only an old temp: nothing worth keeping.
    fs::create_dir(root.join("slot-empty")).unwrap();
    let tmp = root.join("slot-empty").join(".data-x.tmp");
    fs::write(&tmp, b"partial").unwrap();
    let listing = list_slots(&root, 6_000, None, &Limits::DEFAULT).unwrap();
    let keys: Vec<_> = listing
        .entries
        .iter()
        .map(|e| e.slot_key.as_str())
        .collect();
    assert_eq!(keys, ["slot-half", "slot-ok"]);
    assert_eq!(listing.entries[0].generations.len(), 1);
    assert_eq!(listing.entries[0].generations[0].content_revision, 2);
    let mut quarantined = listing.quarantined.clone();
    quarantined.sort();
    assert_eq!(
        quarantined,
        ["slot-bad-manifest", "slot-orphan", "slot-truncated"]
    );
    let names: Vec<String> = fs::read_dir(&root)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    for slot in &quarantined {
        assert!(!root.join(slot).exists());
        assert!(
            names
                .iter()
                .any(|n| n.starts_with(&format!("{slot}.corrupt-"))),
            "{names:?}"
        );
    }
    // The bytes of a corrupt slot survive under the quarantine name.
    let kept = names
        .iter()
        .find(|n| n.starts_with("slot-bad-manifest.corrupt-"))
        .unwrap();
    assert!(root.join(kept).join("manifest.json").exists());
    // The invalid generation was set aside, not deleted.
    assert!(slot_files(&root, "slot-half")
        .iter()
        .any(|f| f.contains(".corrupt-")));
    assert!(!root.join("slot-empty").exists());
    // A second list is stable and never returns quarantined names.
    let again = list_slots(&root, 6_001, None, &Limits::DEFAULT).unwrap();
    assert_eq!(again.entries, listing.entries);
    assert!(again.quarantined.is_empty());
}

#[test]
fn a_slot_saved_after_its_recovery_is_dropped_by_the_origin_rule() {
    let scratch = Scratch::new();
    let root = scratch.root();
    let origin = scratch.0.join("saved.nemo");
    fs::write(&origin, b"saved later").unwrap();
    let mtime = fs::metadata(&origin)
        .unwrap()
        .modified()
        .unwrap()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    let origin = origin.to_str().unwrap().to_owned();
    fn with_origin<'a>(slot: &'a str, origin: &'a str) -> Entry<'a> {
        Entry {
            origin_path: Some(origin),
            ..entry(slot, 1)
        }
    }
    publish(
        &root,
        &with_origin("slot-saved", &origin),
        &project_with("a"),
        mtime - 5_000,
        &mut |_, _| Ok(()),
    )
    .unwrap();
    publish(
        &root,
        &with_origin("slot-newer", &origin),
        &project_with("b"),
        mtime + 5_000,
        &mut |_, _| Ok(()),
    )
    .unwrap();
    let listing = list_slots(&root, mtime + 6_000, None, &Limits::DEFAULT).unwrap();
    let keys: Vec<_> = listing
        .entries
        .iter()
        .map(|e| e.slot_key.as_str())
        .collect();
    assert_eq!(keys, ["slot-newer"]);
    assert!(!root.join("slot-saved").exists());
}

#[test]
fn explicit_discard_and_quarantine_keep_their_scopes() {
    let scratch = Scratch::new();
    let root = scratch.root();
    publish_ok(&root, "slot-a", 1, &project_with("1"), 1_000);
    publish_ok(&root, "slot-a", 2, &project_with("2"), 2_000);
    publish_ok(&root, "slot-b", 1, &project_with("b"), 3_000);
    let m = manifest(&root, "slot-a");
    // One of two generations: only that file leaves service.
    assert_eq!(
        quarantine(&root, "slot-a", &m.generations[0].file, 9_000).unwrap(),
        (true, false)
    );
    let after = manifest(&root, "slot-a");
    assert_eq!(after.generations.len(), 1);
    assert_eq!(after.generations[0].file, m.generations[1].file);
    assert!(slot_files(&root, "slot-a")
        .iter()
        .any(|f| f.contains(".corrupt-9000")));
    // The last generation takes the slot with it, still undeleted.
    assert_eq!(
        quarantine(&root, "slot-a", &after.generations[0].file, 9_001).unwrap(),
        (true, true)
    );
    assert!(!root.join("slot-a").exists());
    assert!(root.join("slot-a.corrupt-9001").is_dir());
    assert_eq!(
        quarantine(&root, "slot-a", &after.generations[0].file, 9_002).unwrap(),
        (false, false)
    );
    // Unknown generation of a live slot is a no-op.
    assert_eq!(
        quarantine(&root, "slot-b", "rec-1-1-ffffffff.json", 9_003).unwrap(),
        (false, false)
    );
    assert!(discard(&root, "slot-b").unwrap());
    assert!(!discard(&root, "slot-b").unwrap());
    assert!(discard(&root, "../x").is_err());
    assert!(
        root.join("slot-a.corrupt-9001").is_dir(),
        "discard never touches quarantine"
    );
}

#[test]
fn oversize_and_empty_payloads_are_refused_without_a_slot() {
    let scratch = Scratch::new();
    let root = scratch.root();
    let large = vec![b'x'; 32 * 1024 * 1024 + 1];
    assert!(publish(&root, &entry("slot-a", 1), &large, 1, &mut |_, _| Ok(())).is_err());
    assert!(publish(&root, &entry("slot-a", 1), b"", 1, &mut |_, _| Ok(())).is_err());
    assert!(!root.join("slot-a").exists());
}

#[test]
#[cfg(unix)]
fn symlinked_root_or_slot_is_never_followed() {
    use std::os::unix::fs::symlink;
    let scratch = Scratch::new();
    let elsewhere = scratch.0.join("elsewhere");
    fs::create_dir(&elsewhere).unwrap();
    // Symlinked slot directory.
    let root = scratch.root();
    fs::create_dir(&root).unwrap();
    symlink(&elsewhere, root.join("slot-a")).unwrap();
    assert!(publish(
        &root,
        &entry("slot-a", 1),
        &project_with("x"),
        1,
        &mut |_, _| Ok(())
    )
    .is_err());
    assert!(fs::read_dir(&elsewhere).unwrap().next().is_none());
    assert!(list_slots(&root, 2, None, &Limits::DEFAULT)
        .unwrap()
        .entries
        .is_empty());
    assert!(!discard(&root, "slot-a").unwrap());
    assert!(elsewhere.exists());
    // Symlinked root.
    let linked = scratch.0.join("linked-root");
    symlink(&elsewhere, &linked).unwrap();
    assert!(publish(
        &linked,
        &entry("slot-b", 1),
        &project_with("x"),
        1,
        &mut |_, _| Ok(())
    )
    .is_err());
    assert!(list_slots(&linked, 2, None, &Limits::DEFAULT).is_err());
    assert!(fs::read_dir(&elsewhere).unwrap().next().is_none());
}
