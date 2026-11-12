import { readFileSync } from 'node:fs';
import assert from 'node:assert/strict';
import { resolve } from 'node:path';
const root = resolve(import.meta.dirname, '..');
const html = readFileSync(resolve(root, 'site/index.html'), 'utf8');
const app = readFileSync(resolve(root, 'site/app.js'), 'utf8');
const worker = readFileSync(resolve(root, 'site/worker.js'), 'utf8');
for (const id of ['file-input','dropzone','result','replace','preview-image','preview-error','format','scale','strict','download','error','engine-status','diagnostics','metadata']) {
  assert.match(html, new RegExp('id="' + id + '"'), 'Missing UI control: ' + id);
}
assert.match(app, /new Worker\(/);
assert.match(worker, /inspectMetafile\(bytes\)/);
assert.match(worker, /metafileToSvg\(bytes, \{ strict \}\)/);
assert.match(html, /Frequently asked questions/);
assert.match(html, /canonical/);
assert.doesNotMatch(app, /fetch\(|XMLHttpRequest|sendBeacon/);
console.log('Static site contract checks passed');

const { samples, getSampleFile } = await import('../site/samples.js');
assert.equal(samples.length, 3);
for (const sample of samples) {
  const file = getSampleFile(sample.id);
  const bytes = new Uint8Array(await file.arrayBuffer());
  const view = new DataView(bytes.buffer);
  assert.equal(view.getUint32(0, true), 0x9ac6cdd7, 'Invalid placeable WMF signature');
  assert.equal(view.getUint16(24, true), 9, 'Invalid standard WMF header');
  assert.equal(view.getUint32(28, true) * 2, bytes.length - 22, 'WMF byte length mismatch');
  assert.ok(bytes.length > 100, 'Sample unexpectedly empty');
  const objectCapacity = view.getUint16(32, true);
  assert.equal(objectCapacity, sample.drawing.length + 1, 'WMF object table must hold the pen and every brush');
  let offset = 40; // 22-byte placeable header + 18-byte standard WMF header
  let createdObjects = 0;
  let selectedBrushes = 0;
  while (offset + 6 <= bytes.length) {
    const recordWords = view.getUint32(offset, true);
    const fn = view.getUint16(offset + 4, true);
    assert.ok(recordWords >= 3 && offset + recordWords * 2 <= bytes.length, 'Invalid WMF record size');
    if (fn === 0x02fa || fn === 0x02fc) createdObjects++;
    if (fn === 0x012d) {
      const selected = view.getUint16(offset + 6, true);
      assert.ok(selected < objectCapacity, 'Selected WMF object exceeds declared capacity');
      if (selected !== 0) selectedBrushes++;
    }
    offset += recordWords * 2;
    if (fn === 0) break;
  }
  assert.equal(createdObjects, objectCapacity, 'Declared WMF object count differs from actual creations');
  assert.equal(selectedBrushes, sample.drawing.length, 'Each shape should select its own brush');
  assert.equal(offset, bytes.length, 'WMF records must consume the complete file');
}
console.log('Three WMF sample structure checks passed');
