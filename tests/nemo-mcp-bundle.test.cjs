const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

const config = JSON.parse(fs.readFileSync(path.join(__dirname, '..', 'src-tauri', 'tauri.conf.json')));
const builder = fs.readFileSync(path.join(__dirname, '..', 'scripts', 'build-mcp-sidecar.cjs'), 'utf8');

test('Tauri build runs the locked MCP sidecar builder and bundles its external binary', () => {
  assert.match(config.build.beforeBuildCommand, /assert-no-private-labs\.py/);
  assert.match(config.build.beforeBuildCommand, /build-mcp-sidecar\.cjs/);
  assert.deepEqual(config.bundle.externalBin, ['binaries/ffmpeg', 'binaries/nemo-mcp']);
});

test('MCP sidecar builder uses locked release Cargo and target-specific staging', () => {
  assert.match(builder, /cargo', \['build', '--locked', '--release'/);
  assert.match(builder, /--target', target/);
  assert.match(builder, /fs\.rmSync\(staged, \{ force: true \}\)/);
  assert.match(builder, /nemo-mcp-\$\{target\}/);
  assert.match(builder, /\.exe/);
});
