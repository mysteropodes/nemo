'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');

function referenceLesson() {
  const window = {};
  vm.runInNewContext(fs.readFileSync('src/js/adapters/tutorial-presentation.js', 'utf8') + '\n' +
    fs.readFileSync('src/js/tutorial.js', 'utf8'), {
    window, document: { addEventListener() {} },
  });
  return window.SMTutorial.MODULES.find(m => m.id === 'reference');
}

test('Reference is an unavailable notice, never an actionable hidden-control lesson', () => {
  const lesson = referenceLesson();
  assert.equal(lesson.unavailable, true);
  assert.equal(lesson.category, 'Médias');
  assert.equal(lesson.time, 'Indisponible');
  assert.equal(lesson.steps.length, 1);
  assert.equal(lesson.steps[0].type, 'info');
  assert.equal(lesson.steps[0].title, 'Référence de rotoscopie indisponible');
  assert.match(lesson.steps[0].body, /pas accessible dans cette version/);
  for (const step of lesson.steps) {
    assert.equal(step.target, undefined);
    assert.equal(step.check, undefined);
    assert.equal(step.before, undefined);
    assert.doesNotMatch(step.body, /Clique|Layer|Importer|Bien joué|jamais exportée/);
  }
});

test('ordinary image/video media imports cannot masquerade as a rotoscopy Reference route', () => {
  const source = fs.readFileSync('src/js/drop-import.js', 'utf8');
  assert.match(source, /imageFiles\.length.*SM\.importImageFiles\(imageFiles\)/);
  assert.match(source, /videoFiles\.length.*SM\.importVideoFile\(f\)/);
  assert.match(source, /otherFiles\.length.*SMReference\.importFiles\(otherFiles\)/);
  assert.doesNotMatch(source, /SMReference\.importFiles\((imageFiles|videoFiles)\)/);
});
