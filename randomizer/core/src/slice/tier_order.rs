use std::collections::{BTreeMap as BM, BTreeSet as BS};

use crate::error::{Error as E, Res};
use crate::logic_items::ItemId;
use crate::progressive::{needs_every_holder, owners, tiers, FAMILIES};
use crate::search::{explore, Search};
use crate::settings::seed::SeedFile;
use crate::slice::data::Data;
use crate::slice::edit::is_sale;
use crate::world::{LocId, Placement, World};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Give {
    Flow,
    Fixed(usize),
    Open,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Slot {
    pub loc: String,
    pub item: String,
    pub family: &'static str,
    pub give: Give,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Need {
    pub held: usize,
    pub reach: bool,
    pub tight: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Verdict {
    pub assign: BM<String, String>,
    pub bad: Vec<String>,
    pub at: Vec<String>,
}

pub fn covered(d: &Data, placed: &BM<String, String>, f: &str) -> bool {
    !needs_every_holder(f) || d.holders(f).iter().all(|h| placed.contains_key(*h) || d.wrappable(h))
}

pub fn rank(f: &str, actor: &str) -> Option<usize> {
    let t: &[&str] = tiers(f).ok()?;
    let o: &[&str] = owners(f).ok()?;
    t.iter().position(|x| *x == actor).or_else(|| o.iter().position(|x| *x == actor))
}

pub fn native(d: &Data, f: &str, loc: &str) -> Give {
    if let Some(l) = d.flow.by_name(loc) {
        let ks: BS<usize> = l.edits.iter().filter_map(|e| rank(f, &e.item)).collect();
        if ks.len() > 1 {
            return Give::Flow;
        }
        if let Some(k) = l.item.as_deref().and_then(|a| rank(f, a)).or_else(|| ks.first().copied()) {
            return Give::Fixed(k);
        }
    }
    Give::Fixed(d.cat.by_name(loc).and_then(|c| rank(f, &c.item)).unwrap_or(0))
}

pub fn fixed_spot(d: &Data, loc: &str) -> bool {
    crate::pot_slice::targets::is_loc(d, loc)
        || crate::shops::is_loc(loc)
        || crate::slice::skulls::is_skull(d, loc)
        || loc == crate::slice::treasure::LOC
        || d.cat.by_name(loc).is_some_and(|l| crate::slice::pickups::is_world(l) || l.kind == "bossheart")
}

/// How an upgrade of family `f` placed at `loc` is handed out: through the tier wrapper
/// (`Flow`, the next tier at run time) or as one fixed tier, which the seed's reach order must
/// then put in the right order. `placed` is every location the seed fills.
pub fn give_of(d: &Data, placed: &BM<String, String>, loc: &str, item: &str, f: &str) -> Give {
    give_and_wrap(d, placed, loc, item, f).0
}

/// [`give_of`], and whether the spot puts the family's tier wrapper in use.
fn give_and_wrap(d: &Data, placed: &BM<String, String>, loc: &str, item: &str, f: &str) -> (Give, bool) {
    // A shelf deal of an upgrade names and gives the next tier at run time (shelf::hook), like
    // a chest chain.
    let shelf: bool = crate::shops::is_loc(loc) && crate::shops::jobs::tiered_family(d, item).is_some();
    if shelf {
        if covered(d, placed, f) { (Give::Flow, true) } else { (Give::Open, false) }
    } else if fixed_spot(d, loc) || (crate::slice::flow_rules::priced_sale(d, loc) && !crate::scrubs_a::home(d, loc, item)) {
        (Give::Open, false)
    } else if is_sale(d, loc) {
        if crate::scrubs_a::home(d, loc, item) {
            (native(d, f, loc), false)
        } else if d.flow_ok(loc) && covered(d, placed, f) {
            (Give::Flow, true)
        } else {
            (Give::Open, false)
        }
    } else if covered(d, placed, f) {
        (Give::Flow, true)
    } else {
        (Give::Open, false)
    }
}

pub fn slots(d: &Data, placed: &BM<String, String>) -> Vec<Slot> {
    let mut out: Vec<Slot> = Vec::new();
    let mut used: BS<&'static str> = BS::new();
    for (loc, item) in placed {
        let Some(f) = d.family(item) else {
            continue;
        };
        let (give, wraps): (Give, bool) = give_and_wrap(d, placed, loc, item, f);
        if wraps {
            used.insert(f);
        }
        out.push(Slot { loc: loc.clone(), item: item.clone(), family: f, give });
    }
    for (f, _) in FAMILIES.iter().copied() {
        let hs: BS<&str> = d.holders(f).into_iter().filter(|h| !placed.contains_key(*h)).collect();
        for h in hs {
            let give: Give = if d.wrappable(h) && used.contains(f) { Give::Flow } else { native(d, f, h) };
            let item: String = d.vanilla_of(h).unwrap_or("").to_string();
            out.push(Slot { loc: h.to_string(), item, family: f, give });
        }
    }
    out
}

fn held_at_start(w: &World, it: ItemId) -> usize {
    w.start.iter().filter(|x| x.0 == it).map(|x| x.1 as usize).sum()
}

pub fn need(w: &World, place: &Placement, it: ItemId, at: LocId) -> Need {
    let mut p: Placement = place.clone();
    p.item[at] = None;
    let s: Search = explore(w, &p, &[]);
    let others: Vec<LocId> = (0..w.locs.len()).filter(|x| *x != at && place.get(*x) == Some(it) && s.loc_access(*x)).collect();
    let held: usize = held_at_start(w, it) + others.len();
    let reach: bool = s.loc_access(at);
    let tight: bool = if !reach || others.is_empty() {
        true
    } else if !w.items.adv[it] {
        false
    } else {
        others.iter().all(|x| {
            let mut q: Placement = p.clone();
            q.item[*x] = None;
            !explore(w, &q, &[]).loc_access(at)
        })
    };
    Need { held, reach, tight }
}

fn top_rule(d: &Data, f: &str, mine: &[&Slot]) -> bool {
    crate::slice::key_spots::TOP_ONLY.contains(&f) && mine.iter().any(|s| s.give == Give::Open && fixed_spot(d, &s.loc))
}

pub fn judge(d: &Data, w: &World, place: &Placement, placed: &BM<String, String>) -> Res<Verdict> {
    let all: Vec<Slot> = slots(d, placed);
    let mut v: Verdict = Verdict::default();
    let fams: BS<&'static str> = all.iter().filter(|s| s.give == Give::Open).map(|s| s.family).collect();
    for f in fams {
        let t: &[&str] = tiers(f)?;
        let mine: Vec<&Slot> = all.iter().filter(|s| s.family == f).collect();
        if top_rule(d, f, &mine) {
            for s in mine.iter().filter(|s| s.give == Give::Open && fixed_spot(d, &s.loc)) {
                v.assign.insert(s.loc.clone(), t[t.len() - 1].to_string());
            }
            continue;
        }
        let name: &str = mine.iter().find(|s| s.give == Give::Open).map(|s| s.item.as_str()).unwrap_or("");
        let it: ItemId = w.item(name).ok_or_else(|| E::Msg(format!("{name} is not in the logic item list")))?;
        for s in mine.iter().filter(|s| s.give != Give::Flow) {
            if s.give == Give::Open && crate::pickup_tiers::runtime(d, &s.loc, f) {
                v.assign.insert(s.loc.clone(), crate::pickup_tiers::base(f)?.to_string());
                continue;
            }
            let Some(l) = w.loc(&s.loc) else {
                v.bad.push(format!("{} holds {} but is not a logic location", s.loc, s.item));
                continue;
            };
            let n: Need = need(w, place, it, l);
            let want: usize = match s.give {
                Give::Fixed(k) => k,
                _ => n.held.min(t.len() - 1),
            };
            if s.give == Give::Open {
                v.assign.insert(s.loc.clone(), t[want].to_string());
            }
            if !n.reach {
                continue;
            }
            if n.held != want || !n.tight {
                v.bad.push(format!(
                    "{} hands out {} ({name} tier {}) but the player can reach it holding {} other copies{}",
                    s.loc,
                    t[want],
                    want + 1,
                    n.held,
                    if n.tight { "" } else { " or fewer" }
                ));
                v.at.push(s.loc.clone());
            }
        }
    }
    Ok(v)
}

pub fn named(w: &World, place: &Placement, free: &[LocId]) -> BM<String, String> {
    free.iter().filter_map(|l| place.get(*l).map(|i| (w.locs[*l].name.clone(), w.items.names[i].clone()))).collect()
}

pub fn fits(d: &Data, w: &World, place: &Placement, free: &[LocId]) -> Res<bool> {
    let placed: BM<String, String> = named(w, place, free);
    if !slots(d, &placed).iter().any(|s| s.give == Give::Open) {
        return Ok(true);
    }
    Ok(judge(d, w, place, &placed)?.bad.is_empty())
}

pub fn pinned(d: &Data, w: &World, free: &[LocId]) -> Vec<&'static str> {
    let names: BS<&str> = free.iter().map(|l| w.locs[*l].name.as_str()).collect();
    FAMILIES.iter().map(|(f, _)| *f).filter(|f| d.holders(f).iter().any(|h| !names.contains(h))).collect()
}

/// The sphere in which each location is first reached (None: never).
fn spheres(w: &World, place: &Placement) -> Vec<Option<usize>> {
    let mut out: Vec<Option<usize>> = vec![None; w.locs.len()];
    let mut s: Search = Search::new(w);
    let mut n: usize = 0;
    let mut found: Vec<LocId> = s.settle(place, false);
    while !found.is_empty() {
        for l in &found {
            out[*l].get_or_insert(n);
            if let Some(i) = place.get(*l) {
                s.add(i, 1);
            }
        }
        n += 1;
        found = s.settle(place, false);
    }
    out
}

/// The tier a chain chest of an upgrade most likely gives, for the model its chest raises: a
/// chest raises its drop actor (PouchContentGuestPerformer), while the chain grants the next
/// tier at run time. Every holder of the family is ordered by the sphere that reaches it, and
/// the k-th one shows tier k past the copies the player starts with. Found in that order, the
/// model matches the item; the item granted is right either way.
pub fn chest_views(d: &Data, seed: &SeedFile) -> Res<BM<String, String>> {
    let chests: Vec<Slot> = slots(d, &seed.placements).into_iter().filter(|s| s.give == Give::Flow && seed.placements.contains_key(&s.loc) && !is_sale(d, &s.loc) && !fixed_spot(d, &s.loc)).collect();
    let mut out: BM<String, String> = BM::new();
    if chests.is_empty() {
        return Ok(out);
    }
    let (w, place): (World, Placement) = crate::slice::starts::rebuild(d, seed)?;
    let when: Vec<Option<usize>> = spheres(&w, &place);
    let fams: BS<&'static str> = chests.iter().map(|s| s.family).collect();
    for f in fams {
        let t: &[&str] = tiers(f)?;
        let Some(name) = chests.iter().find(|s| s.family == f).map(|s| s.item.as_str()) else { continue };
        let Some(it) = w.item(name) else { continue };
        let mut holders: Vec<(usize, &str)> = (0..w.locs.len()).filter(|l| place.get(*l) == Some(it)).map(|l| (when[l].unwrap_or(usize::MAX), w.locs[l].name.as_str())).collect();
        holders.sort();
        let start: usize = held_at_start(&w, it);
        for s in chests.iter().filter(|s| s.family == f) {
            let Some(k) = holders.iter().position(|h| h.1 == s.loc) else { continue };
            out.insert(s.loc.clone(), t[(start + k).min(t.len() - 1)].to_string());
        }
    }
    Ok(out)
}

pub fn fixed_actors(d: &Data, seed: &SeedFile) -> Res<Verdict> {
    // An Ice Trap gives nothing, so the spot counts as junk here (as it did when the seed was
    // generated), not as the item it looks like: a look that opened a route would let the player
    // reach a fixed spot sooner than the logic allows, and the build would refuse a good seed.
    let plain: SeedFile;
    let seed: &SeedFile = if seed.cold.is_empty() {
        seed
    } else {
        let mut s: SeedFile = seed.clone();
        for loc in &seed.cold {
            s.placements.insert(loc.clone(), crate::frost::fake::RUPEE_ITEM.to_string());
            // A shop deal's entry names the same item as its placement (shops::seed::layout).
            if let Some(e) = s.shops.get_mut(loc).filter(|e| e.deal) {
                e.item = crate::frost::fake::RUPEE_ITEM.to_string();
            }
        }
        plain = s;
        &plain
    };
    if !slots(d, &seed.placements).iter().any(|s| s.give == Give::Open) {
        return Ok(Verdict::default());
    }
    let (w, place): (World, Placement) = crate::slice::starts::rebuild(d, seed)?;
    judge(d, &w, &place, &seed.placements)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{Settings, Value};
    use crate::slice::fill::vanilla_place;
    use crate::slice::generate::{generate_data, GenOpts, Generated};
    use crate::slice::logic::world_with;
    use crate::slice::support::{plan, Plan};

    #[test]
    fn chain_chests_show_the_tiers_in_reach_order() {
        let d: Data = Data::load().unwrap();
        let mut later: usize = 0;
        for n in 0..6 {
            let g: Generated = generate_data(&d, &Settings::default(), &format!("views {n}"), &GenOpts::default()).unwrap();
            let v: BM<String, String> = chest_views(&d, &g.seed_file).unwrap();
            let (w, place): (World, Placement) = crate::slice::starts::rebuild(&d, &g.seed_file).unwrap();
            let when: Vec<Option<usize>> = spheres(&w, &place);
            for (f, _) in FAMILIES.iter().copied() {
                let mut mine: Vec<(Option<usize>, usize)> = v
                    .iter()
                    .filter(|(l, _)| g.seed_file.placements.get(*l).is_some_and(|i| d.family(i) == Some(f)))
                    .map(|(l, a)| (when[w.loc(l).unwrap()], rank(f, a).unwrap()))
                    .collect();
                mine.sort();
                // A chest reached in a later sphere never shows a lower tier than an earlier one.
                assert!(mine.windows(2).all(|x| x[0].1 <= x[1].1 || x[0].0 == x[1].0), "{f} {mine:?}");
                later += mine.iter().filter(|m| m.1 > 0).count();
            }
            for (l, a) in &v {
                let f: &str = d.family(&g.seed_file.placements[l]).unwrap();
                assert!(rank(f, a).is_some(), "{l} {a}");
            }
        }
        assert!(later > 0);
    }

    #[test]
    fn a_viewed_tier_that_sets_off_the_pre_grant_falls_back_to_the_safe_drop() {
        use crate::slice::chest_targets::view_drop;
        assert_eq!(view_drop("Strength", Some("Item_Gauntlet_Silver")).unwrap(), "Item_Gauntlet_Silver");
        assert_eq!(view_drop("Hookshot", Some("Item_Longshot")).unwrap(), "Item_Longshot");
        assert_eq!(view_drop("Hookshot", None).unwrap(), "Item_Hookshot");
        assert_eq!(view_drop("BombBag", Some("Item_BombBag_Small")).unwrap(), "Item_BombBag_Medium");
        assert_eq!(view_drop("Strength", Some("Item_Longshot")).unwrap(), crate::pregrant::chest_drop("Strength").unwrap());
    }

    fn map(pairs: &[(&str, &str)]) -> BM<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    fn give(v: &[Slot], loc: &str) -> Option<Give> {
        v.iter().find(|s| s.loc == loc).map(|s| s.give)
    }

    fn with(pairs: &[(&str, &str)]) -> Settings {
        let mut s: Settings = Settings::default();
        for (k, v) in pairs {
            s.set(k, Value::s(v)).unwrap();
        }
        s
    }

    #[test]
    fn ranks_read_tiers_and_pouch_owners() {
        assert_eq!(rank("Bow", "Item_Range_Bow"), Some(0));
        assert_eq!(rank("Bow", "Weapon_Range_Bow"), Some(0));
        assert_eq!(rank("BombBag", "Item_BombBag_Large"), Some(2));
        assert_eq!(rank("BombBag", "Item_Bomb"), None);
    }

    #[test]
    fn chests_resolve_by_ownership_and_pickup_spots_are_open() {
        let d: Data = Data::load().unwrap();
        let v: Vec<Slot> = slots(&d, &map(&[("KF Midos Top Left Chest", "Bomb Bag"), ("KF Top of Sarias Recovery Heart 1", "Bow")]));
        assert_eq!(give(&v, "KF Midos Top Left Chest"), Some(Give::Flow));
        assert_eq!(give(&v, "KF Top of Sarias Recovery Heart 1"), Some(Give::Open));
        assert_eq!(give(&v, "Water Temple Longshot Chest"), Some(Give::Fixed(1)));
        assert_eq!(give(&v, "Kak 30 Gold Skulltula Reward"), Some(Give::Fixed(1)));
    }

    #[test]
    fn an_uncovered_hookshot_copy_is_open_and_its_partner_chest_stays_fixed() {
        let d: Data = Data::load().unwrap();
        let v: Vec<Slot> = slots(&d, &map(&[("KF Midos Top Left Chest", "Progressive Hookshot"), ("Graveyard Dampe Race Hookshot Chest", "Rupees (5)")]));
        assert_eq!(give(&v, "KF Midos Top Left Chest"), Some(Give::Open));
        assert_eq!(give(&v, "Water Temple Longshot Chest"), Some(Give::Fixed(1)));
        let half: Vec<Slot> = slots(&d, &map(&[("KF Midos Top Left Chest", "Progressive Hookshot"), ("Water Temple Longshot Chest", "Rupees (5)")]));
        assert_eq!(give(&half, "KF Midos Top Left Chest"), Some(Give::Open));
        assert_eq!(give(&half, "Graveyard Dampe Race Hookshot Chest"), Some(Give::Fixed(0)));
        let both: Vec<Slot> = slots(
            &d,
            &map(&[
                ("KF Midos Top Left Chest", "Progressive Hookshot"),
                ("Water Temple Longshot Chest", "Progressive Hookshot"),
                ("Graveyard Dampe Race Hookshot Chest", "Rupees (5)"),
            ]),
        );
        assert!(both.iter().filter(|s| s.family == "Hookshot").all(|s| s.give == Give::Flow), "{both:?}");
    }

    fn world_of(d: &Data, s: &Settings) -> (World, Placement) {
        let p: Plan = plan(s).unwrap();
        let w: World = world_with(d, &p.logic()).unwrap();
        let place: Placement = vanilla_place(&w, d).unwrap();
        (w, place)
    }

    #[test]
    fn an_early_copy_gets_the_first_tier_and_the_water_chest_agrees() {
        let d: Data = Data::load().unwrap();
        let s: Settings = with(&[("shuffle_chests", "overworld")]);
        let (w, mut place): (World, Placement) = world_of(&d, &s);
        let hook: ItemId = w.item("Progressive Hookshot").unwrap();
        let five: ItemId = w.item("Rupees (5)").unwrap();
        place.put(w.loc("Graveyard Dampe Race Hookshot Chest").unwrap(), five);
        place.put(w.loc("KF Midos Top Left Chest").unwrap(), hook);
        let placed: BM<String, String> = map(&[("KF Midos Top Left Chest", "Progressive Hookshot"), ("Graveyard Dampe Race Hookshot Chest", "Rupees (5)")]);
        let v: Verdict = judge(&d, &w, &place, &placed).unwrap();
        assert!(v.bad.is_empty(), "{:?}", v.bad);
        assert_eq!(v.assign.get("KF Midos Top Left Chest").map(|s| s.as_str()), Some("Item_Hookshot"));
        let n: Need = need(&w, &place, hook, w.loc("Water Temple Longshot Chest").unwrap());
        assert_eq!((n.held, n.tight), (1, true));
    }

    /// A world whose only Slingshot stands at `at` (the Deku Tree chest holds Rupees (5)).
    fn lone_slingshot(d: &Data, s: &Settings, at: &str) -> (World, Placement) {
        let (w, mut place): (World, Placement) = world_of(d, s);
        let sling: ItemId = w.item("Slingshot").unwrap();
        let five: ItemId = w.item("Rupees (5)").unwrap();
        let alone: Vec<LocId> = (0..w.locs.len()).filter(|l| place.get(*l) == Some(sling)).collect();
        for l in &alone {
            place.put(*l, five);
        }
        place.put(w.loc(at).unwrap(), sling);
        (w, place)
    }

    #[test]
    fn a_shop_deal_of_an_upgrade_gives_the_next_tier_at_run_time_like_a_chest_chain() {
        let d: Data = Data::load().unwrap();
        let s: Settings = with(&[("shuffle_chests", "overworld"), ("shuffle_shops", "deals_2")]);
        // Every upgrade family sells on a shelf, tier by tier (shelf::hook), so the reach order fixes
        // no tier there, however early another copy is found.
        assert_eq!(crate::shops::jobs::tiered_family(&d, "Slingshot"), Some("Slingshot"));
        let (w, mut place): (World, Placement) = lone_slingshot(&d, &s, "KF Shop Right Side Bottom Left");
        let placed: BM<String, String> = map(&[("KF Shop Right Side Bottom Left", "Slingshot"), ("Deku Tree Slingshot Chest", "Rupees (5)")]);
        assert_eq!(give(&slots(&d, &placed), "KF Shop Right Side Bottom Left"), Some(Give::Flow));
        let v: Verdict = judge(&d, &w, &place, &placed).unwrap();
        assert!(!v.assign.contains_key("KF Shop Right Side Bottom Left"), "{:?}", v.assign);
        assert!(!v.bad.iter().any(|b| b.starts_with("KF Shop Right Side Bottom Left")), "{:?}", v.bad);
        place.put(w.loc("KF Midos Top Left Chest").unwrap(), w.item("Slingshot").unwrap());
        let early: BM<String, String> = map(&[("KF Shop Right Side Bottom Left", "Slingshot"), ("KF Midos Top Left Chest", "Slingshot"), ("Deku Tree Slingshot Chest", "Rupees (5)")]);
        assert_eq!(give(&slots(&d, &early), "KF Shop Right Side Bottom Left"), Some(Give::Flow));
        let v: Verdict = judge(&d, &w, &place, &early).unwrap();
        assert!(!v.assign.contains_key("KF Shop Right Side Bottom Left"), "{:?}", v.assign);
        assert!(!v.bad.iter().any(|b| b.starts_with("KF Shop Right Side Bottom Left")), "{:?}", v.bad);
    }

    #[test]
    fn a_priced_sale_gets_the_tier_of_the_reach_order_and_not_the_top_tier() {
        let d: Data = Data::load().unwrap();
        let s: Settings = with(&[("shuffle_chests", "overworld")]);
        // A scrub sale stays one plain call (crate::slice::flow_rules::priced_sale), so an upgrade
        // there sells the tier the copies reachable before it fix.
        let sale: &str = "HF Deku Scrub Grotto";
        assert!(crate::slice::flow_rules::priced_sale(&d, sale));
        let (w, mut place): (World, Placement) = lone_slingshot(&d, &s, sale);
        let placed: BM<String, String> = map(&[(sale, "Slingshot"), ("Deku Tree Slingshot Chest", "Rupees (5)")]);
        assert_eq!(give(&slots(&d, &placed), sale), Some(Give::Open));
        let v: Verdict = judge(&d, &w, &place, &placed).unwrap();
        assert_eq!(v.assign.get(sale).map(|s| s.as_str()), Some("Item_Range_SlingShot"));
        assert!(!v.bad.iter().any(|b| b.starts_with(sale)), "{:?}", v.bad);
        place.put(w.loc("KF Midos Top Left Chest").unwrap(), w.item("Slingshot").unwrap());
        let early: BM<String, String> = map(&[(sale, "Slingshot"), ("KF Midos Top Left Chest", "Slingshot"), ("Deku Tree Slingshot Chest", "Rupees (5)")]);
        let v: Verdict = judge(&d, &w, &place, &early).unwrap();
        assert_eq!(v.assign.get(sale).map(|s| s.as_str()), Some("Item_BulletBag_Medium"));
    }

    #[test]
    fn an_early_longshot_beside_the_vanilla_dampe_hookshot_is_refused() {
        let d: Data = Data::load().unwrap();
        let s: Settings = with(&[("shuffle_chests", "dungeons")]);
        let (w, mut place): (World, Placement) = world_of(&d, &s);
        let hook: ItemId = w.item("Progressive Hookshot").unwrap();
        let five: ItemId = w.item("Rupees (5)").unwrap();
        place.put(w.loc("Water Temple Longshot Chest").unwrap(), five);
        place.put(w.loc("Deku Tree Map Chest").unwrap(), hook);
        let placed: BM<String, String> = map(&[("Deku Tree Map Chest", "Progressive Hookshot"), ("Water Temple Longshot Chest", "Rupees (5)")]);
        let v: Verdict = judge(&d, &w, &place, &placed).unwrap();
        assert_eq!(v.assign.get("Deku Tree Map Chest").map(|s| s.as_str()), Some("Item_Longshot"));
        assert!(v.bad.iter().any(|b| b.starts_with("Graveyard Dampe Race Hookshot Chest")), "{:?}", v.bad);
        assert!(v.bad.iter().any(|b| b.starts_with("Deku Tree Map Chest")), "{:?}", v.bad);
        let p: Plan = plan(&s).unwrap();
        let free: Vec<LocId> = crate::slice::pick::pick(&d, &w, &p).free;
        assert!(!fits(&d, &w, &place, &free).unwrap());
    }

    #[test]
    fn a_family_with_a_holder_left_out_of_the_pool_is_pinned() {
        let d: Data = Data::load().unwrap();
        let s: Settings = with(&[("shuffle_chests", "all")]);
        let (w, _): (World, Placement) = world_of(&d, &s);
        let free: Vec<LocId> = crate::slice::pick::pick(&d, &w, &plan(&s).unwrap()).free;
        let f: Vec<&str> = pinned(&d, &w, &free);
        // The Ocarina of Time scene keeps its Ocarina, so that family is pinned. Darunia's Joy is
        // shuffled now, so every Strength holder is in the pool.
        assert!(f.contains(&"Ocarina"), "{f:?}");
        assert!(!f.contains(&"Strength") && !f.contains(&"Hookshot"), "{f:?}");
        assert!(pinned(&d, &w, &(0..w.locs.len()).collect::<Vec<LocId>>()).is_empty());
    }

    #[test]
    fn fifty_seeds_keep_every_fixed_tier_in_reach_order() {
        let d: Data = Data::load().unwrap();
        let mut open: usize = 0;
        let mut seeds: usize = 0;
        for (chests, rewards, k) in [("overworld", "off", 15), ("dungeons", "off", 25), ("all", "off", 8), ("dungeons", "supported", 2)] {
            let s: Settings = with(&[("shuffle_chests", chests), ("shuffle_rewards", rewards)]);
            for n in 0..k {
                let g: Generated = generate_data(&d, &s, &format!("tier order {chests} {rewards} {n}"), &GenOpts::default()).unwrap();
                seeds += 1;
                let v: Verdict = fixed_actors(&d, &g.seed_file).unwrap();
                assert!(v.bad.is_empty(), "{chests} {rewards} {n}: {:?}", v.bad);
                let sl: Vec<Slot> = slots(&d, &g.seed_file.placements);
                open += sl.iter().filter(|x| x.give == Give::Open).count();
                for x in sl.iter().filter(|x| x.give == Give::Open) {
                    assert!(v.assign.contains_key(&x.loc), "{}", x.loc);
                }
            }
        }
        assert_eq!(seeds, 50);
        assert!(open > 0, "no seed had a fixed upgrade slot, the test proves nothing");
    }
    #[test]
    fn ice_trap_looks_do_not_count_as_items_in_the_build_check() {
        // Races preset seed "p7" (2026-11-27): the fill kept every tier in order, but the build
        // read its Ice Traps as the items they look like (Hover Boots, Din's Fire, ...), reached
        // Sheik in Forest too soon and refused the seed. The traps count as junk now.
        let d: Data = Data::load().unwrap();
        let races = crate::settings::presets::builtin().unwrap().into_iter().find(|p| p.id == "races").unwrap();
        let s: Settings = races.apply(&Settings::default()).unwrap();
        let g: Generated = generate_data(&d, &s, "p7", &GenOpts { extras: true, ..GenOpts::default() }).unwrap();
        let (prep, traps) = crate::frost::prepare(&g.seed_file).unwrap();
        assert!(!traps.is_empty() && prep.cold.len() == traps.len());
        assert!(fixed_actors(&d, &g.seed_file).unwrap().bad.is_empty());
        let v: Verdict = fixed_actors(&d, &prep).unwrap();
        assert!(v.bad.is_empty(), "{:?}", v.bad);
    }
}

