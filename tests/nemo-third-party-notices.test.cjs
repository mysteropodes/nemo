'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const { test } = require('node:test');

const ROOT = path.resolve(__dirname, '..');
const GENERATOR = 'scripts/generate-third-party-notices.py';

// Replace only the external scanner boundary, never generator functions or data.
// Real Python child processes supply scanner JSON; an unknown command fails closed.
// This avoids executable shebang/Windows shell differences and package installs.
const SCANNER_HOOK = `
import json
import subprocess
import sys
from pathlib import Path
from unittest.mock import patch

real_run = subprocess.run
fixture = Path(__file__).parent / "scanner.py"

def scan(command, **kwargs):
    if command not in (["npx", "--yes", "license-checker", "--json"],
                       ["cargo", "license", "--json"]):
        raise AssertionError("Unexpected scanner command: " + repr(command))
    return real_run([sys.executable, str(fixture), json.dumps(command)], **kwargs)

patch("subprocess.run", side_effect=scan).start()
`;

const SCANNER = `
import json
import sys
from pathlib import Path

root = Path(__file__).resolve().parent
cwd = Path.cwd().resolve().relative_to(root).as_posix()
command = json.loads(sys.argv[1])
with (root / "scanner-calls.jsonl").open("a", encoding="utf-8") as log:
    log.write(json.dumps({"command": command, "cwd": cwd}) + "\\n")

if command == ["npx", "--yes", "license-checker", "--json"] and cwd == ".":
    data = {
        "nemo@1.0.0": {"licenses": "GPL-3.0-or-later"},
        "fixture-zeta@2.0.0": {"licenses": "MIT"},
        "fixture-alpha@1.0.0": {"licenses": "MIT", "repository": "https://example.invalid/npm"}
    }
elif command == ["cargo", "license", "--json"] and cwd == "src-tauri":
    data = [
        {"name": "nemo_lib", "version": "1.0.0", "license": "GPL-3.0-or-later"},
        {"name": "fixture-tauri", "version": "3.0.0", "license": "Apache-2.0",
         "repository": "https://example.invalid/tauri"}
    ]
elif command == ["cargo", "license", "--json"] and cwd == "geometry-wasm":
    data = [
        {"name": "geometry-wasm", "version": "1.0.0", "license": "GPL-3.0-or-later"},
        {"name": "fixture-wasm", "version": "4.0.0", "license": None}
    ]
else:
    raise AssertionError("Unexpected scanner invocation: " + repr((command, cwd)))
print(json.dumps(data))
`;

function section(markdown, heading) {
  const sections = markdown.split(/^## /m);
  const found = sections.find((part) => part.split('\n', 1)[0] === heading);
  assert.ok(found, `generated artifact contains section: ${heading}`);
  return found;
}

function generateInScratch(t) {
  const scratch = fs.mkdtempSync(path.join(os.tmpdir(), 'nemo notices test '));
  t.after(() => {
    fs.rmSync(scratch, { recursive: true, force: true });
    assert.equal(fs.existsSync(scratch), false, 'scratch fixture is removed');
  });
  for (const dir of ['scripts', 'src-tauri', 'geometry-wasm', 'empty-bin']) {
    fs.mkdirSync(path.join(scratch, dir));
  }
  const original = fs.readFileSync(path.join(ROOT, GENERATOR));
  fs.writeFileSync(path.join(scratch, GENERATOR), original);
  fs.writeFileSync(path.join(scratch, 'sitecustomize.py'), SCANNER_HOOK);
  fs.writeFileSync(path.join(scratch, 'scanner.py'), SCANNER);

  // python3 is already a repository prerequisite (also used by npm run serve).
  const python = spawnSync('python3', ['-c', 'import sys; print(sys.executable)'], {
    encoding: 'utf8', timeout: 10000,
  });
  assert.ifError(python.error);
  assert.equal(python.status, 0, `python3 prerequisite: ${python.stderr}`);
  const result = spawnSync(python.stdout.trim(), [path.join(scratch, GENERATOR)], {
    cwd: scratch,
    encoding: 'utf8',
    timeout: 20000,
    env: {
      ...process.env,
      PATH: path.join(scratch, 'empty-bin'),
      PYTHONPATH: scratch,
      PYTHONDONTWRITEBYTECODE: '1',
      PYTHONIOENCODING: 'utf-8',
      PYTHONNOUSERSITE: '1',
    },
  });
  assert.ifError(result.error);
  assert.equal(result.status, 0, `generator CLI must succeed: ${result.stderr}`);
  assert.equal(result.stderr, '', 'scanner fixtures generate no warnings');
  assert.match(result.stdout, /Wrote /);
  assert.deepEqual(fs.readFileSync(path.join(scratch, GENERATOR)), original);
  assert.deepEqual(fs.readFileSync(path.join(ROOT, GENERATOR)), original);
  return {
    markdown: fs.readFileSync(path.join(scratch, 'THIRD_PARTY_NOTICES.md'), 'utf8'),
    calls: fs.readFileSync(path.join(scratch, 'scanner-calls.jsonl'), 'utf8')
      .trim().split('\n').map((line) => JSON.parse(line)),
  };
}

test('third-party notices CLI preserves current FFmpeg metadata', async (t) => {
  const { markdown, calls } = generateInScratch(t);

  await t.test('scanner and render controls', () => {
    assert.deepEqual(calls, [
      { command: ['npx', '--yes', 'license-checker', '--json'], cwd: '.' },
      { command: ['cargo', 'license', '--json'], cwd: 'src-tauri' },
      { command: ['cargo', 'license', '--json'], cwd: 'geometry-wasm' },
    ]);
    assert.match(markdown, /^# Third-Party Notices\n/);
    assert.match(markdown, /Auto-generated on \d{4}-\d{2}-\d{2}/);
    const npm = section(markdown, 'npm dependencies (2)');
    assert.match(npm, /<strong>MIT<\/strong> \(2\)/);
    assert.match(npm, /`fixture-alpha@1\.0\.0` — https:\/\/example\.invalid\/npm/);
    assert.ok(npm.indexOf('fixture-alpha@') < npm.indexOf('fixture-zeta@'));
    assert.match(section(markdown, 'Rust crates — src-tauri (1)'),
      /`fixture-tauri@3\.0\.0` — https:\/\/example\.invalid\/tauri/);
    assert.match(section(markdown, 'Rust crates — geometry-wasm (1)'),
      /<strong>UNKNOWN<\/strong> \(1\)[\s\S]*`fixture-wasm@4\.0\.0`/);
    assert.doesNotMatch(markdown, /`(?:nemo|nemo_lib|geometry-wasm)@/);
    const vendored = section(markdown, 'Vendored files (not tracked by npm/cargo)');
    for (const name of ['opentype.js', 'MP4Box.js', 'Delaunator']) {
      assert.ok(vendored.includes(`**${name}**`), `${name} attribution is rendered`);
    }
  });

  const bundled = section(markdown, 'Bundled external binaries');
  const ffmpeg = bundled.match(/^- \*\*ffmpeg\*\*[^\n]*(?:\n(?!- |## )[^\n]*)*/im);
  assert.ok(ffmpeg, 'generated artifact includes the FFmpeg entry');
  const notice = ffmpeg[0].replace(/\s+/g, ' ');

  // Metadata consistency with rebuild-ffmpeg-lgpl.sh and THIRD_PARTY_NOTICES.md;
  // these assertions do not establish binary provenance or legal compliance.
  await t.test('FFmpeg entry states LGPL 2.1 or later', () => {
    assert.match(notice, /\bLGPL(?:-|\s+(?:version\s+|v)?)2\.1(?:-or-later|\s+or\s+later|\+)/i);
  });
  await t.test('FFmpeg entry does not claim the obsolete GPL encoder build', () => {
    assert.doesNotMatch(notice, /\bGPL\s*\(built with[^)]*--enable-gpl[^)]*--enable-libx264[^)]*--enable-libx265/i);
  });
  await t.test('FFmpeg entry does not recommend the obsolete decode-only replacement', () => {
    assert.doesNotMatch(notice, /(?:binary|ffmpeg) should be replaced[^.]*LGPL[^.]*decode-only/i);
  });
});
