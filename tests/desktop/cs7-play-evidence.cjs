'use strict';
// Read the existing collector's terminal copy; operator replies cannot supply
// owner cleanup, package identity or trace completeness.
const fs = require('node:fs'), path = require('node:path');
const { spawnSync } = require('node:child_process');
const { isDeepStrictEqual: same } = require('node:util');
const { requireCheck: check, hash, alive } = require('./native-harness.cjs');
function read(file) {
  const before = fs.lstatSync(file);
  check(before.isFile() && !before.isSymbolicLink() && before.size <= 32 * 1024 * 1024 &&
    fs.realpathSync(file) === file, 'canonical bounded Play evidence required');
  const fd = fs.openSync(file, fs.constants.O_RDONLY | fs.constants.O_NOFOLLOW);
  try {
    const bytes = fs.readFileSync(fd), after = fs.fstatSync(fd);
    check(['dev', 'ino', 'size', 'mtimeMs'].every(k => before[k] === after[k]), 'Play evidence changed');
    return bytes;
  } finally { fs.closeSync(fd); }
}
function qualifyUnsigned(bundle) {
  const r = spawnSync('/usr/bin/codesign', ['-d', '--verbose=4', bundle], { encoding: 'utf8', maxBuffer: 1024 * 1024 });
  check(!r.error && !/^Authority=/m.test(r.stderr || '') &&
    (r.status === 0 && /^Signature=adhoc$/m.test(r.stderr || '') ||
      r.status === 1 && /code object is not signed at all/.test(r.stderr || '')), 'unsigned observer package required');
  return true;
}
function retained({ instance, identity, windowId, directory, cleaned, unsigned }) {
  const stop = instance.stopReceipt, copy = instance.playObserverCollection;
  const observer = instance.snapshot.app.playObserver;
  check(unsigned === true && cleaned && instance.stopped === true && stop?.stopped === true && stop.processTree?.stopped === true &&
    stop.released?.released === true && Array.isArray(stop.released.slots?.retained) && !stop.released.slots.retained.length &&
    stop.retainedData === false && !fs.existsSync(instance.snapshot.runtime.roots.root) &&
    !alive(identity.pid) && !alive(instance.info.pid), 'Play owner cleanup incomplete');
  check(copy?.status === 'complete' && copy.phase === 'after-owner-stop' &&
    copy.source === identity.sourceSha && copy.executableSha256 === identity.executableSha256 &&
    copy.pid === identity.pid && copy.taskId === identity.sessionId && copy.runId === observer.runId,
    'Play collection owner differs or is incomplete');
  check(fs.realpathSync(directory) === directory && !fs.lstatSync(directory).isSymbolicLink(), 'Play collection directory changed');
  const manifest = JSON.parse(read(path.join(directory, 'play-observer.json')));
  check(same(manifest, copy), 'retained Play manifest changed');
  const stderrFile = path.join(directory, 'native-app.stderr.log'), stderr = read(stderrFile);
  check(hash(stderrFile) === copy.stderrSha256, 'retained Play trace changed');
  return { schema: 'nemo.n20r-evidence/1', sourceCommit: identity.sourceSha,
    executableSha256: identity.executableSha256, runId: observer.runId, taskId: identity.sessionId,
    pid: identity.pid, windowId, stderr, collection: copy,
    package: { sourceCommit: identity.sourceSha, executableSha256: identity.executableSha256,
      observerFeature: true, unsigned: true },
    cleanup: { taskId: identity.sessionId, pid: identity.pid, appExited: true, launcherExited: true,
      processTreeStopped: true, slotsReleased: true, taskRemoved: true } };
}
module.exports = { read, retained, qualifyUnsigned };
