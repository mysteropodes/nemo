/* #2256 Q2: the Path (Follow Path) and Time (Link time) rows of the native Motion inspector, for the single active layer.
 * Baseline: renderFollowPathRow / renderTimeLinkRow of src/js/motion.js @1065e9d8 (label, a pill showing the target layer name or
 * the dash, click = the picker menu, right-click = clear, and the Align button next to a followed path). Every write is the
 * native edit of src/js/adapters/native-layer-parent.js (command.document.object.layer.followPath.set / timeLink.set):
 * no legacy setLayerFollowPath / setLayerTimeLink, no fallback writer. The rows hold no state of their own; refresh() re-reads the
 * pinned project rows. A layer outside the scene root shows the rows and the owner's invalid_request refusal on use.
 * D37: a third row, Lock keyframes on, shows the layer's stored key-lock mode and opens the shared lock menu; a pick is the native
 * layer-policy edit (flag keyLock) against the pin the menu was built from, so a stale menu cannot commit. */
(function (root, factory) {
  const commonjs = typeof module === 'object' && module.exports;
  const api = factory(commonjs ? require('./native-layer-parent.js') : null);
  if (commonjs) module.exports = api;
  else root.NemoNativeMotionLayerLinkRows = api;
}(typeof globalThis === 'object' ? globalThis : this, function (ParentModule) {
  'use strict';
  const ROOT_CONTEXT = 'scene-root', BARE = ['folder', 'null', 'effect'];
  const tr = (host, key, fallback) => { const value = host.SM && typeof host.SM.t === 'function' ? host.SM.t(key) : null; return typeof value === 'string' && value && value !== key ? value : fallback; };
  const make = (host, tag, className, text) => { const node = host.document.createElement(tag); if (className) node.className = className; if (text !== undefined) node.textContent = text; return node; };

  // The rows for `target` ({contextId, layerUid}) or null when the layer carries none (a folder, Null or effect layer, an element, an unknown layer).
  function build(host, target) {
    const Parent = ParentModule || host.NemoNativeLayerParent;
    if (!host || !host.document || !Parent || !target || target.strokeId !== undefined || typeof target.layerUid !== 'string') return null;
    const adapter = Parent.desktop(host);
    if (!adapter) return null;
    const inRoot = target.contextId === ROOT_CONTEXT, index = () => adapter.indexOf(target.layerUid);
    if (inRoot && index() < 0) return null;
    if (inRoot && BARE.includes(adapter.rowKind(index()))) return null;
    const nameOfUid = (uid) => { const at = adapter.indexOf(uid); return at < 0 ? null : adapter.nameOf(at); };
    const node = make(host, 'div', 'native-motion-layer-link-rows');
    const refused = (kind) => { if (typeof host.showToast === 'function') host.showToast(Parent.linkRefusalText(kind, { code: 'invalid_request' })); };
    let disposed = false;
    const toast = (message) => { if (typeof host.showToast === 'function') host.showToast(message); };
    const menu = (anchor, items) => {
      if (typeof host.showContextMenu !== 'function') return;
      const box = anchor.getBoundingClientRect();
      host.showContextMenu(box.left, box.bottom + 2, items);
    };

    function pathRow() {
      const row = make(host, 'div', 'lrow motion-prop-row'), label = make(host, 'span', '', tr(host, 'fieldFollowPath', 'Path'));
      label.style.minWidth = '70px'; label.title = tr(host, 'titleFollowPathHint', '');
      row.appendChild(label);
      const uid = inRoot ? adapter.followPathUidOf(index()) : null, name = uid ? nameOfUid(uid) : null;
      const pill = make(host, 'div', 'lparent motion-parent-pill' + (name ? '' : ' none')), text = make(host, 'span', 'mp-label', name || '—');
      pill.title = name ? tr(host, 'titleFollowPathCurrentPrefix', 'Follows ') + name + tr(host, 'titleFollowPathCurrentSuffix', '') : tr(host, 'titleFollowPathNone', 'No path');
      pill.appendChild(text);
      pill.addEventListener('click', (e) => {
        e.stopPropagation();
        if (!inRoot) { refused('path'); return; }
        menu(pill, Parent.followPathMenuItems(host, adapter, index(), refresh));
      });
      pill.addEventListener('contextmenu', (e) => {
        e.stopPropagation(); e.preventDefault();
        if (!inRoot) { refused('path'); return; }
        if (adapter.followPathUidOf(index())) Parent.followPathSet(host, adapter, index(), null, refresh);
      });
      row.appendChild(pill);
      if (name) {
        const on = adapter.followPathAlignOf(index()), button = make(host, 'button', 'mp-align-btn' + (on ? ' on' : ''), tr(host, 'btnAlign', 'Align'));
        button.type = 'button'; button.title = tr(host, 'titleAlignRotationHint', '');
        button.addEventListener('click', (e) => {
          e.stopPropagation();
          Parent.followPathAlignSet(host, adapter, index(), !adapter.followPathAlignOf(index()), refresh);
        });
        row.appendChild(button);
      }
      return row;
    }
    function timeRow() {
      const row = make(host, 'div', 'lrow motion-prop-row'), label = make(host, 'span', '', tr(host, 'fieldTimeLink', 'Time'));
      label.style.minWidth = '70px'; label.title = tr(host, 'titleTimeLinkRowHint', '');
      row.appendChild(label);
      const uid = inRoot ? adapter.timeLinkUidOf(index()) : null, name = uid ? nameOfUid(uid) : null;
      const pill = make(host, 'span', 'lparent' + (name ? '' : ' none'), name || (uid ? 'source introuvable' : '—'));
      pill.style.marginLeft = '4px';
      pill.title = name ? tr(host, 'titleTimeLinkNameHint', '') : tr(host, 'titleNoTimeLink', '');
      pill.addEventListener('click', (e) => {
        e.stopPropagation();
        if (!inRoot) { refused('time'); return; }
        menu(pill, Parent.timeLinkMenuItems(host, adapter, index(), refresh));
      });
      pill.addEventListener('contextmenu', (e) => {
        e.stopPropagation(); e.preventDefault();
        if (!inRoot) { refused('time'); return; }
        if (adapter.timeLinkUidOf(index())) Parent.timeLinkSet(host, adapter, index(), null, undefined, undefined, refresh);
      });
      row.appendChild(pill);
      return row;
    }
    function lockRow() {
      const Menu = host.NemoNativeMotionKeyLockMenu; // a sibling module of the Motion properties adapter: reached through the host, not imported
      if (!Menu) return null;
      const row = make(host, 'div', 'lrow motion-prop-row'), label = make(host, 'span', '', Menu.title(host));
      label.style.minWidth = '70px';
      row.appendChild(label);
      const mode = inRoot ? adapter.keyLockOf(index()) : null;
      const pill = make(host, 'span', 'lparent' + (mode ? '' : ' none'), mode ? Menu.modeName(host, mode) : '—');
      pill.style.marginLeft = '4px';
      pill.addEventListener('click', (e) => {
        e.stopPropagation();
        if (!inRoot) { toast('Key lock is only available on scene layers.'); return; }
        // Everything the write needs is captured now: the layer, its mode and the revision the menu shows.
        const uid = adapter.layerUid(index()), current = adapter.keyLockOf(index()), pin = adapter.pinOf();
        menu(pill, Menu.items(host, (m) => m === current, (next) => adapter.setKeyLock(uid, next, pin).then(() => { refresh(); return true; }, () => {
          toast('Key lock was refused or the project changed; open the menu again.');
          return false;
        })));
      });
      row.appendChild(pill);
      return row;
    }
    function refresh() {
      if (disposed) return false;
      if (inRoot && (index() < 0 || BARE.includes(adapter.rowKind(index())))) { node.replaceChildren(); return false; }
      node.replaceChildren(...[pathRow(), timeRow(), lockRow()].filter(Boolean));
      return true;
    }
    refresh();
    return Object.freeze({ node, refresh, dispose() { disposed = true; node.replaceChildren(); } });
  }

  return Object.freeze({ build });
}));
