'use strict';
// A bounded handoff to the real UI operator. Replies contain raw evidence, never
// pass/fail flags. The scenario's original product oracle validates that evidence.
const fs = require('node:fs');
const path = require('node:path');
const { randomUUID } = require('node:crypto');
const { isDeepStrictEqual } = require('node:util');
const { OperatorInputError } = require('../cs7-runner.cjs');
const ID = /^[a-z][a-z0-9-]*$/;
function check(value, message) { if (!value) throw new Error(message); }

function createOperator({ directory, timeout = 180_000, announce = () => {}, interval = 25 }) {
  check(fs.realpathSync(directory) === directory && fs.lstatSync(directory).isDirectory(), 'canonical operator directory required');
  check(Number.isSafeInteger(timeout) && timeout > 0 && timeout <= 1_800_000, 'bounded operator deadline required');
  let closed = false, failed = false, pending = false, ordinal = 0;
  const used = new Set();
  return {
    async request({ row, phase, instruction, identity, details = {}, budgetMs = timeout }) {
      check(!closed && !failed && !pending, 'operator unavailable');
      check(ID.test(row) && ID.test(phase) && typeof instruction === 'string' && instruction.trim(), 'named operator phase required');
      check(Number.isSafeInteger(budgetMs) && budgetMs > 0 && budgetMs <= timeout, 'operator phase budget invalid');
      const key = row + '-' + phase;
      check(!used.has(key), 'operator phase cannot be replayed'); used.add(key); pending = true;
      const requestId = randomUUID(), record = { schema: 'nemo.cs7-operator-request/1', requestId,
        ordinal: ++ordinal, row, phase, instruction, identity: structuredClone(identity), details,
        responseProtocol: 'Write the complete response to a fresh sibling file, then rename it atomically to responseFile.' };
      const stem = path.join(directory, String(ordinal).padStart(3, '0') + '-' + key);
      const requestFile = stem + '.request.json', responseFile = stem + '.response.json';
      try {
        check(!fs.existsSync(responseFile), 'operator reply predates request');
        fs.writeFileSync(requestFile, JSON.stringify(record, null, 2) + '\n', { flag: 'wx', mode: 0o600 });
        announce({ requestFile, responseFile, requestId, row, phase });
        const deadline = performance.now() + budgetMs;
        while (!closed && performance.now() < deadline) {
          if (!fs.existsSync(responseFile)) { await new Promise(resolve => setTimeout(resolve, interval)); continue; }
          const fd = fs.openSync(responseFile, fs.constants.O_RDONLY | fs.constants.O_NOFOLLOW);
          let bytes;
          try {
            const before = fs.fstatSync(fd);
            check(before.isFile() && before.size > 0 && before.size <= 4 * 1024 * 1024, 'operator reply size/type invalid');
            bytes = fs.readFileSync(fd);
            const after = fs.fstatSync(fd);
            check(before.ino === after.ino && before.size === after.size && before.mtimeMs === after.mtimeMs &&
              bytes.length === before.size, 'operator reply changed during read');
          } finally { fs.closeSync(fd); }
          const reply = JSON.parse(bytes);
          check(reply.schema === 'nemo.cs7-operator-response/1' && reply.requestId === requestId &&
            isDeepStrictEqual(reply.identity, record.identity), 'operator reply owner/phase mismatch');
          check(reply.evidence && typeof reply.evidence === 'object' && !Array.isArray(reply.evidence) &&
            !Object.hasOwn(reply, 'pass') && !Object.hasOwn(reply, 'status'), 'raw operator evidence required');
          return reply.evidence;
        }
        throw Error('operator deadline or close');
      } catch {
        failed = true;
        throw new OperatorInputError('operator-evidence-refused', { command: row + '.' + phase, ordinal });
      } finally { pending = false; }
    },
    close() { closed = true; },
  };
}
module.exports = { createOperator };
