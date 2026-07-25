// ---- NEMO HTML PLUGINS ----
//
// A plugin is an ordinary HTML page in a zip, plus a small JSON manifest. It
// opens as a panel inside the app and drives it through the same `nemo` API
// scripts use (nemo-script.js), reached from the panel as `window.nemo`.
//
// Being HTML is barely a design choice — the app runs in a WebView, so a
// plugin's interface can simply BE a web page, with no toolkit to learn and no
// build step. Anyone who can write a small page can extend Nemo.
//
// PACKAGE FORMAT — deliberately minimal, and ours:
//
//   my-plugin.zip
//     nemo-plugin.json     { "name": "...", "main": "index.html",
//                            "width": 320, "height": 380, "version": "1.0" }
//     index.html
//     ...css/js/images referenced relatively
//
// A plain folder works too via loadFiles(), so development needs no packaging
// step at all.
//
// WHY THE PAGE IS INLINED RATHER THAN SERVED. The HTML references its siblings
// by relative path and there is no server to resolve those against, so every
// stylesheet, script and image is folded into the document before mounting.
// Blob URLs would preserve the paths but break the moment a script builds a
// URL by concatenation.
//
// TRUST, said plainly: the panel runs in a same-origin iframe, which is what
// lets it reach `nemo`. A plugin therefore has the app's own access — it is
// not sandboxed. Only install plugins you trust, exactly as with any editor's
// extensions.
(function () {
  'use strict';

  var _open = [];

  function u16(d, o) { return d[o] | (d[o + 1] << 8); }
  function u32(d, o) { return (d[o] | (d[o + 1] << 8) | (d[o + 2] << 16) | (d[o + 3] << 24)) >>> 0; }

  // Central-directory walk + DecompressionStream for deflate — both native
  // here, so a zip dependency would be dead weight.
  async function readZip(buf) {
    var d = new Uint8Array(buf), eocd = -1;
    for (var i = d.length - 22; i >= 0 && i > d.length - 66000; i--) {
      if (u32(d, i) === 0x06054b50) { eocd = i; break; }
    }
    if (eocd < 0) throw new Error('Archive illisible (fin de répertoire central introuvable)');
    var n = u16(d, eocd + 10), p = u32(d, eocd + 16), out = {};
    for (var k = 0; k < n; k++) {
      if (u32(d, p) !== 0x02014b50) break;
      var method = u16(d, p + 10), csize = u32(d, p + 20);
      var nameLen = u16(d, p + 28), extraLen = u16(d, p + 30), cmtLen = u16(d, p + 32);
      var lho = u32(d, p + 42);
      var name = new TextDecoder().decode(d.subarray(p + 46, p + 46 + nameLen));
      var start = lho + 30 + u16(d, lho + 26) + u16(d, lho + 28);
      var raw = d.subarray(start, start + csize), bytes;
      if (method === 0) bytes = raw.slice();
      else if (method === 8) {
        var ab = await new Response(new Blob([raw]).stream()
          .pipeThrough(new DecompressionStream('deflate-raw'))).arrayBuffer();
        bytes = new Uint8Array(ab);
      } else throw new Error('Compression non supportée dans « ' + name +' »');
      if (!/\/$/.test(name)) out[name] = bytes;
      p += 46 + nameLen + extraLen + cmtLen;
    }
    return out;
  }

  function text(files, path) { var b = files[path]; return b ? new TextDecoder().decode(b) : null; }
  function dirOf(p) { var i = p.lastIndexOf('/'); return i < 0 ? '' : p.slice(0, i + 1); }
  function resolve(base, rel) {
    if (/^(https?:|data:|blob:)/.test(rel)) return rel;
    var parts = (base + rel.replace(/^\.\//, '')).split('/'), st = [];
    parts.forEach(function (s) { if (s === '..') st.pop(); else if (s && s !== '.') st.push(s); });
    return st.join('/');
  }
  var MIME = { png: 'image/png', jpg: 'image/jpeg', jpeg: 'image/jpeg', gif: 'image/gif', svg: 'image/svg+xml', webp: 'image/webp' };
  function dataUrl(files, path) {
    var b = files[path]; if (!b) return null;
    var bin = ''; for (var i = 0; i < b.length; i++) bin += String.fromCharCode(b[i]);
    return 'data:' + (MIME[(path.split('.').pop() || '').toLowerCase()] || 'application/octet-stream') + ';base64,' + btoa(bin);
  }
  function findManifest(files) {
    var keys = Object.keys(files);
    for (var i = 0; i < keys.length; i++) if (/(^|\/)nemo-plugin\.json$/.test(keys[i])) return keys[i];
    return null;
  }

  function inlineDocument(files, mainPath) {
    var html = text(files, mainPath);
    if (html == null) throw new Error('Point d\'entrée introuvable : ' + mainPath);
    var base = dirOf(mainPath);
    var doc = new DOMParser().parseFromString(html, 'text/html');
    Array.prototype.slice.call(doc.querySelectorAll('link[rel="stylesheet"][href]')).forEach(function (l) {
      var css = text(files, resolve(base, l.getAttribute('href')));
      if (css == null) { l.remove(); return; }
      var s = doc.createElement('style'); s.textContent = css;
      l.parentNode.replaceChild(s, l);
    });
    Array.prototype.slice.call(doc.querySelectorAll('img[src]')).forEach(function (im) {
      var u = dataUrl(files, resolve(base, im.getAttribute('src')));
      if (u) im.setAttribute('src', u); else im.remove();
    });
    Array.prototype.slice.call(doc.querySelectorAll('script[src]')).forEach(function (sc) {
      var js = text(files, resolve(base, sc.getAttribute('src')));
      if (js == null) { sc.remove(); return; }
      var n = doc.createElement('script'); n.textContent = js;
      sc.parentNode.replaceChild(n, sc);
    });
    return doc;
  }

  // Injected ahead of the plugin's own scripts so `nemo` exists before they
  // run. The parent's API object is handed over directly rather than proxied:
  // a proxy would have to enumerate the surface and would silently rot as the
  // API grows.
  function bridgeSource(id) {
    return [
      '(function(){',
      '  window.nemo = parent.SMScript.api();',
      '  window.nemo.close = function(){ parent.SMPlugin.close(' + JSON.stringify(id) + '); };',
      '  window.nemo.setTitle = function(t){ parent.SMPlugin.setTitle(' + JSON.stringify(id) + ', t); };',
      '})();'
    ].join('\n');
  }

  function mount(manifest, doc) {
    var id = manifest.id;
    var win = document.createElement('div'); win.className = 'npanel nplugin';
    if (manifest.width) win.style.width = Math.min(manifest.width | 0, 640) + 'px';
    var bar = document.createElement('div'); bar.className = 'npanel-bar';
    var ttl = document.createElement('span'); ttl.textContent = manifest.name || 'Plugin';
    var close = document.createElement('button'); close.className = 'npanel-close'; close.textContent = '×';
    bar.appendChild(ttl); bar.appendChild(close);
    var frame = document.createElement('iframe'); frame.className = 'nplugin-frame';
    frame.style.height = Math.min(manifest.height | 0 || 380, 720) + 'px';
    win.appendChild(bar); win.appendChild(frame);
    document.body.appendChild(win);
    win.style.left = '180px'; win.style.top = '120px';

    var drag = null;
    bar.addEventListener('mousedown', function (e) {
      if (e.target === close) return;
      drag = { x: e.clientX - win.offsetLeft, y: e.clientY - win.offsetTop };
      e.preventDefault();
    });
    document.addEventListener('mousemove', function (e) {
      if (!drag) return;
      win.style.left = Math.max(0, e.clientX - drag.x) + 'px';
      win.style.top = Math.max(0, e.clientY - drag.y) + 'px';
      // The iframe swallows pointer events mid-drag; disabling it keeps the
      // window following the cursor once it passes over its own content.
      frame.style.pointerEvents = 'none';
    });
    document.addEventListener('mouseup', function () { drag = null; frame.style.pointerEvents = ''; });
    close.addEventListener('click', function () { closeById(id); });

    var head = doc.querySelector('head') || doc.documentElement;
    var s = doc.createElement('script'); s.textContent = bridgeSource(id);
    head.insertBefore(s, head.firstChild);
    frame.srcdoc = '<!doctype html>' + doc.documentElement.outerHTML;

    var entry = { id: id, win: win, title: ttl };
    _open.push(entry);
    return entry;
  }

  function closeById(id) {
    _open = _open.filter(function (p) {
      if (p.id !== id) return true;
      if (p.win.parentNode) p.win.parentNode.removeChild(p.win);
      return false;
    });
  }

  function loadFiles(files) {
    var manPath = findManifest(files);
    if (!manPath) throw new Error('Pas de nemo-plugin.json — ce n\'est pas un plugin Nemo');
    var man;
    try { man = JSON.parse(text(files, manPath)); }
    catch (e) { throw new Error('nemo-plugin.json illisible : ' + e.message); }
    if (!man.main) throw new Error('nemo-plugin.json sans « main »');
    man.id = man.id || man.name || ('plugin-' + Object.keys(files).length);
    var root = dirOf(manPath);
    var mainPath = resolve(root, man.main);
    if (!files[mainPath]) throw new Error('Point d\'entrée « ' + man.main + ' » absent du paquet');
    var entry = mount(man, inlineDocument(files, mainPath));
    return { manifest: man, files: Object.keys(files).length, entry: entry };
  }

  async function loadArchive(buf) { return loadFiles(await readZip(buf)); }

  function openFile() {
    var inp = document.createElement('input');
    inp.type = 'file'; inp.accept = '.zip,.nemoplug'; inp.style.display = 'none';
    document.body.appendChild(inp);
    inp.addEventListener('change', function () {
      var f = inp.files && inp.files[0];
      if (!f) { inp.remove(); return; }
      f.arrayBuffer().then(loadArchive).then(function (r) {
        if (window.showToast) showToast('Plugin « ' + r.manifest.name + ' » chargé');
        window.__nemoPluginLast = r;
      }).catch(function (e) {
        if (window.showToast) showToast('« ' + f.name + ' » : ' + e.message);
        window.__nemoPluginLast = { error: e.message };
      }).then(function () { inp.remove(); });
    });
    inp.click();
  }

  window.SMPlugin = {
    openFile: openFile,
    loadArchive: loadArchive,
    loadFiles: loadFiles,
    close: closeById,
    closeAll: function () { _open.slice().forEach(function (p) { closeById(p.id); }); },
    openCount: function () { return _open.length; },
    setTitle: function (id, t) { _open.forEach(function (p) { if (p.id === id) p.title.textContent = t; }); }
  };
})();
