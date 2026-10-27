'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const os = require('node:os');
const { createInput } = require('../cs7-session.cjs');

// Real helper subprocess/pipe/exit lifecycle. It deliberately has no AX or Nemo access.
function fixture(t, mode) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'nemo-cs7-pipe-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const executable = path.join(root, 'helper');
  fs.writeFileSync(executable, `#!${process.execPath}
const mode=process.env.CS7_TEST_MODE,pid=Number(process.argv[3]);
const send=x=>process.stdout.write(JSON.stringify(x)+'\\n');
send({status:'pinned',purpose:'UI32',pid,windowId:17,matchedAX:true,processStartSeconds:10,processStartMicroseconds:20});
let ordinal=0;
const input=require('node:readline').createInterface({input:process.stdin});
input.on('line',line=>{
 const r=JSON.parse(line); ordinal++;
 if(mode==='refuse'){send({status:'refused',actionDiagnostics:{private:'never publish'}});return;}
 if(mode==='timeout')return;
 if(mode==='malformed'){process.stdout.write('not JSON\\n');return;}
 if(mode==='oversized'){process.stdout.write('x'.repeat(1024*1024+1));return;}
 send({...r,status:'ok',ordinal:mode==='stale'?ordinal-1:ordinal,pid:mode==='foreign'?pid+1:pid});
});
input.on('close',()=>process.exit(0));
`, { mode: 0o700 });
  return createInput({ executable, env: { ...process.env, CS7_TEST_MODE: mode }, timeout: mode === 'timeout' ? 100 : 5000 });
}
test('retained helper identity and ordered replies work across multiple commands', async t => {
  const input = fixture(t, 'normal');
  try {
    assert.equal((await input.open(77)).windowId, 17);
    assert.equal((await input.command('observe')).ordinal, 1);
    assert.equal((await input.command('motion.focus', 'search')).ordinal, 2);
    await assert.rejects(input.open(77), /already started/);
  } finally { await input.close(); }
});
test('refusal, timeout, malformed, oversized, stale and foreign replies are terminal with no replay', async t => {
  for (const mode of ['refuse', 'timeout', 'malformed', 'oversized', 'stale', 'foreign']) {
    await t.test(mode, async t => {
      const input = fixture(t, mode);
      try {
        await input.open(77);
        await assert.rejects(input.command('observe'), /input-refused/);
        await assert.rejects(input.command('observe'), /unavailable/);
      } finally { await input.close(); }
    });
  }
});
test('concurrent input and oversized requests are refused before another write', async t => {
  const input = fixture(t, 'normal');
  try {
    await input.open(77);
    await assert.rejects(input.command('observe', 'x'.repeat(2048)), /too large/);
    const first = input.command('observe');
    await assert.rejects(input.command('observe'), /unavailable/);
    assert.equal((await first).ordinal, 1);
  } finally { await input.close(); }
});
