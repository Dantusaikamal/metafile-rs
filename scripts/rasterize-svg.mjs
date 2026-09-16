import { spawn, spawnSync } from 'node:child_process';
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { isAbsolute, join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

const [input, output, widthText = '600', heightText = '400'] = process.argv.slice(2);
if (!input || !output) throw new Error('usage: node scripts/rasterize-svg.mjs input.svg output.png [width] [height]');
const width = Number(widthText), height = Number(heightText);
const timeout = Number(process.env.METAFILE_RASTER_TIMEOUT_MS || 60_000);
const platformCandidates = process.platform === 'win32'
  ? [
      'C:/Program Files/Google/Chrome/Application/chrome.exe',
      'C:/Program Files (x86)/Google/Chrome/Application/chrome.exe',
      'C:/Program Files/Microsoft/Edge/Application/msedge.exe',
      'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe',
    ]
  : ['google-chrome', 'chromium', 'chromium-browser'];
const candidates = [...new Set([process.env.CHROME_PATH, ...platformCandidates].filter(Boolean))];
let browser;
if (process.env.CHROME_PATH && isAbsolute(process.env.CHROME_PATH) && existsSync(process.env.CHROME_PATH)) {
  browser = process.env.CHROME_PATH;
} else {
  browser = candidates.find((candidate) => {
    if (isAbsolute(candidate)) return existsSync(candidate);
    const result = spawnSync(candidate, ['--version'], { stdio: 'ignore', timeout: 10_000 });
    return !result.error && result.status === 0;
  });
}
if (!browser) {
  throw new Error(`Chrome/Chromium/Edge not found; attempted: ${candidates.join(', ')}`);
}
console.log(`Using browser: ${browser}`);
const directory = mkdtempSync(join(tmpdir(), 'metafile-rs-raster-'));
try {
  const svg = readFileSync(resolve(input), 'utf8').replace(
    /<svg\s/,
    `<svg style="display:block;width:${width}px;height:${height}px" `,
  );
  const page = join(directory, 'candidate.svg');
  writeFileSync(page, svg);
  const arguments_ = [
    '--headless=new', '--disable-gpu', '--hide-scrollbars', '--force-device-scale-factor=1',
    '--no-first-run', '--no-default-browser-check', '--disable-extensions',
    '--disable-background-networking',
    `--user-data-dir=${join(directory, 'browser-profile')}`,
    `--window-size=${width},${height}`, `--screenshot=${resolve(output)}`, pathToFileURL(page).href,
  ];
  const result = await new Promise((resolveResult, reject) => {
    const child = spawn(browser, arguments_, { windowsHide: true });
    const stdout = [];
    const stderr = [];
    let settled = false;
    child.stdout.on('data', (chunk) => stdout.push(chunk));
    child.stderr.on('data', (chunk) => stderr.push(chunk));
    const timer = setTimeout(() => {
      settled = true;
      if (process.platform === 'win32' && child.pid !== undefined) {
        // Chromium is multi-process. Killing only the broker can strand a
        // renderer and make later qualification cases hang, so terminate the
        // exact process tree rooted at the child we created.
        spawnSync('taskkill.exe', ['/PID', String(child.pid), '/T', '/F'], {
          stdio: 'ignore',
          windowsHide: true,
        });
      } else {
        child.kill('SIGKILL');
      }
      reject(new Error(`browser rasterization timed out after ${timeout}ms`));
    }, timeout);
    child.once('error', (error) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      reject(error);
    });
    child.once('close', (status, signal) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      resolveResult({
        status,
        signal,
        stdout: Buffer.concat(stdout).toString('utf8'),
        stderr: Buffer.concat(stderr).toString('utf8'),
      });
    });
  });
  const processOutput = `stdout:\n${result.stdout || '<empty>'}\nstderr:\n${result.stderr || '<empty>'}`;
  if (result.status !== 0) {
    throw new Error(`browser exited ${result.status ?? `with signal ${result.signal}`}\n${processOutput}`);
  }
} finally {
  rmSync(directory, { recursive: true, force: true });
}
