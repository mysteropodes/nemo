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

fn track_source_state(repo: &Path) -> bool {
    // This feature is for an isolated test package. Watch every tracked file,
    // not just HEAD/index: an unstaged edit can recompile Rust while leaving
    // Git metadata unchanged. The selected source directories also catch a
    // newly added include before it is staged.
    for source in [
        "src-tauri/src",
        "native-engine/src",
        "nemo-mcp/src",
        "geometry-wasm/src",
        "vectorize-core/src",
        "src",
        "tests/desktop",
    ] {
        println!("cargo:rerun-if-changed={}", repo.join(source).display());
    }
    let output = match Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["ls-files", "-z"])
        .output()
    {
        Ok(output) if output.status.success() => output,
        _ => return false,
    };
    for path in output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
    {
        let Ok(path) = std::str::from_utf8(path) else {
            return false;
        };
        if path.bytes().any(|byte| byte == b'\n' || byte == b'\r') {
            return false;
        }
        println!("cargo:rerun-if-changed={}", repo.join(path).display());
    }
    true
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
        println!("cargo:rerun-if-env-changed=NEMO_OBJECT_PROBE_BUILD_NONCE");
        track_git_path(repo, "HEAD");
        track_git_path(repo, "index");
        if let Some(reference) = git(repo, &["symbolic-ref", "-q", "HEAD"]) {
            track_git_path(repo, &reference);
        }
        let tracked = track_source_state(repo);
        let source = git(repo, &["rev-parse", "--verify", "HEAD"]).unwrap_or_default();
        let clean = tracked
            && source.len() == 40
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
