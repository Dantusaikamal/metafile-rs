import { samples, getSampleFile } from './samples.js';
const $ = (id) => document.getElementById(id);
const MAX_SIZE = 20 * 1024 * 1024;
const state = { file: null, bytes: null, svg: '', info: null, url: null, ready: false, busy: false, generation: 0 };
const worker = new Worker(new URL('./worker.js', import.meta.url), { type: 'module' });
let sequence = 0;
const pending = new Map();
worker.onmessage = ({ data }) => {
  const task = pending.get(data.id);
  if (!task) return;
  pending.delete(data.id);
  data.ok ? task.resolve(data) : task.reject(Object.assign(new Error(data.error.message), { code: data.error.code }));
};
worker.onerror = () => { for (const task of pending.values()) task.reject(new Error('Rendering worker failed. Reload and try again.')); pending.clear(); state.ready = false; $('engine-status').textContent = 'Renderer unavailable'; };
function request(type, bytes, strict) {
  const id = ++sequence;
  return new Promise((resolve, reject) => { pending.set(id, { resolve, reject }); worker.postMessage({ id, type, bytes, strict }); });
}
const showError = error => { $('error').textContent = (error?.code ? error.code + ': ' : '') + (error?.message || String(error)); $('error').hidden = false; };
const clearError = () => { $('error').hidden = true; $('error').textContent = ''; };
const pretty = value => typeof value === 'object' ? JSON.stringify(value) : String(value ?? '—');
function clearPreview() {
  state.svg = '';
  if (state.url) URL.revokeObjectURL(state.url);
  state.url = null;
  $('preview-image').removeAttribute('src');
  $('preview-error').hidden = true;
  $('result').hidden = true;
  $('dropzone').hidden = false;
  $('sample-grid').parentElement.hidden = false;
  $('download').disabled = true;
}
function setBusy(busy, label = 'Renderer ready') {
  state.busy = busy;
  $('engine-status').textContent = busy ? label : (state.ready ? '● Renderer ready' : 'Renderer unavailable');
  $('download').disabled = busy || !state.svg;
  $('replace').disabled = busy;
  $('strict').disabled = busy;
}
function dimensions(svg) {
  const doc = new DOMParser().parseFromString(svg, 'image/svg+xml');
  const root = doc.documentElement;
  if (root.localName !== 'svg' || doc.querySelector('parsererror')) throw new Error('Renderer produced invalid SVG');
  const box = (root.getAttribute('viewBox') || '').trim().split(/[\s,]+/).map(Number);
  if (box.length === 4 && box.every(Number.isFinite) && box[2] > 0 && box[3] > 0) return [box[2], box[3]];
  const w = Number.parseFloat(root.getAttribute('width')), h = Number.parseFloat(root.getAttribute('height'));
  if (Number.isFinite(w) && Number.isFinite(h) && w > 0 && h > 0) return [w, h];
  throw new Error('SVG dimensions are unavailable; raster export cannot proceed.');
}
function updateFormat() {
  const format = $('format').value;
  $('scale').disabled = format === 'svg';
  $('download').firstChild.textContent = 'Download ' + format.toUpperCase() + ' ';
}
function display(data) {
  const { info, result } = data;
  state.info = info;
  state.svg = result.svg;
  if (state.url) URL.revokeObjectURL(state.url);
  state.url = URL.createObjectURL(new Blob([state.svg], { type: 'image/svg+xml' }));
  $('preview-image').src = state.url;
  $('preview-image').alt = 'Preview of ' + state.file.name;
  $('preview-error').hidden = true;
  $('filename').textContent = state.file.name;
  let size = '';
  try { const [w,h] = dimensions(state.svg); size = ' · ' + Math.round(w) + ' × ' + Math.round(h) + ' SVG units'; } catch {}
  $('file-summary').textContent = String(info?.format || 'metafile').toUpperCase() + ' · ' + (state.file.size / 1024).toFixed(1) + ' KB' + size;
  $('metadata').replaceChildren();
  for (const [key,value] of Object.entries(info || {})) {
    const item = document.createElement('div'), title = document.createElement('b');
    title.textContent = key.replace(/([A-Z])/g, ' $1');
    item.append(title, document.createTextNode(pretty(value)));
    $('metadata').append(item);
  }
  const diagnostics = Array.isArray(result.diagnostics) ? result.diagnostics : [];
  $('diagnostic-count').textContent = '(' + diagnostics.length + ')';
  $('diagnostics').replaceChildren();
  for (const value of diagnostics.length ? diagnostics : ['No rendering diagnostics.']) {
    const p = document.createElement('p'); p.textContent = pretty(value); $('diagnostics').append(p);
  }
  $('result').hidden = false;
  $('dropzone').hidden = true;
  $('sample-grid').parentElement.hidden = true;
  updateFormat();
}
async function renderCurrent() {
  if (!state.bytes || !state.ready) return;
  const generation = ++state.generation;
  setBusy(true, 'Rendering…');
  clearError();
  try {
    const data = await request('render', state.bytes, $('strict').checked);
    if (generation !== state.generation) return;
    display(data);
  } catch (error) {
    if (generation !== state.generation) return;
    clearPreview();
    showError(error);
  } finally {
    if (generation === state.generation) setBusy(false);
  }
}
async function loadFile(file) {
  if (!file) return;
  clearError();
  if (!state.ready) return showError(new Error('Renderer is still initializing. Try again shortly.'));
  if (!/\.(emf|wmf)$/i.test(file.name)) return showError(new Error('Choose an EMF or WMF file.'));
  if (!file.size) return showError(new Error('The selected file is empty.'));
  if (file.size > MAX_SIZE) return showError(new Error('File exceeds the 20 MB limit.'));
  ++state.generation;
  clearPreview();
  state.file = file;
  try { state.bytes = new Uint8Array(await file.arrayBuffer()); await renderCurrent(); }
  catch (error) { clearPreview(); showError(error); }
}
function downloadBlob(blob, extension) {
  const url = URL.createObjectURL(blob), a = document.createElement('a');
  a.href = url; a.download = blob instanceof File ? blob.name : state.file.name.replace(/\.[^.]+$/, '') + '.' + extension;
  document.body.append(a); a.click(); a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 10000);
}
async function exportFile() {
  if (!state.svg || state.busy) return;
  clearError();
  const button = $('download'); button.disabled = true;
  try {
    const format = $('format').value;
    if (format === 'svg') { downloadBlob(new Blob([state.svg], { type: 'image/svg+xml;charset=utf-8' }), 'svg'); return; }
    const [width,height] = dimensions(state.svg), scale = Number($('scale').value);
    const w = Math.max(1, Math.ceil(width * scale)), h = Math.max(1, Math.ceil(height * scale));
    if (w > 16384 || h > 16384 || w*h > 40000000) throw new Error('Raster output is too large. Choose a smaller scale.');
    const canvas = document.createElement('canvas'); canvas.width = w; canvas.height = h;
    const ctx = canvas.getContext('2d'); if (!ctx) throw new Error('Canvas export is unavailable.');
    if (format === 'jpeg') { ctx.fillStyle = '#fff'; ctx.fillRect(0,0,w,h); }
    const image = new Image();
    await new Promise((resolve,reject) => { image.onload = resolve; image.onerror = () => reject(new Error('Browser cannot rasterize this SVG. Try SVG export.')); image.src = state.url; });
    ctx.drawImage(image,0,0,w,h);
    const mime = format === 'png' ? 'image/png' : 'image/jpeg';
    const blob = await new Promise(resolve => canvas.toBlob(resolve,mime,.94));
    if (!blob || blob.type !== mime) throw new Error('Browser cannot export this format.');
    downloadBlob(blob,format === 'jpeg' ? 'jpg' : 'png');
  } catch (error) { showError(error); } finally { button.disabled = state.busy || !state.svg; }
}
const input = $('file-input'), zone = $('dropzone');
zone.addEventListener('click', () => input.click());
zone.addEventListener('keydown', event => { if (event.key === 'Enter' || event.key === ' ') { event.preventDefault(); input.click(); } });
input.addEventListener('change', event => { const file = event.target.files?.[0]; event.target.value = ''; loadFile(file); });
for (const type of ['dragenter','dragover']) zone.addEventListener(type, event => { event.preventDefault(); zone.classList.add('dragging'); });
for (const type of ['dragleave','drop']) zone.addEventListener(type, event => { event.preventDefault(); zone.classList.remove('dragging'); });
zone.addEventListener('drop', event => { const sampleId = event.dataTransfer?.getData('application/x-metafile-sample'); if (sampleId) loadFile(getSampleFile(sampleId)); else loadFile(event.dataTransfer?.files?.[0]); });
$('replace').addEventListener('click', () => input.click());
for (const card of document.querySelectorAll('[data-sample]')) {
  const id = card.dataset.sample;
  card.addEventListener('click', event => { if (!event.target.closest('[data-download]')) loadFile(getSampleFile(id)); });
  card.addEventListener('keydown', event => { if (event.target === card && (event.key === 'Enter' || event.key === ' ')) { event.preventDefault(); loadFile(getSampleFile(id)); } });
  card.addEventListener('dragstart', event => {
    event.dataTransfer.effectAllowed = 'copy';
    event.dataTransfer.setData('application/x-metafile-sample', id);
    event.dataTransfer.setData('text/plain', samples.find(sample => sample.id === id).name);
    zone.classList.add('dragging');
  });
  card.addEventListener('dragend', () => zone.classList.remove('dragging'));
}
for (const button of document.querySelectorAll('[data-download]')) {
  button.addEventListener('click', event => {
    event.stopPropagation();
    const file = getSampleFile(button.dataset.download);
    downloadBlob(file, 'wmf');
  });
}
$('strict').addEventListener('change', renderCurrent);
$('format').addEventListener('change', updateFormat);
$('download').addEventListener('click', exportFile);
$('preview-image').addEventListener('error', () => { $('preview-image').hidden = true; $('preview-error').hidden = false; });
$('preview-image').addEventListener('load', () => { $('preview-image').hidden = false; $('preview-error').hidden = true; });
clearPreview();
try { await request('init'); state.ready = true; $('engine-status').textContent = '● Renderer ready'; }
catch (error) { $('engine-status').textContent = 'Renderer unavailable'; showError(error); }
