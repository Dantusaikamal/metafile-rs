import { spawn } from 'node:child_process';
import { createReadStream, existsSync } from 'node:fs';
import { createServer } from 'node:http';
import { resolve } from 'node:path';

const root = resolve(import.meta.dirname, '..');
const browserCandidates = process.env.METAFILE_BROWSER
  ? [process.env.METAFILE_BROWSER]
  : process.platform === 'win32'
    ? ['C:/Program Files/Google/Chrome/Application/chrome.exe', 'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe']
    : ['google-chrome', 'chromium', 'chromium-browser'];
const browser = browserCandidates.find((path) => existsSync(path) || !path.includes('/'));
if (!browser) throw new Error('Chrome/Chromium/Edge not found; set METAFILE_BROWSER');

const page = `<!doctype html><meta charset="utf-8"><body>RUNNING<script type="module">
import init, { inspectMetafile, inspectWmf, metafileToSvg, wmfToSvg } from '/wasm/metafile_wasm.js';
try {
  await init('/wasm/metafile_wasm_bg.wasm');
  const bytes = new Uint8Array(await (await fetch('/fixture.wmf')).arrayBuffer());
  const info = inspectWmf(bytes);
  const first = wmfToSvg(bytes, { strict: false });
  const second = wmfToSvg(bytes, { strict: false });
  const emf = new Uint8Array(await (await fetch('/fixture.emf')).arrayBuffer());
  const emfInfo = inspectMetafile(emf);
  const emfFirst = metafileToSvg(emf, { strict: false });
  const emfSecond = metafileToSvg(emf, { strict: false });
  const emfplus = new Uint8Array(await (await fetch('/fixture.emfplus')).arrayBuffer());
  const emfplusInfo = inspectMetafile(emfplus);
  let emfplusRejected = false;
  try { metafileToSvg(emfplus, { strict: false }); } catch (error) { emfplusRejected = error?.code === 'unsupported_feature'; }
  let aliasesRejectEmf = 0;
  for (const call of [() => inspectWmf(emf), () => wmfToSvg(emf, { strict: false })]) {
    try { call(); } catch (error) { if (error?.code === 'format_mismatch') aliasesRejectEmf++; }
  }
  let structured = false;
  try { inspectMetafile(new Uint8Array([1, 2, 3])); } catch (error) { structured = typeof error?.code === 'string' && typeof error?.message === 'string'; }
  if (info.format !== 'wmf' || !first.svg.includes('<svg') || first.svg !== second.svg || emfInfo.format !== 'emf' || !emfFirst.svg.includes('<svg') || emfFirst.svg !== emfSecond.svg || emfplusInfo.format !== 'emfplus' || !emfplusRejected || aliasesRejectEmf !== 2 || !structured) throw new Error('contract assertion failed');
  document.body.textContent = 'PASS';
} catch (error) { document.body.textContent = 'FAIL: ' + (error?.stack || error); }
</script>`;

const server = createServer((request, response) => {
  const routes = {
    '/wasm/metafile_wasm.js': ['target/wasm-bindgen-web/metafile_wasm.js', 'text/javascript'],
    '/wasm/metafile_wasm_bg.wasm': ['target/wasm-bindgen-web/metafile_wasm_bg.wasm', 'application/wasm'],
    '/fixture.wmf': ['fixtures/real-world/windows-gdi-vector.wmf', 'application/octet-stream'],
    '/fixture.emf': ['fixtures/real-world/windows-gdiplus-emf.emf', 'application/octet-stream'],
    '/fixture.emfplus': ['fixtures/real-world/windows-gdiplus-emfplus-only.emf', 'application/octet-stream'],
  };
  if (request.url === '/') { response.setHeader('content-type', 'text/html'); response.end(page); return; }
  const route = routes[request.url];
  if (!route) { response.statusCode = 404; response.end(); return; }
  response.setHeader('content-type', route[1]); createReadStream(resolve(root, route[0])).pipe(response);
});
await new Promise((done) => server.listen(0, '127.0.0.1', done));
const { port } = server.address();
const output = await new Promise((done, reject) => {
  const child = spawn(browser, ['--headless=new', '--disable-gpu', '--no-first-run', '--dump-dom', '--virtual-time-budget=10000', `http://127.0.0.1:${port}/`]);
  const timer = setTimeout(() => { child.kill('SIGKILL'); reject(new Error('browser smoke timed out after 30 seconds')); }, 30_000);
  let stdout = '', stderr = '';
  child.stdout.on('data', (chunk) => { stdout += chunk; });
  child.stderr.on('data', (chunk) => { stderr += chunk; });
  child.on('error', (error) => { clearTimeout(timer); reject(error); });
  child.on('close', (code) => { clearTimeout(timer); code === 0 ? done(stdout) : reject(new Error(stderr || `browser exited ${code}`)); });
});
server.close();
if (!output.includes('<body>PASS</body>')) throw new Error(`browser smoke failed:\n${output}`);
console.log('Headless browser WASM smoke test passed');
