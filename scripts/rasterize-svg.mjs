import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { delimiter, join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

const [input, output, widthText = '600', heightText = '400'] = process.argv.slice(2);
if (!input || !output) throw new Error('usage: node scripts/rasterize-svg.mjs input.svg output.png [width] [height]');
const width = Number(widthText), height = Number(heightText);
const candidates = process.platform === 'win32'
  ? ['C:/Program Files/Google/Chrome/Application/chrome.exe', 'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe']
  : ['google-chrome', 'chromium', 'chromium-browser'];
const browser = candidates.find((candidate) => {
  const result = spawnSync(candidate, ['--version'], { stdio: 'ignore' });
  return !result.error;
});
if (!browser) throw new Error('Chrome/Chromium/Edge not found');
const directory = mkdtempSync(join(tmpdir(), 'metafile-rs-raster-'));
try {
  const svg = readFileSync(resolve(input), 'utf8').replace(
    /<svg\s/,
    `<svg style="display:block;width:${width}px;height:${height}px" `,
  );
  const page = join(directory, 'candidate.svg');
  writeFileSync(page, svg);
  const result = spawnSync(browser, [
    '--headless=new', '--disable-gpu', '--hide-scrollbars', '--force-device-scale-factor=1',
    `--window-size=${width},${height}`, `--screenshot=${resolve(output)}`, pathToFileURL(page).href,
  ], { encoding: 'utf8' });
  if (result.status !== 0) throw new Error(result.stderr || `browser exited ${result.status}`);
} finally {
  rmSync(directory, { recursive: true, force: true });
}
