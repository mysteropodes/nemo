'use strict';
// Rust coverage ratchet evaluator (T04). Pure: no cargo, no child processes,
// no filesystem. Report generation and tool probing live in
// coverage-rust-job.cjs so this half can be exercised — and mutated — by unit
// tests alone.
//
// The baseline is the coverage actually observed at a reviewed SHA, recorded
// per production source file. A candidate run is compared against those
// floors. There is deliberately no global target: a file's floor IS its own
// last reviewed measurement, so the gate can only ever move one way.
//
// Metrics are lines/regions/functions. Branch coverage is NOT ratcheted and
// must not be added without re-reading BRANCHES_UNAVAILABLE below.

const path = require('node:path');

const SCHEMA = 'nemo.rust-coverage-baseline/1';
const REPORT_TYPE = 'llvm.coverage.json.export';
const METRICS = ['lines', 'regions', 'functions'];

// Observed 2026-09-10 on cargo/rustc 1.91.1 (aarch64-apple-darwin) with
// cargo-llvm-cov 0.9.1: every file reports `branches: {count: 0, percent: 0}`,
// including files that plainly contain `if`/`match`. Branch instrumentation
// needs a nightly `-Z coverage-options=branch` build, so on this toolchain a
// branch floor would compare 0 against 0 and pass unconditionally — a gate
// that can never fail, reported as if it were coverage. It is refused instead.
const BRANCHES_UNAVAILABLE =
  'branch coverage is not instrumented by the stable Rust toolchain (needs nightly '
  + '-Z coverage-options=branch); every branch count is 0, so a branch floor would be a '
  + 'gate that cannot fail. Excluded on purpose — see T04/#1053.';

// Floors are stored floored to two decimals, never rounded. Rounding
// 93.5483…% up to 93.55 would make the very run that produced it fail.
function floorPct(value) {
  if (typeof value !== 'number' || !Number.isFinite(value)) {
    throw new Error(`not a finite percentage: ${value}`);
  }
  return Math.floor(value * 100) / 100;
}

function toPosix(p) {
  return String(p).split(path.sep).join('/');
}

/**
 * Index an llvm-cov JSON export by repo-relative path.
 *
 * llvm-cov emits absolute filenames from whichever checkout produced the
 * report. Two worktrees of this repo therefore yield different absolute paths
 * for the same source, so everything is relativized against `root` before any
 * comparison; a baseline stays valid across worktrees.
 */
function readReport(report, root, sourceRoot) {
  if (!report || report.type !== REPORT_TYPE) {
    throw new Error(`unexpected coverage report type: ${report ? report.type : report} (want ${REPORT_TYPE})`);
  }
  const data = Array.isArray(report.data) ? report.data[0] : null;
  if (!data || !Array.isArray(data.files)) throw new Error('coverage report has no data[0].files array');

  const prefix = toPosix(sourceRoot).replace(/\/+$/, '') + '/';
  const files = new Map();
  const foreign = [];
  for (const entry of data.files) {
    if (!entry || typeof entry.filename !== 'string') throw new Error('coverage report entry has no filename');
    const rel = toPosix(path.relative(root, entry.filename));
    if (!rel.startsWith(prefix)) { foreign.push(rel); continue; }
    files.set(rel, summaryOf(rel, entry.summary));
  }
  return { files, foreign, totals: summaryOf('<totals>', data.totals) };
}

function summaryOf(label, summary) {
  if (!summary || typeof summary !== 'object') throw new Error(`${label}: coverage entry has no summary`);
  const percent = {};
  const counts = {};
  for (const metric of METRICS) {
    const value = summary[metric];
    if (!value || typeof value.percent !== 'number' || !Number.isFinite(value.percent)) {
      throw new Error(`${label}: coverage summary has no numeric ${metric}.percent`);
    }
    percent[metric] = value.percent;
    counts[metric] = { covered: value.covered, count: value.count };
  }
  return { percent, counts };
}

function validateBaseline(baseline) {
  if (!baseline || baseline.schema !== SCHEMA) {
    throw new Error(`unexpected baseline schema: ${baseline ? baseline.schema : baseline} (want ${SCHEMA})`);
  }
  if (typeof baseline.sourceRoot !== 'string' || !baseline.sourceRoot) {
    throw new Error('baseline has no sourceRoot');
  }
  if (!baseline.files || typeof baseline.files !== 'object') throw new Error('baseline has no files map');
  const metrics = baseline.metrics;
  if (!Array.isArray(metrics) || metrics.length !== METRICS.length
    || METRICS.some((m) => !metrics.includes(m))) {
    throw new Error(`baseline metrics must be exactly ${METRICS.join(', ')}`);
  }
  // A branch floor on this toolchain compares 0 against 0 forever. Refuse it
  // at load time rather than let it look like a passing branch gate.
  if (metrics.includes('branches') || Object.values(baseline.files).some((e) => e && Object.hasOwn(e, 'branches'))) {
    throw new Error(`baseline declares a branch floor: ${BRANCHES_UNAVAILABLE}`);
  }
}

function violation(rule, file, message, detail) {
  return { rule, file, message, detail: detail || null };
}

/**
 * Compare a fresh coverage report with the committed reviewed baseline.
 *
 * `sourceFiles` is every production source file found on disk, repo-relative.
 * It is required, and it is what makes a file that produces no coverage record
 * at all visible: a report-only comparison cannot distinguish "measured 0%"
 * from "never compiled in", and silence is the failure mode this whole leaf
 * exists to prevent.
 */
function evaluateRustCoverage(input) {
  const { report, baseline, sourceFiles, root } = input || {};
  validateBaseline(baseline);
  if (!Array.isArray(sourceFiles)) throw new Error('sourceFiles (repo-relative production source list) is required');
  if (typeof root !== 'string' || !root) throw new Error('root is required');

  const { files: candidate, foreign, totals } = readReport(report, root, baseline.sourceRoot);
  const baselineFiles = baseline.files;
  const excluded = new Map((baseline.exclusions || []).map((e) => [e.path, e.reason]));
  const onDisk = new Set(sourceFiles.map(toPosix));

  const violations = [];
  const improvements = [];
  const removals = [];
  const perFile = [];

  for (const [rel, entry] of Object.entries(baselineFiles)) {
    const measured = candidate.get(rel);
    const present = onDisk.has(rel);

    if (entry && entry.noRegions) {
      if (measured) {
        violations.push(violation('coverage-baseline-gained-regions', rel,
          `${rel} was baselined as having no executable regions but now reports coverage; re-baseline it with real floors`,
          { candidate: measured.percent }));
      } else if (!present) {
        removals.push({ file: rel, reason: 'source-removed', noRegions: true });
      } else {
        perFile.push({ path: rel, noRegions: true, reason: entry.reason || null });
      }
      continue;
    }

    if (!measured) {
      if (present) {
        // Never a skip: a baselined file that stops being measured is an
        // instrumentation gap, and reporting it as "fine" is the exact
        // failure this gate refuses.
        violations.push(violation('coverage-baseline-unmeasured', rel,
          `${rel} is present on disk but absent from the coverage report; it was not measured`,
          { floor: pick(entry) }));
      } else {
        removals.push({ file: rel, reason: 'source-removed', priorFloor: pick(entry) });
      }
      continue;
    }

    const row = { path: rel, percent: measured.percent, counts: measured.counts, floor: pick(entry) };
    for (const metric of METRICS) {
      const floor = entry[metric];
      if (typeof floor !== 'number' || !Number.isFinite(floor)) {
        throw new Error(`baseline ${rel} has no numeric ${metric} floor`);
      }
      const actual = measured.percent[metric];
      if (actual < floor) {
        violations.push(violation('coverage-baseline-regression', rel,
          `${rel}: ${metric} ${fmt(actual)}% fell below its ${fmt(floor)}% floor`,
          { metric, floor, candidate: actual, counts: measured.counts[metric] }));
      } else if (floorPct(actual) > floor) {
        improvements.push({ file: rel, metric, floor, candidate: actual });
      }
    }
    perFile.push(row);
  }

  for (const rel of candidate.keys()) {
    if (!Object.hasOwn(baselineFiles, rel)) {
      violations.push(violation('coverage-baseline-unbaselined', rel,
        `${rel} is measured but has no reviewed baseline entry; add it with its observed floors`,
        { candidate: candidate.get(rel).percent }));
    }
  }

  const flagged = new Set(violations.map((v) => v.file));
  for (const rel of onDisk) {
    if (!rel.startsWith(toPosix(baseline.sourceRoot).replace(/\/+$/, '') + '/')) continue;
    if (Object.hasOwn(baselineFiles, rel) || excluded.has(rel) || flagged.has(rel)) continue;
    violations.push(violation('coverage-baseline-unlisted-source', rel,
      `${rel} exists under ${baseline.sourceRoot} but is neither baselined nor a reviewed exclusion`, null));
  }

  const totalsFloor = baseline.totals || null;
  if (totalsFloor) {
    for (const metric of METRICS) {
      const floor = totalsFloor[metric];
      if (typeof floor !== 'number') continue;
      if (totals.percent[metric] < floor) {
        violations.push(violation('coverage-baseline-totals-regression', null,
          `crate totals: ${metric} ${fmt(totals.percent[metric])}% fell below the ${fmt(floor)}% floor`,
          { metric, floor, candidate: totals.percent[metric] }));
      }
    }
  }

  return {
    ok: violations.length === 0,
    violations,
    improvements,
    removals,
    perFile,
    foreign,
    totals: totals.percent,
    baselineFileCount: Object.keys(baselineFiles).length,
    measuredFileCount: candidate.size,
  };
}

function pick(entry) {
  const out = {};
  for (const metric of METRICS) if (entry && typeof entry[metric] === 'number') out[metric] = entry[metric];
  return out;
}

function fmt(value) {
  return Number(value).toFixed(2);
}

/**
 * Build a baseline object from an observed report. Used by the `--update`
 * path so recorded floors are the measurement itself, never typed by hand.
 */
function buildBaseline(input) {
  const { report, root, sourceRoot, sourceFiles, meta } = input || {};
  if (typeof sourceRoot !== 'string' || !sourceRoot) throw new Error('sourceRoot is required');
  if (!Array.isArray(sourceFiles)) throw new Error('sourceFiles is required');
  const { files: candidate, totals } = readReport(report, root, sourceRoot);
  const prefix = toPosix(sourceRoot).replace(/\/+$/, '') + '/';
  const exclusions = (meta && meta.exclusions) || [];
  const excluded = new Set(exclusions.map((e) => e.path));

  // Reviewed prose (why a module sits where it does) is written by a human
  // into the committed baseline. Regenerating floors must not silently erase
  // it, so `note` is carried across from the prior baseline.
  const priorFiles = (meta && meta.priorFiles) || {};
  const files = {};
  for (const rel of [...new Set(sourceFiles.map(toPosix))].sort()) {
    if (!rel.startsWith(prefix) || excluded.has(rel)) continue;
    const note = priorFiles[rel] && priorFiles[rel].note;
    const measured = candidate.get(rel);
    if (!measured) {
      files[rel] = { noRegions: true, reason: 'no executable regions in the coverage report' };
    } else {
      const entry = {};
      for (const metric of METRICS) entry[metric] = floorPct(measured.percent[metric]);
      files[rel] = entry;
    }
    if (note) files[rel].note = note;
  }

  const totalFloors = {};
  for (const metric of METRICS) totalFloors[metric] = floorPct(totals.percent[metric]);

  // `priorFiles` is an input to this function, not a field of the baseline.
  const { priorFiles: _carried, ...recordedMeta } = meta || {};
  return Object.assign({
    schema: SCHEMA,
    metrics: METRICS.slice(),
    branchesExcluded: BRANCHES_UNAVAILABLE,
  }, recordedMeta, {
    sourceRoot,
    totals: totalFloors,
    files,
    exclusions,
  });
}

module.exports = {
  SCHEMA, REPORT_TYPE, METRICS, BRANCHES_UNAVAILABLE,
  floorPct, readReport, validateBaseline, evaluateRustCoverage, buildBaseline,
};
