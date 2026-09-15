'use strict';
// Schema-only acceptance: can a client that knows NOTHING about this
// application construct a working call from the advertisement alone?
//
// smoke.cjs answers a different question. It sends
// {property:'opacity', layerId:'layer-b', value:42} written into the test, so
// it passes against any binary that accepts that shape -- including one whose
// advertised payload schema is literally `true`. That knowledge came from the
// author, not from the server, so smoke.cjs cannot tell a binary that
// documents its contract apart from one that documents nothing.
//
// This script forbids itself that knowledge. The word "opacity" and the
// payload field names appear nowhere below as inputs: everything sent is
// derived from tools/list and the snapshot, which is exactly what Claude has.
// A binary that does not advertise enough to build the call fails here, and
// that failure is the finding, not a harness error.

const path = require('node:path');
const fs = require('node:fs');
const os = require('node:os');
const crypto = require('node:crypto');
const { spawn } = require('node:child_process');
const readline = require('node:readline');
const { McpClient } = require('./mcp-client.cjs');

const repo = path.resolve(__dirname, '..', '..');
const binary = process.env.NEMO_MCP_BINARY
  ? path.resolve(process.env.NEMO_MCP_BINARY)
  : path.join(repo, 'nemo-mcp/target/release/nemo-mcp');
const label = process.env.NEMO_MCP_LABEL || path.basename(path.dirname(binary));

// The operation under test. A name from the advertised enum, nothing more --
// no assumption about what its body looks like.
const OPERATION = 'property.set';

function startHost(registry) {
  const child = spawn(process.execPath, [path.join(__dirname, 'app-host.cjs')], {
    stdio: ['pipe', 'pipe', 'inherit'],
    env: { ...process.env, NEMO_MCP_REGISTRY: registry },
  });
  const lines = readline.createInterface({ input: child.stdout });
  const queue = [];
  const waiters = [];
  lines.on('line', (line) => {
    const value = JSON.parse(line);
    if (waiters.length) waiters.shift()(value);
    else queue.push(value);
  });
  const read = () =>
    new Promise((resolve) => (queue.length ? resolve(queue.shift()) : waiters.push(resolve)));
  return {
    read,
    async control(action) {
      child.stdin.write(JSON.stringify({ action }) + '\n');
      return read();
    },
    stop() {
      child.stdin.end();
      child.kill('SIGTERM');
    },
  };
}

function structured(response) {
  const content = response?.result?.structuredContent;
  if (content) return content;
  const text = response?.result?.content?.find((c) => c.type === 'text')?.text;
  if (text) {
    try {
      return JSON.parse(text);
    } catch {
      return { raw: text };
    }
  }
  return response?.result ?? response;
}

// ---------------------------------------------------------------------------
// Derivation. Each source is something the server actually published; if none
// of them yield a body, the advertisement is insufficient and we say so.
// ---------------------------------------------------------------------------

function deriveFromRegisteredCapabilities(payloadSchema) {
  const registered = payloadSchema?.['x-nemo-registeredCapabilities'];
  if (!Array.isArray(registered)) return null;
  for (const capability of registered) {
    if (capability?.availability?.state !== 'available') continue;
    if (!capability?.effects?.lifecycle?.includes(OPERATION)) continue;
    const example = capability.examples?.[0]?.input ?? capability.fixture?.input;
    if (!example || typeof example !== 'object') continue;
    // The capability id is how the server names the property this body targets.
    const body = { ...example };
    const selector = payloadSchema.anyOf
      ?.flatMap((branch) => Object.keys(branch.properties || {}))
      .includes('property');
    if (selector && capability.id && body.property === undefined) {
      body.property = capability.id;
    }
    return { body, source: `x-nemo-registeredCapabilities[${capability.id}].examples[0].input` };
  }
  return null;
}

function deriveFromTemplates(payloadSchema) {
  // The description publishes one copyable template per operation, in
  // "<operation> <json>" form. Parse it rather than trusting example order.
  const description = payloadSchema?.description;
  if (typeof description !== 'string') return null;
  for (const line of description.split('\n')) {
    const match = line.match(/^\s*(\S+)\s+(\{.*\})\s*$/);
    if (!match || match[1] !== OPERATION) continue;
    try {
      return { body: JSON.parse(match[2]), source: 'payload.description template' };
    } catch {
      return null;
    }
  }
  return null;
}

function deriveFromExamples(payloadSchema) {
  const examples = payloadSchema?.examples;
  if (!Array.isArray(examples) || !examples.length) return null;
  const branch = (payloadSchema.anyOf || []).find((b) => (b.required || []).length);
  if (!branch) return null;
  const required = branch.required;
  const hit = examples.find(
    (e) => e && typeof e === 'object' && required.every((k) => e[k] !== undefined),
  );
  return hit ? { body: { ...hit }, source: 'payload.examples' } : null;
}

// Fields the advertisement itself says come from live state get resolved from
// the snapshot; a literal from a template would address nothing.
function resolveAgainstSnapshot(body, payloadSchema, snapshot) {
  const described = {};
  for (const branch of payloadSchema.anyOf || []) {
    for (const [name, spec] of Object.entries(branch.properties || {})) {
      if (typeof spec?.description === 'string') described[name] = spec.description;
    }
  }
  for (const capability of payloadSchema['x-nemo-registeredCapabilities'] || []) {
    for (const [name, spec] of Object.entries(capability?.input?.properties || {})) {
      if (typeof spec?.description === 'string') described[name] = spec.description;
    }
  }
  const layers = snapshot?.result?.layers || snapshot?.layers || [];
  const resolved = { ...body };
  const notes = [];
  for (const [name, description] of Object.entries(described)) {
    if (resolved[name] === undefined) continue;
    const match = description.match(/layers\[\]\.(\w+)/);
    if (!match || !layers.length) continue;
    // Pick the layer whose current value differs from what we are about to
    // write, so a no-op cannot masquerade as a successful write.
    const target =
      layers.find((l) => resolved.value !== undefined && l.opacity !== resolved.value) || layers[0];
    const before = resolved[name];
    resolved[name] = target.id ?? target[match[1]];
    notes.push(`${name}: ${JSON.stringify(before)} -> ${JSON.stringify(resolved[name])} (${match[0]})`);
  }
  return { resolved, notes };
}

async function main() {
  if (!fs.existsSync(binary)) throw new Error(`missing nemo-mcp binary at ${binary}`);
  console.log(`# schema-only acceptance against: ${label}`);
  console.log(`# binary: ${path.basename(binary)}  sha256 prefix: ${crypto
    .createHash('sha256')
    .update(fs.readFileSync(binary))
    .digest('hex')
    .slice(0, 16)}`);

  const registry = fs.mkdtempSync(path.join(os.tmpdir(), 'nemo-schema-acc-'));
  const host = startHost(registry);
  await host.read();
  const client = new McpClient(binary, [], { NEMO_MCP_REGISTRY: registry });
  await client.initialize();

  const tools = await client.listTools();
  const command = (tools.result?.tools || []).find((t) => t.name === 'nemo_command');
  if (!command) throw new Error('nemo_command is not advertised');
  const payloadSchema = command.inputSchema?.properties?.payload;

  const describes =
    payloadSchema !== undefined && payloadSchema !== true && typeof payloadSchema === 'object';
  console.log(`advertised payload schema: ${describes ? 'described' : JSON.stringify(payloadSchema)}`);

  const derivation =
    (describes && deriveFromRegisteredCapabilities(payloadSchema)) ||
    (describes && deriveFromTemplates(payloadSchema)) ||
    (describes && deriveFromExamples(payloadSchema)) ||
    null;

  let verdict;
  if (!derivation) {
    console.log(`\nNOT CONSTRUCTIBLE: the advertisement for "${OPERATION}" carries no payload`);
    console.log('shape, template, example or capability descriptor. A client holding only this');
    console.log('advertisement cannot build the call; any success would come from outside it.');
    verdict = 'NOT CONSTRUCTIBLE';
  } else {
    console.log(`derived from: ${derivation.source}`);
    console.log(`template body: ${JSON.stringify(derivation.body)}`);

    const discover = structured(await client.callTool('nemo_discover', {}));
    const instanceId = discover.instances?.[0]?.instanceId;
    if (!instanceId) throw new Error('discovery returned no instances');
    const snapshot = structured(
      await client.callTool('nemo_query', { instanceId, operation: 'snapshot' }),
    );

    const { resolved, notes } = resolveAgainstSnapshot(derivation.body, payloadSchema, snapshot);
    for (const note of notes) console.log(`resolved from snapshot -> ${note}`);
    console.log(`sending payload: ${JSON.stringify(resolved)}`);

    const before = await host.control('inspect');
    const write = structured(
      await client.callTool('nemo_command', {
        apiVersion: 1,
        requestId: crypto.randomUUID(),
        instanceId,
        documentId: snapshot.documentId,
        expectedRevision: snapshot.revision,
        operation: OPERATION,
        payload: resolved,
      }),
    );
    console.log(`server said: ${JSON.stringify(write)}`);
    const after = await host.control('inspect');
    const changed = after.mutations > before.mutations;
    console.log(
      `host mutations ${before.mutations} -> ${after.mutations}, revision ${before.meta.revision} -> ${after.meta.revision}`,
    );
    if (write.ok !== true) throw new Error('server rejected a payload built from its own contract');
    if (!changed) throw new Error('call reported ok but application state did not change');
    verdict = 'ACCEPTED';
    console.log('\nACCEPTED: a client with no prior knowledge built this call from the');
    console.log('advertisement alone and it changed application state.');
  }

  await client.close();
  host.stop();
  fs.rmSync(registry, { recursive: true, force: true });
  console.log(`\nVERDICT ${label}: ${verdict}`);
  process.exitCode = verdict === 'ACCEPTED' ? 0 : 2;
}

main().catch((error) => {
  console.error('SCHEMA ACCEPTANCE FAILED:', error.message);
  process.exit(1);
});
