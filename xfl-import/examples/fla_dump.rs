//! `cargo run --example fla_dump -- file.fla|xfl-folder` prints the model as JSON.
fn main() {
    let Some(arg) = std::env::args().nth(1) else {
        eprintln!("usage: fla_dump <file.fla | xfl folder>");
        std::process::exit(2);
    };
    let path = std::path::Path::new(&arg);
    let result = if path.is_dir() {
        nemo_xfl_import::read_xfl_dir(path)
    } else {
        match std::fs::read(path) {
            Ok(bytes) => nemo_xfl_import::read_fla(&bytes),
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        }
    };
    match result {
        Ok(doc) => println!(
            "{}",
            serde_json::to_string_pretty(&doc).expect("model serializes")
        ),
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}
