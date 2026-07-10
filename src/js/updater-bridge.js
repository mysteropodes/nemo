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
  // "StrokeMotion v0.4.0" verbatim, forgotten on every version bump (the
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
      var statusEl = document.getElementById('status-text');
      if (statusEl) statusEl.textContent = 'StrokeMotion v' + v;
      document.title = 'StrokeMotion v' + v;
    } catch (e) {}
  }

  async function doInstall(update, statusEl) {
    try {
      if (statusEl) statusEl.textContent = 'Téléchargement…';
      await update.downloadAndInstall();
      if (statusEl) statusEl.textContent = 'Installée — redémarre l\'app pour l\'utiliser.';
      showToast('Mise à jour installée — redémarre StrokeMotion');
    } catch (e) {
      console.warn('[updater] install failed', e);
      if (statusEl) statusEl.textContent = 'Échec de l\'installation.';
      showToast('Échec de l\'installation de la mise à jour');
    }
  }

  async function checkForUpdate(silent) {
    var btn = document.getElementById('app-check-update');
    var statusEl = document.getElementById('app-update-status');
    if (!tauriOk()) { if (!silent) showToast('Updater indisponible (pas dans l\'app native)'); return; }
    if (btn) { btn.disabled = true; }
    if (statusEl && !silent) statusEl.innerHTML = 'Recherche…';
    try {
      var update = await window.__TAURI__.updater.check();
      if (!update) {
        if (statusEl) statusEl.innerHTML = 'Version <span id="app-version-txt"></span> — à jour';
        showVersion();
        if (!silent) showToast('Aucune mise à jour disponible');
      } else {
        var msg = 'Nouvelle version ' + update.version + ' disponible' + (update.body ? ('\n\n' + update.body) : '') + '\n\nInstaller maintenant ?';
        if (statusEl) statusEl.textContent = 'Version ' + update.version + ' disponible';
        if (confirm(msg)) await doInstall(update, statusEl);
      }
    } catch (e) {
      console.warn('[updater] check failed', e);
      if (statusEl) statusEl.textContent = 'Échec de la vérification.';
      if (!silent) showToast('Échec de la vérification des mises à jour');
    }
    if (btn) btn.disabled = false;
  }

  function init() {
    showVersion();
    var btn = document.getElementById('app-check-update');
    if (btn) btn.addEventListener('click', function () { checkForUpdate(false); });
    // "Vérifier les mises à jour…" in the StrokeMotion app menu (top-left,
    // macOS) — Rust (src-tauri/src/lib.rs setup()) inserts that item into
    // the default menu and emits this event on click; same check as the
    // Réglages button, just reachable from the menu too.
    if (tauriOk() && window.__TAURI__.event) {
      window.__TAURI__.event.listen('menu-check-update', function () { checkForUpdate(false); });
    }
    // Silent startup check, a few seconds in so it never competes with the
    // app's own initial layout/engine-init work for the first paint.
    if (tauriOk()) setTimeout(function () { checkForUpdate(true); }, 4000);
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', init); else init();
})();
