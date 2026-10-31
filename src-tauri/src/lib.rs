use tauri::{Emitter, Manager};
use tauri_plugin_shell::process::CommandEvent;
use tauri_plugin_shell::ShellExt;

// EXPERIMENTAL (experimental/native-video-decode) — see the module header.
mod video_decode;
// Pure text parsing of `ffmpeg -i` output, split out of video_decode (P28).
mod application_mcp;
mod media_probe;
mod native_application;
mod native_application_commands;
mod native_application_contract;
mod native_application_ports;
mod native_application_viewport;
mod native_dispatch;
mod native_object_bootstrap;
mod native_object_desktop_export;
mod native_object_desktop_preview;
mod native_reproduction_session;
mod native_viewport;
mod vectorize;
// Per-task isolation of the app's native mutable state (R06, #902).
mod task_runtime;

#[cfg(feature = "test-play-publication-observer")]
mod test_play_observer;
const CLOSE_PROJECT_MENU_ID: &str = "close_project";

#[derive(Debug, PartialEq)]
enum CloseWindowEdit {
    Remove(usize),
    ReplaceWithCloseProject(usize),
}

/// Which predefined "Close Window" items of one submenu to drop (highest index first, so each
/// index stays valid while applied) and which of them becomes the Cmd+W "Close Project" item.
/// `predefined_text[i]` is the text of item `i` when it is a predefined item, else `None`.
fn close_window_edits(submenu_text: &str, predefined_text: &[Option<String>]) -> Vec<CloseWindowEdit> {
    let mut edits = Vec::new();
    for (index, text) in predefined_text.iter().enumerate().rev() {
        // muda spells the default text "C&lose Window" (mnemonic) and strips the `&` on macOS.
        if text.as_deref().map(|t| t.replace('&', "")).as_deref() != Some("Close Window") {
            continue;
        }
        edits.push(if submenu_text == "File" {
            CloseWindowEdit::ReplaceWithCloseProject(index)
        } else {
            CloseWindowEdit::Remove(index)
        });
    }
    edits
}

/// Tauri's default menu binds Cmd+W to the predefined "Close Window" item, which closes the
/// only window and quits the app. Replace it (File menu: a "Close Project" item carrying the
/// accelerator that the page turns into the tab bar's own close; elsewhere: removed).
#[cfg(target_os = "macos")]
fn route_close_window_to_project<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    menu: &tauri::menu::Menu<R>,
) -> tauri::Result<()> {
    use tauri::menu::{MenuItem, MenuItemKind};
    for top in menu.items()? {
        let MenuItemKind::Submenu(submenu) = top else {
            continue;
        };
        let texts = submenu
            .items()?
            .iter()
            .map(|item| match item {
                MenuItemKind::Predefined(p) => p.text().map(Some),
                _ => Ok(None),
            })
            .collect::<tauri::Result<Vec<_>>>()?;
        for edit in close_window_edits(&submenu.text()?, &texts) {
            match edit {
                CloseWindowEdit::Remove(index) => {
                    submenu.remove_at(index)?;
                }
                CloseWindowEdit::ReplaceWithCloseProject(index) => {
                    submenu.remove_at(index)?;
                    let close = MenuItem::with_id(
                        app,
                        CLOSE_PROJECT_MENU_ID,
                        "Close Project",
                        true,
                        Some("CmdOrCtrl+W"),
                    )?;
                    submenu.insert(&close, index)?;
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod close_window_menu_tests {
    use super::{close_window_edits, CloseWindowEdit};

    fn predefined(texts: &[&str]) -> Vec<Option<String>> {
        texts
            .iter()
            .map(|t| (!t.is_empty()).then(|| t.to_string()))
            .collect()
    }

    #[test]
    fn file_menu_close_window_becomes_the_cmd_w_close_project_item() {
        let edits = close_window_edits("File", &predefined(&["Close Window"]));
        assert_eq!(edits, vec![CloseWindowEdit::ReplaceWithCloseProject(0)]);
    }

    #[test]
    fn the_mnemonic_spelling_is_recognised() {
        let edits = close_window_edits("File", &predefined(&["C&lose Window"]));
        assert_eq!(edits, vec![CloseWindowEdit::ReplaceWithCloseProject(0)]);
    }

    #[test]
    fn window_menu_close_window_is_removed_so_it_cannot_quit_the_app() {
        let items = predefined(&["Minimize", "Zoom", "", "Close Window"]);
        let edits = close_window_edits("Window", &items);
        assert_eq!(edits, vec![CloseWindowEdit::Remove(3)]);
    }

    #[test]
    fn other_predefined_items_and_non_predefined_items_are_untouched() {
        assert!(close_window_edits("Edit", &predefined(&["Undo", "", "Copy"])).is_empty());
        assert!(close_window_edits("File", &[None]).is_empty());
        // Quit lives in the app menu and is deliberately not edited (Cmd+Q is unchanged).
        assert!(close_window_edits("Nemo", &predefined(&["Quit Nemo"])).is_empty());
    }

    #[test]
    fn edits_apply_from_the_highest_index_down() {
        let edits = close_window_edits("File", &predefined(&["Close Window", "", "Close Window"]));
        assert_eq!(
            edits,
            vec![
                CloseWindowEdit::ReplaceWithCloseProject(2),
                CloseWindowEdit::ReplaceWithCloseProject(0)
            ]
        );
    }
}

#[tauri::command]
async fn run_ffmpeg(
    app: tauri::AppHandle,
    window: tauri::Window,
    args: Vec<String>,
) -> Result<i32, String> {
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
    application_mcp::fetch_google_font(family, weight, italic).await
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
    // T12: the native preview/export owner decodes video through the shell's sessions.
    native_dispatch::video::install_video_decoder(video_decode::boxed_decoder);
    let mut ctx = tauri::generate_context!();
    // R06 (#902): opt-in per-task isolation of every mutable native root.
    // Without NEMO_TAURI_DATA_DIR this is None and not one resolved path below
    // changes; with it, an invalid request aborts instead of quietly falling
    // back to the shared production identifier. See src/task_runtime.rs.
    let task_runtime = match task_runtime::TaskRuntime::from_env() {
        Ok(runtime) => runtime,
        Err(err) => {
            eprintln!("nemo: refusing to start an isolated task runtime: {err}");
            std::process::exit(2);
        }
    };
    if let Some(runtime) = &task_runtime {
        runtime.apply_to_config(ctx.config_mut());
    }
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
            application_mcp::nemo_native_status,
            application_mcp::nemo_native_dispatch,
            application_mcp::nemo_native_object_dispatch,
            application_mcp::nemo_native_object_client_status,
            application_mcp::nemo_native_revision_sync,
            native_application_commands::nemo_native_bootstrap,
            native_object_bootstrap::nemo_native_object_bootstrap,
            native_object_bootstrap::nemo_native_object_inactive_project,
            native_object_bootstrap::nemo_native_object_reference,
            native_object_desktop_export::nemo_native_object_export,
            native_object_desktop_preview::nemo_native_object_preview,
            native_object_bootstrap::nemo_native_object_save_as,
            native_object_bootstrap::nemo_native_object_recovery_write,
            native_object_bootstrap::nemo_native_object_recovery_list,
            native_object_bootstrap::nemo_native_object_recovery_discard,
            native_object_bootstrap::nemo_native_object_recovery_quarantine,
            native_reproduction_session::nemo_native_reproduction_session,
            native_dispatch::nemo_native_prepare_inactive_project,
            native_dispatch::nemo_native_cancel_inactive_project,
            native_application_commands::nemo_native_replace,
            native_application_commands::nemo_native_bind_output,
            native_application_commands::nemo_native_preview,
            native_application_commands::nemo_native_viewport_resize,
            native_application_commands::nemo_native_viewport_dispose,
            native_application_commands::nemo_native_release,
            run_ffmpeg,
            fetch_google_font,
            video_decode::open_video_session,
            video_decode::decode_video_frame,
            video_decode::close_video_session,
            video_decode::autobench_config,
            video_decode::autobench_report,
            video_decode::optimized_media_target,
            video_decode::create_optimized_media,
            vectorize::vectorize_image,
            task_runtime::nemo_task_runtime,
            task_runtime::nemo_task_runtime_release,
            #[cfg(feature = "test-object-window-probe")]
            task_runtime::object_window_probe::nemo_test_object_window_probe,
            #[cfg(feature = "test-p38-fixture-open-probe")]
            task_runtime::p38_fixture_open_probe::control::nemo_test_p38_fixture_open_probe,
            #[cfg(feature = "test-play-publication-observer")]
            task_runtime::nemo_test_play_observer,
            #[cfg(feature = "test-play-publication-observer")]
            task_runtime::startup_observer::host::nemo_test_startup_record,
            #[cfg(feature = "test-play-publication-observer")]
            task_runtime::startup_observer::host::footer::nemo_test_startup_cut_drained,
            #[cfg(feature = "test-play-publication-observer")]
            task_runtime::startup_observer::host::footer::nemo_test_startup_cut_snapshot
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
            } else if event.id() == CLOSE_PROJECT_MENU_ID {
                let _ = app.emit("menu-close-project", ());
            }
        })
        .setup(move |app| {
            // First: in the isolated path the configured windows are created
            // here (with their own WebKit data store) instead of by Tauri, so
            // this has to run before anything resolves the "main" window.
            task_runtime::start(app, task_runtime)?;
            #[cfg(debug_assertions)]
            {
                let window = app.get_webview_window("main").unwrap();
                window.open_devtools();
            }
            #[cfg(target_os = "macos")]
            {
                use tauri::menu::{Menu, MenuItem, MenuItemKind};
                let menu = Menu::default(app.handle())?;
                route_close_window_to_project(app.handle(), &menu)?;
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
        .run(ctx)
        .expect("error while running tauri application");
}
