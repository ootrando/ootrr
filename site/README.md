# OOTRR Web

The full OOTRR page as a static site, running on the player's own computer. The player picks their game folder (or a .nup or .nx), generates a seed and downloads the mod as a zip to unzip into their mods folder. Nothing is uploaded and the site holds no game data.

Based on the web build in pull request 7, carried over to the current layout and page.

## How it works

- `ootrr serve --stdio` is the same server as `ootrr serve`, but it answers HTTP requests on stdin and stdout instead of a socket. It is compiled to `wasm32-wasip1`. Each request runs a fresh instance over the same virtual file system, so nothing has to wait between requests.
- `wasi.js` is a small WASI layer. The game is mounted read only at `/game/<name>` (and an update at `/update/<name>`, a package at `/game-package/<file>`). The data folder is an in-memory `/data`. The mod is built into an in-memory `/zip/mods` folder and `zip.js` packs it for download. Hard links share the file instead of copying it, and files over 32 MB are read in ranges, so a large package never has to fit in memory.
- Chrome and Edge open the game folder with the File System Access API. Folders are listed and files are read only when the build opens them, and the WASI calls wait on the browser through JSPI (`WebAssembly.Suspending`).
- Firefox and other browsers use a folder input. The browser lists the files once and the worker reads them with `FileReaderSync`, so the WASI calls stay synchronous and need no JSPI. For the game folder that listing is about 187,000 files and takes many minutes, so Firefox (which has JSPI) also gets a drop zone: a dropped folder is a `FileSystemDirectoryEntry`, which cannot be sent to the worker, so the worker asks the page for one folder listing or one file at a time (`remote.js`) and reads through the same lazy folder code as Chrome. A build lists about 60 folders and opens about 2,400 files.
- `host.js` runs the server and turns calls into HTTP requests. `worker.js` runs it in a Web Worker.
- The page is the desktop page from `web/assets`. `make.mjs` copies it, swaps `api.js` (requests go to the worker) and `picker.js` (browser folder and file pickers), and patches a few lines: the mod folder field becomes a download note, the result gets a Download the mod (.zip) button, and the Tracker link becomes a Source link. Every patch fails loudly if the desktop page moves on, so the web copy is kept in step.
- The tracker is left out. It watches save files with a background thread.

In the Rust code the changes are small: `web/src/stdio.rs` (the stdin and stdout loop, with the token from `OOTRR_STDIO_TOKEN`), jobs run inline when there are no threads, the token mix skips the process id on WASI, the pickup pack pool runs inline with one worker, and the file helpers have a plain fallback where there are no Unix or Windows APIs.

## Build and try it locally

```
rustup target add wasm32-wasip1
cargo build --release -p ootrr --target wasm32-wasip1
node site/make.mjs
python -m http.server -d site/dist 8000
```

Then open http://localhost:8000 in Chrome, Edge or Firefox.

## Checks

These need Node 24 or newer.

- `node site/test/parity.mjs --exe target/release/ootrr --wasm target/wasm32-wasip1/release/ootrr.wasm` generates every preset with a few seeds through the same API calls the page makes, and compares the seed files with `ootrr generate`. It needs no game files and runs in CI.
- `node --experimental-wasm-jspi site/test/drop_build.mjs --wasm <ootrr.wasm> --game <romfs> --out <dir>` does the same through the drop path, with stand-in folder entries that follow the browser's callback API.
- `node site/test/api_build.mjs --mode files|handle --wasm <ootrr.wasm> --game <romfs> --out <dir>` validates the game, generates and builds twice through the web code path in the Firefox or the Chrome mode, and writes the mod and its zip, for a byte comparison with `ootrr build`. The handle mode needs `node --experimental-wasm-jspi`.

## Deploy

`.github/workflows/pages.yml` builds, tests, audits and checks parity on pull requests that touch the code. On a push to `nightly` it also publishes `site/dist` to GitHub Pages (https://ootrando.github.io/ootrr/), stamped with a nightly version. Set Settings, Pages, Source to GitHub Actions once.
