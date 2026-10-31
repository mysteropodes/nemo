/* The Elements panel's tree on a native object project (#2376 G26): the baseline SMMotion.buildShapeTree (1065e9d8 motion.js) rebuilt over
 * the native hit-test listing of the active layer at the playhead. Pure: it reads the listed items (in paint order, bottom first) and returns the
 * rows - nothing is read from the session, the DOM or the legacy state.layers projection, and nothing is written.
 *
 * Baseline rules kept: a group is emitted once, at the position of its first member; a shape's group chain nests it (the listing's groupPath is
 * outermost first); both levels are listed front-first; a ribbon's linked fill is folded into its row (its swatch takes the fill colour);
 * shapes are numbered by DRAW order, independent of the display order and of what is expanded. The native record carries no name of its own, so
 * a shape is always the automatic "Shape N" / "Stroke N" / "Image N" label; a group has its stored name or the automatic one. */
(function (root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeShapeTreeModel = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';

  // labels: { shape, stroke, group, text, image } - the app's own wording (autoNameShape, autoNameStroke, autoNameGroup, autoNameText) and 'Image'.
  function labelOf(entry, number, labels) {
    if (entry.role === 'raster') return labels.image + ' ' + number;
    if (entry.role === 'text') return labels.text + ' ' + number;
    return (entry.fill ? labels.shape : labels.stroke) + ' ' + number;
  }
  function visible(items) {
    return items.filter((item) => !(item.role === 'fill' && item.owner));
  }
  function swatchOf(item, linked) {
    const style = item.style || {};
    const fill = linked && linked.style && linked.style.fill ? linked.style.fill : style.fill || null;
    const real = typeof item.strokeWidth === 'number' && item.strokeWidth > 0 && !!style.stroke;
    return { fill, stroke: real ? style.stroke : null };
  }
  // Every row is a fresh object: { type: 'shape', strokeId, role, label, index, fill, stroke } or
  // { type: 'group', gid, name, named, children, memberIds }.
  function build(items, labels) {
    const list = Array.isArray(items) ? items : [];
    const linked = new Map();
    list.forEach((item) => { if (item.role === 'fill' && item.owner) linked.set(item.owner, item); });
    const flat = visible(list).map((item, index) => {
      const swatch = swatchOf(item, linked.get(item.stableTarget.strokeId));
      const entry = { type: 'shape', strokeId: item.stableTarget.strokeId, role: item.role, index, fill: swatch.fill, stroke: swatch.stroke };
      entry.label = labelOf({ role: item.role, fill: swatch.fill }, index + 1, labels);
      return { entry, path: Array.isArray(item.groupPath) ? item.groupPath : [] };
    });
    const groups = new Map(), top = [];
    flat.forEach((one) => {
      let siblings = top;
      one.path.forEach((g) => {
        let node = groups.get(g.id);
        if (!node) {
          node = { type: 'group', gid: g.id, name: g.name || labels.group, named: !!g.name, children: [], memberIds: [] };
          groups.set(g.id, node);
          siblings.push(node);
        }
        node.memberIds.push(one.entry.strokeId);
        siblings = node.children;
      });
      siblings.push(one.entry);
    });
    const front = (rows) => { rows.reverse(); rows.forEach((row) => { if (row.type === 'group') front(row.children); }); return rows; };
    const rows = front(top);
    rows.flat = flat.map((one) => one.entry);
    return rows;
  }
  // A compact signature of a tree, so an identical listing at another frame repaints nothing.
  function signature(rows) {
    const part = (row) => row.type === 'group'
      ? 'g(' + row.gid + '|' + row.name + '|' + row.children.map(part).join(',') + ')'
      : 's(' + row.strokeId + '|' + row.label + '|' + (row.fill || '') + '|' + (row.stroke || '') + '|' + row.role + ')';
    return rows.map(part).join(';');
  }
  // Every group node under `rows` (any depth), by id.
  function groupsOf(rows) {
    const out = new Map();
    (function walk(list) { list.forEach((row) => { if (row.type === 'group') { out.set(row.gid, row); walk(row.children); } }); }(rows));
    return out;
  }
  return Object.freeze({ build, signature, groupsOf });
}));
