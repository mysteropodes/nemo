// ---- App auto-updater (Tauri updater plugin) ----
// The Rust side (src-tauri/src/lib.rs) registers tauri_plugin_updater with
// a read-only, repo-scoped token baked in at compile time, and
// "updater:default" is already granted in capabilities/default.json — but
// NOTHING on the frontend ever called check()/downloadAndInstall(), so the
// whole plugin sat there wired but silent (reported as "je ne vois pas de
// check d'update dans le menu" — there wasn't one, anywhere). This file is
// that missing piece: a manual button in Réglages, plus a silent check a
// few seconds after launch that only bothers the user if something's
// actually available.
(function () {
  function tauriOk() { return typeof window.__TAURI__ !== 'undefined' && window.__TAURI__.updater; }

  // Reads the version straight from the Tauri app API instead of a
  // hardcoded string in index.html — <title> and #status-text used to say
  // "Nemo v0.4.0" verbatim, forgotten on every version bump (the
  // Réglages panel's own version display had the same problem before this
  // function existed). One source of truth (package.json/tauri.conf.json's
  // "version", read at runtime via getVersion()) instead of three places
  // to remember to edit by hand.
  async function showVersion() {
    if (typeof window.__TAURI__ === 'undefined' || !window.__TAURI__.app) return;
    try {
      var v = await window.__TAURI__.app.getVersion();
      var vEl = document.getElementById('app-version-txt');
      if (vEl) vEl.textContent = v;
      var vElAbout = document.getElementById('app-version-txt-about');
      if (vElAbout) vElAbout.textContent = v;
      var statusEl = document.getElementById('status-text');
      if (statusEl) statusEl.textContent = 'Nemo v' + v;
      document.title = 'Nemo v' + v;
    } catch (e) {}
  }

  async function doInstall(update, statusEl) {
    try {
      // downloadAndInstall() on a ~30MB artifact silently ran for several
      // seconds with ZERO feedback anywhere the user was actually looking —
      // the confirm dialog closes the instant they click OK, and statusEl
      // (Réglages' own status line) is invisible unless that panel happens
      // to be open, which it usually isn't right after a startup prompt.
      // Reported as "je clique OK et rien ne se passe" even though the
      // install had, in fact, fully succeeded. A toast with live progress
      // is the one channel guaranteed visible regardless of what panel is
      // open — plus it's the only sign of life during the download itself.
      var total = 0, received = 0;
      showToast(window.SM.t('updaterDownloading'));
      if (statusEl) statusEl.textContent = window.SM.t('updaterDownloadingShort');
      await update.downloadAndInstall(function (ev) {
        if (ev.event === 'Started') {
          total = ev.data.contentLength || 0;
        } else if (ev.event === 'Progress') {
          received += ev.data.chunkLength || 0;
          if (total > 0) {
            var pct = Math.min(100, Math.round(received / total * 100));
            if (statusEl) statusEl.textContent = window.SM.t('updaterDownloadingPct').replace('{pct}', pct);
          }
        } else if (ev.event === 'Finished') {
          if (statusEl) statusEl.textContent = window.SM.t('updaterInstalling');
        }
      });
      if (statusEl) statusEl.textContent = window.SM.t('updaterInstalledStatus').replace('{v}', update.version);
      // Offer to relaunch right now instead of leaving the user to
      // stumble onto the new version number later (which is how this got
      // reported as "did nothing" the first time — the update HAD worked,
      // there was just no visible confirmation and no easy way to actually
      // pick up the new build besides quitting and reopening by hand).
      var restartNow = tauriOk() && window.__TAURI__.process
        ? await window.__TAURI__.dialog.confirm(window.SM.t('updaterRestartConfirm').replace('{v}', update.version), { title: window.SM.t('updaterRestartConfirmTitle') })
        : false;
      if (restartNow) {
        await window.__TAURI__.process.relaunch();
      } else {
        showToast(window.SM.t('updaterInstalledToast').replace('{v}', update.version));
      }
    } catch (e) {
      console.warn('[updater] install failed', e);
      if (statusEl) statusEl.textContent = window.SM.t('updaterInstallFailedStatus');
      // The raw Rust error (e.g. "TargetsNotFound", "signature mismatch",
      // an HTTP status) used to be swallowed — this was the ONLY thing
      // shown to the user ("Échec de l'installation"), no way to tell
      // auth/network/signature/target-mismatch apart without devtools,
      // which a release build doesn't even expose. Always show it now.
      showToast(window.SM.t('updaterInstallFailedToast').replace('{e}', e && e.message ? e.message : e));
    }
  }

  async function checkForUpdate(silent) {
    var btn = document.getElementById('app-check-update');
    var statusEl = document.getElementById('app-update-status');
    if (!tauriOk()) { if (!silent) showToast(window.SM.t('updaterUnavailable')); return; }
    if (btn) { btn.disabled = true; }
    if (statusEl && !silent) statusEl.innerHTML = window.SM.t('updaterSearching');
    try {
      var update = await window.__TAURI__.updater.check();
      if (!update) {
        if (statusEl) statusEl.innerHTML = window.SM.t('updaterUpToDateStatus');
        showVersion();
        if (!silent) showToast(window.SM.t('updaterNoUpdateToast'));
      } else {
        var msg = window.SM.t('updaterNewVersionMsg').replace('{v}', update.version).replace('{body}', update.body ? ('\n\n' + update.body) : '');
        if (statusEl) statusEl.textContent = window.SM.t('updaterNewVersionStatus').replace('{v}', update.version);
        // Plain window.confirm() is intercepted inside the Tauri webview and
        // routed to a deprecated/missing plugin command ("dialog.confirm not
        // allowed. Command not found") — worse, it returns a Promise, not a
        // synchronous boolean like a real browser, so `if (confirm(msg))`
        // was always truthy (a Promise object) and installed unconditionally
        // regardless of what the user actually clicked. The real, supported,
        // Promise<boolean> API is window.__TAURI__.dialog.confirm().
        var proceed = await window.__TAURI__.dialog.confirm(msg, { title: window.SM.t('updaterAvailableDialogTitle') });
        if (proceed) await doInstall(update, statusEl);
      }
    } catch (e) {
      console.warn('[updater] check failed', e);
      var detail = (e && e.message) ? e.message : String(e);
      if (statusEl) statusEl.textContent = window.SM.t('updaterCheckFailedStatus').replace('{e}', detail);
      if (!silent) showToast(window.SM.t('updaterCheckFailedToast').replace('{e}', detail));
    }
    if (btn) btn.disabled = false;
  }

  // ---- macOS titlebar update button (2026-07) ----------------------------
  // A persistent icon parked right next to the native traffic lights
  // (tauri.conf.json's titleBarStyle:"Overlay" + trafficLightPosition —
  // see #mac-titlebar-strip/#mac-update-btn in index.html) instead of the
  // one-shot confirm() dialog: hidden while up to date, a download-arrow
  // once an update is found (silent startup check populates this instead
  // of interrupting with a dialog), a spinner while downloading, then a
  // reopen-arrow once installed — click advances the state, matching the
  // reference GIFs. macOS + Tauri only; a no-op (icon stays display:none
  // from its own CSS) everywhere else, including this browser preview.
  var ICON_DOWNLOAD = '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><path d="M12 4v12M6 12l6 6 6-6"/></svg>';
  var ICON_REOPEN = '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><path d="M20 12a8 8 0 1 1-2.6-5.9"/><path d="M20 4v5h-5"/></svg>';
  var ICON_SPINNER = '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round"><circle cx="12" cy="12" r="9" stroke-opacity=".3"/><path d="M21 12a9 9 0 0 0-9-9"/></svg>';
  var _macPendingUpdate = null;
  function isMac() { return /Mac/i.test(navigator.platform || navigator.userAgent || ''); }
  function macBtn() { return document.getElementById('mac-update-btn'); }
  function macBtnSetState(state) {
    var btn = macBtn();
    if (!btn) return;
    btn.className = state ? ('state-' + state) : '';
    if (state === 'available') { btn.title = window.SM.t('updaterMacUpdateTitle'); btn.innerHTML = ICON_DOWNLOAD; }
    else if (state === 'downloading') { btn.title = ''; btn.innerHTML = ICON_SPINNER; }
    else if (state === 'installed') { btn.title = window.SM.t('updaterMacReopenTitle'); btn.innerHTML = ICON_REOPEN; }
    else { btn.title = ''; btn.innerHTML = ''; }
  }
  async function macBtnDownload() {
    if (!_macPendingUpdate) return;
    var update = _macPendingUpdate;
    macBtnSetState('downloading');
    try {
      await update.downloadAndInstall(function () {}); // progress not surfaced on this compact icon — the Réglages panel's own status line (doInstall) already covers that for the manual-check flow
      macBtnSetState('installed');
      showToast(window.SM.t('updaterMacInstalledToast').replace('{v}', update.version));
    } catch (e) {
      console.warn('[updater] mac titlebar install failed', e);
      macBtnSetState('available'); // let them retry rather than getting stuck on a dead spinner
      showToast(window.SM.t('updaterInstallFailedToast').replace('{e}', e && e.message ? e.message : e));
    }
  }
  function initMacUpdateButton() {
    if (!isMac() || !tauriOk()) return; // stays display:none — see CSS
    document.body.classList.add('mac-overlay-titlebar');
    var btn = macBtn();
    if (!btn) return;
    btn.addEventListener('click', function () {
      if (btn.classList.contains('state-available')) macBtnDownload();
      else if (btn.classList.contains('state-installed')) { if (window.__TAURI__.process) window.__TAURI__.process.relaunch(); }
    });
  }
  // Silent check populates the titlebar icon instead of interrupting with
  // checkForUpdate's confirm() dialog — the explicit "Vérifier les mises à
  // jour" button in Réglages (and the app-menu item) keep that dialog,
  // since clicking either one already IS the explicit "yes I want to know
  // now" action a dialog makes sense for.
  async function macSilentCheck() {
    if (!isMac() || !tauriOk()) return;
    try {
      var update = await window.__TAURI__.updater.check();
      if (update) { _macPendingUpdate = update; macBtnSetState('available'); }
    } catch (e) { console.warn('[updater] mac silent check failed', e); }
  }

  function init() {
    showVersion();
    initMacUpdateButton();
    var btn = document.getElementById('app-check-update');
    if (btn) btn.addEventListener('click', function () { checkForUpdate(false); });
    // "Vérifier les mises à jour…" in the Nemo app menu (top-left,
    // macOS) — Rust (src-tauri/src/lib.rs setup()) inserts that item into
    // the default menu and emits this event on click; same check as the
    // Réglages button, just reachable from the menu too.
    if (tauriOk() && window.__TAURI__.event) {
      window.__TAURI__.event.listen('menu-check-update', function () { checkForUpdate(false); });
    }
    // Silent startup check, a few seconds in so it never competes with the
    // app's own initial layout/engine-init work for the first paint. On
    // macOS this populates the titlebar icon (macSilentCheck) instead of
    // the dialog-based checkForUpdate(true) used on other platforms.
    if (tauriOk()) setTimeout(function () { if (isMac()) macSilentCheck(); else checkForUpdate(true); }, 4000);
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', init); else init();
})();
