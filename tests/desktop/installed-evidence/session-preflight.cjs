'use strict';
// Pre-flight for every runner that starts the packaged app (run.cjs through candidate.cjs, f03-clients.cjs through candidate.cjs).
// A locked macOS login session (or a covered window) stops the Nemo window from presenting; the app then never completes its first
// command (project.new), and the runner would only time out on it after the launch. The lock is readable beforehand: the console user
// record of `ioreg -n Root -d1` carries CGSSessionScreenIsLocked = Yes while the screen is locked (observed on this Mac:
// `"CGSSessionScreenIsLocked"=Yes` in the IOConsoleUsers entry; `-a` prints it as `<key>CGSSessionScreenIsLocked</key><true/>`), and the
// key is absent when the session is unlocked. Both spellings are read. Only a positive reading refuses: a host without ioreg, or one
// that cannot be read, is reported as unprobed and does not stop a run, because the first-command timeout still names the cause
// (wire.cjs, FIRST_COMMAND_TIMEOUT).
const { execFileSync } = require('node:child_process');
const { requireCheck } = require('../native-harness.cjs');

const LOCKED_TEXT = /"CGSSessionScreenIsLocked"\s*=\s*(Yes|1|true)\b/i;
const LOCKED_XML = /<key>CGSSessionScreenIsLocked<\/key>\s*<true\s*\/>/i;
// The words of the refusal (a plain sentence: run.cjs echoes only short plain refusals).
const LOCKED_REASON = 'the macOS login session is locked (CGSSessionScreenIsLocked): a locked or covered Nemo window stops presenting and project.new never completes; unlock the Mac and run again';
// The words of the timeout of the first command after launch.
const FIRST_COMMAND_TIMEOUT = 'the first command after launch timed out: the Nemo window is probably not presenting (Mac locked or window covered)';

const readIoreg = () => execFileSync('ioreg', ['-n', 'Root', '-d1'], { encoding: 'utf8', timeout: 10_000, stdio: ['ignore', 'pipe', 'ignore'] });

// { locked, probed }: `read` returns the ioreg output (injected in tests).
function sessionState({ platform = process.platform, read = readIoreg } = {}) {
  if (platform !== 'darwin') return { locked: false, probed: false };
  let text;
  try { text = String(read()); } catch { return { locked: false, probed: false }; }
  return { locked: LOCKED_TEXT.test(text) || LOCKED_XML.test(text), probed: true };
}

// Refuses (a check refusal, which run.cjs reports as its reason with exit status refused) when the session is locked.
function requireUnlockedSession(deps = {}) {
  requireCheck(!sessionState(deps).locked, LOCKED_REASON);
}

module.exports = { LOCKED_REASON, FIRST_COMMAND_TIMEOUT, sessionState, requireUnlockedSession };
