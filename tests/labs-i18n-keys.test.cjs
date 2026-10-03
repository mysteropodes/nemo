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
const vm = require('node:vm');

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
  // Labs spec labels and extra hints are key names resolved later through SM.t.
  for (const match of source.matchAll(/(?:label|hintExtra):\s*'(labs[A-Za-z0-9_]*)'/g)) keys.add(match[1]);
  return keys;
}

function missingReferences(sources, bodies) {
  const missing = [];
  for (const [file, source] of sources) {
    for (const key of referencedKeys(source)) {
      const absent = bodies.filter(({ body }) => !declares(body, key)).map(({ locale }) => locale);
      if (absent.length) missing.push(`${file}: ${key} missing from ${absent.join(', ')}`);
    }
  }
  return missing;
}

function labsSources() {
  const dir = path.join(ROOT, 'src/js/labs');
  return fs.readdirSync(dir).filter((name) => name.endsWith('.js'))
    .map((file) => [file, fs.readFileSync(path.join(dir, file), 'utf8')]);
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
  const missing = missingReferences(labsSources(), bodies);
  assert.deepEqual(missing, [], `Labs panels reference i18n keys that do not exist:\n  ${missing.join('\n  ')}`);
});

test('the scanner finds deferred Labs labels and hints, and a one-locale deletion fails', () => {
  const sources = labsSources();
  const panel = sources.find(([file]) => file === 'labs-panel.js');
  assert.ok(panel, 'Labs settings panel is part of the scanned source set');
  assert.ok(referencedKeys(panel[1]).has('labsPanelLabelDiagnostics'));
  assert.ok(referencedKeys(panel[1]).has('labsPanelHintFrenchCurve'));
  assert.ok(!referencedKeys("{ value: '1', label: 'Ones (1)' }").has('Ones'));

  const bodies = localeBodies();
  const mutated = bodies.map(({ locale, body }) => ({
    locale,
    body: locale === 'ja' ? body.replace(/labsPanelLabelDiagnostics\s*:\s*'[^']*',/, '') : body,
  }));
  assert.deepEqual(missingReferences(sources, mutated).filter((entry) => entry.includes('labsPanelLabelDiagnostics')),
    ['labs-panel.js: labsPanelLabelDiagnostics missing from ja']);
});

test('the diagnostics panel keys specifically resolve to real text, not to their own name', () => {
  const bodies = localeBodies();
  const keys = ['labsDiagnosticsTitle', 'labsDiagnosticsRefresh', 'labsDiagnosticsReport',
    'labsDiagnosticsEmpty', 'labsDescribeDiagnosticsPanel', 'labsPanelLabelDiagnostics'];
  for (const { locale, body } of bodies) {
    for (const key of keys) {
      assert.ok(declares(body, key), `${key} is missing from the ${locale} block`);
      const value = new RegExp(`(?:^|[{,\\s])${key}\\s*:\\s*'([^']*)'`).exec(body);
      assert.ok(value && value[1].trim(), `${key} in ${locale} must have non-empty text`);
      assert.notEqual(value[1], key, `${key} in ${locale} must not be its own name`);
    }
  }
});

test('production SM.t resolves all six diagnostics keys in every locale', () => {
  const source = fs.readFileSync(path.join(ROOT, 'src/js/i18n.js'), 'utf8');
  const state = { language: 'en' };
  const window = { state };
  const document = {
    readyState: 'loading',
    addEventListener() {},
    querySelectorAll() { return []; },
    getElementById() { return null; },
  };
  vm.runInNewContext(source, { window, state, document, localStorage: { setItem() {} } },
    { filename: 'src/js/i18n.js' });
  const keys = ['labsDiagnosticsTitle', 'labsDiagnosticsRefresh', 'labsDiagnosticsReport',
    'labsDiagnosticsEmpty', 'labsDescribeDiagnosticsPanel', 'labsPanelLabelDiagnostics'];
  // T10 leaves Report unavailable until a native recording with a recoverable
  // fixture exists; the Labs tooltip must not promise a bundle meanwhile.
  const unavailable = {
    en: /Report is currently unavailable/,
    fr: /rapport est actuellement indisponible/,
    ja: /レポートは現在利用できません/,
    es: /informe no está disponible actualmente/,
  };
  for (const locale of LOCALES) {
    window.SM.setLanguage(locale);
    for (const key of keys) {
      const value = window.SM.t(key);
      assert.ok(typeof value === 'string' && value.trim(), `${locale}: ${key} has text`);
      assert.notEqual(value, key, `${locale}: ${key} did not fall through to the raw key`);
    }
    assert.match(window.SM.t('labsDescribeDiagnosticsPanel'), unavailable[locale],
      `${locale}: diagnostics tooltip must describe Report as unavailable`);
  }
});
