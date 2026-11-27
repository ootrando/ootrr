use std::collections::{BTreeMap as BM, BTreeSet as BS};
use std::fmt::Write as W;
use std::fs;
use std::path::{Path as P, PathBuf as PB};

use crate::byml::Byml;
use crate::chest_slot::LIST;
use crate::error::{Error as E, Res};
use crate::flow_locations::dict_id;
use crate::modpack::{Host, Pack};
use crate::pickup_pack::rows::{self, ACTOR_INFO, GAME_ACTOR_INFO, POUCH_INFO};
use crate::pickup_pack::write::start;
use crate::pickup_pack::{pack_of, sizes, PACK_DICT};
use crate::romfs::Romfs;
use crate::rsizetable::{self, Rst, TABLE_PATH};
use crate::sarc::{self, Entry};
use crate::shelf::actor::{self, int, Shelf};
use crate::shelf::goods::{self, Edit};
use crate::shelf::{banc, counters, hook, inner, split, text, Job, What, BASE, KEEPER_PACKS};
use crate::staging::{BANC_DICT, MARGIN};
use crate::zs;
use crate::TITLE_ID;

#[derive(Debug, Clone)]
pub struct File {
    pub path: String,
    pub raw: Vec<u8>,
    pub old: usize,
    pub inner: Vec<(String, usize, usize)>,
}

#[derive(Debug, Clone)]
pub struct Done {
    pub root: PB,
    pub files: usize,
    pub shelves: Vec<String>,
    pub report: String,
}

struct Ctx<'a> {
    rf: Romfs,
    base: Option<&'a PB>,
    modded: bool,
}

impl Ctx<'_> {
    fn read(&mut self, path: &str) -> Res<Vec<u8>> {
        start(&mut self.rf, self.base, path)
    }

    fn old(&mut self, path: &str, first: &[u8]) -> Res<usize> {
        if self.modded {
            return Ok(first.len());
        }
        Ok(self.rf.read_zs(path)?.len())
    }

    fn file(&mut self, path: &str, edit: impl FnOnce(&[u8]) -> Res<Vec<u8>>) -> Res<File> {
        let first: Vec<u8> = self.read(path)?;
        let raw: Vec<u8> = edit(&first)?;
        let old: usize = self.old(path, &first)?;
        Ok(File { path: path.to_string(), raw, old, inner: Vec::new() })
    }
}

fn base_of(base: Option<&P>) -> Res<Option<PB>> {
    let Some(b) = base else {
        return Ok(None);
    };
    if !b.is_dir() {
        return Err(E::Msg(format!("the base folder {} does not exist", b.display())));
    }
    Ok(Some(b.to_path_buf()))
}

fn shelves(c: &mut Ctx, jobs: &[Job]) -> Res<Vec<Shelf>> {
    let mut out: Vec<Shelf> = Vec::new();
    for j in jobs {
        let What::Deal { item, price, model } = &j.what else {
            continue;
        };
        let name: String = j.shelf_actor();
        if out.iter().any(|s| s.name == name) {
            continue;
        }
        out.push(actor::make(&mut c.rf, c.base, item, model.as_deref(), &name, *price).map_err(|e| E::Msg(format!("the shelf actor for {item} cannot be built: {e}")))?);
    }
    Ok(out)
}

fn base_price(t: &Byml, actor: &str) -> Res<i64> {
    rows::find(t, actor).and_then(|r| int(r, "BuyingPrice")).ok_or_else(|| E::Msg(format!("{actor} has no buying price")))
}

fn edits(rf: &mut Romfs, jobs: &[Job]) -> Res<BM<(String, usize), Edit>> {
    let vanilla: Byml = Byml::parse(&rf.read_zs(POUCH_INFO)?)?;
    let mut out: BM<(String, usize), Edit> = BM::new();
    for j in jobs {
        let e: Edit = match &j.what {
            What::Goods { actor, price } | What::Once { actor, price } => {
                let b: i64 = base_price(&vanilla, actor)?;
                Edit { slot: j.slot, actor: actor.clone(), offset: (*price as i64 - b) as i32, stock: if j.once() { 1 } else { -1 } }
            }
            What::Deal { .. } => Edit { slot: j.slot, actor: j.shelf_actor(), offset: 0, stock: 1 },
        };
        for k in &j.keepers {
            match out.get(&(k.clone(), j.slot)) {
                Some(x) if *x != e => return Err(E::Msg(format!("{} slot {} gets two different goods", k, j.slot))),
                _ => {
                    out.insert((k.clone(), j.slot), e.clone());
                }
            }
        }
    }
    Ok(out)
}

fn keeper_files(c: &mut Ctx, jobs: &[Job]) -> Res<(Vec<File>, BM<String, String>)> {
    let per: BM<(String, usize), Edit> = edits(&mut c.rf, jobs)?;
    let mut packs: BM<String, Vec<Entry>> = BM::new();
    for k in KEEPER_PACKS {
        packs.insert(k.to_string(), sarc::parse(&c.read(&pack_of(k))?)?);
    }
    // The Kakariko Potion Shop gets a goods list of its own (shelf::split).
    let parted: bool = per.keys().any(|(k, _)| k == split::KEEPER);
    let mut grown: split::Grown = Vec::new();
    if parted {
        let es: &mut Vec<Entry> = packs.get_mut(split::KEEPER).ok_or_else(|| E::Msg(format!("{} is not a shelf keeper", split::KEEPER)))?;
        grown = split::split(es)?;
    }
    let mut refs: BM<String, String> = BM::new();
    let mut by_file: BM<String, Vec<Edit>> = BM::new();
    for ((k, _), e) in &per {
        let es: &Vec<Entry> = packs.get(k).ok_or_else(|| E::Msg(format!("{k} is not a shelf keeper")))?;
        let f: String = goods::shop_ref(es, k)?;
        refs.insert(k.clone(), f.clone());
        let v: &mut Vec<Edit> = by_file.entry(f).or_default();
        match v.iter().find(|x| x.slot == e.slot) {
            Some(x) if x != e => return Err(E::Msg(format!("two keepers that share a goods list want different goods in slot {}", e.slot))),
            Some(_) => {}
            None => v.push(e.clone()),
        }
    }
    let mut fresh: BM<String, (Vec<u8>, usize)> = BM::new();
    for (f, es) in &by_file {
        let holder: &Vec<Entry> = packs.values().find(|p| sarc::get(p, f).is_some()).ok_or_else(|| E::Msg(format!("no keeper pack holds {f}")))?;
        let raw: Vec<u8> = sarc::get(holder, f).map(|e| e.data.clone()).unwrap_or_default();
        let w: Vec<u8> = goods::edit(&raw, es)?;
        fresh.insert(f.clone(), (w, raw.len()));
    }
    let mut out: Vec<File> = Vec::new();
    let mut sized: BS<String> = BS::new();
    for k in KEEPER_PACKS {
        let es: &Vec<Entry> = &packs[k];
        let hit: Vec<&String> = fresh.keys().filter(|f| sarc::get(es, f).is_some()).collect();
        let parts: bool = parted && k == split::KEEPER;
        if hit.is_empty() && !parts {
            continue;
        }
        let path: String = pack_of(k);
        let mut inner: Vec<(String, usize, usize)> = if parts { grown.clone() } else { Vec::new() };
        let mut file: File = c.file(&path, |r: &[u8]| {
            let mut es: Vec<Entry> = sarc::parse(r)?;
            if parts {
                split::split(&mut es)?;
            }
            for f in &hit {
                let (w, old): &(Vec<u8>, usize) = &fresh[*f];
                sarc::set(&mut es, f, w.clone());
                if sized.insert((*f).clone()) {
                    inner.push(((*f).clone(), *old, w.len()));
                }
            }
            sarc::write(&es)
        })?;
        file.inner = inner;
        out.push(file);
    }
    Ok((out, refs))
}

/// How far each new shelf actor stands above the vanilla shelf spot: its model hangs that far
/// below its origin (lift::below_of), while the shop's own goods start at theirs.
fn seats(rf: &mut Romfs, made: &[Shelf]) -> Res<BM<String, f32>> {
    let t: Byml = rf.read_byml(ACTOR_INFO)?;
    Ok(made.iter().map(|s| (s.name.clone(), rows::find(&t, &s.look).and_then(crate::pickup_pack::lift::below_of).unwrap_or(0.0))).collect())
}

fn banc_files(c: &mut Ctx, jobs: &[Job], made: &[Shelf]) -> Res<Vec<File>> {
    let mut bancs: BS<String> = BS::new();
    for j in jobs {
        bancs.extend(j.bancs.iter().cloned());
    }
    let stock: Vec<Byml> = bancs.iter().map(|b| Byml::parse(&c.rf.read_zs(b)?)).collect::<Res<Vec<Byml>>>()?;
    let map: BM<String, String> = banc::board_map(&stock);
    let lift: BM<String, f32> = seats(&mut c.rf, made)?;
    let mut hits: BM<(String, usize), usize> = BM::new();
    let mut out: Vec<File> = Vec::new();
    for (b, vanilla) in bancs.iter().zip(&stock) {
        let mut n: usize = 0;
        let file: File = c.file(b, |r: &[u8]| {
            let mut y: Byml = Byml::parse(r)?;
            for j in jobs.iter().filter(|j| j.bancs.contains(b)) {
                for k in &j.keepers {
                    let h: usize = banc::swap(&mut y, k, j.slot, &j.shelf_actor());
                    if let Some(dy) = lift.get(&j.shelf_actor()) {
                        banc::seat(&mut y, vanilla, k, j.slot, *dy);
                    }
                    if let Some(w) = map.get(&j.shelf_actor()) {
                        n += banc::boards(&mut y, k, j.slot, w);
                    }
                    *hits.entry((k.clone(), j.slot)).or_insert(0) += h;
                    n += h;
                }
            }
            y.write()
        })?;
        if n > 0 {
            out.push(file);
        }
    }
    for j in jobs {
        for k in &j.keepers {
            if hits.get(&(k.clone(), j.slot)).copied().unwrap_or(0) == 0 {
                return Err(E::Msg(format!("no shelf actor of {k} slot {} was found in its Banc files", j.slot)));
            }
        }
    }
    Ok(out)
}

fn table_files(c: &mut Ctx, made: &[Shelf]) -> Res<Vec<File>> {
    if made.is_empty() {
        return Ok(Vec::new());
    }
    let firsts: Vec<Vec<u8>> = [ACTOR_INFO, GAME_ACTOR_INFO, POUCH_INFO].iter().map(|t| c.read(t)).collect::<Res<Vec<Vec<u8>>>>()?;
    let mut ts: [Byml; 3] = [Byml::parse(&firsts[0])?, Byml::parse(&firsts[1])?, Byml::parse(&firsts[2])?];
    for sh in made {
        actor::add_rows(&mut ts, sh)?;
    }
    let mut out: Vec<File> = Vec::new();
    for (i, t) in [ACTOR_INFO, GAME_ACTOR_INFO, POUCH_INFO].iter().enumerate() {
        let old: usize = c.old(t, &firsts[i])?;
        out.push(File { path: t.to_string(), raw: ts[i].write()?, old, inner: Vec::new() });
    }
    Ok(out)
}

fn counter_file(c: &mut Ctx, jobs: &[Job]) -> Res<Option<File>> {
    let names: Vec<String> = jobs.iter().filter(|j| j.once()).flat_map(|j| j.counters.iter().cloned()).collect();
    if names.is_empty() {
        return Ok(None);
    }
    let f: File = c.file(LIST, |r: &[u8]| {
        let mut y: Byml = Byml::parse(r)?;
        counters::keep(&mut y, &names)?;
        y.write()
    })?;
    Ok(Some(f))
}

fn text_files(c: &mut Ctx, made: &[Shelf]) -> Res<Vec<File>> {
    let mut ids: Vec<String> = made.iter().map(|s| s.who.clone()).collect();
    ids.sort();
    ids.dedup();
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut langs: Vec<String> = c.rf.list("Mals")?.into_iter().filter(|f| f.ends_with(text::ARCHIVE_TAIL)).collect();
    langs.sort();
    if langs.is_empty() {
        return Err(E::Format("the game has no message archives"));
    }
    let mut out: Vec<File> = Vec::new();
    for f in &langs {
        out.push(c.file(f, |r: &[u8]| Ok(text::archive(r, &ids)?.0))?);
    }
    Ok(out)
}

fn size_table(c: &mut Ctx) -> Res<Rst> {
    match c.base.map(|b| b.join(TABLE_PATH)).filter(|p| p.is_file()) {
        Some(p) => {
            c.modded = true;
            Rst::parse(&c.rf.zs.decompress(&fs::read(p)?)?)
        }
        None => rsizetable::load(&mut c.rf),
    }
}

fn dict(rf: &Romfs, path: &str) -> Res<Option<u32>> {
    let id: Option<u32> = dict_id(&rf.read(path)?);
    Ok(if path.starts_with("Banc/") { Some(id.unwrap_or(BANC_DICT)) } else { id })
}

pub fn run(romfs: &P, base: Option<&P>, load: &P, mod_name: &str, host: Host, jobs: &[Job]) -> Res<Done> {
    run_with(romfs, base, load, mod_name, host, jobs, &[])
}

pub fn run_with(romfs: &P, base: Option<&P>, load: &P, mod_name: &str, host: Host, jobs: &[Job], credits: &[hook::Credit]) -> Res<Done> {
    if jobs.is_empty() {
        return Err(E::Format("no shelf slot to write"));
    }
    let b: Option<PB> = base_of(base)?;
    let mut c: Ctx = Ctx { rf: Romfs::open(romfs)?, base: b.as_ref(), modded: false };
    let mut rst: Rst = size_table(&mut c)?;
    let made: Vec<Shelf> = shelves(&mut c, jobs)?;
    let mut files: Vec<File> = table_files(&mut c, &made)?;
    let (keepers, refs): (Vec<File>, BM<String, String>) = keeper_files(&mut c, jobs)?;
    files.extend(keepers);
    files.extend(banc_files(&mut c, jobs, &made)?);
    files.extend(counter_file(&mut c, jobs)?);
    // Every shelf build lets a child buy a tunic, wherever the shuffle puts one.
    let sold: Vec<hook::Sold> = hook::plan(jobs);
    let shared: Vec<Vec<String>> = hook::shared(jobs);
    let keys: std::collections::BTreeSet<crate::dungeon_packs::Pack> = jobs
        .iter()
        .filter_map(|j| match &j.what {
            What::Deal { item, .. } => crate::dungeon_packs::by_name(item),
            _ => None,
        })
        .collect();
    files.push(c.file(hook::FLOW, |r: &[u8]| crate::dungeon_packs::shop_file(&hook::edit_with(r, &sold, &shared, credits, true)?, &keys))?);
    files.extend(text_files(&mut c, &made)?);
    let mut note: String = String::new();
    let pk: Pack = Pack::create_for(load, TITLE_ID, mod_name, host)?;
    let id: u32 = dict_id(&c.rf.read(&pack_of(BASE))?).unwrap_or(PACK_DICT);
    for sh in &made {
        let path: String = pack_of(&sh.name);
        rst.set_size(&path, sizes::pack_size(sh.pack.len())?)?;
        for (f, len) in &sh.fresh {
            let n: u32 = sizes::file_size(&rst, &sh.peers, f, *len)?;
            rst.set_size(f, n)?;
        }
        pk.write_romfs(&path, &zs::compress_raw(&sh.pack, Some(id))?)?;
        let _ = writeln!(note, "  new shelf actor {} for {} at {} rupees, pouch identity {}, {} bytes", sh.name, sh.item, sh.price, sh.who, sh.pack.len());
    }
    if refs.get(split::KEEPER).is_some_and(|f| f == split::OWN) {
        split::size(&mut rst)?;
    }
    for f in &files {
        pk.write_romfs(&f.path, &zs::compress_raw(&f.raw, dict(&c.rf, &f.path)?)?)?;
        if f.raw.len() > f.old {
            rst.grow(&f.path, f.old as u64, f.raw.len() as u64, MARGIN)?;
        }
        for (p, old, new) in &f.inner {
            let n: u32 = inner::grow(&mut rst, p, *old, *new)?;
            let _ = writeln!(note, "  inner file {p}: {old} -> {new} bytes, size table entry {n}");
        }
    }
    pk.write_romfs(TABLE_PATH, &rst.to_zs()?)?;
    for (k, f) in &refs {
        let _ = writeln!(note, "  keeper {k} reads {f}");
    }
    let shelves: Vec<String> = made.iter().map(|s| s.name.clone()).collect();
    Ok(Done { root: pk.root, files: files.len() + made.len() + 1, shelves, report: note })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shelf::audit;
    use crate::slice::data::Data;

    fn real() -> Option<PB> {
        let p: PB = ootrr_game::test_romfs::path();
        if p.join("Pack").join("ZsDic.pack.zs").is_file() {
            Some(p)
        } else {
            eprintln!("skipped, no RomFS dump at {} (set OOTRR_ROMFS)", p.display());
            None
        }
    }

    fn jobs(d: &Data, list: &[&str]) -> Vec<Job> {
        list.iter().map(|x| crate::shops::jobs::parse(d, x).unwrap()).collect()
    }

    #[test]
    fn the_verified_kokiri_deal_and_moved_goods_build_and_read_back() {
        let Some(root) = real() else {
            return;
        };
        let d: Data = Data::load().unwrap();
        let js: Vec<Job> = jobs(&d, &["KF Shop Right Side Bottom Left=Boomerang@20", "GC Shop Left Side Top Left=Buy Bottle Bug@30", "Market Potion Shop Left Side Top Left=Buy Heart@5", "Kak Potion Shop Left Side Top Left=Buy Heart@5", "Market Bombchu Shop Left Side Top Left=Hover Boots@250"]);
        let load: PB = std::env::temp_dir().join(format!("ootrr_shelf_stage_{}", std::process::id()));
        let _ = fs::remove_dir_all(&load);
        let done: Done = run(&root, None, &load, "shelf", Host::Hookshot, &js).unwrap();
        assert_eq!(done.shelves, vec!["Shop_OOTRR_Item_Boomerang_20".to_string(), "Shop_OOTRR_Item_HoverBoot_250".to_string()]);
        let back: PB = done.root.join("romfs");
        let lines: Vec<String> = audit::run(&root, &back, &js).unwrap();
        assert!(lines.iter().any(|l| l.contains("0 problems")), "{lines:?}");
        let mut rf: Romfs = Romfs::open(&root).unwrap();
        let es: Vec<Entry> = sarc::parse(&rf.zs.decompress(&fs::read(back.join(pack_of("Npc_Kokiri_001_Shop_Child_ShortStock"))).unwrap()).unwrap()).unwrap();
        let f: String = goods::shop_ref(&es, "Npc_Kokiri_001_Shop_Child_ShortStock").unwrap();
        let g = goods::goods(&sarc::get(&es, &f).unwrap().data).unwrap();
        assert_eq!(g[6].get("StockNum"), Some(&crate::byml::Node::I32(1)));
        // The Boomerang deal stands its model's depth below the origin above the vanilla spot.
        let kf: &str = "Banc/KokiriForest/KokiriForest_Child_Static.bcett.byml.zs";
        let y_of = |y: &Byml, g: &str| -> Vec<f32> {
            let list = y.root.get("Actors").and_then(|l| l.as_arr()).unwrap();
            list.iter().filter(|n| n.get("Gyaml").and_then(|v| v.as_str()) == Some(g)).filter_map(|n| match n.get("Translate")?.as_arr()?.get(1)? {
                crate::byml::Node::F32(v) => Some(*v),
                _ => None,
            }).collect()
        };
        let built: Byml = Byml::parse(&rf.zs.decompress(&fs::read(back.join(kf)).unwrap()).unwrap()).unwrap();
        let vanilla: Byml = Byml::parse(&rf.read_zs(kf).unwrap()).unwrap();
        let t: Byml = rf.read_byml(ACTOR_INFO).unwrap();
        let dy: f32 = rows::find(&t, "Item_Boomerang").and_then(crate::pickup_pack::lift::below_of).unwrap();
        assert!(dy > 0.02, "{dy}");
        let raised: Vec<f32> = y_of(&built, "Shop_OOTRR_Item_Boomerang_20");
        let shield: Vec<f32> = y_of(&vanilla, "Shop_Weapon_Shield_Deku");
        assert!(!raised.is_empty() && raised.len() <= shield.len(), "{raised:?} {shield:?}");
        assert!(raised.iter().all(|r| shield.iter().any(|s| (r - (s + dy)).abs() < 1e-4)), "{raised:?} {shield:?} {dy}");
        assert!(g[6].get("Release").is_none_or(|r| r.get("SoldOutGameData").is_none() || r.get("IsCheckSoldOutGameData") != Some(&crate::byml::Node::Bool(true))));
        // A second pass on a built mod adds a deal whose shop has one keeper: a shop with a keeper
        // per age adds the shared sold out entry (hook::SYNC), which the first pass already wrote.
        let again: Done = run(&root, Some(&back), &load, "shelf", Host::Hookshot, &jobs(&d, &["Market Bazaar Right Side Bottom Left=Iron Boots@60"])).unwrap();
        assert_eq!(again.root, done.root);
        let both: Vec<Job> = jobs(&d, &["KF Shop Right Side Bottom Left=Boomerang@20", "Market Bazaar Right Side Bottom Left=Iron Boots@60"]);
        audit::run(&root, &back, &both).unwrap();
        let _ = fs::remove_dir_all(&load);
    }

    #[test]
    fn strength_deals_build_at_every_tier_and_the_bracelet_reports_its_saved_name() {
        let Some(root) = real() else {
            return;
        };
        let d: Data = Data::load().unwrap();
        let js: Vec<Job> = jobs(&d, &["GC Shop Left Side Bottom Right=Item_Gauntlet_Goron@10", "GC Shop Left Side Top Left=Item_Gauntlet_Silver@10", "GC Shop Left Side Top Right=Item_Gauntlet_Gold@10"]);
        let load: PB = std::env::temp_dir().join(format!("ootrr_shelf_strength_{}", std::process::id()));
        let _ = fs::remove_dir_all(&load);
        let done: Done = run(&root, None, &load, "strength", Host::Hookshot, &js).unwrap();
        for who in ["pouch identity Armor_Child_Gauntlet_Goron", "pouch identity Armor_Adult_Gauntlet_Silver", "pouch identity Armor_Adult_Gauntlet_Gold"] {
            assert!(done.report.contains(who), "{who}: {}", done.report);
        }
        let back: PB = done.root.join("romfs");
        let lines: Vec<String> = audit::run(&root, &back, &js).unwrap();
        assert!(lines.iter().any(|l| l.contains("0 problems")), "{lines:?}");
        assert!(back.join("Event/EventFlow/DisplayShop.bfevfl.zs").exists(), "the child tunic edit is always written");
        let _ = fs::remove_dir_all(&load);
    }

    #[test]
    fn the_two_potion_shops_sell_different_goods_in_one_slot() {
        let Some(root) = real() else {
            return;
        };
        let d: Data = Data::load().unwrap();
        let js: Vec<Job> = jobs(&d, &["Market Potion Shop Left Side Top Left=Buy Heart@5", "Kak Potion Shop Left Side Top Left=Buy Fish@50", "Kak Potion Shop Left Side Top Right=Hover Boots@40"]);
        let load: PB = std::env::temp_dir().join(format!("ootrr_shelf_stage_two_{}", std::process::id()));
        let _ = fs::remove_dir_all(&load);
        let done: Done = run(&root, None, &load, "shelf", Host::Hookshot, &js).unwrap();
        assert!(done.report.contains(&format!("keeper {} reads {}", split::KEEPER, split::OWN)), "{}", done.report);
        let back: PB = done.root.join("romfs");
        let lines: Vec<String> = audit::run(&root, &back, &js).unwrap();
        assert!(lines.iter().any(|l| l.contains("0 problems")), "{lines:?}");
        let rst: Rst = Rst::parse(&Romfs::open(&root).unwrap().zs.decompress(&fs::read(back.join(TABLE_PATH)).unwrap()).unwrap()).unwrap();
        assert!(rst.get(split::OWN).is_some());
        let again: Done = run(&root, Some(&back), &load, "shelf", Host::Hookshot, &jobs(&d, &["Kak Potion Shop Left Side Top Left=Buy Heart@5"])).unwrap();
        assert_eq!(again.root, done.root);
        audit::run(&root, &back, &jobs(&d, &["Kak Potion Shop Left Side Top Left=Buy Heart@5", "Market Potion Shop Left Side Top Left=Buy Heart@5"])).unwrap();
        let _ = fs::remove_dir_all(&load);
    }

    #[test]
    fn every_item_class_that_may_stand_on_a_shelf_builds_its_shelf_actor() {
        let Some(root) = real() else {
            return;
        };
        let d: Data = Data::load().unwrap();
        let t = crate::shops::table().unwrap();
        let mut rf: Romfs = Romfs::open(&root).unwrap();
        let mut want: Vec<(String, Option<String>)> = Vec::new();
        for i in d.items.iter().filter(|i| crate::shops::rules::class(&d, &i.name).is_some() && i.name != crate::bombchu_bag::ITEM) {
            match crate::shops::rules::class(&d, &i.name).unwrap() {
                crate::shops::rules::Class::Upgrade => want.extend(i.tiers.iter().map(|a| (a.clone(), crate::shops::jobs::donor(t, a)))),
                crate::shops::rules::Class::Song => want.push((i.actor.clone().unwrap(), t.song_models.first().cloned())),
                crate::shops::rules::Class::Stack => assert!(rf.root.join(pack_of(&t.stack.iter().find(|s| s.0 == i.name).unwrap().1)).is_file(), "{}", i.name),
                _ => want.push((i.actor.clone().unwrap(), None)),
            }
        }
        want.sort();
        want.dedup();
        let mut bad: Vec<String> = Vec::new();
        for m in t.song_models.iter().skip(1) {
            want.push(("Item_Melody_SariaSong".to_string(), Some(m.clone())));
        }
        let mut langs: Vec<String> = rf.list("Mals").unwrap().into_iter().filter(|f| f.ends_with(text::ARCHIVE_TAIL)).collect();
        langs.sort();
        let mals: Vec<u8> = rf.read_zs(&langs[0]).unwrap();
        for (a, m) in &want {
            match actor::make(&mut rf, None, a, m.as_deref(), &crate::shelf::shelf_name(a, 10), 10) {
                Ok(sh) => {
                    if let Err(e) = text::archive(&mals, &[sh.who.clone()]) {
                        bad.push(format!("{a}: {e}"));
                    }
                }
                Err(e) => bad.push(format!("{a}: {e}")),
            }
        }
        assert!(bad.is_empty(), "{bad:#?}");
        assert!(want.len() > 30, "{}", want.len());
    }
}
