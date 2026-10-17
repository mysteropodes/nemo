'use strict';
// L10-J test helper: the shipped Paper.js (src/paper-full.min.js) in a Node vm with the browser stubs paper.setup needs, plus the
// materialiser that builds a parsed-SVG item tree from the plain-data specs of the oracle fixtures. Not a test file.
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

function load() {
  const text = fs.readFileSync(path.join(__dirname, '..', '..', 'src', 'paper-full.min.js'), 'utf8');
  const sb = { Math };
  sb.window = sb; sb.self = sb; sb.navigator = { userAgent: 'node' };
  sb.HTMLCanvasElement = function HTMLCanvasElement() {};
  const ctx2d = new Proxy({}, { get: (t, k) => (k in t ? t[k] : function () { return 1; }), set: (t, k, v) => { t[k] = v; return true; } });
  const canvas = () => {
    const t = Object.assign(Object.create(sb.HTMLCanvasElement.prototype), {
      getContext: () => ctx2d, style: {}, width: 100, height: 100,
      get ownerDocument() { return sb.document; },
      getBoundingClientRect: () => ({ left: 0, top: 0, right: 100, bottom: 100, width: 100, height: 100 }),
    });
    return new Proxy(t, { get: (o, k) => (k in o ? o[k] : (typeof k === 'string' ? function () { return null; } : undefined)), set: (o, k, v) => { o[k] = v; return true; } });
  };
  sb.document = { createElement: canvas, addEventListener() {}, removeEventListener() {}, documentElement: { style: {}, clientLeft: 0, clientTop: 0 },
    getElementsByTagName: () => [], defaultView: sb, readyState: 'complete', body: {} };
  sb.pageXOffset = 0; sb.pageYOffset = 0; sb.getComputedStyle = () => ({});
  sb.requestAnimationFrame = () => 0; sb.addEventListener = () => {}; sb.removeEventListener = () => {};
  sb.setTimeout = () => 0; sb.clearTimeout = () => {};
  vm.runInContext(text + ';this.paper=paper;', vm.createContext(sb), { filename: 'paper-full.min.js' });
  const paper = sb.paper;
  paper.setup(canvas());
  return paper;
}

function paint(paper, it, st) {
  if (st.fill) it.fillColor = new paper.Color(...st.fill);
  if (st.stroke) it.strokeColor = new paper.Color(...st.stroke);
  if (st.sw !== undefined) it.strokeWidth = st.sw;
  if (st.op !== undefined) it.opacity = st.op;
  if (st.cap) it.strokeCap = st.cap;
  if (st.join) it.strokeJoin = st.join;
  if (st.miter !== undefined) it.miterLimit = st.miter;
  if (st.dash) it.dashArray = st.dash;
  if (st.dashOff !== undefined) it.dashOffset = st.dashOff;
}
function mkPath(paper, spec) {
  const p = new paper.Path({ insert: false });
  for (const s of spec.segs) p.add(new paper.Segment(new paper.Point(s[0], s[1]), new paper.Point(s[2], s[3]), new paper.Point(s[4], s[5])));
  p.closed = spec.closed;
  if (spec.style) paint(paper, p, spec.style);
  return p;
}
function mkNode(paper, spec) {
  if (spec.t === 'path') return mkPath(paper, spec);
  if (spec.t === 'comp') {
    const c = new paper.CompoundPath({ children: spec.children.map((k) => mkPath(paper, k)), insert: false });
    paint(paper, c, spec.style);
    return c;
  }
  if (spec.t === 'text') return new paper.PointText({ content: 'txt', insert: false });
  if (spec.t === 'odd') {
    const t = new paper.PointText({ content: 'odd', insert: false });
    Object.defineProperty(t, 'className', { value: spec.cls, configurable: true });
    return t;
  }
  const g = new paper.Group({ children: spec.children.map((k) => mkNode(paper, k)), applyMatrix: false, insert: false });
  if (spec.cls !== undefined) Object.defineProperty(g, 'className', { value: spec.cls, configurable: true });
  for (const op of spec.ops) {
    if (op[0] === 'translate') g.translate(new paper.Point(op[1], op[2]));
    else if (op[0] === 'rotate') g.rotate(op[1]);
    else if (op[0] === 'scale') g.scale(op[1], op[2]);
  }
  return g;
}
function treeTok(paper, it) {
  if (it instanceof paper.Path) return 'p';
  if (it instanceof paper.CompoundPath) return 'c';
  const k = it.children ? it.children.length : 0;
  const cls = it.className;
  const kids = k ? it.children.map((c) => treeTok(paper, c)) : [];
  return [`n:${cls === undefined || cls === null ? '~' : encodeURIComponent(cls) || '$'}:${k}`, ...kids].join(' ');
}
// The adapter's `paper` port: a project double whose importSVG answers the prepared tree (Paper's own parser needs a DOM).
function port(paper, answer) {
  return { project: { importSVG: (text, options) => answer(text, options) }, Matrix: paper.Matrix, Path: paper.Path, CompoundPath: paper.CompoundPath, Segment: paper.Segment, Point: paper.Point };
}
module.exports = { load, mkNode, treeTok, port };
