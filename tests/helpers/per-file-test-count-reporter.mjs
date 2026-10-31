// Node test reporter for nested `node --test` runs: emits one JSON object mapping each test file to the
// number of tests it actually executed (passed or failed; skipped/todo and suite wrappers excluded), so
// a wrapper can require every file to run something instead of trusting a summed, format-dependent count.
export default async function* perFileTestCounts(source) {
  const counts = {};
  for await (const ev of source) {
    if (ev.type !== 'test:pass' && ev.type !== 'test:fail') continue;
    const d = ev.data;
    if (!d.file || d.skip || d.todo || d.details?.type === 'suite') continue;
    counts[d.file] = (counts[d.file] || 0) + 1;
  }
  yield `${JSON.stringify(counts)}\n`;
}
