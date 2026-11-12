'use strict';
// CS3c/#2383 — the ordered body script list of src/index.html as data. The region between the two markers is
// written from engineering/application/script-manifest.json and never edited by hand; classic synchronous
// <script> order is preserved exactly (no dynamic loading). Entries, in load order:
//   { src }                 a classic external script          <script src="SRC"></script>
//   { src, type: 'module' } an external module script          <script type="module" src="SRC"></script>
//   { inline }              an inline classic script           <script>CODE</script>
//   { comment: [lines] }    an HTML comment kept verbatim, line by line
const fs = require('node:fs');
const path = require('node:path');

const MANIFEST = 'engineering/application/script-manifest.json';
const BEGIN = '<!-- script-manifest:begin — generated from engineering/application/script-manifest.json by scripts/nemo/script-manifest.cjs; do not edit -->';
const END = '<!-- script-manifest:end -->';
const CLASSIC = /^<script src="([^"<>]+)"><\/script>$/;
const MODULE = /^<script type="module" src="([^"<>]+)"><\/script>$/;
const INLINE = /^<script>([^<]*)<\/script>$/;

function fail(message) { throw new Error('script-manifest: ' + message); }

function validEntry(entry) {
  if (!entry || typeof entry !== 'object' || Array.isArray(entry)) return false;
  const keys = Object.keys(entry).sort().join(',');
  const src = (v) => typeof v === 'string' && /^[A-Za-z0-9_./-]+\.m?js$/.test(v) && !v.includes('..');
  if (keys === 'src') return src(entry.src);
  if (keys === 'src,type') return src(entry.src) && entry.type === 'module';
  if (keys === 'inline') return typeof entry.inline === 'string' && !/[<\n]/.test(entry.inline);
  if (keys === 'comment') {
    const lines = entry.comment;
    return Array.isArray(lines) && lines.length > 0 && lines.every((l) => typeof l === 'string' && !l.includes('\n')) &&
      lines[0].trimStart().startsWith('<!--') && lines[lines.length - 1].trimEnd().endsWith('-->') &&
      !lines.some((l) => l.includes('script-manifest:'));
  }
  return false;
}

function parse(manifest) {
  if (!manifest || manifest.format !== 'nemo.script-manifest' || manifest.formatVersion !== 1 || manifest.html !== 'src/index.html' ||
      !Array.isArray(manifest.entries)) fail('not a nemo.script-manifest v1 for src/index.html');
  manifest.entries.forEach((entry, at) => { if (!validEntry(entry)) fail(`entry ${at} is invalid: ${JSON.stringify(entry)}`); });
  const seen = new Set();
  for (const entry of manifest.entries) {
    if (!entry.src) continue;
    if (seen.has(entry.src)) fail(`${entry.src} is listed twice`);
    seen.add(entry.src);
  }
  return manifest;
}

function renderEntry(entry) {
  if (entry.comment) return entry.comment.join('\n');
  if (entry.inline !== undefined) return `<script>${entry.inline}</script>`;
  if (entry.type === 'module') return `<script type="module" src="${entry.src}"></script>`;
  return `<script src="${entry.src}"></script>`;
}

function render(manifest) {
  return [BEGIN, ...parse(manifest).entries.map(renderEntry), END].join('\n');
}

/** Entries of a hand-written block (initial extraction, or reading the generated region back). */
function extract(lines) {
  const entries = [];
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    let m;
    if ((m = line.match(CLASSIC))) entries.push({ src: m[1] });
    else if ((m = line.match(MODULE))) entries.push({ src: m[1], type: 'module' });
    else if ((m = line.match(INLINE))) entries.push({ inline: m[1] });
    else if (line.trimStart().startsWith('<!--')) {
      const comment = [line];
      while (!comment[comment.length - 1].includes('-->')) {
        if (++i >= lines.length) fail('unterminated comment in the script block');
        comment.push(lines[i]);
      }
      entries.push({ comment });
    } else fail(`line ${i + 1} of the script block is not a script tag or comment: ${line.slice(0, 80)}`);
  }
  return entries;
}

/** The region of `html` between the markers: { before, region, after } with exact text. */
function split(html) {
  if (html.includes('\r')) fail('src/index.html must use LF line endings');
  const begin = html.indexOf(BEGIN), end = html.indexOf(END);
  if (begin < 0 || end < 0 || end < begin || html.indexOf(BEGIN, begin + 1) >= 0 || html.indexOf(END, end + 1) >= 0) {
    fail('src/index.html must hold exactly one script-manifest begin/end marker pair');
  }
  return { before: html.slice(0, begin), region: html.slice(begin, end + END.length), after: html.slice(end + END.length) };
}

function write(html, manifest) {
  const parts = split(html);
  return parts.before + render(manifest) + parts.after;
}

/** Every <script> start tag of a page, one-based ordinal and line, as the coverage policy counts them. */
function observeScripts(html, htmlPath = 'src/index.html') {
  const attr = (text, name) => {
    const match = text.match(new RegExp('\\b' + name + '\\s*=\\s*(?:"([^"]*)"|\'([^\']*)\'|([^\\s>]+))', 'i'));
    return match && (match[1] || match[2] || match[3]);
  };
  return [...html.matchAll(/<script\b([^>]*)>/gi)].map((match, index) => ({
    src: attr(match[1], 'src'), module: attr(match[1], 'type') === 'module',
    path: htmlPath, line: html.slice(0, match.index).split('\n').length, scriptOrdinal: index + 1,
  }));
}

function readManifest(root) {
  return parse(JSON.parse(fs.readFileSync(path.join(root, MANIFEST), 'utf8')));
}

/** One entry per line, so an added adapter is a one-line diff. */
function format(manifest) {
  const head = Object.fromEntries(Object.entries(manifest).filter(([key]) => key !== 'entries'));
  const body = manifest.entries.map((entry) => '    ' + JSON.stringify(entry)).join(',\n');
  const top = JSON.stringify(head, null, 2).replace(/\n}$/, '');
  return `${top},\n  "entries": [\n${body}\n  ]\n}\n`;
}

module.exports = { MANIFEST, BEGIN, END, parse, render, extract, split, write, observeScripts, readManifest, format, validEntry };
