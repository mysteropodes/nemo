'use strict';
// These desktop compositions use fake DOMs but the real presentation adapter.
const Underlay = require('../../src/js/adapters/native-viewport-underlay.js');
function install(root) {
  if (!root.document) throw new Error('Desktop fixture requires its existing document');
  const element = root.document.documentElement || (root.document.documentElement = {});
  if (!element.classList) {
    const classes = new Set();
    element.classList = { contains: key => classes.has(key),
      add: key => classes.add(key), remove: key => classes.delete(key) };
  }
  if (!element.style) element.style = {};
  if (typeof element.style.getPropertyValue !== 'function') {
    const priorities = new Map(), style = element.style;
    Object.assign(style, {
      getPropertyValue: key => style[key] || '', getPropertyPriority: key => priorities.get(key) || '',
      setProperty(key, value, priority = '') { style[key] = String(value); priorities.set(key, priority); },
      removeProperty(key) { const old = style[key] || ''; delete style[key]; priorities.delete(key); return old; }
    });
  }
  if (element.clientWidth === undefined) element.clientWidth = 1280;
  if (element.clientHeight === undefined) element.clientHeight = 900;
  root.NemoNativeViewportUnderlay = Underlay;
}
module.exports = { install };
