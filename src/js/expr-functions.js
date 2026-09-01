// ---- EXPRESSION FUNCTION REFERENCE (2026-09-01) ----
// Cyril, after seeing the examples library: "vois aussi ce qui pourrait
// encore mieux aider l'utilisateur dans l'édition, bibliothèque de
// fonctionnalité utilisable" — pointing at aescripts' expressCode/
// Expressionist as the bar (IntelliSense-style hover docs, a function
// library beside the editor).
//
// expr-examples.js is COMPLETE RECIPES ("how do I make a bounce"); this file
// is the atomic building blocks underneath them ("what does angleTo() take
// and return"), one entry per name in EXPR_PUBLIC_NAMES (motion.js) — never
// the undocumented AE-compatibility aliases, same rule as the examples file.
// `insert` is the literal text a click drops at the cursor — a real call
// shape with placeholder argument names, not just the bare identifier, so
// it reads as a fill-in-the-blanks template the way a real IDE snippet does.
(function () {
  var CATEGORIES = [
    {
      id: 'time',
      label: 'Time & Frame',
      fns: [
        { name: 'time', insert: 'time', doc: 'Current time in SECONDS (frame-rate aware).' },
        { name: 'frame', insert: 'frame', doc: 'Current frame number.' },
        { name: 'value', insert: 'value', doc: "This property's own keyframed/static value — a number (1D) or [x, y] (2D)." },
        { name: 'stepTime(n)', insert: 'stepTime(4)', doc: 'Snaps the clock to every n frames — stop-motion feel. Put it first; everything below then sees the snapped time.' },
        { name: 'toFrames(seconds?)', insert: 'toFrames()', doc: 'Seconds to frames at the current fps. No argument = the current time.' },
        { name: 'toSeconds(frame?)', insert: 'toSeconds()', doc: 'Frames to seconds at the current fps. No argument = the current frame.' },
      ],
    },
    {
      id: 'random',
      label: 'Randomness',
      fns: [
        { name: 'wiggle(freq, amp, octaves?)', insert: 'wiggle(2, 20)', doc: 'Random motion, freq per second, amplitude in the property’s own unit. Returns [x, y] on a 2D property — `value + wiggle(...)` then STRING-CONCATENATES instead of adding; use add(value, wiggle(...)) or index value[0]/[1] instead.' },
        { name: 'noise(x, y?)', insert: 'noise(time)', doc: 'Smooth -1..1 noise (nearby inputs give nearby results) — better than wiggle for a slow organic drift.' },
        { name: 'random(min?, max?)', insert: 'random(0, 1)', doc: 'Redraws every frame. No args = 0..1, one arg = 0..max, two = min..max (either may be an array for per-axis).' },
        { name: 'randomFixed(min?, max?)', insert: 'randomFixed(0, 1)', doc: 'Same as random(), but drawn ONCE and held for the whole timeline — a permanent per-layer offset.' },
        { name: 'randomGauss(min?, max?)', insert: 'randomGauss(0, 1)', doc: 'Bell-curve counterpart of random() — values cluster toward the middle of the range.' },
        { name: 'randomGaussFixed(min?, max?)', insert: 'randomGaussFixed(0, 1)', doc: 'Bell-curve counterpart of randomFixed() — one permanent draw per layer.' },
        { name: 'seed(n)', insert: "seed(self.name.length)", doc: 'Picks the random stream explicitly — same seed always draws the same sequence. Good for a per-layer unique but repeatable shake.' },
      ],
    },
    {
      id: 'math',
      label: 'Math & Vectors',
      fns: [
        { name: 'clamp(v, lo, hi)', insert: 'clamp(value, 0, 100)', doc: 'Restricts a number (or array) to a range.' },
        { name: 'remap(v, loT, hiT) / (v, loF, hiF, loT, hiT)', insert: 'remap(time, 0, 1, 0, 100)', doc: 'Rescales v from one range to another, linearly. 3-arg form reads v on 0..1.' },
        { name: 'remapEase(...)', insert: 'remapEase(time, 0, 1, 0, 100)', doc: 'Same as remap(), eased smooth at both ends (smoothstep).' },
        { name: 'remapEaseIn(...)', insert: 'remapEaseIn(time, 0, 1, 0, 100)', doc: 'Same as remap(), eased only at the start.' },
        { name: 'remapEaseOut(...)', insert: 'remapEaseOut(time, 0, 1, 0, 100)', doc: 'Same as remap(), eased only at the end.' },
        { name: 'degrees(radians)', insert: 'degrees(Math.PI)', doc: 'Radians to degrees (Nemo’s own Rotation unit).' },
        { name: 'radians(degrees)', insert: 'radians(90)', doc: 'Degrees to radians (for Math.sin/cos/atan2...).' },
        { name: 'add(a, b)', insert: 'add(value, [10, 0])', doc: 'Per-component addition — the correct way to add two [x, y] values (value + [x,y] concatenates strings instead).' },
        { name: 'sub(a, b)', insert: 'sub(a, b)', doc: 'Per-component subtraction, same reasoning as add().' },
        { name: 'mul(a, scalarOrVec)', insert: 'mul(value, 1.5)', doc: 'Scales a vector by a number, or per-axis by another vector.' },
        { name: 'div(a, scalarOrVec)', insert: 'div(value, 2)', doc: 'Divides a vector by a number, or per-axis by another vector.' },
        { name: 'dot(a, b)', insert: 'dot(a, b)', doc: 'Dot product.' },
        { name: 'cross(a, b)', insert: 'cross(a, b)', doc: '2D inputs: the Z component (a number). 3D inputs: the full 3-vector.' },
        { name: 'length(v) / (a, b)', insert: 'length(a, b)', doc: 'One argument: the vector’s own magnitude. Two: the distance between two points.' },
        { name: 'normalize(v)', insert: 'normalize(v)', doc: 'Same direction, length 1.' },
        { name: 'angleTo(from, to)', insert: 'angleTo(here, target)', doc: 'Angle in DEGREES from one point to another — feed straight into a Rotation expression.' },
      ],
    },
    {
      id: 'looping',
      label: 'Looping & Keys',
      fns: [
        { name: "loopAfter(mode, keyframes?)", insert: "loopAfter('cycle')", doc: "Replays past the LAST keyframe. mode: 'cycle' | 'pingpong' | 'offset' | 'continue'." },
        { name: "loopBefore(mode, keyframes?)", insert: "loopBefore('cycle')", doc: 'Same as loopAfter(), before the FIRST keyframe.' },
        { name: 'self.at(frame?)', insert: 'self.at(frame - 1)', doc: "This property's own RAW (pre-expression) value at another frame — safe against self-recursion." },
        { name: 'self.velocity(frame?)', insert: 'self.velocity(frame)', doc: 'Rate of change in units per SECOND, a numeric derivative of the raw track.' },
        { name: 'self.speed(frame?)', insert: 'self.speed(frame)', doc: 'Magnitude of self.velocity() — always a positive number, even on a 2D property.' },
        { name: 'self.keys.count', insert: 'self.keys.count', doc: 'How many keyframes this property has.' },
        { name: 'self.keys.at(i)', insert: 'self.keys.at(1)', doc: '1-indexed keyframe lookup — {frame, time, value, index}, or null.' },
        { name: 'self.keys.nearest(frame)', insert: 'self.keys.nearest(frame)', doc: 'The keyframe closest to a given frame — {frame, time, value, index}, or null.' },
      ],
    },
    {
      id: 'layers',
      label: 'Layers, Controls & Rig',
      fns: [
        { name: "layer(nameOrUid)", insert: "layer('LayerName')", doc: 'Read-only snapshot of another layer: {position, anchor, rotation, scale, opacity, name, index, inPoint, outPoint, hasParent, parent, at(prop, f), marker, control(name, f)}. null if not found.' },
        { name: 'self', insert: 'self', doc: 'This SAME holder — {at, velocity, speed, keys, control, index, name, property, hasParent, parent, marker, isShape}.' },
        { name: 'self.hasParent / self.parent', insert: 'self.hasParent ? self.parent.rotation : value', doc: 'self.parent is a full layer() snapshot of the parent, or null.' },
        { name: 'self.index / self.name', insert: 'self.name', doc: "This layer's position in the layer stack, and its name." },
        { name: 'self.inPoint / self.outPoint', insert: 'self.inPoint', doc: 'The frame range this layer is actually active for (in frames).' },
        { name: 'self.property', insert: 'self.property', doc: "The property name this expression is running on (e.g. 'position') — for one shared expression pasted onto several different properties that should behave differently on each." },
        { name: 'comp', insert: 'comp', doc: '{width, height, fps, frames, layers, name, marker} — the scene/component this expression lives in.' },
        { name: 'marker.at(i) / marker.nearest(f)', insert: 'marker.nearest(frame)', doc: 'Comp-level markers — {frame, time, name, color, index}, or null. self.marker is the same API scoped to this layer.' },
        { name: 'control(name, frame?)', insert: "control('Amount')", doc: "This layer's own expression control (rig-widget.js), by name — the self shorthand for self.control(...)." },
        { name: 'layerControl(uidOrName, name, frame?)', insert: "layerControl('LayerName', 'Amount')", doc: 'Cross-layer control read — cheaper than layer(x).control(y) when only one number is needed (skips building a whole snapshot).' },
        { name: 'contentBox()', insert: 'contentBox()', doc: 'This holder’s own drawn bounds this frame — {x, y, width, height, top, left}.' },
      ],
    },
    {
      // Only meaningful on a per-vertex property (vtx0, vtx1, ...) of an
      // image mesh (image-mesh.js, CLAUDE.md §12ter) — narrow enough that
      // it would clutter the main Layers category above for the common
      // case, but real, working, and documented (§12ter's own worked
      // example is literally `self.vertexUV`), so it belongs here rather
      // than being silently missing from what claims to be the reference.
      id: 'mesh',
      label: 'Image Mesh (per-vertex)',
      fns: [
        { name: 'self.isMesh', insert: 'self.isMesh', doc: 'True when this expression is running on an image-mesh vertex property.' },
        { name: 'self.vertexIndex', insert: 'self.vertexIndex', doc: "This vertex's index into the mesh." },
        { name: 'self.vertexCount', insert: 'self.vertexCount', doc: 'Total vertex count of this mesh.' },
        { name: 'self.vertexUV', insert: 'self.vertexUV', doc: "This vertex's REST position, normalized 0..1 over the image — [u, v]. The offset a vtxN expression returns is ADDED to the sculpted pose, same as any other Motion property." },
        { name: 'self.isOutlineVertex', insert: 'self.isOutlineVertex', doc: 'True when this vertex sits on the mesh’s outline (its mask boundary) rather than the interior.' },
      ],
    },
  ];

  window.SM_EXPR_FUNCTIONS = CATEGORIES;
})();
