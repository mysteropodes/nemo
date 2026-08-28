// ---- Assets panel top tabs (2026-08) ----
// Feedback: "réunisse media et animation preset dans le même onglet mais
// comme dans motion stack des onglets de switch horizontaux" — #assets-sec
// (index.html) hosts both the Media catalog (media-library.js) and the
// Animation presets library (motion-preset-picker.js) under one header,
// switched by a MotionStack-style horizontal tab bar. This file only owns
// the switch itself; each view still renders its own content independently.
(function () {
  var tabs = [
    { btn: 'assets-tab-media', view: 'assets-view-media' },
    { btn: 'assets-tab-presets', view: 'assets-view-presets' },
  ];
  function select(activeBtnId) {
    tabs.forEach(function (t) {
      var isActive = t.btn === activeBtnId;
      var btn = document.getElementById(t.btn);
      var view = document.getElementById(t.view);
      if (btn) btn.classList.toggle('active', isActive);
      if (view) view.style.display = isActive ? '' : 'none';
    });
  }
  tabs.forEach(function (t) {
    var btn = document.getElementById(t.btn);
    if (btn) btn.addEventListener('click', function () { select(t.btn); });
  });
})();
