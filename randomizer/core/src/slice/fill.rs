use std::collections::HashMap as HM;

use crate::error::{Error as E, Res};
use crate::logic_items::ItemId;
use crate::rng::Rng;
use crate::search::{explore, Search};
use crate::slice::data::Data;
use crate::slice::fit::{Fit, Grid, Short};
use crate::slice::watch::Watch;
use crate::world::{LocId, Placement, World};

pub fn vanilla_place(w: &World, d: &Data) -> Res<Placement> {
    let mut p: Placement = Placement::new(w.locs.len());
    for l in &w.real {
        let Some(name) = d.vanilla_of(&w.locs[*l].name) else {
            continue;
        };
        let i: ItemId = w
            .item(name)
            .ok_or_else(|| E::Msg(format!("vanilla item {name} of {} is not in the item list", w.locs[*l].name)))?;
        p.put(*l, i);
    }
    Ok(p)
}

pub const WINS: u32 = 16;
pub const RETRY: u32 = 8;
pub const SWITCH: u32 = 500;
pub const SHORT: &str = "the shuffled items cannot all fit the shuffled locations";

struct Job<'a> {
    w: &'a World,
    fixed: &'a Placement,
    free: &'a [LocId],
    pool: &'a [ItemId],
    g: &'a Grid,
    at: &'a [usize],
    kind: &'a [usize],
    keys: &'a HM<ItemId, usize>,
    rank: Option<&'a [usize]>,
}

fn key(j: &Job, i: ItemId) -> usize {
    j.keys.get(&i).copied().unwrap_or(0)
}

fn fits(j: &Job, i: ItemId, l: LocId) -> bool {
    j.g.ok[j.kind[i]][j.at[l]]
}

fn once(j: &Job, rng: &mut Rng) -> Res<Placement> {
    let w: &World = j.w;
    let mut place: Placement = j.fixed.clone();
    let mut items: Vec<ItemId> = j.pool.to_vec();
    rng.shuffle(&mut items);
    items.sort_by_key(|i| key(j, *i));
    let prog: Vec<ItemId> = items.iter().copied().filter(|i| w.items.adv[*i]).collect();
    let mut open: Vec<LocId> = j.free.to_vec();
    let mut base: Search = Search::new(w);
    base.settle(&place, true);
    let mut assumed: Search = base.clone();
    assumed.add_all(&prog);
    for it in items {
        rng.shuffle(&mut open);
        if let Some(r) = j.rank.filter(|_| !w.items.adv[it]) {
            open.sort_by_key(|l| r[*l]);
        }
        let pick: Option<usize> = if w.items.adv[it] {
            assumed.remove(it);
            let mut mx: Search = assumed.clone();
            mx.settle(&place, true);
            open.iter().position(|l| fits(j, it, *l) && mx.loc_access(*l))
        } else {
            open.iter().position(|l| fits(j, it, *l))
        };
        match pick {
            Some(k) => {
                let l: LocId = open.swap_remove(k);
                place.put(l, it);
            }
            None => {
                return Err(E::Msg(format!(
                    "no spot for {} among {} free locations",
                    w.items.names[it],
                    open.len()
                )));
            }
        }
    }
    if !explore(w, &place, &[]).won() {
        return Err(E::Format("the game is not beatable after the fill"));
    }
    Ok(place)
}

fn once_fit(j: &Job, g: &Grid, rng: &mut Rng) -> Res<Placement> {
    let w: &World = j.w;
    let mut f: Fit = Fit::new(g);
    if !f.full() {
        return Err(E::Msg(SHORT.to_string()));
    }
    let mut place: Placement = j.fixed.clone();
    let mut items: Vec<usize> = (0..j.pool.len()).collect();
    rng.shuffle(&mut items);
    items.sort_by_key(|p| key(j, j.pool[*p]));
    let prog: Vec<ItemId> = items.iter().map(|p| j.pool[*p]).filter(|i| w.items.adv[*i]).collect();
    let mut open: Vec<usize> = (0..j.free.len()).collect();
    let mut base: Search = Search::new(w);
    base.settle(&place, true);
    let mut assumed: Search = base.clone();
    assumed.add_all(&prog);
    for p in items {
        let it: ItemId = j.pool[p];
        rng.shuffle(&mut open);
        if let Some(r) = j.rank.filter(|_| !w.items.adv[it]) {
            open.sort_by_key(|x| r[j.free[*x]]);
        }
        let mx: Option<Search> = if w.items.adv[it] {
            assumed.remove(it);
            let mut m: Search = assumed.clone();
            m.settle(&place, true);
            Some(m)
        } else {
            None
        };
        let mut pick: Option<usize> = None;
        for (k, x) in open.iter().enumerate() {
            if !g.fits(p, *x) || mx.as_ref().is_some_and(|m| !m.loc_access(j.free[*x])) {
                continue;
            }
            if f.take(p, *x) {
                pick = Some(k);
                break;
            }
        }
        match pick {
            Some(k) => {
                let x: usize = open.swap_remove(k);
                place.put(j.free[x], it);
            }
            None => {
                return Err(E::Msg(format!(
                    "no spot for {} among {} free locations",
                    w.items.names[it],
                    open.len()
                )));
            }
        }
    }
    if !explore(w, &place, &[]).won() {
        return Err(E::Format("the game is not beatable after the fill"));
    }
    Ok(place)
}

pub fn changed(fixed: &Placement, got: &Placement, free: &[LocId]) -> usize {
    free.iter().filter(|l| fixed.get(**l) != got.get(**l)).count()
}

pub fn short_text(s: &Short) -> String {
    let names: Vec<&str> = s.kinds.iter().take(4).map(|k| k.as_str()).collect();
    let more: &str = if s.kinds.len() > 4 { " and others" } else { "" };
    format!(
        "{SHORT}: {} items ({}{more}) can only go to {} free locations",
        s.items.len(),
        names.join(", "),
        s.spots
    )
}

pub fn fill(
    w: &World,
    vanilla: &Placement,
    free: &[LocId],
    allow: &dyn Fn(ItemId, LocId) -> bool,
    rng: &mut Rng,
    tries: u32,
    plan: &[(LocId, ItemId)],
    check_vanilla: bool,
) -> Res<Placement> {
    fill_ranked(w, vanilla, free, allow, rng, tries, plan, check_vanilla, None)
}

pub fn fill_ranked(
    w: &World,
    vanilla: &Placement,
    free: &[LocId],
    allow: &dyn Fn(ItemId, LocId) -> bool,
    rng: &mut Rng,
    tries: u32,
    plan: &[(LocId, ItemId)],
    check_vanilla: bool,
    rank: Option<&dyn Fn(LocId) -> usize>,
) -> Res<Placement> {
    fill_ordered(w, vanilla, free, allow, None, rng, tries, plan, check_vanilla, rank, false, SWITCH, &Watch::idle())
}

fn reach_keys(w: &World, fixed: &Placement, open: &[LocId], pool: &[ItemId], kinds: &[ItemId], allow: &dyn Fn(ItemId, LocId) -> bool, keys: &mut HM<ItemId, usize>) {
    let prog: Vec<ItemId> = pool.iter().copied().filter(|i| w.items.adv[*i]).collect();
    let mut base: Search = Search::new(w);
    base.settle(fixed, true);
    for i in kinds.iter().filter(|i| w.items.adv[**i]) {
        let mut s: Search = base.clone();
        let mut skip: bool = true;
        for x in &prog {
            if skip && x == i {
                skip = false;
                continue;
            }
            s.add(*x, 1);
        }
        s.settle(fixed, true);
        let n: usize = open.iter().filter(|l| allow(*i, **l) && s.loc_access(**l)).count();
        keys.insert(*i, n);
    }
}

pub fn fill_ordered(
    w: &World,
    vanilla: &Placement,
    free: &[LocId],
    allow: &dyn Fn(ItemId, LocId) -> bool,
    order: Option<&dyn Fn(ItemId, LocId) -> bool>,
    rng: &mut Rng,
    tries: u32,
    plan: &[(LocId, ItemId)],
    check_vanilla: bool,
    rank: Option<&dyn Fn(LocId) -> usize>,
    reach: bool,
    switch: u32,
    watch: &Watch,
) -> Res<Placement> {
    let mut fixed: Placement = vanilla.clone();
    let mut pool: Vec<ItemId> = Vec::with_capacity(free.len());
    for l in free {
        if let Some(i) = fixed.get(*l) {
            pool.push(i);
        }
        fixed.item[*l] = None;
    }
    for (l, i) in plan {
        let at: usize = pool
            .iter()
            .position(|x| x == i)
            .ok_or_else(|| E::Msg(format!("plando: {} has no copy left to place at {}", w.items.names[*i], w.locs[*l].name)))?;
        pool.swap_remove(at);
        fixed.put(*l, *i);
    }
    let open: Vec<LocId> = free.iter().copied().filter(|l| !plan.iter().any(|(p, _)| p == l)).collect();
    if check_vanilla && !explore(w, vanilla, &[]).won() {
        return Err(E::Format("the vanilla placement does not reach the goal under this logic"));
    }
    if !plan.is_empty() && !explore(w, &fixed, &pool).won() {
        return Err(E::Format("plando: the seed cannot be beaten with these fixed placements"));
    }
    let g: Grid = Grid::new(&pool, &open, allow);
    if let Some(s) = Fit::new(&g).short(w) {
        return Err(E::Msg(short_text(&s)));
    }
    let sorter: &dyn Fn(ItemId, LocId) -> bool = order.unwrap_or(allow);
    let mut keys: HM<ItemId, usize> = HM::new();
    for i in &g.kinds {
        keys.insert(*i, open.iter().filter(|l| sorter(*i, **l)).count());
    }
    if reach {
        reach_keys(w, &fixed, &open, &pool, &g.kinds, sorter, &mut keys);
    }
    let mut at: Vec<usize> = vec![usize::MAX; w.locs.len()];
    for (k, l) in open.iter().enumerate() {
        at[*l] = k;
    }
    let mut kind: Vec<usize> = vec![usize::MAX; w.items.len()];
    for (k, i) in g.kinds.iter().enumerate() {
        kind[*i] = k;
    }
    let rk: Option<Vec<usize>> = rank.map(|r| {
        let mut v: Vec<usize> = vec![0; w.locs.len()];
        for l in &open {
            v[*l] = r(*l);
        }
        v
    });
    let j: Job = Job { w, fixed: &fixed, free: &open, pool: &pool, g: &g, at: &at, kind: &kind, keys: &keys, rank: rk.as_deref() };
    let mut won: Vec<Placement> = Vec::new();
    let mut last: E = E::Format("no fill attempts were made");
    let base: u32 = tries.max(1);
    for n in 0..base.saturating_mul(RETRY) {
        if n >= base && !won.is_empty() {
            break;
        }
        watch.tick()?;
        let fall: bool = (n >= switch || watch.fell()) && won.is_empty();
        if fall {
            watch.falling_back();
        }
        let r: Res<Placement> = if fall { once_fit(&j, &g, rng) } else { once(&j, rng) };
        match r {
            Ok(p) => {
                won.push(p);
                if fall || won.len() >= WINS as usize {
                    break;
                }
            }
            Err(e) => last = e,
        }
    }
    if won.is_empty() {
        if plan.is_empty() {
            return Err(last);
        }
        return Err(E::Msg(format!("plando: no seed could be made with these fixed placements, last problem: {last}")));
    }
    let moved: Vec<usize> = (0..won.len()).filter(|i| changed(vanilla, &won[*i], free) > 0).collect();
    let k: usize = if moved.is_empty() {
        rng.below(won.len() as u64) as usize
    } else {
        moved[rng.below(moved.len() as u64) as usize]
    };
    Ok(won.swap_remove(k))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Value;
    use crate::slice::logic::{world, world_with};

    #[test]
    fn the_vanilla_game_reaches_the_goal() {
        let d: Data = Data::load().unwrap();
        let w: World = world(&d).unwrap();
        let p: Placement = vanilla_place(&w, &d).unwrap();
        assert!(explore(&w, &p, &[]).won());
    }

    #[test]
    fn no_small_key_is_given_for_free() {
        let d: Data = Data::load().unwrap();
        let w: World = world(&d).unwrap();
        assert!(w.start.iter().all(|(i, _)| !w.items.names[*i].starts_with("Small Key")));
    }

    #[test]
    fn removing_a_ring_removes_all_of_its_keys() {
        let d: Data = Data::load().unwrap();
        let mut s: crate::settings::Settings = crate::settings::Settings::default();
        s.set(crate::key_rings::SETTING, Value::Bool(true)).unwrap();
        s.set("shuffle_smallkeys", Value::s("keysanity")).unwrap();
        let p: crate::slice::support::Plan = crate::slice::support::plan(&s).unwrap();
        let w: World = world_with(&d, &p.logic()).unwrap();
        for r in crate::key_rings::RINGS {
            let ring: crate::logic_items::ItemId = w.item(r.item).unwrap();
            let key: crate::logic_items::ItemId = w.item(r.key).unwrap();
            let mut search: crate::search::Search = crate::search::Search::new(&w);
            search.add(ring, 1);
            assert_eq!(search.cnt[key], r.count, "{}", r.item);
            search.remove(ring);
            assert_eq!(search.cnt[key], 0, "{}", r.item);
        }
    }

    #[test]
    fn each_vanilla_key_layout_needs_its_own_slack() {
        let d: Data = Data::load().unwrap();
        for k in ["fire_key_layout", "water_key_layout", "spirit_key_layout"] {
            let w: World = world_with(&d, &[(k, "shuffled")]).unwrap();
            let p: Placement = vanilla_place(&w, &d).unwrap();
            assert!(!explore(&w, &p, &[]).won(), "{k}");
        }
    }
}
