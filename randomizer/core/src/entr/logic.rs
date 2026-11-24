use std::collections::HashMap as HM;

use crate::entr::data::{load, Cat};
use crate::entr::layout::{ages_fit, Layout};
use crate::entr::mode::Mode;
use crate::entr::pools::{find, DUNGEONS};
use crate::error::{Error as E, Res};
use crate::rng::Rng;
use crate::search::Search;
use crate::logic_items::ItemId;
use crate::world::{ExitId, LocId, Placement, RegionId, World};

pub const STEPS: u32 = 600;

#[derive(Debug, Clone)]
pub struct Wires {
    pub entry: Vec<ExitId>,
    pub exit: Vec<ExitId>,
    pub inner: Vec<RegionId>,
    pub outer: Vec<RegionId>,
}

fn exit_of(ix: &HM<String, ExitId>, from: &str, to: &str) -> Res<ExitId> {
    ix.get(&format!("{from} -> {to}")).copied().ok_or_else(|| E::Msg(format!("the logic has no exit {from} -> {to}")))
}

pub fn wires(w: &World) -> Res<Wires> {
    let ix: HM<String, ExitId> = w.exits.iter().enumerate().map(|(i, x)| (x.name.clone(), i)).collect();
    let mut out: Wires = Wires { entry: Vec::new(), exit: Vec::new(), inner: Vec::new(), outer: Vec::new() };
    for c in DUNGEONS {
        out.entry.push(exit_of(&ix, c.entry.0, c.entry.1)?);
        out.exit.push(exit_of(&ix, c.exit.0, c.exit.1)?);
        out.inner.push(w.region(c.entry.1).ok_or_else(|| E::Msg(format!("the logic has no region {}", c.entry.1)))?);
        out.outer.push(w.region(c.exit.1).ok_or_else(|| E::Msg(format!("the logic has no region {}", c.exit.1)))?);
    }
    Ok(out)
}

pub fn wire(w: &mut World, x: &Wires, l: &Layout) {
    for (spot, d) in l.at.iter().enumerate() {
        w.exits[x.entry[spot]].to = x.inner[*d];
        w.exits[x.exit[*d]].to = x.outer[spot];
    }
}

pub fn apply(w: &mut World, l: &Layout) -> Res<()> {
    l.check()?;
    let x: Wires = wires(w)?;
    wire(w, &x, l);
    Ok(())
}

#[derive(Debug, Clone)]
pub struct Have {
    pub fixed: Placement,
    pub prog: Vec<ItemId>,
    pub keys: Vec<(ItemId, String)>,
    pub free: Vec<LocId>,
}

fn key_home(name: &str) -> Option<String> {
    let rest: &str = name.strip_prefix("Boss Key (").or_else(|| name.strip_prefix("Small Key ("))?;
    rest.strip_suffix(')').map(|d| d.to_string())
}

pub fn have(w: &World, vanilla: &Placement, free: &[LocId]) -> Have {
    let mut fixed: Placement = vanilla.clone();
    let mut prog: Vec<ItemId> = Vec::new();
    for l in free {
        if let Some(i) = fixed.get(*l)
            && w.items.adv[i]
        {
            prog.push(i);
        }
        fixed.item[*l] = None;
    }
    let mut keys: Vec<(ItemId, String)> = Vec::new();
    for i in &prog {
        if let Some(d) = key_home(&w.items.names[*i])
            && !keys.iter().any(|k| k.0 == *i)
        {
            keys.push((*i, d));
        }
    }
    Have { fixed, prog, keys, free: free.to_vec() }
}

pub fn reach(w: &World, h: &Have) -> (bool, Vec<bool>) {
    let mut s: Search = Search::new(w);
    s.settle(&h.fixed, true);
    s.add_all(&h.prog);
    s.settle(&h.fixed, true);
    let got: Vec<bool> = w.real.iter().map(|l| s.loc_access(*l)).collect();
    (s.won(), got)
}

pub fn keeps(base: &[bool], got: &[bool]) -> bool {
    base.iter().zip(got.iter()).all(|(b, g)| !*b || *g)
}

pub fn keys_fit(w: &World, h: &Have) -> bool {
    for (k, home) in &h.keys {
        if find(home).is_none() {
            continue;
        }
        let mut prog: Vec<ItemId> = h.prog.clone();
        if let Some(at) = prog.iter().position(|x| x == k) {
            prog.swap_remove(at);
        }
        let mut s: Search = Search::new(w);
        s.settle(&h.fixed, true);
        s.add_all(&prog);
        s.settle(&h.fixed, true);
        let open: bool = h.free.iter().any(|l| w.regions[w.locs[*l].region].dungeon.as_deref() == Some(home.as_str()) && s.loc_access(*l));
        if !open {
            return false;
        }
    }
    true
}

pub fn pick(w: &mut World, m: Mode, seed: &str, h: &Have) -> Res<Layout> {
    if m != Mode::Dungeons {
        return Err(E::Msg(format!("entrance mode {} has no pool in this build", m.id())));
    }
    let cat: Cat = load()?;
    let x: Wires = wires(w)?;
    let (won, base): (bool, Vec<bool>) = reach(w, h);
    if !won {
        return Err(E::Format("the logic cannot reach the goal with the shuffled items even before the entrances move"));
    }
    let mut r: Rng = Rng::from_text(&format!("{seed}/entrances"));
    let n: u64 = DUNGEONS.len() as u64;
    let mut cur: Layout = Layout::vanilla();
    let mut tried: HM<Vec<usize>, bool> = HM::new();
    for _ in 0..STEPS {
        let i: usize = r.below(n) as usize;
        let k: usize = r.below(n - 1) as usize;
        let j: usize = if k >= i { k + 1 } else { k };
        let mut next: Layout = cur.clone();
        next.at.swap(i, j);
        let good: bool = match tried.get(&next.at) {
            Some(g) => *g,
            None => {
                let g: bool = ages_fit(&cat, &next)?.is_empty() && {
                    wire(w, &x, &next);
                    let (ok, got): (bool, Vec<bool>) = reach(w, h);
                    ok && keeps(&base, &got) && keys_fit(w, h)
                };
                tried.insert(next.at.clone(), g);
                g
            }
        };
        if good {
            cur = next;
        }
    }
    wire(w, &x, &cur);
    if cur.moved() == 0 {
        return Err(E::Msg(format!("no dungeon entrance could move in {STEPS} tries under these settings, try another seed text")));
    }
    Ok(cur)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entr::pools::find;
    use crate::slice::data::Data;
    use crate::slice::fill::vanilla_place;
    use crate::slice::logic::world;

    fn all(d: &Data, w: &World) -> Have {
        let p: crate::slice::support::Plan = crate::slice::support::plan(&crate::settings::Settings::default()).unwrap();
        let pk: crate::slice::pick::Pick = crate::slice::pick::pick(d, w, &p);
        have(w, &vanilla_place(w, d).unwrap(), &pk.free)
    }

    #[test]
    fn wiring_moves_both_directions_and_vanilla_puts_them_back() {
        let d: Data = Data::load().unwrap();
        let mut w: World = world(&d).unwrap();
        let x: Wires = wires(&w).unwrap();
        let deku: usize = find("Deku Tree").unwrap();
        let fire: usize = find("Fire Temple").unwrap();
        let mut l: Layout = Layout::vanilla();
        l.at.swap(deku, fire);
        wire(&mut w, &x, &l);
        assert_eq!(w.regions[w.exits[x.entry[deku]].to].name, "Fire Temple Lower");
        assert_eq!(w.regions[w.exits[x.exit[fire]].to].name, "KF Outside Deku Tree");
        assert_eq!(w.regions[w.exits[x.entry[fire]].to].name, "Deku Tree Lobby");
        assert_eq!(w.regions[w.exits[x.exit[deku]].to].name, "DMC Fire Temple Entrance");
        wire(&mut w, &x, &Layout::vanilla());
        assert_eq!(w.regions[w.exits[x.entry[deku]].to].name, "Deku Tree Lobby");
        assert_eq!(w.regions[w.exits[x.exit[fire]].to].name, "DMC Fire Temple Entrance");
    }

    #[test]
    fn picked_layouts_keep_every_location_and_the_goal() {
        let d: Data = Data::load().unwrap();
        let w0: World = world(&d).unwrap();
        let h: Have = all(&d, &w0);
        let (_, base): (bool, Vec<bool>) = reach(&w0, &h);
        let mut moved: usize = 0;
        for n in 0..6 {
            let mut w: World = world(&d).unwrap();
            let l: Layout = pick(&mut w, Mode::Dungeons, &format!("pick {n}"), &h).unwrap();
            let (won, got): (bool, Vec<bool>) = reach(&w, &h);
            assert!(won && keeps(&base, &got), "{n}");
            assert!(ages_fit(&load().unwrap(), &l).unwrap().is_empty());
            moved += l.moved();
            let mut v: World = world(&d).unwrap();
            apply(&mut v, &l).unwrap();
            for (a, b) in v.exits.iter().zip(w.exits.iter()) {
                assert_eq!(a.to, b.to);
            }
        }
        assert!(moved > 6);
    }

    #[test]
    fn a_child_only_door_cannot_hold_an_adult_dungeon() {
        let d: Data = Data::load().unwrap();
        let mut w: World = world(&d).unwrap();
        let h: Have = all(&d, &w);
        let (_, base): (bool, Vec<bool>) = reach(&w, &h);
        let mut l: Layout = Layout::vanilla();
        l.at.swap(find("Jabu Jabus Belly").unwrap(), find("Fire Temple").unwrap());
        apply(&mut w, &l).unwrap();
        let (_, got): (bool, Vec<bool>) = reach(&w, &h);
        assert!(!keeps(&base, &got));
    }
}
