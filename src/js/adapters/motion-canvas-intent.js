/* Canvas intent only: named geometry readers, no document/controller ownership. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoMotionCanvasIntent = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  function freeze(value) {
    if (value && typeof value === 'object') {
      Object.keys(value).forEach(function (key) { freeze(value[key]); });
      Object.freeze(value);
    }
    return value;
  }
  function create(readers, admission, initialize, checkpoint) {
    var required = ['multiLayerBox', 'valueAtFrame', 'unifiedMotionTargets', 'unifiedFrames',
      'unifiedPointAt', 'activeMotionTarget', 'findElementItem', 'motionBoxGeom', 'elementVertexPoints',
      'hit3DGizmoAxis', 'hit3DGizmoRing', 'gizmo3DAxisScreenPoints', 'outerWorldPoint', 'gizmo3DOriginScreen',
      'hitMotionBoxHandle', 'motionHandlePositions', 'activePositionKeys', 'hitPositionHandle',
      'hitPositionDot', 'hitAnchorPoint', 'hitEffectorHandle', 'outerLocalPoint', 'context',
      'yieldToShape', 'targetKind', 'vertexExpanded', 'probeAvailable'];
    required.forEach(function (name) {
      if (!readers || typeof readers[name] !== 'function') throw new TypeError('Missing canvas reader: ' + name);
    });
    if (typeof admission !== 'function' || typeof initialize !== 'function' || typeof checkpoint !== 'function') {
      throw new TypeError('Missing canvas writer admission');
    }
    function targetRef(t) {
      return { layerIndex: t.li, layerUid: readers.targetKind(t.li).layerUid, strokeId: t.strokeId || null };
    }
    function decision(mode, target, detail, targets) {
      return { mode: mode, targets: targets || (target ? [targetRef(target)] : []), detail: detail == null ? null : detail };
    }
  function select(event, observe) {
    var context = readers.context();
    // Inside a group with nothing picked yet, a click that lands ON a shape
    // belongs to that shape (2026-08-31). Without this the whole-LAYER gizmo
    // claimed it first — its ring, corners and body all sit over the very
    // shapes you are trying to pick, and the more the layer is rotated or
    // scaled the more of them fall under the cursor. Measured with the layer
    // at rotation 25°: clicking a shape inside the group targeted nothing
    // (_motionExpandedElement stayed empty) and the following drag moved the
    // whole layer instead, which reads exactly as "I can select an object
    // but not drag it".
    //
    // Deliberately narrow: only while per-object mode is on for THIS layer,
    // only while no element is targeted yet, and only when the point is
    // really on a shape. Every other Motion gesture — including the layer
    // gizmo outside a group, and every grab once an element IS targeted —
    // is untouched. Returning false hands the click to select-bridge, whose
    // Motion block does the targeting.
    if (readers.yieldToShape(event)) return null;
    var ml = readers.multiLayerBox();
    if (ml) {
      var mb = ml.bounds, mz = 1 / Math.max(0.0001, context.zoom);
      var dRing = Math.abs(Math.hypot(event.point.x - ml.pivot.x, event.point.y - ml.pivot.y) - ml.ringRadius);
      var hitCorner = false;
      [{x:mb.left,y:mb.top},{x:mb.right,y:mb.top},{x:mb.right,y:mb.bottom},{x:mb.left,y:mb.bottom}].forEach(function(p){
        if(Math.hypot(event.point.x-p.x,event.point.y-p.y)<9*mz)hitCorner=true;
      });
      var inside = event.point.x >= mb.left && event.point.x <= mb.right && event.point.y >= mb.top && event.point.y <= mb.bottom;
      if (dRing < 7 * mz || hitCorner || inside) {
        if (observe) return decision(dRing < 7 * mz ? 'multiLayerRotate' : hitCorner ? 'multiLayerScale' : 'multiLayerMove', null, null,
          ml.targets.map(function (r) { return targetRef(r.t); }));
        checkpoint();
        var records = ml.targets.map(function (rec) {
          return { t: rec.t, center: rec.center, pos: readers.valueAtFrame(rec.t.holder, 'position', context.frame).slice(), scale: readers.valueAtFrame(rec.t.holder, 'scale', context.frame).slice(), rot: readers.valueAtFrame(rec.t.holder, 'rotation', context.frame)[0] };
        });
        if (dRing < 7 * mz) {
          return { mode: 'multiLayerRotate', pivot: ml.pivot, startAngle: Math.atan2(event.point.y-ml.pivot.y,event.point.x-ml.pivot.x)*180/Math.PI, records: records };
        } else if (hitCorner) {
          return { mode: 'multiLayerScale', pivot: ml.pivot, origDist: Math.max(1e-6,Math.hypot(event.point.x-ml.pivot.x,event.point.y-ml.pivot.y)), records: records };
        } else {
          return { mode: 'multiLayerMove', start: {x:event.point.x,y:event.point.y}, records: records };
        }

      }
      return null;
    }
    // Unified multi-selection path first — while it's active the overlay
    // shows ONLY the unified dots (see buildOverlayItems), so the single-
    // target hit-tests below would grab invisible geometry.
    var u = readers.unifiedMotionTargets();
    if (u) {
      var uFrames = readers.unifiedFrames(u.targets), uTol = 8 / context.zoom;
      for (var ui = 0; ui < uFrames.length; ui++) {
        var upt = readers.unifiedPointAt(u, uFrames[ui]);
        if (Math.hypot(event.point.x - upt.x, event.point.y - upt.y) < uTol) {
          if (observe) return decision('unified', null, uFrames[ui], u.targets.map(function (r) {
            return targetRef({ li: context.activeLayer, strokeId: r.strokeId });
          }));
          checkpoint();
          return { mode: 'unified', u: u, frame: uFrames[ui], last: { x: event.point.x, y: event.point.y } };

        }
      }
      return null;
    }
    var t = readers.activeMotionTarget(observe);
    // Vertex handles (2026-07) checked FIRST, before the box/position/anchor
    // hit-tests below — once the Path group is expanded the user's whole
    // focus is on a specific vertex, so a vertex dot must win any incidental
    // overlap with the (usually much larger) scale/rotate box.
    if (t && t.strokeId && readers.vertexExpanded(t)) {
      var vItem2 = readers.findElementItem(t.li, t.strokeId);
      var vg2 = readers.motionBoxGeom(t);
      var vPts2 = readers.elementVertexPoints(vItem2);
      if (vPts2.length && vg2) {
        var vTol = 9 / context.zoom;
        for (var vi2 = 0; vi2 < vPts2.length; vi2++) {
          var seg2 = vPts2[vi2];
          var voff2 = readers.valueAtFrame(t.holder, 'vtx' + vi2, context.frame);
          var wp2 = vg2.fwd(seg2.x + (voff2[0] || 0), seg2.y + (voff2[1] || 0));
          if (Math.hypot(event.point.x - wp2.x, event.point.y - wp2.y) < vTol) {
            if (observe) return decision('vertex', t, vi2);
            checkpoint();
            return { mode: 'vertex', t: t, vi: vi2, basePt: { x: seg2.x, y: seg2.y } };

          }
        }
      }
    }
    // Position keys/handles checked BEFORE the anchor point (2026-07-17
    // motion-path-at-anchor fix made this ordering matter): a key at its
    // default [0,0] delta now draws its dot exactly ON the anchor
    // crosshair (motionPivotOf) — dragging keyframes is the far more
    // common gesture, so it wins the overlap; the anchor stays reachable
    // once a key has been moved away from it (the ordinary case) or by
    // starting the drag from a few px off-center.
    if (t) {
      // 3D gizmo (2026-07-28) checked BEFORE the 2D scale/rotate box below —
      // when a layer has 3D on, its own axis arrows/rotation rings are the
      // deliberate, precise controls for it, same "precise grab wins"
      // priority the box-handles-before-position-dots ordering already
      // established for the 2D case. Between the two 3D control types
      // (arrows vs rings), whichever is NUMERICALLY CLOSER to the click
      // wins — found by testing that a fixed "arrows always win" priority
      // let an arrow's line steal a click clearly aimed at a nearby ring
      // sample point.
      var axisHit3D = readers.hit3DGizmoAxis(event.point, t);
      var ringHit3D = readers.hit3DGizmoRing(event.point, t);
      if (axisHit3D && (!ringHit3D || axisHit3D.dist <= ringHit3D.dist)) {
        if (observe) return decision('axis3d', t, axisHit3D.axis);
        checkpoint();
        var axisPts3D_ = readers.gizmo3DAxisScreenPoints(axisHit3D.pose);
        var o3d = readers.outerWorldPoint(t, axisPts3D_[axisHit3D.axis].origin), tp3d = readers.outerWorldPoint(t, axisPts3D_[axisHit3D.axis].tip);
        var dx3d = tp3d.x - o3d.x, dy3d = tp3d.y - o3d.y, dl3d = Math.hypot(dx3d, dy3d) || 1;
        return {
          mode: 'axis3d', t: t, axis: axisHit3D.axis,
          dirX: dx3d / dl3d, dirY: dy3d / dl3d,
          startPt: { x: event.point.x, y: event.point.y },
          baseline: axisHit3D.axis === 'z' ? axisHit3D.pose.posZ : axisHit3D.pose.pos[axisHit3D.axis === 'x' ? 0 : 1],
        };

      }
      if (ringHit3D) {
        if (observe) return decision('ring3d', t, ringHit3D.axis);
        checkpoint();
        var center3d = readers.outerWorldPoint(t, readers.gizmo3DOriginScreen(ringHit3D.pose));
        var startAngle3D = Math.atan2(event.point.y - center3d.y, event.point.x - center3d.x) * 180 / Math.PI;
        return {
          mode: 'ring3d', t: t, axis: ringHit3D.axis, center: center3d, startAngle: startAngle3D,
          baseline: ringHit3D.axis === 'x' ? ringHit3D.pose.rotX : (ringHit3D.axis === 'y' ? ringHit3D.pose.rotY : ringHit3D.pose.rot),
        };

      }
      // Scale/rotate box handles checked FIRST — same priority order as
      // Animation 2D's own hitTestHandles (select-bridge.js): a corner/
      // rotate grab is a deliberate, precise action, so it should win any
      // rare overlap with a position dot/anchor rather than the reverse.
      // Skipped entirely for a 3D layer — the box isn't drawn there (see
      // buildOverlayItems' is3DTargetForBox), so it must not still be a
      // live (invisible) hit-target either.
      // Also skipped for a Null layer (feedback #59, "un petit bounding box
      // que l'on peu déplacer" never actually moved on drag): motionBoxGeom
      // gives a Null a fixed tiny 24px-equivalent box (hs=12/zoom) so
      // ringRadius (30% of that) collapses to ~7.2px — right on top of
      // hitMotionBoxHandle's own ±7px ring tolerance. The tolerance band
      // then swallows the ENTIRE clickable marker, so every click matched
      // 'rotate' and the correctly-working move handler in select-bridge.js
      // (mode:'null-drag', a few lines below this file's own onDown return)
      // never got a chance to run — confirmed live, dragging always rotated,
      // position never budged. A Null has no real use for a canvas
      // rotate/scale drag anyway (both properties stay reachable from the
      // panel) — skip the box gizmo outright so a plain click always falls
      // through to the dedicated move handler instead of chasing a
      // per-layer-type ring-radius tune.
      var kind = readers.targetKind(t.li), isNullTarget = kind.isNull;
      var boxHit = (isNullTarget || (kind.threeD && !t.strokeId)) ? null : readers.hitMotionBoxHandle(event.point, t);
      if (boxHit) {
        if (observe) return decision(boxHit.type === 'rotate' ? 'motionRotate' : 'motionScale', t, boxHit.dir);
        checkpoint();
        var g = readers.motionBoxGeom(t);
        if (boxHit.type === 'rotate') {
          var startAngle = Math.atan2(event.point.y - g.pivot.y, event.point.x - g.pivot.x) * 180 / Math.PI;
          return { mode: 'motionRotate', t: t, pivot: g.pivot, startAngle: startAngle, origRot: g.rot };
        } else {
          var corner = readers.motionHandlePositions(t).corners[boxHit.dir];
          var origDist = Math.hypot(corner.x - g.pivot.x, corner.y - g.pivot.y) || 1;
          // Single-axis edge handle (feedback #98) — the handle's own
          // world-space direction from the pivot (already rotation-correct,
          // since it's the ACTUAL rendered position, same box the corner
          // branch already trusts) becomes the axis to project the drag
          // onto, so only n/s scales Y and only e/w scales X. Two-letter
          // corners keep the untouched uniform-ratio path below.
          var axisDir = null;
          if (boxHit.dir === 'n' || boxHit.dir === 's' || boxHit.dir === 'e' || boxHit.dir === 'w') {
            axisDir = { ux: (corner.x - g.pivot.x) / origDist, uy: (corner.y - g.pivot.y) / origDist };
          }
          return { mode: 'motionScale', t: t, pivot: g.pivot, dir: boxHit.dir, axisDir: axisDir, origDist: origDist, origScale: g.scl.slice() };
        }

      }
      var ks = readers.activePositionKeys(t);
      if (ks) {
        var anc2=readers.valueAtFrame(t.holder,'anchor',context.frame);
        var pv={x:t.boundsCenter.x+anc2[0],y:t.boundsCenter.y+anc2[1]};
        var hp = readers.hitPositionHandle(event.point, ks, pv,t);
        if (hp) {
          if (observe) return decision('handle', t, { frame: hp.key.frame, tangent: hp.which });
          checkpoint();
          return { mode: 'handle', key: hp.key, which: hp.which, pv: pv,t:t };
        }
        var pk = readers.hitPositionDot(event.point, ks, pv,t);
        if (pk) {
          if (observe) return decision('point', t, pk.frame);
          checkpoint();
          return { mode: 'point', key: pk, pv: pv,t:t };
        }
      }
      // Alt required (2026-08-21, "pour bouger le point d'ancrage c'est
      // clic + alt + drag il me semble pas le cas là") — matches Animation
      // 2D's own anchor-crosshair convention (select-bridge.js's
      // hitTestHandles: "checked FIRST/exclusively, but ONLY while Alt is
      // held... a click in that same spot now falls through to the normal
      // move/marquee logic below" when Alt isn't held). Motion mode never
      // had that gate — a plain click within the small hit radius grabbed
      // the anchor unconditionally, which is what "il bouge encore" (the
      // artwork/box moving when the user only meant to click-drag
      // normally) was really describing: an accidental anchor grab, not
      // the anchor itself misbehaving.
      var ap = event.altKey ? readers.hitAnchorPoint(event.point, t) : null;
      if (ap) {
        if (observe) return decision('anchor', ap.target);
        checkpoint();
        return { mode: 'anchor', holder: ap.holder, bc: ap.bc, t:ap.target };
      }
      // Effector handles (2026-07-29) — checked last, lowest priority: they
      // only exist on duplicator layers and the user places them wherever
      // they like, so overlap with the box/position/anchor controls above
      // is rare, but those established grabs should still win if it happens.
      var effHit = readers.hitEffectorHandle(event.point, t);
      if (effHit) {
        if (observe) return decision('effector', t, effHit.id);
        checkpoint();
        return { mode: 'effector', t: t, eff: effHit };
      }
      // Dragging the BODY of a per-element box moves that element
      // (2026-08-30, feedback #170 follow-up: "si je bouge la box de
      // l'ellement aprés double clic ça bouge l'ensemble et pas les
      // propriété de la shape en question").
      //
      // onDown had no body-move mode at all — only ring/corners/anchor/
      // handles/vertices/effectors. For a whole-LAYER target that is
      // correct and deliberate: returning false lets select-bridge.js take
      // the gesture and move the layer, which is what its box means. But a
      // box that now hugs ONE element still fell through to that same
      // layer move, so the visible box and the thing that moved were
      // different objects. Measured before the fix: drag the element box by
      // (200,80) and ld.motionStatic.position became [200,80] while the
      // element holder stayed null.
      //
      // Gated on t.strokeId so the whole-layer path is untouched, and
      // placed LAST so every more specific grab above still wins.
      if (t.strokeId) {
        var gBody = readers.motionBoxGeom(t);
        if (gBody && gBody.bounds && gBody.inv) {
          // Test in the box's OWN local space, not as a world-space AABB:
          // gBody.bounds is un-transformed geometry and the drawn box can be
          // rotated/scaled, so comparing a world point against it directly
          // would hit-test a rectangle that isn't the one on screen. inv()
          // exists for exactly this (it was added for vertex dragging).
          //
          // event.point must go through outerLocalPoint FIRST (2026-08-31
          // fix) — motionBoxGeom's own inv is explicitly LOCAL-only (see its
          // comment: "no outer wrapping... every caller composes that
          // separately via outerWorldPoint/outerLocalPoint"), but this is
          // the one caller in this file that fed it the raw world point
          // directly. Every sibling grab just above (handles, position
          // dots, anchor, effector) already wraps through outerWorldPoint/
          // outerLocalPoint; this one, added later for feedback #170, never
          // got the same treatment. Invisible as long as the CONTAINING
          // layer had no Motion of its own — the two points coincide then —
          // which is why it went unnoticed until Cyril moved a whole group
          // and then tried to drag one of its elements: the box still drew
          // in the right (rotated) place, but a click dead-center on it
          // computed `lp` as if the layer had never moved, missing the
          // element's own local bounds entirely. Measured: layer rotated
          // 76.8°, click at the element's true rendered center — old code
          // path declined every time; onDown now grabs it.
          var outerPt = readers.outerLocalPoint(t, { x: event.point.x, y: event.point.y });
          var lp = gBody.inv(outerPt.x, outerPt.y);
          var bb = gBody.bounds;
          var insideEl = lp && lp.x >= bb.left && lp.x <= bb.right && lp.y >= bb.top && lp.y <= bb.bottom;
          if (insideEl) {
            if (observe) return decision('elementMove', t);
            checkpoint();
            return {
              mode: 'elementMove', t: t,
              start: { x: event.point.x, y: event.point.y },
              basePos: readers.valueAtFrame(t.holder, 'position', context.frame).slice()
            };

          }
        }
      }
    }
    return null;
  }

    function probe(event) {
      if (!readers.probeAvailable()) return Object.freeze({ available: false, handled: false });
      var hit = select(event, true);
      return freeze({ available: true, handled: !!hit, mode: hit && hit.mode,
        context: readers.context(), point: { x: event.point.x, y: event.point.y },
        altKey: !!event.altKey, targets: hit ? hit.targets : [], detail: hit && hit.detail });
    }
    function begin(intent) {
      if (!intent || !intent.available || !intent.handled) return false;
      if (intent.context.nativeOwned) return false;
      if (!admission('motion-canvas')) return false;
      if (JSON.stringify(probe(intent)) !== JSON.stringify(intent)) return false;
      return initialize(select(intent, false));
    }
    function onDown(event) {
      if (readers.context().nativeOwned) return false;
      if (!admission('motion-canvas')) return false;
      return initialize(select(event, false));
    }
    return Object.freeze({ probe: probe, begin: begin, onDown: onDown });
  }
  return Object.freeze({ create: create });
}));
