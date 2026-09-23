'use strict';
// T11/#1404 -- every i18n key a Labs panel asks for must exist in EVERY locale.
//
// The failure this prevents is silent by construction: SM.t falls back to
// returning the key itself, so a missing entry renders the raw key name as UI
// text instead of throwing. T08 shipped a panel referencing five keys, none of
// which existed in i18n.js, and nothing anywhere noticed. CLAUDE.md section 13
// already records the same escape for the Guide layer.
//
// Checking all four locale blocks rather than only `en` is the point: adding
// to `en` alone reproduces the identical silent gap in fr/ja/es.

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

const ROOT = path.resolve(__dirname, '..');
const LOCALES = ['en', 'fr', 'ja', 'es'];

function localeBodies() {
  const source = fs.readFileSync(path.join(ROOT, 'src/js/i18n.js'), 'utf8');
  const marks = LOCALES.map((locale) => {
    const at = source.indexOf(`\n    ${locale}:{`);
    assert.notEqual(at, -1, `i18n.js no longer declares a "${locale}" block the way this check expects`);
    return { locale, at };
  });
  return marks.map((mark, index) => ({
    locale: mark.locale,
    body: source.slice(mark.at, index + 1 < marks.length ? marks[index + 1].at : source.length),
  }));
}

// A key is declared when it appears as `key:` at an object-entry boundary.
// The leading class matters: these blocks are enormous single lines, so a bare
// substring match would also accept a key that merely ENDS with this name.
function declares(body, key) {
  return new RegExp(`(?:^|[{,\\s])${key}\\s*:`).test(body);
}

function referencedKeys(source) {
  const keys = new Set();
  for (const match of source.matchAll(/(?:SM\.t|[^a-zA-Z_]t2?)\(\s*'([a-zA-Z][a-zA-Z0-9_]*)'\s*\)/g)) keys.add(match[1]);
  for (const match of source.matchAll(/describe:\s*'([a-zA-Z][a-zA-Z0-9_]*)'/g)) keys.add(match[1]);
  return keys;
}

test('the check itself detects a declared key, so a green result is not vacuous', () => {
  const bodies = localeBodies();
  assert.equal(bodies.length, 4, 'all four locale blocks must be found');
  for (const { locale, body } of bodies) {
    assert.ok(declares(body, 'labsToastEnabledWord'), `${locale} declares a known pre-existing labs key`);
    assert.equal(declares(body, 'labsKeyThatDoesNotExistAnywhere'), false, `${locale} must not report a missing key as present`);
  }
});

test('every i18n key referenced by a Labs panel is declared in all four locales', () => {
  const bodies = localeBodies();
  const dir = path.join(ROOT, 'src/js/labs');
  const missing = [];
  for (const file of fs.readdirSync(dir).filter((name) => name.endsWith('.js'))) {
    const source = fs.readFileSync(path.join(dir, file), 'utf8');
    for (const key of referencedKeys(source)) {
      const absent = bodies.filter(({ body }) => !declares(body, key)).map(({ locale }) => locale);
      if (absent.length) missing.push(`${file}: ${key} missing from ${absent.join(', ')}`);
    }
  }
  assert.deepEqual(missing, [], `Labs panels reference i18n keys that do not exist:\n  ${missing.join('\n  ')}`);
});

test('the diagnostics panel keys specifically resolve to real text, not to their own name', () => {
  const bodies = localeBodies();
  const keys = ['labsDiagnosticsTitle', 'labsDiagnosticsRefresh', 'labsDiagnosticsReport',
    'labsDiagnosticsEmpty', 'labsDescribeDiagnosticsPanel'];
  for (const { locale, body } of bodies) {
    for (const key of keys) {
      assert.ok(declares(body, key), `${key} is missing from the ${locale} block`);
      const value = new RegExp(`(?:^|[{,\\s])${key}\\s*:\\s*'([^']*)'`).exec(body);
      assert.ok(value && value[1].trim(), `${key} in ${locale} must have non-empty text`);
      assert.notEqual(value[1], key, `${key} in ${locale} must not be its own name`);
    }
  }
});
