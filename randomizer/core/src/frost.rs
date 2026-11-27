pub mod chest;
pub mod fake;
pub mod ground;
pub mod keys;
pub mod sensor;
pub mod trigger;
pub mod typo;

use std::collections::BTreeMap as BM;
use std::fs;
use std::path::{Path as P, PathBuf as PB};

use crate::bfevfl::Bf;
use crate::byml::{Byml, Node};
use crate::error::{Error as E, Res};
use crate::flow_locations::dict_id;
use crate::modpack::{Host, Pack};
use crate::msbt::{self, Msbt};
use crate::pickup_pack::actor::{pouch_file, POUCH_ACTOR};
use crate::pickup_pack::closure::{actor_file, work};
use crate::pickup_pack::rows::{self, ACTOR_INFO, GAME_ACTOR_INFO, POUCH_INFO};
use crate::pickup_pack::write::start;
use crate::pickup_pack::{pack_of, sizes, PACK_DICT};
use crate::progressive::{ANALYZED_TABLE, SETTING_TABLE};
use crate::rng::Rng;
use crate::romfs::Romfs;
use crate::rsizetable::{self, Rst, TABLE_PATH};
use crate::sarc::{self, Entry};
use crate::settings::seed::SeedFile;
use crate::slice::data::Data;
use crate::slice::edit::Built;
use crate::staging::{BANC_DICT, MARGIN};
use crate::zs;
use crate::TITLE_ID;

pub const TRAP: &str = "Ice Trap";
pub const LOOKS: [&str; 24] = [
    "Ocarina",
    "Slingshot",
    "Boomerang",
    "Bow",
    "Bomb Bag",
    "Progressive Hookshot",
    "Megaton Hammer",
    "Lens of Truth",
    "Dins Fire",
    "Farores Wind",
    "Nayrus Love",
    "Fire Arrows",
    "Ice Arrows",
    "Light Arrows",
    "Iron Boots",
    "Hover Boots",
    "Goron Tunic",
    "Zora Tunic",
    "Mirror Shield",
    "Bottle",
    "Kokiri Sword",
    "Progressive Strength Upgrade",
    "Biggoron Sword",
    "Progressive Scale",
];
const NO_SONG: &str = "Scarecrow Song";
pub const SETTING: &str = "shuffle_ice_traps";
pub const COUNT: &str = "ice_trap_count";
pub const NUM: u16 = 204;
pub const COUNT_NUM: u16 = 205;
/// The most Ice Traps a seed can ask for, for now.
pub const MAX: i64 = 10;
pub const DEFAULT: i64 = crate::slice::trap_chests::NAMES.len() as i64;
const GROUND_KINDS: [&str; 4] = ["freestanding", "heart", "bossheart", "key"];
const POUCH_MSG: &str = "ActorMsg/PouchContent.msbt";
const CHILD: &str = "Player_Child";
const CHILD_CONDITION: &str = "Component/ConditionParam/Player_Child.game__component__ConditionParam.bgyml";
const CHILD_LINK: &str = "Component/ELink/Player.engine__component__ELinkParam.bgyml";
const ADULT_USER: &str = "Player_Adult";
const CHILD_USER: &str = "OOTRR_Player_Child_Cold";
const SHELLS: usize = 16;
const BONE_BIT: u64 = 1 << 12;
const SCALE_BIT: u64 = 1 << 13;
const CHILD_SIZE: f32 = 0.5;
const GET_TEXT: &str = "FOOL!";
const FROZEN_CALL: &str = "Condition_Frozen";
const FULL_BODY_CALL: &str = "全身凍結";
const PARAM_AT: usize = 0x20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    Chest,
    Ground,
    Shop(i64),
    Skull,
    Holder,
    Flow,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trap {
    pub loc: String,
    pub place: Place,
    pub look: String,
    pub item: String,
    pub actor: String,
    pub fake: String,
    pub kit: String,
}

pub fn fake_name(actor: &str) -> String {
    format!("OOTRR_{actor}_Cold")
}

/// What a trap's stand-in keeps of the pouch row of the item it looks like: how its get is shown.
const SHOWN: [&str; 2] = ["BundleActorNum", "PouchGetType"];
const CATEGORY: &str = "PouchCategory";
const TAB: &str = "PouchTabCategory";
/// The category of a plain key item (a medallion, a dungeon map).
pub const PLAIN: &str = "KeyItem";
const NO_DEAL: &str = "None";

/// The pouch row columns that change what Link owns or can do when the row's item is added: bag
/// sizes and refills, lift grade, dive depth, magic, equipment, weapons, bottles, bundles.
pub const EFFECTS: [&str; 32] = [
    "ActorFullOnlyIfIsGetWhenGetBag",
    "AdultEquipment",
    "AfterExpansionMaxNum",
    "ArmorEffectType",
    "AssignableItem",
    "BottleItem",
    "BowQuiverName",
    "BundleActor",
    "ChildEquipment",
    "DiveDepth",
    "EquipmentPerformance",
    "ExpansionAvility",
    "ExpansionMaxNumActor",
    "FixedMaxNum",
    "HitPointRecover",
    "IsAbleToGetBagIfGet",
    "IsEquipDerivationActor",
    "IsFairy",
    "IsItemStockMaxIfGet",
    "LiftableGrade",
    "MPMax",
    "MPRecover",
    "MaxNumGameData",
    "OcarinaTypeValue",
    "ShootableActor",
    "SmallBagActor",
    "WeaponSheathName",
    "WeaponSubType",
    "WeaponType",
    "IsUsable",
    "IsUsableSelectItemInWater",
    "IsUsableSelectItemRiding",
];

/// The pouch row of a trap's stand-in. It must not be the row of the item it looks like: that row
/// makes the stand-in a working copy (a second Slingshot with no bullet bag, a bag or scale that
/// raises what Link can carry or do, an Ocarina that plays), and the freeze only takes it out
/// afterwards, if it runs at all. So the stand-in is a plain key item that never enters the
/// pouch, like a dungeon map: only how its get is shown stays.
pub fn inert(look: &Node, name: &str) -> Node {
    let mut r: Node = look.clone();
    let gone: Vec<String> = match &r {
        Node::Map(m) => m.iter().map(|e| e.0.clone()).filter(|k| !SHOWN.contains(&k.as_str())).collect(),
        _ => Vec::new(),
    };
    for k in &gone {
        r.remove(k);
    }
    r.set(rows::ROW_ID, Node::Str(name.to_string()));
    r.set(CATEGORY, Node::Str(PLAIN.to_string()));
    r.set(TAB, Node::Str(PLAIN.to_string()));
    r.set(rows::DEAL, Node::Str(rows::QUIET_DEAL.to_string()));
    r
}

/// Takes what an item does out of the row of the pickup, kit or shelf that hands a stand-in out.
/// Its name, price and how it is shown stay.
pub fn disarm(r: &mut Node) {
    for k in EFFECTS {
        r.remove(k);
    }
    let deal: Option<&str> = r.get(rows::DEAL).and_then(|v| v.as_str());
    if deal.is_some_and(|x| x != NO_DEAL && x != rows::QUIET_DEAL) {
        r.set(rows::DEAL, Node::Str(NO_DEAL.to_string()));
    }
}

/// The item a trap's location holds in the prepared seed when the trap does not show as the item
/// placed there. Only a rupee: a chest that holds an upgrade is sent to the upgrade chain and a
/// chest that holds ammo to the ammo guard, whatever is put inside it afterwards, so either would
/// hand out the real thing in place of the trap.
fn stand_in(held: Option<&String>) -> String {
    held.filter(|h| crate::slice::silver::junk(h).is_some()).cloned().unwrap_or_else(|| fake::RUPEE_ITEM.to_string())
}

impl Trap {
    pub fn trigger_names(&self) -> Vec<String> {
        match self.place {
            Place::Chest => vec![self.fake.clone()],
            Place::Ground => vec![self.fake.clone(), ground::name_for(&self.fake)],
            Place::Shop(p) => vec![self.fake.clone(), crate::shelf::shelf_name(&self.actor, p as i32)],
            Place::Skull => vec![self.fake.clone(), self.kit.clone()],
            Place::Holder => vec![self.fake.clone(), ground::name_for(&self.fake)],
            Place::Flow => vec![self.fake.clone()],
        }
    }
}

fn spheres(w: &crate::world::World, place: &crate::world::Placement) -> BM<usize, usize> {
    let mut s: crate::search::Search = crate::search::Search::new(w);
    let mut out: BM<usize, usize> = BM::new();
    let mut n: usize = 0;
    let mut found: Vec<usize> = s.settle(place, false);
    while !found.is_empty() {
        for l in &found {
            out.entry(*l).or_insert(n);
            if let Some(i) = place.get(*l) {
                s.add(i, 1);
            }
        }
        n += 1;
        found = s.settle(place, false);
    }
    out
}

fn pool(d: &Data, seed: &SeedFile) -> Vec<String> {
    let set: Option<crate::settings::Settings> = seed.settings().ok().map(|l| l.settings);
    let is = |id: &str, v: &str| set.as_ref().and_then(|s| s.text(id).ok()).is_some_and(|x| x == v);
    let mut out: Vec<String> = LOOKS.iter().map(|x| x.to_string()).collect();
    for i in &d.items {
        let song: bool = i.kind == "Song" && i.name != NO_SONG && is("shuffle_songs", "anywhere");
        let reward: bool = i.kind == "DungeonReward" && is("shuffle_dungeon_rewards", "anywhere");
        if (song || reward) && i.actor.is_some() {
            out.push(i.name.clone());
        }
    }
    out.extend(keys::looks(d));
    out.retain(|n| d.item(n).is_some_and(|i| i.actor.is_some()));
    out
}

fn place_of(d: &Data, seed: &SeedFile, loc: &str) -> Res<Place> {
    let kind: Option<&str> = d.cat.by_name(loc).map(|l| l.kind.as_str());
    Ok(if crate::pot_slice::targets::is_loc(d, loc) {
        Place::Holder
    } else if flow_spot(d, loc) {
        Place::Flow
    } else if crate::shops::is_loc(loc) {
        Place::Shop(seed.shops.get(loc).map(|s| s.price).ok_or_else(|| E::Msg(format!("{loc} holds {TRAP} but has no shop entry")))?)
    } else if kind == Some("chest") {
        Place::Chest
    } else if kind.is_some_and(|k| GROUND_KINDS.contains(&k)) {
        Place::Ground
    } else if kind == Some("gs") {
        Place::Skull
    } else {
        return Err(E::Msg(format!("{loc} holds {TRAP}, which this build can only put in a chest, on the ground, in a shop or at a Gold Skulltula")));
    })
}

const FLOW_CATS: [&str; 6] = ["npc_gift", "cutscene_gift", "great_fairy", "gs_reward", "minigame", "dungeon_reward"];

fn flow_spot(d: &Data, loc: &str) -> bool {
    d.flow.by_name(loc).is_some_and(|l| {
        l.supported
            && FLOW_CATS.contains(&l.cat.as_str())
            && !l.edits.is_empty()
            && crate::slice::chest_targets::is_sale(d, loc)
            && d.flow_ok(loc)
            && !crate::slice::flow_rules::priced_sale(d, loc)
            && !crate::flow_extra::trade::is(loc)
            && !crate::flow_extra::cows::is(loc)
            && !crate::flow_extra::frogs::is(loc)
    })
}

fn holder_spot(d: &Data, loc: &str) -> bool {
    crate::pot_slice::targets::is_loc(d, loc) && d.pots.state_of(loc) == Some(crate::slice::data::State::Eligible)
}

pub fn can_hold(d: &Data, loc: &str) -> bool {
    holder_spot(d, loc) || flow_spot(d, loc) || crate::shops::is_loc(loc) || d.cat.by_name(loc).is_some_and(|l| l.kind == "chest" || l.kind == "gs" || GROUND_KINDS.contains(&l.kind.as_str()))
}

fn tier_of(d: &Data, item: &str, held: usize) -> Option<(String, bool)> {
    let i = d.item(item)?;
    if i.tiers.is_empty() {
        return (held == 0).then(|| (i.actor.clone().unwrap_or_default(), false));
    }
    i.tiers.get(held).map(|t| (t.clone(), true))
}

/// Whether the gift at `loc` has Link hold up an item that has no animation (a song). The hold up
/// plays Rotate_GetDemo on the item actor, and a stand-in is built on the actor it looks like, so
/// Link reached up for nothing and the game crashed (a Bolero-look trap at the Kakariko Shooting
/// Gallery). A real song there is handed out as its pickup actor (crate::ocarina_hold); a trap is not.
/// The reward scenes the story skips turn into the item get (the bosses, Rauru, Saria, the Light
/// Arrows, the Ocarina of Time) hold the item up too, whatever their vanilla gift did.
fn held_without_anim(d: &Data, loc: &str, actor: &str) -> bool {
    crate::ocarina_hold::no_anim(actor) && (d.flow.by_name(loc).is_some_and(crate::slice::flow_rules::holds_up) || crate::ocarina_hold::scene_locs().iter().any(|l| l == loc))
}

fn fits(d: &Data, place: Place, loc: &str, item: &str, actor: &str, tiered: bool) -> bool {
    if keys::is_key(d, item) {
        return matches!(place, Place::Shop(_));
    }
    let tier: Option<&str> = tiered.then_some(actor);
    match place {
        Place::Chest => true,
        Place::Ground | Place::Holder => crate::slice::key_spots::resolve(d, item, tier).is_ok(),
        Place::Flow => !tiered && !held_without_anim(d, loc, actor) && (d.flow.usable(actor) || d.flow.by_name(loc).is_some_and(|l| l.name == crate::slice::flow_rules::SARIA || crate::slice::flow_rules::any_item(l))),
        Place::Skull => crate::slice::key_spots::resolve_kit(d, item, tier).is_ok_and(|p| p.hang_with().is_ok()),
        Place::Shop(_) => !tiered && d.item(item).is_some_and(|i| i.kind == "Item"),
    }
}

fn looks(d: &Data, seed: &SeedFile, places: &BM<String, Place>) -> Res<BM<String, (String, String, bool)>> {
    let (w, place): (crate::world::World, crate::world::Placement) = crate::slice::starts::rebuild(d, seed)?;
    let at: BM<usize, usize> = spheres(&w, &place);
    let names: Vec<String> = pool(d, seed);
    let held = |item: &str, here: usize| -> usize {
        let Some(it) = w.item(item) else { return 0 };
        let start: usize = w.start.iter().filter(|(i, _)| *i == it).map(|(_, n)| *n as usize).sum();
        if here == usize::MAX {
            return start;
        }
        start + w.real.iter().filter(|l| place.get(**l) == Some(it) && at.get(*l).is_some_and(|s| *s < here)).count()
    };
    let skull = |l: &str| d.cat.by_name(l).is_some_and(|c| c.kind == "gs");
    let at_skulls: Vec<&String> = seed.placements.iter().filter(|(l, i)| *i != TRAP && skull(l)).map(|(_, i)| i).collect();
    let at_shops: Vec<&String> = seed.placements.iter().filter(|(l, i)| *i != TRAP && crate::shops::is_loc(l)).map(|(_, i)| i).collect();
    let mut out: BM<String, (String, String, bool)> = BM::new();
    for (loc, pl) in places {
        let here: usize = w.loc(loc).and_then(|l| at.get(&l).copied()).unwrap_or(usize::MAX);
        let taken = |n: &String| match pl {
            Place::Skull => at_skulls.contains(&n),
            Place::Shop(_) => at_shops.contains(&n),
            _ => false,
        };
        let pick = |late: bool| -> Vec<(String, String, bool)> {
            names
                .iter()
                .filter(|n| !taken(n))
                .filter_map(|n| keys::actor(d, n).map(|a| (a, false)).or_else(|| tier_of(d, n, if late { held(n, here) } else { held(n, usize::MAX) })).map(|(a, t)| (n.clone(), a, t)))
                .filter(|(n, a, t)| fits(d, *pl, loc, n, a, *t))
                .collect()
        };
        let mut c: Vec<(String, String, bool)> = pick(true);
        if c.is_empty() {
            c = pick(false);
        }
        if c.is_empty() {
            return Err(E::Msg(format!("{loc} holds {TRAP} but no key item can show there")));
        }
        let mut r: Rng = Rng::from_text(&format!("{}/{loc}/look", seed.seed));
        out.insert(loc.clone(), c.swap_remove(r.below(c.len() as u64) as usize));
    }
    Ok(out)
}

pub fn prepare(seed: &SeedFile) -> Res<(SeedFile, Vec<Trap>)> {
    if !seed.placements.values().any(|v| v == TRAP) {
        return Ok((seed.clone(), Vec::new()));
    }
    let d: Data = Data::load()?;
    let mut places: BM<String, Place> = BM::new();
    for (loc, item) in &seed.placements {
        if item == TRAP {
            places.insert(loc.clone(), place_of(&d, seed, loc)?);
        }
    }
    let chosen: BM<String, (String, String, bool)> = looks(&d, seed, &places)?;
    let mut out: SeedFile = seed.clone();
    let mut traps: Vec<Trap> = Vec::new();
    let mut short: Vec<String> = shortfall(&d, seed);
    for (loc, place) in places {
        let (look, actor, tiered): (String, String, bool) = chosen.get(&loc).cloned().ok_or_else(|| E::Msg(format!("{loc} has no look")))?;
        let carry: bool = tiered || matches!(place, Place::Ground | Place::Skull | Place::Holder);
        let pick: Option<usize> = short.iter().position(|x| !d.item(x).is_some_and(|i| i.adv)).or(if short.is_empty() { None } else { Some(0) });
        let held: Option<String> = pick.map(|k| short.remove(k));
        let stand: String = if carry { stand_in(held.as_ref()) } else { look.clone() };
        if let Some(h) = &held {
            *out.copies.entry(h.clone()).or_insert(0) -= 1;
        }
        *out.copies.entry(stand.clone()).or_insert(0) += 1;
        let fake: String = fake_name(&actor);
        let mut kit: String = String::new();
        match place {
            Place::Ground | Place::Holder => {
                let g: String = ground::name_for(&fake);
                if tiered {
                    out.tiers.insert(g.clone(), actor.clone());
                }
                out.named.insert(g.clone(), look.clone());
                if place == Place::Holder {
                    out.drops.insert(loc.clone(), g);
                }
            }
            Place::Skull => {
                let tier: Option<&str> = tiered.then_some(actor.as_str());
                let t: crate::slice::skulls::Table = crate::slice::skulls::table()?;
                kit = crate::slice::skulls::drop_actor(&t, &d, &look, tier)?;
                if crate::slice::skulls::pickup(&t, &d, &look).is_none() {
                    out.kits.insert(kit.clone(), look.clone());
                    if let Some(a) = tier {
                        out.tiers.insert(kit.clone(), a.to_string());
                    }
                }
                out.drops.insert(loc.clone(), kit.clone());
            }
            Place::Shop(_) => {
                if let Some(s) = out.shops.get_mut(&loc) {
                    s.item = look.clone();
                }
            }
            Place::Flow => {
                out.drops.insert(loc.clone(), fake.clone());
            }
            Place::Chest => {}
        }
        out.placements.insert(loc.clone(), stand);
        out.cold.insert(loc.clone());
        traps.push(Trap { loc, place, look: look.clone(), item: look, fake, actor, kit });
    }
    Ok((out, traps))
}

pub fn sprinkle(d: &Data, s: &crate::settings::Settings, seed: &str, f: &mut SeedFile, excluded: &[String]) -> usize {
    if !s.flag(SETTING).unwrap_or(false) {
        return 0;
    }
    let n: usize = s.int(COUNT).unwrap_or(0).clamp(0, MAX) as usize;
    let contested: Vec<String> = crate::slice::skulls::shared().map(|t| t.contested.clone()).unwrap_or_default();
    let mut spots: Vec<String> = f
        .placements
        .iter()
        .filter(|(l, i)| {
            crate::slice::place_rules::junk(d, i)
                && !excluded.contains(l)
                && can_hold(d, l)
                && !crate::slice::silver::is_spot(l)
                && (!crate::shops::is_loc(l) || f.shops.get(*l).is_some_and(|e| e.deal && e.item == **i))
                && d.cat.by_name(l).is_none_or(|c| c.kind != "gs" || (c.conf != crate::catalog::Conf::Pool && !contested.contains(l)))
        })
        .map(|(l, _)| l.clone())
        .collect();
    let mut r: Rng = Rng::from_text(&format!("{seed}/cold"));
    let mut out: usize = 0;
    while out < n && !spots.is_empty() {
        let l: String = spots.swap_remove(r.below(spots.len() as u64) as usize);
        f.placements.insert(l.clone(), TRAP.to_string());
        if let Some(e) = f.shops.get_mut(&l) {
            e.item = TRAP.to_string();
        }
        out += 1;
    }
    out
}

fn shortfall(d: &Data, seed: &SeedFile) -> Vec<String> {
    let mut need: BM<String, isize> = BM::new();
    for (loc, item) in &seed.placements {
        if crate::pot_slice::targets::is_loc(d, loc) || crate::shops::seed::skip(loc) || crate::slice::silver::is_spot(loc) {
            continue;
        }
        if let Some(v) = d.pool_item(loc) {
            *need.entry(v.to_string()).or_insert(0) += 1;
        }
        if item != TRAP && !crate::slice::silver::is_item(item) {
            *need.entry(item.clone()).or_insert(0) -= 1;
        }
    }
    let mut out: Vec<String> = Vec::new();
    for (k, n) in need {
        let net: isize = crate::pot_slice::targets::delta(d, seed, &k) + crate::shops::seed::delta(seed, &k) + crate::slice::silver::delta(seed, &k);
        out.extend(std::iter::repeat_n(k, (n + net).max(0) as usize));
    }
    out
}

struct Ctx<'a> {
    rf: Romfs,
    base: &'a PB,
    modded: bool,
}

struct File {
    path: String,
    raw: Vec<u8>,
    old: usize,
    plain: bool,
}

impl Ctx<'_> {
    fn read(&mut self, path: &str) -> Res<Vec<u8>> {
        start(&mut self.rf, Some(self.base), path)
    }

    fn file(&mut self, path: &str, edit: impl FnOnce(&[u8]) -> Res<Vec<u8>>) -> Res<File> {
        let first: Vec<u8> = self.read(path)?;
        let raw: Vec<u8> = edit(&first)?;
        let old: usize = if self.modded { first.len() } else { self.rf.read_zs(path)?.len() };
        Ok(File { path: path.to_string(), raw, old, plain: false })
    }

    fn plain(&mut self, path: &str, edit: impl FnOnce(&[u8]) -> Res<Vec<u8>>) -> Res<File> {
        let own: PB = path.split('/').fold(self.base.clone(), |a, s| a.join(s));
        let first: Vec<u8> = if own.is_file() { fs::read(own)? } else { self.rf.read(path)? };
        let raw: Vec<u8> = edit(&first)?;
        let old: usize = if self.modded { first.len() } else { self.rf.read(path)?.len() };
        Ok(File { path: path.to_string(), raw, old, plain: true })
    }

    fn pack(&mut self, name: &str) -> Res<Vec<Entry>> {
        sarc::parse(&self.read(&pack_of(name))?)
    }
}

fn dict(rf: &Romfs, path: &str) -> Res<Option<u32>> {
    if !rf.root.join(path).is_file() {
        return Ok(if path.starts_with("Pack/") { Some(PACK_DICT) } else { None });
    }
    let id: Option<u32> = dict_id(&rf.read(path)?);
    Ok(if path.starts_with("Banc/") { Some(id.unwrap_or(BANC_DICT)) } else { id })
}

fn add_actor(ts: &mut [Byml; 3], from: &str, name: &str, pouch: Option<&str>) -> Res<()> {
    let h: i32 = rows::heap(&ts[0], from)?;
    let mut a: Node = rows::actor_row(rows::need(&ts[0], from, ACTOR_INFO)?, name, h);
    if a.get(rows::ACTOR_NAME).is_some() {
        a.set(rows::ACTOR_NAME, Node::Str(name.to_string()));
    }
    let g: Node = rows::game_row(rows::need(&ts[1], from, GAME_ACTOR_INFO)?, rows::need(&ts[1], from, GAME_ACTOR_INFO)?, name);
    let i: usize = rows::insert(&mut ts[0], a)?;
    let j: usize = rows::insert(&mut ts[1], g)?;
    if i != j {
        return Err(E::Msg(format!("{name} sits at {i} in the actor table and at {j} in the game actor table")));
    }
    if let Some(p) = pouch {
        let q: Node = rows::pouch_row(rows::need(&ts[2], p, POUCH_INFO)?, name);
        rows::insert(&mut ts[2], q)?;
    }
    Ok(())
}

fn tables(c: &mut Ctx, fakes: &[(String, String)], ids: &BM<String, String>, grounds: &[(String, String)]) -> Res<Vec<File>> {
    let firsts: Vec<Vec<u8>> = [ACTOR_INFO, GAME_ACTOR_INFO, POUCH_INFO].iter().map(|t| c.read(t)).collect::<Res<Vec<Vec<u8>>>>()?;
    let mut ts: [Byml; 3] = [Byml::parse(&firsts[0])?, Byml::parse(&firsts[1])?, Byml::parse(&firsts[2])?];
    add_actor(&mut ts, sensor::SUN, sensor::NAME, None)?;
    for (look, name) in fakes {
        let id: &str = ids.get(name).map(|x| x.as_str()).unwrap_or(look);
        let pouch: String = [id, look.as_str()].iter().find(|k| rows::find(&ts[2], k).is_some()).map(|k| k.to_string()).unwrap_or_else(|| fake::RUPEE_ROW.to_string());
        add_actor(&mut ts, look, name, Some(&pouch))?;
        let list: &mut Vec<Node> = ts[2].root.as_arr_mut().ok_or(E::Format("the pouch table is not a list"))?;
        let r: &mut Node = list.iter_mut().find(|r| rows::id(r) == Some(name.as_str())).ok_or_else(|| E::Msg(format!("the pouch table has no {name} row")))?;
        *r = inert(r, name);
    }
    for (g, fake) in grounds {
        let list: &mut Vec<Node> = ts[0].root.as_arr_mut().ok_or(E::Format("the actor table is not a list"))?;
        let r: &mut Node = list.iter_mut().find(|r| rows::id(r) == Some(g.as_str())).ok_or_else(|| E::Msg(format!("the actor table has no {g} row")))?;
        r.set(rows::ACTOR_NAME, Node::Str(fake.clone()));
        if let Some(q) = ts[2].root.as_arr_mut().and_then(|l| l.iter_mut().find(|r| rows::id(r) == Some(g.as_str()))) {
            disarm(q);
        }
    }
    let mut out: Vec<File> = Vec::new();
    for (i, t) in [ACTOR_INFO, GAME_ACTOR_INFO, POUCH_INFO].iter().enumerate() {
        let old: usize = if c.modded { firsts[i].len() } else { c.rf.read_zs(t)?.len() };
        out.push(File { path: t.to_string(), raw: ts[i].write()?, old, plain: false });
    }
    Ok(out)
}

fn text(raw: &[u8], fakes: &[(String, String)], seed: &str, english: bool) -> Res<Vec<u8>> {
    let mut es: Vec<Entry> = sarc::parse(raw)?;
    let b: Vec<u8> = sarc::get(&es, POUCH_MSG).ok_or(E::Format("a message archive has no PouchContent.msbt"))?.data.clone();
    let mut m: Msbt = msbt::parse(&b)?;
    let places: Option<Msbt> = sarc::get(&es, crate::dungeon_text::PLACES).map(|e| msbt::parse(&e.data)).transpose()?;
    for (look, name) in fakes {
        for sfx in ["_Name", "_Caption", "_GetCaption"] {
            let own: String = format!("{name}{sfx}");
            if m.find(&own).is_some() {
                continue;
            }
            if let (true, Some(p), Some(pl)) = (sfx == "_Name", keys::pack_of(look), places.as_ref()) {
                let mut r: Rng = Rng::from_text(&format!("{seed}/{name}/typo"));
                let t: Vec<crate::msbt::Tok> = keys::name(&m, pl, p, english, &mut r)?;
                m.put_tokens(&own, &t)?;
                continue;
            }
            if let Some(v) = m.raw(&format!("{look}{sfx}")).map(|x| x.to_vec()) {
                m.add_raw(&own, v)?;
                if sfx == "_Name" {
                    let t: Vec<crate::msbt::Tok> = match typo::old_name(name).filter(|_| english) {
                        Some(n) => msbt::from_markup(n)?,
                        None => typo::name(&m.text(&own)?, &mut Rng::from_text(&format!("{seed}/{name}/typo"))),
                    };
                    m.set_tokens(&own, &t)?;
                }
            }
        }
        let shop: String = format!("{name}_ShopCaption");
        if m.find(&shop).is_none() {
            if let Some(v) = ["_ShopCaption", "_Caption", "_GetCaption"].iter().find_map(|sfx| m.raw(&format!("{look}{sfx}")).map(|x| x.to_vec())) {
                m.add_raw(&shop, v)?;
            }
        }
        m.put_markup(&format!("{name}_GetMessage"), GET_TEXT)?;
    }
    let w: Vec<u8> = m.write()?;
    msbt::parse(&w)?;
    sarc::set(&mut es, POUCH_MSG, w);
    sarc::write(&es)
}

fn full_body(raw: &[u8]) -> Res<Vec<u8>> {
    let mut l: crate::difficulty::elink::Link = crate::difficulty::elink::Link::parse(raw)?;
    let mut n: usize = 0;
    for u in 0..l.users.len() {
        let (Some(f), Some(g)) = (l.find_key(u, FROZEN_CALL), l.find_key(u, FULL_BODY_CALL)) else { continue };
        let pos: u64 = l.users[u].calls[g].param_pos();
        l.users[u].calls[f].raw[PARAM_AT..PARAM_AT + 8].copy_from_slice(&pos.to_le_bytes());
        n += 1;
    }
    if n == 0 {
        return Err(E::Format("the effect table has no frozen and full body calls in one user"));
    }
    child_user(&mut l)?;
    l.write()
}

fn child_user(l: &mut crate::difficulty::elink::Link) -> Res<()> {
    if l.user(CHILD_USER).is_some() {
        return Ok(());
    }
    let a: usize = l.user(ADULT_USER).ok_or(E::Format("the effect table has no adult Link user"))?;
    let scale: u32 = l.direct_f32(CHILD_SIZE).ok_or(E::Format("the effect table has no 0.7 scale value"))?;
    let mut u: crate::difficulty::elink::User = l.users[a].clone();
    for k in 1..=SHELLS {
        let c: usize = l.find_key(a, &format!("Condition_Frozen_{k:02}")).ok_or_else(|| E::Msg(format!("adult Link has no frozen shell {k}")))?;
        let mut p: crate::difficulty::elink::Param = l.param_at(u.calls[c].param_pos()).cloned().ok_or(E::Format("a frozen shell has no field record"))?;
        if p.mask & BONE_BIT == 0 || p.mask & SCALE_BIT != 0 {
            return Err(E::Msg(format!("frozen shell {k} is not sized by a bone")));
        }
        let at: usize = (p.mask & (BONE_BIT - 1)).count_ones() as usize;
        p.mask = (p.mask & !BONE_BIT) | SCALE_BIT;
        *p.vals.get_mut(at).ok_or(E::Format("a frozen shell record is short"))? = scale;
        let pos: u64 = l.add_param(p);
        u.calls[c].raw[PARAM_AT..PARAM_AT + 8].copy_from_slice(&pos.to_le_bytes());
    }
    let h: u32 = crate::rsizetable::hash(CHILD_USER);
    u.hash = h;
    let at: usize = l.hashes.partition_point(|x| *x < h);
    l.users.push(u);
    let slot: usize = l.users.len() - 1;
    l.hashes.insert(at, h);
    l.slots.insert(at, slot);
    Ok(())
}

fn child(raw: &[u8]) -> Res<Vec<u8>> {
    let mut es: Vec<Entry> = sarc::parse(raw)?;
    let e: &Entry = sarc::get(&es, CHILD_CONDITION).ok_or(E::Format("child Link has no condition file"))?;
    let mut y: Byml = Byml::parse(&e.data)?;
    y.root.set("IsEnableFrozen", Node::Bool(true));
    sarc::set(&mut es, CHILD_CONDITION, y.write()?);
    let e: &Entry = sarc::get(&es, CHILD_LINK).ok_or(E::Format("child Link has no effect file"))?;
    let mut y: Byml = Byml::parse(&e.data)?;
    y.root.set("UserName", Node::Str(CHILD_USER.to_string()));
    sarc::set(&mut es, CHILD_LINK, y.write()?);
    sarc::write(&es)
}

fn shelf(raw: &[u8], shelf: &str, fake: &str) -> Res<Vec<u8>> {
    let mut es: Vec<Entry> = sarc::parse(raw)?;
    let f: String = pouch_file(shelf);
    let e: &Entry = sarc::get(&es, &f).ok_or_else(|| E::Msg(format!("the shelf actor {shelf} has no pouch file")))?;
    let mut y: Byml = Byml::parse(&e.data)?;
    y.root.set(POUCH_ACTOR, Node::Str(work(&actor_file(fake))));
    sarc::set(&mut es, &f, y.write()?);
    sarc::write(&es)
}

pub fn after(traps: &[Trap], seed: &str, romfs: &P, load: &P, mod_name: &str, host: Host, b: Built) -> Res<Built> {
    if traps.is_empty() {
        return Ok(b);
    }
    let d: Data = Data::load()?;
    let base: PB = b.root.join("romfs");
    let mut c: Ctx = Ctx { rf: Romfs::open(romfs)?, base: &base, modded: false };
    let mut rst: Rst = match Some(base.join(TABLE_PATH)).filter(|p| p.is_file()) {
        Some(p) => {
            c.modded = true;
            Rst::parse(&c.rf.zs.decompress(&fs::read(p)?)?)?
        }
        None => rsizetable::load(&mut c.rf)?,
    };
    let mut fakes: Vec<(String, String)> = traps.iter().map(|t| (t.actor.clone(), t.fake.clone())).collect();
    fakes.sort();
    fakes.dedup();
    let mut packs: Vec<(String, Vec<u8>, Vec<(String, usize)>, Vec<(String, usize)>)> = Vec::new();
    let sun: Vec<Entry> = c.pack(sensor::SUN)?;
    let frz: Vec<Entry> = c.pack(sensor::FREEZARD)?;
    let box4: Vec<Entry> = c.pack(sensor::TRAP_CHEST)?;
    let s: sensor::Made = sensor::make(&sun, &frz, &box4)?;
    packs.push((sensor::NAME.to_string(), s.pack, s.fresh, s.peers));
    let mut ids: BM<String, String> = BM::new();
    for (look, name) in &fakes {
        let lp: Vec<Entry> = c.pack(look)?;
        let f: fake::Fake = fake::make(&lp, look, name)?;
        ids.insert(f.name.clone(), f.id.clone());
        packs.push((f.name, f.pack, f.fresh, f.peers));
    }
    let mut grounds: Vec<(String, String)> = traps.iter().filter(|t| matches!(t.place, Place::Ground | Place::Holder)).map(|t| (ground::name_for(&t.fake), t.fake.clone())).collect();
    grounds.sort();
    grounds.dedup();
    let mut renamed: Vec<(String, String)> = grounds.clone();
    for t in traps {
        if let Place::Shop(p) = t.place {
            renamed.push((crate::shelf::shelf_name(&t.actor, p as i32), t.fake.clone()));
        }
        if t.place == Place::Skull {
            renamed.push((t.kit.clone(), t.fake.clone()));
        }
    }
    renamed.sort();
    renamed.dedup();
    let mut files: Vec<File> = tables(&mut c, &fakes, &ids, &renamed)?;
    for (g, fake) in &grounds {
        files.push(c.file(&pack_of(g), |r: &[u8]| shelf(r, g, fake))?);
    }
    let mut kits: Vec<(String, String)> = traps.iter().filter(|t| t.place == Place::Skull).map(|t| (t.kit.clone(), t.fake.clone())).collect();
    kits.sort();
    kits.dedup();
    for (k, fake) in &kits {
        files.push(c.file(&pack_of(k), |r: &[u8]| shelf(r, k, fake))?);
    }
    let info: Byml = c.rf.read_byml(ACTOR_INFO)?;
    let mut by_file: BM<String, BM<u64, chest::Swap>> = BM::new();
    for t in traps {
        let swap: chest::Swap = match t.place {
            Place::Chest => chest::Swap::Content(t.fake.clone()),
            Place::Ground => chest::Swap::Actor(ground::name_for(&t.fake), ground::lift(&d, &info, &t.look, &t.actor)),
            Place::Shop(_) | Place::Skull | Place::Holder | Place::Flow => continue,
        };
        let l = d.cat.by_name(&t.loc).ok_or_else(|| E::Msg(format!("{} is not in the location list", t.loc)))?;
        for r in &l.refs {
            for f in &r.files {
                by_file.entry(f.clone()).or_default().insert(r.hash, swap.clone());
            }
        }
    }
    for (f, hits) in &by_file {
        let mut hit: usize = 0;
        let file: File = c.file(f, |r: &[u8]| {
            let (out, n): (Vec<u8>, usize) = chest::banc(r, hits)?;
            hit = n;
            Ok(out)
        })?;
        if hit > 0 {
            files.push(file);
        }
    }
    for t in traps {
        if let Place::Shop(p) = t.place {
            let name: String = crate::shelf::shelf_name(&t.actor, p as i32);
            let fake: String = t.fake.clone();
            files.push(c.file(&pack_of(&name), |r: &[u8]| shelf(r, &name, &fake))?);
        }
    }
    let mut flows: BM<String, Vec<(&crate::flow_locations::Edit, String)>> = BM::new();
    for t in traps.iter().filter(|t| t.place == Place::Flow) {
        let l = d.flow.by_name(&t.loc).ok_or_else(|| E::Msg(format!("{} is not a flow location", t.loc)))?;
        for e in &l.edits {
            for f in std::iter::once(&e.file).chain(e.also.iter()) {
                flows.entry(crate::flow_locations::path_of(f)).or_default().push((e, t.fake.clone()));
            }
        }
    }
    for (path, es) in &flows {
        if [trigger::GET_FILE, trigger::GIVE_FILE, trigger::SHOP_FILE].contains(&path.as_str()) || trigger::PICKUPS.iter().any(|p| path.ends_with(&format!("/{p}.bfevfl.zs"))) {
            return Err(E::Msg(format!("{path} is both a trap reward and a shared get flow, this build cannot edit it twice")));
        }
        files.push(c.file(path, |r: &[u8]| {
            let mut b: Bf = Bf::parse(r)?;
            let f: &mut crate::bfevfl::Flow = b.flows.first_mut().ok_or(E::Format("a reward flow file holds no flow"))?;
            let mut grants: Vec<usize> = Vec::new();
            for (e, fake) in es {
                trigger::set_name(f, e.index, &e.event, &e.param, fake)?;
                if e.role == "grant" {
                    grants.push(e.index);
                }
            }
            trigger::mark_after(f, &grants)?;
            b.write()
        })?);
    }
    let mut names: Vec<String> = traps.iter().flat_map(|t| t.trigger_names()).collect();
    names.sort();
    names.dedup();
    let fake_names: Vec<String> = fakes.iter().map(|f| f.1.clone()).collect();
    files.push(c.file(trigger::GET_FILE, |r: &[u8]| {
        let mut b: Bf = Bf::parse(r)?;
        trigger::hook_get(&mut b, &names)?;
        b.write()
    })?);
    for flow in trigger::PICKUPS {
        files.push(c.file(&format!("Event/EventFlow/{flow}.bfevfl.zs"), |r: &[u8]| {
            let mut b: Bf = Bf::parse(r)?;
            trigger::hook_pickup(&mut b, flow, &names)?;
            b.write()
        })?);
    }
    if traps.iter().any(|t| matches!(t.place, Place::Shop(_))) {
        files.push(c.file(trigger::SHOP_FILE, |r: &[u8]| {
            let mut b: Bf = Bf::parse(r)?;
            trigger::hook_shop(&mut b, &names)?;
            b.write()
        })?);
    }
    files.push(c.file(trigger::GIVE_FILE, |r: &[u8]| {
        let mut b: Bf = Bf::parse(r)?;
        trigger::hook_give(&mut b, &names)?;
        trigger::add_event(&mut b, &fake_names)?;
        b.write()
    })?);
    files.push(c.file(SETTING_TABLE, trigger::setting)?);
    files.push(c.file(ANALYZED_TABLE, trigger::analyzed)?);
    files.push(c.file(crate::chest_slot::LIST, |r: &[u8]| {
        let mut y: Byml = Byml::parse(r)?;
        crate::ap::saved::add_bools(&mut y, &[trigger::FLAG.to_string()])?;
        y.write()
    })?);
    files.push(c.plain(trigger::MAIN, trigger::watch)?);
    let named: Vec<(String, String)> = fakes.iter().map(|(look, name)| (ids.get(name).cloned().unwrap_or_else(|| look.clone()), name.clone())).collect();
    for l in crate::hints::languages(&c.rf)? {
        files.push(c.file(&l, |r: &[u8]| text(r, &named, seed, crate::dungeon_text::is_english(&l)))?);
    }
    files.push(c.file(&pack_of(CHILD), child)?);
    files.push(c.file(crate::difficulty::elink::FILE, full_body)?);
    let pk: Pack = Pack::create_for(load, TITLE_ID, mod_name, host)?;
    let id: u32 = dict_id(&c.rf.read(&pack_of(sensor::SUN))?).unwrap_or(PACK_DICT);
    for (name, raw, fresh, peers) in &packs {
        let path: String = pack_of(name);
        rst.set_size(&path, sizes::pack_size(raw.len())?)?;
        for (f, len) in fresh {
            let n: u32 = sizes::file_size(&rst, peers, f, *len)?;
            rst.set_size(f, n)?;
        }
        pk.write_romfs(&path, &zs::compress_raw(raw, Some(id))?)?;
    }
    for f in &files {
        if f.plain {
            pk.write_romfs(&f.path, &f.raw)?;
        } else {
            pk.write_romfs(&f.path, &zs::compress_raw(&f.raw, dict(&c.rf, &f.path)?)?)?;
        }
        if f.raw.len() > f.old {
            rst.grow(&f.path, f.old as u64, f.raw.len() as u64, MARGIN)?;
        }
    }
    pk.write_romfs(TABLE_PATH, &rst.to_zs()?)?;
    if pk.root != b.root {
        return Err(E::Msg(format!("the files went to {} and not to the seed mod {}", pk.root.display(), b.root.display())));
    }
    Ok(b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{Settings, Value};
    use crate::slice::generate::{generate_data, GenOpts, Generated};

    /// A song has no animation, so a song-look trap at a gift that holds the item up (the shooting
    /// gallery prize) left Link reaching up for nothing until the game crashed.
    #[test]
    fn a_trap_at_a_gift_that_holds_the_item_up_never_looks_like_a_song() {
        let d: Data = Data::load().unwrap();
        let mut songs: Vec<String> = d.items.iter().filter(|i| i.kind == "Song").filter_map(|i| i.actor.clone()).collect();
        assert!(!songs.is_empty());
        songs.extend(crate::ocarina_hold::OCARINAS.iter().map(|a| a.to_string()));
        let scenes: Vec<String> = crate::ocarina_hold::scene_locs();
        let holds: Vec<String> = d.flow.locs.iter().filter(|l| (crate::slice::flow_rules::holds_up(l) || scenes.contains(&l.name)) && flow_spot(&d, &l.name)).map(|l| l.name.clone()).collect();
        assert!(holds.iter().any(|l| l == "Market Shooting Gallery Reward"), "{holds:?}");
        // The skips turn these reward scenes into the item get that holds the item up.
        for l in ["Queen Gohma", "Twinrova", crate::slice::flow_rules::SARIA] {
            if flow_spot(&d, l) {
                assert!(holds.iter().any(|h| h == l), "{l} is not checked: {holds:?}");
            }
        }
        assert!(holds.iter().any(|h| scenes.contains(h)), "no reward scene can hold a trap: {holds:?}");
        for l in &holds {
            for a in &songs {
                assert!(!fits(&d, Place::Flow, l, "", a, false), "{l} can show a trap as {a}");
            }
        }
        assert!(fits(&d, Place::Flow, "Market Shooting Gallery Reward", "Lens of Truth", "Item_GlassTruth", false));
        assert!(fits(&d, Place::Chest, "", "Song of Time", "Item_Melody_TimeSong", false));
    }

    fn real() -> Option<PB> {
        let p: PB = ootrr_game::test_romfs::path();
        if p.join("Pack").join("ZsDic.pack.zs").is_file() {
            Some(p)
        } else {
            eprintln!("skipped, no RomFS dump at {} (set OOTRR_ROMFS)", p.display());
            None
        }
    }

    fn wide() -> Settings {
        let mut s: Settings = Settings::default();
        for (k, v) in [("shuffle_pots", "all"), ("shuffle_crates", "all"), ("shuffle_rocks", "all"), ("shuffle_grass", "all"), ("shuffle_freestanding_items", "all"), ("shuffle_skulltulas", "pinned"), ("shuffle_songs", "anywhere"), ("shuffle_dungeon_rewards", "anywhere")] {
            s.set(k, Value::s(v)).unwrap();
        }
        s.set(SETTING, Value::Bool(true)).unwrap();
        s
    }

    #[test]
    fn traps_are_off_until_asked_for_and_at_most_ten() {
        let d: Data = Data::load().unwrap();
        let sch = crate::settings::schema();
        let (on, count) = (sch.get(SETTING).unwrap(), sch.get(COUNT).unwrap());
        assert_eq!(on.default, Value::Bool(false));
        assert_eq!((count.default.clone(), count.min, count.max), (Value::Int(DEFAULT), 1, MAX));
        assert_eq!(MAX, 10);
        let plain: Settings = Settings::default();
        assert!(!plain.flag(SETTING).unwrap());
        assert!(traps(&generate_data(&d, &plain, "cold default", &super::tests::on()).unwrap()).is_empty());
        let mut s: Settings = wide();
        assert_eq!(traps(&generate_data(&d, &s, "cold ten", &super::tests::on()).unwrap()).len(), DEFAULT as usize);
        assert!(s.set(COUNT, Value::Int(MAX + 1)).is_err());
        assert!(s.set(COUNT, Value::Int(0)).is_err());
        s.set(COUNT, Value::Int(1)).unwrap();
        assert_eq!(traps(&generate_data(&d, &s, "cold one", &super::tests::on()).unwrap()).len(), 1);
    }

    /// A string with no value for the setting comes from a release that had no traps.
    #[test]
    fn a_settings_string_without_the_setting_has_no_traps() {
        let mut on: Settings = Settings::default();
        on.set(SETTING, Value::Bool(true)).unwrap();
        on.set(COUNT, Value::Int(4)).unwrap();
        let (a, b): (String, String) = (on.encode().unwrap(), Settings::default().encode().unwrap());
        assert_ne!(a, b);
        let back: Settings = Settings::decode(&a).unwrap().settings;
        assert!(back.flag(SETTING).unwrap());
        assert_eq!(back.int(COUNT).unwrap(), 4);
        assert!(!Settings::decode(&b).unwrap().settings.flag(SETTING).unwrap());
    }

    fn traps(g: &Generated) -> Vec<String> {
        g.seed_file.placements.iter().filter(|(_, i)| *i == TRAP).map(|(l, _)| l.clone()).collect()
    }

    fn on() -> GenOpts {
        GenOpts { extras: true, ..GenOpts::default() }
    }

    #[test]
    fn the_set_count_replaces_junk_at_spots_that_can_hold_it() {
        let d: Data = Data::load().unwrap();
        let s: Settings = wide();
        for n in 0..3 {
            let seed: String = format!("cold count {n}");
            let plain: Generated = generate_data(&d, &s, &seed, &GenOpts::default()).unwrap();
            let g: Generated = generate_data(&d, &s, &seed, &on()).unwrap();
            let at: Vec<String> = traps(&g);
            assert_eq!(at.len(), DEFAULT as usize, "{seed}");
            assert!(traps(&plain).is_empty());
            for l in &at {
                assert!(can_hold(&d, l), "{l}");
                let was: &str = plain.seed_file.placements.get(l).map(|x| x.as_str()).unwrap_or("");
                assert!(crate::slice::place_rules::junk(&d, was), "{l} held {was}");
            }
            for (l, i) in &g.seed_file.placements {
                if !at.contains(l) {
                    assert_eq!(plain.seed_file.placements.get(l), Some(i), "{l}");
                }
            }
        }
    }

    #[test]
    fn the_setting_and_its_count_decide_how_many() {
        let d: Data = Data::load().unwrap();
        let mut s: Settings = wide();
        s.set(COUNT, Value::Int(3)).unwrap();
        assert_eq!(traps(&generate_data(&d, &s, "cold three", &on()).unwrap()).len(), 3);
        s.set(SETTING, Value::Bool(false)).unwrap();
        assert!(traps(&generate_data(&d, &s, "cold off", &on()).unwrap()).is_empty());
    }

    #[test]
    fn excluded_locations_never_hold_one() {
        let d: Data = Data::load().unwrap();
        let mut s: Settings = wide();
        let plain: Generated = generate_data(&d, &s, "cold excluded", &on()).unwrap();
        let first: Vec<String> = traps(&plain);
        s.set("disabled_locations", Value::Set(first.clone())).unwrap();
        let g: Generated = generate_data(&d, &s, "cold excluded", &on()).unwrap();
        assert!(traps(&g).iter().all(|l| !first.contains(l)));
    }

    #[test]
    fn prepared_seeds_hold_no_trap_and_pass_the_count_checks() {
        let d: Data = Data::load().unwrap();
        let s: Settings = wide();
        for n in 0..3 {
            let g: Generated = generate_data(&d, &s, &format!("cold prepare {n}"), &on()).unwrap();
            let (out, ts): (SeedFile, Vec<Trap>) = prepare(&g.seed_file).unwrap();
            assert_eq!(ts.len(), DEFAULT as usize);
            assert!(out.placements.values().all(|i| i != TRAP));
            crate::slice::chest_targets::actors_for(&d, &out).unwrap();
            let fixed = crate::slice::tier_order::fixed_actors(&d, &out).unwrap();
            assert!(fixed.bad.is_empty(), "{:?}", fixed.bad);
            crate::slice::seed_packs::need(&d, &out, &fixed.assign).unwrap();
            crate::slice::skull_edit::drops_with(&d, &out, &fixed.assign).unwrap();
            for t in &ts {
                assert!(LOOKS.contains(&t.item.as_str()) || d.item(&t.item).is_some_and(|i| i.kind == "Song" || i.kind == "DungeonReward" || i.kind == keys::SMALL), "{}", t.item);
            }
        }
    }

    fn row(id: &str, cols: &[(&str, Node)]) -> Node {
        let mut r: Node = Node::Map(Vec::new());
        r.set(rows::ROW_ID, Node::Str(id.to_string()));
        for (k, v) in cols {
            r.set(k, v.clone());
        }
        r
    }

    fn s(x: &str) -> Node {
        Node::Str(x.to_string())
    }

    /// The rows as the game ships them: a trap that showed as one of these used to be a copy of it.
    fn shipped() -> Vec<Node> {
        vec![
            row(
                "Weapon_Range_SlingShot",
                &[("ActionGuideType", s("SlingShot")), ("AssignableItem", s("SlingShot")), ("BundleActorNum", Node::I32(1)), ("EquipmentPerformance", Node::I32(1)), ("IsAutoSaveAfterGet", Node::Bool(true)), ("PouchCategory", s("Bow")), ("PouchGetType", s("Large")), ("PouchSpecialDeal", s("None")), ("PouchTabCategory", s("Equipment")), ("ShootableActor", s("Work/Actor/Item_DekuSeed.engine__actor__ActorParam.gyml")), ("WeaponType", s("Bow"))],
            ),
            row(
                "Item_BombBag_Medium",
                &[("AfterExpansionMaxNum", Node::I32(30)), ("BundleActorNum", Node::I32(1)), ("ExpansionMaxNumActor", s("Work/Actor/Item_Bomb.engine__actor__ActorParam.gyml")), ("IsItemStockMaxIfGet", Node::Bool(true)), ("MaxNumGameData", s("Bomb")), ("PouchCategory", s("KeyItem")), ("PouchGetType", s("Large")), ("PouchSpecialDeal", s("None")), ("PouchTabCategory", s("KeyItem"))],
            ),
            row(
                "Item_Gauntlet_Silver",
                &[("AdultEquipment", s("Work/Actor/Armor_Adult_Gauntlet_Silver.engine__actor__ActorParam.gyml")), ("BundleActorNum", Node::I32(1)), ("ExpansionAvility", s("LiftGradeUP")), ("LiftableGrade", s("Middle")), ("PouchCategory", s("KeyItem")), ("PouchGetType", s("Large")), ("PouchSpecialDeal", s("Gauntlet")), ("PouchTabCategory", s("KeyItem"))],
            ),
            row("Item_Scale_Silver", &[("BundleActorNum", Node::I32(1)), ("DiveDepth", Node::F32(6.0)), ("ExpansionAvility", s("DiveDepthUP")), ("LiftableGrade", s("None")), ("PouchCategory", s("KeyItem")), ("PouchGetType", s("Large")), ("PouchSpecialDeal", s("None")), ("PouchTabCategory", s("KeyItem"))]),
            row("Item_Ocarina_Fairy", &[("BundleActorNum", Node::I32(1)), ("OcarinaTypeValue", Node::I32(1)), ("PouchCategory", s("KeyItem")), ("PouchGetType", s("Small")), ("PouchSpecialDeal", s("Ocarina")), ("PouchTabCategory", s("KeyItem"))]),
        ]
    }

    /// Players got bag and strength upgrades from traps, and a second Slingshot with no seeds.
    #[test]
    fn a_stand_in_row_is_a_plain_key_item_that_never_enters_the_pouch() {
        for look in shipped() {
            let id: String = rows::id(&look).unwrap().to_string();
            let name: String = fake_name(&id);
            let r: Node = inert(&look, &name);
            let Node::Map(m) = &r else { panic!("{id}") };
            let keys: Vec<&str> = m.iter().map(|e| e.0.as_str()).collect();
            assert_eq!(keys, ["BundleActorNum", "PouchCategory", "PouchGetType", "PouchSpecialDeal", "PouchTabCategory", "__RowId"], "{id}");
            assert_eq!(rows::id(&r), Some(name.as_str()));
            assert_eq!(r.get(rows::DEAL).and_then(|v| v.as_str()), Some(rows::QUIET_DEAL), "{id}");
            assert_eq!(r.get(CATEGORY).and_then(|v| v.as_str()), Some(PLAIN), "{id}");
            assert_eq!(r.get(TAB).and_then(|v| v.as_str()), Some(PLAIN), "{id}");
            assert_eq!(r.get("PouchGetType"), look.get("PouchGetType"), "{id}");
            for k in EFFECTS {
                assert!(r.get(k).is_none(), "{id} keeps {k}");
            }
        }
    }

    #[test]
    fn a_disarmed_row_keeps_how_it_is_shown_and_loses_what_it_does() {
        for look in shipped() {
            let id: String = rows::id(&look).unwrap().to_string();
            let mut r: Node = look.clone();
            r.set("BuyingPrice", Node::I32(120));
            disarm(&mut r);
            for k in EFFECTS {
                assert!(r.get(k).is_none(), "{id} keeps {k}");
            }
            for k in [rows::ROW_ID, CATEGORY, TAB, "PouchGetType", "BundleActorNum", "BuyingPrice"] {
                assert!(r.get(k).is_some(), "{id} lost {k}");
            }
            let deal: &str = r.get(rows::DEAL).and_then(|v| v.as_str()).unwrap();
            assert_eq!(deal, NO_DEAL, "{id}");
        }
        let mut quiet: Node = row("Obj_X", &[(rows::DEAL, s(rows::QUIET_DEAL))]);
        disarm(&mut quiet);
        assert_eq!(quiet.get(rows::DEAL).and_then(|v| v.as_str()), Some(rows::QUIET_DEAL));
    }

    /// A chest that holds an upgrade or ammo hands that out whatever is put inside it afterwards.
    #[test]
    fn a_trap_spot_only_ever_holds_a_rupee_in_place_of_the_trap() {
        let d: Data = Data::load().unwrap();
        for junk in ["Rupee (1)", "Rupees (5)", "Rupees (20)"] {
            assert_eq!(stand_in(Some(&junk.to_string())), junk);
        }
        for real in ["Bomb Bag", "Slingshot", "Progressive Hookshot", "Arrows (10)", "Bombs (5)", "Deku Seeds (30)", "Deku Shield", "Piece of Heart"] {
            assert!(d.item(real).is_some(), "{real}");
            assert_eq!(stand_in(Some(&real.to_string())), fake::RUPEE_ITEM, "{real}");
        }
        assert_eq!(stand_in(None), fake::RUPEE_ITEM);
        let s: Settings = wide();
        for n in 0..3 {
            let g: Generated = generate_data(&d, &s, &format!("cold stand {n}"), &on()).unwrap();
            let (out, ts): (SeedFile, Vec<Trap>) = prepare(&g.seed_file).unwrap();
            for t in &ts {
                let held: &str = out.placements.get(&t.loc).map(|x| x.as_str()).unwrap();
                assert!(held == t.look || crate::slice::silver::junk(held).is_some(), "{} holds {held} for a trap that shows as {}", t.loc, t.look);
                assert!(held != t.look || d.family(held).is_none(), "{} holds the upgrade {held}", t.loc);
            }
        }
    }

    #[test]
    fn the_spoiler_names_a_trap_and_a_hint_passes_it_off_as_junk() {
        assert_eq!(crate::spoiler::shown(TRAP), TRAP);
        assert_eq!(crate::spoiler::hinted(TRAP), "Rupee (1)");
    }

    #[test]
    fn a_seed_with_them_builds() {
        let Some(root) = real() else {
            return;
        };
        let d: Data = Data::load().unwrap();
        let g: Generated = generate_data(&d, &wide(), "cold build", &on()).unwrap();
        let load: PB = std::env::temp_dir().join(format!("ootrr_cold_build_{}", std::process::id()));
        let _ = fs::remove_dir_all(&load);
        let b: Built = crate::slice::edit::build_for(&g.seed_file, &root, &load, "cold", Host::Hookshot).unwrap();
        let actors: PB = b.root.join("romfs").join("Pack").join("Actor");
        assert!(actors.join(format!("{}.pack.zs", sensor::NAME)).is_file());
        let mut rf: Romfs = Romfs::open(&root).unwrap();
        let table: PB = POUCH_INFO.split('/').fold(b.root.join("romfs"), |a, s| a.join(s));
        let y: Byml = Byml::parse(&rf.zs.decompress(&fs::read(table).unwrap()).unwrap()).unwrap();
        let (_, ts): (SeedFile, Vec<Trap>) = prepare(&g.seed_file).unwrap();
        assert_eq!(ts.len(), DEFAULT as usize);
        for t in &ts {
            let r: &Node = rows::find(&y, &t.fake).unwrap_or_else(|| panic!("{} has no pouch row", t.fake));
            assert_eq!(r.get(rows::DEAL).and_then(|v| v.as_str()), Some(rows::QUIET_DEAL), "{}", t.fake);
            assert_eq!(r.get(CATEGORY).and_then(|v| v.as_str()), Some(PLAIN), "{}", t.fake);
            for k in EFFECTS {
                assert!(r.get(k).is_none(), "{} keeps {k}", t.fake);
            }
            for hand in t.trigger_names().iter().filter(|n| **n != t.fake) {
                if let Some(q) = rows::find(&y, hand) {
                    for k in EFFECTS {
                        assert!(q.get(k).is_none(), "{hand} keeps {k}");
                    }
                }
            }
            let pack: Vec<u8> = rf.zs.decompress(&fs::read(actors.join(format!("{}.pack.zs", t.fake))).unwrap()).unwrap();
            let es: Vec<Entry> = sarc::parse(&pack).unwrap();
            let own: Byml = Byml::parse(&sarc::get(&es, &pouch_file(&t.fake)).unwrap().data).unwrap();
            for (k, v) in fake::inert_params() {
                assert_eq!(own.root.get(k), Some(&v), "{} {k}", t.fake);
            }
        }
        for flow in trigger::PICKUPS {
            let path: PB = b.root.join("romfs").join("Event").join("EventFlow").join(format!("{flow}.bfevfl.zs"));
            let bf: Bf = Bf::parse(&rf.zs.decompress(&fs::read(path).unwrap()).unwrap()).unwrap();
            let f: &crate::bfevfl::Flow = bf.flows.iter().find(|f| f.name == flow).unwrap();
            assert!(trigger::asks_wait(f), "{flow} asks for the picked name in the frame it is set");
        }
        let _ = fs::remove_dir_all(&load);
    }
}
