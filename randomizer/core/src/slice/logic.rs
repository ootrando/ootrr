use std::collections::HashSet as HS;

use crate::error::{Error as E, Res};
use crate::logic_json::{parse, Json};
use crate::logic_val::{Settings, Val};
use crate::slice::data::{Data, SEP};
use crate::world::{Spec, World};

const SHIMS: &str = include_str!("../../../../data/logic/shims.json");
const SETTINGS: &str = include_str!("../../../../data/logic/settings.json");
const HELPERS: &str = include_str!("../../../../data/logic/LogicHelpers.json");

const TOWER: [&str; 3] = ["Ganons Castle Tower", "Ganons Castle Tower Below Boss", "Ganondorf Boss Room"];

fn unique(files: Vec<Json>) -> Vec<Json> {
    let mut seen: HS<String> = HS::new();
    let mut out: Vec<Json> = Vec::new();
    for f in files {
        let Json::Arr(regions) = f else {
            out.push(f);
            continue;
        };
        let mut rs: Vec<Json> = Vec::with_capacity(regions.len());
        for r in regions {
            let Json::Obj(fields) = r else {
                rs.push(r);
                continue;
            };
            let name: String = fields
                .iter()
                .find(|(k, _)| k == "region_name")
                .and_then(|(_, v)| v.as_str())
                .unwrap_or("")
                .to_string();
            let mut nf: Vec<(String, Json)> = Vec::with_capacity(fields.len());
            for (k, v) in fields {
                if k == "locations"
                    && let Json::Obj(locs) = v
                {
                    let mut nl: Vec<(String, Json)> = Vec::with_capacity(locs.len());
                    for (ln, lr) in locs {
                        let full: String = if seen.contains(&ln) { format!("{ln}{SEP}{name}") } else { ln.clone() };
                        seen.insert(ln);
                        nl.push((full, lr));
                    }
                    nf.push((k, Json::Obj(nl)));
                } else {
                    nf.push((k, v));
                }
            }
            rs.push(Json::Obj(nf));
        }
        out.push(Json::Arr(rs));
    }
    out
}

/// The settings whose "hearts" value makes hearts a condition of the logic.
const HEART_GOALS: [&str; 3] = ["bridge", "lacs_condition", "shuffle_ganon_bosskey"];

pub fn spec(d: &Data, over: &[(&str, &str)]) -> Res<Spec> {
    let mut s: Spec = Spec::from_bundle(SHIMS)?;
    s.settings = Settings::from_json(&parse(SETTINGS)?)?;
    for (k, v) in over {
        match v.parse::<i64>() {
            Ok(n) => s.settings.set_int(k, n),
            Err(_) => s.settings.set_str(k, v),
        }
    }
    // Hearts only unlock something under a "hearts" bridge, Light Arrows or Ganon's Boss Key
    // condition. Otherwise they are not progress, so the fill may put them where the logic never
    // goes (a Piece of Heart behind a bean spot nobody can plant).
    let hearts: bool = HEART_GOALS.iter().any(|k| s.settings.get(k) == Some(&Val::Str("hearts".to_string())));
    let rings: bool = s.settings.flag(crate::key_rings::SETTING);
    let hideout_ring: bool = s.settings.flag(crate::key_rings::HIDEOUT_SETTING);
    let rs: Vec<&'static crate::key_rings::Ring> = if rings {
        let picked: Vec<&'static crate::key_rings::Ring> = crate::key_rings::RINGS
            .iter()
            .filter(|r| s.settings.flag(r.select))
            .collect();

        if picked.is_empty() { crate::key_rings::RINGS.iter().collect() } else { picked }
    } else {
        Vec::new()
    };
    for it in &d.items {
        let heart: bool = crate::slice::generate::HEARTS.iter().any(|h| it.name.starts_with(h));
        if let Some(r) = crate::key_rings::by_item_any(&it.name) {
            let active: bool = rs.iter().any(|x| x.item == r.item)
                || (hideout_ring && r.item == crate::key_rings::HIDEOUT_RING.item);
            if active {
                s.add_ring(r.item, r.key, r.count);
            } else {
                s.add_item(&it.name, it.adv && (hearts || !heart), 0);
            }
            continue;
        }
        s.add_item(&it.name, it.adv && (hearts || !heart), 0);
    }
    s.add_helpers(HELPERS)?;
    let mut files: Vec<Json> = Vec::new();
    for t in crate::slice::world_files::all() {
        files.push(parse(t)?);
    }
    if crate::slice::skull_rules::wanted(over) {
        crate::slice::skull_rules::apply(&mut files)?;
    }
    crate::spots::inject(&mut files, over)?;
    if crate::flow_extra::potion::wanted(over) {
        crate::flow_extra::potion::apply(&mut files)?;
    }
    s.regions.extend(unique(files));
    Ok(s)
}

pub fn world(d: &Data) -> Res<World> {
    world_with(d, &[])
}

pub fn world_with(d: &Data, over: &[(&str, &str)]) -> Res<World> {
    world_from(spec(d, over)?)
}

pub fn world_from(s: Spec) -> Res<World> {
    let mut w: World = World::build(&s).map_err(|e| E::Msg(format!("logic world: {e}")))?;
    for r in w.regions.iter_mut() {
        if r.dungeon.is_none() && TOWER.contains(&r.name.as_str()) {
            r.dungeon = Some("Ganons Castle".to_string());
        }
    }
    Ok(w)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logic_items::ItemId;
    use crate::rules::{eval, Age, Ctx};
    use crate::search::{explore, Search};
    use crate::slice::fill::vanilla_place;
    use crate::world::{Exit, Placement};

    #[test]
    fn the_vanilla_world_builds_strictly() {
        let d: Data = Data::load().unwrap();
        let w: World = world(&d).unwrap();
        assert!(w.real.len() > 1000);
        assert!(w.loc("Deku Tree Map Chest").is_some());
        assert!(w.loc("Ganon").is_some());
    }

    fn vanilla_without(w: &World, d: &Data, gone: &[&str]) -> Placement {
        let mut p: Placement = vanilla_place(w, d).unwrap();
        let g: Vec<ItemId> = gone.iter().map(|n| w.item(n).unwrap()).collect();
        let f: ItemId = w.item("Rupees (5)").unwrap();
        for l in &w.real {
            if p.get(*l).is_some_and(|i| g.contains(&i)) {
                p.put(*l, f);
            }
        }
        p
    }

    fn gauntlets(w: &World) -> Vec<ItemId> {
        vec![w.item("Progressive Strength Upgrade").unwrap(); 3]
    }

    fn seen(s: &Search, w: &World, name: &str) -> bool {
        s.loc_access(w.loc(name).unwrap())
    }

    fn exit_open(s: &Search, w: &World, name: &str, a: Age) -> bool {
        let x: &Exit = w.exits.iter().find(|x| x.name == name).unwrap();
        let c: Ctx = Ctx { cnt: &s.cnt, age: Some(a), tod: 0, region: x.from, q: Some(s) };
        x.rule.always || (!x.rule.never && eval(&x.rule.node, &c))
    }

    #[test]
    fn an_open_door_of_time_reaches_the_master_sword_without_the_song_of_time() {
        let d: Data = Data::load().unwrap();
        let closed: World = world(&d).unwrap();
        let s: Search = explore(&closed, &vanilla_without(&closed, &d, &["Song of Time"]), &gauntlets(&closed));
        assert!(!seen(&s, &closed, "Master Sword Pedestal"));
        let open: World = world_with(&d, &[("open_door_of_time", "open")]).unwrap();
        let s: Search = explore(&open, &vanilla_without(&open, &d, &["Song of Time"]), &gauntlets(&open));
        assert!(seen(&s, &open, "Master Sword Pedestal"));
    }

    #[test]
    fn the_door_of_time_requirement_moves_zeldas_escape() {
        let d: Data = Data::load().unwrap();
        let reach = |pairs: &[(&str, &str)], gone: &[&str]| -> bool {
            let w: World = world_with(&d, pairs).unwrap();
            let s: Search = explore(&w, &vanilla_without(&w, &d, gone), &gauntlets(&w));
            seen(&s, &w, "HF Ocarina of Time Item")
        };
        assert!(reach(&[], &["Zora Sapphire"]));
        assert!(!reach(&[("door_of_time_condition", "stones")], &["Zora Sapphire"]));
        assert!(reach(&[("door_of_time_condition", "stones")], &[]));
        assert!(reach(&[("door_of_time_condition", "stones"), ("door_of_time_escape", "moat")], &["Zora Sapphire"]));
        let songless: World = world_with(&d, &[("open_door_of_time", "req")]).unwrap();
        let s: Search = explore(&songless, &vanilla_without(&songless, &d, &["Song of Time"]), &gauntlets(&songless));
        assert!(seen(&s, &songless, "Master Sword Pedestal"));
        let classic: World = world_with(&d, &[("open_door_of_time", "req"), ("door_of_time_condition", "stones")]).unwrap();
        let sword = |gone: &[&str]| -> bool {
            let s: Search = explore(&classic, &vanilla_without(&classic, &d, gone), &gauntlets(&classic));
            seen(&s, &classic, "Master Sword Pedestal")
        };
        assert!(sword(&["Song of Time"]));
        assert!(!sword(&["Song of Time", "Zora Sapphire"]));
    }

    #[test]
    fn every_audit_gate_can_be_satisfied_in_the_vanilla_world() {
        let d: Data = Data::load().unwrap();
        let w: World = world(&d).unwrap();
        let s: Search = explore(&w, &vanilla_place(&w, &d).unwrap(), &gauntlets(&w));
        for n in [
            "Water Temple Gate Open from Ice Cavern End",
            "Light Arrows Scene Done from Temple of Time",
            "Defeat Bongo Bongo from Bongo Bongo Boss Room",
            "Defeat Twinrova from Twinrova Boss Room",
            "Defeat Phantom Ganon from Phantom Ganon Boss Room",
            "ToT Light Arrows Cutscene",
            "Sheik at Temple",
            "Colossus Great Fairy Reward",
            "OGC Great Fairy Reward",
            "Big Poe Kill",
            "LH Sun",
            "Market Lost Dog",
            "HF Ocarina of Time Item",
            "Song from Ocarina of Time",
            "Water Temple Map Chest",
        ] {
            assert!(seen(&s, &w, n), "{n}");
        }
        for r in ["Water Temple Lobby", "Market Bombchu Shop", "Ganons Castle Grounds"] {
            assert!(s.region_reached(w.region(r).unwrap(), None), "{r}");
        }
        assert!(exit_open(&s, &w, "ZR Front -> Zora River", Age::Adult));
        assert!(exit_open(&s, &w, "ZR Front -> Zora River", Age::Child));
        assert!(s.won());
    }

    #[test]
    fn open_kakariko_opens_the_death_mountain_gate_for_a_child_without_the_letter() {
        let d: Data = Data::load().unwrap();
        let mut s: crate::settings::Settings = crate::settings::Settings::default();
        let shut: World = world_with(&d, &crate::slice::support::plan(&s).unwrap().logic()).unwrap();
        s.set("open_forest", crate::settings::Value::s("open")).unwrap();
        s.set("open_kakariko", crate::settings::Value::s("open")).unwrap();
        let open: World = world_with(&d, &crate::slice::support::plan(&s).unwrap().logic()).unwrap();
        let gate: &str = "Kakariko Village -> Kak Behind Gate";
        let a: Search = explore(&shut, &vanilla_without(&shut, &d, &["Zeldas Letter"]), &[]);
        let b: Search = explore(&open, &vanilla_without(&open, &d, &["Zeldas Letter"]), &[]);
        assert!(!exit_open(&a, &shut, gate, Age::Child));
        assert!(exit_open(&b, &open, gate, Age::Child));
        assert!(exit_open(&a, &shut, gate, Age::Adult));
        let c: Search = explore(&shut, &vanilla_place(&shut, &d).unwrap(), &[]);
        assert!(exit_open(&c, &shut, gate, Age::Child));
    }

    #[test]
    fn the_mask_shop_needs_no_letter_once_the_gate_starts_open() {
        let d: Data = Data::load().unwrap();
        let mut s: crate::settings::Settings = crate::settings::Settings::default();
        let shut: World = world_with(&d, &crate::slice::support::plan(&s).unwrap().logic()).unwrap();
        s.set("open_forest", crate::settings::Value::s("open")).unwrap();
        s.set("open_kakariko", crate::settings::Value::s("open")).unwrap();
        let open: World = world_with(&d, &crate::slice::support::plan(&s).unwrap().logic()).unwrap();
        let door: &str = "Market Mask Shop -> Market Mask Shop Storefront";
        let a: Search = explore(&shut, &vanilla_without(&shut, &d, &["Zeldas Letter"]), &[]);
        let b: Search = explore(&open, &vanilla_without(&open, &d, &["Zeldas Letter"]), &[]);
        assert!(!exit_open(&a, &shut, door, Age::Child));
        assert!(exit_open(&b, &open, door, Age::Child));
    }

    #[test]
    fn every_fountain_and_the_summit_owl_open_without_dins_fire() {
        let d: Data = Data::load().unwrap();
        let w: World = world(&d).unwrap();
        let all: Search = explore(&w, &vanilla_place(&w, &d).unwrap(), &gauntlets(&w));
        let cut: Search = explore(&w, &vanilla_without(&w, &d, &["Dins Fire"]), &gauntlets(&w));
        let mute: Search = explore(&w, &vanilla_without(&w, &d, &["Zeldas Lullaby"]), &gauntlets(&w));
        let poor: Search = explore(&w, &vanilla_without(&w, &d, &["Progressive Wallet"]), &gauntlets(&w));
        for n in ["HC Great Fairy Reward", "ZF Great Fairy Reward", "DMC Great Fairy Reward", "Colossus Great Fairy Reward", "OGC Great Fairy Reward", "DMT Great Fairy Reward"] {
            assert!(seen(&all, &w, n), "{n}");
            assert!(seen(&cut, &w, n), "{n}");
            assert!(!seen(&mute, &w, n), "{n}");
            assert_eq!(seen(&poor, &w, n), !n.starts_with("Colossus") && !n.starts_with("OGC"), "{n}");
        }
        let owl: &str = "Death Mountain Summit -> DMT Owl Flight";
        assert!(exit_open(&all, &w, owl, Age::Child));
        assert!(exit_open(&cut, &w, owl, Age::Child));
        assert!(!exit_open(&all, &w, owl, Age::Adult));
    }

    #[test]
    fn the_wallet_and_hookshot_gates_bind() {
        let d: Data = Data::load().unwrap();
        let w: World = world(&d).unwrap();
        let s: Search = explore(&w, &vanilla_without(&w, &d, &["Progressive Wallet"]), &gauntlets(&w));
        assert!(!seen(&s, &w, "Colossus Great Fairy Reward"));
        assert!(!seen(&s, &w, "OGC Great Fairy Reward"));
        let s: Search = explore(&w, &vanilla_without(&w, &d, &["Progressive Hookshot"]), &[]);
        assert!(!seen(&s, &w, "Big Poe Kill"));
        assert!(!s.region_reached(w.region("Water Temple Lobby").unwrap(), None));
    }

    #[test]
    fn the_starting_wallet_pays_medigoron_granny_and_the_carpet_seller() {
        let d: Data = Data::load().unwrap();
        let w: World = world(&d).unwrap();
        let s: Search = explore(&w, &vanilla_without(&w, &d, &["Progressive Wallet"]), &gauntlets(&w));
        for n in ["GC Medigoron", "Kak Granny Buy Blue Potion", "Wasteland Bombchu Salesman"] {
            assert!(seen(&s, &w, n), "{n}");
        }
    }

    #[test]
    fn hidden_grottos_open_without_the_stone_of_agony() {
        let d: Data = Data::load().unwrap();
        let w: World = world(&d).unwrap();
        let mut sp: Spec = spec(&d, &[]).unwrap();
        sp.settings.set_bool("logic_grottos_without_agony", false);
        let old: World = world_from(sp).unwrap();
        let s: Search = explore(&w, &vanilla_without(&w, &d, &["Stone of Agony"]), &gauntlets(&w));
        let o: Search = explore(&old, &vanilla_without(&old, &d, &["Stone of Agony"]), &gauntlets(&old));
        for r in ["HF Inside Fence Grotto", "HF Cow Grotto", "Kak Redead Grotto", "KF Storms Grotto", "ZR Storms Grotto", "DMT Storms Grotto"] {
            assert!(s.region_reached(w.region(r).unwrap(), None), "{r}");
            assert!(!o.region_reached(old.region(r).unwrap(), None), "{r}");
        }
        assert!(s.won() && o.won());
    }

    // The Market Potion Shop shelves want a child with a bottle (the keeper sells after his bottle
    // business); the meeting with Zelda is not asked for.
    #[test]
    fn the_market_potion_shop_shelves_want_a_child_with_a_bottle() {
        let d: Data = Data::load().unwrap();
        let w: World = world(&d).unwrap();
        let all: Search = explore(&w, &vanilla_place(&w, &d).unwrap(), &[]);
        let unmet: Search = explore(&w, &vanilla_without(&w, &d, &["Weird Egg"]), &[]);
        let bottles: Vec<&str> = ["Bottle", "Bottle with Milk", "Rutos Letter", "Bottle with Red Potion", "Bottle with Green Potion", "Bottle with Blue Potion", "Bottle with Fairy", "Bottle with Fish", "Bottle with Blue Fire", "Bottle with Bugs", "Bottle with Poe", "Bottle with Big Poe"]
            .into_iter()
            .filter(|b| w.item(b).is_some())
            .collect();
        let dry: Search = explore(&w, &vanilla_without(&w, &d, &bottles), &[]);
        let shelves: Vec<&str> = w.locs.iter().map(|l| l.name.as_str()).filter(|n| crate::flow_extra::potion::shelf(n)).collect();
        assert_eq!(shelves.len(), 8);
        let shop: crate::world::RegionId = w.region("Market Potion Shop").unwrap();
        for n in shelves {
            assert!(seen(&all, &w, n), "{n}");
            assert!(seen(&unmet, &w, n), "{n}");
            assert!(!seen(&dry, &w, n), "{n}");
        }
        assert!(dry.region_reached(shop, None), "the shop itself is reached without a bottle");
    }

    #[test]
    fn the_water_temple_gate_needs_no_ice_cavern() {
        let d: Data = Data::load().unwrap();
        let w: World = world(&d).unwrap();
        let boots: ItemId = w.item("Iron Boots").unwrap();
        let lobby: usize = w.region("Water Temple Lobby").unwrap();
        let cut: Placement = vanilla_without(&w, &d, &["Silver Rupee (Ice Cavern Push Block)", "Silver Rupee Pouch (Ice Cavern Push Block)"]);
        let s: Search = explore(&w, &cut, &[boots]);
        assert!(!seen(&s, &w, "Water Temple Gate Open from Ice Cavern End"));
        assert!(s.region_reached(lobby, None));
        let s: Search = explore(&w, &vanilla_place(&w, &d).unwrap(), &[boots]);
        assert!(seen(&s, &w, "Water Temple Gate Open from Ice Cavern End"));
        assert!(s.region_reached(lobby, None));
    }

    const REAL_KEYS: [(&str, &str, usize); 3] = [
        ("Small Key (Water Temple)", "Water Temple", 4),
        ("Small Key (Bottom of the Well)", "Bottom of the Well", 2),
        ("Small Key (Ganons Castle)", "Ganons Castle", 1),
    ];

    fn count_in(w: &World, p: &Placement, item: &str) -> usize {
        let i: ItemId = w.item(item).unwrap();
        w.real.iter().filter(|l| p.get(**l) == Some(i)).count()
    }

    fn all_but(w: &World, skip: &str) -> Vec<ItemId> {
        let mut out: Vec<ItemId> = Vec::new();
        for (i, n) in w.items.names.iter().enumerate() {
            if w.items.adv[i] && !n.starts_with(skip) {
                out.extend([i; 3]);
            }
        }
        out
    }

    #[test]
    fn the_remake_key_counts_are_truthful() {
        let d: Data = Data::load().unwrap();
        let w: World = world(&d).unwrap();
        let p: Placement = vanilla_place(&w, &d).unwrap();
        for (k, _, n) in REAL_KEYS {
            assert_eq!(count_in(&w, &p, k), n, "{k}");
        }
        for gone in ["Water Temple Cracked Wall Chest", "Water Temple River Chest"] {
            assert!(w.loc(gone).is_none(), "{gone}");
        }
        for k in ["Water Temple Torches Chest", "Water Temple Compass Room Chest", "Water Temple Dragon Chest", "Water Temple Central Bow Target Chest"] {
            assert_eq!(d.vanilla_of(k), Some("Small Key (Water Temple)"), "{k}");
        }
    }

    #[test]
    fn the_real_keys_open_every_dungeon_location_under_both_layouts() {
        let d: Data = Data::load().unwrap();
        for lay in ["vanilla", "shuffled"] {
            let w: World = world_with(&d, &[("water_key_layout", lay)]).unwrap();
            let p: Placement = vanilla_place(&w, &d).unwrap();
            for (k, dg, _) in REAL_KEYS {
                let s: Search = explore(&w, &p, &all_but(&w, k));
                for l in w.real.iter().filter(|l| w.regions[w.locs[**l].region].dungeon.as_deref() == Some(dg)) {
                    assert!(s.loc_access(*l), "{lay} {}", w.locs[*l].name);
                }
            }
        }
    }

    #[test]
    fn three_water_keys_never_open_the_strict_water_doors() {
        let d: Data = Data::load().unwrap();
        let w: World = world_with(&d, &[("water_key_layout", "shuffled")]).unwrap();
        let k: ItemId = w.item("Small Key (Water Temple)").unwrap();
        let mut p: Placement = vanilla_place(&w, &d).unwrap();
        for l in &w.real {
            if p.get(*l) == Some(k) {
                p.item[*l] = None;
            }
        }
        let mut extra: Vec<ItemId> = all_but(&w, "Small Key (Water Temple)");
        extra.extend([k; 3]);
        let s: Search = explore(&w, &p, &extra);
        assert!(!s.region_reached(w.region("Water Temple Falling Platform Room").unwrap(), None));
        assert!(!s.region_reached(w.region("Water Temple North Basement").unwrap(), None));
        extra.push(k);
        let s: Search = explore(&w, &p, &extra);
        assert!(s.region_reached(w.region("Water Temple Dark Link Region").unwrap(), None));
        assert!(s.region_reached(w.region("Water Temple Boss Key Chest Room").unwrap(), None));
    }

    fn opens_with(d: &Data, lay: &str, key: &str, n: usize, at: &str) -> bool {
        let w: World = world_with(d, &[("fire_key_layout", lay), ("water_key_layout", lay), ("spirit_key_layout", lay)]).unwrap();
        let k: ItemId = w.item(key).unwrap();
        let mut p: Placement = vanilla_place(&w, d).unwrap();
        for l in &w.real {
            if p.get(*l) == Some(k) {
                p.item[*l] = None;
            }
        }
        let mut extra: Vec<ItemId> = all_but(&w, key);
        extra.extend(vec![k; n]);
        let s: Search = explore(&w, &p, &extra);
        match w.loc(at) {
            Some(l) => s.loc_access(l),
            None => s.region_reached(w.region(at).unwrap(), None),
        }
    }

    #[test]
    fn every_remake_small_key_door_costs_its_place_in_the_chain() {
        let d: Data = Data::load().unwrap();
        let locks: [(&str, usize, &str); 11] = [
            ("Small Key (Forest Temple)", 1, "Forest Temple First Stalfos Chest"),
            ("Small Key (Forest Temple)", 1, "Forest Temple Central Area"),
            ("Small Key (Forest Temple)", 2, "Forest Temple Eye Switch Chest"),
            ("Small Key (Forest Temple)", 3, "Forest Temple Boss Key Chest"),
            ("Small Key (Forest Temple)", 3, "Forest Temple Bow Chest"),
            ("Small Key (Forest Temple)", 4, "Forest Temple Blue Poe Chest"),
            ("Small Key (Shadow Temple)", 5, "Shadow Temple Boss Key Chest"),
            ("Small Key (Shadow Temple)", 5, "Shadow Temple Spike Walls Left Chest"),
            ("Small Key (Fire Temple)", 8, "Fire Temple Boss Key Chest"),
            ("Small Key (Fire Temple)", 8, "Fire Temple Flare Dancer Chest"),
            ("Small Key (Water Temple)", 4, "Water Temple North Basement"),
        ];
        for lay in ["vanilla", "shuffled"] {
            for (key, n, at) in locks {
                assert!(!opens_with(&d, lay, key, n - 1, at), "{lay} {at} opens with {}", n - 1);
                assert!(opens_with(&d, lay, key, n, at), "{lay} {at} stays shut with {n}");
            }
            assert!(opens_with(&d, lay, "Small Key (Shadow Temple)", 4, "Bongo Bongo Boss Room"), "{lay}");
        }
    }

    fn final_count(w: &World, d: &Data, placed:&std::collections::BTreeMap<String, String>, item: &str) -> usize {
        let v: Placement = vanilla_place(w, d).unwrap();
        let i: ItemId = w.item(item).unwrap();
        let kept: usize = w.real.iter().filter(|l| v.get(**l) == Some(i) && !placed.contains_key(&w.locs[**l].name)).count();
        kept + placed.values().filter(|x| x.as_str() == item).count()
    }

    #[test]
    fn vanilla_layout_and_roaming_key_seeds_hold_exactly_the_real_keys() {
        let d: Data = Data::load().unwrap();
        for (mode, chests) in [("dungeon", "all"), ("dungeon", "dungeons"), ("any_dungeon", "all"), ("keysanity", "all")] {
            let remake: &str = chests;
            let mut s: crate::settings::Settings = crate::settings::Settings::default();
            s.set("shuffle_smallkeys", crate::settings::Value::s(mode)).unwrap();
            s.set("shuffle_chests", crate::settings::Value::s(chests)).unwrap();
            let g: crate::slice::Generated = crate::slice::generate::generate_data(&d, &s, "keys1", &crate::slice::GenOpts::default()).unwrap();
            let w: World = world_with(&d, &crate::slice::support::plan(&s).unwrap().logic()).unwrap();
            let placed: std::collections::BTreeMap<String, String> = g.seed_file.placements.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
            for (k, _, n) in REAL_KEYS {
                assert_eq!(final_count(&w, &d, &placed, k), n, "{mode} {remake} {k}");
            }
        }
    }

    #[test]
    fn the_zora_river_wall_needs_a_blast_or_a_smash() {
        let d: Data = Data::load().unwrap();
        let w: World = world(&d).unwrap();
        let s: Search = explore(&w, &vanilla_without(&w, &d, &["Bomb Bag"]), &[]);
        let h: ItemId = w.item("Megaton Hammer").unwrap();
        assert_eq!(exit_open(&s, &w, "ZR Front -> Zora River", Age::Adult), s.cnt[h] > 0);
        assert!(!exit_open(&s, &w, "ZR Front -> Zora River", Age::Child));
    }

    #[test]
    fn spells_the_lens_and_the_arrows_need_the_meter_item_and_nothing_else_gives_one() {
        let d: Data = Data::load().unwrap();
        let w: World = world(&d).unwrap();
        let every: Vec<ItemId> = w.items.names.iter().enumerate().filter(|(i, _)| w.items.adv[*i]).flat_map(|(i, _)| [i; 3]).collect();
        let without: Vec<ItemId> = w.items.names.iter().enumerate().filter(|(i, n)| w.items.adv[*i] && !n.starts_with("Magic Meter")).flat_map(|(i, _)| [i; 3]).collect();
        for spell in ["Dins Fire", "Farores Wind", "Nayrus Love", "Lens of Truth", "Fire Arrows", "Light Arrows", "Ice Arrows"] {
            assert!(w.item(spell).is_some_and(|i| without.contains(&i)), "{spell}");
        }
        let reached = |s: &Search| w.real.iter().filter(|l| s.loc_access(**l)).count();
        let rich: Search = explore(&w, &vanilla_place(&w, &d).unwrap(), &every);
        let bare: Search = explore(&w, &vanilla_without(&w, &d, &["Magic Meter"]), &without);
        assert!(reached(&bare) < reached(&rich));
    }

    #[test]
    fn the_logic_files_never_let_a_magic_item_stand_in_for_the_meter() {
        use std::path::{Path, PathBuf};
        let dir: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/logic");
        let squeeze = |s: &str| s.split_whitespace().collect::<Vec<&str>>().join(" ");
        let h: String = squeeze(&std::fs::read_to_string(dir.join("LogicHelpers.json")).unwrap());
        assert!(h.contains("\"can_use(item)\": \"(_is_magic_item(item) and item and Magic_Meter) or (_is_adult_item(item)"));
        assert!(h.contains("or (_is_magic_arrow(item) and is_adult and item and Bow and Magic_Meter)"));
        assert!(h.contains("\"_is_magic_item(item)\": \"item == Dins_Fire or item == Farores_Wind or item == Nayrus_Love or item == Lens_of_Truth\""));
        assert!(h.contains("\"_is_magic_arrow(item)\": \"item == Fire_Arrows or item == Light_Arrows or item == Blue_Fire_Arrows\""));
        assert_eq!(h.matches("magic_spell").count(), 3);
        let mut raw: Vec<String> = Vec::new();
        let mut rest: String = String::new();
        fn walk(d: &Path, out: &mut Vec<PathBuf>) {
            for e in std::fs::read_dir(d).unwrap() {
                let p: PathBuf = e.unwrap().path();
                if p.is_dir() {
                    walk(&p, out);
                } else {
                    out.push(p);
                }
            }
        }
        let mut files: Vec<PathBuf> = Vec::new();
        walk(&dir.join("World"), &mut files);
        files.sort();
        for f in files {
            for line in std::fs::read_to_string(&f).unwrap().lines() {
                let plain: String = line.replace("(Magic_Meter and Fire_Arrows)", "");
                let mut t: String = plain.clone();
                for g in ["Dins_Fire", "Farores_Wind", "Nayrus_Love", "Lens_of_Truth", "Fire_Arrows", "Ice_Arrows", "Light_Arrows", "Blue_Fire_Arrows"] {
                    t = t.replace(&format!("can_use({g})"), "");
                }
                if t.contains("Dins_Fire") {
                    raw.push(t.trim().split('"').nth(1).unwrap_or("").to_string());
                }
                rest.push_str(&t.replace("Dins_Fire", ""));
            }
        }
        assert!(raw.is_empty(), "{raw:?}");
        for g in ["Farores_Wind", "Nayrus_Love", "Lens_of_Truth", "Fire_Arrows", "Ice_Arrows", "Light_Arrows", "Blue_Fire_Arrows"] {
            assert!(!rest.contains(g), "{g}");
        }
    }
}
