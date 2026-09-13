import { readFile } from "node:fs/promises";
import init, { inspectMetafile, inspectWmf, metafileToSvg, wmfToSvg } from "../target/wasm-bindgen-web/metafile_wasm.js";

const u16 = (v) => [v & 255, (v >>> 8) & 255];
const u32 = (v) => [v & 255, (v >>> 8) & 255, (v >>> 16) & 255, (v >>> 24) & 255];
const i16 = (v) => u16(v & 0xffff);
const record = (fn, params = []) => new Uint8Array([...u32(params.length / 2 + 3), ...u16(fn), ...params]);
const standard = () => {
  const move = record(0x0214, [...i16(10), ...i16(5)]);
  const line = record(0x0213, [...i16(20), ...i16(15)]);
  const eof = record(0);
  const body = [...move, ...line, ...eof];
  const max = Math.max(move.length, line.length, eof.length) / 2;
  return new Uint8Array([...u16(1), ...u16(9), ...u16(0x300), ...u32((18 + body.length) / 2), ...u16(0), ...u32(max), ...u16(0), ...body]);
};
const placeable = () => {
  const header = [...u32(0x9ac6cdd7), ...u16(0), ...i16(0), ...i16(0), ...i16(100), ...i16(100), ...u16(1440), ...u32(0)];
  let checksum = 0;
  for (let i = 0; i < header.length; i += 2) checksum ^= header[i] | (header[i + 1] << 8);
  return new Uint8Array([...header, ...u16(checksum), ...standard()]);
};

const wasm = await readFile(new URL("../target/wasm-bindgen-web/metafile_wasm_bg.wasm", import.meta.url));
await init({ module_or_path: wasm });

for (const bytes of [standard(), placeable()]) {
  const info = inspectWmf(bytes);
  if (info.format !== "wmf") throw new Error("inspection failed");
  const first = wmfToSvg(bytes, { strict: false });
  const second = wmfToSvg(bytes, { strict: false });
  if (!first.svg.includes("<svg") || !first.svg.includes("M 5 10 L 15 20")) throw new Error("render failed");
  if (first.svg !== second.svg) throw new Error("rendering is not deterministic");
}

const emf = new Uint8Array(await readFile(new URL("../fixtures/real-world/windows-gdiplus-emf.emf", import.meta.url)));
const emfInfo = inspectMetafile(emf);
const emfFirst = metafileToSvg(emf, { strict: false });
const emfSecond = metafileToSvg(emf, { strict: false });
if (emfInfo.format !== "emf" || !emfFirst.svg.includes("<svg") || emfFirst.svg !== emfSecond.svg) throw new Error("EMF render failed");

try {
  inspectMetafile(new Uint8Array([1, 2, 3]));
  throw new Error("malformed input unexpectedly succeeded");
} catch (error) {
  if (!error || typeof error.code !== "string" || typeof error.message !== "string") throw new Error("error was not structured");
}

try {
  wmfToSvg(standard(), { limits: { maxInputBytes: "bad" } });
  throw new Error("invalid options unexpectedly succeeded");
} catch (error) {
  if (error?.code !== "invalid_options") throw new Error("invalid option error was not structured");
}

console.log("WASM runtime smoke test passed");
