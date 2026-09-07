'use strict';
// Chain smoke test: application host -> nemo-mcp -> MCP client.
//
// The single question this answers is the one every prior R14 evidence run
// left open: does nemo_discover return a NON-EMPTY instance list, and can a
// query and a write actually reach a live application through the compiled
// MCP binary? Every recorded client attempt so far returned
// {"apiVersion":1,"instances":[]}, which proves the binary launched and
// nothing else.

const path = require('node:path');
const fs = require('node:fs');
const os = require('node:os');
const crypto = require('node:crypto');
const { spawn } = require('node:child_process');
const readline = require('node:readline');
const { McpClient } = require('./mcp-client.cjs');

const repo = path.resolve(__dirname, '..', '..');
const binary = path.join(repo, 'nemo-mcp/target/release/nemo-mcp');

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
    child,
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

async function main() {
  if (!fs.existsSync(binary)) throw new Error(`missing nemo-mcp binary at ${binary}`);
  const registry = fs.mkdtempSync(path.join(os.tmpdir(), 'nemo-parity-reg-'));
  const host = startHost(registry);
  const ready = await host.read();
  console.log('host ready:', JSON.stringify(ready));

  const client = new McpClient(binary, [], { NEMO_MCP_REGISTRY: registry });
  const init = await client.initialize();
  console.log('serverInfo:', JSON.stringify(init.result?.serverInfo));

  const tools = await client.listTools();
  console.log('tools:', (tools.result?.tools || []).map((t) => t.name).join(', '));

  const discover = structured(await client.callTool('nemo_discover', {}));
  console.log('discover:', JSON.stringify(discover));
  const instances = discover.instances || [];
  console.log(
    instances.length ? `DISCOVER NON-EMPTY: ${instances.length} instance(s)` : 'DISCOVER EMPTY',
  );
  if (!instances.length) throw new Error('discovery returned no instances');

  const instanceId = instances[0].instanceId;
  const snapshot = structured(
    await client.callTool('nemo_query', { instance_id: instanceId, operation: 'snapshot' }),
  );
  console.log('snapshot:', JSON.stringify(snapshot));

  const write = structured(
    await client.callTool('nemo_command', {
      apiVersion: 1,
      requestId: crypto.randomUUID(),
      instanceId,
      documentId: snapshot.documentId,
      expectedRevision: snapshot.revision,
      operation: 'property.set',
      payload: { property: 'opacity', layerId: 'layer-b', value: 42 },
    }),
  );
  console.log('property.set:', JSON.stringify(write));

  const after = await host.control('inspect');
  const opacity = after.state.layers.find((l) => l.layerUid === 'layer-b').motionStatic.opacity[0];
  console.log(`layer-b opacity now ${opacity}; mutations=${after.mutations} rev=${after.meta.revision}`);
  if (opacity !== 42) throw new Error(`expected 42, host shows ${opacity}`);

  await client.close();
  host.stop();
  fs.rmSync(registry, { recursive: true, force: true });
  console.log('\nCHAIN OK: a write crossed compiled MCP into a live application and changed state.');
}

main().catch((error) => {
  console.error('SMOKE FAILED:', error.message);
  process.exit(1);
});
