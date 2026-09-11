import { execFileSync } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const target = resolve(root, "target/wasm32-unknown-unknown/release/metafile_wasm.wasm");
const out = resolve(root, "target/wasm-bindgen-web");
mkdirSync(out, { recursive: true });

if (process.env.METAFILE_SKIP_CARGO_BUILD !== "1") {
  execFileSync("cargo", ["+1.88.0", "build", "-p", "metafile-wasm", "--target", "wasm32-unknown-unknown", "--release"], { cwd: root, stdio: "inherit" });
}
execFileSync("wasm-bindgen", [target, "--target", "web", "--out-dir", out, "--out-name", "metafile_wasm"], { cwd: root, stdio: "inherit" });
writeFileSync(resolve(out, "package.json"), '{"type":"module","private":true}\n');
