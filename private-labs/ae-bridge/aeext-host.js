// ---- AE CEP EXTENSION HOST (.zxp) ----
//
// An After Effects extension is not a script — it is a CEP panel: a .zxp
// (a signed zip) containing CSXS/manifest.xml plus an ordinary HTML/CSS/JS
// front-end, which drives the host application by calling
// CSInterface.evalScript() with ExtendScript source.
//
// That shape is unusually friendly to us. The panel's UI is ALREADY HTML/JS
// running in an embedded Chromium (CEF) — and we are a browser. So there is
// nothing to port on the UI side at all: mount the panel's own HTML, and give
// it the two host objects it expects (CSInterface and the low-level
// window.__adobe_cep__). Its evalScript calls then route into
// aescript-host.js, which is the same ExtendScript host that already runs
// standalone .jsx files.
//
// WHY THE PANEL IS INLINED RATHER THAN SERVED. A .zxp's HTML references its
// siblings by relative path (./js/main.js, ./css/style.css). With no server
// to resolve those against, the reliable move is to read every entry out of
// the zip and fold scripts/styles/images into the document before mounting it,
// so nothing has to resolve a relative URL at runtime. Blob URLs for each file
// would preserve the paths but break the moment a script builds a URL by
// string concatenation, which panels do constantly.
//
// SANDBOX. The panel runs in a same-origin iframe (srcdoc), which is what lets
// CSInterface reach the host. That means an extension has as much access as
// the app itself — the same trust level AE gives a CEP panel, and worth saying
// out loud rather than implying isolation that isn't there.
//
// NOT DONE, and refused by name rather than faked: .zxp signature validation
// (the zip is read, the signature is not checked), CEP's own persistent
// storage, node/CEP filesystem APIs, and the CSXS event bus beyond a local
// dispatch/listen pair.
(function () {
  'use strict';

  var _panels = [];

  // ---- minimal ZIP reader ----
  // Central-directory walk + DecompressionStream for deflate. Native since
  // Chromium 80-ish, and this app already targets a modern WebView, so a zip
  // dependency would be dead weight.
  function u16(d, o) { return d[o] | (d[o + 1] << 8); }
  function u32(d, o) { return (d[o] | (d[o + 1] << 8) | (d[o + 2] << 16) | (d[o + 3] << 24)) >>> 0; }

  async function readZip(buf) {
    var d = new Uint8Array(buf);
    // End-of-central-directory: scan backwards for its signature.
    var eocd = -1;
    for (var i = d.length - 22; i >= 0 && i > d.length - 66000; i--) {
      if (u32(d, i) === 0x06054b50) { eocd = i; break; }
    }
    if (eocd < 0) throw new Error('Archive .zxp illisible (fin de répertoire central introuvable)');
    var n = u16(d, eocd + 10), cdOff = u32(d, eocd + 16);
    var out = {}, p = cdOff;
    for (var k = 0; k < n; k++) {
      if (u32(d, p) !== 0x02014b50) break;
      var method = u16(d, p + 10);
      var csize = u32(d, p + 20), usize = u32(d, p + 24);
      var nameLen = u16(d, p + 28), extraLen = u16(d, p + 30), cmtLen = u16(d, p + 32);
      var lho = u32(d, p + 42);
      var name = new TextDecoder().decode(d.subarray(p + 46, p + 46 + nameLen));
      // Local header tells us where the payload really starts.
      var lNameLen = u16(d, lho + 26), lExtraLen = u16(d, lho + 28);
      var dataStart = lho + 30 + lNameLen + lExtraLen;
      var raw = d.subarray(dataStart, dataStart + csize);
      var bytes;
      if (method === 0) bytes = raw.slice();
      else if (method === 8) {
        var ds = new DecompressionStream('deflate-raw');
        var ab = await new Response(new Blob([raw]).stream().pipeThrough(ds)).arrayBuffer();
        bytes = new Uint8Array(ab);
      } else throw new Error('Compression zip non supportée dans « ' + name +' » (méthode ' + method + ')');
      if (!/\/$/.test(name)) out[name] = bytes;
      p += 46 + nameLen + extraLen + cmtLen;
    }
    return out;
  }

  function textOf(files, path) {
    var b = files[path];
    return b ? new TextDecoder().decode(b) : null;
  }
  function findFile(files, suffix) {
    var keys = Object.keys(files);
    for (var i = 0; i < keys.length; i++) if (keys[i].toLowerCase().indexOf(suffix.toLowerCase()) >= 0) return keys[i];
    return null;
  }
  function dirOf(p) { var i = p.lastIndexOf('/'); return i < 0 ? '' : p.slice(0, i + 1); }
  function resolve(base, rel) {
    if (/^(https?:|data:|blob:)/.test(rel)) return rel;
    rel = rel.replace(/^\.\//, '');
    var parts = (base + rel).split('/'), stack = [];
    parts.forEach(function (s) { if (s === '..') stack.pop(); else if (s !== '.' && s !== '') stack.push(s); });
    return stack.join('/');
  }
  var MIME = { png: 'image/png', jpg: 'image/jpeg', jpeg: 'image/jpeg', gif: 'image/gif', svg: 'image/svg+xml', webp: 'image/webp' };
  function dataUrl(files, path) {
    var b = files[path]; if (!b) return null;
    var ext = (path.split('.').pop() || '').toLowerCase();
    var bin = ''; for (var i = 0; i < b.length; i++) bin += String.fromCharCode(b[i]);
    return 'data:' + (MIME[ext] || 'application/octet-stream') + ';base64,' + btoa(bin);
  }

  // ---- CSXS/manifest.xml ----
  function parseManifest(xmlText) {
    var doc = new DOMParser().parseFromString(xmlText, 'application/xml');
    if (doc.querySelector('parsererror')) throw new Error('CSXS/manifest.xml illisible');
    var ext = doc.querySelector('Extension');
    var res = doc.querySelector('Resources MainPath, MainPath');
    var geo = doc.querySelector('Geometry Size, Size');
    var name = doc.querySelector('Menu') || doc.querySelector('DispatchInfo Menu');
    return {
      id: ext ? ext.getAttribute('Id') : 'extension',
      main: res ? res.textContent.trim() : null,
      title: name ? name.textContent.trim() : (ext ? ext.getAttribute('Id') : 'Extension'),
      width: geo ? parseInt((geo.querySelector('Width') || {}).textContent || '340', 10) : 340,
      height: geo ? parseInt((geo.querySelector('Height') || {}).textContent || '420', 10) : 420
    };
  }

  // ---- fold the panel into one self-contained document ----
  function inlineDocument(files, mainPath) {
    var html = textOf(files, mainPath);
    if (html == null) throw new Error('Point d\'entrée introuvable dans l\'archive : ' + mainPath);
    var base = dirOf(mainPath);
    var doc = new DOMParser().parseFromString(html, 'text/html');

    // Styles first, so the panel is laid out before its scripts run.
    Array.prototype.slice.call(doc.querySelectorAll('link[rel="stylesheet"][href]')).forEach(function (l) {
      var css = textOf(files, resolve(base, l.getAttribute('href')));
      if (css == null) { l.remove(); return; }
      var s = doc.createElement('style'); s.textContent = css;
      l.parentNode.replaceChild(s, l);
    });
    Array.prototype.slice.call(doc.querySelectorAll('img[src]')).forEach(function (im) {
      var u = dataUrl(files, resolve(base, im.getAttribute('src')));
      if (u) im.setAttribute('src', u); else im.remove();
    });
    // CSInterface.js ships inside nearly every extension; ours must win, so
    // the bundled copy is dropped rather than inlined.
    var dropped = [];
    Array.prototype.slice.call(doc.querySelectorAll('script[src]')).forEach(function (sc) {
      var src = sc.getAttribute('src');
      if (/csinterface|cep_engine/i.test(src)) { dropped.push(src); sc.remove(); return; }
      var js = textOf(files, resolve(base, src));
      var n = doc.createElement('script');
      if (js == null) { sc.remove(); return; }
      n.textContent = js;
      sc.parentNode.replaceChild(n, sc);
    });
    return { doc: doc, dropped: dropped };
  }

  // ---- the shims the panel expects ----
  // Written as source text because it has to execute INSIDE the iframe, ahead
  // of the extension's own scripts.
  function shimSource(manifest) {
    return [
      '(function(){',
      '  var HOST = parent;',
      '  function CSEvent(type, scope, appId, extId){ this.type=type; this.scope=scope||"APPLICATION";',
      '    this.appId=appId; this.extensionId=extId; this.data=""; }',
      '  window.CSEvent = CSEvent;',
      '  var _listeners = {};',
      '  // The low-level object CSInterface itself talks to. Panels that skip',
      '  // CSInterface and call __adobe_cep__ directly (some do) then work too.',
      '  window.__adobe_cep__ = {',
      '    evalScript: function(src, cb){',
      '      var r = HOST.SMAEScript.run(src);',
      '      var out = r.ok ? (r.value === undefined ? "undefined" : String(r.value)) : ("EvalScript error: " + r.error);',
      '      if (typeof cb === "function") cb(out);',
      '      return out;',
      '    },',
      '    getHostEnvironment: function(){ return JSON.stringify({',
      '      appName: "AEFT", appVersion: "24.0", appLocale: "fr_FR",',
      '      appUILocale: "fr_FR", appId: "AEFT", isAppOnline: true,',
      '      appSkinInfo: { panelBackgroundColor: { color: { red: 32, green: 31, blue: 37, alpha: 255 } } }',
      '    }); },',
      '    getSystemPath: function(){ return ""; },',
      '    addEventListener: function(t, l){ (_listeners[t]=_listeners[t]||[]).push(l); },',
      '    removeEventListener: function(t, l){ var a=_listeners[t]||[]; var i=a.indexOf(l); if(i>=0)a.splice(i,1); },',
      '    dispatchEvent: function(e){ (_listeners[e.type]||[]).forEach(function(l){ try{ l(e); }catch(err){} }); },',
      '    getExtensionID: function(){ return ' + JSON.stringify(manifest.id) + '; },',
      '    initResourceBundle: function(){ return {}; },',
      '    closeExtension: function(){ HOST.SMAEExt.close(' + JSON.stringify(manifest.id) + '); },',
      '    requestOpenExtension: function(){},',
      '    getCurrentApiVersion: function(){ return JSON.stringify({major:11,minor:0,micro:0}); }',
      '  };',
      '  function CSInterface(){}',
      '  CSInterface.prototype.evalScript = function(s, cb){ return window.__adobe_cep__.evalScript(s, cb); };',
      '  CSInterface.prototype.getHostEnvironment = function(){ return JSON.parse(window.__adobe_cep__.getHostEnvironment()); };',
      '  CSInterface.prototype.getApplicationID = function(){ return "AEFT"; };',
      '  CSInterface.prototype.getSystemPath = function(){ return ""; };',
      '  CSInterface.prototype.getExtensionID = function(){ return window.__adobe_cep__.getExtensionID(); };',
      '  CSInterface.prototype.addEventListener = function(t,l){ window.__adobe_cep__.addEventListener(t,l); };',
      '  CSInterface.prototype.removeEventListener = function(t,l){ window.__adobe_cep__.removeEventListener(t,l); };',
      '  CSInterface.prototype.dispatchEvent = function(e){ window.__adobe_cep__.dispatchEvent(e); };',
      '  CSInterface.prototype.closeExtension = function(){ window.__adobe_cep__.closeExtension(); };',
      '  CSInterface.prototype.requestOpenExtension = function(){};',
      '  CSInterface.prototype.getHostCapabilities = function(){ return { EXTENDED_PANEL_MENU:true }; };',
      '  CSInterface.prototype.setWindowTitle = function(t){ try{ HOST.SMAEExt.setTitle(' + JSON.stringify(manifest.id) + ', t); }catch(e){} };',
      '  CSInterface.prototype.resizeContent = function(){};',
      '  CSInterface.prototype.getOSInformation = function(){ return navigator.platform; };',
      '  CSInterface.prototype.openURLInDefaultBrowser = function(u){ window.open(u, "_blank"); };',
      '  CSInterface.THEME_COLOR_CHANGED_EVENT = "com.adobe.csxs.events.ThemeColorChanged";',
      '  window.CSInterface = CSInterface;',
      '  // Panels commonly reach for these; a missing global throws before the',
      '  // panel ever draws, so they exist and refuse only when actually used.',
      '  window.cep = { fs: { readFile: function(){ throw new Error("CEP fs non supporté"); },',
      '                       writeFile: function(){ throw new Error("CEP fs non supporté"); } },',
      '                 process: {} };',
      '  window.require = function(m){ throw new Error("node require(\\"" + m + "\\") non supporté dans le pont"); };',
      '})();'
    ].join('\n');
  }

  // ---- panel window ----
  function mount(manifest, docObj, dropped) {
    var win = document.createElement('div');
    win.className = 'sui-window aeext-window';
    win.style.width = Math.min(manifest.width || 340, 640) + 'px';
    var bar = document.createElement('div'); bar.className = 'sui-titlebar';
    var ttl = document.createElement('span'); ttl.textContent = manifest.title || 'Extension';
    var close = document.createElement('button'); close.className = 'sui-close'; close.textContent = '×';
    bar.appendChild(ttl); bar.appendChild(close);
    var frame = document.createElement('iframe');
    frame.className = 'aeext-frame';
    frame.style.height = Math.min(manifest.height || 420, 720) + 'px';
    win.appendChild(bar); win.appendChild(frame);
    document.body.appendChild(win);
    win.style.left = '160px'; win.style.top = '110px';

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

    var entry = { id: manifest.id, win: win, frame: frame, title: ttl };
    close.addEventListener('click', function () { closeById(manifest.id); });
    _panels.push(entry);

    // Shim first, then the panel's own (already inlined) scripts.
    var head = docObj.querySelector('head') || docObj.documentElement;
    var s = docObj.createElement('script');
    s.textContent = shimSource(manifest);
    head.insertBefore(s, head.firstChild);
    frame.srcdoc = '<!doctype html>' + docObj.documentElement.outerHTML;
    return entry;
  }

  function closeById(id) {
    _panels = _panels.filter(function (p) {
      if (p.id !== id) return true;
      if (p.win.parentNode) p.win.parentNode.removeChild(p.win);
      return false;
    });
  }

  async function loadArchive(buf, fileName) {
    var files = await readZip(buf);
    var manPath = findFile(files, 'CSXS/manifest.xml');
    if (!manPath) throw new Error('Pas de CSXS/manifest.xml — ce fichier n\'est pas une extension CEP');
    var manifest = parseManifest(textOf(files, manPath));
    if (!manifest.main) throw new Error('manifest.xml sans <MainPath>');
    // MainPath is relative to the extension root, which is the manifest's
    // parent of CSXS/.
    var root = dirOf(manPath).replace(/CSXS\/$/, '');
    var mainPath = resolve(root, manifest.main);
    if (!files[mainPath]) {
      var alt = findFile(files, manifest.main.replace(/^\.\//, ''));
      if (!alt) throw new Error('Point d\'entrée « ' + manifest.main + ' » absent de l\'archive');
      mainPath = alt;
    }
    var inl = inlineDocument(files, mainPath);
    var entry = mount(manifest, inl.doc, inl.dropped);
    return { manifest: manifest, files: Object.keys(files).length, entry: entry, droppedScripts: inl.dropped };
  }

  function openFile() {
    var inp = document.createElement('input');
    inp.type = 'file'; inp.accept = '.zxp,.zip'; inp.style.display = 'none';
    document.body.appendChild(inp);
    inp.addEventListener('change', function () {
      var f = inp.files && inp.files[0];
      if (!f) { inp.remove(); return; }
      f.arrayBuffer().then(function (buf) {
        return loadArchive(buf, f.name);
      }).then(function (r) {
        if (window.showToast) showToast('Extension « ' + r.manifest.title + ' » chargée (' + r.files + ' fichiers)');
        window.__aeExtLast = r;
      }).catch(function (e) {
        if (window.showToast) showToast('« ' + f.name + ' » : ' + e.message);
        window.__aeExtLast = { error: e.message };
      }).then(function () { inp.remove(); });
    });
    inp.click();
  }

  window.SMAEExt = {
    openFile: openFile,
    loadArchive: loadArchive,
    close: closeById,
    closeAll: function () { _panels.slice().forEach(function (p) { closeById(p.id); }); },
    openCount: function () { return _panels.length; },
    setTitle: function (id, t) { _panels.forEach(function (p) { if (p.id === id) p.title.textContent = t; }); }
  };
})();
