use std::{path::Path, process::Command};

fn git(repo: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8(output.stdout).ok()?;
    Some(value.trim().to_owned())
}

fn track_git_path(repo: &Path, path: &str) {
    if let Some(value) = git(repo, &["rev-parse", "--git-path", path]) {
        let value = Path::new(&value);
        let resolved = if value.is_absolute() {
            value.to_path_buf()
        } else {
            repo.join(value)
        };
        println!("cargo:rerun-if-changed={}", resolved.display());
    }
}

fn main() {
    // frontendDist ("../src") is embedded into the binary at compile time,
    // but Cargo only tracks .rs files for incremental rebuilds by default —
    // it never notices frontend-only edits (js/css/html) and silently keeps
    // serving whatever was embedded at the last real Rust rebuild. Emitting
    // rerun-if-changed for the whole frontend tree forces a fresh embed
    // (and a real recompile, not a 0.7s no-op) whenever those files change.
    println!("cargo:rerun-if-changed=../src");
    if std::env::var_os("CARGO_FEATURE_TEST_OBJECT_WINDOW_PROBE").is_some() {
        let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("repository parent");
        track_git_path(repo, "HEAD");
        track_git_path(repo, "index");
        if let Some(reference) = git(repo, &["symbolic-ref", "-q", "HEAD"]) {
            track_git_path(repo, &reference);
        }
        let source = git(repo, &["rev-parse", "--verify", "HEAD"]).unwrap_or_default();
        let clean = source.len() == 40
            && source.bytes().all(|byte| byte.is_ascii_hexdigit())
            && git(repo, &["status", "--porcelain", "--untracked-files=normal"])
                .is_some_and(|status| status.is_empty());
        println!("cargo:rustc-env=NEMO_OBJECT_PROBE_BUILD_SHA={source}");
        println!(
            "cargo:rustc-env=NEMO_OBJECT_PROBE_BUILD_CLEAN={}",
            if clean { "1" } else { "0" }
        );
    }
    tauri_build::build()
}
