'use strict';
// #2269: Resume and Recent-project refusals toast, but the toast sat at z-index 100 under the start screen (500), so
// the owner saw "nothing happens". A toast is non-interactive and must be above every overlay, the start screen first.
const assert=require('node:assert/strict');
const fs=require('node:fs');
const test=require('node:test');
const css=fs.readFileSync('src/css/style.css','utf8');
const html=fs.readFileSync('src/index.html','utf8');
function zIndex(selector){
  const escaped=selector.replace(/[#.]/g,'\\$&');
  const rule=new RegExp('(?:^|\\n)'+escaped+'\\s*\\{([^}]*)\\}').exec(css);
  assert.ok(rule,selector+' rule');
  const z=/z-index\s*:\s*(\d+)/.exec(rule[1]);
  assert.ok(z,selector+' z-index');
  return Number(z[1]);
}
test('the toast is above the start screen so a refused Resume or Recent entry is visible',()=>{
  assert.ok(zIndex('#toast')>zIndex('#start-screen'));
  assert.match(css,/#toast\{[^}]*pointer-events:none/,'the toast never intercepts clicks on the start screen');
});
test('the toast is above every other stacked layer in the stylesheet and the page',()=>{
  const toast=zIndex('#toast');
  const all=[...(css+html).matchAll(/z-index\s*:\s*(\d+)/g)].map(m=>Number(m[1]));
  const others=all.filter(z=>z!==toast);
  assert.ok(others.length>0);
  assert.ok(others.every(z=>z<toast),'highest other layer '+Math.max(...others)+' vs toast '+toast);
});
