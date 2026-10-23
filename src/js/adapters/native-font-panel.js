/* G41 Document font resources. DOM state only; the injected resource port owns
 * all import/read validation, receipts, native history and parsed-font lifetime.
 * This panel never creates text, Paper objects or a document/font-name binding. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeFontPanel = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  var bundled = ['Roboto-Regular', 'Roboto-Bold', 'DejaVuSans-Regular', 'DejaVuSans-Bold',
    'DejaVuSerif-Regular', 'DejaVuSerif-Bold', 'Manrope-Regular', 'Inter-Regular'];
  function owner(a, b) {
    return !!a && !!b && ['instanceId', 'documentId', 'lifecycleGeneration'].every(function (k) { return a[k] === b[k]; });
  }
  function pin(a, b) {
    return owner(a, b) && a.contentRevision === b.contentRevision && a.documentSnapshotId === b.documentSnapshotId;
  }
  function create(container, ports) {
    var doc = ports && ports.document;
    if (!container || !doc || typeof ports.readInfo !== 'function' || typeof ports.resources !== 'function')
      throw new TypeError('Native font panel ports are unavailable');
    var retired = false, opened = false, epoch = 0, serial = 0, listing = 0;
    var identity = null, busy = false, listBusy = false, unknown = false, selected = null, rows = [], abort = null;
    function node(tag, text, id) {
      var n = doc.createElement(tag); if (text !== undefined) n.textContent = text;
      if (id) n.id = id; return n;
    }
    function button(text, id, action) {
      var n = node('button', text, id); n.type = 'button'; n.className = 'pbtn';
      n.addEventListener('click', action); return n;
    }
    var section = node('section', undefined, 'native-font-resources');
    section.className = 'psec';
    var toggle = button('Fonts', 'native-fonts-toggle', function () {
      if (!alive()) return; opened = !opened; body.hidden = !opened;
      toggle.setAttribute('aria-expanded', String(opened));
      if (opened) return reload();
    });
    toggle.setAttribute('aria-expanded', 'false');
    var body = node('div', undefined, 'native-fonts-panel'); body.hidden = true;
    body.className = 'pbdy';
    body.setAttribute('role', 'region'); body.setAttribute('aria-label', 'Document fonts');
    var status = node('div', 'Open Fonts to inspect this document.', 'native-fonts-status');
    status.setAttribute('role', 'status'); status.setAttribute('aria-live', 'polite');
    var list = node('div', undefined, 'native-fonts-list'); list.setAttribute('role', 'list');
    var loadButton = button('Load selected resource', 'native-fonts-load', function () {
      var row = rows.find(function (r) { return r.assetId === selected; });
      if (row && row.available) return loadFont(row.assetId);
    });
    var refreshButton = button('Refresh', 'native-fonts-refresh', function () { return reload(); });
    var bundledSelect = node('select', undefined, 'native-fonts-bundled');
    bundledSelect.className = 'psel';
    bundledSelect.setAttribute('aria-label', 'Bundled font');
    bundled.forEach(function (key) { var option = node('option', key); option.value = key; bundledSelect.appendChild(option); });
    bundledSelect.value = bundled[0];
    var bundledButton = button('Import bundled font', 'native-fonts-import-bundled', function () {
      if (!bundled.includes(bundledSelect.value)) return;
      return importFont({kind: 'bundled', fontKey: bundledSelect.value});
    });
    var family = node('input', undefined, 'native-fonts-google-family'); family.type = 'text'; family.maxLength = 255;
    family.className = 'pi'; family.style.maxWidth = '100%';
    family.placeholder = 'Google Fonts family'; family.setAttribute('aria-label', 'Google Fonts family');
    var weight = node('select', undefined, 'native-fonts-google-weight'); weight.setAttribute('aria-label', 'Google font weight');
    weight.className = 'psel';
    [['400', 'Regular'], ['700', 'Bold']].forEach(function (pair) {
      var option = node('option', pair[1]); option.value = pair[0]; weight.appendChild(option);
    });
    weight.value = '400';
    var googleButton = button('Import Google font', 'native-fonts-import-google', function () {
      var value = (family.value || '').trim();
      if (!value || !['400', '700'].includes(weight.value)) { status.textContent = 'Enter a Google Fonts family.'; return; }
      return importFont({kind: 'google', family: value, weight: Number(weight.value), italic: false});
    });
    var useNotice = node('div', 'Document font resources.');
    [status, refreshButton, list, loadButton, bundledSelect, bundledButton, family, weight, googleButton, useNotice]
      .forEach(function (n) { body.appendChild(n); });
    section.appendChild(toggle); section.appendChild(body); container.appendChild(section);
    function alive() { return !retired && section.parentNode === container; }
    function info() {
      try {
        var p = ports.readInfo();
        return p && ['instanceId', 'documentId', 'documentSnapshotId'].every(function (k) { return typeof p[k] === 'string' && p[k]; }) &&
          Number.isSafeInteger(p.lifecycleGeneration) && p.lifecycleGeneration >= 1 && Number.isSafeInteger(p.contentRevision) && p.contentRevision >= 0 ? Object.assign({}, p) : null;
      } catch (_) { return null; }
    }
    function resources() {
      try {
        var r = ports.resources();
        return r && ['list', 'load', 'importSource'].every(function (k) { return typeof r[k] === 'function'; }) ? r : null;
      } catch (_) { return null; }
    }
    function controls() {
      var ready = alive() && !!identity && !!resources();
      [bundledSelect, family, weight].forEach(function (n) { n.disabled = !ready || busy || listBusy || unknown; });
      [bundledButton, googleButton].forEach(function (n) { n.disabled = !ready || busy || listBusy || unknown; });
      refreshButton.disabled = !ready || busy;
      loadButton.disabled = !ready || busy || listBusy || !rows.some(function (r) { return r.assetId === selected && r.available; });
      renderRows();
    }
    function sync() {
      var next = info(), changed = !pin(identity, next);
      if (!owner(identity, next)) {
        epoch++; serial++; listing++; if (abort) abort.abort(); abort = null;
        busy = false; listBusy = false; unknown = false; selected = null; rows = [];
        status.textContent = next ? 'Document changed. Refresh to inspect its fonts.' : 'Native font document is unavailable.';
      }
      identity = next; return changed;
    }
    function token() { return {pin: Object.assign({}, identity), epoch: epoch, serial: serial}; }
    function current(t, revision) {
      return alive() && t.epoch === epoch && t.serial === serial && (revision ? pin(t.pin, info()) : owner(t.pin, info()));
    }
    function renderRows() {
      list.replaceChildren();
      rows.forEach(function (row) {
        var item = node('div'); item.setAttribute('role', 'listitem'); item.setAttribute('data-asset-id', row.assetId);
        item.style.overflowWrap = 'anywhere';
        item.appendChild(node('span', row.name + ' — ' + row.assetId));
        if (!row.available) item.appendChild(node('span', ' Unavailable: ' + row.reason));
        var choose = button('Select', null, function () {
          if (!alive()) return; var previous = identity; sync();
          if (!pin(previous, identity)) { controls(); if (opened && !busy) return reload(); return; }
          if (busy || listBusy || !rows.includes(row)) { controls(); return; }
          selected = row.assetId; status.textContent = 'Selected resource: ' + selected;
          if (unknown) status.textContent += ' Import outcome unknown; do not repeat the import.'; controls();
        });
        choose.setAttribute('data-select-asset-id', row.assetId); choose.setAttribute('aria-pressed', String(selected === row.assetId));
        choose.disabled = busy || listBusy || !identity;
        item.appendChild(choose); list.appendChild(item);
      });
    }
    async function reload() {
      if (!alive()) return; sync();
      var api = resources();
      if (!identity || !api) { status.textContent = 'Native font resources are unavailable.'; controls(); return; }
      var t = token(), ticket = ++listing; listBusy = true; controls();
      status.textContent = 'Reading document fonts…';
      try {
        var result = await api.list();
        if (!current(t, true) || ticket !== listing) return;
        var seen = new Set();
        if (!Array.isArray(result) || result.some(function (r) {
          if (!r || typeof r.assetId !== 'string' || !r.assetId || typeof r.name !== 'string' ||
            typeof r.available !== 'boolean' || !r.available && typeof r.reason !== 'string' || seen.has(r.assetId)) return true;
          seen.add(r.assetId); return false;
        })) throw new Error('Native font catalog is invalid');
        rows = result.map(function (r) { return Object.freeze(Object.assign({}, r)); });
        if (selected !== null && !seen.has(selected)) selected = null;
        status.textContent = rows.length ? rows.length + ' font resource(s) in this document.' : 'No fonts in this document.';
        if (unknown) status.textContent += ' Import outcome unknown; do not repeat the import.';
      } catch (error) {
        if (current(t, true) && ticket === listing) { rows = []; status.textContent = error.message || 'Font refresh failed.'; }
      } finally {
        if (alive() && ticket === listing) { listBusy = false; controls(); }
      }
    }
    async function perform(action, importing) {
      if (!alive()) return; var expected = identity; sync();
      if (!pin(expected, identity)) { controls(); if (opened && !busy) return reload(); return; }
      var api = resources();
      if (!identity || !api || busy || listBusy || importing && unknown) { controls(); return; }
      busy = true; serial++; var t = token();
      abort = typeof AbortController === 'function' ? new AbortController() : null;
      var signal = abort && abort.signal; status.textContent = importing ? 'Importing font resource…' : 'Loading font resource…'; controls();
      try {
        var value = await action(api, signal);
        if (!current(t, !importing)) return;
        if (importing) {
          if (!value || typeof value.assetId !== 'string' || !value.assetId) throw new Error('Imported font identity is unavailable');
          selected = value.assetId;
        }
        status.textContent = importing ? 'Imported resource: ' + selected : 'Loaded resource: ' + selected;
        if (unknown) status.textContent += ' Import outcome unknown; do not repeat the import.';
      } catch (error) {
        if (current(t, false)) {
          if (error.code === 'font_import_indeterminate') unknown = true;
          status.textContent = unknown ? 'Import outcome unknown. Refresh the catalog; do not repeat the import.' : error.message || 'Font resource operation failed.';
        }
      } finally {
        if (alive() && t.epoch === epoch && t.serial === serial) {
          busy = false; abort = null; var message = status.textContent;
          sync(); if (opened && importing && !unknown) { await reload(); if (owner(t.pin, info())) status.textContent = message; }
          controls();
        }
      }
    }
    function importFont(source) {
      return perform(function (api, signal) { return api.importSource(Object.freeze(source), signal); }, true);
    }
    function loadFont(assetId) {
      return perform(function (api) {
        if (!rows.some(function (r) { return r.assetId === assetId && r.available; })) throw new Error('Font selection changed');
        selected = assetId; return api.load(assetId);
      }, false);
    }
    function refresh() {
      if (!alive()) return; var changed = sync(); if (!changed) return; controls();
      if (opened && changed && !busy) return reload();
    }
    function retire() {
      if (retired) return; retired = true; epoch++; serial++; listing++;
      if (abort) abort.abort(); rows = []; selected = null; busy = false; listBusy = false; section.remove();
    }
    refresh();
    return Object.freeze({refresh: refresh, retire: retire});
  }
  return Object.freeze({create: create});
}));
