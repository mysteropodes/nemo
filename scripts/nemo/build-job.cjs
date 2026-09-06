#!/usr/bin/env node
'use strict';
const { SCHEMA, runBuildJob } = require('./lib/build-job.cjs');

runBuildJob({ reportDir: process.argv[2] }).then(result => {
  process.stdout.write(JSON.stringify(result) + '\n');
  process.exit(result.status === 'pass' ? 0 : result.status === 'blocked' ? 2 : 1);
}).catch(error => {
  process.stdout.write(JSON.stringify({ schema: SCHEMA, status: 'fail', exitCode: 1,
    reason: error.message, artifacts: [], details: {}, limitations: [] }) + '\n');
  process.exit(1);
});
