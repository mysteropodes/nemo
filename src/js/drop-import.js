// ---- DRAG-AND-DROP IMPORT (Finder -> canvas or timeline) ----
// Dropping audio/video/image files directly onto the canvas or the
// timeline routes them to the SAME importers the toolbar/Media panel
// already use (audio-bridge.js's audio track import, images.js's real
// image/video layer import) — no separate code path to keep in sync, just
// a different entry point into the exact same logic, so "which track/
// layer gets created" always matches what a manual Importer… click (or a
// drop onto the Media panel) would do.
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
    var imageFiles = files.filter(function (f) { return f.type.indexOf('image/') === 0; });
    var videoFiles = files.filter(function (f) { return f.type.indexOf('video/') === 0; });
    var otherFiles = files.filter(function (f) {
      return f.type.indexOf('audio/') !== 0 && f.type.indexOf('image/') !== 0 && f.type.indexOf('video/') !== 0;
    });
    if (audioFiles.length && window.SMAudio) audioFiles.forEach(function (f) { window.SMAudio.importFile(f); });
    // Image/video dropped on canvas or timeline become real layers — same
    // pipeline the Media panel's own drop zone uses (media-library.js's
    // initDropZone) — NOT the rotoscopy reference import this used to route
    // to (2026-08 fix, feedback: "quand on drag and drop une image ou vidéo
    // dans le canvas celui ci doit devenir un élément layer comme quand on
    // importé via média").
    if (imageFiles.length && window.SM && window.SM.importImageFiles) window.SM.importImageFiles(imageFiles);
    if (videoFiles.length && window.SM && window.SM.importVideoFile) videoFiles.forEach(function (f) { window.SM.importVideoFile(f); });
    // Anything left (neither audio, image, nor video — e.g. a PDF) still
    // falls back to the roto reference importer, which reports its own
    // "format not recognized" error for genuinely unsupported files.
    if (otherFiles.length && window.SMReference) window.SMReference.importFiles(otherFiles);
    if (!audioFiles.length && !imageFiles.length && !videoFiles.length && !otherFiles.length) return;
    var parts = [];
    if (audioFiles.length) parts.push(audioFiles.length + ' piste(s) audio');
    if (imageFiles.length) parts.push(imageFiles.length + ' image(s)');
    if (videoFiles.length) parts.push(videoFiles.length + ' vidéo(s)');
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
