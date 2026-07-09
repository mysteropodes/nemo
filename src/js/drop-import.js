// ---- DRAG-AND-DROP IMPORT (Finder -> canvas or timeline) ----
// Dropping audio/video/image files directly onto the canvas or the
// timeline routes them to the SAME importers the toolbar buttons already
// use (audio-bridge.js's audio track import, reference-bridge.js's roto
// reference import) — no separate code path to keep in sync, just a
// different entry point into the exact same logic, so "which track/layer
// gets created" always matches what a manual Importer… click would do.
(function () {
  function isDraggingFiles(e) {
    return e.dataTransfer && Array.prototype.indexOf.call(e.dataTransfer.types || [], 'Files') >= 0;
  }
  function handleDragOver(e) {
    if (!isDraggingFiles(e)) return;
    e.preventDefault();
    e.dataTransfer.dropEffect = 'copy';
    e.currentTarget.classList.add('drop-hover');
  }
  function handleDragLeave(e) {
    e.currentTarget.classList.remove('drop-hover');
  }
  function handleDrop(e) {
    if (!isDraggingFiles(e)) return;
    e.preventDefault();
    e.currentTarget.classList.remove('drop-hover');
    var files = Array.prototype.slice.call(e.dataTransfer.files || []);
    if (!files.length) return;
    var audioFiles = files.filter(function (f) { return f.type.indexOf('audio/') === 0; });
    var otherFiles = files.filter(function (f) { return f.type.indexOf('audio/') !== 0; });
    if (audioFiles.length && window.SMAudio) audioFiles.forEach(function (f) { window.SMAudio.importFile(f); });
    // otherFiles may mix video/image — reference-bridge.js's own importFiles
    // already picks the right interpretation (single video, single image,
    // or a multi-image sequence) and reports an error for anything else.
    if (otherFiles.length && window.SMReference) window.SMReference.importFiles(otherFiles);
    if (!audioFiles.length && !otherFiles.length) return;
    var parts = [];
    if (audioFiles.length) parts.push(audioFiles.length + ' piste(s) audio');
    if (otherFiles.length) parts.push('référence');
    if (window.showToast) showToast('Import : ' + parts.join(' + '));
  }
  function wire(id) {
    var el = document.getElementById(id);
    if (!el) return;
    el.addEventListener('dragover', handleDragOver);
    el.addEventListener('dragleave', handleDragLeave);
    el.addEventListener('drop', handleDrop);
  }
  function init() {
    wire('canvas-area');
    wire('timeline-area');
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', init);
  else init();
})();
