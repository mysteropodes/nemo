// Bind the application service to existing document, history and display ports.
(function (root) {
  'use strict';
  var domain = NemoOpacityDomain;
  function newId() { return root.crypto.randomUUID(); }
  function ensureIds() { root.state.layers.forEach(function (layer) { root.SMMotion.ensureLayerUid(layer); }); }
  function read(layer, frame) { return root.SMMotion.valueAtFrame(layer, 'opacity', frame); }
  function refresh(layer) {
    var index = root.state.layers.indexOf(layer);
    if (index >= 0 && root.state.appMode === 'motion') root.SM.setActiveLayer(index, true);
    if (root.renderLayerList) root.renderLayerList();
    if (root.renderTimeline) root.renderTimeline();
    if (root.updateUI) root.updateUI();
    if (root.SMEngineBridge) root.SMEngineBridge.renderNow();
  }
  function write(operation, layer, payload) {
    var frame = root.state.currentFrame;
    if (operation === 'property.set') return domain.setValue(layer, [payload.value], frame);
    if (operation === 'property.key.set') return domain.setKeyAtFrame(layer, payload.frame, [payload.value], payload.curvePoints);
    if (operation === 'property.key.remove') return domain.removeKeyAtFrame(layer, payload.frame);
    if (operation === 'property.animation.set') return domain.setAnimated(layer, payload.animated, frame, read(layer, frame));
    throw new Error('Unsupported opacity mutation');
  }
  function history(action) {
    var stack = root.state[action === 'undo' ? 'undoStack' : 'redoStack'];
    var count = stack.length;
    root[action]();
    return stack.length < count;
  }
  var app = NemoOpacityApplicationCore.create({
    newId: newId, state: function () { return root.state; }, valueAtFrame: read, write: write,
    context: function () { return JSON.stringify([root.state.activeSymbolId || null, root.state.activeMontageViewId || null]); },
    canMutate: function () { return !root._scrubLiveActive; },
    snapshot: function () { return { layers: root.state.layers.map(function (layer) {
      return { id: layer.layerUid, name: layer.name || '', opacity: read(layer, root.state.currentFrame)[0] };
    }), frame: root.state.currentFrame, totalFrames: root.state.totalFrames }; },
    history: { checkpoint: function () { root.pushUndo(); },
      undo: function () { return history('undo'); }, redo: function () { return history('redo'); } },
    afterMutation: refresh
  });
  ensureIds();
  root.NemoApplication = { handle: app.handle, setInstanceId: app.setInstanceId };
  root.NemoOpacityApplication = {
    meta: app.meta,
    historyChanged: app.historyChanged,
    documentChanged: function () {
      root.state.undoStack = []; root.state.undoLabels = [];
      root.state.redoStack = []; root.state.redoLabels = [];
      ensureIds(); app.documentChanged();
      if (root.renderHistoryPanelIfOpen) root.renderHistoryPanelIfOpen();
    },
    legacy: function (kind, holder, values, frame, curvePoints) {
      var result;
      if (kind === 'set') result = domain.setValue(holder, values, root.state.currentFrame);
      else if (kind === 'key-current') result = domain.setKeyAtFrame(holder, root.state.currentFrame, values);
      else if (kind === 'key-frame') result = domain.setKeyAtFrame(holder, frame, values, curvePoints);
      else if (kind === 'remove') result = domain.removeKeyAtFrame(holder, frame);
      else if (kind === 'animated') result = domain.setAnimated(holder, values, root.state.currentFrame, read(holder, root.state.currentFrame));
      else throw new Error('Unsupported opacity edit');
      app.changed();
      // Existing UI handlers own their gesture checkpoint and repaint schedule.
      return result;
    }
  };
  // These classic-script functions are global bindings. Wrapping them here
  // covers keyboard/UI history and API history without a second history stack.
  [['pushUndoLayers', 'undoStack'], ['pushUndoActiveFrame', 'undoStack'], ['undo', 'undoStack'], ['redo', 'redoStack']].forEach(function (entry) {
    var original = root[entry[0]];
    if (typeof original !== 'function') return;
    root[entry[0]] = function () {
      var stack = root.state[entry[1]], length = stack.length, last = stack[length - 1];
      var result = original.apply(this, arguments);
      stack = root.state[entry[1]];
      if (stack.length !== length || stack[stack.length - 1] !== last) app.historyChanged();
      return result;
    };
  });
})(window);
