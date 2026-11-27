use std::collections::{BTreeSet as BS, HashMap as HM};
use std::fs;
use std::path::PathBuf as PB;

use crate::byml::Byml;
use crate::error::{Error as E, Res};
use crate::flow_locations::dict_id;
use crate::modpack::Pack;
use crate::pickup_pack::build::Plan;
use crate::pickup_pack::kit::{copy_rows, fresh_sizes, hang_of, parent_of, plan, shared_files, sources, token_pack, verify, world, TOKEN};
use crate::pickup_pack::rows::{ACTOR_INFO, GAME_ACTOR_INFO, POUCH_INFO};
use crate::pickup_pack::{pack_of, Spec, Style, PACK_DICT};
use crate::pickup_persist::{kept, rows, Extra};
use crate::romfs::Romfs;
use crate::rsizetable::{self, Rst};
use crate::sarc::{self, Entry};
use crate::slice::skull_edit::Drop;
use crate::staging::MARGIN;
use crate::zs;

pub const TABLES: [&str; 3] = [ACTOR_INFO, GAME_ACTOR_INFO, POUCH_INFO];

pub fn kit_actor(pickup: &str) -> String {
    if pickup == TOKEN { pickup.to_string() } else { hang_of(pickup) }
}

pub fn actors(ds: &[Drop]) -> BS<&str> {
    ds.iter().map(|d| d.actor.as_str()).collect()
}

fn spec(kit: &str) -> Res<Spec> {
    let from: &str = parent_of(kit).ok_or_else(|| E::Msg(format!("{kit} is not a kit pickup, a skulltula that drops it would come back after every visit")))?;
    Ok(Spec { item: from.to_string(), name: kit.to_string(), style: Style::Touch, heap: None })
}

pub fn build(rf: &mut Romfs, token: &[Entry], kit: &str) -> Res<Plan> {
    let s: Spec = spec(kit)?;
    if !rf.root.join(pack_of(&s.item)).is_file() {
        return Err(E::Msg(format!("the game folder has no pickup actor {}, so the kit pickup {kit} cannot be made and a skulltula cannot drop it", s.item)));
    }
    plan(&sources(rf, &s.item, token)?, &s)
}

pub fn check(rf: &mut Romfs, ds: &[Drop], extra: &Extra) -> Res<()> {
    if ds.is_empty() {
        return Ok(());
    }
    world(rf)?;
    let token: Vec<Entry> = token_pack(rf)?;
    for a in actors(ds) {
        match extra.packs.get(a) {
            Some(es) => verify(es, a, &token)?,
            None => verify(&sarc::parse(&build(rf, &token, a)?.sarc)?, a, &token)?,
        };
    }
    let saved: HM<u64, (i64, i64)> = rows(rf)?;
    for d in ds {
        if let Some(h) = [d.hash, d.hash ^ 2].into_iter().find(|h| !kept(&saved, *h)) {
            return Err(E::Msg(format!("{} has no permanent saved row for {h:#018x}, so its drop could not keep it dead", d.name)));
        }
    }
    Ok(())
}

fn current(rf: &mut Romfs, pk: &Pack, path: &str) -> Res<Vec<u8>> {
    let f: PB = pk.work.join("romfs").join(path);
    if f.is_file() { rf.zs.decompress(&fs::read(f)?) } else { rf.read_zs(path) }
}

pub fn write(rf: &mut Romfs, pk: &Pack, rst: &mut Option<Rst>, ds: &[Drop], extra: &Extra) -> Res<usize> {
    let made: Vec<&str> = actors(ds).into_iter().filter(|a| !extra.has(a)).collect();
    if made.is_empty() {
        return Ok(0);
    }
    let token: Vec<Entry> = token_pack(rf)?;
    let shared: Vec<String> = shared_files(&token)?;
    let mut raws: Vec<Vec<u8>> = Vec::new();
    for t in TABLES {
        raws.push(current(rf, pk, t)?);
    }
    let mut ys: [Byml; 3] = [Byml::parse(&raws[0])?, Byml::parse(&raws[1])?, Byml::parse(&raws[2])?];
    if rst.is_none() {
        *rst = Some(rsizetable::load(rf)?);
    }
    let Some(table) = rst.as_mut() else {
        return Err(E::Format("skulltula kits: no size table"));
    };
    let id: u32 = dict_id(&rf.read(&pack_of(TOKEN))?).unwrap_or(PACK_DICT);
    let mut n: usize = 0;
    for a in made {
        let mut pl: Plan = build(rf, &token, a)?;
        if crate::bomb_pickup::quieted(a) {
            // Ammo a skulltula drops is refused silently without its bag, as on a breakable.
            let (b, hit): (Vec<u8>, bool) = crate::bomb_pickup::quiet_all(&pl.sarc)?;
            if !hit {
                return Err(E::Msg(format!("skulltula kits: {a} no longer plays the get event at a full pouch, the game files may have changed")));
            }
            pl.sarc = b;
        }
        copy_rows(&mut ys, &pl)?;
        fresh_sizes(table, &pl, &shared)?;
        pk.write_romfs(&pack_of(a), &zs::compress_raw(&pl.sarc, Some(id))?)?;
        n += 1;
    }
    for (i, t) in TABLES.iter().enumerate() {
        let out: Vec<u8> = ys[i].write()?;
        if !pk.work.join("romfs").join(t).is_file() {
            n += 1;
        }
        pk.write_romfs(t, &zs::compress_raw(&out, dict_id(&rf.read(t)?))?)?;
        table.grow(t, raws[i].len() as u64, out.len() as u64, MARGIN)?;
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use std::path::Path as P;

    use super::*;
    use crate::byml::Node;
    use crate::modpack::Host;
    use crate::pickup_pack::build::pool_of;
    use crate::pickup_pack::kit::{dropped, is_kit, token_like, touch, SKULL_DEATH};
    use crate::pickup_pack::rows;
    use crate::pickup_pack::write::{Done, Opts};
    use crate::placement::banc_files;
    use crate::settings::seed::SeedFile;
    use crate::settings::{Settings, Value};
    use crate::slice::data::Data;
    use crate::slice::edit::{build_for, Built};
    use crate::slice::generate::{generate_data, GenOpts, Generated};
    use crate::slice::skull_edit::drops;
    use crate::slice::skulls::table as skull_table;

    fn real() -> Option<PB> {
        let p: PB = ootrr_game::test_romfs::path();
        if p.join("Pack").join("ZsDic.pack.zs").is_file() {
            Some(p)
        } else {
            eprintln!("skipped, no RomFS dump at {} (set OOTRR_ROMFS)", p.display());
            None
        }
    }

    #[test]
    fn an_ammo_kit_carries_a_pouch_file_the_quiet_edit_reaches() {
        let Some(root) = real() else {
            return;
        };
        let mut rf: Romfs = Romfs::open(&root).unwrap();
        let token: Vec<Entry> = token_pack(&mut rf).unwrap();
        for a in crate::bomb_pickup::ACTORS {
            let kit: String = kit_actor(a);
            assert!(crate::bomb_pickup::quieted(&kit), "{kit}");
            let pl: Plan = build(&mut rf, &token, &kit).unwrap();
            let (out, hit): (Vec<u8>, bool) = crate::bomb_pickup::quiet_all(&pl.sarc).unwrap();
            assert!(hit, "{kit}");
            assert_eq!(out.len(), pl.sarc.len(), "{kit}");
            assert!(!crate::bomb_pickup::quiet_all(&out).unwrap().1, "{kit}");
        }
    }

    fn temp(tag: &str) -> PB {
        let p: PB = std::env::temp_dir().join(format!("ootrr_skull_kit_{tag}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    fn drop(a: &str, h: u64) -> Drop {
        Drop { name: "GS".to_string(), item: "x".to_string(), hash: h, actor: a.to_string() }
    }

    const TREE: u64 = 2504273934093629549;

    #[test]
    fn a_kit_name_is_the_pickup_name_with_a_mark_and_the_token_stays() {
        assert_eq!(kit_actor("Obj_Rupee_Red"), "Obj_Rupee_Red_GH");
        assert_eq!(kit_actor(TOKEN), TOKEN);
        assert_eq!(spec("Obj_Heart_GS").unwrap().item, "Obj_Heart");
        assert_eq!(spec("Obj_Heart_GH").unwrap().item, "Obj_Heart");
        assert!(spec("_GH").is_err());
        assert!(spec("Obj_Heart").is_err());
        assert!(spec("_GS").is_err());
        let ds: Vec<Drop> = vec![drop("Obj_Heart_GS", 1), drop("Obj_Heart_GS", 2), drop("Obj_Bomb_GS", 3)];
        assert_eq!(actors(&ds).len(), 2);
    }

    #[test]
    fn every_pickup_a_skulltula_may_drop_gets_a_kit_that_passes_the_guard() {
        let Some(root) = real() else {
            return;
        };
        let mut rf: Romfs = Romfs::open(&root).unwrap();
        let token: Vec<Entry> = token_pack(&mut rf).unwrap();
        let all: BS<String> = skull_table().unwrap().pickups.values().filter(|a| a.as_str() != TOKEN).cloned().collect();
        // The Business Scrub Seed and the Fairy stay off the drops: their packs are not built on
        // Obj_Item_Base, so no kit can be made of them.
        assert_eq!(all.len(), 17);
        let mut press: Vec<&str> = Vec::new();
        for a in &all {
            let kit: String = kit_actor(a);
            let pl: Plan = build(&mut rf, &token, &kit).unwrap();
            let es: Vec<Entry> = sarc::parse(&pl.sarc).unwrap();
            assert_eq!(verify(&es, &kit, &token).unwrap().len(), 3, "{kit}");
            assert!(is_kit(&es, &kit), "{kit}");
            assert!(!touch(&pool_of(&[es.clone()]).unwrap(), &kit).unwrap(), "{kit}");
            assert!(token_like(&pool_of(&[es.clone()]).unwrap(), &pool_of(&[token.clone()]).unwrap(), &kit).unwrap(), "{kit}");
            let own: Vec<Entry> = sarc::parse(&rf.read_zs(&pack_of(a)).unwrap()).unwrap();
            if !touch(&pool_of(&[own.clone()]).unwrap(), a).unwrap() {
                press.push(a.as_str());
            }
            assert!(!is_kit(&own, a), "{a}");
            assert!(verify(&own, a, &token).is_err(), "{a}");
        }
        assert_eq!(press, ["Obj_Arrow_Fire", "Obj_Heart_Container", "Obj_Heart_Piece"]);
    }

    #[test]
    fn a_plain_pickup_a_missing_pickup_and_a_missing_row_are_refused() {
        let Some(root) = real() else {
            return;
        };
        let mut rf: Romfs = Romfs::open(&root).unwrap();
        let none: Extra = Extra::default();
        assert!(check(&mut rf, &[], &none).is_ok());
        assert!(check(&mut rf, &[drop("Obj_Rupee_Red_GS", TREE)], &none).is_ok());
        let e: String = check(&mut rf, &[drop("Obj_Rupee_Red", TREE)], &none).unwrap_err().to_string();
        assert!(e.contains("would come back"), "{e}");
        let e: String = check(&mut rf, &[drop("Obj_Nothing_GS", TREE)], &none).unwrap_err().to_string();
        assert!(e.contains("no pickup actor"), "{e}");
        let e: String = check(&mut rf, &[drop("Obj_Rupee_Red_GS", 12345)], &none).unwrap_err().to_string();
        assert!(e.contains("no permanent saved row"), "{e}");
        let mut plain: Extra = Extra::default();
        plain.add_pack("Obj_Fake_GS", &rf.read_zs(&pack_of("Obj_Rupee_Red")).unwrap()).unwrap();
        assert!(check(&mut rf, &[drop("Obj_Fake_GS", TREE)], &plain).is_err());
    }

    #[test]
    fn a_skulltula_seed_with_only_kit_drops_builds_through_the_normal_path() {
        let Some(root) = real() else {
            return;
        };
        let d: Data = Data::load().unwrap();
        let mut s: Settings = Settings::default();
        crate::slice::pick::shuffle_both(&mut s);
        s.set("shuffle_rewards", Value::s("off")).unwrap();
        s.set("shuffle_skulltulas", Value::s("matched")).unwrap();
        let g: Generated = generate_data(&d, &s, "kit build 1", &GenOpts::default()).unwrap();
        let mut rf: Romfs = Romfs::open(&root).unwrap();
        let seed: SeedFile = g.seed_file;
        let ds: Vec<Drop> = drops(&d, &seed).unwrap();
        assert!(!ds.is_empty());
        assert!(ds.iter().all(|x| x.actor.ends_with("_GH")));
        let load: PB = temp("seed");
        let b: Built = build_for(&seed, &root, &load, "kits", Host::Hookshot).unwrap();
        let romfs: PB = b.root.join("romfs");
        assert!(!romfs.join(SKULL_DEATH).exists());
        let token: Vec<Entry> = token_pack(&mut rf).unwrap();
        let mut ys: Vec<Byml> = Vec::new();
        for t in TABLES {
            let y: Byml = Byml::parse(&rf.zs.decompress(&fs::read(romfs.join(t)).unwrap()).unwrap()).unwrap();
            assert!(rows::sorted(&y).unwrap(), "{t}");
            ys.push(y);
        }
        let rst: Rst = Rst::parse(&rf.zs.decompress(&fs::read(romfs.join(rsizetable::TABLE_PATH)).unwrap()).unwrap()).unwrap();
        let kits: BS<&str> = actors(&ds);
        // Every seed also builds the song and Ocarina pickups the story skips hold up (crate::ocarina_hold).
        let scene: BS<String> = crate::ocarina_hold::wanted(&d, &seed).into_iter().map(|(_, p)| p.pack).filter(|p| !kits.contains(p.as_str())).collect();
        // An item the seed lays on the ground brings its own pickup pack too (Obj_Range_Bow for a Bow),
        // under the name the build gives it: a renamed pack (Obj_Token_GoldLand, Obj_Arrow_FireLand) or
        // a dungeon item's own pickup (Obj_OOTRR_<Kind>_<Dungeon>) is not the item actor's default name.
        let base: Byml = Byml::parse(&rf.read_zs(TABLES[0]).unwrap()).unwrap();
        let fixed: std::collections::BTreeMap<String, String> = crate::slice::tier_order::fixed_actors(&d, &seed).unwrap().assign;
        let placed: BS<String> = crate::slice::seed_packs::need(&d, &seed, &fixed)
            .unwrap()
            .into_keys()
            .filter(|n| !kits.contains(n.as_str()) && !scene.contains(n) && rows::find(&base, n).is_none())
            .collect();
        let bag: String = crate::bombchu_bag::ACTOR.to_string();
        let want: BS<String> = kits.iter().map(|a| a.to_string()).chain(scene.iter().cloned()).chain(placed.iter().cloned()).chain(std::iter::once(bag)).collect();
        let shipped: std::collections::HashMap<&str, &Node> = rows::index(&base);
        let added: BS<String> = ys[0].root.as_arr().unwrap().iter().filter_map(rows::id).filter(|n| !shipped.contains_key(n)).map(|n| n.to_string()).collect();
        assert_eq!(added, want);
        assert_eq!(ys[0].root.as_arr().unwrap().len(), 8363 + kits.len() + scene.len() + placed.len() + 1);
        for a in &kits {
            let raw: Vec<u8> = rf.zs.decompress(&fs::read(romfs.join(pack_of(a))).unwrap()).unwrap();
            assert_eq!(verify(&sarc::parse(&raw).unwrap(), a, &token).unwrap().len(), 3, "{a}");
            assert_eq!(rst.get(&pack_of(a)), Some(crate::pickup_pack::sizes::pack_size(raw.len()).unwrap()), "{a}");
            for y in &ys {
                assert!(rows::find(y, a).is_some(), "{a}");
            }
            assert_eq!(rows::find(&ys[1], a).and_then(|r| r.get(rows::RESPAWN)), Some(&Node::Bool(true)));
        }
        let mut seen: usize = 0;
        for f in banc_files(&rf).unwrap() {
            let p: PB = romfs.join(&f);
            if !p.is_file() {
                continue;
            }
            let y: Byml = Byml::parse(&rf.zs.decompress(&fs::read(p).unwrap()).unwrap()).unwrap();
            for x in &ds {
                if let Some(a) = dropped(&y, x.hash) {
                    assert_eq!(a, x.actor);
                    seen += 1;
                }
            }
        }
        assert!(seen >= ds.iter().map(|x| x.hash).collect::<BS<u64>>().len());
        assert!(b.report.contains("saved when its item is picked up"));
        let again: Built = build_for(&seed, &root, &load, "kits", Host::Hookshot).unwrap();
        assert_eq!(again.counts.files, b.counts.files);
        let _ = fs::remove_dir_all(&load);
    }

    #[test]
    fn a_new_item_kit_from_the_pool_is_accepted_and_not_written_twice() {
        let Some(root) = real() else {
            return;
        };
        let mut rf: Romfs = Romfs::open(&root).unwrap();
        let s: Spec = Spec { item: "Item_Hookshot".to_string(), name: "Obj_Hookshot_GS".to_string(), style: Style::Touch, heap: None };
        let o: Opts = Opts { host: Host::Hookshot, base: None, rst_from: None, places: Vec::new(), dry: true };
        let done: Done = crate::pickup_pack::kit::run(&root, P::new("unused"), "unused", &s, &o, &[TREE]).unwrap();
        let x: Extra = crate::pickup_pack::guard::extra(&done.files, &done.carried).unwrap();
        assert!(x.has("Obj_Hookshot_GS"));
        let ds: Vec<Drop> = vec![drop("Obj_Hookshot_GS", TREE)];
        assert!(check(&mut rf, &ds, &x).is_ok());
        assert!(check(&mut rf, &ds, &Extra::default()).is_err());
        let load: PB = temp("pool");
        let pk: Pack = Pack::begin_for(&load, crate::TITLE_ID, "pool", Host::Hookshot).unwrap();
        let mut rst: Option<Rst> = None;
        assert_eq!(write(&mut rf, &pk, &mut rst, &ds, &x).unwrap(), 0);
        assert!(rst.is_none());
        let _ = fs::remove_dir_all(&load);
    }
}
