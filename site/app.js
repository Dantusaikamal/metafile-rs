import init, { inspectMetafile, metafileToSvg } from './wasm/metafile_wasm.js';
const $ = (id) => document.getElementById(id);
const state = { bytes: null, name: '', svg: '', info: null, result: null, url: null, ready: false };
const MAX_SIZE = 20 * 1024 * 1024;
const showError = (error) => { const message = error?.message || String(error); $('error').textContent = (error?.code ? error.code + ': ' : '') + message; $('error').hidden = false; };
const clearError = () => { $('error').hidden = true; $('error').textContent = ''; };
const pretty = (value) => typeof value === 'object' ? JSON.stringify(value) : String(value ?? '—');
const downloadBlob = (blob, extension) => { const url = URL.createObjectURL(blob); const link = document.createElement('a'); link.href = url; link.download = state.name.replace(/\.[^.]+$/, '') + '.' + extension; document.body.append(link); link.click(); link.remove(); setTimeout(() => URL.revokeObjectURL(url), 1500); };
const dimensions = (svg) => { const doc = new DOMParser().parseFromString(svg, 'image/svg+xml'); const root = doc.documentElement; if (root.localName !== 'svg' || doc.querySelector('parsererror')) throw new Error('Renderer produced invalid SVG'); const box = (root.getAttribute('viewBox') || '').trim().split(/[\s,]+/).map(Number); if (box.length === 4 && box.every(Number.isFinite) && box[2] > 0 && box[3] > 0) return [box[2], box[3]]; const w = parseFloat(root.getAttribute('width')), h = parseFloat(root.getAttribute('height')); return [w > 0 ? w : 800, h > 0 ? h : 600]; };
function render() {
  const result = metafileToSvg(state.bytes, { strict: $('strict').checked });
  if (!result || typeof result.svg !== 'string' || !result.svg.includes('<svg')) throw new Error('Renderer returned no SVG');
  state.result = result; state.svg = result.svg;
  if (state.url) URL.revokeObjectURL(state.url);
  state.url = URL.createObjectURL(new Blob([state.svg], { type: 'image/svg+xml' }));
  $('preview-image').src = state.url;
  $('result').hidden = false; $('dropzone').hidden = true;
  $('filename').textContent = state.name;
  $('file-summary').textContent = [String(state.info?.format || 'Metafile').toUpperCase(), (state.bytes.length / 1024).toFixed(1) + ' KB'].join('  ·  ');
  const details = { ...state.info };
  $('metadata').replaceChildren();
  for (const [key, value] of Object.entries(details)) { const item = document.createElement('div'); const title = document.createElement('b'); title.textContent = key.replace(/([A-Z])/g, ' $1'); item.append(title, document.createTextNode(pretty(value))); $('metadata').append(item); }
  const diagnostics = Array.isArray(result.diagnostics) ? result.diagnostics : [];
  $('diagnostic-count').textContent = '(' + diagnostics.length + ')';
  $('diagnostics').replaceChildren();
  if (!diagnostics.length) { const p = document.createElement('p'); p.textContent = 'No rendering diagnostics.'; $('diagnostics').append(p); }
  for (const diagnostic of diagnostics) { const p = document.createElement('p'); p.textContent = pretty(diagnostic); $('diagnostics').append(p); }
  updateFormat();
}
function updateFormat() { const format = $('format').value; $('scale').disabled = format === 'svg'; $('download').firstChild.textContent = 'Download ' + format.toUpperCase() + ' '; }
async function loadFile(file) {
  clearError(); if (!state.ready) return showError(new Error('Renderer is still loading. Try again in a moment.'));
  if (!file) return;
  if (!/\.(wmf|emf)$/i.test(file.name)) return showError(new Error('Choose a .wmf or .emf file.'));
  if (file.size > MAX_SIZE) return showError(new Error('File exceeds the 20 MB limit.'));
  try { $('engine-status').textContent = 'Inspecting…'; state.bytes = new Uint8Array(await file.arrayBuffer()); state.name = file.name; state.info = inspectMetafile(state.bytes); render(); }
  catch (error) { state.bytes = null; showError(error); }
  finally { $('engine-status').textContent = 'Renderer ready'; }
}
async function exportFile() {
  if (!state.svg) return;
  const button = $('download'); button.disabled = true; clearError();
  try {
    const format = $('format').value;
    if (format === 'svg') { downloadBlob(new Blob([state.svg], { type: 'image/svg+xml;charset=utf-8' }), 'svg'); return; }
    const [width, height] = dimensions(state.svg), scale = Number($('scale').value);
    const w = Math.max(1, Math.ceil(width * scale)), h = Math.max(1, Math.ceil(height * scale));
    if (w > 16384 || h > 16384 || w * h > 40000000) throw new Error('Raster output is too large. Choose a smaller scale.');
    const canvas = document.createElement('canvas'); canvas.width = w; canvas.height = h;
    const ctx = canvas.getContext('2d'); if (!ctx) throw new Error('Canvas export is unavailable in this browser.');
    if (format === 'jpeg') { ctx.fillStyle = '#ffffff'; ctx.fillRect(0, 0, w, h); }
    const image = new Image(); image.decoding = 'sync';
    await new Promise((resolve, reject) => { image.onload = resolve; image.onerror = () => reject(new Error('Browser could not rasterize this SVG. Download SVG instead.')); image.src = state.url; });
    ctx.drawImage(image, 0, 0, w, h);
    const blob = await new Promise(resolve => canvas.toBlob(resolve, format === 'png' ? 'image/png' : 'image/jpeg', .94));
    if (!blob) throw new Error('Browser could not export this image.');
    downloadBlob(blob, format === 'jpeg' ? 'jpg' : 'png');
  } catch (error) { showError(error); } finally { button.disabled = false; }
}
const zone = $('dropzone');
zone.addEventListener('click', () => $('file-input').click());
zone.addEventListener('keydown', (event) => { if (event.key === 'Enter' || event.key === ' ') { event.preventDefault(); $('file-input').click(); } });
$('file-input').addEventListener('change', (event) => { loadFile(event.target.files?.[0]); event.target.value = ''; });
for (const type of ['dragenter','dragover']) zone.addEventListener(type, event => { event.preventDefault(); zone.classList.add('dragging'); });
for (const type of ['dragleave','drop']) zone.addEventListener(type, event => { event.preventDefault(); zone.classList.remove('dragging'); });
zone.addEventListener('drop', event => loadFile(event.dataTransfer?.files?.[0]));
$('replace').addEventListener('click', () => $('file-input').click());
$('strict').addEventListener('change', () => { if (state.bytes) { clearError(); try { render(); } catch (error) { showError(error); } } });
$('format').addEventListener('change', updateFormat);
$('download').addEventListener('click', exportFile);
try { await init(new URL('./wasm/metafile_wasm_bg.wasm', import.meta.url)); state.ready = true; $('engine-status').textContent = '● Renderer ready'; }
catch (error) { $('engine-status').textContent = 'Renderer unavailable'; showError(new Error('Could not load WebAssembly. Reload the page or check the site build. ' + (error?.message || ''))); }