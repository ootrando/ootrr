use std::collections::{BTreeMap as BM, BTreeSet as BS};

use crate::progressive::{needs_every_holder, Grant, Wrap, FAMILIES};
use crate::settings::seed::SeedFile;
use crate::slice::data::Data;
use crate::slice::edit::is_sale;

pub const PRIZES: &[(&str, &str, bool)] = &[
    ("Market Shooting Gallery Reward", "Slingshot", false),
    ("Market Bombchu Bowling First Prize", "BombBag", false),
    ("DMC Great Fairy Reward", "Magic", true),
];

pub fn wraps(d: &Data, seed: &SeedFile) -> BM<String, &'static str> {
    let mut out: BM<String, &'static str> = BM::new();
    for (loc, item) in &seed.placements {
        if !is_sale(d, loc) || !d.flow_ok(loc) || crate::scrubs_a::home(d, loc, item) || crate::slice::flow_rules::priced_sale(d, loc) {
            continue;
        }
        let Some(f) = d.family(item) else {
            continue;
        };
        let covered: bool = d.holders(f).iter().all(|h| seed.placements.contains_key(*h) || d.wrappable(h));
        if needs_every_holder(f) && !covered {
            continue;
        }
        out.insert(loc.clone(), f);
    }
    out
}

pub fn grants(d: &Data, seed: &SeedFile, chest_families: &BS<&str>, placed: &BM<String, &'static str>) -> Vec<Grant> {
    let mut fams: BS<&str> = chest_families.clone();
    fams.extend(placed.values().copied());
    let mut out: Vec<Grant> = Vec::new();
    for f in FAMILIES.iter().filter(|x| fams.contains(x.0)) {
        out.extend(d.grants_except(f.0, &|n| seed.placements.contains_key(n)));
    }
    for (loc, f) in placed {
        if crate::flow_extra::cows::is(loc) {
            continue;
        }
        let Some(l) = d.flow.by_name(loc) else {
            continue;
        };
        let forms: Vec<String> = crate::ocarina_hold::forms(d, loc, f);
        for e in &l.edits {
            let files: Vec<&String> = std::iter::once(&e.file).chain(e.also.iter()).collect();
            for file in files {
                out.push(Grant { file: file.clone(), index: e.index, event: e.event.clone(), param: e.param.clone(), family: f, wrap: Wrap::Chain, forms: forms.clone() });
            }
        }
    }
    out.extend(guards(d, seed));
    out
}

pub fn guards(d: &Data, seed: &SeedFile) -> Vec<Grant> {
    let mut out: Vec<Grant> = Vec::new();
    for (loc, fam, plain) in PRIZES {
        if seed.placements.contains_key(*loc) {
            continue;
        }
        let (Some(l), Some(family)) = (d.flow.by_name(loc), FAMILIES.iter().find(|x| x.0 == *fam).map(|x| x.0)) else {
            continue;
        };
        for e in l.edits.iter().filter(|e| e.role == "grant") {
            out.push(Grant { file: e.file.clone(), index: e.index, event: e.event.clone(), param: e.param.clone(), family, wrap: Wrap::Base { plain: *plain }, forms: Vec::new() });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seed_with(d: &Data, pairs: &[(&str, &str)]) -> SeedFile {
        let mut map: BM<String, String> = BM::new();
        for (k, v) in pairs {
            map.insert(k.to_string(), v.to_string());
        }
        SeedFile::new("t", &crate::settings::Settings::default(), &d.scan.game, map).unwrap()
    }

    #[test]
    fn a_family_item_on_a_flow_location_becomes_a_wrap_and_the_vanilla_holder_is_skipped() {
        let d: Data = Data::load().unwrap();
        let s: SeedFile = seed_with(&d, &[("Kak 50 Gold Skulltula Reward", "Deku Stick Capacity"), ("Kak 10 Gold Skulltula Reward", "Piece of Heart")]);
        let w: BM<String, &'static str> = wraps(&d, &s);
        assert_eq!(w.get("Kak 50 Gold Skulltula Reward"), Some(&"StickBag"));
        assert_eq!(w.len(), 1);
        let fams: BS<&str> = BS::new();
        let g: Vec<Grant> = grants(&d, &s, &fams, &w);
        assert!(g.iter().any(|x| x.index == 249 && x.family == "StickBag"));
        let mut chest: BS<&str> = BS::new();
        chest.insert("Wallet");
        let g2: Vec<Grant> = grants(&d, &s, &chest, &w);
        assert!(g2.iter().all(|x| x.index != 245));
        assert!(g2.iter().any(|x| x.index == 247 && x.family == "Wallet"));
    }

    #[test]
    fn progressive_cows_use_once_grants_instead_of_wrapping_the_milk() {
        let d: Data = Data::load().unwrap();
        let s: SeedFile = seed_with(&d, &[
            ("KF Links House Cow", "Bow"),
            ("GV Cow", "Deku Stick Capacity"),
            ("GF Cow", "Magic Meter"),
            ("DMT Cow Grotto Cow", "Bomb Bag"),
        ]);
        let w: BM<String, &'static str> = wraps(&d, &s);
        assert_eq!(w.len(), 4);
        let gs: Vec<Grant> = grants(&d, &s, &BS::new(), &w);
        assert!(!gs.iter().any(|g| g.file.starts_with("Animal_Cow")));
        let cows = crate::flow_extra::cows::progressive_grants(&d, &w).unwrap();
        assert_eq!(cows.len(), 4);
        for g in cows {
            let crate::flow_extra::How::Once(w) = &g.op.how else { panic!("cow grant is not once") };
            assert_eq!(w.sub, "Dm_SY_GetInEvent:GetInEvent_NotTurn_NotNpc");
            assert_eq!(w.params.iter().find(|(k, _)| k == "ActorName").map(|(_, v)| v),
                Some(&crate::bfevfl::Val::Str(crate::progressive::tiers(g.family).unwrap()[0].to_string())));
        }
        let held: BM<String, &'static str> = [("KF Links House Cow".to_string(), "Ocarina")].into_iter().collect();
        let cows = crate::flow_extra::cows::progressive_grants(&d, &held).unwrap();
        assert_eq!(cows[0].forms, crate::ocarina_hold::forms_of("Ocarina"));
        let held_seed: SeedFile = seed_with(&d, &[("KF Links House Cow", "Ocarina")]);
        let packs = crate::ocarina_hold::wanted(&d, &held_seed);
        assert!(packs.iter().any(|(loc, p)| loc == "KF Links House Cow" && p.pack == "Obj_Ocarina_Fairy"));
        assert!(packs.iter().any(|(loc, p)| loc == "KF Links House Cow" && p.pack == "Obj_Ocarina_TimeLand"));
    }

    #[test]
    fn the_vanilla_prizes_are_guarded_only_while_nothing_is_placed_on_them() {
        let d: Data = Data::load().unwrap();
        let bare: SeedFile = seed_with(&d, &[]);
        let g: Vec<Grant> = guards(&d, &bare);
        let mut got: Vec<(String, usize, &str, Wrap)> = g.iter().map(|x| (x.file.clone(), x.index, x.family, x.wrap)).collect();
        got.sort();
        let want: Vec<(String, usize, &str, Wrap)> = vec![
            ("Dm_FairyFountain_Wisdom.bfevfl".to_string(), 72, "Magic", Wrap::Base { plain: true }),
            ("MiniGame_BombchuBowling.bfevfl".to_string(), 297, "BombBag", Wrap::Base { plain: false }),
            ("MiniGame_BombchuBowling.bfevfl".to_string(), 298, "BombBag", Wrap::Base { plain: false }),
            ("MiniGame_HitTarget.bfevfl".to_string(), 291, "Slingshot", Wrap::Base { plain: false }),
            ("MiniGame_HitTarget.bfevfl".to_string(), 292, "Slingshot", Wrap::Base { plain: false }),
        ];
        assert_eq!(got, want);
        let placed: SeedFile = seed_with(&d, &[("Market Shooting Gallery Reward", "Piece of Heart"), ("DMC Great Fairy Reward", "Rupees (50)")]);
        let g2: Vec<Grant> = guards(&d, &placed);
        assert_eq!(g2.iter().map(|x| x.index).collect::<Vec<usize>>(), vec![297, 298]);
        let fams: BS<&str> = BS::new();
        let all: Vec<Grant> = grants(&d, &bare, &fams, &BM::new());
        assert_eq!(all.len(), 5);
    }

    #[test]
    fn a_built_seed_with_rewards_off_carries_the_guards_and_keeps_the_vanilla_branch() {
        use crate::bfevfl::{Bf, Kind, Val};
        use crate::slice::generate::{generate_data, GenOpts, Generated};
        let r: std::path::PathBuf = ootrr_game::test_romfs::path();
        if !r.join("Pack").join("ZsDic.pack.zs").is_file() {
            return;
        }
        let d: Data = Data::load().unwrap();
        let mut st: crate::settings::Settings = crate::settings::Settings::default();
        st.set("shuffle_rewards", crate::settings::Value::s("off")).unwrap();
        let g: Generated = generate_data(&d, &st, "prize guards", &GenOpts::default()).unwrap();
        for (loc, _, _) in PRIZES {
            assert!(!g.seed_file.placements.contains_key(*loc), "{loc}");
        }
        let load: std::path::PathBuf = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/guard_mods");
        let _ = std::fs::remove_dir_all(&load);
        let b: crate::slice::edit::Built = crate::slice::edit::build_for(&g.seed_file, &r, &load, "ootrr-guards", crate::modpack::Host::Eden).unwrap();
        let mut rf: crate::romfs::Romfs = crate::romfs::Romfs::open(&r).unwrap();
        let want: [(&str, usize, &str, &str, &str, &str); 5] = [
            ("MiniGame_HitTarget.bfevfl", 291, "ReveiveItem", "Weapon_Range_SlingShot", "Item_BulletBag_Large", "Item_Range_SlingShot"),
            ("MiniGame_HitTarget.bfevfl", 292, "ReveiveItem", "Weapon_Range_SlingShot", "Item_BulletBag_Medium", "Item_Range_SlingShot"),
            ("MiniGame_BombchuBowling.bfevfl", 297, "ReveiveItem", "Item_BombBag_Small", "Item_BombBag_Medium", "Item_BombBag_Small"),
            ("MiniGame_BombchuBowling.bfevfl", 298, "ReveiveItem", "Item_BombBag_Small", "Item_BombBag_Large", "Item_BombBag_Small"),
            ("Dm_FairyFountain_Wisdom.bfevfl", 72, "ActorName", "Item_FairyBless_MP", "Item_FairyBless_MPDouble", "Item_FairyBless_MP"),
        ];
        for (file, i, param, owner, vanilla, base) in want {
            let z: Vec<u8> = std::fs::read(b.root.join("romfs").join(crate::flow_locations::path_of(file))).unwrap();
            let out: Bf = Bf::parse(&rf.zs.decompress(&z).unwrap()).unwrap();
            let f: &crate::bfevfl::Flow = &out.flows[0];
            let Kind::Switch { cases, params, .. } = &f.events[i].kind else {
                panic!("{file} {i} is not a guard switch");
            };
            assert_eq!(params.as_ref().and_then(|c| c.get("ActorName")), Some(&Val::Str(owner.to_string())), "{file} {i}");
            let item = |ev: u16| -> String {
                let Kind::Sub { params, .. } = &f.events[usize::from(ev)].kind else {
                    panic!("{file} {i}: a guard branch is not a sub");
                };
                match params.as_ref().and_then(|c| c.get(param)) {
                    Some(Val::Str(x)) => x.clone(),
                    other => panic!("{file} {i}: {other:?}"),
                }
            };
            let by = |v: u32| cases.iter().find(|c| c.val == v).map(|c| c.ev).unwrap();
            assert_eq!(item(by(0)), vanilla, "{file} {i}");
            let Kind::Switch { cases: second, params: stock, .. } = &f.events[usize::from(by(1))].kind else {
                panic!("{file} {i}: the second test is not a switch");
            };
            assert_eq!(stock.as_ref().and_then(|c| c.get("Actor")), Some(&Val::Str(owner.to_string())), "{file} {i}");
            let at = |v: u32| second.iter().find(|c| c.val == v).map(|c| c.ev).unwrap();
            assert_eq!(item(at(0)), vanilla, "{file} {i}");
            assert_eq!(item(at(1)), base, "{file} {i}");
        }
        let _ = std::fs::remove_dir_all(&load);
    }

    #[test]
    fn non_family_items_and_unsupported_locations_never_wrap() {
        let d: Data = Data::load().unwrap();
        let s: SeedFile = seed_with(&d, &[("Kak 50 Gold Skulltula Reward", "Bombchus (20)"), ("Song from Impa", "Deku Stick Capacity")]);
        assert!(wraps(&d, &s).is_empty());
    }
}
