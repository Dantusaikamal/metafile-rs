# Metafile Studio

A static, privacy-first WMF / EMF / EMF+ viewer and converter built on the repository's **actual** `metafile-wasm` API.

## Local build

Requires Rust 1.88, the wasm32-unknown-unknown target, wasm-bindgen-cli 0.2.128, and Node 22.

```sh
node scripts/build-wasm.mjs
node scripts/prepare-site.mjs
python3 -m http.server 8000 -d site
```

Visit http://localhost:8000/. Never open the HTML directly with file:// because WebAssembly modules require HTTP.

The prepare script copies the generated wasm-bindgen artifacts into `site/wasm/`. They are intentionally generated rather than committed. GitHub Actions builds them before publishing.

## Architecture

- The browser reads selected bytes using File.arrayBuffer; nothing is sent to a server.
- A dedicated Web Worker initializes the WASM module and performs inspection/rendering off the main UI thread.
- `inspectMetafile(bytes)` detects the actual format and returns metadata.
- `metafileToSvg(bytes, { strict })` returns the actual SVG and diagnostics.
- SVG preview uses an object URL in an image element.
- SVG export saves the engine's bytes. PNG/JPEG export uses browser Canvas to rasterize that SVG locally; the rasterized result may differ from native resvg.
- A 20 MB input cap and 40 MP raster cap protect the UI. The Rust parser applies its own bounded resource limits.

## Verification

Run `node --check site/app.js`, `node --check site/worker.js` and `node scripts/site-check.mjs` for syntax and static contract checks. The CI workflow also builds the WASM engine and runs its Node and headless-Chrome smoke tests. Manually verify replacement, strict-mode failures, preview, PNG/JPEG transparency, and mobile behavior using representative WMF, EMF, and EMF+ fixtures before merging. Static checks alone do not prove visual or format fidelity.

## Hosting

The `pages.yml` workflow publishes the `site/` directory on pushes to `main`. Enable **Settings → Pages → Build and deployment → GitHub Actions** once. The site uses relative URLs so it works at `/metafile-rs/` and local root. No paid services, authentication, database, analytics or runtime API.

## Privacy and limitations

No file uploads, telemetry, or remote processing. GitHub Pages serves only static code and WASM. Browser raster export may fail for complex SVG; SVG remains available. The engine documents partial support for certain historical records and reports approximations as diagnostics. Files are not stored across reloads.
