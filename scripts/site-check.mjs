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
