import { copyFileSync, mkdirSync } from 'node:fs';
import { resolve } from 'node:path';
const root = resolve(import.meta.dirname, '..');
const source = resolve(root, 'target/wasm-bindgen-web');
const destination = resolve(root, 'site/wasm');
mkdirSync(destination, { recursive: true });
for (const name of ['metafile_wasm.js', 'metafile_wasm_bg.wasm']) copyFileSync(resolve(source, name), resolve(destination, name));
console.log('Prepared site/wasm for static hosting');
