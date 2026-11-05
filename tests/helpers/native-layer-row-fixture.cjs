'use strict';
// Shared fixture of the layer-row write tests: a fake native project session that applies the small part of the engine's
// semantics the tests read back (rename, color, reorder, delete, duplicate, folder collapse), records every request it is
// sent, and a minimal event-capable DOM (the repository's fake DOM has no listeners, geometry or elementFromPoint).
class El {
  constructor(tag) { this.tagName = String(tag).toUpperCase(); this.children = []; this.parentNode = null; this.dataset = {}; this.style = {}; this.className = ''; this.hidden = false; this.title = ''; this.textContent = ''; this.listeners = {}; this.box = null; this.value = ''; this.focused = false; }
  appendChild(n) { if (n.parentNode) n.parentNode.removeChild(n); this.children.push(n); n.parentNode = this; return n; }
  removeChild(n) { const at = this.children.indexOf(n); if (at >= 0) this.children.splice(at, 1); n.parentNode = null; return n; }
  insertBefore(n, ref) { if (n.parentNode) n.parentNode.removeChild(n); const at = ref ? this.children.indexOf(ref) : -1; if (at < 0) this.children.push(n); else this.children.splice(at, 0, n); n.parentNode = this; return n; }
  replaceChildren(...k) { this.children.forEach((c) => { c.parentNode = null; }); this.children = []; k.forEach((c) => this.appendChild(c)); }
  remove() { if (this.parentNode) this.parentNode.removeChild(this); }
  get nextSibling() { const c = this.parentNode && this.parentNode.children; return c ? c[c.indexOf(this) + 1] || null : null; }
  setAttribute() {}
  addEventListener(type, fn) { (this.listeners[type] = this.listeners[type] || []).push(fn); }
  removeEventListener(type, fn) { this.listeners[type] = (this.listeners[type] || []).filter((f) => f !== fn); }
  // Dispatches like the DOM: the target first, then every ancestor, until a listener stops the propagation.
  fire(type, event) {
    const e = Object.assign({ type, target: this, preventDefault() { this.prevented = true; }, stopPropagation() { this.stopped = true; } }, event);
    for (let node = this; node && !e.stopped; node = node.parentNode) (node.listeners[type] || []).slice().forEach((f) => f(e));
    return e;
  }
  focus() { this.focused = true; } select() {}
  getBoundingClientRect() { const b = this.box || { left: 0, top: 0, width: 100, height: 22 }; return { left: b.left, top: b.top, width: b.width, height: b.height, right: b.left + b.width, bottom: b.top + b.height }; }
}
function makeHost() {
  const host = new El('window'); const list = new El('div'); list.box = { left: 0, top: 0, width: 200, height: 400 };
  const body = new El('body');
  const doc = new El('document');
  doc.createElement = (t) => new El(t); doc.body = body; doc.getElementById = (id) => (id === 'layer-list' ? list : null);
  doc.elementFromPoint = (x, y) => list.children.find((r) => { const b = r.box; return b && y >= b.top && y < b.top + b.height; }) || null;
  host.document = doc; host.toasts = []; host.showToast = (t) => host.toasts.push(t); host.menus = [];
  host.showContextMenu = (x, y, items) => host.menus.push(items); host.state = { activeLayerIdx: 0, currentFrame: 3 }; host._layerSel = [0];
  return { host, list, doc, body };
}
const PIN = ['instanceId', 'documentId', 'lifecycleGeneration', 'contentRevision', 'documentSnapshotId'];
function session(layers, options = {}) {
  const state = { instanceId: 'i', documentId: 'd', lifecycleGeneration: 1, contentRevision: 7, documentSnapshotId: 'snap-7', totalFrames: 24, layers: layers.map((l) => ({ ...l })) };
  const s = { requests: [], serial: 0, active: true, state };
  const reply = (request, fields) => Object.assign({ apiVersion: 2, requestId: request.requestId, instanceId: request.instanceId, documentId: request.documentId }, fields);
  function apply(request) {
    const p = request.payload, L = state.layers, at = (uid) => L.findIndex((l) => l.layerUid === uid);
    if (options.refuse) return { code: options.refuse };
    let result = { applied: true, historyEntriesAdded: 1 };
    switch (p.command) {
      case 'object.layer.rename': L[at(p.layerUid)].name = p.name; break;
      case 'object.layer.color.set': (p.selected && p.selected.length > 1 && p.selected.includes(p.layerUid) ? p.selected : [p.layerUid]).forEach((u) => { L[at(u)].color = p.color; }); break;
      case 'object.layer.compositing.edit': L[at(p.layerUid)].folderCollapsed = p.collapsed; break;
      case 'object.layer.delete': state.layers = L.filter((l) => !p.selection.selected.includes(l.layerUid)); break;
      case 'object.layer.duplicate': { const c = { ...L[at(p.layerUid)], layerUid: 'copy-' + s.serial }; L.splice(at(p.layerUid) + 1, 0, c); result = { applied: true, historyEntriesAdded: 1, layerUid: c.layerUid }; break; }
      case 'object.layer.reorder': {
        const moved = L.filter((l) => p.layerUids.includes(l.layerUid)), rest = L.filter((l) => !p.layerUids.includes(l.layerUid));
        rest.splice(L.slice(0, p.gap).filter((l) => !p.layerUids.includes(l.layerUid)).length, 0, ...moved); state.layers = rest; break;
      }
      case 'object.layer.split': break;
      default: return { code: 'invalid_request' };
    }
    return { result };
  }
  Object.assign(s, {
    isActive: () => s.active, blocksLegacy: () => true, info: () => state,
    requestId: (label) => 'req-' + label + '-' + (++s.serial),
    runResourceCommand: async (action) => action({
      dispatch: async (request) => {
        s.requests.push(JSON.parse(JSON.stringify(request)));
        if (request.expectedRevision !== state.contentRevision) return reply(request, { contentRevision: state.contentRevision, ok: false, error: { code: 'stale_revision' } });
        const out = apply(request);
        if (out.code) return reply(request, { contentRevision: state.contentRevision, ok: false, error: { code: out.code } });
        state.contentRevision += 1; state.documentSnapshotId = 'snap-' + state.contentRevision;
        return reply(request, { contentRevision: state.contentRevision, ok: true, result: out.result });
      },
      reconcile: async () => state,
    }),
  });
  return s;
}
module.exports = { El, makeHost, session, PIN };
