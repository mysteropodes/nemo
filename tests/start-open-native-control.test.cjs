'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

const root = path.resolve(__dirname, '..');
const html = fs.readFileSync(path.join(root, 'src/index.html'), 'utf8');
const css = fs.readFileSync(path.join(root, 'src/css/style.css'), 'utf8');
const project = fs.readFileSync(path.join(root, 'src/js/project.js'), 'utf8');
const entry = fs.readFileSync(path.join(root, 'src/js/project-entry.js'), 'utf8');
const i18n = fs.readFileSync(path.join(root, 'src/js/i18n.js'), 'utf8');

function openCard() {
  const card = html.match(/<button\b([^>]*\bid="start-open"[^>]*)>([\s\S]*?)<\/button>/);
  assert.ok(card, 'Open Project must be a native button, not a clickable div');
  assert.equal((html.match(/\bid="start-open"/g) || []).length, 1);
  return { attributes: card[1], children: card[2] };
}

test('Open Project uses a named native button with legal, localized children', () => {
  const { attributes, children } = openCard();
  assert.match(attributes, /\btype="button"/);
  assert.match(attributes, /\bclass="start-card"/);
  assert.match(attributes, /\baria-labelledby="start-open-title"/);
  assert.match(attributes, /\baria-describedby="start-open-sub"/);
  assert.match(children, /<span\b[^>]*\bclass="start-card-ico"[^>]*\baria-hidden="true"/);
  assert.match(children, /<span\b[^>]*\bid="start-open-title"[^>]*\bdata-i18n="startOpenTitle"/);
  assert.match(children, /<span\b[^>]*\bid="start-open-sub"[^>]*\bdata-i18n="startOpenSub"/);
  assert.doesNotMatch(children, /<(?:div|button|input|select|textarea)\b/i);
  assert.equal((i18n.match(/\bstartOpenTitle:/g) || []).length, 4);
  assert.equal((i18n.match(/\bstartOpenSub:/g) || []).length, 4);
});

test('the existing ordinary Open callback remains the sole start-card click binding', () => {
  assert.equal((project.match(/SMProjectEntry\.bindStartScreen\(document,/g) || []).length, 1);
  assert.match(project, /resume:resumeAutosave,open:openDialog/);
  assert.equal((entry.match(/\.addEventListener\('click',ports\.open\)/g) || []).length, 1);
  assert.doesNotMatch(project, /startOpen\.addEventListener/);
  // The companion shipped presenter tests execute keyboard handlers and prove
  // default activation is neither cancelled nor synthesized at this boundary.
  assert.doesNotMatch(entry, /open\.addEventListener\('key(?:down|up)',[^)]*preventDefault/);
});

test('the button keeps the card layout and an explicit keyboard focus cue', () => {
  assert.match(css, /#start-open\{[^}]*appearance:none;[^}]*border:0;[^}]*box-sizing:border-box;[^}]*font:inherit;[^}]*\}/);
  assert.match(css, /#start-open:focus-visible\{[^}]*outline:[^}]+\}/);
});
