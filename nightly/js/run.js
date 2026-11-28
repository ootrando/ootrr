import { h, fill, notice, copy, save_text } from "./dom.js";
import { get, post, q, last_zip } from "./api.js";
import { get_blob, post_blob } from "./api.js";
import { state, touch, color_request } from "./store.js";
import { effective } from "./rules.js";
import { button as report_button } from "./report.js";
import { panel as spoiler_panel, full_text } from "./spoiler.js";
import { gen_missing, mod_target, name_ok, update_used } from "./config.js";

export const run = { phase: "idle", line: "", job: null, error: "", target: "", ticket: "" };
const subs = new Set();
let pages = [];

export function set_pages(p) {
  pages = p;
}

export function on_run(fn) {
  subs.add(fn);
}

function changed() {
  for (const f of subs) f();
}

export const busy = () => run.phase === "generating" || run.phase === "building";

export function progress_text(p) {
  const base = `${p.seconds} s so far, attempt ${p.attempts}.`;
  return p.fallback ? `${base} The usual fill keeps failing with these settings, so the careful fill is running now.` : base;
}

function spoiler_for(res) {
  if (!state.spoiler_log || res.race) return "";
  return full_text(res, { schema: state.schema, vals: state.vals, pages, plando: state.plando, cosmetic: state.cosmetic, version: state.info && state.info.version });
}

async function build(res) {
  const p = state.paths;
  if (!name_ok(p.name)) throw new Error("Mod name: use 1 to 64 letters, digits, spaces and _ - ! . characters, with no spaces at the ends.");
  run.phase = "building";
  run.line = "Building the mod";
  run.target = mod_target();
  changed();
  const r = await post("/api/build", {
    seed_text: res.seed_text,
    game_dir: p.game.trim(),
    update_dir: p.update.trim(),
    include_update: update_used() && state.include_update,
    mod_dir: p.mod.trim(),
    mod_name: p.name,
    host: p.host || "eden",
    music: state.cosmetic.music,
    sfx: state.cosmetic.sfx,
    ...color_request(state.cosmetic),
    cosmetic_seed: state.cosmetic.seed,
    title_logo: state.cosmetic.title !== false,
    delete_others: !!state.delete_others,
    spoiler_text: spoiler_for(res),
  });
  run.target = r.target || run.target;
  for (let i = 0; i < 900; i++) {
    const job = await get(`/api/jobs/${r.job_id}`);
    run.job = job;
    changed();
    if (job.finished) {
      state.build_note = job.message || job.state;
      if (job.state !== "done") throw new Error(job.message || "The build failed.");
      return;
    }
    await new Promise((ok) => setTimeout(ok, 400));
  }
  throw new Error("The build is taking too long. Reload the page and try again.");
}

export async function generate() {
  if (busy()) return;
  const miss = gen_missing();
  if (miss) {
    run.phase = "failed";
    run.error = `Fill in ${miss} on the Configuration page first.`;
    return changed();
  }
  const ticket = `g${Date.now().toString(36)}${Math.random().toString(36).slice(2, 10)}`;
  const t0 = Date.now();
  Object.assign(run, { phase: "generating", line: "Starting", job: null, error: "", target: "", ticket });
  changed();
  const timer = setInterval(async () => {
    try {
      const p = await get(`/api/generate/progress?ticket=${q(ticket)}`);
      if (p && p.running && run.phase === "generating") {
        run.line = progress_text(p);
        changed();
      }
    } catch {
      run.line = `${Math.round((Date.now() - t0) / 1000)} s so far`;
    }
  }, 1000);
  try {
    const res = await post("/api/generate", { seed: state.seed.trim(), settings: effective(state.schema, state.vals), spoiler: state.spoiler_log && !state.race_seed, race: !!state.race_seed, plando: state.plando, ticket });
    clearInterval(timer);
    state.result = res;
    state.last_error = "";
    touch();
    await build(res);
    run.phase = "done";
  } catch (e) {
    run.phase = "failed";
    run.error = e.message;
    state.last_error = e.message;
  }
  clearInterval(timer);
  changed();
}

export function cancel() {
  if (run.phase !== "generating") return;
  run.line = "Cancelling";
  changed();
  post("/api/generate/cancel", { ticket: run.ticket }).catch(() => {});
}

async function rebuild() {
  if (busy() || !state.result) return;
  Object.assign(run, { error: "", job: null });
  try {
    await build(state.result);
    run.phase = "done";
  } catch (e) {
    run.phase = "failed";
    run.error = e.message;
  }
  changed();
}

const SYMS = { done: ["ok", "✓"], failed: ["bad", "✗"], unavailable: ["wait", "–"], running: ["wait", "…"] };

function steps_view(job) {
  return h(
    "ul",
    { class: "steps-list" },
    job.steps.map((s) => {
      const [k, c] = SYMS[s.state] || SYMS.running;
      return h("li", null, h("span", { class: `mark ${k}`, "aria-hidden": "true" }, c), h("span", null, s.label));
    }),
  );
}

function instructions() {
  const host = state.paths.host;
  const flat = host === "hookshot" || host === "longshot" || host === "nx2navi";
  return h(
    "div",
    { class: "msec" },
    h("h4", null, host === "longshot" ? "Using the mod in Longshot" : "Using the mod"),
    h(
      "ol",
      null,
      host === "longshot"
        ? h("li", null, `Pick Longshot's mods folder. The mod is written as mods/<name>/romfs. Longshot loads every mod in mods, so keep one seed at a time (Delete all other seeds does that).`)
        : flat
        ? h("li", null, "Unzip the download into your Hookshot or Longshot folder, so the mod lands in mods/<name>/romfs. For Hookshot, name it randomizer-<seed> so co-op can match the seed.")
        : h("li", null, `Unzip the download into the folder your emulator loads mods from. It holds a folder named ${state.info.title_id} with your mod inside.`),
      host === "longshot"
        ? h("li", null, "Turn other mods off by putting _ or - in front of their folder names. Of the mods that ship a size table, the game uses the one whose name sorts last.")
        : h("li", null, "Make sure that mod folder is the one your emulator loads mods from, and tick the mod in the game's add-ons list."),
      h("li", null, "Start the game from a fresh save."),
    ),
  );
}

// Loads a seed file as the current seed (its settings too). Returns an error text, or "" when it worked.
export async function load_seed_file(f) {
  if (!f) return "Pick a seed file first.";
  if (f.size > 8 * 1024 * 1024) return "That file is too large to be a seed file.";
  const text = await f.text();
  try {
    const res = await post("/api/seed/read", { seed_text: text });
    res.seed_text = text;
    res.file_name = f.name.replace(/[^A-Za-z0-9._-]/g, "-");
    state.vals = res.settings;
    state.result = res;
    Object.assign(run, { phase: "idle", job: null, error: "", target: "" });
    touch();
    return "";
  } catch (e) {
    return e.message;
  }
}

async function open_file(pick, out, redraw) {
  const f = pick.files && pick.files[0];
  pick.value = "";
  if (!f) return;
  const err = await load_seed_file(f);
  if (err) return fill(out, notice("error", err));
  redraw();
}

function files_note() {
  const res = state.result;
  if (res && res.race) return ", with the race file Seed.ootrr (no spoiler log)";
  return state.spoiler_log ? ", with Spoiler.txt and Seed.ootrr" : "";
}

// The game's item icons, read from the game folder set on the Config page (the tracker's game
// folder when that one is empty or unreadable). The icon API needs the session token and the page
// only shows images from itself, so they are fetched and drawn on a canvas, as the tracker does.
// Only icons that loaded are kept, so a game folder set later still brings them.
const hash_icons = new Map();

async function icon_blob(id) {
  const game = state.paths.game.trim();
  if (game) {
    try {
      return await post_blob("/api/hash-icon", { id, game_dir: game, update_dir: state.paths.update.trim() });
    } catch {
      // Fall back to the tracker's game folder.
    }
  }
  return get_blob(`/api/tracker/icon?id=${q(id)}`);
}

function icon_of(id) {
  const key = `${state.paths.game.trim()}\n${id}`;
  if (!hash_icons.has(key)) {
    const p = icon_blob(id).then((b) => createImageBitmap(b)).catch(() => null);
    hash_icons.set(key, p);
    p.then((bmp) => {
      if (!bmp) hash_icons.delete(key);
    });
  }
  return hash_icons.get(key);
}

// The seed hash a race seed shows on the title screen: the game's own item icons, by name.
export function hash_row(res) {
  const names = Array.isArray(res.hash) ? res.hash : [];
  const ids = Array.isArray(res.hash_ids) ? res.hash_ids : [];
  return h(
    "span",
    { class: "hashrow" },
    names.map((n, i) => {
      const cv = ids[i] ? h("canvas", { width: "56", height: "56", "aria-hidden": "true", class: "hashicon" }) : null;
      if (cv) {
        icon_of(ids[i]).then((bmp) => {
          if (!bmp) return cv.remove();
          cv.getContext("2d").drawImage(bmp, 0, 0, 56, 56);
        });
      }
      return h("span", { class: "hashitem" }, cv, n);
    }),
  );
}

function zip_button() {
  const z = last_zip();
  const name = `${(state.paths.name || "ootrr").replace(/[^A-Za-z0-9._ !-]/g, "-")}.zip`;
  return h("button", { type: "button", class: "btn primary", disabled: !z, onclick: () => {
    const u = URL.createObjectURL(z);
    const a = h("a", { href: u, download: name });
    document.body.append(a);
    a.click();
    a.remove();
    setTimeout(() => URL.revokeObjectURL(u), 60000);
  } }, "Download the mod (.zip)");
}

function summary(res) {
  return h(
    "dl",
    { class: "kv" },
    h("dt", null, "Seed"),
    h("dd", { class: "mono" }, res.seed),
    h("dt", null, "Settings string"),
    h("dd", { class: "mono" }, res.settings_string),
    h("dt", null, "Checksum"),
    h("dd", { class: "mono" }, res.checksum),
    res.race ? [h("dt", null, "Seed hash"), h("dd", null, hash_row(res))] : null,
    h("dt", null, "Placements"),
    h("dd", null, res.race ? "Hidden: this is a race seed" : String(res.placement_count)),
    run.target ? [h("dt", null, "Mod"), h("dd", { class: "mono" }, run.target)] : null,
  );
}

export function render_seed(redraw) {
  const res = state.result;
  const out = h("div", { "aria-live": "polite" });
  const pick = h("input", { type: "file", accept: ".ootrr,.json,application/json", class: "hide" });
  pick.onchange = () => open_file(pick, out, redraw);
  const open_btn = h("button", { type: "button", class: "btn", onclick: () => pick.click() }, "Open a seed file");
  const kids = [];

  if (run.phase === "generating") {
    kids.push(h("div", { class: "msec" }, h("h4", null, "Generating"), h("progress", { "aria-label": "Generating" }), h("p", { class: "muted" }, run.line),
      h("div", { class: "row" }, h("button", { type: "button", class: "btn", onclick: cancel }, "Cancel"))));
  }
  if (run.phase === "building" || (run.job && run.phase !== "generating")) {
    const job = run.job;
    const done = job && job.finished && job.state === "done";
    kids.push(h("div", { class: "msec" }, h("h4", null, "Build"),
      job && !job.finished ? h("progress", { max: "100", value: String(job.percent), "aria-label": "Build progress" }) : null,
      job ? steps_view(job) : h("p", { class: "muted" }, run.line),
      done ? h("div", { class: "row" }, h("p", { class: "genline ready" }, `The mod is ready${files_note()}. Download it and unzip it into your mods folder.`), zip_button()) : null,
      job && job.message && !done && job.finished ? notice("error", job.message) : null,
      done && job.message ? h("details", { class: "fold" }, h("summary", null, "Build report"), h("pre", { class: "report" }, job.message)) : null));
  }
  if (run.phase === "failed" && run.error) kids.push(notice("error", run.error));

  if (!res) {
    if (run.phase === "idle") kids.push(notice("info", "No seed yet. Fill in the folders on the Configuration page, then press Generate at the top right."));
    kids.push(h("div", { class: "row" }, open_btn, pick), out);
    return kids;
  }

  const rebuild_btn = h("button", { type: "button", class: "btn", disabled: busy() || Boolean(gen_missing()), onclick: rebuild }, "Build this seed again");
  const open_mod = zip_button();

  kids.push(
    res.placements_ready === false ? notice("warn", "This seed changes no items. Turn on a shuffle setting if you want items to move.") : null,
    res.warnings && res.warnings.length ? notice("warn", res.warnings.join(" ")) : null,
    Array.isArray(res.notes) && res.notes.length
      ? h("details", { class: "msec fold" }, h("summary", null, `Notes about these settings (${res.notes.length})`), h("pre", { class: "report" }, res.notes.join("\n")))
      : null,
    state.plando.length && res.report && res.report.includes("Plandomizer:")
      ? notice("info", `Plandomizer: ${state.plando.length} hand placed ${state.plando.length === 1 ? "entry was" : "entries were"} used. Share the seed file, because the seed text and settings alone cannot recreate it.`)
      : null,
    h(
      "details",
      { class: "msec fold" },
      h("summary", null, `Your seed · ${res.seed}`),
      summary(res),
      h(
        "div",
        { class: "row" },
        h("button", { type: "button", class: "btn", onclick: () => save_text(res.file_name || "seed.ootrr", res.seed_text) }, res.race ? "Download race file" : "Download seed file"),
        h("button", { type: "button", class: "btn", onclick: () => copy(res.settings_string) }, "Copy settings string"),
        report_button(res),
        open_mod,
        rebuild_btn,
        open_btn,
        pick,
      ),
      out,
    ),
    res.race
      ? h("div", { class: "msec" }, h("h4", null, "Race seed"),
        notice("info", "No spoiler log exists for this seed, for anyone. Share the race file: anyone can open it here and build the same seed. Everyone should see these icons under the logo on the title screen:"),
        h("p", null, hash_row(res)))
      : h("div", { class: "msec" }, h("h4", null, "Spoiler"), spoiler_panel(res, true)),
    instructions(),
  );
  return kids;
}
