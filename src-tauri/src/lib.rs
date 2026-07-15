use tauri::{Emitter, Manager};
use tauri_plugin_shell::process::CommandEvent;
use tauri_plugin_shell::ShellExt;

// EXPERIMENTAL (experimental/native-video-decode) — see the module header.
mod video_decode;

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

// Beta-tester feedback → public GitHub repo (mysteropodes/strokemotion-feedback),
// one Issue per feedback entry. The write token is baked in at compile time
// via env! (same pattern as STROKEMOTION_UPDATER_TOKEN above — export
// STROKEMOTION_FEEDBACK_TOKEN before `tauri build`), scoped as a
// fine-grained PAT to ONLY this one repo, ONLY "Issues: write" — nothing
// else, so an extracted token can at worst spam issues in a repo that
// contains no app source. The POST happens entirely in Rust (never in JS)
// so the token never touches the webview's network inspector or any
// devtools-visible fetch() call — see feedback-bridge.js's comment for why
// this command exists instead of a plain JS fetch.
#[tauri::command]
async fn submit_feedback_issue(title: String, body: String, labels: Vec<String>) -> Result<(), String> {
    let token = env!("STROKEMOTION_FEEDBACK_TOKEN");
    let client = reqwest::Client::new();
    let payload = serde_json::json!({ "title": title, "body": body, "labels": labels });
    let resp = client
        .post("https://api.github.com/repos/mysteropodes/strokemotion-feedback/issues")
        .header("Authorization", format!("Bearer {}", token))
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", "strokemotion-app")
        .json(&payload)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if resp.status().is_success() {
        Ok(())
    } else {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        Err(format!("GitHub API error {}: {}", status, text))
    }
}

// Feedback screenshot attachments — committed via the GitHub Contents API
// into strokemotion-feedback's attachments/ folder, then linked from the
// issue body via raw.githubusercontent.com (GFM doesn't render data:
// URIs, so the image has to actually be a file in the repo to show up
// inline). Needs "Contents: Read and write" on top of submit_feedback_
// issue's "Issues" permission — a deliberate scope increase on the same
// embedded, repo-scoped token (see CLAUDE.md's feedback section for the
// trade-off): a leaked token can now write arbitrary files into this one
// code-free repo, not just spam issues, but still can't touch anything
// else. content_base64 is passed straight through — the Contents API's
// `content` field IS base64 already, no decode/re-encode needed here.
#[tauri::command]
async fn upload_feedback_attachment(filename: String, content_base64: String) -> Result<String, String> {
    let token = env!("STROKEMOTION_FEEDBACK_TOKEN");
    let client = reqwest::Client::new();
    let path = format!("attachments/{}", filename);
    let payload = serde_json::json!({
        "message": format!("Feedback attachment: {}", filename),
        "content": content_base64,
    });
    let resp = client
        .put(format!(
            "https://api.github.com/repos/mysteropodes/strokemotion-feedback/contents/{}",
            path
        ))
        .header("Authorization", format!("Bearer {}", token))
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", "strokemotion-app")
        .json(&payload)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if resp.status().is_success() {
        Ok(format!(
            "https://raw.githubusercontent.com/mysteropodes/strokemotion-feedback/main/{}",
            path
        ))
    } else {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        Err(format!("GitHub upload error {}: {}", status, text))
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
        // Lets the frontend offer an immediate "redémarrer maintenant" after
        // downloadAndInstall() succeeds (updater-bridge.js) — without this,
        // an installed update only takes effect on the NEXT manual launch,
        // which reads as "nothing happened" (reported: user only noticed the
        // update had actually worked because they happened to relaunch the
        // app later and saw the new version number).
        .plugin(tauri_plugin_process::init())
        .manage(video_decode::VideoSessions::default())
        .invoke_handler(tauri::generate_handler![
            run_ffmpeg,
            submit_feedback_issue,
            upload_feedback_attachment,
            video_decode::open_video_session,
            video_decode::decode_video_frame,
            video_decode::close_video_session,
            video_decode::autobench_config,
            video_decode::autobench_report,
            video_decode::optimized_media_target,
            video_decode::create_optimized_media
        ])
        // "Vérifier les mises à jour…" in the app (StrokeMotion) menu — same
        // check the Réglages button and the silent startup check already
        // do (updater-bridge.js), just reachable from the menu too (asked
        // for after that UI shipped: "on peut mettre la vérif d'update
        // ici aussi ?"). Tauri's Menu::default() builds the whole standard
        // macOS menu tree (App/File/Edit/View/Window/Help) — rebuilding it
        // by hand here would silently drop items on every tauri upgrade;
        // instead take the default tree and insert one item into the
        // already-built App submenu, right after About (index 0, before
        // the separator at index 1).
        .on_menu_event(|app, event| {
            if event.id() == "check_update" {
                let _ = app.emit("menu-check-update", ());
            }
        })
        .setup(|app| {
            #[cfg(debug_assertions)]
            {
                let window = app.get_webview_window("main").unwrap();
                window.open_devtools();
            }
            #[cfg(target_os = "macos")]
            {
                use tauri::menu::{Menu, MenuItem, MenuItemKind};
                let menu = Menu::default(app.handle())?;
                if let Some(MenuItemKind::Submenu(app_submenu)) = menu.items()?.first() {
                    let check_update_item = MenuItem::with_id(
                        app.handle(),
                        "check_update",
                        "Vérifier les mises à jour…",
                        true,
                        None::<&str>,
                    )?;
                    app_submenu.insert(&check_update_item, 1)?;
                }
                app.set_menu(menu)?;
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
