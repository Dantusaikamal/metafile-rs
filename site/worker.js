import init, { inspectMetafile, metafileToSvg } from './wasm/metafile_wasm.js';
let ready;
function initialize() { return ready ||= init(new URL('./wasm/metafile_wasm_bg.wasm', import.meta.url)); }
self.onmessage = async ({ data }) => {
  const { id, type, bytes, strict } = data;
  try {
    await initialize();
    if (type === 'init') { self.postMessage({ id, ok: true }); return; }
    if (type === 'render') {
      const info = inspectMetafile(bytes);
      const result = metafileToSvg(bytes, { strict });
      if (typeof result?.svg !== 'string' || !result.svg.includes('<svg')) throw new Error('Renderer returned no SVG');
      self.postMessage({ id, ok: true, info, result });
    }
  } catch (error) {
    self.postMessage({ id, ok: false, error: { code: error?.code || 'conversion_error', message: error?.message || String(error) } });
  }
};