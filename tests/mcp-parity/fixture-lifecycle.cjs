'use strict';
// The fixed opacity fixture, driven end to end from the advertisement alone:
// discover -> inspect -> edit -> undo -> redo.
//
// schema-acceptance.cjs establishes that a single call is constructible. This
// asks the rest of P16's completion check: that each step is distinguishable,
// that undo actually restores the prior value rather than merely reporting ok,
// and that redo reinstates it. Every body is derived, never written in.
//
// History state is read from the application host, not inferred from the
// server's reply -- an ok response that left the document untouched is exactly
// the failure this is here to catch.

const crypto = require('node:crypto');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { McpClient } = require('./mcp-client.cjs');
const {
  startHost,
  structured,
  deriveFromRegisteredCapabilities,
  deriveFromTemplates,
  deriveFromExamples,
  resolveAgainstSnapshot,
  instanceFieldName,
  binary,
  label,
} = require('./schema-acceptance.cjs');

function derive(payloadSchema, operation) {
  if (payloadSchema === undefined || payloadSchema === true || typeof payloadSchema !== 'object') {
    return null;
  }
  return (
    deriveFromRegisteredCapabilities(payloadSchema, operation) ||
    deriveFromTemplates(payloadSchema, operation) ||
    deriveFromExamples(payloadSchema, operation) ||
    null
  );
}

function opacityOf(inspect, layerId) {
  return inspect.state.layers.find((l) => l.layerUid === layerId).motionStatic.opacity[0];
}

async function main() {
  if (!fs.existsSync(binary)) throw new Error(`missing nemo-mcp binary at ${binary}`);
  console.log(`# fixture lifecycle against: ${label}`);

  const registry = fs.mkdtempSync(path.join(os.tmpdir(), 'nemo-lifecycle-'));
  const host = startHost(registry);
  await host.read();
  const client = new McpClient(binary, [], { NEMO_MCP_REGISTRY: registry });
  await client.initialize();

  const tools = await client.listTools();
  const command = (tools.result?.tools || []).find((t) => t.name === 'nemo_command');
  const payloadSchema = command?.inputSchema?.properties?.payload;

  // --- discover -------------------------------------------------------------
  const discover = structured(await client.callTool('nemo_discover', {}));
  const instanceId = discover.instances?.[0]?.instanceId;
  if (!instanceId) throw new Error('discovery returned no instances');
  console.log(`discover: ${discover.instances.length} instance(s)`);

  // --- inspect --------------------------------------------------------------
  // The instance field is named by the advertisement, not by us: the shipped
  // 66ece06 build requires `instance_id` where current source takes
  // `instanceId`. Hard-coding it here crashed this file against the one binary
  // whose NOT CONSTRUCTIBLE verdict below is the point of running it, turning a
  // clean finding into an indistinguishable harness error.
  const snapshot = structured(
    await client.callTool('nemo_query', {
      [instanceFieldName(tools)]: instanceId,
      operation: 'snapshot',
    }),
  );
  console.log(`inspect: ${JSON.stringify(snapshot.result.layers)}`);

  const editPlan = derive(payloadSchema, 'property.set');
  if (!editPlan) {
    console.log('\nNOT CONSTRUCTIBLE: no derivable body for property.set; lifecycle not attempted.');
    await client.close();
    host.stop();
    fs.rmSync(registry, { recursive: true, force: true });
    console.log(`\nVERDICT ${label}: NOT CONSTRUCTIBLE`);
    process.exitCode = 2;
    return;
  }

  const { resolved } = resolveAgainstSnapshot(editPlan.body, payloadSchema, snapshot);
  const layerId = resolved.layerId;
  const before = await host.control('inspect');
  const original = opacityOf(before, layerId);
  console.log(`\nedit target ${layerId}, currently ${original}, writing ${resolved.value}`);
  if (original === resolved.value) throw new Error('fixture value equals target; no-op would pass');

  let revision = snapshot.revision;
  const send = async (operation, payload) => {
    const reply = structured(
      await client.callTool('nemo_command', {
        apiVersion: 1,
        requestId: crypto.randomUUID(),
        instanceId,
        documentId: snapshot.documentId,
        expectedRevision: revision,
        operation,
        payload,
      }),
    );
    if (reply.ok !== true) throw new Error(`${operation} rejected: ${JSON.stringify(reply)}`);
    revision = reply.revision;
    return reply;
  };

  // --- edit -----------------------------------------------------------------
  await send('property.set', resolved);
  const afterEdit = await host.control('inspect');
  const edited = opacityOf(afterEdit, layerId);
  console.log(
    `edit:  ${original} -> ${edited}  mutations=${afterEdit.mutations} undo=${afterEdit.undoDepth} redo=${afterEdit.redoDepth}`,
  );
  if (edited !== resolved.value) throw new Error(`edit did not land: host shows ${edited}`);

  // --- undo -----------------------------------------------------------------
  const undoPlan = derive(payloadSchema, 'history.undo');
  if (!undoPlan) throw new Error('history.undo is advertised but carries no derivable body');
  await send('history.undo', undoPlan.body);
  const afterUndo = await host.control('inspect');
  const undone = opacityOf(afterUndo, layerId);
  console.log(
    `undo:  ${edited} -> ${undone}  undo=${afterUndo.undoDepth} redo=${afterUndo.redoDepth}`,
  );
  if (undone !== original) {
    throw new Error(`undo reported ok but value is ${undone}, expected ${original}`);
  }

  // --- redo -----------------------------------------------------------------
  const redoPlan = derive(payloadSchema, 'history.redo');
  if (!redoPlan) throw new Error('history.redo is advertised but carries no derivable body');
  await send('history.redo', redoPlan.body);
  const afterRedo = await host.control('inspect');
  const redone = opacityOf(afterRedo, layerId);
  console.log(
    `redo:  ${undone} -> ${redone}  undo=${afterRedo.undoDepth} redo=${afterRedo.redoDepth}`,
  );
  if (redone !== resolved.value) {
    throw new Error(`redo reported ok but value is ${redone}, expected ${resolved.value}`);
  }

  await client.close();
  host.stop();
  fs.rmSync(registry, { recursive: true, force: true });
  console.log(
    `\nLIFECYCLE OK: discover, inspect, edit, undo and redo each distinguished, every body derived from the advertisement.`,
  );
  console.log(`\nVERDICT ${label}: LIFECYCLE OK`);
}

main().catch((error) => {
  console.error('LIFECYCLE FAILED:', error.message);
  process.exit(1);
});
