'use strict';
// Playwright config for tests/browser (T03/#1052). Previously there was no
// config at all — scripts/nemo/lib/jobs.cjs invoked the CLI with only
// `--output`, so a failure produced nothing but an exit code: no trace, no
// HTML/JUnit report. This file is the single source of report/retention
// policy; the job adapter only points NEMO_BROWSER_REPORT_DIR at its own
// per-run report directory.
const path = require('node:path');

// Falls back under the already-gitignored /reports/ (scripts/nemo's own
// report convention) rather than a new root-level directory, so a manual
// standalone run needs no .gitignore change.
const reportDir = process.env.NEMO_BROWSER_REPORT_DIR || path.join(__dirname, 'reports', 'playwright-manual');

// The negative control (tests/browser/_negative-control.spec.cjs) exists to
// prove this pipeline catches a real failure; it must never run as part of
// the normal suite. Playwright's CLI --grep/--grep-invert compose with (not
// override) the config's grep/grepInvert, so a CLI-only opt-in doesn't work
// here — verified empirically. This env var is the actual switch: verify the
// negative control on its own with
//   NEMO_BROWSER_NEGATIVE_CONTROL=1 node_modules/.bin/playwright test tests/browser
// which runs ONLY the tagged test (and is expected to fail).
const negativeControlOnly = !!process.env.NEMO_BROWSER_NEGATIVE_CONTROL;

module.exports = {
  testDir: 'tests/browser',
  outputDir: path.join(reportDir, 'test-results'),
  // Explicit, not left to Playwright's CI-autodetect default: a retried test
  // that then passes must never mask the original failure.
  retries: 0,
  grep: negativeControlOnly ? /@negative-control/ : undefined,
  grepInvert: negativeControlOnly ? undefined : /@negative-control/,
  reporter: [
    ['list'],
    ['html', { outputFolder: path.join(reportDir, 'html'), open: 'never' }],
    ['junit', { outputFile: path.join(reportDir, 'junit.xml') }],
  ],
  use: {
    // retain-on-failure: passing runs keep the bounded (empty) footprint they
    // have today; only a real failure leaves inspectable evidence behind.
    trace: 'retain-on-failure',
    screenshot: 'only-on-failure',
    video: 'retain-on-failure',
  },
};
