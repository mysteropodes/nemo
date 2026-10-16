/* Presentation-only WKWebView opening; native code owns the actual viewport. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeViewportUnderlay = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  var className = 'nemo-native-underlay';
  var properties = ['left', 'top', 'right', 'bottom'].map(function (edge) {
    return '--nemo-native-' + edge;
  });
  function identity(value) {
    if (!value || !['instanceId', 'documentId'].every(function (key) {
      return typeof value[key] === 'string' && /^[A-Za-z0-9][A-Za-z0-9._:/-]{0,127}$/.test(value[key]);
    }) || !Number.isSafeInteger(value.lifecycleGeneration) || value.lifecycleGeneration < 1) {
      throw new TypeError('Native underlay requires a confirmed presentation owner');
    }
    return { instanceId: value.instanceId, documentId: value.documentId,
      lifecycleGeneration: value.lifecycleGeneration };
  }
  function same(a, b) {
    return !!a && !!b && ['instanceId', 'documentId', 'lifecycleGeneration'].every(function (key) {
      return a[key] === b[key];
    });
  }
  function create(ports) {
    var element = ports.document.documentElement, style = element.style;
    if (element.classList.contains(className)) throw new Error('Native underlay already has an owner');
    var previous = properties.map(function (property) {
      return { property: property, value: style.getPropertyValue(property),
        priority: style.getPropertyPriority(property) };
    });
    var owner = null, disposed = false;
    function clear(expected) {
      if (expected !== undefined && !same(owner, expected)) return false;
      element.classList.remove(className);
      previous.forEach(function (item) {
        if (item.value) style.setProperty(item.property, item.value, item.priority);
        else style.removeProperty(item.property);
      });
      owner = null;
      return true;
    }
    function measure() {
      var rect = ports.measure(), width = element.clientWidth, height = element.clientHeight;
      if (!rect || ![rect.left, rect.top, rect.width, rect.height, width, height].every(Number.isFinite) ||
          rect.width <= 0 || rect.height <= 0 || width <= 0 || height <= 0) {
        throw new Error('Native underlay canvas bounds are unavailable');
      }
      var edges = [Math.max(0, rect.left), Math.max(0, rect.top),
        Math.min(width, rect.left + rect.width), Math.min(height, rect.top + rect.height)];
      if (!edges.every(Number.isFinite) || edges[0] >= edges[2] || edges[1] >= edges[3]) {
        throw new Error('Native underlay canvas is outside the viewport');
      }
      return edges;
    }
    function present(value) {
      if (disposed) throw new Error('Native underlay is disposed');
      var next;
      try {
        next = identity(value);
        var edges = measure();
        properties.forEach(function (property, index) { style.setProperty(property, edges[index] + 'px'); });
        owner = next;
        element.classList.add(className);
        return true;
      } catch (error) { clear(); throw error; }
    }
    return Object.freeze({ present: present,
      refresh: function (expected) { return !disposed && same(owner, expected) ? present(expected) : false; },
      clear: clear,
      dispose: function () { if (!disposed) { clear(); disposed = true; } }
    });
  }
  function desktop(root) {
    var underlay = null, bounds = null;
    return Object.freeze({
      projectionReady: function (info) {
        return !!root.NemoNativeObjectProjectFile && root.NemoNativeObjectProjectFile.projectionMatches(root, info);
      },
      frame: function () { return root.state.currentFrame; },
      mapping: function (info) {
        var rect = root.document.getElementById('drawing-canvas').getBoundingClientRect();
        var dpr = root.devicePixelRatio || 1;
        return { cssBounds: { x: rect.left, y: rect.top, width: rect.width, height: rect.height },
          physicalExtent: { width: Math.max(1, Math.round(rect.width * dpr)), height: Math.max(1, Math.round(rect.height * dpr)) },
          compositionExtent: { width: info.canvasW, height: info.canvasH }, reportedDpr: dpr };
      },
      surfaceChanged: function (presentation, mapping) {
        if (!presentation) { if (underlay) underlay.clear(); bounds = null; return; }
        if (!underlay) underlay = create({ document: root.document, measure: function () { return bounds; } });
        bounds = { left: mapping.x, top: mapping.y, width: mapping.width, height: mapping.height };
        underlay.present(presentation);
      }
    });
  }
  return Object.freeze({ create: create, desktop: desktop });
}));
