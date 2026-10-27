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
  function bounds(root,pin){var doc=root.document,el=doc.getElementById('drawing-canvas'),r=el&&el.getBoundingClientRect(),html=doc.documentElement;
      if(!r||![r.left,r.top,r.width,r.height,pin.canvasW,pin.canvasH].every(Number.isFinite)||r.width<=0||r.height<=0||pin.canvasW<=0||pin.canvasH<=0||r.left<0||r.top<0||r.left+r.width>html.clientWidth||r.top+r.height>html.clientHeight||!html.classList.contains('nemo-native-underlay'))return null;
      var edges=[r.left,r.top,r.left+r.width,r.top+r.height];
      if(typeof html.style.getPropertyValue!=='function'||!['left','top','right','bottom'].every(function(k,i){return parseFloat(html.style.getPropertyValue('--nemo-native-'+k))===edges[i];}))return null;
      return {left:r.left,top:r.top,width:r.width,height:r.height};}
  function transform(c,inverse){
    var v=c.presentation&&c.presentation.view;if(v===undefined)return [1,0,0,1,0,0];
    var m=v[inverse?'compositionToDocument':'documentToComposition'];
    if(!['canvas','stored-camera'].includes(v.mode)||!Array.isArray(m)||m.length!==6||!m.every(Number.isFinite))throw new Error('Native presented coordinates are unavailable');
    return m;
  }
  function mapped(p,c,inverse){var m=transform(c,inverse);return {x:m[0]*p.x+m[2]*p.y+m[4],y:m[1]*p.x+m[3]*p.y+m[5]};}
  function matchesView(root,p){if(!p||(p.view?p.view.mode:'canvas')!==(root.state&&root.state.cameraView===true?'stored-camera':'canvas'))return false;var project=root.NemoNativeObjectProject,session=project&&typeof project.desktop==='function'&&project.desktop(root);if(!session||typeof session.readPresentedObjectSession!=='function')return !(p.view&&p.view.navigation&& (p.view.navigation.zoom!==1||p.view.navigation.pan.some(function(n){return n!==0;})));var shown=session.readPresentedObjectSession();return !!shown&&!!shown.presentation&&['instanceId','documentId','contentRevision','documentSnapshotId','lifecycleGeneration','frame','workId','viewGeneration'].every(function(k){return shown.presentation[k]===p[k];})&&JSON.stringify(shown.presentation.view)===JSON.stringify(p.view);}
  function point(e,c){
    var m=transform(c,false),inverse=transform(c,true),nativeDet=m[0]*m[3]-m[1]*m[2];
    if(!Number.isFinite(nativeDet)||nativeDet===0)throw new Error('Native presented coordinates are unavailable');
    var expected=[m[3]/nativeDet,-m[1]/nativeDet,-m[2]/nativeDet,m[0]/nativeDet,
      (m[2]*m[5]-m[3]*m[4])/nativeDet,(m[1]*m[4]-m[0]*m[5])/nativeDet];
    if(!inverse.every(function(n,i){return n===expected[i];}))throw new Error('Native presented coordinates are unavailable');
    // Compose the same CSS affine as screen(), then remove its origin before
    // inversion. Applying a translated composition inverse after rescaling
    // introduces avoidable cancellation even for exact integer world points.
    var a=m[0]*c.bounds.width/c.canvasW,b=m[1]*c.bounds.height/c.canvasH,
      d=m[2]*c.bounds.width/c.canvasW,f=m[3]*c.bounds.height/c.canvasH,
      tx=m[4]*c.bounds.width/c.canvasW,ty=m[5]*c.bounds.height/c.canvasH,
      x=e.clientX-c.bounds.left-tx,y=e.clientY-c.bounds.top-ty,det=a*f-b*d;
    if(![a,b,d,f,tx,ty,x,y,det].every(Number.isFinite)||det===0)throw new Error('Native presented coordinates are unavailable');
    var result={x:(f*x-d*y)/det,y:(a*y-b*x)/det};
    if(!Number.isFinite(result.x)||!Number.isFinite(result.y))throw new Error('Native presented coordinates are unavailable');
    return result;
  }
  function screen(p,c){var q=mapped(p,c,false);return {x:c.bounds.left+q.x*c.bounds.width/c.canvasW,y:c.bounds.top+q.y*c.bounds.height/c.canvasH};}
  function create(ports) {
    var element = ports.document.documentElement, style = element.style;
    if (element.classList.contains(className)) throw new Error('Native underlay already has an owner');
    var previous = properties.map(function (property) {
      return { property: property, value: style.getPropertyValue(property),
        priority: style.getPropertyPriority(property) };
    });
    var owner = null, disposed = false, backdropHidden = false;
    function clear(expected) {
      if (expected !== undefined && !same(owner, expected)) return false;
      element.classList.remove(className);
      previous.forEach(function (item) {
        if (item.value) style.setProperty(item.property, item.value, item.priority);
        else style.removeProperty(item.property);
      });
      owner = null;
      if (backdropHidden) { backdropHidden = false; ports.setBackdropHidden(false); }
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
        if (typeof ports.setBackdropHidden === 'function') {
          backdropHidden = true; ports.setBackdropHidden(true);
        }
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
    var underlay = null, bounds = null, backdrop = null;
    // The stage is a non-document Paper backdrop; retain all other Paper/DOM overlays.
    function setBackdropHidden(hidden) {
      if (hidden) {
        var stage = root.stageLayer, view = root.paper && root.paper.view;
        if (!stage || typeof stage.visible !== 'boolean' || !view || typeof view.update !== 'function')
          throw new Error('Native presentation stage backdrop is unavailable');
        if (backdrop && backdrop.stage !== stage) throw new Error('Native presentation stage changed');
        if (!backdrop) backdrop = { stage: stage, visible: stage.visible, view: view };
        if (stage.visible) { stage.visible = false; view.update(); }
      } else if (backdrop) {
        var previous = backdrop; backdrop = null;
        previous.stage.visible = previous.visible; previous.view.update();
      }
    }
    // The host draws a document-sized composition into the surface framed at the canvas element, so the element takes
    // the document's aspect (native-viewport-underlay.css) before it is measured: one uniform presented scale whatever
    // the window's shape (#2343 B1). Ending the presentation restores the ordinary canvas.
    function fit(info) {
      var html = root.document.documentElement, aspect = info.canvasW / info.canvasH;
      if (!(aspect > 0) || !Number.isFinite(aspect)) throw new Error('Native canvas aspect is unavailable');
      html.style.setProperty('--nemo-native-aspect', String(aspect));
      html.classList.add('nemo-native-fit');
    }
    function unfit() {
      var html = root.document.documentElement;
      html.classList.remove('nemo-native-fit'); html.style.removeProperty('--nemo-native-aspect');
    }
    return Object.freeze({
      projectionReady: function (info) {
        return !!root.NemoNativeObjectProjectFile && root.NemoNativeObjectProjectFile.projectionMatches(root, info);
      },
      frame: function () { return root.state.currentFrame; },
      mapping: function (info) {
        fit(info);
        var rect = root.document.getElementById('drawing-canvas').getBoundingClientRect();
        var dpr = root.devicePixelRatio || 1;
        return { cssBounds: { x: rect.left, y: rect.top, width: rect.width, height: rect.height },
          physicalExtent: { width: Math.max(1, Math.round(rect.width * dpr)), height: Math.max(1, Math.round(rect.height * dpr)) },
          compositionExtent: { width: info.canvasW, height: info.canvasH }, reportedDpr: dpr };
      },
      surfaceChanged: function (presentation, mapping) {
        if (!presentation) { if (underlay) underlay.clear(); bounds = null; unfit(); return; }
        if (!underlay) underlay = create({ document: root.document, measure: function () { return bounds; }, setBackdropHidden: setBackdropHidden });
        bounds = { left: mapping.x, top: mapping.y, width: mapping.width, height: mapping.height };
        underlay.present(presentation);
      }
    });
  }
  return Object.freeze({ create: create, desktop: desktop, canvasMapping: Object.freeze({bounds:bounds,point:point,screen:screen,transform:transform,matchesView:matchesView}) });
}));
