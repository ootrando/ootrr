# OOTRR nightly: changes since 0.9.0-nightly.20261125.15

Covers the nightly branch from `fe71df1` (the source of nightly 0.9.0-nightly.20261125.15).

## Fixes

- **Shuffled cows give progressive upgrades through their one-time check.** The upgrade no longer replaces the repeatable milk-in-bottle event. Rebuild the seed's mod to apply the fix. Not checked in game.

## Archipelago

- **Gold Skulltulas are seen through the same hash the build uses.** The location table had 24 Skulltulas' hashes swapped in cycles (in Dodongo's Cavern, Jabu-Jabu's Belly, the Fire, Water and Spirit Temples, Hyrule Field, Lon Lon Ranch, Lake Hylia and the Lost Woods), so in 0.9.0-nightly.20261125.14 killing one could send another Skulltula's check. 0.9.0-nightly.20261125.15 already ignored those wrong hashes because two spots shared them. The client now reads each Skulltula's own hash directly, so this no longer depends on that. *Not checked in game.*
- **Cosmetics apply to Archipelago mods.** Building from a `.apootrr` file drew the title line but skipped the colours, music and sound effects, so the tunic stayed its default colour. It now runs the same cosmetics step as Generate, with the choices from the Cosmetics page.
- **The Plandomizer works in Archipelago rooms.** The player file the app saves now carries your Plandomizer entries, as the OOTRR option `ootrr_plando`. The OOTRR world places them itself, so they work even though Archipelago's own item plando is off unless the host turns it on. An entry that names an item or location these settings do not have stops generation with a message that says which. Make the player file again to include them.
