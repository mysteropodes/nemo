fn main() {
    // frontendDist ("../src") is embedded into the binary at compile time,
    // but Cargo only tracks .rs files for incremental rebuilds by default —
    // it never notices frontend-only edits (js/css/html) and silently keeps
    // serving whatever was embedded at the last real Rust rebuild. Emitting
    // rerun-if-changed for the whole frontend tree forces a fresh embed
    // (and a real recompile, not a 0.7s no-op) whenever those files change.
    println!("cargo:rerun-if-changed=../src");
    tauri_build::build()
}
