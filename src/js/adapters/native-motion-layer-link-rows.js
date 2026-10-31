/* #2256 Q2: the Path (Follow Path) and Time (Link time) rows of the native Motion inspector, for the single active layer.
 * Baseline: renderFollowPathRow / renderTimeLinkRow of src/js/motion.js @1065e9d8 (label, a pill showing the target layer name or
 * the dash, click = the picker menu, right-click = clear, and the Align button next to a followed path). Every write is the
 * native edit of src/js/adapters/native-layer-parent.js (command.document.object.layer.followPath.set / timeLink.set):
 * no legacy setLayerFollowPath / setLayerTimeLink, no fallback writer. The rows hold no state of their own; refresh() re-reads the
 * pinned project rows. A layer outside the scene root shows the rows and the owner's invalid_request refusal on use.
 * D37: a third row, Lock keyframes on, shows the layer's stored key-lock mode and opens the shared lock menu; a pick is the native
 * layer-policy edit (flag keyLock) against the pin the menu was built from, so a stale menu cannot commit.
 * D36 (#2521): between Time and Lock, the Matte row of renderMatteRow (pill "source  ·  mode", "+" once a matte exists, one dim
 * "+ Matte" row per additional matte) and its two menus, openMatteConfigMenu / openExtraMatteMenu (@1065e9d8): Matte layer:, Mode:,
 * Remove the matte. Every pick is ONE native compositing edit (matteSource / matteMode / matteClear / matteMoreAdd / Set / Remove) of
 * src/js/adapters/native-object-layer-compositing.js, reached through the host like the lock menu. A menu captures the layer uid and the
 * matte it was built from; the write re-resolves the layer and refuses (toast, nothing sent) when that matte changed meanwhile. */
(function (root, factory) {
  const commonjs = typeof module === 'object' && module.exports;
  const api = factory(commonjs ? require('./native-layer-parent.js') : null);
  if (commonjs) module.exports = api;
  else root.NemoNativeMotionLayerLinkRows = api;
}(typeof globalThis === 'object' ? globalThis : this, function (ParentModule) {
  'use strict';
  const ROOT_CONTEXT = 'scene-root', BARE = ['folder', 'null', 'effect'];
  const tr = (host, key, fallback) => { const value = host.SM && typeof host.SM.t === 'function' ? host.SM.t(key) : null; return typeof value === 'string' && value && value !== key ? value : fallback; };
  const MODES = [['alpha', 'matteAlpha'], ['alphaInverted', 'matteAlphaInverted'], ['luma', 'matteLuma'], ['lumaInverted', 'matteLumaInverted']];
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
    // The compositing adapter of the active native project (null when the host has none), resolved at use time.
    const comp = () => { const C = host.NemoNativeObjectLayerCompositing; try { return C && typeof C.desktop === 'function' ? C.desktop(host) : null; } catch (_) { return null; } };
    const modeName = (mode) => { const hit = MODES.find((m) => m[0] === mode); return hit ? tr(host, hit[1], mode) : String(mode); };
    const popup = (e, anchor, items) => {
      if (typeof host.showContextMenu !== 'function') return;
      if (e && Number.isFinite(e.clientX) && Number.isFinite(e.clientY)) host.showContextMenu(e.clientX, e.clientY, items); else menu(anchor, items);
    };
    const settled = () => { refresh(); if (typeof host.nativeCompositingRefresh === 'function') host.nativeCompositingRefresh(); };
    const failed = () => { toast('Native layer compositing command was refused.'); refresh(); };
    // One matte write for the layer `uid`: re-resolve the layer and re-check that the matte the menu was built from is still the stored one.
    function matteWrite(uid, same, write) {
      const C = comp(), at = adapter.indexOf(uid);
      if (!C || at < 0 || !same(C, at)) { toast(tr(host, 'toastMatteChanged', 'The matte changed; open the menu again.')); refresh(); return; }
      write(C, at).then(settled, failed);
    }
    const entry = (text, action) => ({ label: text, action });
    const sourceItems = (self, chosen, pick) => {
      const items = [{ label: tr(host, 'matteMenuSourceHdr', 'Matte layer:'), disabled: true, action() {} }];
      for (let oi = 0; oi < adapter.count(); oi += 1) {
        const other = adapter.layerUid(oi);
        if (oi === self || !other) continue;
        items.push(entry('  ' + adapter.nameOf(oi) + (chosen(other, oi) ? '  ✓' : ''), () => pick(other)));
      }
      return items;
    };
    const modeItems = (current, pick) => [{ sep: true }, { label: tr(host, 'matteMenuModeHdr', 'Mode:'), disabled: true, action() {} }]
      .concat(MODES.map((m) => entry('  ' + modeName(m[0]) + (current === m[0] ? '  ✓' : ''), () => pick(m[0]))));
    const removeItem = (remove) => [{ sep: true }, { label: tr(host, 'matteMenuRemove', 'Remove the matte'), action: remove }];

    // openMatteConfigMenu: the first matte. Picking a source with no mode lets the owner default to Alpha, as the baseline does.
    function matteMenu(e, anchor, at) {
      const uid = adapter.layerUid(at), C = comp(), mode = C.matteOf(at), source = C.sourceOf(at), on = mode !== 'none';
      const same = (K, i) => K.matteOf(i) === mode && K.sourceOf(i) === source;
      const items = sourceItems(at, (other, oi) => (source ? source === other : on && oi === at + 1), (other) => matteWrite(uid, same, (K, i) => K.setMatteSource(i, other)))
        .concat(modeItems(mode, (next) => matteWrite(uid, same, (K, i) => K.setMatteMode(i, next))));
      popup(e, anchor, on ? items.concat(removeItem(() => matteWrite(uid, same, (K, i) => K.setMatteMode(i, 'none')))) : items);
    }
    // openExtraMatteMenu: the same two lists for one additional matte, plus Remove of just that entry.
    function extraMatteMenu(e, anchor, at, mi, entry) {
      const uid = adapter.layerUid(at);
      const same = (K, i) => { const now = K.mattesMoreOf(i)[mi]; return !!now && now.uid === entry.uid && now.mode === entry.mode; };
      const items = sourceItems(at, (other) => entry.uid === other, (other) => matteWrite(uid, same, (K, i) => K.setMatteMore(i, mi, { source: other })))
        .concat(modeItems(entry.mode, (next) => matteWrite(uid, same, (K, i) => K.setMatteMore(i, mi, { mode: next }))))
        .concat(removeItem(() => matteWrite(uid, same, (K, i) => K.removeMatteMore(i, mi))));
      popup(e, anchor, items);
    }
    // First layer that is neither this one nor already in its matte stack, so "+" lands on something meaningful.
    function firstFreeLayer(C, at) {
      const used = new Set([C.sourceOf(at)].concat(C.mattesMoreOf(at).map((m) => m.uid)).filter(Boolean));
      for (let k = 0; k < adapter.count(); k += 1) { const u = adapter.layerUid(k); if (k !== at && u && !used.has(u)) return u; }
      return null;
    }
    function matteRows() {
      const C = comp();
      if (!C) return [];
      const label = (text, dim) => { const l = make(host, 'span', '', text); l.style.minWidth = '70px'; if (dim) l.style.opacity = '.7'; return l; };
      const at = inRoot ? index() : -1, mode = at >= 0 ? C.matteOf(at) : 'none', on = mode !== 'none';
      let srcAt = at >= 0 && C.sourceOf(at) ? adapter.indexOf(C.sourceOf(at)) : -1;
      if (srcAt < 0 && on && at + 1 < adapter.count()) srcAt = at + 1; // the matte comes from the layer directly above when none is stored
      const row = make(host, 'div', 'lrow motion-prop-row'), pill = make(host, 'div', 'lparent motion-parent-pill' + (on ? '' : ' none'));
      const text = make(host, 'span', 'mp-label', on ? (srcAt >= 0 ? adapter.nameOf(srcAt) : '?') + '  ·  ' + modeName(mode) : '—');
      const name = label(tr(host, 'fieldMatte', 'Matte'));
      name.title = tr(host, 'titleMatteHint', '');
      row.appendChild(name);
      pill.title = tr(host, 'titleMattePickHint', ''); pill.appendChild(text);
      pill.addEventListener('click', (e) => {
        e.preventDefault(); e.stopPropagation();
        if (!inRoot) { toast('Matte is only available on scene layers.'); return; }
        matteMenu(e, pill, index());
      });
      row.appendChild(pill);
      const rows = [row];
      if (!on) return rows;
      const add = make(host, 'button', 'motion-expr-glob motion-matte-add', '+');
      add.type = 'button'; add.title = tr(host, 'titleMatteAddHint', '');
      add.addEventListener('click', (e) => {
        e.preventDefault(); e.stopPropagation();
        const K = comp(), here = index(), pick = K && here >= 0 ? firstFreeLayer(K, here) : null;
        if (pick === null) { toast(tr(host, 'toastNoLayerLeftForMatte', 'No other layer left to use as a matte')); return; }
        matteWrite(adapter.layerUid(here), () => true, (X, i) => X.addMatteMore(i, pick, 'alpha'));
      });
      row.appendChild(add);
      C.mattesMoreOf(at).forEach((entry, mi) => {
        const more = make(host, 'div', 'lrow motion-prop-row'), extra = make(host, 'div', 'lparent motion-parent-pill');
        const at2 = entry.uid ? adapter.indexOf(entry.uid) : -1;
        more.appendChild(label('+ ' + tr(host, 'fieldMatte', 'Matte'), true));
        extra.title = tr(host, 'titleMattePickHint', '');
        extra.appendChild(make(host, 'span', 'mp-label', (at2 >= 0 ? adapter.nameOf(at2) : '?') + '  ·  ' + modeName(entry.mode)));
        extra.addEventListener('click', (e) => { e.preventDefault(); e.stopPropagation(); extraMatteMenu(e, extra, index(), mi, entry); });
        more.appendChild(extra);
        rows.push(more);
      });
      return rows;
    }
    function refresh() {
      if (disposed) return false;
      if (inRoot && (index() < 0 || BARE.includes(adapter.rowKind(index())))) { node.replaceChildren(); return false; }
      node.replaceChildren(...[pathRow(), timeRow()].concat(matteRows(), [lockRow()]).filter(Boolean));
      return true;
    }
    refresh();
    return Object.freeze({ node, refresh, dispose() { disposed = true; node.replaceChildren(); } });
  }

  return Object.freeze({ build });
}));
