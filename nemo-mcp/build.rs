use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn main() {
    let sha = git(&["rev-parse", "HEAD"]).unwrap_or_else(|| "source-unknown".into());
    let dirty = git(&["status", "--porcelain", "--untracked-files=no"])
        .map(|status| !status.is_empty())
        .unwrap_or(true);
    println!(
        "cargo:rustc-env=NEMO_MCP_SOURCE_ID={sha}{}",
        if dirty { "-dirty" } else { "" }
    );
    for path in [
        "src",
        "Cargo.toml",
        "Cargo.lock",
        "../src",
        "../src-tauri/src",
    ] {
        println!("cargo:rerun-if-changed={path}");
    }
    if let Some(path) = git(&["rev-parse", "--git-path", "HEAD"]) {
        println!("cargo:rerun-if-changed={path}");
    }
    if let Some(reference) = git(&["symbolic-ref", "-q", "HEAD"]) {
        if let Some(path) = git(&["rev-parse", "--git-path", &reference]) {
            println!("cargo:rerun-if-changed={path}");
        }
    }
}
