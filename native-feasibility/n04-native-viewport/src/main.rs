#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod native_surface;

use native_surface::{burst, configure, evidence, shutdown, ConfigureRequest, ProofEvidence};

#[tauri::command]
fn n04_configure(request: ConfigureRequest) -> Result<ProofEvidence, String> {
    configure(request)
}

#[tauri::command]
fn n04_evidence() -> Result<ProofEvidence, String> {
    evidence()
}

#[tauri::command]
fn n04_burst(count: u32) -> Result<ProofEvidence, String> {
    burst(count)
}

#[tauri::command]
fn n04_shutdown() -> Result<ProofEvidence, String> {
    shutdown()
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            native_surface::install(app.handle())?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            n04_configure,
            n04_evidence,
            n04_burst,
            n04_shutdown
        ])
        .run(tauri::generate_context!())
        .expect("N04 Tauri application failed");
}
