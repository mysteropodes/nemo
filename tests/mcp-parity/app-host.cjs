'use strict';
// Standalone application host for R14 Lane F client-boundary parity.
//
// This is a faithful port of the APPLICATION_HOST fixture embedded in
// nemo-mcp/tests/stdio_application.rs. It exists as a real file because the
// parity harness has to keep one instance alive across many separate client
// processes, which a string embedded in a Rust test cannot do.
//
// It supplies ONLY document/history ports and the endpoint transport. Request
// validation, revision accounting, retry retention and mutation dispatch are
// the production JS modules required below. That is the whole point: the
// client path and the oracle path exercise the same application code, so any
// divergence between them is attributable to the client boundary and nothing
// else.
//
// Deliberate deviations from the Rust fixture, both required by the harness
// and neither touching application behaviour:
//   1. The TCP server keeps serving instead of exiting after one exchange.
//      wire::call opens a fresh connection per request, so per-connection
//      handling is unchanged; only process lifetime differs.
//   2. The host writes its own discovery record so an out-of-process client
//      can find it, and removes it on exit.
//
// This host does NOT establish native UI history or installed-client
// acceptance on its own. It isolates the client-path variable. Acceptance
// against the real installed distribution is Lane H's package plus stage 2.

const path = require('node:path');
const fs = require('node:fs');
const net = require('node:net');
const readline = require('node:readline');
const crypto = require('node:crypto');

const repo = path.resolve(__dirname, '..', '..');
const core = require(path.join(repo, 'src/js/application/opacity-application.js'));
const domain = require(path.join(repo, 'src/js/domain/animation/opacity.js'));

const instanceId = process.env.NEMO_PARITY_INSTANCE || crypto.randomUUID();
const secret = process.env.NEMO_PARITY_SECRET || crypto.randomUUID();
const buildId = process.env.NEMO_PARITY_BUILD || 'parity-host';
const registryRoot = process.env.NEMO_MCP_REGISTRY;
if (!registryRoot || !path.isAbsolute(registryRoot)) {
  throw new Error('NEMO_MCP_REGISTRY must be set to an absolute path');
}

let identity = 0;
let context = {};
let checkpoints = 0;
let mutations = 0;
let state = fresh();
let undo = [];
let redo = [];

// A slightly richer document than the Rust fixture's single layer: parity over
// one layer cannot show a client mis-selecting among several.
function fresh() {
  return {
    currentFrame: 0,
    totalFrames: 24,
    layers: [
      { layerUid: 'layer-a', name: 'Layer A', motionStatic: { opacity: [100] } },
      { layerUid: 'layer-b', name: 'Layer B', motionStatic: { opacity: [60] } },
      { layerUid: 'layer-c', name: 'Layer C', motionStatic: { opacity: [25] } },
    ],
  };
}

function copy(value) {
  return JSON.parse(JSON.stringify(value));
}

const api = core.create({
  newId: () => 'document-' + (++identity),
  context: () => context,
  state: () => state,
  canMutate: () => true,
  valueAtFrame: (layer) => layer.motionStatic.opacity,
  snapshot: () => ({
    frame: state.currentFrame,
    layers: state.layers.map((layer) => ({
      id: layer.layerUid,
      name: layer.name,
      opacity: layer.motionStatic.opacity[0],
    })),
  }),
  history: {
    checkpoint() {
      undo.push(copy(state));
      redo = [];
      checkpoints++;
    },
    undo() {
      if (!undo.length) return false;
      redo.push(copy(state));
      state = undo.pop();
      return true;
    },
    redo() {
      if (!redo.length) return false;
      undo.push(copy(state));
      state = redo.pop();
      return true;
    },
  },
  write(operation, layer, payload) {
    if (operation !== 'property.set') {
      throw new Error('Unexpected fixture operation: ' + operation);
    }
    domain.setValue(layer, [payload.value], state.currentFrame);
  },
  afterMutation() {
    mutations++;
  },
});

if (!api.setInstanceId(instanceId).ok) throw new Error('Instance binding failed');

function local(message) {
  if (message.action === 'call') return api.handle(message.request);
  if (message.action === 'replace') {
    state = fresh();
    undo = [];
    redo = [];
    context = {};
    api.documentChanged();
  } else if (message.action !== 'inspect') {
    throw new Error('Unexpected fixture action');
  }
  return {
    meta: api.meta(),
    state: copy(state),
    checkpoints,
    mutations,
    undoDepth: undo.length,
    redoDepth: redo.length,
  };
}

const server = net.createServer((socket) => {
  socket.setTimeout(5000, () => socket.destroy());
  let data = '';
  let handled = false;
  socket.on('error', () => {});
  socket.on('data', (bytes) => {
    if (handled) return;
    data += bytes.toString();
    if (Buffer.byteLength(data) > 1048576) return socket.destroy();
    if (!data.includes('\n')) return;
    handled = true;
    try {
      const message = JSON.parse(data.slice(0, data.indexOf('\n')));
      if (message.secret !== secret) throw new Error('Wrong endpoint secret');
      socket.end(JSON.stringify(api.handle(message.request)) + '\n');
    } catch (error) {
      console.error(error.message);
      socket.destroy();
    }
  });
});

let recordPath = null;
function removeRecord() {
  if (recordPath) {
    try {
      fs.rmSync(recordPath, { force: true });
    } catch {
      /* best effort on shutdown */
    }
    recordPath = null;
  }
}

server.listen(0, '127.0.0.1', () => {
  const port = server.address().port;
  // Same shape and permissions the Rust Registration type writes, so the
  // server under test reads a record it would accept in production.
  fs.mkdirSync(registryRoot, { recursive: true, mode: 0o700 });
  fs.chmodSync(registryRoot, 0o700);
  recordPath = path.join(registryRoot, `${instanceId}.json`);
  fs.writeFileSync(
    recordPath,
    JSON.stringify({ instanceId, port, secret, buildId }),
    { mode: 0o600 },
  );
  console.log(JSON.stringify({ ready: true, instanceId, port, buildId, record: recordPath }));

  readline
    .createInterface({ input: process.stdin })
    .on('line', (line) => {
      const trimmed = line.trim();
      if (!trimmed) return;
      console.log(JSON.stringify(local(JSON.parse(trimmed))));
    })
    .on('close', () => {
      removeRecord();
      process.exit(0);
    });
});

for (const signal of ['SIGINT', 'SIGTERM']) {
  process.on(signal, () => {
    removeRecord();
    process.exit(0);
  });
}
process.on('exit', removeRecord);
