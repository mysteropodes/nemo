'use strict';
// Minimal MCP stdio client for R14 Lane F.
//
// Purpose: give the parity harness a *middle* measurement point. The three
// paths under comparison are
//
//   oracle       TCP -> application host                  (no MCP, no client)
//   direct MCP   this client -> nemo-mcp -> host          (MCP transport only)
//   client       Codex / Claude Code -> nemo-mcp -> host  (full client path)
//
// Diffing client against oracle is what the R14 gate asks for. Having direct
// MCP in between is what makes a divergence *attributable*: if client and
// direct MCP agree but both differ from the oracle, the transport is
// responsible; if direct MCP matches the oracle and only the client differs,
// the client path is responsible. Without the middle point a divergence is
// just a mystery.
//
// This is intentionally hand-rolled JSON-RPC 2.0 over newline-delimited stdio
// rather than an SDK: an SDK could normalise away exactly the wire-level
// differences this harness exists to detect.

const { spawn } = require('node:child_process');
const readline = require('node:readline');

class McpClient {
  constructor(command, args, env) {
    this.child = spawn(command, args, {
      stdio: ['pipe', 'pipe', 'pipe'],
      env: { ...process.env, ...env },
    });
    this.nextId = 1;
    this.pending = new Map();
    this.stderr = '';
    this.transcript = [];
    this.child.stderr.on('data', (b) => {
      this.stderr += b.toString();
    });
    this.rl = readline.createInterface({ input: this.child.stdout });
    this.rl.on('line', (line) => {
      const trimmed = line.trim();
      if (!trimmed) return;
      let message;
      try {
        message = JSON.parse(trimmed);
      } catch {
        return; // not a protocol frame
      }
      this.transcript.push({ dir: 'in', at: Date.now(), message });
      if (message.id !== undefined && this.pending.has(message.id)) {
        const { resolve } = this.pending.get(message.id);
        this.pending.delete(message.id);
        resolve(message);
      }
    });
    this.exited = new Promise((resolve) => {
      this.child.on('exit', (code, signal) => resolve({ code, signal }));
    });
  }

  send(method, params) {
    const id = this.nextId++;
    const frame = { jsonrpc: '2.0', id, method, params };
    this.transcript.push({ dir: 'out', at: Date.now(), message: frame });
    const done = new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      setTimeout(() => {
        if (this.pending.has(id)) {
          this.pending.delete(id);
          reject(new Error(`timeout waiting for ${method}`));
        }
      }, 30000);
    });
    this.child.stdin.write(JSON.stringify(frame) + '\n');
    return done;
  }

  notify(method, params) {
    const frame = { jsonrpc: '2.0', method, params };
    this.transcript.push({ dir: 'out', at: Date.now(), message: frame });
    this.child.stdin.write(JSON.stringify(frame) + '\n');
  }

  async initialize() {
    const response = await this.send('initialize', {
      protocolVersion: '2024-11-05',
      capabilities: {},
      clientInfo: { name: 'nemo-r14-parity', version: '1.0.0' },
    });
    this.notify('notifications/initialized', {});
    return response;
  }

  async listTools() {
    return this.send('tools/list', {});
  }

  async callTool(name, args) {
    return this.send('tools/call', { name, arguments: args || {} });
  }

  async close() {
    try {
      this.child.stdin.end();
    } catch {
      /* already closed */
    }
    const timer = setTimeout(() => this.child.kill('SIGKILL'), 3000);
    const result = await this.exited;
    clearTimeout(timer);
    return result;
  }
}

module.exports = { McpClient };
