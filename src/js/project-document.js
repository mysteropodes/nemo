// Pure project JSON validation plus the deliberately narrow N20 opacity cutover.
(function (root, factory) {
  'use strict';
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else (root.window || root).SMProjectDocument = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';

  var IDENTIFIER = /^[A-Za-z0-9][A-Za-z0-9._:/-]{0,127}$/;
  var LAYER_UID = 'r08_curve_layer';
  var STROKE_ID = 'r08_curve_rect';
  var DEFAULT_CURVE = [[0, 0], [0.25, 0.156], [0.5, 0.5], [0.75, 0.844], [1, 1]];

  function has(value, key) { return Object.prototype.hasOwnProperty.call(value, key); }
  function plain(value) {
    return value !== null && typeof value === 'object' &&
      (Object.getPrototypeOf(value) === Object.prototype || Object.getPrototypeOf(value) === null);
  }
  function exact(value, required, optional, label) {
    if (!plain(value)) throw new TypeError(label + ' must be an exact plain object');
    var allowed = required.concat(optional || []);
    if (required.some(function (key) { return !has(value, key); }) ||
        Object.keys(value).some(function (key) { return allowed.indexOf(key) < 0; })) {
      throw new TypeError(label + ' has missing or unsupported fields');
    }
    return value;
  }
  function exactArray(value, expected, label) {
    if (!Array.isArray(value) || value.length !== expected.length ||
        value.some(function (part, index) { return part !== expected[index]; })) {
      throw new TypeError(label + ' does not match the exact supported value');
    }
  }
  function identifier(value, label) {
    if (typeof value !== 'string' || !IDENTIFIER.test(value)) {
      throw new TypeError(label + ' must be a bounded identifier');
    }
    return value;
  }
  function opacity(value, label) {
    if (!Array.isArray(value) || value.length !== 1 || typeof value[0] !== 'number' ||
        !Number.isFinite(value[0]) || value[0] < 0 || value[0] > 100) {
      throw new TypeError(label + ' is invalid opacity');
    }
    return value;
  }
  function deepFreeze(value) {
    if (value && typeof value === 'object' && !Object.isFrozen(value)) {
      Object.keys(value).forEach(function (key) { deepFreeze(value[key]); });
      Object.freeze(value);
    }
    return value;
  }
  function clone(value) { return JSON.parse(JSON.stringify(value)); }
  function encodedBytes(value) {
    var encoded = JSON.stringify(value);
    if (typeof encoded !== 'string') throw new TypeError('value is not JSON serializable');
    return typeof TextEncoder === 'function'
      ? new TextEncoder().encode(encoded).length
      : unescape(encodeURIComponent(encoded)).length;
  }

  function parse(json) {
    var d = JSON.parse(json);
    if (!d || typeof d !== 'object' || (!d.layers && !d.frames)) throw new Error('Invalid');
    if (!d.layers) d.layers = [{ name: 'Layer 1', visible: true, locked: false, frames: d.frames }];
    // Reject malformed layers and frames before the importer tears down the
    // current document. Older frame-only files retain their existing migration.
    if (!Array.isArray(d.layers) || !d.layers.length) throw new Error('Fichier invalide (layers)');
    d.layers.forEach(function (ld, li) {
      if (!ld || !Array.isArray(ld.frames)) throw new Error('Fichier invalide (calque ' + (li + 1) + ')');
      ld.frames.forEach(function (f, fi) {
        if (!f || !Array.isArray(f.strokes)) throw new Error('Fichier invalide (calque ' + (li + 1) + ', frame ' + (fi + 1) + ')');
      });
    });
    return d;
  }

  function baseName(path) {
    var parts = path.split(/[\\/]/); var f = parts[parts.length - 1] || path;
    return f.replace(/\.json$/i, '');
  }

  function validatePoint(value, expected, label) {
    exact(value, ['point', 'handleIn', 'handleOut'], [], label);
    exactArray(value.point, expected, label + '.point');
    exactArray(value.handleIn, [0, 0], label + '.handleIn');
    exactArray(value.handleOut, [0, 0], label + '.handleOut');
  }

  function validateFrames(frames) {
    if (!Array.isArray(frames) || frames.length !== 120) {
      throw new TypeError('native opacity shell requires exactly 120 stored frame slots');
    }
    frames.forEach(function (frame, index) {
      exact(frame, ['strokes', 'isKeyframe', 'isInterpolated'], [], 'frame[' + index + ']');
      if (!Array.isArray(frame.strokes) || frame.isInterpolated !== false) {
        throw new TypeError('frame[' + index + '] is unsupported');
      }
      if (index !== 0) {
        if (frame.strokes.length !== 0 || frame.isKeyframe !== false) {
          throw new TypeError('only the characterized frame-zero geometry is supported');
        }
        return;
      }
      if (frame.isKeyframe !== true || frame.strokes.length !== 1) {
        throw new TypeError('frame zero must contain the exact characterized geometry');
      }
      var stroke = frame.strokes[0];
      exact(stroke, [
        'segments', 'closed', 'strokeColor', 'hasRealStroke', 'strokeWidth', 'strokeCap',
        'strokeJoin', 'miterLimit', 'fillColor', 'opacity', 'dashOffset', 'strokeId',
      ], [], 'frame[0].stroke');
      if (!Array.isArray(stroke.segments) || stroke.segments.length !== 4) {
        throw new TypeError('characterized rectangle requires four segments');
      }
      [[20, 80], [20, 60], [40, 60], [40, 80]].forEach(function (point, pointIndex) {
        validatePoint(stroke.segments[pointIndex], point, 'frame[0].stroke.segment[' + pointIndex + ']');
      });
      if (stroke.closed !== true || stroke.strokeColor !== '#ffffff' || stroke.hasRealStroke !== false ||
          stroke.strokeWidth !== 1 || stroke.strokeCap !== 'butt' || stroke.strokeJoin !== 'miter' ||
          stroke.miterLimit !== 10 || stroke.fillColor !== '#ff0000' || stroke.opacity !== 1 ||
          stroke.dashOffset !== 0 || stroke.strokeId !== STROKE_ID) {
        throw new TypeError('frame-zero paint or geometry is unsupported');
      }
    });
  }

  function validatePosition(position) {
    exact(position, ['keys'], [], 'motion.position');
    if (!Array.isArray(position.keys) || position.keys.length !== 2) {
      throw new TypeError('motion.position requires the exact characterized keys');
    }
    position.keys.forEach(function (key, index) {
      exact(key, ['frame', 'v', 'curvePoints', 'hOut', 'hIn'], [], 'motion.position key');
      if (key.frame !== (index ? 20 : 0)) throw new TypeError('motion.position frame is unsupported');
      exactArray(key.v, index ? [128, 0] : [0, 0], 'motion.position value');
      exactArray(key.hOut, [0, 0], 'motion.position hOut');
      exactArray(key.hIn, [0, 0], 'motion.position hIn');
      if (!Array.isArray(key.curvePoints) || key.curvePoints.length !== 2) {
        throw new TypeError('motion.position curve is unsupported');
      }
      key.curvePoints.forEach(function (point, pointIndex) {
        exact(point, ['x', 'y', 'tx', 'ty'], [], 'motion.position curve point');
        var expected = pointIndex ? [1, 1, 1, 0] : [0, 0, 1, 0];
        exactArray([point.x, point.y, point.tx, point.ty], expected, 'motion.position curve point');
      });
    });
  }

  function validateOpacityCurve(points) {
    if (!Array.isArray(points) || points.length !== DEFAULT_CURVE.length) {
      throw new TypeError('opacity curve is unsupported');
    }
    points.forEach(function (point, index) {
      exact(point, ['x', 'y'], [], 'opacity curve point');
      exactArray([point.x, point.y], DEFAULT_CURVE[index], 'opacity curve point');
    });
  }

  function validateKeyedOpacity(track) {
    exact(track, ['keys'], [], 'motion.opacity');
    if (!Array.isArray(track.keys) || track.keys.length !== 2) {
      throw new TypeError('keyed opacity requires the exact characterized keys');
    }
    track.keys.forEach(function (key, index) {
      exact(key, ['frame', 'v', 'curvePoints', 'hOut', 'hIn'], [], 'motion.opacity key');
      if (key.frame !== (index ? 20 : 0)) throw new TypeError('keyed opacity frame is unsupported');
      exactArray(key.v, [index ? 80 : 20], 'keyed opacity value');
      exactArray(key.hOut, [0, 0], 'keyed opacity hOut');
      exactArray(key.hIn, [0, 0], 'keyed opacity hIn');
      validateOpacityCurve(key.curvePoints);
    });
  }

  function validateLegacyOpacitySource(source) {
    exact(source, [
      'version', 'totalFrames', 'fps', 'canvasW', 'canvasH', 'canvasBg',
      'waIn', 'waOut', 'layers', 'symbols', 'cameraKeys',
    ], [], 'native opacity source');
    if (source.version !== 13 || source.totalFrames !== 21 || source.fps !== 24 ||
        source.canvasW !== 320 || source.canvasH !== 180 || source.canvasBg !== '#ffffff' ||
        source.waIn !== 0 || source.waOut !== 20) {
      throw new TypeError('native opacity source metadata is unsupported');
    }
    if (!Array.isArray(source.layers) || source.layers.length !== 1 ||
        !plain(source.symbols) || Object.keys(source.symbols).length !== 0 ||
        !Array.isArray(source.cameraKeys) || source.cameraKeys.length !== 0) {
      throw new TypeError('native opacity source requires the exact single-layer family');
    }
    var layer = source.layers[0];
    exact(layer, [
      'name', 'visible', 'locked', 'frames', 'color', 'motion', 'layerUid', 'parentLayerUid',
    ], ['motionStatic'], 'native opacity layer');
    if (layer.name !== 'R08 rectangle' || layer.visible !== true || layer.locked !== false ||
        layer.color !== '#5FA875' || layer.layerUid !== LAYER_UID || layer.parentLayerUid !== null) {
      throw new TypeError('native opacity layer identity is unsupported');
    }
    identifier(layer.layerUid, 'layerUid');
    validateFrames(layer.frames);
    exact(layer.motion, ['position'], ['opacity'], 'native opacity layer.motion');
    validatePosition(layer.motion.position);
    if (!has(layer, 'motionStatic')) throw new TypeError('native opacity source requires motionStatic.opacity');
    exact(layer.motionStatic, ['opacity'], [], 'native opacity layer.motionStatic');
    opacity(layer.motionStatic.opacity, 'motionStatic.opacity');
    if (has(layer.motion, 'opacity')) validateKeyedOpacity(layer.motion.opacity);
    return has(layer.motion, 'opacity') ? 'keyed' : 'static';
  }

  function projectNative(source, mode) {
    var layer = source.layers[0];
    var projectedLayer = {
      layerUid: LAYER_UID,
      motionStatic: { opacity: layer.motionStatic.opacity.slice() },
    };
    if (mode === 'keyed') projectedLayer.motion = { opacity: clone(layer.motion.opacity) };
    return {
      format: 'nemo.native-opacity-document',
      formatVersion: 1,
      totalFrames: 21,
      layers: [projectedLayer],
    };
  }

  function geometryResources() {
    var resources = [], frames = [];
    for (var frame = 0; frame < 21; frame++) {
      var resourceId = 'geometry/r08/frame-' + frame;
      resources.push({
        resourceId: resourceId,
        resourceVersion: 'v1',
        layers: [{
          layerUid: LAYER_UID,
          bounds: [20, 60, 40, 80],
          transform: [1, 0, 0, 1, frame * 6.4, 0],
          paint: { red: 255, green: 0, blue: 0 },
        }],
      });
      frames.push({
        sourceFrame: frame,
        geometryHandle: { resourceId: resourceId, resourceVersion: 'v1' },
      });
    }
    return { resources: resources, frames: frames };
  }

  function assertBoundedReadEnvelopes(document) {
    var longest = new Array(129).join('x');
    var common = {
      apiVersion: 2, requestId: longest, instanceId: longest, documentId: longest,
      contentRevision: 9007199254740991, ok: true,
    };
    var serialize = Object.assign({}, common, { result: {
      atRevision: 9007199254740991, documentSnapshotId: longest, document: document,
    } });
    var evaluate = Object.assign({}, common, { result: {
      documentSnapshotId: longest, documentId: longest, contentRevision: 9007199254740991,
      contextId: longest, frame: 20, layers: [{ layerUid: LAYER_UID, value: 100 }],
    } });
    if (encodedBytes(serialize) > 4096 || encodedBytes(evaluate) > 4096) {
      throw new RangeError('native opacity read response exceeds 4096 encoded bytes');
    }
  }

  function prepareNativeOpacity(input) {
    var candidate = typeof input === 'string' ? JSON.parse(input) : input;
    var mode = validateLegacyOpacitySource(candidate);
    var source = clone(candidate);
    var projection = projectNative(source, mode);
    assertBoundedReadEnvelopes(projection);
    var geometry = geometryResources();
    return deepFreeze({
      opacityMode: mode,
      layerUid: LAYER_UID,
      totalFrames: 21,
      shell: source,
      projection: projection,
      resources: geometry.resources,
      frames: geometry.frames,
    });
  }

  function validateNativeDocument(document, expectedMode) {
    exact(document, ['format', 'formatVersion', 'totalFrames', 'layers'], [], 'native serialized document');
    if (document.format !== 'nemo.native-opacity-document' || document.formatVersion !== 1 ||
        document.totalFrames !== 21 || !Array.isArray(document.layers) || document.layers.length !== 1) {
      throw new TypeError('native serialized document is unsupported');
    }
    var layer = document.layers[0];
    exact(layer, ['layerUid', 'motionStatic'], expectedMode === 'keyed' ? ['motion'] : [], 'native serialized layer');
    if (layer.layerUid !== LAYER_UID) throw new TypeError('native serialized layer identity mismatch');
    exact(layer.motionStatic, ['opacity'], [], 'native serialized motionStatic');
    opacity(layer.motionStatic.opacity, 'native serialized motionStatic.opacity');
    if (expectedMode === 'keyed') {
      if (!has(layer, 'motion')) throw new TypeError('native keyed opacity is missing');
      exact(layer.motion, ['opacity'], [], 'native serialized motion');
      validateKeyedOpacity(layer.motion.opacity);
    }
    return layer;
  }

  function validateIdentity(value) {
    exact(value, ['instanceId', 'documentId', 'contentRevision'], [], 'native identity');
    identifier(value.instanceId, 'identity.instanceId');
    identifier(value.documentId, 'identity.documentId');
    if (!Number.isSafeInteger(value.contentRevision) || value.contentRevision < 0) {
      throw new TypeError('identity.contentRevision is invalid');
    }
    return value;
  }

  function composeNativeOpacity(prepared, response, identityValue) {
    var identity = validateIdentity(identityValue);
    if (encodedBytes(response) > 4096) throw new RangeError('native serialize response exceeds 4096 encoded bytes');
    exact(response, [
      'apiVersion', 'requestId', 'instanceId', 'documentId', 'contentRevision', 'ok', 'result',
    ], [], 'native serialize response');
    if (response.apiVersion !== 2 || response.ok !== true ||
        response.instanceId !== identity.instanceId || response.documentId !== identity.documentId ||
        response.contentRevision !== identity.contentRevision) {
      throw new Error('native serialize response identity mismatch');
    }
    exact(response.result, ['atRevision', 'documentSnapshotId', 'document'], [], 'native serialize result');
    identifier(response.requestId, 'response.requestId');
    identifier(response.result.documentSnapshotId, 'response.documentSnapshotId');
    if (response.result.atRevision !== identity.contentRevision) {
      throw new Error('native serialize response revision mismatch');
    }
    var nativeLayer = validateNativeDocument(response.result.document, prepared.opacityMode);
    var composed = clone(prepared.shell);
    composed.layers[0].motionStatic.opacity = nativeLayer.motionStatic.opacity.slice();
    if (prepared.opacityMode === 'keyed') composed.layers[0].motion.opacity = clone(nativeLayer.motion.opacity);
    else delete composed.layers[0].motion.opacity;
    if (validateLegacyOpacitySource(composed) !== prepared.opacityMode) {
      throw new Error('composed native opacity mode mismatch');
    }
    return deepFreeze(composed);
  }

  return Object.freeze({
    parse: parse,
    baseName: baseName,
    prepareNativeOpacity: prepareNativeOpacity,
    composeNativeOpacity: composeNativeOpacity,
    stringifyNativeOpacity: function (prepared, response, identity) {
      return JSON.stringify(composeNativeOpacity(prepared, response, identity));
    },
  });
}));
