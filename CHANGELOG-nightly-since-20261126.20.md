# OOTRR nightly: changes since 0.9.0-nightly.20261126.20

## New settings

- **Full Wallets** (Logic > Full Wallets, off by default). A new game starts with a full wallet, and each Adult's Wallet or Giant's Wallet fills to its new size, with the rupee counter on screen. This was a greyed-out entry before. Default seeds are unchanged. *Checked in game: chests, pots, a Deku Scrub sale, an NPC gift, a shop purchase, a ground pickup, the Gold Skulltula reward, and a new game with and without the Adult's Wallet. The Great Fairy reward was checked only by script.*
- **Skip Zelda Meeting** (on in the Easy and Open World presets). A new game starts after the castle visit: Malon and Talon are back at Lon Lon Ranch, Zelda and Impa have been met, the Kakariko gate is open and the Happy Mask Shop quest has started. Whatever is placed at Zelda's Letter and the Song from Impa is given when the game starts, and the Letter and the Weird Egg leave the item pool. It needs Kokiri Forest set to Open. This is what Kakariko Gate: Open used to do.
- **Kakariko Gate** now only opens the gate. Malon, Talon, Zelda and Impa stay where they are, and their checks stay. Skip Zelda Meeting opens the gate too, whatever this says.
- **Shuffle Weird Egg** (on by default, which matches the old behaviour). The egg can be anywhere, and Malon's castle gift holds a random item. The egg hatches wherever you carry it, and you need its cucco to wake Talon, so the Zelda meeting waits for the egg. With Skip Zelda Meeting on, this is locked to Off.
- **Time Always Passes is gone from the menu.** The clock always runs now. Old settings strings still load.

## Ice Traps

- **No song- or Ocarina-look trap at a gift where Link holds the item up.** Songs and Ocarinas have no item-get animation, so a trap that looked like one made Link reach up for nothing until the game crashed. This was seen as Bolero of Fire at the Kakariko Shooting Gallery. This covers the minigame and NPC gifts, and the reward scenes the cutscene skips turn into an item get: the bosses, Rauru, Saria, the Light Arrows cutscene and the Ocarina of Time.
- **Trap chests no longer hand out a trap chest.** With Ice Traps on, the six trap chests become ordinary shuffled chests and add Rupees (5) to the pool. A chest that got one of those handed Link the trap chest itself and froze the game (seen at the Deku Tree Compass Room Side Chest). They are blue rupees now.
- **The spoiler names every trap** as "Ice Trap (looks like X)", in the chest list and in a new Ice Traps section that also covers shops, Gold Skulltulas and rewards. Hints still show a trap as Rupee (1).

## Fixes

- **Sheik at Colossus no longer softlocks with ammo or an upgrade as the Requiem reward.** With Bombs (5) there, for example, the cutscene never started properly: the HUD was hidden, no item came, and Link stuttered while walking. The report blamed Ice Traps, which weren't involved. *Not checked in game.*
- **Treasure Chest Game:** ammo that needs a bag in the first reward chest now gives rupees to a player without the bag, as other checks do.
- **King Zora's thaw gift** keeps its shuffled item when the Iron Boots chest edit is also applied to the same flow.
- **Malon's castle gift** no longer hands out the Weird Egg whatever her check holds.
- **The in-game music** plays from the start of a new game. Before, the skipped opening left it off until Link's house door scene, which is also skipped.
- **Kokiri Forest (open forest):** every Kokiri but Mido and Saria acts as they do after the Deku Tree from a new game on. Mido keeps his "bring a sword" talk until Gohma is beaten and never blocks the path.
- **Gerudo Valley bridge:** it is repaired by the carpenters' rescue (or the bridge setting), no longer by the Gerudo Card. Before, the bridge stayed broken after a rescue whose card scene held something else.
- **Zelda's escape** no longer waits for the Zelda meeting. It follows the Door of Time requirement alone.
- **The rolling Goron and the jailed carpenters** behave correctly after their quests. Their schedules applied the wrong condition.

## Logic

- **DMC Volcano Freestanding PoH:** adult with a planted bean only. The Hover Boots trick is no longer accepted.
- **Zora's River:**
  - Adult crosses the boulders without explosives or the Hammer.
  - The two freestanding Pieces of Heart need child or the Hover Boots (the bean and the river tricks no longer count).
  - Going from behind the waterfall into Zora's Domain needs Zelda's Lullaby.
- **Lake Hylia to Zora's Domain:** adult can also take this way after the Water Temple, with a dive or the Iron Boots.
- **Spirit Temple (child):** the area past the metal bridges needs the Boomerang or the five early-torch Silver Rupees, and no explosives.
- **The Spiritual Stones requirement** no longer needs a child visit to the castle garden.

## Cosmetics

- **New colour parts:**
  - Tunics: the Kokiri Tunic's cloth weave and its ropes and collar, and the Zora Tunic's pattern, ropes and feather.
  - Swords: the Master Sword hilt and blade, and the Biggoron Sword hilt, gold and blade.
  - Shields: the Hylian Shield trim and the Mirror Shield mirror.
  - Gear: the Silver and Golden Gauntlets, and Goron's Bracelet and its gem.
  - Link: his eyes.
- Metal parts (blades, trims) are recoloured by brightness, so they stay metallic.
- A part split from another follows that part's colour until you give it its own.

## Seed files

- **Generate writes `Seed.ootrr` next to `Spoiler.txt`** when the spoiler log is on. That's the file the Import menu reads, so a built seed can be imported again or shared. A build without a spoiler writes no seed file and removes an old one.
