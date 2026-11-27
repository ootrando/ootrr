import { cpSync, existsSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const repo = join(here, "..");
const assets = join(repo, "web", "assets");
const args = process.argv.slice(2);
const flag = (k, d) => {
  const i = args.indexOf(k);
  return i >= 0 ? args[i + 1] : d;
};
const wasm = flag("--wasm", join(repo, "target", "wasm32-wasip1", "release", "ootrr.wasm"));
const out = flag("--out", join(here, "dist"));
const SOURCE = "https://github.com/ootrando/ootrr";
const MODS = "/zip/mods";

if (!existsSync(wasm)) {
  console.error(`no ${wasm}. Run: cargo build --release -p ootrr --target wasm32-wasip1`);
  process.exit(1);
}

function edit(p, f) {
  const before = readFileSync(p, "utf8").replace(/\r\n/g, "\n");
  const after = f(before);
  if (after === before) throw new Error(`${p} did not change, the page source moved on and make.mjs needs an update`);
  writeFileSync(p, after);
}

function must(text, find, put) {
  if (!text.includes(find)) throw new Error(`expected to find:\n${find}\nThe page source moved on, update site/make.mjs.`);
  return text.replaceAll(find, put);
}

rmSync(out, { recursive: true, force: true });
mkdirSync(out, { recursive: true });
cpSync(assets, out, { recursive: true, filter: (p) => !/[\\/]fonts$/.test(p) && !/tracker[^\\/]*\.(html|css|js)$/.test(p) });
mkdirSync(join(out, "fonts"));
const fonts = {
  "cinzel.woff2": "cinzel-latin-wght-normal.woff2",
  "garamond.woff2": "eb-garamond-latin-wght-normal.woff2",
  "garamond-italic.woff2": "eb-garamond-latin-wght-italic.woff2",
  "inter.woff2": "inter-latin-wght-normal.woff2",
  "space-grotesk.woff2": "space-grotesk-latin-wght-normal.woff2",
  "jetbrains-mono.woff2": "jetbrains-mono-latin-wght-normal.woff2",
};
for (const [to, from] of Object.entries(fonts)) cpSync(join(assets, "fonts", from), join(out, "fonts", to));
for (const f of readdirSync(join(assets, "fonts")).filter((x) => x.startsWith("OFL-"))) cpSync(join(assets, "fonts", f), join(out, "fonts", f));

for (const f of ["api.js", "picker.js", "folders.js", "drop.js"]) cpSync(join(here, "web", f), join(out, "js", f));
const ROOT = ["worker.js", "host.js", "wasi.js", "zip.js", "remote.js"];
for (const f of ROOT) cpSync(join(here, f), join(out, f));
cpSync(wasm, join(out, "ootrr.wasm"));
for (const f of ["LICENSE", "NOTICE"]) if (existsSync(join(repo, f))) cpSync(join(repo, f), join(out, `${f}.txt`));

const js = (f) => join(out, "js", f);

edit(js("run.js"), (t) => {
  let s = must(t, 'import { get, post, q } from "./api.js";', 'import { get, post, q, last_zip } from "./api.js";');
  s = must(s, `  const open_mod = h("button", { type: "button", class: "btn", disabled: !state.paths.mod.trim(), onclick: async () => {
    try {
      await post("/api/open-folder", { path: state.paths.mod.trim() });
    } catch (e) {
      fill(out, notice("error", e.message));
    }
  } }, "Open mod folder");`, "  const open_mod = zip_button();");
  s = must(s, "function summary(res) {", `function zip_button() {
  const z = last_zip();
  const name = \`\${(state.paths.name || "ootrr").replace(/[^A-Za-z0-9._ !-]/g, "-")}.zip\`;
  return h("button", { type: "button", class: "btn primary", disabled: !z, onclick: () => {
    const u = URL.createObjectURL(z);
    const a = h("a", { href: u, download: name });
    document.body.append(a);
    a.click();
    a.remove();
    setTimeout(() => URL.revokeObjectURL(u), 60000);
  } }, "Download the mod (.zip)");
}

function summary(res) {`);
  s = must(s, 'done && run.target ? h("p", { class: "genline ready" }, `Mod written to ${run.target}${files_note()}.`) : null,',
    'done ? h("div", { class: "row" }, h("p", { class: "genline ready" }, `The mod is ready${files_note()}. Download it and unzip it into your mods folder.`), zip_button()) : null,');
  s = must(s, '"The mod is written as mods/<name>/romfs in the folder you chose. For Hookshot, name it randomizer-<seed> so co-op can match the seed."',
    '"Unzip the download into your Hookshot or Longshot folder, so the mod lands in mods/<name>/romfs. For Hookshot, name it randomizer-<seed> so co-op can match the seed."');
  s = must(s, "`The mod is written to your mod folder, inside a folder named ${state.info.title_id} and then your mod name.`",
    "`Unzip the download into the folder your emulator loads mods from. It holds a folder named ${state.info.title_id} with your mod inside.`");
  s = must(s, 'h("dt", null, "Mod written to")', 'h("dt", null, "Mod")');
  s = must(s, "Check the server window.", "Reload the page and try again.");
  return s;
});

edit(js("ap.js"), (t) => {
  let s = must(t, 'import { get, get_blob, post, q } from "./api.js";', 'import { get, get_blob, post, q, last_zip } from "./api.js";');
  s = must(s, "function build_status() {", `function zip_button() {
  const z = last_zip();
  const name = \`\${((ap.sum && ap.sum.mod_name) || "ootrr").replace(/[^A-Za-z0-9._ !-]/g, "-")}.zip\`;
  return h("button", { type: "button", class: "btn primary", disabled: !z, onclick: () => {
    const u = URL.createObjectURL(z);
    const a = h("a", { href: u, download: name });
    document.body.append(a);
    a.click();
    a.remove();
    setTimeout(() => URL.revokeObjectURL(u), 60000);
  } }, "Download the mod (.zip)");
}

function build_status() {`);
  s = must(s, 'done && ap.target ? h("p", { class: "genline ready" }, `Mod written to ${ap.target}. Start a new save with it.`) : null,',
    'done ? h("div", { class: "row" }, h("p", { class: "genline ready" }, "The mod is ready. Download it, unzip it into your mods folder and start a new save with it."), zip_button()) : null,');
  s = must(s, "It builds your mod the same way Generate does, into the mod folder on the Configuration page.", "It builds your mod the same way Generate does, and offers it as a zip download.");
  s = must(s, "Check the server window.", "Reload the page and try again.");
  return s;
});

edit(js("config.js"), (t) => {
  let s = must(t, '  if (f.id === "game" || f.id === "mod") {',
    `  if (f.id === "mod") return row(f, terms, h("p", { class: "muted" }, "In the browser the mod is built in memory on your computer and offered as a zip download when it is ready. Unzip it into the folder your emulator loads mods from."));
  if (f.id === "game" || f.id === "mod") {`);
  s = must(s, 'label: "Mod folder", tip: "The folder your emulator or loader reads mods from. Generate writes the mod here." }',
    'label: "Mod download", tip: "The browser builds the mod and offers it as a zip. Nothing is uploaded." }');
  s = must(s, "  return `${base}${base.endsWith(sep) ? \"\" : sep}${mid}${state.paths.name}`;", "  return `${mid}${state.paths.name}, in the zip`;");
  s = must(s, 'import { pick_folder, pick_package } from "./picker.js";', 'import { pick_folder, pick_package } from "./picker.js";\nimport { drop_zone } from "./drop.js";');
  s = must(s, 'return row(f, terms, h("div", { class: "pathrow" }, inp, browse, pack), [out, upd && upd.el], stat);',
    'return row(f, terms, h("div", { class: "pathrow" }, inp, browse, pack), [out, f.id === "game" ? drop_zone(set) : null, upd && upd.el], stat);');
  return s;
});

edit(js("store.js"), (t) => {
  let s = must(t, 'paths: { game: "", update: "", mod: "", name: "My Seed", host: "eden" },', `paths: { game: "", update: "", mod: "${MODS}", name: "My Seed", host: "eden" },`);
  s = must(s, 'if (!["eden", "hookshot", "longshot", "nx2navi"].includes(state.paths.host)) state.paths.host = "eden";',
    `if (!["eden", "hookshot", "longshot", "nx2navi"].includes(state.paths.host)) state.paths.host = "eden";\n    state.paths.mod = "${MODS}";`);
  return s;
});

edit(js("menu.js"), (t) => must(t, 'h("a", { class: "btn small", href: "/tracker" }, "Tracker")', `h("a", { class: "btn small", href: "${SOURCE}", rel: "noopener" }, "Source")`));

for (const f of readdirSync(out).filter((x) => x.endsWith(".js") && !ROOT.includes(x))) {
  const p = join(out, f);
  const t = readFileSync(p, "utf8");
  if (t.includes('"/js/')) writeFileSync(p, t.replaceAll('"/js/', '"./js/'));
}
for (const f of readdirSync(join(out, "js")).filter((x) => x.endsWith(".js"))) {
  const p = js(f);
  const t = readFileSync(p, "utf8");
  if (t.includes('"/js/')) writeFileSync(p, t.replaceAll('"/js/', '"./'));
}
for (const f of readdirSync(out).filter((x) => x.endsWith(".css"))) {
  const p = join(out, f);
  writeFileSync(p, readFileSync(p, "utf8").replaceAll('url("/fonts/', 'url("fonts/'));
}
edit(join(out, "index.html"), (t) => {
  let s = t.replace(/(href|src)="\/(?!\/)/g, '$1="');
  s = must(s, "<title>OOTRR</title>", '<title>OOTRR Web</title>\n<link rel="icon" href="data:,">');
  return s;
});

const left = [];
for (const f of readdirSync(out, { recursive: true }).filter((x) => /\.(js|html|css)$/.test(x))) {
  const t = readFileSync(join(out, f), "utf8");
  if (/["'(]\/(js|fonts)\//.test(t) || /(href|src)="\/[^/]/.test(t) || /["']\/tracker["']/.test(t)) left.push(f);
}
if (left.length) throw new Error(`root paths left in ${left.join(", ")}`);
console.log(`site written to ${out}`);
