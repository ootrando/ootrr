import { h, fill, notice, save_text, debounce } from "./dom.js";
import { get, get_blob, post, q, last_zip } from "./api.js";
import { state, color_request } from "./store.js";
import { effective, match_preset } from "./rules.js";
import { gen_missing, update_used } from "./config.js";
import { pick_folder } from "./picker.js";
import { go } from "./nav.js";

// The Archipelago page, in the order a player goes through it: set up Archipelago, make the
// player file, build the mod from the host's .apootrr file, open the client.
const KEY = "ootrr.ap";
const NAME_MAX = 16;

const ap = { folder: "", name: "", address: "", loaded: false, setup: null, checking: false, setup_error: "", installing: false, text: "", file: "", sum: null, phase: "idle", job: null, error: "", target: "" };
let subs = [];

function changed() {
  for (const f of subs) f();
}

// False on the browser build of this page: it cannot see an Archipelago folder or start programs.
const desktop = () => !(state.info && state.info.desktop === false);

function load() {
  if (ap.loaded) return;
  ap.loaded = true;
  try {
    const s = JSON.parse(localStorage.getItem(KEY) || "null");
    if (s && typeof s === "object") {
      for (const k of ["folder", "name", "address"]) if (typeof s[k] === "string") ap[k] = s[k].slice(0, 1000);
    }
  } catch {
    ap.folder = "";
  }
  if (desktop()) check();
}

function save() {
  try {
    localStorage.setItem(KEY, JSON.stringify({ folder: ap.folder, name: ap.name, address: ap.address }));
  } catch {
    return;
  }
}

export const ap_busy = () => ap.phase === "building";

async function check() {
  const asked = ap.folder.trim();
  ap.checking = true;
  ap.setup_error = "";
  changed();
  try {
    const r = await get(`/api/ap/setup${asked ? `?path=${q(asked)}` : ""}`);
    if (asked !== ap.folder.trim()) return;
    if (!asked && r.guess) {
      ap.folder = r.guess;
      save();
      ap.checking = false;
      return check();
    }
    ap.setup = r;
  } catch (e) {
    if (asked !== ap.folder.trim()) return;
    ap.setup = null;
    ap.setup_error = e.message;
  }
  ap.checking = false;
  changed();
}

async function install() {
  if (ap.installing) return;
  ap.installing = true;
  ap.setup_error = "";
  changed();
  try {
    ap.setup = await post("/api/ap/setup", { path: ap.folder.trim() });
  } catch (e) {
    ap.setup_error = e.message;
  }
  ap.installing = false;
  changed();
}

function mark(ok, label, detail) {
  const [k, c] = ok === null ? ["wait", "–"] : ok ? ["ok", "✓"] : ["bad", "✗"];
  return h("li", null, h("span", { class: `mark ${k}`, "aria-hidden": "true" }, c), h("span", null, label, detail ? h("span", { class: "muted" }, `  ${detail}`) : null));
}

function setup_status() {
  const s = ap.setup;
  if (ap.checking && !s) return h("p", { class: "muted" }, "Checking");
  if (ap.setup_error && !s) return notice("error", ap.setup_error);
  if (!s || !ap.folder.trim()) return notice("info", "Pick the folder Archipelago is installed in. It holds ArchipelagoLauncher. You need Archipelago 0.6.7 or newer.");
  if (!s.found) return [h("ul", { class: "steps-list" }, mark(false, "Archipelago not found", `in ${s.path}`)), notice("warn", "Pick the folder that holds ArchipelagoLauncher.")];
  const w = s.world || {};
  const world_detail = !w.installed ? "" : w.current ? `version ${w.version}` : w.built_by ? `version ${w.version || "unknown"}, from OOTRR ${w.built_by}. This app is ${s.app_version}` : "installed by hand, so it may not match this app";
  const btn = h("button", { type: "button", class: "btn primary", disabled: ap.installing, onclick: install }, ap.installing ? "Setting up" : "Set up Archipelago for OOTRR");
  return [
    h(
      "ul",
      { class: "steps-list" },
      mark(true, "Archipelago found", `in ${s.path}`),
      mark(w.installed ? (w.current ? true : null) : false, w.installed ? (w.current ? "OOTRR world installed" : "OOTRR world installed, but not this app's") : "OOTRR world not installed", world_detail),
      mark(s.knows_app, s.knows_app ? "Archipelago knows where this OOTRR app is" : s.host_exe ? "Archipelago points at another OOTRR app" : "Archipelago does not know where this OOTRR app is", s.knows_app ? "" : s.host_exe),
    ),
    ap.setup_error ? notice("error", ap.setup_error) : null,
    s.ready
      ? notice("ok", "Archipelago is ready for OOTRR. Come back and set it up again after you update the OOTRR app.")
      : [h("p", { class: "muted" }, `This puts the OOTRR world (version ${s.world_version}) in Archipelago's custom_worlds folder and writes this app's place into host.yaml. Close Archipelago first.`), h("div", { class: "row" }, btn)],
  ];
}

function setup_section() {
  if (!desktop()) {
    return h(
      "div",
      { class: "msec" },
      h("h4", null, "1. Set up Archipelago"),
      notice("info", "The browser cannot reach your Archipelago folder, so set it up by hand: save the OOTRR world file in step 2 and put it in the custom_worlds folder of Archipelago (0.6.7 or newer). The OOTRR app for your computer does this for you."),
    );
  }
  const inp = h("input", { type: "text", id: "ap_folder", value: ap.folder, spellcheck: "false", autocomplete: "off", placeholder: "The folder Archipelago is installed in" });
  const later = debounce(check, 400);
  inp.addEventListener("input", () => {
    ap.folder = inp.value;
    save();
    later();
  });
  const browse = h("button", { type: "button", class: "btn", onclick: async () => {
    browse.disabled = true;
    let v = null;
    try {
      v = await pick_folder(inp.value.trim(), "Pick the Archipelago folder");
    } finally {
      browse.disabled = false;
    }
    if (!v) return;
    inp.value = v;
    ap.folder = v;
    save();
    check();
  } }, "Browse");
  const again = h("button", { type: "button", class: "btn", onclick: check }, "Check again");
  const out = h("div", { "aria-live": "polite" });
  subs.push(() => {
    if (document.activeElement !== inp && inp.value !== ap.folder) inp.value = ap.folder;
    fill(out, setup_status());
  });
  return h("div", { class: "msec" }, h("h4", null, "1. Set up Archipelago"), h("div", { class: "w" }, h("div", { class: "wl" }, h("label", { for: "ap_folder" }, "Archipelago folder")), inp, h("div", { class: "row" }, browse, again)), out);
}

function name_input(id) {
  const inp = h("input", { type: "text", id, value: ap.name, maxlength: String(NAME_MAX), spellcheck: "false", autocomplete: "off", placeholder: "Your name in the room" });
  inp.addEventListener("input", () => {
    ap.name = inp.value;
    save();
    for (const other of document.querySelectorAll("input.ap-name")) if (other !== inp) other.value = ap.name;
  });
  inp.classList.add("ap-name");
  return inp;
}

function world_line() {
  const id = match_preset(state.schema, state.vals);
  const p = id && state.schema.presets.find((x) => x.id === id);
  return p
    ? `Your world uses the ${p.name} preset. To change it, change the settings on the other pages first, then save the file again.`
    : "Your world uses the settings you picked on the other pages. Change them there first, then save the file again.";
}

function player_section() {
  const out = h("div", { "aria-live": "polite" });
  const save_btn = h("button", { type: "button", class: "btn primary", onclick: async () => {
    if (!ap.name.trim()) return fill(out, notice("warn", "Type your player name first."));
    save_btn.disabled = true;
    try {
      const r = await post("/api/ap/player", { name: ap.name.trim(), settings: effective(state.schema, state.vals), plando: state.plando });
      save_text(r.file_name, r.text, "text/yaml");
      fill(
        out,
        r.preset
          ? notice("ok", `Saved ${r.file_name} with the ${r.preset_name} preset. Send it to the host.`)
          : notice("ok", `Saved ${r.file_name} with your own settings. Send it to the host. Because they are not a preset, the host needs the OOTRR app set up for Archipelago too (step 1, on their computer).`),
      );
    } catch (e) {
      fill(out, notice("error", e.message));
    }
    save_btn.disabled = false;
  } }, "Save my player file");
  const world_btn = h("button", { type: "button", class: "btn", onclick: async () => {
    world_btn.disabled = true;
    try {
      save_text("ootrr.apworld", await get_blob("/api/ap/world"), "application/octet-stream");
      fill(out, notice("ok", "Saved ootrr.apworld. The host puts it in Archipelago's custom_worlds folder."));
    } catch (e) {
      fill(out, notice("error", e.message));
    }
    world_btn.disabled = false;
  } }, "Save the OOTRR world for the host");
  return h(
    "div",
    { class: "msec" },
    h("h4", null, "2. Make your player file"),
    h("p", null, world_line()),
    h("div", { class: "w" }, h("div", { class: "wl" }, h("label", { for: "ap_name" }, "Player name")), name_input("ap_name")),
    h("div", { class: "row" }, save_btn, world_btn),
    h("p", { class: "muted" }, "The host only needs your player file. If the host does not play OOTRR, also send them the OOTRR world file, which they put in Archipelago's custom_worlds folder."),
    out,
  );
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

async function build() {
  if (ap_busy() || !ap.sum) return;
  const p = state.paths;
  Object.assign(ap, { phase: "building", job: null, error: "", target: "" });
  changed();
  try {
    const r = await post("/api/ap/build", {
      patch_text: ap.text,
      game_dir: p.game.trim(),
      update_dir: p.update.trim(),
      include_update: update_used() && state.include_update,
      mod_dir: p.mod.trim(),
      mod_name: ap.sum.mod_name,
      host: p.host || "eden",
      title_logo: state.cosmetic.title !== false,
      music: state.cosmetic.music,
      sfx: state.cosmetic.sfx,
      ...color_request(state.cosmetic),
      cosmetic_seed: state.cosmetic.seed,
      delete_others: !!state.delete_others,
    });
    ap.target = r.target || "";
    for (let i = 0; i < 900; i++) {
      const job = await get(`/api/jobs/${r.job_id}`);
      ap.job = job;
      changed();
      if (job.finished) {
        if (job.state !== "done") throw new Error(job.message || "The build failed.");
        ap.phase = "done";
        return changed();
      }
      await new Promise((ok) => setTimeout(ok, 400));
    }
    throw new Error("The build is taking too long. Reload the page and try again.");
  } catch (e) {
    Object.assign(ap, { phase: "failed", error: e.message });
    changed();
  }
}

async function open_patch(pick) {
  const f = pick.files && pick.files[0];
  pick.value = "";
  if (!f || ap_busy()) return;
  if (f.size > 64 * 1024 * 1024) {
    Object.assign(ap, { phase: "failed", error: "That file is too large to be an Archipelago patch.", job: null });
    return changed();
  }
  const text = await f.text();
  try {
    const sum = await post("/api/ap/read", { patch_text: text });
    Object.assign(ap, { text, file: f.name, sum, phase: "idle", job: null, error: "", target: "" });
  } catch (e) {
    Object.assign(ap, { phase: "failed", error: e.message, job: null });
    return changed();
  }
  if (!ap.name.trim()) {
    ap.name = ap.sum.slot.slice(0, NAME_MAX);
    save();
    for (const el of document.querySelectorAll("input.ap-name")) el.value = ap.name;
  }
  build();
}

function zip_button() {
  const z = last_zip();
  const name = `${((ap.sum && ap.sum.mod_name) || "ootrr").replace(/[^A-Za-z0-9._ !-]/g, "-")}.zip`;
  return h("button", { type: "button", class: "btn primary", disabled: !z, onclick: () => {
    const u = URL.createObjectURL(z);
    const a = h("a", { href: u, download: name });
    document.body.append(a);
    a.click();
    a.remove();
    setTimeout(() => URL.revokeObjectURL(u), 60000);
  } }, "Download the mod (.zip)");
}

function build_status() {
  const s = ap.sum;
  const job = ap.job;
  const done = ap.phase === "done";
  return [
    s
      ? h(
          "dl",
          { class: "kv" },
          h("dt", null, "File"),
          h("dd", { class: "mono" }, ap.file),
          h("dt", null, "Player"),
          h("dd", null, s.slot),
          h("dt", null, "World"),
          h("dd", null, `${s.preset}, ${s.locations} shuffled locations`),
          h("dt", null, "Items"),
          h("dd", null, `${s.foreign} for other players, ${s.receive} coming to you`),
        )
      : null,
    job && !job.finished ? h("progress", { max: "100", value: String(job.percent), "aria-label": "Build progress" }) : null,
    ap.phase === "building" && !job ? h("progress", { "aria-label": "Build progress" }) : null,
    job ? steps_view(job) : null,
    done ? h("div", { class: "row" }, h("p", { class: "genline ready" }, "The mod is ready. Download it, unzip it into your mods folder and start a new save with it."), zip_button()) : null,
    ap.phase === "failed" && ap.error ? notice("error", ap.error) : null,
    done && job && job.message ? h("details", { class: "fold" }, h("summary", null, "Build report"), h("pre", { class: "report" }, job.message)) : null,
  ];
}

function build_section() {
  const miss = gen_missing();
  const pick = h("input", { type: "file", accept: ".apootrr,application/json", class: "hide" });
  pick.onchange = () => open_patch(pick);
  const btn = h("button", { type: "button", class: "btn primary", disabled: Boolean(miss), onclick: () => pick.click() }, "Open my .apootrr file and build");
  const out = h("div", { "aria-live": "polite" });
  subs.push(() => {
    btn.disabled = Boolean(miss) || ap_busy();
    fill(out, build_status());
  });
  return h(
    "div",
    { class: "msec" },
    h("h4", null, "3. Build your mod"),
    h("p", null, "When the host has generated the room, they send you a file that ends in .apootrr. It builds your mod the same way Generate does, and offers it as a zip download."),
    miss ? notice("warn", `Fill in ${miss} on the `, h("a", { href: "#/config", onclick: (e) => (e.preventDefault(), go("config")) }, "Configuration"), " page first.") : null,
    h("div", { class: "row" }, btn, pick),
    out,
  );
}

function play_section() {
  if (!desktop()) {
    return h(
      "div",
      { class: "msec" },
      h("h4", null, "4. Play"),
      notice("info", "Playing in a room needs the OOTRR app on your computer: Archipelago's OOTRR Client runs it to read the game and hand over items. Open the Archipelago Launcher, start OOTRR Client and connect with the room's address and your player name. The first time, it asks where ootrr.exe is."),
    );
  }
  const addr = h("input", { type: "text", id: "ap_address", value: ap.address, spellcheck: "false", autocomplete: "off", maxlength: "255", placeholder: "For example archipelago.gg:38281" });
  addr.addEventListener("input", () => {
    ap.address = addr.value;
    save();
  });
  const out = h("div", { "aria-live": "polite" });
  const btn = h("button", { type: "button", class: "btn primary", onclick: async () => {
    btn.disabled = true;
    try {
      await post("/api/ap/client", { path: ap.folder.trim(), address: ap.address.trim(), name: ap.name.trim() });
      fill(out, notice("ok", "The Archipelago Launcher is opening the OOTRR Client. It can take a few seconds."));
    } catch (e) {
      fill(out, notice("error", e.message));
    }
    sync();
  } }, "Open the OOTRR Client");
  const sync = () => {
    const s = ap.setup;
    btn.disabled = !(s && s.found && s.can_launch);
    btn.title = btn.disabled ? "Pick the Archipelago folder in step 1 first." : "";
  };
  subs.push(sync);
  return h(
    "div",
    { class: "msec" },
    h("h4", null, "4. Play"),
    h("div", { class: "w" }, h("div", { class: "wl" }, h("label", { for: "ap_address" }, "Room address")), addr),
    h("div", { class: "w" }, h("div", { class: "wl" }, h("label", { for: "ap_name2" }, "Player name")), name_input("ap_name2")),
    h("div", { class: "row" }, btn),
    h("p", { class: "muted" }, "Start the game with the mod, then connect. Items from other players arrive with their normal item get. Loading an older save gives back only what that save is missing."),
    out,
  );
}

function hosting() {
  return h(
    "details",
    { class: "msec fold" },
    h("summary", null, "Hosting the room"),
    h(
      "ol",
      null,
      h("li", null, "Do step 1 on your computer, or put the OOTRR world file a player sent you in Archipelago's custom_worlds folder."),
      h("li", null, "Put every player's file in Archipelago's Players folder."),
      h("li", null, "Start Generate from the Archipelago Launcher. The zip it writes to the output folder holds one .apootrr file for each OOTRR player. Send each player theirs."),
      h("li", null, "Host the room: upload the zip on archipelago.gg, or start Host from the Launcher. Give the players the room's address."),
    ),
    h("p", { class: "muted" }, "A player whose file carries a settings string instead of a preset needs this app on the computer that generates: step 1 takes care of it. Entrance shuffle is refused, and dungeon items stay in their own world. Archipelago support is experimental."),
  );
}

export function ap_page() {
  subs = [];
  load();
  const kids = [setup_section(), player_section(), build_section(), play_section(), hosting()];
  changed();
  return kids;
}
