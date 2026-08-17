// WebGPU gate for the public web beta. Runs synchronously, before the rest
// of the app's scripts even load, so an unsupported browser gets a clear
// explanation instead of a silent crash mid-boot (Rust/vello engine, canvas
// pipeline, etc. all assume navigator.gpu exists). Never runs inside the
// Tauri desktop build — WKWebView's navigator.gpu is verified working there
// (see geometry-wasm-loader.js), and __TAURI__ is always present in that
// shell before any script executes.
(function () {
  if (typeof window.__TAURI__ !== 'undefined') return;
  if (navigator.gpu) return;

  document.title = 'Nemo — navigateur non compatible';
  var style = document.createElement('style');
  style.textContent =
    '#gpu-gate{position:fixed;inset:0;z-index:999999;background:#14161c;color:#e8e9ee;' +
    'font-family:Manrope,-apple-system,BlinkMacSystemFont,sans-serif;display:flex;' +
    'align-items:center;justify-content:center;text-align:center;padding:32px;box-sizing:border-box;}' +
    '#gpu-gate .card{max-width:520px;}' +
    '#gpu-gate h1{font-size:22px;margin:0 0 12px;}' +
    '#gpu-gate p{font-size:15px;line-height:1.5;color:#b7bac4;margin:0 0 16px;}' +
    '#gpu-gate ul{text-align:left;display:inline-block;margin:0 0 8px;padding-left:20px;color:#e8e9ee;font-size:14px;}' +
    '#gpu-gate li{margin:4px 0;}' +
    '#gpu-gate .small{font-size:12px;color:#7a7d88;margin-top:20px;}';
  document.head.appendChild(style);

  var gate = document.createElement('div');
  gate.id = 'gpu-gate';
  gate.innerHTML =
    '<div class="card">' +
      '<h1>Ton navigateur ne supporte pas WebGPU</h1>' +
      '<p>Nemo a besoin de WebGPU pour dessiner — ce navigateur ne l\'active pas (ou pas encore).</p>' +
      '<p>Navigateurs compatibles aujourd\'hui :</p>' +
      '<ul>' +
        '<li>Chrome / Edge / Brave récents (Windows, macOS, ChromeOS)</li>' +
        '<li>Safari 18+ sur macOS Sequoia (à activer dans Réglages Safari &rarr; Avancé &rarr; Fonctionnalités Web si besoin)</li>' +
      '</ul>' +
      '<p>Firefox stable et la plupart des navigateurs mobiles ne sont pas encore compatibles.</p>' +
      '<p class="small">Si tu penses que ton navigateur devrait fonctionner, essaie de le mettre à jour puis recharge cette page.</p>' +
    '</div>';

  // Stop the rest of the app's scripts from ever running: every later
  // <script src> on the page throws immediately, so nothing touches
  // navigator.gpu-dependent code paths. Cheap and total — this page is
  // done either way once the gate is up.
  window.stop();
  var showGate = function () {
    document.body.innerHTML = '';
    document.body.appendChild(gate);
  };
  if (document.body) showGate();
  else document.addEventListener('DOMContentLoaded', showGate);

  throw new Error('[gpu-gate] WebGPU unavailable, blocking app boot');
})();
