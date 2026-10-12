'use strict';
// C1 only: trusted native control-plane callback, never process control or seal.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;
const hex = (s, n) => typeof s === 'string' && new RegExp('^[0-9a-f]{' + n + '}$').test(s);
const uint = (n, max) => Number.isSafeInteger(n) && n >= 0 && n <= max;
function fault(reason) { return Object.assign(Error('startup collection refused'), { reason }); }
function need(ok, reason) { if (!ok) throw fault(reason); }
function keys(value, names) {
  return value && typeof value === 'object' && !Array.isArray(value)
    && Object.keys(value).length === names.length && Object.keys(value).every(k => names.includes(k));
}
function directory(value) {
  try {
    const absolute = path.resolve(value), meta = fs.lstatSync(absolute), parent = fs.lstatSync(path.dirname(absolute));
    need(meta.isDirectory() && !meta.isSymbolicLink() && parent.isDirectory() && !parent.isSymbolicLink(), 'directory-refused');
    return fs.realpathSync(absolute); // Standard macOS /var alias is legitimate.
  } catch (_) { throw fault('directory-refused'); }
}
function expectedPins(input) {
  try {
    const pins = Object.fromEntries(['runId','taskKey','pid','window','loadGeneration',
      'sourceSha','executableSha256','buildProofSha256'].map(k => [k, input[k]]));
    need(typeof pins.runId === 'string' && UUID.test(pins.runId) && hex(pins.taskKey, 64) && uint(pins.pid, 2147483647)
      && pins.pid > 0 && pins.window === 'main' && pins.loadGeneration === 1
      && hex(pins.sourceSha, 40) && hex(pins.executableSha256, 64) && hex(pins.buildProofSha256, 64), 'expected-identity');
    return Object.freeze(pins);
  } catch (_) { throw fault('expected-identity'); }
}
function identity(value, pins, js = false) {
  need(value.runId === pins.runId && value.taskKey === pins.taskKey && value.pid === pins.pid
    && value.window === pins.window && (!js || value.loadGeneration === pins.loadGeneration), 'source-identity');
}
function nativeProjection(v, pins) {
  need(keys(v, ['schema','runId','taskKey','pid','window','status','stages','accepted','rejected','dropped',
    'sealed','jsReadiness','visiblePaint']) && v.schema === 'nemo.startup-observer/1', 'source-schema');
  identity(v, pins);
  need(v.sealed === false && v.jsReadiness === 'not-observed' && v.visiblePaint === 'not-observed'
    && ['partial','refused','native-load-observed'].includes(v.status)
    && Array.isArray(v.stages) && v.stages.length <= 3 && v.accepted === v.stages.length
    && uint(v.rejected, 4294967295) && uint(v.dropped, 4294967295), 'source-schema');
  let elapsed = 0;
  v.stages.forEach((s, index) => {
    need(keys(s, ['sequence','stage','elapsedMs']) && s.sequence === index + 1
      && s.stage === ['window-created','native-page-started','native-page-finished'][index]
      && uint(s.elapsedMs, 600000) && s.elapsedMs >= elapsed, 'source-schema');
    elapsed = s.elapsedMs;
  });
  need(v.status !== 'native-load-observed' || (v.accepted === 3 && v.rejected === 0 && v.dropped === 0), 'source-schema');
  return v;
}
function jsRecord(v, pins, index) {
  const fields = {
    'early-script': [], 'dom-ready': [], 'window-load': [],
    'ready-state-observed': ['readyState'], 'startup-control': ['control','visibility'],
    'raf-ack': ['ordinal'], 'js-error': ['category'], 'unhandled-rejection': ['category'], 'probe-refused': ['reason'],
  };
  need(v && Object.hasOwn(fields, v.stage) && keys(v, ['schema','runId','loadGeneration','sequence','stage', ...fields[v.stage]])
    && v.schema === 'nemo.startup-js/1' && v.runId === pins.runId && v.loadGeneration === 1 && v.sequence === index + 1, 'source-schema');
  if ('readyState' in v) need(['interactive','complete'].includes(v.readyState), 'source-schema');
  if ('control' in v) need(v.control === 'start-open' && ['missing','hidden','visible','unknown'].includes(v.visibility), 'source-schema');
  if ('ordinal' in v) need([1,2].includes(v.ordinal), 'source-schema');
  if ('category' in v) need(['SyntaxError','ReferenceError','TypeError','RangeError','unknown'].includes(v.category), 'source-schema');
  if ('reason' in v) need(['synthetic-event','stage-order','unsupported-document','unsupported-port'].includes(v.reason), 'source-schema');
  need(Buffer.byteLength(JSON.stringify(v)) <= 1024, 'source-schema');
}
function jsProjection(outer, pins) {
  need(keys(outer, ['adapterRefused','js']) && typeof outer.adapterRefused === 'boolean', 'source-schema');
  const v = outer.js;
  need(keys(v, ['schema','runId','taskKey','pid','window','loadGeneration','nativePhase','status','accepted',
    'rejected','rejectionOverflow','fence','probeRefused','records','sealed','visiblePaint'])
    && v.schema === 'nemo.startup-js-native/1', 'source-schema');
  identity(v, pins, true);
  need(uint(v.nativePhase, 3) && ['fenced','provisional','native-start-correlated'].includes(v.status)
    && Array.isArray(v.records) && v.records.length <= 15 && v.accepted === v.records.length
    && uint(v.rejected, 15) && typeof v.rejectionOverflow === 'boolean' && typeof v.probeRefused === 'boolean'
    && [null,'navigation','replacement','closed','native-mismatch'].includes(v.fence)
    && (v.status === 'fenced') === (v.fence !== null) && v.sealed === false && v.visiblePaint === 'not-observed', 'source-schema');
  v.records.forEach((record, index) => jsRecord(record, pins, index));
  return outer;
}
function same(a, b) { return a.dev === b.dev && a.ino === b.ino; }
function openSource(file, max) {
  let fd;
  try {
    const named = fs.lstatSync(file, { bigint: true });
    need(named.isFile() && !named.isSymbolicLink() && named.size <= BigInt(max), 'source-io');
    fd = fs.openSync(file, fs.constants.O_RDONLY | fs.constants.O_NOFOLLOW);
    const stat = fs.fstatSync(fd, { bigint: true }); need(stat.isFile() && same(stat, named) && stat.size <= BigInt(max), 'source-io');
    return { fd, stat, max };
  } catch (error) {
    if (fd !== undefined) fs.closeSync(fd);
    if (error.code === 'ENOENT') return null;
    throw fault(error.reason || 'source-io');
  }
}
function finalSource(handle, validate) {
  if (!handle) return { hash: null, projection: null };
  const before = fs.fstatSync(handle.fd, { bigint: true });
  need(before.isFile() && same(before, handle.stat)
    && before.size > 0n && before.size <= BigInt(handle.max), 'source-unstable');
  const bytes = Buffer.alloc(Number(before.size));
  need(fs.readSync(handle.fd, bytes, 0, bytes.length, 0) === bytes.length, 'source-unstable');
  const after = fs.fstatSync(handle.fd, { bigint: true });
  need(same(before, after) && before.size === after.size && before.mtimeNs === after.mtimeNs && before.ctimeNs === after.ctimeNs, 'source-unstable');
  let value;
  try {
    const text = bytes.toString('utf8'); value = JSON.parse(text);
    // Current native producers emit compact ASCII scalar JSON. Reject hidden
    // duplicate keys, lossy UTF8 or alternate unreviewed encodings, not repair.
    need(JSON.stringify(value) === text, 'source-encoding'); validate(value);
  } catch (error) { throw fault(error.reason || 'source-schema'); }
  return { hash: crypto.createHash('sha256').update(bytes).digest('hex'), projection: value };
}
function validStop(proof, pins) {
  return keys(proof, ['schema','runId','taskKey','pid','appExited','launcherExited','processGroupExited','rootsRemoved','slotsReleased'])
    && proof.schema === 'nemo.startup-stop-proof/1' && proof.runId === pins.runId && proof.taskKey === pins.taskKey && proof.pid === pins.pid
    && ['appExited','launcherExited','processGroupExited','rootsRemoved','slotsReleased'].every(k => proof[k] === true);
}
function prepareCollection(ownedRoot, input, destination) {
  const pins = expectedPins(input), root = directory(ownedRoot), taskRoot = path.dirname(root), dest = directory(destination);
  const relative = path.relative(taskRoot, dest);
  need(relative === '..' || relative.startsWith('..' + path.sep) || path.isAbsolute(relative), 'destination-refused');
  const handles = [], name = path.join(dest, 'startup-collection.json'); let out, consumed = false, closed = false;
  function close() {
    if (closed) return; closed = true;
    for (const h of handles) if (h) { try { fs.closeSync(h.fd); } catch (_) {} }
    if (out !== undefined) { try { fs.closeSync(out); } catch (_) {} }
  }
  try {
    handles.push(openSource(path.join(root, 'startup-observer.json'), 8192));
    handles.push(openSource(path.join(root, 'startup-js.json'), 16384));
    out = fs.openSync(name, fs.constants.O_WRONLY | fs.constants.O_CREAT | fs.constants.O_EXCL | fs.constants.O_NOFOLLOW, 0o600);
  } catch (error) { close(); throw fault(error.reason || 'destination-refused'); }
  async function finalize(stopVerifier, timeoutMs = 1000) {
    need(!consumed && !closed, 'already-closed'); consumed = true;
    const receipt = { schema: 'nemo.startup-collection/1', ...pins, status: 'partial',
      stopProof: 'not-established', native: null, js: null, hashes: { native: null, js: null },
      reasons: [], sealed: false, visiblePaint: 'not-observed' };
    try {
      let proof, timer;
      try {
        if (typeof stopVerifier === 'function') proof = await Promise.race([
          Promise.resolve().then(() => stopVerifier(pins)),
          new Promise(resolve => { timer = setTimeout(resolve, uint(timeoutMs, 10000) && timeoutMs > 0 ? timeoutMs : 1000); }),
        ]);
      } catch (_) { /* No error payload. */ } finally { clearTimeout(timer); }
      if (!validStop(proof, pins) || fs.existsSync(taskRoot)) receipt.reasons.push('stop-unverified');
      else {
        receipt.stopProof = 'control-plane-verified';
        const native = finalSource(handles[0], v => nativeProjection(v, pins));
        const js = finalSource(handles[1], v => jsProjection(v, pins));
        receipt.native = native.projection; receipt.js = js.projection;
        receipt.hashes = { native: native.hash, js: js.hash };
        if (!native.projection) receipt.reasons.push('native-missing');
        if (!js.projection) receipt.reasons.push('js-missing');
        receipt.reasons.push('unsealed-source'); // C1 NEVER fabricates a cut or completion.
      }
    } catch (error) {
      receipt.status = 'refused'; receipt.reasons = [error.reason || 'source-io'];
      receipt.native = receipt.js = null; receipt.hashes = { native: null, js: null };
    }
    try {
      const bytes = Buffer.from(JSON.stringify(receipt)); need(bytes.length <= 32768, 'output-budget');
      const named = fs.lstatSync(name, { bigint: true }), held = fs.fstatSync(out, { bigint: true });
      need(named.isFile() && !named.isSymbolicLink() && same(named, held)
        && named.size === 0n && held.size === 0n, 'destination-refused');
      fs.writeFileSync(out, bytes); fs.fsyncSync(out);
    } catch (_) { throw fault('destination-refused'); } finally { close(); }
    return receipt;
  }
  return Object.freeze({ finalize, close, closed: () => closed });
}
module.exports = { prepareCollection };
