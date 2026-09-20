fn main() {
    // `tauri::generate_context!` requires an icon at compile time even when the
    // standalone proof has bundling disabled. Generate a local transparent 1x1
    // PNG so this crate neither reads nor alters the production Tauri tree.
    let icon = std::path::Path::new("icons/n04.png");
    if !icon.exists() {
        std::fs::create_dir_all("icons").expect("create isolated icon directory");
        std::fs::write(
            icon,
            [
                137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0,
                1, 8, 6, 0, 0, 0, 31, 21, 196, 137, 0, 0, 0, 13, 73, 68, 65, 84, 8, 215, 99, 248,
                255, 255, 255, 127, 0, 9, 251, 3, 253, 42, 134, 228, 140, 0, 0, 0, 0, 73, 69, 78,
                68, 174, 66, 96, 130,
            ],
        )
        .expect("write isolated icon");
    }
    println!("cargo:rerun-if-changed=build.rs");
    tauri_build::build()
}
