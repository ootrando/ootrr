# Starting hearts and the heart item pool

## Why this changed

Link normally starts with three hearts. A Heart Container adds one heart, and
four Pieces of Heart add one heart. Changing the starting count without changing
the items in the world would leave more or fewer hearts available than before.

The generator now adjusts the shuffled item pool to compensate for starting
with a different number of hearts. It adjusts relative to the existing pool;
it does not independently establish that the resulting world contains exactly
20 hearts. See [Current limitations](#current-limitations).

## The adjustment

The **Starting hearts** setting allows 1 through 20. The new **Heart pool
adjustment** setting appears when the starting count differs from three:

- **Pieces of Heart:** prefer groups of four pieces.
- **Heart Containers:** prefer whole containers.
- **Random:** choose a feasible mix using the seed text, so the same seed and
  settings give the same adjustment.

If the preferred type cannot cover the full adjustment, the generator uses
the other type for the remainder. Pieces are always added or removed in
groups of four; a container counts as one heart.

| Starting hearts | Adjustment to the shuffled pool |
| --- | --- |
| 3 | No heart adjustment. |
| 1 or 2 | Add heart items worth `3 - starting hearts`. |
| 4 through 20 | Remove heart items worth `starting hearts - 3`. |

For example, starting with five hearts requires removing two hearts from the
pool: eight pieces, two containers, or four pieces and one container. Starting
with one heart requires adding two hearts by the same alternatives.

Adding heart items replaces shuffled rupee items; removing heart items replaces
them with green rupees. This keeps the number of items and shuffled locations
unchanged. The resulting items are then placed by the normal fill process, so
the locations originally holding the replaced items do not necessarily receive
their replacements.

If there are not enough suitable shuffled hearts or rupees to make the chosen
adjustment, generation stops with an error explaining the available counts.
The user can shuffle more applicable checks or choose a starting count closer
to three.

## Which checks count?

The adjustment looks at items from **shuffled** locations. Heart items at
unshuffled locations remain at their vanilla checks and are not removed from
the pool. Other settings determine which locations are shuffled:

| Check | How it joins the shuffled pool |
| --- | --- |
| Treasure Chest Game reward | Enable **Treasure Chest Game reward** and shuffle overworld chests. |
| Frog bug game Heart Piece | Enable **Zora's River frog bug game Heart Piece** and **Story and NPC rewards**. |
| `ZR Frogs Ocarina Game` reward | Enable **Story and NPC rewards**. |
| Hyrule Field heart scrub | Enable **Story and NPC rewards**. |
| Death Mountain Crater heart scrub | Enable **One time scrub sales** and **Story and NPC rewards**. |
| Supported freestanding heart pieces | These are shuffled even when **Freestanding items** is Off. |
| Boss Heart Containers | Enable **Boss Heart Containers**. |

**Story and NPC rewards** (`shuffle_rewards`) is currently hidden in the
settings menu and defaults to `supported`. When it is `off`, those rewards
remain fixed instead of contributing their items to the shuffled pool.
There are 15 mapped ground-level heart-piece checks and two additional
heart-piece checks held in containers; the current pickup selection includes
them even when **Freestanding items** is Off.

The separate **Zora's River frog songs** option adds five other frog-song
checks, whose original rewards are rupees. It does not control the separately
mapped `ZR Frogs Ocarina Game` reward or the bug-game heart reward. The old
`ZR Frogs in the Rain` logic entry refers to the same reward as
`ZR Frogs Ocarina Game`, not an additional Piece of Heart.

The generator recognizes the normal Piece of Heart and the distinct logic
variants for the Treasure Chest Game and out-of-logic pieces as heart pieces.
Adding a piece uses the normal Piece of Heart item.

## Spoiler log

The spoiler log includes **Loot pool changes**, showing net item additions and
removals after the standard shuffled pool is built. Normal progressive upgrade
copies are part of that baseline, not additional items. The changes include
other settings as well as heart adjustments; moving an item during the shuffle
is not a pool change.

**Total Heart Pool** counts the starting hearts and the Heart Containers and
all Piece of Heart variants in shuffled locations, unshuffled mapped rewards,
and unmapped logic rewards. Selection is based on the actual shuffled
locations for the settings, so a heart reward is counted as either shuffled
or fixed, not both. The log lists the fixed and unmapped locations, shows
extra pieces separately, and checks whether the modeled total is exactly 20.
If unmapped logic entries remain, the total is labeled an estimate because they
may not correspond to distinct in-game rewards. For the unshuffled total, the
logic-only `GF Freestanding PoH` and `Ice Cavern Freestanding PoH` entries are
excluded because those rewards do not exist in the remake, and `ZR Frogs in
the Rain` is excluded because it duplicates `ZR Frogs Ocarina Game`.
Pool changes and heart totals are unavailable when loading an older or imported
seed without this generation information.

For the checked example spoiler, starting with 1 heart and finding 10
containers and 36 pieces gives `1 + 10 + 36 / 4 = 20` hearts. The treasure
chest, frog bug game, scrub, and freestanding settings were enabled; the log
reported no additional unshuffled or unmapped hearts. The location list was
independently counted, including one Treasure Chest Game piece.

## How it is implemented

1. The generator selects locations using the current shuffle settings and
   builds their item pool.
2. Before filling locations, it calculates the difference between the selected
   starting hearts and the default of three.
3. It replaces the necessary shuffled heart or rupee items, choosing the mix
   according to **Heart pool adjustment**.
4. It records the number of containers and pieces added or removed in the
   generation report, then runs the existing placement rules.
5. During the mod build, added hearts receive the appropriate heart item actor,
   rather than the green-rupee actor used for removed items.

The main adjustment is in
[`randomizer/core/src/start_items/pool.rs`](randomizer/core/src/start_items/pool.rs).
It is called during generation from
[`randomizer/core/src/slice/generate.rs`](randomizer/core/src/slice/generate.rs).
The setting is defined in
[`randomizer/core/src/settings/list.rs`](randomizer/core/src/settings/list.rs),
and build-time actor assignment is handled by
[`randomizer/core/src/slice/chest_targets.rs`](randomizer/core/src/slice/chest_targets.rs).

### Spoiler data flow

- In `generate_data`, `vanilla` holds original location items; `pooled` is
  built with `restore_upgrades`, restoring standard progressive item copies.
  `base` applies starting-item removals and heart adjustments to that pool,
  and `got` is the filled placement.
- `loot_pool_text` compares `pooled` with `base` at the selected shuffled
  locations. It reports **net** counts, so the standard Bow, Bomb Bag and
  Slingshot copies are not falsely reported as additions, and ordinary
  placement swaps do not appear as pool changes.
- The same function counts final shuffled items in `got`, then scans the
  remaining real logic locations for fixed heart rewards. It excludes the
  three legacy entries listed above and classifies other entries without a
  catalog or flow mapping as unverified. It compares quarter-heart units
  against 80, so partial groups of pieces do not incorrectly pass the check.
- `Generated::loot_pool` carries the text only when spoilers are requested.
  [`web/src/api_generate.rs`](web/src/api_generate.rs) exposes it under
  `spoiler.loot_pool`; [`web/assets/js/spoiler.js`](web/assets/js/spoiler.js)
  includes it in both full and compact spoiler downloads. The full log puts it
  after settings and before the playthrough. CLI spoilers also include it.
  Imported seed descriptions lack the original pool baseline and display the
  summary as unavailable rather than guessing.

## Verification for merging

- `cargo test -p ootrr-core start_items::pool::tests` covers heart adjustment,
  optional heart checks, fallback/error behavior, and replacement actors.
- `cargo test -p ootrr-core loot_pool_counts_net_changes_and_all_heart_variants`
  checks variants, fixed/unmapped counts, partial hearts, and double-counting.
- `cargo test -p ootrr-core heart_summary_accounts_for_fixed_and_unmapped_checks`
  checks the treasure, frog, scrub, NPC, boss-heart and freestanding cases.
- `cargo test -p ootrr-core legacy_heart_entries_do_not_inflate_generated_totals`
  verifies generated seeds total 20 hearts with the optional checks off and on.
- `cargo test -p ootrr-core generated_spoiler_reports_heart_additions_and_removals`
  checks actual generated pool changes and excludes normal upgrade copies.
- `cargo test -p ootrr-web api_generate::tests` checks spoiler API behavior,
  including no summary when spoilers are disabled.
- `node --test site/test/spoiler.mjs` checks full/compact formatting and missing
  summaries; `cargo check --workspace` checks compilation.

## Current limitations

- The adjustment preserves the baseline total represented by the existing
  world and item pool. It does **not** count every actual in-game heart reward
  and enforce an exact 20-heart total.
- Some heart locations are not mapped to editable remake checks. Logic entries
  and actual in-game rewards are not necessarily a one-to-one match, so their
  counts should not be treated as a verified in-game inventory.
- Starting with 20 hearts fails under the default shuffle settings because
  there are not enough shuffled heart items to remove the required 17 hearts.
  Other high starting counts can also fail depending on the selected checks.
- Tests cover the adjustment, setting combinations, generated placements, and
  build-target actors. They do not replace an in-game playtest.
