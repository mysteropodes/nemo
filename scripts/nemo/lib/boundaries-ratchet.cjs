'use strict';
// Exact-path size-baseline comparison for the R05 boundary checker.
//
// Both inputs use engineering/boundaries/profile.schema.json. The baseline is
// the previously reviewed/committed profile and the candidate is the policy
// being checked. Profile names and module assignments are deliberately not
// identities here: the root-relative source path and its effective ceiling are.

const fs = require('node:fs');
const path = require('node:path');
const { countNonBlankLines, validateProfile } = require('./boundaries.cjs');

function sizeEntries(profile) {
  validateProfile(profile);
  const exceptions = new Map(
    (profile.exceptions || [])
      .filter((exception) => exception.rule === 'size')
      .map((exception) => [exception.path, exception]),
  );
  const entries = new Map();
  for (const module of profile.modules) {
    const limits = profile.sizeProfiles[module.sizeProfile];
    for (const file of module.files) {
      const sourcePath = path.posix.join(module.dir, file);
      const exception = exceptions.get(sourcePath) || null;
      entries.set(sourcePath, {
        module: module.id,
        sizeProfile: module.sizeProfile,
        hardMax: limits.hardMax,
        ceiling: exception ? exception.ceiling : limits.hardMax,
        exception,
      });
    }
  }
  return entries;
}

function sourceState(root, sourcePath) {
  const absolute = path.resolve(root, ...sourcePath.split('/'));
  try {
    const stat = fs.statSync(absolute);
    if (!stat.isFile()) return { present: true, actualLines: null };
    return { present: true, actualLines: countNonBlankLines(fs.readFileSync(absolute, 'utf8')) };
  } catch (error) {
    if (error && error.code === 'ENOENT') return { present: false, actualLines: null };
    throw error;
  }
}

/**
 * Compare a candidate profile with its explicit prior baseline.
 *
 * This is a policy comparison, not a Git lookup. Callers must provide the
 * committed baseline file explicitly so a missing/unreadable reference fails
 * at the CLI boundary instead of silently disabling the ratchet.
 */
function compareSizeBaseline(baselineProfile, candidateProfile, opts = {}) {
  if (baselineProfile === undefined || baselineProfile === null) {
    throw new Error('invalid baseline profile: baseline is required');
  }
  if (candidateProfile === undefined || candidateProfile === null) {
    throw new Error('invalid candidate profile: candidate is required');
  }
  let baseline;
  let candidate;
  try {
    baseline = sizeEntries(baselineProfile);
  } catch (error) {
    throw new Error(`invalid baseline profile: ${error.message}`);
  }
  try {
    candidate = sizeEntries(candidateProfile);
  } catch (error) {
    throw new Error(`invalid candidate profile: ${error.message}`);
  }

  const root = path.resolve(opts.root || process.cwd());
  const violations = [];
  const reductions = [];
  const removals = [];

  // Ratchet the ordinary policy as well as per-path effective ceilings. A
  // retained profile name cannot grow. A renamed/new profile may reuse or
  // lower an adopted budget, but cannot introduce a larger ordinary maximum.
  const priorOrdinaryMax = Math.max(
    ...Object.values(baselineProfile.sizeProfiles).map((limits) => limits.hardMax),
  );
  for (const [name, limits] of Object.entries(candidateProfile.sizeProfiles)) {
    const priorLimits = Object.hasOwn(baselineProfile.sizeProfiles, name)
      ? baselineProfile.sizeProfiles[name]
      : null;
    if (priorLimits && limits.hardMax > priorLimits.hardMax) {
      violations.push({
        rule: 'size-baseline-profile-growth', module: null, file: null, line: null,
        message: `Size profile "${name}" hard maximum increased from ${priorLimits.hardMax} to ${limits.hardMax}`,
        detail: { sizeProfile: name, priorHardMax: priorLimits.hardMax, candidateHardMax: limits.hardMax },
      });
    } else if (!priorLimits && limits.hardMax > priorOrdinaryMax) {
      violations.push({
        rule: 'size-baseline-new-profile', module: null, file: null, line: null,
        message: `New size profile "${name}" hard maximum ${limits.hardMax} exceeds the prior ordinary maximum ${priorOrdinaryMax}`,
        detail: { sizeProfile: name, priorOrdinaryMax, candidateHardMax: limits.hardMax },
      });
    }
  }

  for (const [sourcePath, prior] of baseline) {
    const next = candidate.get(sourcePath);
    const source = sourceState(root, sourcePath);
    if (!next) {
      if (source.present) {
        violations.push({
          rule: 'size-baseline-path', module: prior.module, file: sourcePath, line: null,
          message: `Previously baselined path ${sourcePath} is still present but missing from the candidate profile`,
          detail: { priorCeiling: prior.ceiling, actualLines: source.actualLines },
        });
      } else {
        removals.push({ file: sourcePath, priorCeiling: prior.ceiling, reason: 'source-removed' });
      }
      continue;
    }

    if (next.ceiling > prior.ceiling) {
      violations.push({
        rule: 'size-baseline-growth', module: next.module, file: sourcePath, line: null,
        message: `Size ceiling for ${sourcePath} increased from ${prior.ceiling} to ${next.ceiling}`,
        detail: {
          priorCeiling: prior.ceiling,
          candidateCeiling: next.ceiling,
          actualLines: source.actualLines,
          priorSizeProfile: prior.sizeProfile,
          candidateSizeProfile: next.sizeProfile,
        },
      });
      continue;
    }

    // A legacy exception carries owner/issue/expiry accountability. It may be
    // removed only when the resulting ordinary profile ceiling is lower.
    if (prior.exception && !next.exception && next.ceiling >= prior.ceiling) {
      violations.push({
        rule: 'size-baseline-exception', module: next.module, file: sourcePath, line: null,
        message: `Size exception for ${sourcePath} was removed without reducing its ${prior.ceiling}-line allowance`,
        detail: {
          priorCeiling: prior.ceiling,
          candidateCeiling: next.ceiling,
          actualLines: source.actualLines,
          priorException: prior.exception,
        },
      });
      continue;
    }

    if (next.ceiling < prior.ceiling) {
      reductions.push({
        file: sourcePath,
        priorCeiling: prior.ceiling,
        candidateCeiling: next.ceiling,
        actualLines: source.actualLines,
      });
    }
  }

  // A new exact-path exception is new legacy debt. Reassigning or renaming an
  // old exception therefore cannot make it disappear from the comparison.
  for (const [sourcePath, next] of candidate) {
    if (!baseline.has(sourcePath) && next.exception) {
      const source = sourceState(root, sourcePath);
      violations.push({
        rule: 'size-baseline-new-exception', module: next.module, file: sourcePath, line: null,
        message: `Candidate adds a size exception for path absent from the prior baseline: ${sourcePath}`,
        detail: { candidateCeiling: next.ceiling, actualLines: source.actualLines, exception: next.exception },
      });
    }
  }

  return {
    ok: violations.length === 0,
    baselinePathCount: baseline.size,
    candidatePathCount: candidate.size,
    violations,
    reductions,
    removals,
  };
}

module.exports = { compareSizeBaseline };
