// Shared helpers for the R18.2 prep oracles. Reads motion.js VERBATIM from a
// pinned commit and extracts named function declarations by brace matching.
import { execFileSync } from 'node:child_process';

export const PIN = process.env.PIN_SHA || '660ac5ef79f9a1da54cfabbe5279aeb76a941f73';

export function motionSource(repo) {
  return execFileSync('git', ['-C', repo, 'show', `${PIN}:src/js/motion.js`], {
    encoding: 'utf8',
    maxBuffer: 64 << 20,
  });
}

export function extract(src, name) {
  const head = `function ${name}(`;
  const starts = [];
  let at = src.indexOf(head);
  while (at >= 0) { starts.push(at); at = src.indexOf(head, at + 1); }
  if (starts.length !== 1) throw new Error(`${name}: ${starts.length} declarations`);
  let i = src.indexOf('{', starts[0]);
  let depth = 0;
  for (; i < src.length; i++) {
    const c = src[i];
    if (c === '{') depth++;
    else if (c === '}' && --depth === 0) return src.slice(starts[0], i + 1);
    else if (c === '/' && src[i + 1] === '/') { while (src[i] !== '\n') i++; }
    else if (c === '/' && src[i + 1] === '*') { i = src.indexOf('*/', i + 2) + 1; }
    else if (c === "'" || c === '"') { // skip simple string literals
      const q = c; i++;
      while (src[i] !== q) { if (src[i] === '\\') i++; i++; }
    }
  }
  throw new Error(`${name}: unbalanced`);
}

const buf = new DataView(new ArrayBuffer(8));
export function hx(x) {
  buf.setFloat64(0, x);
  return buf.getBigUint64(0).toString(16).padStart(16, '0');
}
export const hl = (xs) => xs.map(hx).join(',');

// Deterministic mulberry32.
export function rng(seed) {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}
