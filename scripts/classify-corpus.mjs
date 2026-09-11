import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs';
import { extname, join, resolve } from 'node:path';

function classify(bytes) {
  if (bytes.length >= 4 && bytes.readUInt32LE(0) === 0x9ac6cdd7) return 'wmf';
  if (bytes.length >= 18) {
    const type = bytes.readUInt16LE(0), headerWords = bytes.readUInt16LE(2), version = bytes.readUInt16LE(4);
    if ((type === 1 || type === 2) && headerWords === 9 && (version === 0x0100 || version === 0x0300)) return 'wmf';
  }
  if (bytes.length >= 44 && bytes.readUInt32LE(0) === 1 && bytes.toString('ascii', 40, 44) === ' EMF') {
    return bytes.includes(Buffer.from('EMF+')) ? 'emfplus' : 'emf';
  }
  return 'unknown';
}

function files(directory) {
  if (!directory || !existsSync(directory)) return [];
  return readdirSync(directory, { recursive: true })
    .map((name) => join(directory, name))
    .filter((path) => statSync(path).isFile() && ['.wmf', '.emf'].includes(extname(path).toLowerCase()));
}

const roots = [resolve('fixtures/real-world'), resolve('fixtures/private')];
if (process.env.METAFILE_FIXTURE_DIR) roots.push(resolve(process.env.METAFILE_FIXTURE_DIR));
const counts = { wmf: 0, emf: 0, emfplus: 0, unknown: 0 };
for (const root of roots) {
  for (const path of files(root)) {
    const format = classify(readFileSync(path)); counts[format] += 1;
    console.log(`${format}\t${path}`);
  }
}
console.log(JSON.stringify(counts));
