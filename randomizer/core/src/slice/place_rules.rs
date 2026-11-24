use crate::logic_items::ItemId;
use crate::slice::data::Data;
use crate::slice::support::Plan;
use crate::world::{LocId, World};

pub struct Spot<'a> {
    pub d: &'a Data,
    pub w: &'a World,
    pub p: &'a Plan,
    pub it: ItemId,
    pub l: LocId,
    pub name: &'a str,
    pub item: &'a str,
}

pub type Rule = fn(&Spot) -> Option<bool>;

/// The rules the settings ask for: they stay when every item may go everywhere.
pub const SETTING_RULES: [(&str, Rule); 7] = [
    ("ganon's boss key on the light arrows cutscene", lacs_key),
    ("dungeon rewards at the end of dungeons", end_of_dungeons),
    ("excluded locations", excluded),
    ("silver rupees", silver),
    ("adult trade items", adult_trade),
    ("skipped locations", skipped),
    ("song spots", songs),
];

pub fn setting_verdict(s: &Spot) -> Option<bool> {
    SETTING_RULES.iter().find_map(|(_, r)| r(s))
}

pub const RULES: [(&str, Rule); 2] = [
    ("native hideout guard drops", native_hideout),
    ("hideout small keys", hideout_key),
];

pub fn verdict(s: &Spot) -> Option<bool> {
    RULES.iter().find_map(|(_, r)| r(s))
}

fn native_hideout(s: &Spot) -> Option<bool> {
    if !crate::hideout_keys::is_native(s.name) {
        return None;
    }
    let i: &crate::slice::data::Item = s.d.item(s.item)?;
    Some(crate::gerudo_drops::eligible(s.d, &i.name))
}

fn hideout_key(s: &Spot) -> Option<bool> {
    if s.p.hideout == crate::slice::support::Keep::Vanilla {
        return None;
    }
    let i: &crate::slice::data::Item = s.d.item(s.item)?;
    if i.kind != "HideoutSmallKey" {
        return None;
    }
    let here: Option<&str> = s.w.regions[s.w.locs[s.l].region].dungeon.as_deref();
    let dungeon: bool = here.is_some();
    let place: bool = match s.p.hideout {
        crate::slice::support::Keep::AnyDungeon => dungeon,
        _ => true,
    };
    let chest: bool = s.d.flow.by_name(s.name).is_none() && !s.d.special(s.name) && crate::slice::dungeon_items::chest_ok(s.d, s.name) && s.d.cat.by_name(s.name).is_some_and(|c| c.kind == "chest");
    let gift: bool = crate::gift_keys::spot(s.d, s.name);
    Some(place && i.dungeon.is_some() && (chest || gift))
}

/// The adult trade chain keeps its items at home: the sequence is not shuffled, unless adult trade
/// shuffle makes each trade item its own item (crate::open_world::trade_shuffle).
fn adult_trade(s: &Spot) -> Option<bool> {
    if s.p.rewards.trade_items {
        return None;
    }
    let child: [&str; 2] = ["Item_WeirdEgg", "Item_ZeldaLetter"];
    s.d.item(s.item)
        .filter(|i| i.actor.as_deref().is_some_and(|a| crate::flow_extra::trade::chain(a) && !child.contains(&a)))
        .is_some_and(|i| !crate::flow_extra::trade::keeps(s.d, s.name, i))
        .then_some(false)
}

fn lacs_key(s: &Spot) -> Option<bool> {
    if s.p.keys.ganon != crate::slice::support::Keep::Lacs {
        return None;
    }
    if s.name == crate::lacs_key::LOC {
        return Some(s.item == crate::lacs_key::ITEM);
    }
    (s.item == crate::lacs_key::ITEM).then_some(false)
}

/// Dungeon Rewards = End of dungeons: a Stone or Medallion only goes to a boss or Rauru reward
/// event, and those events only take a Stone or Medallion. A fitting pair is left to the other rules.
fn end_of_dungeons(s: &Spot) -> Option<bool> {
    if !s.p.rewards.dungeon_own {
        return None;
    }
    let item: bool = s.d.item(s.item).is_some_and(|i| i.kind == "DungeonReward");
    let spot: bool = s.d.flow.by_name(s.name).is_some_and(|l| l.cat == crate::slice::support::DUNGEON_REWARD_CAT);
    (item != spot).then_some(false)
}

fn excluded(s: &Spot) -> Option<bool> {
    (!s.p.excluded.is_empty() && s.p.excluded.iter().any(|e| *e == s.name) && !junk(s.d, s.item)).then_some(false)
}

pub const JUNK: [&str; 17] = [
    "Rupee (1)",
    "Rupees (5)",
    "Rupees (20)",
    "Bomb (1)",
    "Bombs (5)",
    "Bombs (10)",
    "Bombs (20)",
    "Arrows (5)",
    "Arrows (10)",
    "Arrows (30)",
    "Deku Nuts (5)",
    "Deku Nuts (10)",
    "Deku Stick (1)",
    "Deku Sticks (3)",
    "Deku Seeds (30)",
    "Recovery Heart",
    "Magic Jar (Large)",
];

pub fn junk(d: &Data, item: &str) -> bool {
    JUNK.contains(&item) || d.item(item).is_some_and(|i| !i.adv && i.kind == "Shop" && item != "Sold Out")
}

pub const LOW_JUNK: [(&str, u64); 8] = [
    ("Rupee (1)", 35),
    ("Rupees (5)", 35),
    ("Arrows (5)", 6),
    ("Arrows (10)", 4),
    ("Bombs (5)", 6),
    ("Deku Nuts (5)", 5),
    ("Deku Sticks (3)", 5),
    ("Deku Seeds (30)", 4),
];

pub fn roll_low_junk(w: &World, l: LocId, ok: &dyn Fn(ItemId, LocId) -> bool, rng: &mut crate::rng::Rng) -> Option<ItemId> {
    let cands: Vec<(ItemId, u64)> = LOW_JUNK.iter().filter_map(|(n, wt)| w.item(n).filter(|i| ok(*i, l)).map(|i| (i, *wt))).collect();
    let total: u64 = cands.iter().map(|c| c.1).sum();
    if total == 0 {
        return None;
    }
    let mut r: u64 = rng.below(total);
    for (i, wt) in cands {
        if r < wt {
            return Some(i);
        }
        r -= wt;
    }
    None
}

fn silver(s: &Spot) -> Option<bool> {
    crate::slice::silver::at(s.d, s.w, s.p, s.it, s.l)
}

fn skipped(s: &Spot) -> Option<bool> {
    (crate::slice::edit::skipped_locations(s.p).contains(&s.name) && !s.d.item(s.item).is_some_and(|i| i.actor.is_some() || !i.tiers.is_empty())).then_some(false)
}

fn songs(s: &Spot) -> Option<bool> {
    crate::slice::song_spots::gate(s.d, s.p, s.name, s.item)
}

