use std::collections::{BTreeMap as BM, BTreeSet as BS};
use std::fmt::Write as W;

use crate::error::Res;
use crate::logic_items::ItemId;
use crate::progressive::{family_of, owners, tiers, FAMILIES};
use crate::search::{explore, Search};
use crate::settings::seed::SeedFile;
use crate::slice::data::Data;
use crate::slice::edit::is_sale;
use crate::slice::support::{plan_with_seed, Plan};
use crate::slice::tier_order::{fixed_actors, rank, slots, Give, Slot, Verdict};
use crate::start_items::Set;
use crate::world::{LocId, Placement, World};

pub const CHAIN: &str = "ownership chain";
pub const FIXED: &str = "fixed tier";
pub const GUARD: &str = "base guard";
pub const VANILLA: &str = "vanilla flow";
pub const DIRECT: &str = "direct grant";

pub const RUNTIME: &str = "runtime";
pub const BASE: &str = "base";
pub const LATE: &str = "late base";
pub const FORCED: &str = "forced";
pub const EARLY: &str = "early";
pub const NO_BASE: &str = "no base";
pub const AWAY: &str = "unreachable";
pub const UNCHECKED: &str = "unchecked";
pub const START_OK: &str = "start ok";
pub const START_BAD: &str = "start no base";
pub const OUTSIDE: &str = "not in logic";

pub const BAD: [&str; 3] = [EARLY, NO_BASE, START_BAD];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub loc: String,
    pub family: &'static str,
    pub check: String,
    pub mech: &'static str,
    pub tier: String,
    pub risk: &'static str,
    pub held: usize,
    pub forced: usize,
}

pub fn check(d: &Data, loc: &str) -> String {
    if crate::shops::is_loc(loc) {
        return "shop deal".to_string();
    }
    if crate::slice::skulls::is_skull(d, loc) {
        return "skulltula drop".to_string();
    }
    if let Some(k) = d.pots.kind_of(loc) {
        return format!("{k:?}").to_lowercase();
    }
    if is_sale(d, loc) {
        if let Some(l) = d.flow.by_name(loc) {
            return format!("flow {}", l.cat);
        }
    }
    if let Some(c) = d.cat.by_name(loc) {
        if crate::slice::pickups::is_world(c) {
            return "ground pickup".to_string();
        }
        if c.refs.iter().any(|r| crate::special::of(r.hash).is_some()) {
            return "chest special".to_string();
        }
        return c.kind.clone();
    }
    "other".to_string()
}

fn forced(w: &World, place: &Placement, it: ItemId, at: LocId) -> (bool, usize, usize) {
    let start: usize = w.start.iter().filter(|x| x.0 == it).map(|x| x.1 as usize).sum();
    let mut p: Placement = place.clone();
    p.item[at] = None;
    let s: Search = explore(w, &p, &[]);
    let reach: bool = s.loc_access(at);
    let others: Vec<LocId> = (0..w.locs.len()).filter(|x| *x != at && place.get(*x) == Some(it) && s.loc_access(*x)).collect();
    let mut n: usize = start;
    for x in &others {
        let mut q: Placement = p.clone();
        q.item[*x] = None;
        if !explore(w, &q, &[]).loc_access(at) {
            n += 1;
        }
    }
    (reach, start + others.len(), n)
}

fn risk_of(reach: bool, k: usize, held: usize, n: usize) -> &'static str {
    if !reach {
        AWAY
    } else if k == 0 && held == 0 {
        BASE
    } else if k == 0 {
        LATE
    } else if n >= k {
        FORCED
    } else if n == 0 {
        NO_BASE
    } else {
        EARLY
    }
}

fn fixed_row(w: &World, place: &Placement, s: &Slot, check: String, k: Option<usize>) -> Row {
    let t: &[&str] = tiers(s.family).unwrap_or(&[]);
    let tier: String = k.and_then(|i| t.get(i)).map(|x| x.to_string()).unwrap_or_else(|| "none".to_string());
    let mut r: Row = Row { loc: s.loc.clone(), family: s.family, check, mech: FIXED, tier, risk: OUTSIDE, held: 0, forced: 0 };
    let (Some(it), Some(l), Some(k)) = (w.item(&s.item), w.loc(&s.loc), k) else {
        return r;
    };
    let (reach, held, n): (bool, usize, usize) = forced(w, place, it, l);
    r.held = held;
    r.forced = n;
    r.risk = risk_of(reach, k, held, n);
    r
}

fn start_rows(d: &Data, p: &Plan) -> Vec<Row> {
    let mut out: Vec<Row> = Vec::new();
    for (logic, n) in p.kit.logic() {
        let Some(f) = d.item(logic).and_then(|i| family_of(&i.tiers)) else {
            continue;
        };
        let Ok(o) = owners(f) else {
            continue;
        };
        let base: &str = o[0];
        let set: bool = p.kit.sets.iter().any(|x| matches!(x, Set::Bool(k, true) if *k == format!("IsGet.{base}")));
        let given: bool = set || p.kit.pushes.contains(&base) || p.start.pouch.contains(&base);
        out.push(Row {
            loc: "starting items".to_string(),
            family: f,
            check: "start items".to_string(),
            mech: DIRECT,
            tier: format!("{n} copies"),
            risk: if given { START_OK } else { START_BAD },
            held: n as usize,
            forced: n as usize,
        });
    }
    out
}

fn guarded(loc: &str) -> bool {
    crate::slice::flow_wrap::PRIZES.iter().any(|p| p.0 == loc)
}

fn prize_rows(d: &Data, seed: &SeedFile) -> Vec<Row> {
    let mut out: Vec<Row> = Vec::new();
    for l in &d.flow.locs {
        if seed.placements.contains_key(&l.name) || d.vanilla_of(&l.name).and_then(|v| d.family(v)).is_some() {
            continue;
        }
        let Some((f, ks)) = FAMILIES.iter().find_map(|(f, _)| {
            let ks: BS<usize> = l.edits.iter().filter(|e| e.role == "grant").filter_map(|e| rank(f, &e.item)).collect();
            (!ks.is_empty()).then_some((*f, ks))
        }) else {
            continue;
        };
        let t: &[&str] = tiers(f).unwrap_or(&[]);
        let names: Vec<&str> = ks.iter().filter_map(|k| t.get(*k).copied()).collect();
        out.push(Row {
            loc: l.name.clone(),
            family: f,
            check: format!("vanilla {}", l.cat),
            mech: if guarded(&l.name) { GUARD } else { VANILLA },
            tier: names.join(" or "),
            risk: if guarded(&l.name) { RUNTIME } else { UNCHECKED },
            held: 0,
            forced: 0,
        });
    }
    out
}

pub fn census(d: &Data, seed: &SeedFile) -> Res<Vec<Row>> {
    let (w, place): (World, Placement) = crate::slice::starts::rebuild(d, seed)?;
    let p: Plan = plan_with_seed(&seed.settings()?.settings, Some(&seed.seed))?;
    let v: Verdict = fixed_actors(d, seed)?;
    let sl: Vec<Slot> = slots(d, &seed.placements);
    let wrapped: BM<String, &'static str> = crate::slice::flow_wrap::wraps(d, seed);
    let used: BS<&str> = sl.iter().filter(|s| s.give == Give::Flow && seed.placements.contains_key(&s.loc)).map(|s| s.family).collect();
    let mut out: Vec<Row> = Vec::new();
    for s in &sl {
        let placed: bool = seed.placements.contains_key(&s.loc);
        let c: String = if placed { check(d, &s.loc) } else { format!("vanilla {}", check(d, &s.loc)) };
        let t: &[&str] = tiers(s.family)?;
        let mut row: Row = match s.give {
            Give::Flow => {
                let chain: bool = if !placed {
                    d.wrappable(&s.loc) && used.contains(s.family)
                } else if is_sale(d, &s.loc) {
                    wrapped.contains_key(&s.loc)
                } else {
                    true
                };
                Row {
                    loc: s.loc.clone(),
                    family: s.family,
                    check: c,
                    mech: if chain { CHAIN } else { VANILLA },
                    tier: "by ownership".to_string(),
                    risk: if chain { RUNTIME } else { UNCHECKED },
                    held: 0,
                    forced: 0,
                }
            }
            Give::Fixed(k) => fixed_row(&w, &place, s, c, Some(k)),
            Give::Open => {
                let k: Option<usize> = v.assign.get(&s.loc).and_then(|a| t.iter().position(|x| x == a));
                fixed_row(&w, &place, s, c, k)
            }
        };
        if !placed && guarded(&s.loc) {
            row.mech = GUARD;
            row.risk = RUNTIME;
        }
        out.push(row);
    }
    out.extend(start_rows(d, &p));
    out.extend(prize_rows(d, seed));
    Ok(out)
}

pub fn text(rows: &[Row]) -> String {
    let mut out: String = String::new();
    for r in rows {
        let _ = writeln!(out, "{}\t{}\t{}\t{}\t{}\t{}\theld {} forced {}", r.family, r.loc, r.check, r.mech, r.tier, r.risk, r.held, r.forced);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{Settings, Value};
    use crate::slice::generate::{generate_data, GenOpts, Generated};

    fn with(pairs: &[(&str, &str)]) -> Settings {
        let mut s: Settings = Settings::default();
        for (k, v) in pairs {
            let val: Value = match *v {
                "true" => Value::Bool(true),
                "false" => Value::Bool(false),
                x => Value::s(x),
            };
            s.set(k, val).unwrap();
        }
        s
    }

    fn sets() -> Vec<(&'static str, Vec<(&'static str, &'static str)>, usize)> {
        let wide: Vec<(&str, &str)> = vec![
            ("shuffle_freestanding_items", "all"),
            ("shuffle_pots", "all"),
            ("shuffle_crates", "all"),
            ("shuffle_skulltulas", "matched"),
            ("shuffle_rocks", "all"),
            ("shuffle_grass", "all"),
            ("shuffle_cows", "true"),
            ("shuffle_beehives", "true"),
            ("shuffle_rewards", "supported"),
        ];
        let shop: Vec<(&str, &str)> = vec![("shuffle_shops", "deals_2")];
        let mut ow: Vec<(&str, &str)> = vec![("shuffle_chests", "overworld")];
        ow.extend(wide.iter().copied());
        let mut du: Vec<(&str, &str)> = vec![("shuffle_chests", "dungeons")];
        du.extend(wide.iter().copied());
        let mut all: Vec<(&str, &str)> = vec![("shuffle_chests", "all")];
        all.extend(wide.iter().copied());
        let mut sh: Vec<(&str, &str)> = all.clone();
        sh.extend(shop.iter().copied());
        let mut dp: Vec<(&str, &str)> = vec![("shuffle_chests", "dungeons")];
        dp.extend(wide.iter().copied().filter(|x| x.0 != "shuffle_freestanding_items"));
        let off: Vec<(&str, &str)> = vec![("shuffle_chests", "all"), ("shuffle_rewards", "off"), ("shuffle_freestanding_items", "all"), ("shuffle_pots", "all")];
        vec![
            ("overworld wide", ow, 8),
            ("dungeons wide", du, 4),
            ("dungeons no freestanding", dp, 6),
            ("all wide", all, 8),
            ("all wide shops", sh, 8),
            ("all rewards off", off, 8),
        ]
    }

    #[test]
    fn every_placed_family_copy_and_every_vanilla_holder_has_a_row() {
        let d: Data = Data::load().unwrap();
        let g: Generated = generate_data(&d, &with(&[("shuffle_chests", "all"), ("shuffle_freestanding_items", "all")]), "census rows", &GenOpts::default()).unwrap();
        let rows: Vec<Row> = census(&d, &g.seed_file).unwrap();
        for (loc, item) in &g.seed_file.placements {
            if d.family(item).is_some() {
                assert!(rows.iter().any(|r| &r.loc == loc), "{loc}");
            }
        }
        for (f, _) in FAMILIES {
            for h in d.holders(f) {
                if !g.seed_file.placements.contains_key(h) {
                    assert!(rows.iter().any(|r| r.loc == h), "{h}");
                }
            }
        }
    }

    #[test]
    #[ignore]
    fn census_of_forty_seeds() {
        let d: Data = Data::load().unwrap();
        let dir: Option<std::path::PathBuf> = std::env::var_os("OOTRR_CENSUS_SEEDS").map(std::path::PathBuf::from);
        let mut by_family: BM<(String, String), usize> = BM::new();
        let mut by_check: BM<(String, String, String), usize> = BM::new();
        let mut lines: Vec<String> = Vec::new();
        let mut seeds: usize = 0;
        let only: Option<String> = std::env::var("OOTRR_CENSUS_SET").ok();
        let mut want: usize = 0;
        for (name, pairs, k) in sets() {
            if only.as_deref().is_some_and(|o| o != name) {
                continue;
            }
            want += k;
            let s: Settings = with(&pairs);
            for n in 0..k {
                let label: String = format!("census {name} {n}");
                let g: Generated = match generate_data(&d, &s, &label, &GenOpts::default()) {
                    Ok(g) => g,
                    Err(e) => {
                        println!("FAILED {label}: {e}");
                        continue;
                    }
                };
                seeds += 1;
                if let Some(p) = &dir {
                    let file: String = format!("{}.ootrr", label.replace(' ', "_"));
                    g.seed_file.write(&p.join(file)).unwrap();
                }
                for r in census(&d, &g.seed_file).unwrap() {
                    *by_family.entry((r.family.to_string(), r.risk.to_string())).or_insert(0) += 1;
                    *by_check.entry((r.check.clone(), r.mech.to_string(), r.risk.to_string())).or_insert(0) += 1;
                    if BAD.contains(&r.risk) || r.risk == UNCHECKED || r.risk == LATE {
                        lines.push(format!("{label}\t{}", text(std::slice::from_ref(&r)).trim_end()));
                    }
                }
            }
        }
        println!("SEEDS {seeds}");
        for ((f, r), n) in &by_family {
            println!("FAMILY\t{f}\t{r}\t{n}");
        }
        for ((c, m, r), n) in &by_check {
            println!("CHECK\t{c}\t{m}\t{r}\t{n}");
        }
        for l in &lines {
            println!("ROW\t{l}");
        }
        println!("WANTED {want}");
        assert!(seeds > 0, "{seeds} of {want}");
    }

    #[test]
    fn every_start_tier_of_every_family_gives_its_base() {
        let d: Data = Data::load().unwrap();
        for (ids, fam, want) in [
            (vec!["slingshot"], "Slingshot", START_OK),
            (vec!["big_bullet_bag"], "Slingshot", START_OK),
            (vec!["biggest_bullet_bag"], "Slingshot", START_OK),
            (vec!["slingshot", "big_bullet_bag"], "Slingshot", START_OK),
            (vec!["big_quiver"], "Bow", START_OK),
            (vec!["biggest_quiver"], "Bow", START_OK),
            (vec!["big_bomb_bag"], "BombBag", START_OK),
            (vec!["biggest_bomb_bag"], "BombBag", START_OK),
            (vec!["big_nut_bag"], "NutBag", START_OK),
            (vec!["biggest_stick_bag"], "StickBag", START_OK),
            (vec!["silver_gauntlets"], "Strength", START_OK),
            (vec!["longshot"], "Hookshot", START_OK),
            (vec!["golden_gauntlets"], "Strength", START_OK),
            (vec!["golden_scale"], "Scale", START_OK),
            (vec!["giant_wallet"], "Wallet", START_OK),
            (vec!["double_magic"], "Magic", START_OK),
            (vec!["ocarina_of_time"], "Ocarina", START_OK),
        ] {
            let mut s: Settings = Settings::default();
            s.set("starting_items", Value::set(&ids)).unwrap();
            let p: Plan = plan_with_seed(&s, None).unwrap();
            let rows: Vec<Row> = start_rows(&d, &p);
            let r: &Row = rows.iter().find(|r| r.family == fam).unwrap_or_else(|| panic!("{ids:?}"));
            println!("{ids:?}\t{}\t{}\tpouch {:?}", r.tier, r.risk, p.start.pouch);
            assert_eq!(r.risk, want, "{ids:?}");
        }
    }
}
