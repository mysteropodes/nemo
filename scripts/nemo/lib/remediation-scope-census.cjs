'use strict';

// Census partitions declare their packets' source ownership as free-form strings
// ("path", "path:12-46", "path:20,25-30 (symbols)", "path: symbol list", "a, b, c (3)").
// This module turns those declarations into paths and line ranges, maps ranges written
// against an older source SHA onto the frozen tree, and computes per-file line coverage.
// It has no opinion about ownership: every packet is evidence, overlaps are reported.

const CENSUS_PATH = /^engineering\/inventory\/census\/[^/]+\.json$/;
const RANGE_LIST = /^\d+(?:-\d+)?(?:\s*,\s*\d+(?:-\d+)?)*$/;

function parseRanges(rest) {
  if (!rest) return null;
  const head = rest.replace(/\s*\(.*$/, '').replace(/~/g, '').trim();
  let list = RANGE_LIST.test(head) ? head : null;
  if (!list) {
    const inParens = /\((\d+)-(\d+)(?:[,\s][^)]*)?\)/.exec(rest);
    list = inParens ? `${inParens[1]}-${inParens[2]}` : null;
  }
  if (!list) return null;
  return list.split(/\s*,\s*/).map((part) => {
    const [start, end] = part.split('-').map(Number);
    return [start, end === undefined ? start : end];
  });
}

// Glob, brace and directory declarations are left as written: they never match a tracked
// path literally, so they contribute no line coverage and no mechanical reference.
function parseDeclaration(text) {
  const trimmed = String(text).trim();
  const body = trimmed.replace(/\s*\([^()]*\)\s*$/, '');
  if (!body.includes(':') && body.includes(', ')) {
    return body.split(/\s*,\s*/).filter(Boolean).map((entry) => ({ path: entry, ranges: null }));
  }
  const colon = trimmed.indexOf(':');
  const file = (colon < 0 ? body : trimmed.slice(0, colon)).trim();
  return [{ path: file, ranges: parseRanges(colon < 0 ? '' : trimmed.slice(colon + 1).trim()) }];
}

// Supplements replaced C08's preliminary packets; only its documentation map remains.
function censusPackets(document) {
  const out = [];
  for (const area of document.areas || []) {
    if (document.census_id === 'C08' && area.area !== 'documentation-and-process') continue;
    for (const packet of area.packets || []) out.push(packet);
  }
  return out;
}

// One declaration row per (packet, path): { packet, census, sourceCommit, path, ranges }.
function declarations(censuses) {
  const rows = [];
  for (const { document } of censuses) {
    for (const packet of censusPackets(document)) {
      for (const text of packet.files || []) {
        for (const { path, ranges } of parseDeclaration(text)) {
          rows.push({ packet: packet.id, census: document.census_id,
            sourceCommit: document.source_sha, path, ranges });
        }
      }
    }
  }
  return rows;
}

// Hunks of `git diff --unified=0 from to -- path` as [oldStart, oldLen, newStart, newLen].
function parseHunks(diffText) {
  const hunks = [];
  const pattern = /^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@/gm;
  let match;
  while ((match = pattern.exec(diffText)) !== null) {
    hunks.push([Number(match[1]), match[2] === undefined ? 1 : Number(match[2]),
      Number(match[3]), match[4] === undefined ? 1 : Number(match[4])]);
  }
  return hunks;
}

// Map one old line number through hunks; null when the line was deleted or rewritten.
// Git anchors a zero-length side one line early (`-5,0 +6,3` inserts after old 5,
// `-5,2 +4,0` deletes 5-6), so a zero-length side is normalised to the next line.
function mapLine(hunks, line) {
  let offset = 0;
  for (const [oldStart, oldLen, newStart, newLen] of hunks) {
    const from = oldLen === 0 ? oldStart + 1 : oldStart;
    const to = newLen === 0 ? newStart + 1 : newStart;
    if (line < from) return line + offset;
    if (line < from + oldLen) return null;
    offset = to + newLen - (from + oldLen);
  }
  return line + offset;
}

function compressLines(lines) {
  const spans = [];
  for (const line of [...lines].sort((a, b) => a - b)) {
    const last = spans[spans.length - 1];
    if (last && line === last[1] + 1) last[1] = line;
    else if (!last || line !== last[1]) spans.push([line, line]);
  }
  return spans;
}

function mapRanges(hunks, ranges) {
  const lines = [];
  for (const [start, end] of ranges) {
    for (let line = start; line <= end; line += 1) {
      const mapped = mapLine(hunks, line);
      if (mapped !== null) lines.push(mapped);
    }
  }
  return compressLines(lines);
}

// Blank lines and lines holding only closing brackets/separators belong to no packet: a
// span made only of them is dropped and `uncovered` counts code lines inside kept spans.
const STRUCTURAL = /^\s*[\])};,]*\s*$/;

// tools.lines(path) → string[]|null at the frozen tree;
// tools.hunks(path, fromCommit) → parsed hunks from that commit to the frozen tree.
function rangeCoverage(rows, tools) {
  const byPath = new Map();
  for (const row of rows) {
    const entry = byPath.get(row.path) || { whole: false, ranges: [] };
    if (row.ranges) entry.ranges.push(row);
    else entry.whole = true;
    byPath.set(row.path, entry);
  }
  const report = [];
  for (const [file, entry] of [...byPath].sort()) {
    if (entry.whole || !entry.ranges.length) continue;
    const body = tools.lines(file);
    if (body === null) continue;
    const lines = body.length;
    const hits = new Uint16Array(lines + 1);
    for (const row of entry.ranges) {
      const hunks = tools.hunks(file, row.sourceCommit);
      const mapped = hunks.length ? mapRanges(hunks, row.ranges) : row.ranges;
      for (const [start, end] of mapped) {
        for (let line = start; line <= Math.min(end, lines); line += 1) hits[line] += 1;
      }
    }
    const code = (line) => !STRUCTURAL.test(body[line - 1]);
    const spans = compressLines([...hits.keys()].filter((line) => line >= 1 && hits[line] === 0))
      .filter(([start, end]) => { for (let l = start; l <= end; l += 1) if (code(l)) return true; return false; });
    let uncovered = 0;
    for (const [start, end] of spans) for (let l = start; l <= end; l += 1) if (code(l)) uncovered += 1;
    let overlap = 0;
    for (let line = 1; line <= lines; line += 1) if (hits[line] > 1) overlap += 1;
    report.push({ path: file, lines, uncovered, overlap, spans });
  }
  return report;
}

module.exports = {
  CENSUS_PATH, parseRanges, parseDeclaration, censusPackets, declarations,
  parseHunks, mapLine, mapRanges, compressLines, rangeCoverage,
};
