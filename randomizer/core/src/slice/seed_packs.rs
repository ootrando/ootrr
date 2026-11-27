use std::collections::BTreeMap as BM;
use std::fmt::Write as W;
use std::fs;
use std::path::{Path as P, PathBuf as PB};

use crate::byml::Byml;
use crate::error::{Error as E, Res};
use crate::modpack::Host;
use crate::pickup_pack::rows::{self, ACTOR_INFO, GAME_ACTOR_INFO, POUCH_INFO};
use crate::pickup_pack::write::{run_with, Done, Opts};
use crate::pickup_pack::{fall, guard, kit, pack_of, sizes, Layers, Spec};
use crate::pickup_persist::{check_swaps_with, Extra};
use crate::romfs::Romfs;
use crate::rsizetable::{Rst, TABLE_PATH};
use crate::settings::seed::SeedFile;
use crate::slice::data::Data;
use crate::slice::edit::Built;
use crate::slice::key_spots::{resolve, shield, Pick};
use crate::slice::tier_order::{fixed_actors, Verdict};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Free,
    Holder,
    Skull,
}

impl Kind {
    pub fn kit(self) -> bool {
        self == Kind::Skull
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Want {
    pub name: String,
    pub pick: Pick,
    pub kind: Kind,
    pub spots: Vec<String>,
    pub named: bool,
}

impl Want {
    pub fn spec(&self) -> Res<(Spec, Layers)> {
        let (s, l): (Spec, Layers) = match self.kind {
            Kind::Skull => self.pick.hang_with()?,
            _ => self.pick.spec()?,
        };
        Ok(if self.named { (Spec { name: self.name.clone(), ..s }, l) } else { (s, l) })
    }
}

fn kind_of(d: &Data, loc: &str, item: &str) -> Option<Kind> {
    if crate::slice::skulls::is_skull(d, loc) {
        let t: crate::slice::skulls::Table = crate::slice::skulls::table().ok()?;
        let shipped: bool = item == crate::slice::skulls::TOKEN || crate::slice::skulls::pickup(&t, d, item).is_some();
        return (!shipped).then_some(Kind::Skull);
    }
    if d.pots.has(loc) {
        return crate::pot_slice::pickup::of(item).is_none().then_some(Kind::Holder);
    }
    if d.cat.by_name(loc).is_some_and(crate::slice::pickups::on_ground) {
        let shipped: bool = crate::slice::pickups::ground_actor(item).is_some() || shield(item).is_some();
        return (!shipped).then_some(Kind::Free);
    }
    None
}

pub fn need(d: &Data, seed: &SeedFile, fixed: &BM<String, String>) -> Res<BM<String, Want>> {
    let mut out: BM<String, Want> = BM::new();
    for (loc, item) in &seed.placements {
        if d.vanilla_of(loc) == Some(item.as_str()) {
            continue;
        }
        let shelf: bool = crate::shops::data::is_loc(loc) && crate::dungeon_packs::of(d, item).is_some();
        let Some(k) = kind_of(d, loc, item).or(shelf.then_some(Kind::Free)) else {
            continue;
        };
        let found: Res<Pick> = if k.kit() { crate::slice::key_spots::resolve_kit(d, item, fixed.get(loc).map(|s| s.as_str())) } else { resolve(d, item, fixed.get(loc).map(|s| s.as_str())) };
        let p: Pick = match found {
            Ok(p) => p,
            // A boss heart whose item has no pack keeps the chest it always had (crate::boss_heart).
            Err(_) if d.cat.by_name(loc).is_some_and(|l| l.kind == "bossheart") => continue,
            Err(e) => return Err(E::Msg(format!("{loc} holds {item}, which this build cannot place there yet: {e}"))),
        };
        let name: String = if k.kit() { p.hang() } else { p.pack.clone() };
        let w: &mut Want = out.entry(name.clone()).or_insert_with(|| Want { name, pick: p, kind: k, spots: Vec::new(), named: false });
        w.spots.push(loc.clone());
    }
    // An Ocarina at an NPC gift that holds the item up is handed out as its pickup actor, which
    // either tier's branch can show, so both packs are built (crate::ocarina_hold).
    for (loc, p) in crate::ocarina_hold::wanted(d, seed) {
        let w: &mut Want = out.entry(p.pack.clone()).or_insert_with(|| Want { name: p.pack.clone(), pick: p, kind: Kind::Free, spots: Vec::new(), named: false });
        w.spots.push(loc);
    }
    for (name, item) in &seed.kits {
        let p: Pick = crate::slice::key_spots::resolve_kit(d, item, seed.tiers.get(name).map(|t| t.as_str())).map_err(|e| E::Msg(format!("the skulltula drop {name} for {item} cannot be built: {e}")))?;
        out.entry(name.clone()).or_insert_with(|| Want { name: name.clone(), pick: p, kind: Kind::Skull, spots: Vec::new(), named: false });
    }
    for (name, item) in &seed.named {
        let p: Pick = resolve(d, item, seed.tiers.get(name).map(|t| t.as_str())).map_err(|e| E::Msg(format!("the pickup {name} for {item} cannot be built: {e}")))?;
        out.insert(name.clone(), Want { name: name.clone(), pick: p, kind: Kind::Free, spots: Vec::new(), named: true });
    }
    for w in out.values() {
        w.spec()?;
    }
    Ok(out)
}

fn opts(host: Host, base: Option<PB>, dry: bool) -> Opts {
    Opts { host, base, rst_from: None, places: Vec::new(), dry }
}

fn run_one(romfs: &P, load: &P, mod_name: &str, w: &Want, o: &Opts) -> Res<Done> {
    let (s, mut l): (Spec, Layers) = w.spec()?;
    // Every pickup the seed places turns in place, off the floor.
    l.spin = true;
    if w.kind.kit() {
        return kit::run_with(romfs, load, mod_name, &s, o, &[], &l);
    }
    run_with(romfs, load, mod_name, &s, o, &l)
}

const POOL_THREADS: usize = 8;

fn dry(romfs: &P, host: Host, w: &Want) -> Res<Extra> {
    let d: Done = run_one(romfs, P::new("unused"), "unused", w, &opts(host, None, true)).map_err(|e| E::Msg(format!("the pickup pack {} cannot be built: {e}", w.name)))?;
    guard::extra(&d.files, &d.carried)
}

pub fn pool(romfs: &P, host: Host, wants: &BM<String, Want>) -> Res<Extra> {
    let ws: Vec<&Want> = wants.values().collect();
    let n: usize = std::thread::available_parallelism().map_or(1, |x| x.get()).clamp(1, POOL_THREADS).min(ws.len().max(1));
    let parts: Vec<Vec<(usize, Res<Extra>)>> = if n == 1 {
        vec![ws.iter().enumerate().map(|(i, w)| (i, dry(romfs, host, w))).collect()]
    } else {
        std::thread::scope(|sc| {
        let hs: Vec<std::thread::ScopedJoinHandle<Vec<(usize, Res<Extra>)>>> = (0..n)
            .map(|k| {
                let ws: &[&Want] = &ws;
                sc.spawn(move || (k..ws.len()).step_by(n).map(|i| (i, dry(romfs, host, ws[i]))).collect())
            })
            .collect();
        hs.into_iter().map(|h| h.join().unwrap_or_default()).collect()
        })
    };
    let mut done: Vec<Option<Res<Extra>>> = (0..ws.len()).map(|_| None).collect();
    for (i, r) in parts.into_iter().flatten() {
        done[i] = Some(r);
    }
    let mut out: Extra = Extra::default();
    for (i, r) in done.into_iter().enumerate() {
        let x: Extra = r.ok_or_else(|| E::Msg(format!("the pickup pack {} was not built", ws[i].name)))??;
        out.packs.extend(x.packs);
        out.flags.extend(x.flags);
    }
    Ok(out)
}

fn table(rf: &mut Romfs, base: &P, path: &str) -> Res<Byml> {
    Byml::parse(&rf.zs.decompress(&fs::read(base.join(path))?)?)
}

pub fn audit(rf: &mut Romfs, base: &P, wants: &BM<String, Want>) -> Res<Vec<String>> {
    let x: Extra = Extra::load(rf, base)?;
    let ys: Vec<Byml> = [ACTOR_INFO, GAME_ACTOR_INFO, POUCH_INFO].iter().map(|t| table(rf, base, t)).collect::<Res<Vec<Byml>>>()?;
    let rst: Rst = Rst::parse(&rf.zs.decompress(&fs::read(base.join(TABLE_PATH))?)?)?;
    let mut out: Vec<String> = Vec::new();
    for w in wants.values() {
        let raw: Vec<u8> = rf.zs.decompress(&fs::read(base.join(pack_of(&w.name)))?)?;
        if !x.has(&w.name) || x.flags.get(&w.name) != Some(&true) {
            return Err(E::Msg(format!("audit: {} is not in the mod or its game actor row does not keep it collected", w.name)));
        }
        if ys.iter().any(|y| rows::find(y, &w.name).is_none()) {
            return Err(E::Msg(format!("audit: {} lacks a row in one of the three actor tables", w.name)));
        }
        if rst.get(&pack_of(&w.name)) != Some(sizes::pack_size(raw.len())?) {
            return Err(E::Msg(format!("audit: {} has the wrong size table entry", w.name)));
        }
        out.push(format!("  {} | {} | {}", w.name, w.spots.join(", "), match w.kind {
            Kind::Free => "on the ground",
            Kind::Holder => "dropped by a pot, crate, flying pot, beehive, rock or grass tuft",
            Kind::Skull => "skulltula kit drop",
        }));
    }
    for y in &ys {
        if !rows::sorted(y)? {
            return Err(E::Format("audit: an actor table is no longer sorted"));
        }
    }
    Ok(out)
}

pub fn build(seed: &SeedFile, romfs: &P, load: &P, mod_name: &str, host: Host) -> Res<Built> {
    build_hooked(seed, romfs, load, mod_name, host, None)
}

pub fn build_hooked(seed: &SeedFile, romfs: &P, load: &P, mod_name: &str, host: Host, hook: Option<&mut crate::slice::edit::Hook>) -> Res<Built> {
    let d: Data = Data::load()?;
    let order: Verdict = fixed_actors(&d, seed)?;
    let wants: BM<String, Want> = need(&d, seed, &order.assign)?;
    let gives: Vec<crate::pot_slice::targets::Give> = crate::pot_slice::targets::targets_with(&d, seed, &order.assign)?;
    let twins: Vec<&str> = fall::wanted(gives.iter().map(|g| g.actor.as_str()));
    if wants.is_empty() && twins.is_empty() {
        return crate::slice::edit::build_data(Data::load()?, seed, romfs, load, mod_name, host, &Extra::default(), hook);
    }
    if let Some(b) = order.bad.first() {
        return Err(E::Msg(format!("this seed places key items at pickup spots out of upgrade tier order, generate it again with this build: {b}")));
    }
    let x: Extra = if wants.is_empty() { Extra::default() } else { pool(romfs, host, &wants)? };
    let mut rf: Romfs = Romfs::open(romfs)?;
    check_swaps_with(&mut rf, &[], &x)?;
    let mut b: Built = crate::slice::edit::build_data(Data::load()?, seed, romfs, load, mod_name, host, &x, hook)?;
    let base: PB = b.root.join("romfs");

    for w in wants.values() {
        let done: Done = run_one(romfs, load, mod_name, w, &opts(host, Some(base.clone()), false))?;
        if done.root.as_deref() != Some(b.root.as_path()) {
            return Err(E::Msg(format!("the pickup pack {} went to another folder than the seed mod", w.name)));
        }
    }
    if !twins.is_empty() {
        let done: Done = fall::run(romfs, load, mod_name, &twins, &opts(host, Some(base.clone()), false))?;
        if done.root.as_deref() != Some(b.root.as_path()) {
            return Err(E::Msg("the falling pickups went to another folder than the seed mod".to_string()));
        }
        b.counts.files += twins.len();
        let names: Vec<String> = twins.iter().map(|t| fall::name_of(t)).collect();
        let _ = writeln!(b.report, "Falling pickups were added to this mod for items dropped by pots ({}): {}. They fall to the ground like a rupee instead of hanging where the pot broke. Not tested in the game yet.", names.len(), names.join(", "));
    }
    let songs = crate::slice::support::plan(&seed.settings()?.settings)?;
    if songs.songs == crate::slice::support::Songs::Anywhere && songs.harp_colors {
        let names: Vec<String> = wants.keys().cloned().collect();
        let tinted: Vec<String> = crate::song_models::color_packs(&mut rf, &base, &names)?;
        if !tinted.is_empty() {
            let _ = writeln!(b.report, "Song pickups show the harp in their song colour: {}", tinted.join(", "));
        }
    }
    let lines: Vec<String> = audit(&mut rf, &base, &wants)?;
    let names: Vec<String> = wants.keys().cloned().collect();
    let bags: Vec<String> = crate::bombchu_bag::pickup::repoint(&mut rf, &base, &names)?;
    if !bags.is_empty() {
        let _ = writeln!(b.report, "Bombchu Bag pickups hand out the bag with its own model: {}", bags.join(", "));
    }
    b.counts.files += wants.len();
    let _ = writeln!(b.report, "{} pickup packs for key items at pickup spots were added to this mod with their actor rows and size table entries, all audited. None of them is tested in the game yet:", wants.len());
    for l in lines {
        let _ = writeln!(b.report, "{l}");
    }
    Ok(b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;

    fn seed(pairs: &[(&str, &str)]) -> SeedFile {
        let m: BM<String, String> = pairs.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect();
        SeedFile::new("s", &Settings::default(), "g", m).unwrap()
    }

    #[test]
    fn a_seed_needs_one_pack_per_new_item_and_a_kit_per_skulltula_drop() {
        let d: Data = Data::load().unwrap();
        let pot: String = d.pots.state.iter().filter(|(n, s)| **s == crate::slice::data::State::Eligible && crate::slice::copies::copies(&d, n) < 2).map(|(n, _)| n.clone()).min().unwrap();
        let free: &str = "LLR Freestanding PoH";
        let skull: &str = "KF GS Bean Patch";
        let s: SeedFile = seed(&[(free, "Boomerang"), (pot.as_str(), "Megaton Hammer"), (skull, "Progressive Hookshot"), ("KF Midos Top Left Chest", "Bow")]);
        let fixed: BM<String, String> = [(skull.to_string(), "Item_Hookshot".to_string())].into_iter().collect();
        let w: BM<String, Want> = need(&d, &s, &fixed).unwrap();
        // Every seed also builds the song and Ocarina pickups the story skips hold up (crate::ocarina_hold).
        let scene: Vec<String> = crate::ocarina_hold::wanted(&d, &s).into_iter().map(|(_, p)| p.pack).collect();
        let names: Vec<&str> = w.keys().map(|k| k.as_str()).filter(|k| !scene.iter().any(|x| x == k)).collect();
        assert_eq!(names, vec!["Obj_Boomerang", "Obj_Hammer", "Obj_Hookshot_GH"]);
        assert_eq!(w["Obj_Boomerang"].kind, Kind::Free);
        assert_eq!(w["Obj_Hammer"].kind, Kind::Holder);
        assert_eq!(w["Obj_Hammer"].spec().unwrap().0.name, "Obj_Hammer");
        assert_eq!(w["Obj_Hookshot_GH"].kind, Kind::Skull);
        assert_eq!(w["Obj_Hookshot_GH"].spec().unwrap().0.name, "Obj_Hookshot_GH");
        let both: SeedFile = seed(&[(pot.as_str(), "Megaton Hammer"), (skull, "Megaton Hammer")]);
        let wb: BM<String, Want> = need(&d, &both, &BM::new()).unwrap();
        assert_eq!(wb.keys().map(|k| k.as_str()).filter(|k| !scene.iter().any(|x| x == k)).collect::<Vec<&str>>(), vec!["Obj_Hammer", "Obj_Hammer_GH"]);
        assert_eq!((wb["Obj_Hammer"].kind, wb["Obj_Hammer_GH"].kind), (Kind::Holder, Kind::Skull));
        assert!(need(&d, &s, &BM::new()).is_err());
        let plain: SeedFile = seed(&[(free, "Rupees (5)"), (pot.as_str(), "Rupees (20)"), ("KF Midos Top Left Chest", "Bow")]);
        assert!(need(&d, &plain, &BM::new()).unwrap().keys().all(|k| scene.iter().any(|x| x == k)));
        // A kit cannot carry the Bow's ammo layer, so a skulltula drops the plain Bow kit (the
        // starter bag hook adds the quiver after its get); a Bow on the ground keeps the ammo layer.
        let bow: SeedFile = seed(&[(skull, "Bow"), (free, "Bow")]);
        let wbow: BM<String, Want> = need(&d, &bow, &[(skull.to_string(), "Item_Range_Bow".to_string()), (free.to_string(), "Item_Range_Bow".to_string())].into_iter().collect()).unwrap();
        assert_eq!(wbow["Obj_Range_Bow_GH"].kind, Kind::Skull);
        let (ks, kl): (Spec, Layers) = wbow["Obj_Range_Bow_GH"].spec().unwrap();
        assert_eq!((ks.name.as_str(), ks.item.as_str(), kl.ammo), ("Obj_Range_Bow_GH", "Item_Range_Bow", false));
        assert_eq!(wbow["Obj_Range_Bow"].kind, Kind::Free);
        assert!(wbow["Obj_Range_Bow"].spec().unwrap().1.ammo);
        let two: SeedFile = seed(&[(free, "Megaton Hammer"), (pot.as_str(), "Megaton Hammer")]);
        let both: BM<String, Want> = need(&d, &two, &BM::new()).unwrap();
        assert_eq!(both.keys().filter(|k| !scene.iter().any(|x| x == *k)).count(), 1);
        assert_eq!(both["Obj_Hammer"].spots.len(), 2);
        let shipped: SeedFile = seed(&[(pot.as_str(), "Fire Arrows")]);
        assert!(need(&d, &shipped, &BM::new()).unwrap().keys().all(|k| scene.iter().any(|x| x == k)));
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
        for (k, v) in [("shuffle_rewards", "off"), ("shuffle_chests", "all"), ("shuffle_freestanding_items", "all"), ("shuffle_pots", "all"), ("shuffle_crates", "all"), ("shuffle_skulltulas", "matched")] {
            s.set(k, crate::settings::Value::s(v)).unwrap();
        }
        s
    }

    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    enum At {
        Free,
        Pot,
        Crate,
        Skull,
    }

    fn at_of(d: &Data, name: &str) -> Option<At> {
        if crate::slice::skulls::is_skull(d, name) {
            return Some(At::Skull);
        }
        if let Some(l) = d.pots.get(name) {
            return Some(if l.kind == "crate" { At::Crate } else { At::Pot });
        }
        d.cat.by_name(name).is_some_and(crate::slice::pickups::is_world).then_some(At::Free)
    }

    fn moved(w: &crate::world::World, pl: &crate::world::Placement, it: crate::logic_items::ItemId, l: crate::world::LocId) -> Option<crate::world::Placement> {
        let from: crate::world::LocId = w.real.iter().copied().find(|x| pl.get(*x) == Some(it))?;
        let mut next: crate::world::Placement = pl.clone();
        let was: crate::logic_items::ItemId = next.get(l)?;
        next.put(l, it);
        next.put(from, was);
        crate::search::explore(w, &next, &[]).won().then_some(next)
    }

    fn plando_for(d: &Data, s: &Settings, want: &[(&str, &str)]) -> Vec<String> {
        use crate::slice::pick::{pick, Pick as Chosen};
        use crate::slice::support::{plan, Plan};
        use crate::world::{LocId, World};
        let p: Plan = plan(s).unwrap();
        let counted: Vec<(&str, &str)> = p.logic().into_iter().filter(|(k, _)| !k.ends_with("_key_layout")).collect();
        let w: World = crate::slice::logic::world_with(d, &counted).unwrap();
        let pk: Chosen = pick(d, &w, &p);
        let (_, items): (Vec<String>, Vec<String>) = crate::slice::generate::plando_options(d, s).unwrap();
        let mut locs: Vec<LocId> = pk.free.clone();
        locs.sort_by(|a, b| w.locs[*a].name.cmp(&w.locs[*b].name));
        let mut used: Vec<LocId> = Vec::new();
        let mut out: Vec<String> = Vec::new();
        let mut pl: crate::world::Placement = crate::slice::fill::vanilla_place(&w, d).unwrap();
        for (item, class) in want {
            if !items.iter().any(|i| i == item) {
                continue;
            }
            let it = w.item(item).unwrap();
            let c: At = match *class {
                "free" => At::Free,
                "pot" => At::Pot,
                "crate" => At::Crate,
                _ => At::Skull,
            };
            let hit: Option<LocId> = locs.iter().copied().find(|l| {
                let n: &str = &w.locs[*l].name;
                !used.contains(l) && at_of(d, n) == Some(c) && d.vanilla_of(n) != Some(*item) && crate::slice::generate::allowed(d, &w, &p, it, *l) && moved(&w, &pl, it, *l).is_some()
            });
            if let Some(l) = hit {
                pl = moved(&w, &pl, it, l).unwrap();
                used.push(l);
                out.push(format!("{}={item}", w.locs[l].name));
            }
        }
        out
    }

    #[test]
    fn a_seed_with_the_cow_clones_and_a_key_item_in_a_pot_builds() {
        use crate::slice::generate::{generate_data, GenOpts, Generated};
        let Some(root) = real() else {
            return;
        };
        let d: Data = Data::load().unwrap();
        let mut s: Settings = wide();
        s.set("shuffle_rewards", crate::settings::Value::s("supported")).unwrap();
        s.set(crate::flow_extra::cows::SETTING, crate::settings::Value::Bool(true)).unwrap();
        let list: Vec<String> = plando_for(&d, &s, &[("Hover Boots", "pot")]);
        assert_eq!(list.len(), 1, "{list:?}");
        let o: GenOpts = GenOpts { plando: list.iter().map(|x| crate::slice::plando::parse(x).unwrap()).collect(), ..GenOpts::default() };
        let g: Generated = generate_data(&d, &s, "cow clones and packs", &o).unwrap();
        let load: PB = std::env::temp_dir().join(format!("ootrr_seed_packs_cows_{}", std::process::id()));
        let _ = fs::remove_dir_all(&load);
        let b: Built = crate::slice::edit::build_for(&g.seed_file, &root, &load, "cows packs", Host::Longshot).unwrap();
        assert!(b.report.contains("pickup packs for key items"), "{}", b.report);
        let romfs: PB = b.root.join("romfs");
        for c in ["Animal_Cow_DMT", "Animal_Cow_Kakariko", "Animal_Cow_HFGrotto", "Animal_Cow_LLR_Left", "Animal_Cow_LLR_Right"] {
            assert!(romfs.join(pack_of(c)).is_file(), "{c}");
        }
        let _ = fs::remove_dir_all(&load);
    }

    #[test]
    fn a_key_item_on_a_boss_heart_stands_there_as_its_pickup_instead_of_a_chest() {
        use crate::slice::generate::{generate_data, GenOpts, Generated};
        let Some(root) = real() else {
            return;
        };
        let d: Data = Data::load().unwrap();
        let mut s: Settings = wide();
        s.set("shuffle_boss_hearts", crate::settings::Value::Bool(true)).unwrap();
        let loc: &str = "Deku Tree Queen Gohma Heart";
        let o: GenOpts = GenOpts { plando: vec![crate::slice::plando::parse(&format!("{loc}=Megaton Hammer")).unwrap()], ..GenOpts::default() };
        let g: Generated = generate_data(&d, &s, "boss heart pickup", &o).unwrap();
        assert_eq!(g.seed_file.placements.get(loc).map(|v| v.as_str()), Some("Megaton Hammer"));
        let order: Verdict = fixed_actors(&d, &g.seed_file).unwrap();
        let wants: BM<String, Want> = need(&d, &g.seed_file, &order.assign).unwrap();
        assert!(wants.get("Obj_Hammer").is_some_and(|w| w.spots.iter().any(|x| x == loc)), "{:?}", wants.keys());
        let load: PB = std::env::temp_dir().join(format!("ootrr_boss_heart_{}", std::process::id()));
        let _ = fs::remove_dir_all(&load);
        let b: Built = crate::slice::edit::build_for(&g.seed_file, &root, &load, "boss heart", Host::Hookshot).unwrap();
        let l: &crate::catalog::Loc = d.cat.by_name(loc).unwrap();
        let mut rf: Romfs = Romfs::open(&root).unwrap();
        for r in &l.refs {
            for f in &r.files {
                let raw: Vec<u8> = rf.zs.decompress(&fs::read(b.root.join("romfs").join(f)).unwrap()).unwrap();
                let y: Byml = Byml::parse(&raw).unwrap();
                let a: &crate::byml::Node = y.root.get("Actors").unwrap().as_arr().unwrap().iter().find(|a| crate::placement::hash_of(a) == Some(r.hash)).unwrap();
                assert_eq!(a.get("Gyaml").and_then(|v| v.as_str()), Some("Obj_Hammer"), "{f}");
            }
        }
        assert!(b.root.join("romfs").join(pack_of("Obj_Hammer")).is_file());
        let _ = fs::remove_dir_all(&load);
    }

    #[test]
    fn dungeon_items_on_the_ground_in_pots_and_on_shelves_build_as_their_own_dungeon_packs() {
        use crate::slice::generate::{generate_data, GenOpts, Generated};
        let Some(root) = real() else {
            return;
        };
        let d: Data = Data::load().unwrap();
        let mut s: Settings = wide();
        for (k, v) in [("shuffle_smallkeys", "keysanity"), ("shuffle_bosskeys", "keysanity"), ("shuffle_mapcompass", "keysanity"), (crate::shops::plan::SETTING, "deals_4"), (crate::shops::plan::PRICES, "affordable")] {
            s.set(k, crate::settings::Value::s(v)).unwrap();
        }
        let want: [(&str, crate::dungeon_packs::Spot); 5] = {
            use crate::dungeon_packs::Spot::*;
            [("Small Key (Fire Temple)", Ground), ("Boss Key (Water Temple)", Holder), ("Map (Ice Cavern)", Ground), ("Compass (Deku Tree)", Holder), ("Small Key (Forest Temple)", Shelf)]
        };
        let list: Vec<String> = {
            let p = crate::slice::support::plan(&s).unwrap();
            let w: crate::world::World = crate::slice::logic::world_with(&d, &p.logic()).unwrap();
            let mut locs: Vec<crate::world::LocId> = crate::slice::pick::pick(&d, &w, &p).free;
            locs.extend((0..w.locs.len()).filter(|l| crate::shops::data::is_loc(&w.locs[*l].name) && w.locs[*l].name.ends_with("Right Side Top Left")));
            locs.sort_by(|a, b| w.locs[*a].name.cmp(&w.locs[*b].name));
            let mut used: Vec<crate::world::LocId> = Vec::new();
            let mut out: Vec<String> = Vec::new();
            for (item, class) in want {
                let it = w.item(item).unwrap();
                let hit = locs.iter().copied().find(|l| {
                    let n: &str = &w.locs[*l].name;
                    !used.contains(l) && w.regions[w.locs[*l].region].dungeon.is_none() && crate::dungeon_packs::spot(&d, n) == Some(class) && crate::slice::generate::allowed(&d, &w, &p, it, *l)
                });
                let l = hit.unwrap_or_else(|| panic!("no {class:?} spot takes {item}"));
                used.push(l);
                out.push(format!("{}={item}", w.locs[l].name));
            }
            out
        };
        assert!(list.iter().any(|x| x.ends_with("=Small Key (Forest Temple)")), "{list:?}");
        let o: GenOpts = GenOpts { spoiler: true, plando: list.iter().map(|x| crate::slice::plando::parse(x).unwrap()).collect(), ..GenOpts::default() };
        let g: Generated = generate_data(&d, &s, "dungeon packs", &o).unwrap();
        for x in &list {
            let (l, i) = x.split_once('=').unwrap();
            assert_eq!(g.seed_file.placements.get(l).map(|v| v.as_str()), Some(i));
        }
        let order: Verdict = fixed_actors(&d, &g.seed_file).unwrap();
        let wants: BM<String, Want> = need(&d, &g.seed_file, &order.assign).unwrap();
        for (_, i) in list.iter().filter_map(|x| x.split_once('=')) {
            let name: String = crate::dungeon_packs::of(&d, i).unwrap().name();
            assert!(wants.contains_key(&name), "{name} {:?}", wants.keys());
        }
        let load: PB = std::env::temp_dir().join(format!("ootrr_dungeon_packs_{}", std::process::id()));
        let _ = fs::remove_dir_all(&load);
        let b: Built = crate::slice::edit::build_for(&g.seed_file, &root, &load, "dungeon packs", Host::Hookshot).unwrap();
        let romfs: PB = b.root.join("romfs");
        let mut rf: Romfs = Romfs::open(&root).unwrap();
        let pouch: Byml = table(&mut rf, &romfs, POUCH_INFO).unwrap();
        let actors: Byml = table(&mut rf, &romfs, ACTOR_INFO).unwrap();
        for name in wants.keys().filter(|n| crate::dungeon_packs::by_name(n).is_some()) {
            assert!(romfs.join(pack_of(name)).is_file(), "{name}");
            let r = rows::find(&pouch, name).unwrap();
            assert_eq!(r.get(rows::DEAL).and_then(|v| v.as_str()), Some(rows::QUIET_DEAL), "{name}");
            assert_eq!(rows::find(&actors, name).unwrap().get(rows::ACTOR_NAME).and_then(|v| v.as_str()), Some(name.as_str()), "{name}");
        }
        for flow in ["Dm_SY_Get", "Dm_SY_Get_WithoutPlayer"] {
            let raw: Vec<u8> = rf.zs.decompress(&fs::read(romfs.join(format!("Event/EventFlow/{flow}.bfevfl.zs"))).unwrap()).unwrap();
            assert!(raw.windows(23).any(|w| w == b"Obj_SmallKey_FireTemple"), "{flow} does not credit the Fire Temple key");
        }
        let shop: Vec<u8> = rf.zs.decompress(&fs::read(romfs.join(crate::dungeon_packs::SHOP_FLOW)).unwrap()).unwrap();
        assert!(shop.windows(25).any(|w| w == b"Obj_SmallKey_ForestTemple"), "the shop does not credit the Forest Temple key");
        assert!(b.report.contains("Obj_SmallKey_ForestTemple"), "{}", b.report);
        let _ = fs::remove_dir_all(&load);
    }

    #[test]
    fn key_items_at_every_spot_class_generate_and_build_into_one_mod_with_their_packs() {
        use crate::slice::generate::{generate_data, GenOpts, Generated};
        let Some(root) = real() else {
            return;
        };
        let d: Data = Data::load().unwrap();
        let s: Settings = wide();
        let want: [(&str, &str); 9] = [("Boomerang", "free"), ("Mirror Shield", "free"), ("Megaton Hammer", "skull"), ("Lens of Truth", "skull"), ("Iron Boots", "skull"), ("Hover Boots", "pot"), ("Goron Tunic", "pot"), ("Gold Skulltula Token", "pot"), ("Fire Arrows", "pot")];
        let list: Vec<String> = plando_for(&d, &s, &want);
        assert!(list.len() >= 4, "{list:?}");
        let o: GenOpts = GenOpts { spoiler: true, plando: list.iter().map(|x| crate::slice::plando::parse(x).unwrap()).collect(), ..GenOpts::default() };
        let g: Generated = generate_data(&d, &s, "wp4 key spots", &o).unwrap();
        let again: Generated = generate_data(&d, &s, "wp4 key spots", &o).unwrap();
        assert_eq!(g.seed_file.to_text(), again.seed_file.to_text());
        let sp: String = g.spoiler.clone().unwrap();
        assert!(sp.contains("Key items at pickup spots"), "{sp}");
        for x in &list {
            let (l, i) = x.split_once('=').unwrap();
            assert_eq!(g.seed_file.placements.get(l).map(|v| v.as_str()), Some(i));
            assert!(sp.contains(&format!("  {l}: {i} as ")), "{l}");
        }
        let load: PB = std::env::temp_dir().join(format!("ootrr_seed_packs_{}", std::process::id()));
        let _ = fs::remove_dir_all(&load);
        let b: Built = crate::slice::edit::build_for(&g.seed_file, &root, &load, "wp4 keys", Host::Hookshot).unwrap();
        assert!(b.report.contains("pickup packs for key items"), "{}", b.report);
        assert!(!b.report.contains("variants were added to this mod"), "{}", b.report);
        let romfs: PB = b.root.join("romfs");
        for p in ["Obj_Pot_A_01_GS", "Obj_BrokenBox_A_01_GS", "Obj_Kokiri_Beehive_A_01_GS", "Obj_LiftRock_A_01_GS", "Obj_Plant_Grass_A_01_GS"] {
            assert!(!romfs.join(pack_of(p)).is_file(), "{p}");
        }
        let tokened: bool = list.iter().any(|x| x.split_once('=').is_some_and(|(l, i)| i == "Gold Skulltula Token" && at_of(&d, l) == Some(At::Pot)));
        assert!(tokened, "{list:?}");
        assert!(b.report.contains("Falling pickups were added"), "{}", b.report);
        let twin: String = fall::name_of(kit::TOKEN);
        let mut rt: Romfs = Romfs::open(&root).unwrap();
        let es: Vec<crate::sarc::Entry> = crate::sarc::parse(&rt.zs.decompress(&fs::read(romfs.join(pack_of(&twin))).unwrap()).unwrap()).unwrap();
        assert_eq!(fall::verify(&es, &twin, &kit::token_pack(&mut rt).unwrap()).unwrap().len(), 2);
        let tag: crate::pickup_pack::tag::Table = crate::pickup_pack::tag::Table::parse(&rt.zs.decompress(&fs::read(romfs.join(crate::pickup_pack::tag::TAG)).unwrap()).unwrap()).unwrap();
        assert_eq!(tag.tags_of(&twin), vec!["Token_Gold".to_string()]);
        let held: usize = list.iter().filter(|x| x.split_once('=').is_some_and(|(l, _)| at_of(&d, l) == Some(At::Pot))).count();
        assert!(held >= 2, "{list:?}");
        let mut swapped: usize = 0;
        let mut rf0: Romfs = Romfs::open(&root).unwrap();
        for f in crate::placement::banc_files(&rf0).unwrap() {
            let p: PB = romfs.join(&f);
            if p.is_file() {
                let raw: Vec<u8> = rf0.zs.decompress(&fs::read(p).unwrap()).unwrap();
                swapped += raw.windows(6).filter(|w| w == b"_GS   " || w == b"A_01_G" || w == b"A_02_G" || w == b"A_03_G" || w == b"A_04_G" || w == b"A_05_G" || w == b"A_06_G" || w == b"A_07_G" || w == b"A_08_G").count();
            }
        }
        assert_eq!(swapped, 0, "a holder in the mod's scenes was swapped to a variant");
        let order: Verdict = fixed_actors(&d, &g.seed_file).unwrap();
        let wants: BM<String, Want> = need(&d, &g.seed_file, &order.assign).unwrap();
        assert!(wants.len() >= list.len().min(4), "{:?}", wants.keys());
        let mut rf: Romfs = Romfs::open(&root).unwrap();
        assert_eq!(audit(&mut rf, &romfs, &wants).unwrap().len(), wants.len());
        let token: Vec<crate::sarc::Entry> = kit::token_pack(&mut rf).unwrap();
        for w in wants.values() {
            assert!(romfs.join(pack_of(&w.name)).is_file(), "{}", w.name);
            if w.kind == Kind::Skull {
                let es: Vec<crate::sarc::Entry> = crate::sarc::parse(&rf.zs.decompress(&fs::read(romfs.join(pack_of(&w.name))).unwrap()).unwrap()).unwrap();
                assert_eq!(kit::verify(&es, &w.name, &token).unwrap().len(), 3, "{}", w.name);
            }
        }
        let first: Vec<(String, Vec<u8>)> = files(&romfs);
        let b2: Built = crate::slice::edit::build_for(&g.seed_file, &root, &load, "wp4 keys", Host::Hookshot).unwrap();
        assert_eq!(files(&b2.root.join("romfs")), first);
        let _ = fs::remove_dir_all(&load);
    }

    #[test]
    fn tier_items_at_pickup_spots_get_the_tier_the_order_names_and_their_packs() {
        use crate::slice::generate::{generate_data, GenOpts, Generated};
        let Some(root) = real() else {
            return;
        };
        let d: Data = Data::load().unwrap();
        let s: Settings = wide();
        let want: [(&str, &str); 3] = [("Progressive Hookshot", "free"), ("Bow", "free"), ("Bomb Bag", "skull")];
        let list: Vec<String> = plando_for(&d, &s, &want);
        assert_eq!(list.len(), 3, "{list:?}");
        let o: GenOpts = GenOpts { spoiler: true, plando: list.iter().map(|x| crate::slice::plando::parse(x).unwrap()).collect(), ..GenOpts::default() };
        let g: Generated = generate_data(&d, &s, "wp4 tier spots", &o).unwrap();
        let order: Verdict = fixed_actors(&d, &g.seed_file).unwrap();
        assert!(order.bad.is_empty(), "{:?}", order.bad);
        let wants: BM<String, Want> = need(&d, &g.seed_file, &order.assign).unwrap();
        for x in &list {
            let (l, _) = x.split_once('=').unwrap();
            let tier: &str = order.assign.get(l).map(|t| t.as_str()).unwrap();
            assert!(wants.values().any(|w| w.spots.iter().any(|s| s == l) && w.pick.content == tier), "{l} {tier}");
        }
        assert!(wants.values().any(|w| w.kind == Kind::Skull && w.name.starts_with("Obj_BombBag_")));
        for w in wants.values().filter(|w| w.pick.content == "Item_Range_Bow") {
            assert!(w.spec().unwrap().1.ammo, "{}", w.name);
        }
        let load: PB = std::env::temp_dir().join(format!("ootrr_seed_packs_tier_{}", std::process::id()));
        let _ = fs::remove_dir_all(&load);
        let b: Built = crate::slice::edit::build_for(&g.seed_file, &root, &load, "wp4 tiers", Host::Hookshot).unwrap();
        let mut rf: Romfs = Romfs::open(&root).unwrap();
        assert_eq!(audit(&mut rf, &b.root.join("romfs"), &wants).unwrap().len(), wants.len());
        let _ = fs::remove_dir_all(&load);
    }

    fn files(root: &P) -> Vec<(String, Vec<u8>)> {
        let mut out: Vec<(String, Vec<u8>)> = Vec::new();
        let mut stack: Vec<PB> = vec![root.to_path_buf()];
        while let Some(d) = stack.pop() {
            for e in fs::read_dir(&d).unwrap() {
                let p: PB = e.unwrap().path();
                if p.is_dir() {
                    stack.push(p);
                } else {
                    out.push((p.strip_prefix(root).unwrap().to_string_lossy().to_string(), fs::read(&p).unwrap()));
                }
            }
        }
        out.sort();
        out
    }

    #[test]
    fn a_seed_with_no_new_item_builds_only_the_held_song_and_ocarina_pickups() {
        use crate::slice::generate::{generate_data, GenOpts, Generated};
        let Some(root) = real() else {
            return;
        };
        let d: Data = Data::load().unwrap();
        // The story skips always hand out the lessons' songs and the Ocarinas with the item get, as
        // their pickups (crate::ocarina_hold), so every seed builds those packs; a seed with no
        // other new item at a pickup spot builds only them.
        let scene = |g: &Generated| -> std::collections::BTreeSet<String> { crate::ocarina_hold::wanted(&d, &g.seed_file).into_iter().map(|(_, p)| p.pack).collect() };
        let mut s: Settings = Settings::default();
        s.set("shuffle_rewards", crate::settings::Value::s("off")).unwrap();
        s.set("shuffle_chests", crate::settings::Value::s("off")).unwrap();
        let g: Generated = generate_data(&d, &s, "wp4 plain", &GenOpts::default()).unwrap();
        let w: BM<String, Want> = need(&d, &g.seed_file, &crate::slice::tier_order::fixed_actors(&d, &g.seed_file).unwrap().assign).unwrap();
        assert!(w.keys().all(|k| scene(&g).contains(k) || k == "Obj_Bottle_LetterLand"), "{:?}", w.keys().collect::<Vec<&String>>());
        assert!(!scene(&g).is_empty());
        let load: PB = std::env::temp_dir().join(format!("ootrr_seed_packs_plain_{}", std::process::id()));
        let _ = fs::remove_dir_all(&load);
        let a: Built = crate::slice::edit::build_for(&g.seed_file, &root, &load.join("a"), "m", Host::Hookshot).unwrap();
        let built: std::collections::BTreeSet<String> = files(&a.root.join("romfs")).into_iter().filter_map(|(f, _)| f.replace(std::path::MAIN_SEPARATOR, "/").strip_prefix("Pack/Actor/").and_then(|x| x.strip_suffix(".pack.zs")).map(|x| x.to_string())).filter(|x| x.starts_with("Obj_")).collect();
        assert!(scene(&g).iter().all(|p| built.contains(p)), "{built:?}");
        let _ = fs::remove_dir_all(&load);
    }
}
