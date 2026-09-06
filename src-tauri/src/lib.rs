use tauri::{Emitter, Manager};
use tauri_plugin_shell::process::CommandEvent;
use tauri_plugin_shell::ShellExt;

// EXPERIMENTAL (experimental/native-video-decode) — see the module header.
mod video_decode;
mod vectorize;
mod application_mcp;

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

// Beta-tester feedback (issues + screenshot attachments) used to POST
// straight to GitHub from here, with a write-scoped token baked in via
// env!() at compile time. That token had to be embedded in every
// distributed binary, and — since this bypassed the nemo-feedback Worker
// entirely — it also skipped the Worker's spam filter (worker-feedback/
// src/index.js's looksLikeSpam), the same protection the web build already
// gets. Removed 2026-09: feedback-bridge.js now posts through that Worker
// on desktop too, using tauri_plugin_http's fetch (window.__TAURI__.http.
// fetch) instead of `window.fetch` — a plain fetch() would be blocked by
// both the CSP's connect-src (which doesn't list the Worker) and the
// Worker's own CORS allowlist (which only lists the web origins, not
// tauri://localhost); routing the request through Rust via the plugin
// sidesteps both without having to widen either guard. See
// worker-feedback/README.md for the Worker side.

// Live Google Fonts (2026-08-28, feedback: "peux t'on avoir d'autre typo
// sans probleme de droit genre les google fonts ?" then "un vrai catalogue
// Google Fonts en ligne") — fetches ANY of the 1800+ OFL-licensed Google
// Fonts on demand, not just the handful bundled as local TTFs
// (vector-text-bridge.js). Graphite's own web editor does the same idea
// (editor.graphite.art) via their own backend proxying Google's official
// Fonts Developer API — that API needs a server-side secret key, so it
// can't be called straight from a browser without exposing it. This avoids
// needing any key at all: Google Fonts' public CSS2 endpoint
// (fonts.googleapis.com/css2) serves a DIFFERENT font file format
// depending on the request's User-Agent (browsers get woff2, IE9-era UAs
// get woff/eot) — an old Android 2.2 UA specifically gets a plain,
// uncompressed .ttf URL, exactly what opentype.js (the same glyph-outline
// parser vector-text-bridge.js already uses for the bundled fonts) needs,
// with zero decoding step. Confirmed live via curl before writing this:
// modern UA → woff2, this UA → "format('truetype')" pointing at a plain
// fonts.gstatic.com/.../*.ttf, itself CORS-open (access-control-allow-
// origin: *) — this second fetch could in principle happen straight from
// the webview, but reqwest is the one thing that can actually SEND a
// custom User-Agent: browsers/webviews treat User-Agent as a forbidden
// header a page's own fetch() may never override, which is the one thing
// this whole trick depends on — hence a Tauri command instead of plain JS.
// The Cloudflare Worker equivalent for the web build (mirrors
// worker-feedback/'s own trust-boundary role, see that folder's README)
// is a separate, not-yet-built piece — this command only covers desktop.
#[tauri::command]
async fn fetch_google_font(family: String, weight: u32, italic: bool) -> Result<String, String> {
    use base64::Engine;
    let axis = if italic { "ital,wght@1," } else { "wght@" };
    let css_url = format!(
        "https://fonts.googleapis.com/css2?family={}:{}{}&display=swap",
        family.replace(' ', "+"),
        axis,
        weight
    );
    let client = reqwest::Client::new();
    let css = client
        .get(&css_url)
        .header("User-Agent", "Mozilla/5.0 (Linux; U; Android 2.2)")
        .send()
        .await
        .map_err(|e| e.to_string())?
        .text()
        .await
        .map_err(|e| e.to_string())?;
    // The response is one @font-face block per requested weight/style —
    // just one here since exactly one weight/italic combo was asked for.
    // Extract the URL inside its first src: url(...) rather than a full
    // CSS parse — this endpoint's own output shape is stable/simple enough
    // (confirmed via curl) that a real parser would be pure overhead.
    let ttf_url = css
        .split("url(")
        .nth(1)
        .and_then(|rest| rest.split(')').next())
        .ok_or_else(|| format!("no font URL found for '{}' — family name may not exist on Google Fonts", family))?
        .to_string();
    let bytes = client
        .get(&ttf_url)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .bytes()
        .await
        .map_err(|e| e.to_string())?;
    Ok(base64::engine::general_purpose::STANDARD.encode(&bytes))
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
        // The update channel is the public `nemo` repo's GitHub Releases —
        // tauri.conf.json's updater endpoint points at the "latest release"
        // alias (releases/latest/download/latest.json), which .github/
        // workflows/release.yml writes via `includeUpdaterJson`. No auth
        // needed: it's a public repo, read over plain HTTPS. (Previously
        // this hit a private repo's Contents API with a baked-in token —
        // dropped along with that repo.)
        .plugin(tauri_plugin_updater::Builder::new().build())
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
        .manage(application_mcp::ApplicationMcp::default())
        .invoke_handler(tauri::generate_handler![
            application_mcp::nemo_mcp_identity,
            application_mcp::nemo_mcp_ready,
            application_mcp::nemo_mcp_reply,
            run_ffmpeg,
            fetch_google_font,
            video_decode::open_video_session,
            video_decode::decode_video_frame,
            video_decode::close_video_session,
            video_decode::autobench_config,
            video_decode::autobench_report,
            video_decode::optimized_media_target,
            video_decode::create_optimized_media,
            vectorize::vectorize_image
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
