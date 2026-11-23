import { strict as assert } from "node:assert";
import { test } from "node:test";
import { full_text, text } from "../../web/assets/js/spoiler.js";

const ctx = { schema: { settings: [], presets: [] }, vals: {}, pages: [] };
const summary = "Loot pool changes\n  Added:\n    4 x Piece of Heart\n\nTotal Heart Pool\n  Exactly 20 hearts: Yes\n";
const res = {
  seed: "test",
  checksum: "check",
  settings: {},
  spoiler: { loot_pool: summary, locations: [["Location", "Piece of Heart"]], playthrough: [[["Location", "Piece of Heart"]]] },
};

test("full and compact spoilers contain the generated summary", () => {
  const full = full_text(res, ctx);
  assert.ok(full.indexOf("Settings\n") < full.indexOf("Loot pool changes"));
  assert.ok(full.indexOf("Total Heart Pool") < full.indexOf("Playthrough"));
  assert.ok(full.includes(summary.trimEnd()));
  assert.ok(text(res).includes(summary.trimEnd()));
});

test("older seeds do not claim a pool summary", () => {
  const old = { ...res, spoiler: { locations: res.spoiler.locations } };
  for (const log of [full_text(old, ctx), text(old)]) {
    assert.match(log, /Loot pool changes[\s\S]*Unavailable for this seed\./);
    assert.match(log, /Total Heart Pool[\s\S]*Unavailable for this seed\./);
    assert.ok(!log.includes("Exactly 20 hearts"));
  }
});
