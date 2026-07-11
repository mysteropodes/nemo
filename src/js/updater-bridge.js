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
      showToast('Téléchargement de la mise à jour…');
      if (statusEl) statusEl.textContent = 'Téléchargement…';
      await update.downloadAndInstall(function (ev) {
        if (ev.event === 'Started') {
          total = ev.data.contentLength || 0;
        } else if (ev.event === 'Progress') {
          received += ev.data.chunkLength || 0;
          if (total > 0) {
            var pct = Math.min(100, Math.round(received / total * 100));
            if (statusEl) statusEl.textContent = 'Téléchargement… ' + pct + '%';
          }
        } else if (ev.event === 'Finished') {
          if (statusEl) statusEl.textContent = 'Installation…';
        }
      });
      if (statusEl) statusEl.textContent = 'Installée — v' + update.version;
      // Offer to relaunch right now instead of leaving the user to
      // stumble onto the new version number later (which is how this got
      // reported as "did nothing" the first time — the update HAD worked,
      // there was just no visible confirmation and no easy way to actually
      // pick up the new build besides quitting and reopening by hand).
      var restartNow = tauriOk() && window.__TAURI__.process
        ? await window.__TAURI__.dialog.confirm('Mise à jour installée (v' + update.version + '). Redémarrer Nemo maintenant ?', { title: 'Mise à jour installée' })
        : false;
      if (restartNow) {
        await window.__TAURI__.process.relaunch();
      } else {
        showToast('Mise à jour v' + update.version + ' installée — redémarre Nemo pour l\'utiliser');
      }
    } catch (e) {
      console.warn('[updater] install failed', e);
      if (statusEl) statusEl.textContent = 'Échec de l\'installation.';
      // The raw Rust error (e.g. "TargetsNotFound", "signature mismatch",
      // an HTTP status) used to be swallowed — this was the ONLY thing
      // shown to the user ("Échec de l'installation"), no way to tell
      // auth/network/signature/target-mismatch apart without devtools,
      // which a release build doesn't even expose. Always show it now.
      showToast('Échec de l\'installation — ' + (e && e.message ? e.message : e));
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
        // Plain window.confirm() is intercepted inside the Tauri webview and
        // routed to a deprecated/missing plugin command ("dialog.confirm not
        // allowed. Command not found") — worse, it returns a Promise, not a
        // synchronous boolean like a real browser, so `if (confirm(msg))`
        // was always truthy (a Promise object) and installed unconditionally
        // regardless of what the user actually clicked. The real, supported,
        // Promise<boolean> API is window.__TAURI__.dialog.confirm().
        var proceed = await window.__TAURI__.dialog.confirm(msg, { title: 'Mise à jour disponible' });
        if (proceed) await doInstall(update, statusEl);
      }
    } catch (e) {
      console.warn('[updater] check failed', e);
      var detail = (e && e.message) ? e.message : String(e);
      if (statusEl) statusEl.textContent = 'Échec de la vérification — ' + detail;
      if (!silent) showToast('Échec de la vérification — ' + detail);
    }
    if (btn) btn.disabled = false;
  }

  function init() {
    showVersion();
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
    // app's own initial layout/engine-init work for the first paint.
    if (tauriOk()) setTimeout(function () { checkForUpdate(true); }, 4000);
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', init); else init();
})();
