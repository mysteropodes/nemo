use tauri::{Emitter, Manager};
use tauri_plugin_shell::process::CommandEvent;
use tauri_plugin_shell::ShellExt;

#[tauri::command]
async fn run_ffmpeg(app: tauri::AppHandle, window: tauri::Window, args: Vec<String>) -> Result<i32, String> {
    let sidecar = app
        .shell()
        .sidecar("ffmpeg")
        .map_err(|e| e.to_string())?
        .args(args);
    let (mut rx, _child) = sidecar.spawn().map_err(|e| e.to_string())?;

    let mut exit_code: i32 = -1;
    while let Some(event) = rx.recv().await {
        match event {
            CommandEvent::Stderr(line) => {
                let text = String::from_utf8_lossy(&line).to_string();
                let _ = window.emit("ffmpeg-progress", text);
            }
            CommandEvent::Stdout(line) => {
                let text = String::from_utf8_lossy(&line).to_string();
                let _ = window.emit("ffmpeg-progress", text);
            }
            CommandEvent::Terminated(payload) => {
                exit_code = payload.code.unwrap_or(-1);
            }
            CommandEvent::Error(err) => {
                return Err(err);
            }
            _ => {}
        }
    }
    if exit_code == 0 {
        Ok(exit_code)
    } else {
        Err(format!("ffmpeg exited with code {}", exit_code))
    }
}

// Native stylus pressure (macOS): tablet drivers (XP-Pen, Huion, Wacom…)
// deliver pen pressure through AppKit NSEvents — mouse events carrying the
// NSEventSubtypeTabletPoint subtype — which WKWebView does not forward to
// web content on all driver stacks. A local NSEvent monitor reads that
// pressure at the AppKit level (same channel Photoshop/Callipeg use) and
// streams it to the frontend as `stylus-pressure` events.
#[cfg(target_os = "macos")]
fn start_tablet_pressure_monitor(app: tauri::AppHandle) {
    use block::ConcreteBlock;
    use objc::runtime::Object;
    use objc::{class, msg_send, sel, sel_impl};
    use std::sync::atomic::{AtomicU64, Ordering};

    static LAST_EMIT_MS: AtomicU64 = AtomicU64::new(0);

    unsafe {
        // NSEventMask: LeftMouseDown (1<<1) | LeftMouseDragged (1<<6)
        let mask: u64 = (1u64 << 1) | (1u64 << 6);
        let block = ConcreteBlock::new(move |event: *mut Object| -> *mut Object {
            // NSEventSubtypeTabletPoint == 1 — only tablet-generated mouse
            // events carry meaningful pressure; plain mice report a
            // constant that would otherwise drown the speed-based fallback.
            let subtype: i16 = msg_send![event, subtype];
            if subtype == 1 {
                let pressure: f32 = msg_send![event, pressure];
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(0);
                // ~120Hz cap so high-rate tablets don't flood the IPC bridge
                if now.saturating_sub(LAST_EMIT_MS.load(Ordering::Relaxed)) >= 8 {
                    LAST_EMIT_MS.store(now, Ordering::Relaxed);
                    let _ = app.emit("stylus-pressure", pressure);
                }
            }
            event
        });
        let block = block.copy();
        let _monitor: *mut Object = msg_send![
            class!(NSEvent),
            addLocalMonitorForEventsMatchingMask: mask
            handler: &*block
        ];
        std::mem::forget(block);
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin({
            // The update channel is a private GitHub repo (strokemotion-updates),
            // served through the Contents API rather than Releases — a release
            // asset's numeric ID changes every publish, which the updater plugin
            // can't resolve (it only ever does one static GET); a repo file's
            // Contents-API URL stays the same forever, so `latest.json` and the
            // platform artifact just get overwritten by scripts/publish-update.sh
            // on each release and the same endpoint URL keeps working.
            //
            // The token is baked in at compile time via env! so it never sits in
            // source control — export STROKEMOTION_UPDATER_TOKEN before running
            // `tauri build` (see scripts/publish-update.sh's header comment for
            // how to mint a fine-grained PAT scoped to just this one repo).
            let token = env!("STROKEMOTION_UPDATER_TOKEN");
            tauri_plugin_updater::Builder::new()
                .header("Authorization", format!("Bearer {}", token))
                .expect("invalid updater Authorization header")
                .header("Accept", "application/vnd.github.raw+json")
                .expect("invalid updater Accept header")
                .build()
        })
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_http::init())
        .invoke_handler(tauri::generate_handler![run_ffmpeg])
        .setup(|app| {
            #[cfg(debug_assertions)]
            {
                let window = app.get_webview_window("main").unwrap();
                window.open_devtools();
            }
            #[cfg(target_os = "macos")]
            start_tablet_pressure_monitor(app.handle().clone());
            #[cfg(not(target_os = "macos"))]
            let _ = app;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
