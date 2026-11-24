use std::collections::BTreeMap as BM;

use crate::byml::Byml;
use crate::dungeon_credit::flag;
use crate::dungeon_grant::{mark, struct_of, Grants, Kind, Marks, KINDS};
use crate::error::{Error as E, Res};
use crate::settings::seed::SeedFile;
use crate::settings::Settings;
use crate::slice::data::{Data, Item, State};
use crate::slice::edit::Edited;
use crate::slice::support::{Keep, Mode, Plan};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Keys {
    pub small: Keep,
    pub boss: Keep,
    pub map: Keep,
    pub ganon: Keep,
}

const SMALL_OK: [&str; 4] = ["vanilla", "dungeon", "any_dungeon", "keysanity"];

impl Keys {
    pub fn read(s: &Settings, bad: &mut Vec<String>) -> Keys {
        Keys {
            small: keep(s, "shuffle_smallkeys", &SMALL_OK, bad),
            boss: keep(s, "shuffle_bosskeys", &BOSS_OK, bad),
            map: keep(s, "shuffle_mapcompass", &MAP_OK, bad),
            ganon: keep(s, "shuffle_ganon_bosskey", &GANON_OK, bad),
        }
    }
}

fn keep(s: &Settings, id: &str, ok: &[&str], bad: &mut Vec<String>) -> Keep {
    match s.text(id) {
        Ok(v) if ok.contains(&v) => keep_value(v),
        Ok(v) => {
            bad.push(format!("{id} is {v}, which this build cannot place yet (only {} work)", listed(ok)));
            Keep::Vanilla
        }
        Err(e) => {
            bad.push(e.to_string());
            Keep::Vanilla
        }
    }
}
use crate::world::{LocId, World};

pub const BOSS_OK: [&str; 7] = ["remove", "vanilla", "dungeon", "regional", "overworld", "any_dungeon", "keysanity"];
pub const MAP_OK: [&str; 7] = ["remove", "startwith", "vanilla", "dungeon", "regional", "any_dungeon", "keysanity"];
pub const GANON_OK: [&str; 9] = ["remove", "vanilla", "dungeon", "regional", "overworld", "any_dungeon", "keysanity", crate::lacs_key::VALUE, crate::medal_gate::LACS_MEDALLIONS_VALUE];

pub const BOSS_DUNGEONS: [&str; 5] = ["ForestTemple", "FireTemple", "WaterTemple", "ShadowTemple", "SpiritTemple"];
pub const GANON: &str = "InsideGanonCastle";
pub const MAP_DUNGEONS: [&str; 10] = [
    "InsideTheDekuTree",
    "DodongoCavern",
    "InsideJabuJabuBelly",
    "ForestTemple",
    "FireTemple",
    "WaterTemple",
    "ShadowTemple",
    "SpiritTemple",
    "BottomOfTheWell",
    "IceCavern",
];

pub const GROUPS: [(&str, &[&str]); 6] = [
    ("forest", &["KokiriForest", "LostWood", "SacredForestMeadow", "InsideTheDekuTree", "ForestTemple"]),
    ("mountain", &["DeathMountainTrail", "DeathMountainCrater", "GoronCity", "DodongoCavern", "FireTemple"]),
    ("water", &["ZoraRiver", "ZoraDomain", "ZoraFountain", "InsideJabuJabuBelly", "IceCavern", "WaterTemple"]),
    ("kakariko", &["KakarikoVillage", "Graveyard", "BottomOfTheWell", "ShadowTemple"]),
    ("gerudo", &["GerudoValley", "GerudoFortress", "HountedWasteland", "DesertColossus", "SpiritTemple", "GerudoTrainingGround"]),
    ("castle", &["HyruleField", "Market", "HyruleCastle", "GanonCastle", "InsideGanonCastle"]),
];

pub const ROAM_BOSS: &str = "Boss Keys can be found outside their own dungeon. The chest sets the boss key of the dungeon it belongs to and says which.";
pub const START_BOSS: &str = "Boss Keys are removed and you start with every boss door key. The Boss Keys in shuffled locations are replaced by Rupee (1).";
pub const ROAM_MAP: &str = "Maps and Compasses can be found outside their own dungeon. The chest sets the map or compass of the dungeon it belongs to and says which.";
pub const START_MAP: &str = "You start with every Map and Compass. The Maps and Compasses in shuffled locations are replaced by Rupee (1).";

pub const CHEST_ROWS: [(&str, &str); 4] = [
    ("shuffle_smallkeys", "Small Keys"),
    ("shuffle_bosskeys", "Boss Keys"),
    ("shuffle_ganon_bosskey", "Ganon's Boss Key"),
    ("shuffle_mapcompass", "Maps and Compasses"),
];

pub fn chest_conflict(chests: Mode, keep: Keep, id: &str, label: &str, value: &str) -> Option<String> {
    let small: bool = id == CHEST_ROWS[0].0;
    let tail: &str = if small {
        "the small key chests stay as they are while the logic counts every key as roaming, which no seed can satisfy"
    } else {
        "nothing of this kind can roam"
    };
    match (chests, keep) {
        (Mode::Off, k) if k.roams() => Some(format!(
            "{id} is {value} while shuffle_chests is off, so no chest is shuffled and {tail}. Set Chests to Dungeons only or Everywhere, or set {label} to Own dungeon"
        )),
        (Mode::Overworld, k) if k.roams() => Some(format!(
            "{id} is {value} while shuffle_chests is overworld, so the dungeon chests are not shuffled and {tail}. Set Chests to Dungeons only or Everywhere, or set {label} to Own dungeon"
        )),
        (Mode::Dungeons, Keep::Overworld) => Some(format!(
            "{id} is {value} while shuffle_chests is dungeons, so no overworld chest is shuffled to hold it. Set Chests to Everywhere, or set {label} to Any dungeon"
        )),
        _ => None,
    }
}

pub fn chest_guard(s: &Settings, chests: Mode, keeps: [Keep; 4], bad: &mut Vec<String>) {
    for ((id, label), keep) in CHEST_ROWS.iter().zip(keeps) {
        if let Ok(v) = s.text(id)
            && let Some(m) = chest_conflict(chests, keep, id, label, v)
        {
            bad.push(m);
        }
    }
}

pub fn keep_value(v: &str) -> Keep {
    match v {
        "vanilla" => Keep::Vanilla,
        "dungeon" => Keep::Dungeon,
        "any_dungeon" => Keep::AnyDungeon,
        "keysanity" => Keep::Anywhere,
        "overworld" => Keep::Overworld,
        "regional" => Keep::Regional,
        crate::lacs_key::VALUE | crate::medal_gate::LACS_MEDALLIONS_VALUE => Keep::Lacs,
        _ => Keep::Start,
    }
}

pub fn listed(ok: &[&str]) -> String {
    match ok.split_last() {
        Some((l, rest)) if !rest.is_empty() => format!("{} and {l}", rest.join(", ")),
        Some((l, _)) => l.to_string(),
        None => String::new(),
    }
}

pub fn keep_of(p: &Plan, kind: &str) -> Option<Keep> {
    match kind {
        "BossKey" => Some(p.keys.boss),
        "GanonBossKey" => Some(p.keys.ganon),
        "Map" | "Compass" => Some(p.keys.map),
        _ => None,
    }
}

pub fn free(k: Keep) -> bool {
    !matches!(k, Keep::Vanilla | Keep::Dungeon)
}

pub fn credited(k: Keep) -> bool {
    !matches!(k, Keep::Vanilla)
}

pub fn handles(p: &Plan, kind: &str) -> bool {
    keep_of(p, kind).is_some_and(free)
}

pub fn exempt(p: &Plan, kind: &str) -> bool {
    (p.keys_roam() && kind == "SmallKey") || handles(p, kind)
}

pub fn group(folder: &str) -> Option<&'static str> {
    GROUPS.iter().find(|(_, f)| f.contains(&folder)).map(|(g, _)| *g)
}

fn near(d: &Data, i: &Item, name: &str) -> bool {
    let home: Option<&str> = i.dungeon.as_deref().and_then(struct_of).and_then(group);
    let at: Option<&str> = d.cat.by_name(name).map(|c| c.folder.as_str()).and_then(group);
    home.is_some() && home == at
}

pub fn allowed(d: &Data, w: &World, p: &Plan, i: &Item, l: LocId) -> bool {
    let Some(k) = keep_of(p, &i.kind) else {
        return false;
    };
    let name: &str = &w.locs[l].name;
    let here: Option<&str> = w.regions[w.locs[l].region].dungeon.as_deref();
    let spot: bool = match k {
        Keep::AnyDungeon => here.is_some(),
        Keep::Overworld => here.is_none(),
        Keep::Regional => here == i.dungeon.as_deref() || near(d, i, name),
        Keep::Anywhere | Keep::Start => true,
        Keep::Vanilla | Keep::Dungeon | Keep::Lacs => false,
    };
    // A gift scene hands a dungeon item out through crate::gift_keys, a chest through its own credit.
    let chest: bool = d.flow.by_name(name).is_none() && !d.special(name) && chest_ok(d, name) && d.cat.by_name(name).is_some_and(|c| c.kind == "chest");
    spot && i.dungeon.is_some() && (chest || crate::gift_keys::spot(d, name))
}

/// Whether the setting of a dungeon item allows the location, wherever it is.
pub fn spot_ok(d: &Data, w: &World, p: &Plan, i: &Item, l: LocId) -> bool {
    let Some(k) = keep_of(p, &i.kind) else {
        return false;
    };
    let here: Option<&str> = w.regions[w.locs[l].region].dungeon.as_deref();
    let spot: bool = match k {
        Keep::AnyDungeon => here.is_some(),
        Keep::Overworld => here.is_none(),
        Keep::Regional => here == i.dungeon.as_deref() || near(d, i, &w.locs[l].name),
        Keep::Anywhere | Keep::Start => true,
        Keep::Vanilla | Keep::Dungeon | Keep::Lacs => false,
    };
    spot && i.dungeon.is_some()
}

pub fn chest_ok(d: &Data, name: &str) -> bool {
    match d.state_of(name) {
        Some(State::Eligible) => !d.keeps_event(name),
        Some(State::Event) => d.event_ok(name) && !d.keeps_event(name),
        _ => false,
    }
}

pub fn blocked(d: &Data, p: &Plan, kind: &str, name: &str) -> bool {
    Kind::of(kind).is_some_and(|k| keeps(p, k).into_iter().any(credited)) && d.keeps_event(name)
}

pub fn start_items(d: &Data, p: &Plan) -> Vec<String> {
    d.items
        .iter()
        .filter(|i| i.dungeon.is_some() && matches!(i.kind.as_str(), "BossKey" | "GanonBossKey") && keep_of(p, &i.kind) == Some(Keep::Start))
        .map(|i| i.name.clone())
        .collect()
}

pub fn start_flags(p: &Plan) -> Vec<(String, bool)> {
    let mut out: Vec<(String, bool)> = Vec::new();
    if p.keys.boss == Keep::Start {
        out.extend(BOSS_DUNGEONS.iter().map(|d| (flag(d, Kind::Boss.member()), true)));
    }
    if p.keys.ganon == Keep::Start {
        out.push((flag(GANON, Kind::Boss.member()), true));
    }
    if p.keys.map == Keep::Start {
        for d in MAP_DUNGEONS {
            out.push((flag(d, Kind::Map.member()), true));
            out.push((flag(d, Kind::Compass.member()), true));
        }
    }
    out
}

pub fn notes(boss: Keep, ganon: Keep, map: Keep) -> Vec<String> {
    let roam = |k: Keep| free(k) && k != Keep::Start && k != Keep::Lacs;
    let mut out: Vec<String> = Vec::new();
    for (on, t) in [
        (roam(boss) || roam(ganon), ROAM_BOSS),
        (boss == Keep::Start || ganon == Keep::Start, START_BOSS),
        (roam(map), ROAM_MAP),
        (map == Keep::Start, START_MAP),
    ] {
        if on {
            out.push(t.to_string());
        }
    }
    out
}

fn keeps(p: &Plan, k: Kind) -> Vec<Keep> {
    match k {
        Kind::Boss => vec![p.keys.boss, p.keys.ganon],
        Kind::Map | Kind::Compass => vec![p.keys.map],
    }
}

pub fn grants(d: &Data, seed: &SeedFile, p: &Plan) -> Res<Grants> {
    let skip: Vec<Kind> = KINDS.iter().copied().filter(|k| keeps(p, *k).into_iter().any(credited)).collect();
    let sweep: Vec<Kind> = KINDS.iter().copied().filter(|k| keeps(p, *k).into_iter().any(|x| credited(x) && x != Keep::Start)).collect();
    let mut credits: BM<u64, (Kind, &'static str)> = BM::new();
    let mut wrapped: std::collections::BTreeSet<u64> = std::collections::BTreeSet::new();
    for (loc, item) in &seed.placements {
        // Gift scenes give dungeon items through their own scene edit (crate::gift_keys).
        if crate::gift_keys::spot(d, loc) || crate::dungeon_packs::handed(d, loc, item) {
            continue;
        }
        let Some(i) = d.item(item) else {
            continue;
        };
        let Some(k) = Kind::of(&i.kind).filter(|k| sweep.contains(k)) else {
            continue;
        };
        let s: &'static str = i
            .dungeon
            .as_deref()
            .and_then(struct_of)
            .ok_or_else(|| E::Msg(format!("{item} at {loc} belongs to no dungeon this build can credit")))?;
        // Ground spots, skulltulas, shops and NPCs credit their dungeon items themselves
        // (crate::debug_names key and location credits, crate::shelf::hook::Credit).
        // A boss heart stands as the dungeon item's own pickup (crate::debug_names::key_credits).
        let Some(l) = d.cat.by_name(loc).filter(|c| c.kind == "chest") else {
            continue;
        };
        // A chest whose get runs a story event, and the Treasure Chest Game prize, keep their
        // event and credit after it (crate::debug_names::stage_credit_chests).
        if loc == crate::slice::treasure::LOC || l.refs.iter().any(|r| d.events.keep.contains(&r.hash)) {
            wrapped.extend(l.refs.iter().map(|r| r.hash));
            continue;
        }
        for r in &l.refs {
            credits.insert(r.hash, (k, s));
        }
    }
    Ok(Grants { skip, sweep, credits, wrapped })
}

pub fn apply(file: &str, raw: &[u8], prev: Option<Edited>, g: &Grants, m: &mut Marks) -> Res<Option<Edited>> {
    if !g.active() {
        return Ok(prev);
    }
    let mut y: Byml = match &prev {
        Some(e) => Byml::parse(&e.bytes)?,
        None => Byml::parse(raw)?,
    };
    if mark(&mut y, file, g, m)? == 0 {
        return Ok(prev);
    }
    let bytes: Vec<u8> = y.write()?;
    Ok(Some(match prev {
        Some(mut e) => {
            e.bytes = bytes;
            e
        }
        None => Edited {
            file: file.to_string(),
            old_len: raw.len(),
            bytes,
            hits: Vec::new(),
            used: Vec::new(),
            hearts: Vec::new(),
            wraps: Vec::new(),
            touched: Vec::new(),
        },
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::explore;
    use crate::settings::{Settings, Value};
    use crate::slice::fill::vanilla_place;
    use crate::slice::generate::{generate_data, GenOpts, Generated};
    use crate::slice::logic::world_with;
    use crate::slice::support::plan;
    use crate::world::Placement;

    fn with(pairs: &[(&str, &str)]) -> Settings {
        let mut s: Settings = Settings::default();
        for (k, v) in pairs {
            s.set(k, Value::s(v)).unwrap();
        }
        s
    }

    fn full(d: &Data, s: &Settings, g: &Generated) -> (World, Placement) {
        let p: Plan = plan(s).unwrap();
        let mut w: World = world_with(d, &p.logic()).unwrap();
        for n in start_items(d, &p) {
            let i: crate::logic_items::ItemId = w.item(&n).unwrap();
            w.start.push((i, 1));
        }
        let mut pl: Placement = vanilla_place(&w, d).unwrap();
        for (loc, item) in &g.seed_file.placements {
            pl.put(w.loc(loc).unwrap(), w.item(item).unwrap());
        }
        (w, pl)
    }

    fn counts(w: &World, pl: &Placement) -> BM<String, usize> {
        let mut out: BM<String, usize> = BM::new();
        for l in &w.real {
            if let Some(i) = pl.get(*l) {
                *out.entry(w.items.names[i].clone()).or_insert(0) += 1;
            }
        }
        out
    }

    fn dungeon_items(c: &BM<String, usize>) -> BM<String, usize> {
        c.iter()
            .filter(|(k, _)| k.starts_with("Boss Key (") || k.starts_with("Map (") || k.starts_with("Compass (") || k.starts_with("Small Key ("))
            .map(|(k, v)| (k.clone(), *v))
            .collect()
    }

    #[test]
    fn the_new_values_are_accepted_and_the_goal_values_are_refused() {
        for v in BOSS_OK {
            assert!(plan(&with(&[("shuffle_bosskeys", v)])).is_ok(), "{v}");
            assert!(plan(&with(&[("shuffle_ganon_bosskey", v)])).is_ok(), "{v}");
        }
        for v in MAP_OK {
            assert!(plan(&with(&[("shuffle_mapcompass", v)])).is_ok(), "{v}");
        }
        for v in ["stones", "medallions", "dungeons", "tokens", "hearts"] {
            let e: String = plan(&with(&[("shuffle_ganon_bosskey", v)])).err().unwrap().to_string();
            assert!(e.contains(&format!("shuffle_ganon_bosskey is {v}")), "{e}");
        }
        assert!(Settings::default().set("shuffle_bosskeys", Value::s("startwith")).is_err());
        assert!(plan(&with(&[("shuffle_mapcompass", "overworld")])).err().unwrap().to_string().contains("shuffle_mapcompass is overworld"));
        assert_eq!(listed(&["a", "b", "c"]), "a, b and c");
        assert_eq!(listed(&["a"]), "a");
    }

    #[test]
    fn defaults_add_no_flags_or_notes_but_credit_every_boss_key_map_and_compass_chest() {
        let p: Plan = plan(&Settings::default()).unwrap();
        assert!(start_flags(&p).is_empty());
        assert!(notes(p.keys.boss, p.keys.ganon, p.keys.map).is_empty());
        assert!(!handles(&p, "BossKey") && !handles(&p, "Map") && !handles(&p, "GanonBossKey"));
        let d: Data = Data::load().unwrap();
        let g: Generated = generate_data(&d, &Settings::default(), "grant default", &GenOpts::default()).unwrap();
        let gr: Grants = grants(&d, &g.seed_file, &p).unwrap();
        assert!(gr.active());
        assert_eq!(gr.skip.len(), KINDS.len());
        assert_eq!(gr.sweep.len(), KINDS.len());
        assert!(!gr.credits.is_empty());
        assert!(start_items(&d, &p).is_empty());
    }

    #[test]
    fn start_and_remove_set_the_flags_of_every_dungeon() {
        let p: Plan = plan(&with(&[("shuffle_mapcompass", "startwith"), ("shuffle_bosskeys", "remove"), ("shuffle_ganon_bosskey", "remove")])).unwrap();
        let f: Vec<(String, bool)> = start_flags(&p);
        assert_eq!(f.len(), 5 + 1 + 20);
        assert!(f.iter().all(|(_, v)| *v));
        assert!(f.iter().any(|(n, _)| n == "DungeonInfo.InsideGanonCastle.IsGetBossKey"));
        assert!(f.iter().any(|(n, _)| n == "DungeonInfo.IceCavern.IsGetCompass"));
        assert!(p.start_flags().len() == 26 + 1);
        let d: Data = Data::load().unwrap();
        assert_eq!(start_items(&d, &p).len(), 6);
        let q: Plan = plan(&with(&[("shuffle_mapcompass", "remove")])).unwrap();
        assert_eq!(start_flags(&q).len(), 20);
    }

    #[test]
    fn every_folder_with_a_chest_is_in_one_region_group() {
        let d: Data = Data::load().unwrap();
        for l in d.cat.locs.iter().filter(|l| l.kind == "chest") {
            assert!(group(&l.folder).is_some(), "{} in {}", l.name, l.folder);
        }
        let n: usize = GROUPS.iter().map(|(_, f)| f.len()).sum();
        let mut all: Vec<&str> = GROUPS.iter().flat_map(|(_, f)| f.iter().copied()).collect();
        all.sort();
        all.dedup();
        assert_eq!(all.len(), n);
        for d in MAP_DUNGEONS.iter().chain(BOSS_DUNGEONS.iter()).chain([GANON].iter()) {
            assert!(group(d).is_some(), "{d}");
        }
    }

    /// A chest whose get runs a story event, or the Treasure Chest Game prize: grants leaves it to
    /// crate::debug_names::stage_credit_chests.
    fn story_chest(d: &Data, loc: &str) -> bool {
        loc == crate::slice::treasure::LOC || d.cat.by_name(loc).is_some_and(|c| c.kind == "chest" && c.refs.iter().any(|r| d.events.keep.contains(&r.hash)))
    }

    fn check(d: &Data, s: &Settings, tag: &str, n: usize) -> usize {
        let p: Plan = plan(s).unwrap();
        let mut vanilla_keys: Settings = Settings::default();
        vanilla_keys.set("shuffle_smallkeys", crate::settings::Value::s("vanilla")).unwrap();
        let base: Generated = generate_data(d, &vanilla_keys, "count base", &GenOpts::default()).unwrap();
        let (bw, bp): (World, Placement) = full(d, &vanilla_keys, &base);
        let want: BM<String, usize> = dungeon_items(&counts(&bw, &bp));
        let mut away: usize = 0;
        for k in 0..n {
            let g: Generated = generate_data(d, s, &format!("{tag} {k}"), &GenOpts::default()).unwrap();
            let (w, pl): (World, Placement) = full(d, s, &g);
            assert!(explore(&w, &pl, &[]).won(), "{tag} {k} is not beatable");
            let gone = |n: &str| d.item(n).is_some_and(|i| keep_of(&p, &i.kind) == Some(Keep::Start));
            let fr: Vec<LocId> = crate::slice::pick::pick(d, &w, &p).free;
            let van: Placement = vanilla_place(&w, d).unwrap();
            let mut kept: BM<String, usize> = want.iter().filter(|(n, _)| !gone(n)).map(|(n, c)| (n.clone(), *c)).collect();
            for l in w.real.iter().filter(|l| !fr.contains(l)) {
                if let Some(i) = van.get(*l).filter(|i| gone(&w.items.names[*i])) {
                    *kept.entry(w.items.names[i].clone()).or_insert(0) += 1;
                }
            }
            if p.fire_loop_open() && w.loc(crate::slice::support::FIRE_SPARE_KEY).is_some_and(|l| fr.contains(&l)) {
                if let Some(c) = kept.get_mut("Small Key (Fire Temple)") {
                    *c -= 1;
                }
            }
            assert_eq!(dungeon_items(&counts(&w, &pl)), kept, "{tag} {k}");
            for (loc, item) in &g.seed_file.placements {
                let Some(i) = d.item(item) else {
                    continue;
                };
                if !handles(&p, &i.kind) {
                    continue;
                }
                let l: LocId = w.loc(loc).unwrap();
                let it: crate::logic_items::ItemId = w.item(item).unwrap();
                assert!(spot_ok(d, &w, &p, i, l) && crate::slice::generate::buildable(d, &w, &p, it, l), "{item} at {loc}");
                let here: Option<&str> = w.regions[w.locs[l].region].dungeon.as_deref();
                match keep_of(&p, &i.kind) {
                    Some(Keep::AnyDungeon) => assert!(here.is_some(), "{item} at {loc}"),
                    Some(Keep::Overworld) => assert!(here.is_none(), "{item} at {loc}"),
                    _ => {}
                }
                if here != i.dungeon.as_deref() {
                    away += 1;
                }
            }
            let gr: Grants = grants(d, &g.seed_file, &p).unwrap();
            let picks: Vec<crate::debug_names::KeyCredit> = crate::debug_names::key_credits(d, &g.seed_file);
            for (loc, item) in &g.seed_file.placements {
                let Some(i) = d.item(item).filter(|i| Kind::of(&i.kind).is_some_and(|k| gr.sweep.contains(&k))) else {
                    continue;
                };
                if d.flow.by_name(loc).is_some() {
                    continue;
                }
                let Some(c) = d.cat.by_name(loc).filter(|_| !crate::dungeon_packs::handed(d, loc, item)) else {
                    continue;
                };
                let want: Option<&str> = i.dungeon.as_deref().and_then(struct_of);
                // A ground, skulltula or boss heart spot stands as the item's own pickup, which
                // credits its dungeon after the get (crate::debug_names::key_credits).
                if c.kind != "chest" {
                    let k: Option<crate::gift_keys::Item> = crate::gift_keys::Item::of(&i.kind);
                    assert!(picks.iter().any(|x| Some(x.item) == k && Some(x.dungeon) == want), "{item} at {loc} has no pickup credit");
                    continue;
                }
                // A chest that keeps its story event, and the Treasure Chest Game prize, credit
                // after that event (crate::debug_names::stage_credit_chests); every other chest
                // through its own get.
                let story: bool = story_chest(d, loc);
                for r in &c.refs {
                    if story {
                        assert!(gr.wrapped.contains(&r.hash) && !gr.credits.contains_key(&r.hash), "{item} at {loc} is not left to its story credit");
                    } else {
                        assert_eq!(gr.credits.get(&r.hash).map(|c| c.1), want, "{item} at {loc}");
                    }
                }
            }
        }
        away
    }

    #[test]
    fn roaming_boss_keys_stay_one_per_dungeon_and_every_seed_is_beatable() {
        let d: Data = Data::load().unwrap();
        let mut away: usize = 0;
        for v in ["any_dungeon", "keysanity", "regional", "overworld"] {
            away += check(&d, &with(&[("shuffle_bosskeys", v)]), &format!("boss {v}"), 3);
        }
        assert!(away > 4, "{away}");
    }

    #[test]
    fn removed_boss_keys_still_generate() {
        let d: Data = Data::load().unwrap();
        check(&d, &with(&[("shuffle_bosskeys", "remove"), ("shuffle_ganon_bosskey", "remove")]), "boss remove", 3);
    }

    #[test]
    fn ganons_boss_key_can_roam() {
        let d: Data = Data::load().unwrap();
        let mut away: usize = 0;
        for v in ["any_dungeon", "keysanity", "regional", "overworld"] {
            away += check(&d, &with(&[("shuffle_ganon_bosskey", v)]), &format!("ganon {v}"), 3);
        }
        assert!(away > 2, "{away}");
    }

    #[test]
    fn maps_and_compasses_keep_their_counts_in_every_mode() {
        let d: Data = Data::load().unwrap();
        let mut away: usize = 0;
        for v in ["startwith", "remove", "any_dungeon", "keysanity", "regional"] {
            away += check(&d, &with(&[("shuffle_mapcompass", v)]), &format!("map {v}"), 2);
        }
        assert!(away > 4, "{away}");
    }

    #[test]
    fn small_key_counts_hold_with_roaming_boss_keys() {
        let d: Data = Data::load().unwrap();
        for v in ["any_dungeon", "keysanity"] {
            let s: Settings = with(&[("shuffle_smallkeys", v), ("shuffle_bosskeys", v), ("shuffle_ganon_bosskey", v), ("shuffle_mapcompass", v)]);
            check(&d, &s, &format!("all {v}"), 3);
        }
    }

    fn dump() -> Option<std::path::PathBuf> {
        let p: std::path::PathBuf = ootrr_game::test_romfs::path();
        if p.is_dir() {
            Some(p)
        } else {
            eprintln!("skipped: no romfs at {}, set OOTRR_ROMFS", p.display());
            None
        }
    }

    fn actor(y: &Byml, hash: u64) -> Option<&crate::byml::Node> {
        y.root.get("Actors")?.as_arr()?.iter().find(|a| crate::placement::hash_of(a) == Some(hash) && a.get("Dynamic").is_some_and(|d| d.get(crate::dungeon_credit::DROP).is_some()))
    }

    fn dy<'a>(a: &'a crate::byml::Node, k: &str) -> &'a str {
        a.get("Dynamic").and_then(|d| d.get(k)).and_then(|v| v.as_str()).unwrap_or("")
    }

    /// A story chest's get event `id` is its OOTRR_KeyCredit_N wrap: a row in the event table whose
    /// entry runs the story event, then sets the dungeon flag of `k` for `want`.
    fn story_credit(rf: &mut crate::romfs::Romfs, out: &std::path::Path, id: &str, k: Kind, want: &str, loc: &str) {
        use crate::bfevfl::{Bf, Kind as Ev, Val};
        let n: &str = id.strip_prefix("OOTRR_KeyCredit_").unwrap_or_else(|| panic!("{loc}: get event {id} is no key credit wrap"));
        let table: Byml = Byml::parse(&rf.zs.decompress(&std::fs::read(out.join(crate::progressive::SETTING_TABLE)).unwrap()).unwrap()).unwrap();
        let row: crate::debug_names::Row = crate::debug_names::row_of(&table, id).unwrap_or_else(|| panic!("{loc}: no event table row {id}"));
        assert_eq!(row.entry, format!("KeyCredit_{n}"), "{loc}");
        let path: std::path::PathBuf = out.join(format!("Event/EventFlow/{}.bfevfl.zs", row.stem));
        let b: Bf = Bf::parse(&rf.zs.decompress(&std::fs::read(&path).unwrap()).unwrap()).unwrap();
        let (f, e): (&crate::bfevfl::Flow, &crate::bfevfl::Entry) =
            b.flows.iter().find_map(|f| f.entries.iter().find(|e| e.name == row.entry).map(|e| (f, e))).unwrap_or_else(|| panic!("{loc}: {} has no entry {}", row.stem, row.entry));
        let first: u16 = match &f.events[usize::from(e.main)].kind {
            Ev::Sub { next, .. } => *next,
            other => panic!("{loc}: {} does not start with the story event, {other:?}", row.entry),
        };
        let flag_name: Option<&Val> = f.events[usize::from(first)].kind.params().and_then(|c| c.get("GameDataName"));
        assert_eq!(flag_name, Some(&Val::Str(format!("DungeonInfo.{want}.{}", k.member()))), "{loc}");
    }

    fn built_and_checked(root: &std::path::Path, s: &Settings, seed: &str, name: &str) -> (usize, usize) {
        use crate::bfevfl::{Bf, Val};
        use crate::progressive::{Size, BIG_FLOW, SETTING_TABLE, SMALL_FLOW};
        let d: Data = Data::load().unwrap();
        let p: Plan = plan(s).unwrap();
        let g: Generated = generate_data(&d, s, seed, &GenOpts::default()).unwrap();
        let gr: Grants = grants(&d, &g.seed_file, &p).unwrap();
        let load: std::path::PathBuf = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/dungeon_items_mods");
        let b: crate::slice::edit::Built = crate::slice::edit::build(&g.seed_file, root, &load, name).unwrap();
        let out: std::path::PathBuf = b.root.join("romfs");
        let mut rf: crate::romfs::Romfs = crate::romfs::Romfs::open(root).unwrap();
        let mut away: usize = 0;
        for (loc, item) in &g.seed_file.placements {
            let Some(i) = d.item(item).filter(|i| Kind::of(&i.kind).is_some_and(|k| gr.sweep.contains(&k))) else {
                continue;
            };
            let k: Kind = Kind::of(&i.kind).unwrap();
            let want: &str = i.dungeon.as_deref().and_then(struct_of).unwrap();
            let Some(c) = d.cat.by_name(loc).filter(|_| !crate::dungeon_packs::handed(&d, loc, item)) else {
                continue;
            };
            // A ground, skulltula or boss heart spot stands as the item's own pickup, credited by
            // crate::debug_names::key_credits (checked in check()).
            if c.kind != "chest" {
                away += c.refs.iter().flat_map(|r| r.files.iter()).filter(|f| crate::dungeon_grant::scene_of(f) != Some(want)).count();
                continue;
            }
            let story: bool = story_chest(&d, loc);
            for r in &c.refs {
                for f in &r.files {
                    let old: Byml = Byml::parse(&rf.read_zs(f).unwrap()).unwrap();
                    let new: Byml = Byml::parse(&rf.zs.decompress(&std::fs::read(out.join(f)).unwrap()).unwrap()).unwrap();
                    let a: &crate::byml::Node = actor(&new, r.hash).unwrap();
                    // The vanilla chest may have no drop field yet (slice::banc::DEFAULT_DROPS).
                    let v: &crate::byml::Node = old.root.get("Actors").and_then(|l| l.as_arr()).and_then(|l| l.iter().find(|a| crate::placement::hash_of(a) == Some(r.hash))).unwrap();
                    let size: Size = Size::of_gyaml(a.get("Gyaml").and_then(|x| x.as_str()).unwrap_or(""));
                    assert_eq!(dy(a, crate::dungeon_credit::DROP), k.actor(), "{loc}");
                    if story {
                        story_credit(&mut rf, &out, dy(a, "GetEventName"), k, want, loc);
                    } else {
                        assert_eq!(dy(a, "GetEventName"), crate::dungeon_grant::event_name(k, want, size), "{loc}");
                    }
                    assert_ne!(dy(v, "GetEventName"), dy(a, "GetEventName"), "{loc}");
                    if crate::dungeon_grant::scene_of(f) != Some(want) {
                        away += 1;
                    }
                }
            }
        }
        let mut entries: usize = 0;
        if gr.active() && !gr.sweep.is_empty() {
            let big: Bf = Bf::parse(&rf.zs.decompress(&std::fs::read(out.join(BIG_FLOW)).unwrap()).unwrap()).unwrap();
            let small: Bf = Bf::parse(&rf.zs.decompress(&std::fs::read(out.join(SMALL_FLOW)).unwrap()).unwrap()).unwrap();
            let table: Byml = Byml::parse(&rf.zs.decompress(&std::fs::read(out.join(SETTING_TABLE)).unwrap()).unwrap()).unwrap();
            for (loc, item) in &g.seed_file.placements {
                let Some(i) = d.item(item).filter(|i| Kind::of(&i.kind).is_some_and(|k| gr.sweep.contains(&k))) else {
                    continue;
                };
                if !d.cat.by_name(loc).is_some_and(|c| c.kind == "chest") || story_chest(&d, loc) || crate::dungeon_packs::handed(&d, loc, item) {
                    continue;
                }
                let k: Kind = Kind::of(&i.kind).unwrap();
                let want: &str = i.dungeon.as_deref().and_then(struct_of).unwrap();
                for bf in [&big, &small] {
                    let f: &crate::bfevfl::Flow = &bf.flows[0];
                    let e: &crate::bfevfl::Entry = f.entries.iter().find(|e| e.name == crate::dungeon_grant::entry_name(k, want)).unwrap_or_else(|| panic!("{loc} {item}"));
                    let flag_name: Option<&Val> = f.events[e.main as usize].kind.params().and_then(|c| c.get("GameDataName"));
                    assert_eq!(flag_name, Some(&Val::Str(format!("DungeonInfo.{want}.{}", k.member()))));
                    entries += 1;
                }
                for size in [Size::Small, Size::Big] {
                    let id: String = crate::dungeon_grant::event_name(k, want, size);
                    let row: &crate::byml::Node = table.root.as_arr().unwrap().iter().find(|r| r.get("__RowId").and_then(|x| x.as_str()) == Some(id.as_str())).unwrap();
                    assert_eq!(row.get("EntryPointName").and_then(|x| x.as_str()), Some(crate::dungeon_grant::entry_name(k, want).as_str()));
                }
            }
        }
        (away, entries)
    }

    #[test]
    fn built_mods_give_each_roaming_chest_the_key_of_its_own_dungeon() {
        let Some(root) = dump() else {
            return;
        };
        let s: Settings = with(&[("shuffle_smallkeys", "keysanity"), ("shuffle_bosskeys", "keysanity"), ("shuffle_ganon_bosskey", "keysanity"), ("shuffle_mapcompass", "any_dungeon")]);
        let (away, entries): (usize, usize) = built_and_checked(&root, &s, "dungeon items build", "OOTRR Dungeon Items Test");
        assert!(away > 4, "{away}");
        assert!(entries > 10, "{entries}");
        let t: Settings = with(&[("shuffle_bosskeys", "remove"), ("shuffle_ganon_bosskey", "regional"), ("shuffle_mapcompass", "startwith")]);
        built_and_checked(&root, &t, "dungeon items start", "OOTRR Dungeon Items Start Test");
        let o: Settings = with(&[("shuffle_bosskeys", "overworld"), ("shuffle_ganon_bosskey", "overworld"), ("shuffle_mapcompass", "keysanity")]);
        let (out, _): (usize, usize) = built_and_checked(&root, &o, "dungeon items overworld", "OOTRR Dungeon Items Overworld Test");
        assert!(out >= 6, "{out}");
    }

    #[test]
    fn a_default_build_credits_every_boss_key_map_and_compass_chest() {
        let Some(root) = dump() else {
            return;
        };
        let (_, entries): (usize, usize) = built_and_checked(&root, &Settings::default(), "default credit", "OOTRR Default Credit Test");
        assert!(entries > 10, "{entries}");
    }

    #[test]
    fn the_keysanity_preset_generates() {
        let d: Data = Data::load().unwrap();
        let pr = crate::settings::presets::builtin().unwrap().into_iter().find(|p| p.id == "keysanity").unwrap();
        let s: Settings = pr.apply(&Settings::default()).unwrap();
        check(&d, &s, "preset keysanity", 3);
    }
}
