use super::*;
use std::io::Write;
use std::os::unix::fs::{symlink, PermissionsExt};

struct Fixture {
    task: PathBuf,
    reports: PathBuf,
    root: Arc<Root>,
}
impl Fixture {
    fn new() -> Self {
        let task = std::env::temp_dir().join(format!("nemo-request-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&task).unwrap();
        let task = std::fs::canonicalize(task).unwrap();
        let reports = task.join("reports");
        std::fs::create_dir(&reports).unwrap();
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(reports.join("startup-js.json"))
            .unwrap();
        let root = Root::new(&reports).unwrap();
        Self {
            task,
            reports,
            root,
        }
    }
    fn path(&self) -> PathBuf {
        self.reports.join(FILE)
    }
    fn publish(&self, bytes: &[u8]) {
        let temp = self.reports.join("private-input.tmp");
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temp)
            .unwrap();
        file.write_all(bytes).unwrap();
        file.sync_all().unwrap();
        std::fs::rename(temp, self.path()).unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.task);
    }
}
#[test]
fn actual_owned_bytes_are_bounded_and_reconfirmed_without_json_or_secret_output() {
    let f = Fixture::new();
    assert!(f.root.read().unwrap().is_none());
    f.publish(&vec![b' '; 1024]);
    let mut input = f.root.read().unwrap().unwrap();
    assert_eq!(input.bytes(), vec![b' '; 1024]);
    input.reconfirm().unwrap();
    std::fs::write(f.path(), vec![b'x'; 1024]).unwrap();
    assert_eq!(input.reconfirm(), Err(ERROR));
    // Reader does not invent a parser/expected token: owner core handles bytes.
    for bytes in [Vec::new(), vec![b'x'; 1025]] {
        let f = Fixture::new();
        f.publish(&bytes);
        assert!(f.root.read().is_err());
    }
}
#[test]
fn regular_owner_and_exact_mode_include_special_bits_and_reject_links_or_devices() {
    for mode in [0o400, 0o644, 0o1600, 0o2600, 0o4600] {
        let f = Fixture::new();
        f.publish(b"{}");
        std::fs::set_permissions(f.path(), std::fs::Permissions::from_mode(mode)).unwrap();
        assert!(f.root.read().is_err(), "mode {mode:o}");
    }
    let f = Fixture::new();
    f.publish(b"{}");
    let m = std::fs::symlink_metadata(f.path()).unwrap();
    assert!(!valid(&m, m.uid().wrapping_add(1)));
    let extra = f.task.join("outside-link");
    std::fs::hard_link(f.path(), &extra).unwrap();
    assert!(f.root.read().is_err());
    let f = Fixture::new();
    symlink(f.reports.join("startup-js.json"), f.path()).unwrap();
    assert!(f.root.read().is_err());
    let f = Fixture::new();
    std::fs::create_dir(f.path()).unwrap();
    assert!(f.root.read().is_err());
    let f = Fixture::new();
    assert!(std::process::Command::new("/usr/bin/mkfifo")
        .arg(f.path())
        .status()
        .unwrap()
        .success());
    assert!(f.root.read().is_err());
}
#[test]
fn held_named_file_and_original_directory_identity_cannot_be_rebound() {
    for fault in ["file", "reports", "task", "mode", "symlink"] {
        let f = Fixture::new();
        f.publish(b"{}");
        let mut input = f.root.read().unwrap().unwrap();
        match fault {
            "file" => {
                std::fs::rename(f.path(), f.task.join("held-old")).unwrap();
                f.publish(b"{}");
            }
            "reports" => {
                std::fs::rename(&f.reports, f.task.join("old-reports")).unwrap();
                std::fs::create_dir(&f.reports).unwrap();
                f.publish(b"{}");
            }
            "task" => {
                let old = f.task.with_extension("old");
                std::fs::rename(&f.task, &old).unwrap();
                std::fs::create_dir(&f.task).unwrap();
                std::fs::create_dir(&f.reports).unwrap();
                f.publish(b"{}");
                assert!(input.reconfirm().is_err());
                std::fs::remove_dir_all(&old).unwrap();
            }
            "mode" => std::fs::set_permissions(&f.reports, std::fs::Permissions::from_mode(0o777))
                .unwrap(),
            _ => {
                std::fs::remove_file(f.path()).unwrap();
                symlink(f.task.join("absent"), f.path()).unwrap();
            }
        }
        assert!(input.reconfirm().is_err(), "{fault}");
    }
}
#[test]
fn raced_fifo_open_is_nonblocking_and_canonical_alias_does_not_rebind_pinned_root() {
    let f = Fixture::new();
    assert!(std::process::Command::new("/usr/bin/mkfifo")
        .arg(f.path())
        .status()
        .unwrap()
        .success());
    let start = std::time::Instant::now();
    let held = open(&f.path()).unwrap();
    assert!(!valid(&held.metadata().unwrap(), f.root.uid));
    assert!(start.elapsed() < std::time::Duration::from_secs(1));
    std::fs::remove_file(f.path()).unwrap();
    let alias = f.task.join("alias");
    symlink(&f.reports, &alias).unwrap();
    let root = Root::new(&alias).unwrap();
    assert_eq!(root.reports.path, f.reports);
    std::fs::remove_file(&alias).unwrap();
    symlink(f.task.join("foreign"), &alias).unwrap();
    assert!(root.read().unwrap().is_none()); // canonical ORIGINAL retained, not alias.
}
