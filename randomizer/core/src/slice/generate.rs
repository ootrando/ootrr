use std::cell::Cell;
use std::collections::{BTreeMap as BM, HashMap as HM, HashSet as HS};
use std::fmt::Write as W;
use std::sync::Arc;
use std::time::Duration;

use crate::error::{Error as E, Res};
use crate::logic_items::ItemId;
use crate::rng::Rng;
use crate::settings::Settings;
use crate::settings::seed::SeedFile;
use crate::slice::data::Data;
use crate::slice::fill::{changed, fill_ordered, vanilla_place, SWITCH};
use crate::slice::logic::world_with;
use crate::slice::pick::{pick, scoped, Pick};
use crate::slice::pickups as pu;
use crate::slice::plando::{loc_of, resolve, Entry};
use crate::slice::support::{plan, Keep, Plan, Songs};
use crate::slice::watch::{Live, Watch};
use crate::spoiler::{playthrough, Spoiler};
use crate::world::{LocId, Placement, World};

pub const TRIES: u32 = 600;
pub const TIER_TRIES: u32 = 40;
pub const LIMIT: Duration = Duration::from_secs(90);

#[derive(Debug, Clone)]
pub struct GenOpts {
    pub spoiler: bool,
    pub tries: u32,
    pub plando: Vec<Entry>,
    pub limit: Option<Duration>,
    pub live: Option<Arc<Live>>,
    pub switch: u32,
}

impl Default for GenOpts {
    fn default() -> GenOpts {
        GenOpts { spoiler: false, tries: TRIES, plando: Vec::new(), limit: None, live: None, switch: SWITCH }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Counts {
    pub candidates: usize,
    pub randomized: usize,
    pub left_vanilla: usize,
    pub changed: usize,
    pub fixed_elsewhere: usize,
}

#[derive(Debug, Clone)]
pub struct Generated {
    pub seed_file: SeedFile,
    pub counts: Counts,
    pub report: String,
    pub spoiler: Option<String>,
    pub spheres: Vec<Vec<(String, String)>>,
    pub loot_pool: Option<String>,
}

pub fn is_song(actor: &str) -> bool {
    actor.starts_with("Item_Melody_") && actor != "Item_Melody_ScarecrowSong"
}

/// Whether `it` may be placed at `l`. Every item may go to every shuffled location; only the
/// rules the settings ask for remain (crate::slice::place_rules::SETTING_RULES, songs at song
/// spots, and where dungeon items may go). Where a location's give path cannot yet hand out an
/// item, the mod build says so, or the item misbehaves in game and is fixed at that path.
/// Whether the mod build has a form for `it` at `l`: a pickup actor at a ground spot or a Gold
/// Skulltula, a shelf actor at a shop deal. Every other spot gives any item through its grant,
/// and every spot credits a dungeon item to its dungeon (a chest or boss heart chest through its
/// own credit, an NPC grant through crate::gift_keys, ground and skulltula pickups through their
/// dungeon's pickup actor, shop deals through the shelf hook, the other NPCs, sale offers and
/// story chests through crate::debug_names location credits).
pub fn buildable(d: &Data, w: &World, p: &Plan, it: ItemId, l: LocId) -> bool {
    let name: &str = &w.locs[l].name;
    let item: &str = &w.items.names[it];
    // The adult trade sequence is not shuffled: its spots clone their chain event, and a trade
    // quest item shares the menu slot with the chain.
    if crate::flow_extra::trade::is(name) && d.item(item).and_then(|i| i.actor.as_deref()).is_some_and(crate::flow_extra::trade::chain) && d.vanilla_of(name) != Some(item) {
        return false;
    }
    if crate::shops::is_loc(name) && p.shops.on() {
        return crate::shops::rules::class(d, item).is_some_and(|c| c.on(&p.shops, p.songs == Songs::Anywhere));
    }
    if let Some(ok) = crate::slice::skulls::accepts(d, w, it, l).filter(|_| p.skulls != crate::slice::skulls::Mode::Off) {
        return ok;
    }
    if d.cat.by_name(name).is_some_and(pu::is_world) {
        return pu::ground_actor(item).is_some() || crate::slice::key_spots::shield(item).is_some() || crate::slice::key_spots::placeable(d, item, crate::slice::key_spots::Spot::Free);
    }
    if let Some(ok) = crate::pot_slice::rules::at(d, name, item) {
        return ok;
    }
    true
}

pub fn allowed(d: &Data, w: &World, p: &Plan, it: ItemId, l: LocId) -> bool {
    let name: &str = &w.locs[l].name;
    let spot: crate::slice::place_rules::Spot = crate::slice::place_rules::Spot { d, w, p, it, l, name, item: &w.items.names[it] };
    if let Some(ok) = crate::slice::place_rules::setting_verdict(&spot) {
        return ok;
    }
    let Some(i) = d.item(&w.items.names[it]) else {
        return false;
    };
    if !buildable(d, w, p, it, l) {
        return false;
    }
    let here: Option<&str> = w.regions[w.locs[l].region].dungeon.as_deref();
    if scoped(p, &i.kind) {
        return i.dungeon.is_some() && i.dungeon.as_deref() == here;
    }
    if crate::slice::dungeon_items::handles(p, &i.kind) {
        return crate::slice::dungeon_items::spot_ok(d, w, p, i, l);
    }
    if p.keys_roam() && i.kind == "SmallKey" {
        return i.dungeon.is_some() && (p.keys.small != Keep::AnyDungeon || here.is_some());
    }
    if p.songs == Songs::Own {
        let song_item: bool = i.actor.as_deref().is_some_and(is_song);
        if song_item != d.song_ok(name) {
            return false;
        }
    }
    true
}

fn item_name(w: &World, v: &Placement, l: LocId) -> String {
    match v.get(l) {
        Some(i) => w.items.names[i].clone(),
        None => "(nothing)".to_string(),
    }
}

type Steps = Vec<Vec<(String, String)>>;

// Legacy logic entries that do not represent additional remake heart rewards.
const NON_REMAKE_HEARTS: [&str; 3] = ["GF Freestanding PoH", "Ice Cavern Freestanding PoH", "ZR Frogs in the Rain"];

fn loot_pool_text(d: &Data, w: &World, pooled: &Placement, base: &Placement, got: &Placement, free: &[LocId], starting: i64) -> String {
    let mut changes: BM<&str, i64> = BM::new();
    let mut pieces: usize = 0;
    let mut containers: usize = 0;
    for &l in free {
        if pooled.get(l) != base.get(l) {
            if let Some(i) = pooled.get(l) {
                *changes.entry(&w.items.names[i]).or_default() -= 1;
            }
            if let Some(i) = base.get(l) {
                *changes.entry(&w.items.names[i]).or_default() += 1;
            }
        }
        if let Some(i) = got.get(l) {
            let name: &str = &w.items.names[i];
            if name == crate::start_items::pool::CONTAINER {
                containers += 1;
            } else if crate::start_items::pool::PIECES.contains(&name) {
                pieces += 1;
            }
        }
    }
    let free_set: HS<LocId> = free.iter().copied().collect();
    let mut fixed: Vec<(&str, &str)> = Vec::new();
    let mut unmapped: Vec<(&str, &str)> = Vec::new();
    for &l in &w.real {
        if free_set.contains(&l) || NON_REMAKE_HEARTS.contains(&w.locs[l].name.as_str()) {
            continue;
        }
        if let Some(i) = got.get(l) {
            let item: &str = &w.items.names[i];
            if item == crate::start_items::pool::CONTAINER || crate::start_items::pool::PIECES.contains(&item) {
                let name: &str = &w.locs[l].name;
                let group = if d.cat.by_name(name).is_some() || d.flow.by_name(name).is_some() { &mut fixed } else { &mut unmapped };
                group.push((name, item));
            }
        }
    }
    fixed.sort_unstable();
    unmapped.sort_unstable();
    let mut out: String = String::new();
    let _ = writeln!(out, "Loot pool changes");
    let _ = writeln!(out, "  Net changes after the standard shuffled pool is built:");
    for (heading, positive) in [("Added", true), ("Removed", false)] {
        let _ = writeln!(out, "  {heading}:");
        let mut any: bool = false;
        for (item, count) in &changes {
            if (positive && *count > 0) || (!positive && *count < 0) {
                let _ = writeln!(out, "    {} x {}", count.unsigned_abs(), item);
                any = true;
            }
        }
        if !any {
            let _ = writeln!(out, "    None");
        }
    }
    let fixed_pieces: usize = fixed.iter().filter(|(_, item)| crate::start_items::pool::PIECES.contains(item)).count();
    let fixed_containers: usize = fixed.len() - fixed_pieces;
    let unmapped_pieces: usize = unmapped.iter().filter(|(_, item)| crate::start_items::pool::PIECES.contains(item)).count();
    let unmapped_containers: usize = unmapped.len() - unmapped_pieces;
    let total: i64 = starting * 4 + (containers + fixed_containers + unmapped_containers) as i64 * 4 + (pieces + fixed_pieces + unmapped_pieces) as i64;
    let _ = writeln!(out, "\nTotal Heart Pool");
    let _ = writeln!(out, "  Starting hearts: {starting}");
    let _ = writeln!(out, "  Heart Containers in shuffled locations: {containers}");
    let _ = writeln!(out, "  Pieces of Heart in shuffled locations: {pieces}");
    let _ = writeln!(out, "  Unshuffled mapped rewards: {fixed_containers} Heart Containers, {fixed_pieces} Pieces of Heart");
    for (name, item) in &fixed {
        let _ = writeln!(out, "    {name}: {item}");
    }
    let _ = writeln!(out, "  Unmapped logic rewards (unverified): {unmapped_containers} Heart Containers, {unmapped_pieces} Pieces of Heart");
    for (name, item) in &unmapped {
        let _ = writeln!(out, "    {name}: {item}");
    }
    let _ = writeln!(out, "  Total Heart Containers (modeled): {}", containers + fixed_containers + unmapped_containers);
    let _ = writeln!(out, "  Total Pieces of Heart (modeled): {}", pieces + fixed_pieces + unmapped_pieces);
    if unmapped.is_empty() {
        let _ = writeln!(out, "  Total: {} hearts and {} extra pieces", total / 4, total % 4);
        let _ = writeln!(out, "  Exactly 20 hearts: {}", if total == 80 { "Yes" } else { "No" });
    } else {
        let _ = writeln!(out, "  Estimated total: {} hearts and {} extra pieces", total / 4, total % 4);
        let _ = writeln!(out, "  Exactly 20 hearts (estimated): {}", if total == 80 { "Yes" } else { "No" });
        let _ = writeln!(out, "  Unmapped logic rewards may not correspond to distinct in-game rewards; the total is not verified in game.");
    }
    out
}

fn spoiler_text(w: &World, vanilla: &Placement, got: &Placement, pk: &Pick) -> Res<(String, Steps)> {
    let sp: Spoiler = playthrough(w, got)?;
    let steps: Steps = sp
        .spheres
        .iter()
        .map(|sphere| sphere.iter().map(|(l, i)| (w.locs[*l].name.clone(), w.items.names[*i].clone())).collect())
        .collect();
    let mut out: String = String::new();
    out.push_str(crate::spoiler::LEGEND);
    let _ = writeln!(out, "Randomized chests");
    for l in &pk.free {
        let _ = writeln!(
            out,
            "  {}: {} (vanilla {})",
            w.locs[*l].name,
            crate::spoiler::shown(&item_name(w, got, *l)),
            crate::spoiler::shown(&item_name(w, vanilla, *l))
        );
    }
    let _ = writeln!(out, "Playthrough, {} required locations in {} spheres", sp.required.len(), sp.spheres.len());
    for (n, sphere) in sp.spheres.iter().enumerate() {
        let _ = writeln!(out, "Sphere {}", n + 1);
        for (l, i) in sphere {
            let _ = writeln!(out, "  {}: {}", w.locs[*l].name, crate::spoiler::shown(&w.items.names[*i]));
        }
    }
    Ok((out, steps))
}

fn report(s: &str, p: &Plan, pk: &Pick, c: &Counts, moved: &[String]) -> String {
    let mut out: String = String::new();
    let _ = writeln!(out, "Chest shuffle slice, seed {s:?}");
    let _ = writeln!(
        out,
        "Randomized {} chests and changed {} of them. {} of the {} candidate chests stay vanilla. {} other locations hold their vanilla items.",
        c.randomized, c.changed, c.left_vanilla, c.candidates, c.fixed_elsewhere
    );
    if !pk.left.is_empty() {
        let _ = writeln!(out, "Candidate chests left vanilla:");
        for (k, v) in &pk.left {
            let _ = writeln!(out, "  {v} because {k}");
        }
    }
    if !moved.is_empty() {
        let _ = writeln!(out, "Changed chests:");
        for m in moved {
            let _ = writeln!(out, "  {m}");
        }
    }
    for n in &p.notes {
        let _ = writeln!(out, "{n}");
    }
    let _ = writeln!(
        out,
        "Logic: OoTR glitchless rules for the vanilla world, small keys counted for real, with the Fire, Water and Spirit keys kept in their vanilla chests, every other location holds its vanilla item."
    );
    out
}

/// The pool placement: the vanilla placement with each shuffled location holding its
/// Data::pool_item, so the tiered minigame prizes add their OoTR progressive item to the pool.
/// The location itself keeps its junk vanilla item for the rules and the spoiler log. The copies
/// come back only when the shuffled locations have room for them (twice as many chests and reward
/// events that take one of these items as there are copies, pots and other pickups do not count), so a seed with few shuffled chests keeps the junk.
pub fn restore_upgrades(w: &World, d: &Data, vanilla: &Placement, free: &[LocId], ok: &dyn Fn(ItemId, LocId) -> bool) -> Placement {
    let mut out: Placement = vanilla.clone();
    let mut back: Vec<ItemId> = Vec::new();
    for l in free {
        let name: &str = &w.locs[*l].name;
        if let Some(i) = d.pool_item(name).filter(|p| Some(*p) != d.vanilla_of(name)).and_then(|p| w.item(p)) {
            out.put(*l, i);
            back.push(i);
        }
    }
    if back.is_empty() {
        return out;
    }
    let copies: usize = free.iter().filter(|l| out.get(**l).is_some_and(|i| back.contains(&i))).count();
    let holds = |l: LocId| {
        let name: &str = &w.locs[l].name;
        d.cat.by_name(name).is_some_and(|c| c.kind == "chest") || d.flow.by_name(name).is_some()
    };
    let room: usize = free.iter().filter(|l| holds(**l) && back.iter().any(|i| ok(*i, **l))).count();
    if room < 2 * copies {
        return vanilla.clone();
    }
    out
}

pub fn check_excluded(w: &World, p: &Plan, free: &[LocId], base: &Placement) -> Res<Vec<String>> {
    let here: Vec<&LocId> = free.iter().filter(|l| p.excluded.contains(&w.locs[**l].name)).collect();
    let mut notes: Vec<String> = Vec::new();
    if here.len() < p.excluded.len() {
        let gone: Vec<&str> = p.excluded.iter().filter(|e| !here.iter().any(|l| w.locs[**l].name == **e)).map(|e| e.as_str()).collect();
        notes.push(format!("Excluded but not shuffled with these settings, so they keep their vanilla item: {}.", gone.join(", ")));
    }
    let junk: usize = free.iter().filter_map(|l| base.get(*l)).filter(|i| !w.items.adv[*i]).count();
    if here.len() > junk {
        return Err(E::Msg(format!(
            "too many locations are excluded: {} excluded locations can only hold items that are not needed for progression, and these settings shuffle only {junk} of those. Exclude fewer locations or shuffle more items",
            here.len()
        )));
    }
    if !here.is_empty() {
        notes.push(format!("{} excluded locations hold only items that are not needed for progression.", here.len()));
    }
    Ok(notes)
}

pub fn generate_data(d: &Data, s: &Settings, seed: &str, o: &GenOpts) -> Res<Generated> {
    if d.scan.rows.is_empty() {
        return Err(E::Format("the chest list in data/slice.json is empty, run slice-scan first"));
    }
    let watch: Watch = Watch::new(o.limit, o.live.clone());
    let mut p: Plan = plan(s)?;
    p.notes.extend(p.trials.note());
    let room: crate::shops::layout::Room = crate::shops::rules::room(d, &p)?;
    let forced: Vec<String> = o.plando.iter().map(|e| e.location.clone()).collect();
    let mut lay: Option<crate::shops::Layout> = crate::shops::layout::of_with(&p.shops, seed, room, &forced)?;
    p.notes.extend(lay.as_ref().and_then(|l| crate::shops::rules::cut_note(l.cut, room)));
    let owned: Option<Data> = lay.as_ref().map(|l| crate::shops::logic::data_with(d, l)).transpose()?;
    let d: &Data = owned.as_ref().unwrap_or(d);
    let mut w: World = match &lay {
        Some(l) => crate::shops::logic::world(d, &p.logic(), l)?,
        None => world_with(d, &p.logic())?,
    };
    if settle_keys(d, &mut p, &w) {
        w = match &lay {
            Some(l) => crate::shops::logic::world(d, &p.logic(), l)?,
            None => world_with(d, &p.logic())?,
        };
    }
    let mut named: Vec<String> = Vec::with_capacity(p.excluded.len());
    for e in &p.excluded {
        let l: LocId = loc_of(&w, e).map_err(|x| E::Msg(x.to_string().replacen("plando:", "disabled locations:", 1)))?;
        named.push(w.locs[l].name.clone());
    }
    p.excluded = named;
    crate::slice::starts::add(d, &p, &mut w, seed)?;
    let vanilla: Placement = vanilla_place(&w, d)?;
    let mut pk: Pick = pick(d, &w, &p);
    if let Some(l) = &lay {
        l.add_free(&w, &mut pk.free)?;
    }
    let gates: Option<crate::entr::Layout> = crate::entr::choose(s, &mut w, seed, &vanilla, &pk.free)?;
    if p.world.on() && !pk.free.iter().any(|l| pu::listed(d, &w.locs[*l].name)) {
        p.notes.retain(|n| !pu::is_note(n));
    }
    let pooled: Placement = restore_upgrades(&w, d, &vanilla, &pk.free, &|it: ItemId, l: LocId| allowed(d, &w, &p, it, l));
    let (mut base, gone): (Placement, Vec<String>) = crate::start_items::pool::base(d, &w, &p, &pooled, &pk.free)?;
    p.notes.extend(gone);
    if let Some(note) = crate::start_items::pool::adjust_hearts(&w, s, seed, &mut base, &pk.free)? {
        p.notes.push(note);
    }
    let ex: Vec<String> = check_excluded(&w, &p, &pk.free, &base)?;
    p.notes.extend(ex);
    let seen: Vec<Cell<u8>> = vec![Cell::new(0); w.items.len() * w.locs.len()];
    let ok = |it: ItemId, l: LocId| {
        let c: &Cell<u8> = &seen[it * w.locs.len() + l];
        match c.get() {
            1 => false,
            2 => true,
            _ => {
                let v: bool = allowed(d, &w, &p, it, l);
                c.set(1 + v as u8);
                v
            }
        }
    };
    for l in &pk.free {
        if let Some(i) = vanilla.get(*l)
            && !p.excluded.contains(&w.locs[*l].name)
            && !ok(i, *l)
            && !d.item(&w.items.names[i]).is_some_and(|x| crate::slice::dungeon_items::exempt(&p, &x.kind))
            && !(lay.is_some() && crate::shops::is_loc(&w.locs[*l].name))
            && !(p.silver && crate::slice::silver::is_spot(&w.locs[*l].name))
            // The Light Arrows Cutscene holds Ganon's Boss Key in this mode; its Light Arrows join the pool.
            && !(p.keys.ganon == Keep::Lacs && w.locs[*l].name == crate::lacs_key::LOC)
        {
            return Err(E::Msg(format!(
                "the vanilla item {} of {} is not allowed there under these settings",
                w.items.names[i], w.locs[*l].name
            )));
        }
    }
    let plan: Vec<(LocId, ItemId)> = resolve(&w, &o.plando)?;
    for (l, i) in &plan {
        if !pk.free.contains(l) {
            return Err(E::Msg(format!(
                "plando: {} is not shuffled with these settings, so it cannot hold {}",
                w.locs[*l].name, w.items.names[*i]
            )));
        }
        if !ok(*i, *l) {
            return Err(E::Msg(format!("plando: {} cannot be placed at {} under these settings", w.items.names[*i], w.locs[*l].name)));
        }
    }
    let hand: usize = plan.len();
    let mut plan: Vec<(LocId, ItemId)> = plan;
    let lacs: Vec<(LocId, ItemId)> = crate::lacs_key::forced(&w, &p, &pk.free, &base, &plan)?;
    plan.extend(lacs);
    let mut rng: Rng = Rng::from_text(seed);
    let shelf = |it: ItemId, l: LocId| ok(it, l) && !(crate::shops::is_loc(&w.locs[l].name) && d.family(&w.items.names[it]).is_some());
    let mut run = |careful: bool, reach: bool| -> Res<Placement> {
        let ok: &dyn Fn(ItemId, LocId) -> bool = if careful { &shelf } else { &ok };
        let fix: bool = crate::slice::tier_fix::active(d, &w, &pk.free);
        let narrow = |it: ItemId, l: LocId| ok(it, l) && !crate::slice::key_spots::widened(d, &w.locs[l].name, &w.items.names[it]);
        let order: Option<&dyn Fn(ItemId, LocId) -> bool> = if fix { Some(&narrow) } else { None };
        let caps: HM<LocId, usize> = crate::pot_slice::rules::caps(d, &w, &pk.free, &base, order.unwrap_or(ok));
        let rank = |l: LocId| caps.get(&l).copied().unwrap_or(0);
        let r: Option<&dyn Fn(LocId) -> usize> = if caps.is_empty() { None } else { Some(&rank) };
        let mut sr: Rng = Rng::from_text(&format!("{seed} songs first{}{}", if careful { " careful" } else { "" }, if reach { " reach" } else { "" }));
        let mut fixed: Vec<(LocId, ItemId)> = plan.clone();
        if p.songs == Songs::Anywhere {
            fixed.extend(crate::slice::song_spots::first_with(d, &w, &base, &pk.free, &plan, ok, &mut sr, careful)?);
        }
        let mut keep: Vec<LocId> = fixed.iter().map(|x| x.0).collect();
        let mut n: u32 = 0;
        let mut redo: u32 = 0;
        loop {
            let quick: bool = p.songs == Songs::Anywhere && redo + 1 < crate::slice::song_spots::TRIES;
            let (tries, switch): (u32, u32) = if quick { (o.tries.min(crate::slice::song_spots::QUICK_TRIES), o.switch.min(crate::slice::song_spots::QUICK_SWITCH)) } else { (o.tries, o.switch) };
            let g: Placement = match fill_ordered(&w, &base, &pk.free, ok, order, &mut rng, tries, &fixed, p.strict_layouts().is_empty() && gates.is_none(), r, reach, switch, &watch) {
                Ok(g) => g,
                // A run out of time (or cancelled) stops here with that message, not a retry's.
                Err(_) if quick && watch.check().is_ok() => {
                    redo += 1;
                    fixed = plan.clone();
                    fixed.extend(crate::slice::song_spots::first_with(d, &w, &base, &pk.free, &plan, ok, &mut sr, careful)?);
                    keep = fixed.iter().map(|x| x.0).collect();
                    continue;
                }
                Err(e) => return Err(crate::slice::advice::explain(&p, e)),
            };
            if crate::slice::tier_order::fits(d, &w, &g, &pk.free)? {
                return Ok(g);
            }
            if fix {
                let mut fr: Rng = Rng::from_text(&format!("{seed} tier repair {n}"));
                let c: crate::slice::tier_fix::Ctx = crate::slice::tier_fix::Ctx { d, w: &w, free: &pk.free, keep: &keep, ok };
                if let Some(x) = crate::slice::tier_fix::repair(&c, &g, &mut fr)? {
                    return Ok(x);
                }
            }
            n += 1;
            if n >= TIER_TRIES {
                let hint: &str = if o.plando.is_empty() {
                    "these settings leave too little room, try another seed text or fewer shuffled pickup spots"
                } else {
                    "check the plando entries for upgrade items, or try another seed text"
                };
                let last: Vec<String> = crate::slice::tier_fix::verdict(d, &w, &g, &pk.free)?.bad.into_iter().take(2).collect();
                return Err(E::Msg(format!("no fill in {TIER_TRIES} tries kept every fixed upgrade tier in order, {hint}. The last fill failed because {}", last.join(", and "))));
            }
        }
    };
    // Pieces of Heart on the ground are always shuffled. When they and the Heart Containers are
    // all the pool holds (chests and NPC rewards off), they only trade places with each other,
    // which changes nothing, and the assumed fill cannot place them since the logic counts
    // hearts. They keep their own spots.
    let hearts_only: bool = pk.free.iter().all(|l| base.get(*l).is_some_and(|i| HEARTS.contains(&w.items.names[i].as_str())));
    let got: Placement = if pk.free.is_empty() || hearts_only {
        base.clone()
    } else {
        let anywhere: bool = p.songs == Songs::Anywhere;
        match run(false, false) {
            Ok(g) => g,
            Err(e) if watch.check().is_err() => return Err(e),
            Err(_) => match anywhere.then(|| run(true, false)) {
                Some(Ok(g)) => g,
                Some(Err(x)) if watch.check().is_err() => return Err(x),
                _ => run(lay.is_some() || anywhere, true)?,
            },
        }
    };
    if p.skulls != crate::slice::skulls::Mode::Off {
        crate::slice::skulls::check_tokens(&w, &vanilla, &got)?;
    }
    let mut map: BM<String, String> = BM::new();
    let mut moved: Vec<String> = Vec::new();
    for l in &pk.free {
        let new: String = item_name(&w, &got, *l);
        let old: String = item_name(&w, &vanilla, *l);
        if new != old {
            moved.push(format!("{}: {} instead of {}", w.locs[*l].name, new, old));
        }
        map.insert(w.locs[*l].name.clone(), new);
    }
    let counts: Counts = Counts {
        candidates: d.scan.rows.len() + pk.flow,
        randomized: pk.free.len(),
        left_vanilla: (d.scan.rows.len() + pk.flow).saturating_sub(pk.free.len()),
        changed: changed(&vanilla, &got, &pk.free),
        fixed_elsewhere: w.real.len() - pk.free.len(),
    };
    let mut seed_file: SeedFile = SeedFile::new(seed, s, &d.scan.game, map)?;
    if let Some(l) = lay.as_mut() {
        crate::shops::layout::fill_deals(l, &seed_file.placements)?;
        seed_file.shops = crate::shops::seed::entries(l);
    }
    if let Some(l) = &gates {
        seed_file.entrances = l.to_map();
    }
    let loot_pool: Option<String> = if o.spoiler {
        Some(loot_pool_text(d, &w, &pooled, &base, &got, &pk.free, s.int("starting_hearts")?))
    } else {
        None
    };
    let (sp, spheres): (Option<String>, Steps) = if o.spoiler {
        let (mut t, st): (String, Steps) = spoiler_text(&w, &vanilla, &got, &pk)?;
        if let Some(summary) = &loot_pool {
            t.insert_str(0, &format!("{summary}\n"));
        }
        let fixed: BM<String, String> = if crate::slice::tier_fix::active(d, &w, &pk.free) {
            crate::slice::tier_fix::verdict(d, &w, &got, &pk.free)?.assign
        } else {
            BM::new()
        };
        t.push_str(&crate::slice::skulls::spoiler_with(d, &w, &p, &pk.free, &got, &fixed));
        t.push_str(&crate::slice::key_spots::spoiler(d, &w, &pk.free, &got, &fixed));
        if let Some(l) = &lay {
            t.push_str(&crate::shops::spoiler::text(l));
        }
        if let Some(l) = &gates {
            t.insert_str(0, &crate::entr::text::spoiler(l));
        }
        (Some(t), st)
    } else {
        (None, Vec::new())
    };
    let mut rep: String = report(seed, &p, &pk, &counts, &moved);
    if hand > 0 {
        let _ = writeln!(rep, "Plandomizer: {hand} placements were fixed by hand.");
    }
    if let Some(l) = &gates {
        rep.push_str(&crate::entr::text::report(l));
    }
    Ok(Generated { seed_file, counts, report: rep, spoiler: sp, spheres, loot_pool })
}

pub fn settle_keys(d: &Data, p: &mut Plan, w: &World) -> bool {
    if p.keys.small != Keep::Dungeon {
        return false;
    }
    let kept: Vec<(&'static str, &'static str)> = crate::slice::pick::own_strict_kept(d, w, p, &pick(d, w, p).free);
    if kept == p.own_strict {
        return false;
    }
    p.own_strict = kept;
    true
}

pub fn plando_options(d: &Data, s: &Settings) -> Res<(Vec<String>, Vec<String>)> {
    let mut p: Plan = plan(s)?;
    let mut w: World = world_with(d, &p.logic())?;
    if settle_keys(d, &mut p, &w) {
        w = world_with(d, &p.logic())?;
    }
    let vanilla: Placement = vanilla_place(&w, d)?;
    let pk: Pick = pick(d, &w, &p);
    let mut locs: Vec<String> = pk.free.iter().map(|l| w.locs[*l].name.clone()).collect();
    let mut items: Vec<String> = pk.free.iter().filter_map(|l| vanilla.get(*l)).map(|i| w.items.names[i].clone()).collect();
    locs.sort();
    items.sort();
    items.dedup();
    Ok((locs, items))
}

/// The heart items that only trade places with each other when nothing else is shuffled.
pub const HEARTS: [&str; 2] = ["Piece of Heart", "Heart Container"];

pub fn generate_with(s: &Settings, seed: &str, o: &GenOpts) -> Res<Generated> {
    generate_data(&Data::load()?, s, seed, o)
}

pub fn generate(s: &Settings, seed: &str) -> Res<Generated> {
    generate_with(s, seed, &GenOpts::default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Value;
    use crate::slice::data::{CATALOG_JSON, ITEMS_JSON, REMAKE_JSON, VANILLA_JSON};
    use crate::slice::logic::world;

    const BIG: [&str; 6] = [
        "Forest Temple Bow Chest",
        "Dodongos Cavern Bomb Bag Chest",
        "Deku Tree Slingshot Chest",
        "Jabu Jabus Belly Boomerang Chest",
        "Ice Cavern Iron Boots Chest",
        "Bottom of the Well Lens of Truth Chest",
    ];

    fn synthetic() -> Data {
        let mut rows: Vec<String> = Vec::new();
        for n in BIG {
            rows.push(format!("{{\"name\":\"{n}\",\"state\":\"eligible\"}}"));
        }
        let scan: String = format!(
            "{{\"format\":1,\"game\":\"oot-remake:synthetic\",\"files\":[],\"chests\":[{}]}}",
            rows.join(",")
        );
        let mut d: Data = Data::new(ITEMS_JSON, VANILLA_JSON, &scan, CATALOG_JSON).unwrap().with_remake(REMAKE_JSON).unwrap();
        // Only six chests are scanned, so the tools that reach most freestanding Pieces of Heart
        // (always shuffled) are never found. The synthetic world leaves the ground spots out.
        d.cat.locs.retain(|l| !crate::slice::pickups::is_world(l) && !crate::slice::pickups::is_held(&l.name));
        d
    }

    /// Every chest, with the newer default-on rewards (More NPC rewards, boss Heart
    /// Containers) off, as these tests were written for. The synthetic data has too few
    /// spots for them.
    fn all_chests() -> Settings {
        let mut s: Settings = Settings::default();
        // Seeds made before Ocarina and Kokiri Sword shuffle had both shuffled.
        crate::slice::pick::shuffle_both(&mut s);
        s.set("shuffle_chests", Value::s("all")).unwrap();
        s.set(crate::flow_extra::SETTING, Value::Bool(false)).unwrap();
        s.set("shuffle_boss_hearts", Value::Bool(false)).unwrap();
        s
    }

    #[test]
    fn loot_pool_counts_net_changes_and_all_heart_variants() {
        let d: Data = Data::load().unwrap();
        let w: World = world(&d).unwrap();
        let item = |name: &str| w.items.names.iter().position(|n| n == name).unwrap();
        let green: ItemId = item("Rupees (5)");
        let piece: ItemId = item("Piece of Heart");
        let outside: ItemId = item("Piece of Heart (Out of Logic)");
        let treasure: ItemId = item("Piece of Heart (Treasure Chest Game)");
        let container: ItemId = item("Heart Container");
        let free: Vec<LocId> = w.real.iter().copied().take(4).collect();
        let mut vanilla: Placement = Placement::new(w.locs.len());
        let mut base: Placement = Placement::new(w.locs.len());
        let mut got: Placement = Placement::new(w.locs.len());
        for &l in &free {
            vanilla.put(l, green);
        }
        for (&l, i) in free.iter().zip([piece, outside, treasure, container]) {
            base.put(l, i);
        }
        for (&l, i) in free.iter().zip([container, treasure, piece, outside]) {
            got.put(l, i);
        }
        let text: String = loot_pool_text(&d, &w, &vanilla, &base, &got, &free, 18);
        assert!(text.contains("4 x Rupees (5)"));
        assert!(text.contains("1 x Piece of Heart (Treasure Chest Game)"));
        assert!(text.contains("Heart Containers in shuffled locations: 1"));
        assert!(text.contains("Pieces of Heart in shuffled locations: 3"));
        assert!(text.contains("19 hearts and 3 extra pieces"));
        assert!(text.contains("Exactly 20 hearts: No"));
        let over: String = loot_pool_text(&d, &w, &vanilla, &base, &got, &free, 19);
        assert!(over.contains("20 hearts and 3 extra pieces"));
        assert!(over.contains("Exactly 20 hearts: No"));
        got.put(free[0], piece);
        let exact: String = loot_pool_text(&d, &w, &vanilla, &base, &got, &free, 19);
        assert!(exact.contains("Exactly 20 hearts: Yes"));
        let mapped: LocId = w.loc("LLR Freestanding PoH").unwrap();
        let mapped_container: LocId = w.loc("DMT Freestanding PoH").unwrap();
        let unknown: LocId = w.real.iter().copied().find(|l| {
            let name: &str = &w.locs[*l].name;
            !free.contains(l) && !NON_REMAKE_HEARTS.contains(&name) && d.cat.by_name(name).is_none() && d.flow.by_name(name).is_none()
        }).unwrap();
        got.put(mapped, piece);
        got.put(mapped_container, container);
        got.put(unknown, piece);
        let with_fixed: String = loot_pool_text(&d, &w, &vanilla, &base, &got, &free, 19);
        assert!(with_fixed.contains("Unshuffled mapped rewards: 1 Heart Containers, 1 Pieces of Heart"));
        assert!(with_fixed.contains("Unmapped logic rewards (unverified): 0 Heart Containers, 1 Pieces of Heart"));
        assert!(with_fixed.contains("Total Heart Containers (modeled): 1"));
        assert!(with_fixed.contains("Total Pieces of Heart (modeled): 6"));
        assert!(with_fixed.contains("Estimated total: 21 hearts and 2 extra pieces"));
        let mut more_free: Vec<LocId> = free.clone();
        more_free.extend([mapped, mapped_container, unknown]);
        let moved_to_pool: String = loot_pool_text(&d, &w, &vanilla, &base, &got, &more_free, 19);
        assert!(moved_to_pool.contains("Unshuffled mapped rewards: 0 Heart Containers, 0 Pieces of Heart"));
        assert!(moved_to_pool.contains("Unmapped logic rewards (unverified): 0 Heart Containers, 0 Pieces of Heart"));
        assert!(moved_to_pool.contains("Total: 21 hearts and 2 extra pieces"));
        let unchanged: String = loot_pool_text(&d, &w, &vanilla, &vanilla, &vanilla, &free, 3);
        assert!(unchanged.contains("Added:\n    None\n  Removed:\n    None"));
    }

    #[test]
    fn heart_summary_accounts_for_fixed_and_unmapped_checks() {
        let d: Data = Data::load().unwrap();
        for enabled in [false, true] {
            let mut s: Settings = Settings::default();
            s.set("shuffle_treasure_reward", Value::Bool(enabled)).unwrap();
            s.set("shuffle_frog_bug_heart", Value::Bool(enabled)).unwrap();
            s.set("shuffle_scrubs_a", Value::Bool(enabled)).unwrap();
            let p: Plan = plan(&s).unwrap();
            let w: World = world_with(&d, &p.logic()).unwrap();
            let v: Placement = vanilla_place(&w, &d).unwrap();
            let pk: Pick = pick(&d, &w, &p);
            let text: String = loot_pool_text(&d, &w, &v, &v, &v, &pk.free, 3);
            for name in ["Market Treasure Chest Game Reward", "ZR Frogs Bug Game", "DMC Deku Scrub"] {
                assert_eq!(text.contains(&format!("    {name}:")), !enabled, "{name}\n{text}");
            }
            for name in NON_REMAKE_HEARTS {
                assert!(!text.contains(&format!("    {name}:")), "{name}\n{text}");
            }
            assert!(!text.contains("    LLR Freestanding PoH:"));
        }
        let mut s: Settings = Settings::default();
        s.set("shuffle_rewards", Value::s("off")).unwrap();
        s.set("shuffle_boss_hearts", Value::Bool(false)).unwrap();
        let p: Plan = plan(&s).unwrap();
        let w: World = world_with(&d, &p.logic()).unwrap();
        let v: Placement = vanilla_place(&w, &d).unwrap();
        let pk: Pick = pick(&d, &w, &p);
        let text: String = loot_pool_text(&d, &w, &v, &v, &v, &pk.free, 3);
        for name in ["ZR Frogs Ocarina Game", "HF Deku Scrub Grotto", "Deku Tree Queen Gohma Heart"] {
            assert!(text.contains(&format!("    {name}:")), "{name}\n{text}");
        }
        let l: LocId = w.loc("LLR Freestanding PoH").unwrap();
        let free: Vec<LocId> = pk.free.iter().copied().filter(|x| *x != l).collect();
        let text: String = loot_pool_text(&d, &w, &v, &v, &v, &free, 3);
        assert!(text.contains("    LLR Freestanding PoH: Piece of Heart"));
    }

    #[test]
    fn legacy_heart_entries_do_not_inflate_generated_totals() {
        let d: Data = Data::load().unwrap();
        for enabled in [false, true] {
            let mut s: Settings = Settings::default();
            s.set("starting_hearts", Value::Int(1)).unwrap();
            s.set("shuffle_treasure_reward", Value::Bool(enabled)).unwrap();
            s.set("shuffle_frog_bug_heart", Value::Bool(enabled)).unwrap();
            s.set("shuffle_scrubs_a", Value::Bool(enabled)).unwrap();
            let g: Generated = generate_data(&d, &s, "heart total", &GenOpts { spoiler: true, ..GenOpts::default() }).unwrap();
            let summary: &str = g.loot_pool.as_deref().unwrap();
            assert!(summary.contains("Total: 20 hearts and 0 extra pieces"), "{summary}");
            assert!(summary.contains("Exactly 20 hearts: Yes"), "{summary}");
            for name in NON_REMAKE_HEARTS {
                assert!(!summary.contains(&format!("    {name}:")), "{summary}");
            }
        }
    }

    #[test]
    fn generated_spoiler_reports_heart_additions_and_removals() {
        let d: Data = Data::load().unwrap();
        for (start, heading, count) in [(1, "Added", 2), (4, "Removed", 1)] {
            let mut s: Settings = Settings::default();
            s.set("starting_hearts", Value::Int(start)).unwrap();
            s.set("heart_pool", Value::s("containers")).unwrap();
            let g: Generated = generate_data(&d, &s, "heart summary", &GenOpts { spoiler: true, ..GenOpts::default() }).unwrap();
            let summary: &str = g.loot_pool.as_deref().unwrap();
            let section: &str = summary.split(&format!("  {heading}:\n")).nth(1).unwrap().split("\n  Removed:").next().unwrap().split("\n\nTotal Heart Pool").next().unwrap();
            assert!(section.contains(&format!("{count} x Heart Container")), "{summary}");
            for upgrade in [" x Bomb Bag", " x Bow", " x Slingshot"] {
                assert!(!summary.split("\n\nTotal Heart Pool").next().unwrap().contains(upgrade), "{summary}");
            }
            assert!(g.spoiler.as_deref().unwrap().contains(summary.trim_end()));
        }
    }

    #[test]
    fn the_shipped_data_generates_a_valid_seed_for_many_seeds() {
        let d: Data = Data::load().unwrap();
        for n in 0..4 {
            let g: Generated = generate_data(&d, &Settings::default(), &format!("seed {n}"), &GenOpts::default()).unwrap();
            assert_eq!(g.counts.randomized, g.seed_file.placements.len());
            assert!(g.counts.changed <= g.counts.randomized);
            let back: SeedFile = SeedFile::parse(&g.seed_file.to_text()).unwrap();
            assert_eq!(back, g.seed_file);
        }
    }

    #[test]
    fn verified_event_chests_are_part_of_the_pool() {
        let d: Data = Data::load().unwrap();
        let g: Generated = generate_data(&d, &Settings::default(), "events", &GenOpts::default()).unwrap();
        for name in ["Deku Tree Slingshot Chest", "Jabu Jabus Belly Compass Chest", "Jabu Jabus Belly Boomerang Chest"] {
            assert!(g.seed_file.placements.contains_key(name), "{name}");
        }
        // A held chest, released with the other remake chests now that they follow Chests.
        assert!(g.seed_file.placements.contains_key("Ice Cavern Iron Boots Chest"));
        assert!(!g.seed_file.placements.keys().any(|k| k.starts_with("Market Treasure Chest Game")));
    }

    #[test]
    fn generation_is_deterministic() {
        let d: Data = synthetic();
        let a: Generated = generate_data(&d, &all_chests(), "same", &GenOpts::default()).unwrap();
        let b: Generated = generate_data(&d, &all_chests(), "same", &GenOpts::default()).unwrap();
        assert_eq!(a.seed_file.to_text(), b.seed_file.to_text());
        assert_eq!(a.report, b.report);
        let mut seen: Vec<BM<String, String>> = Vec::new();
        for n in 0..6 {
            let c: Generated = generate_data(&d, &all_chests(), &format!("other {n}"), &GenOpts::default()).unwrap();
            if !seen.contains(&c.seed_file.placements) {
                seen.push(c.seed_file.placements);
            }
        }
        assert!(seen.len() > 1);
    }

    #[test]
    fn progression_is_shuffled_and_always_beatable() {
        let d: Data = synthetic();
        let mut p: Plan = crate::slice::support::plan(&all_chests()).unwrap();
        let w0: World = world_with(&d, &p.logic()).unwrap();
        settle_keys(&d, &mut p, &w0);
        let w: World = world_with(&d, &p.logic()).unwrap();
        let mut moved: usize = 0;
        for n in 0..12 {
            let g: Generated = generate_data(&d, &all_chests(), &format!("p{n}"), &GenOpts::default()).unwrap();
            let mut place: Placement = vanilla_place(&w, &d).unwrap();
            let mut names: Vec<&String> = Vec::new();
            for (loc, item) in &g.seed_file.placements {
                let l: LocId = w.loc(loc).unwrap();
                place.put(l, w.item(item).unwrap());
                names.push(item);
            }
            assert!(crate::search::explore(&w, &place, &[]).won(), "seed p{n} is not beatable");
            let mut sorted: Vec<&String> = names.clone();
            sorted.sort();
            let mut want: Vec<String> = BIG.iter().map(|b| d.vanilla_of(b).unwrap().to_string()).collect();
            want.sort();
            assert_eq!(sorted.into_iter().cloned().collect::<Vec<String>>(), want);
            moved += g.counts.changed;
        }
        assert!(moved > 12 * 2);
    }

    #[test]
    fn dungeon_items_never_leave_their_dungeon() {
        let d: Data = Data::load().unwrap();
        let w: World = world(&d).unwrap();
        let p: Plan = plan(&Settings::default()).unwrap();
        for n in 0..4 {
            let g: Generated = generate_data(&d, &Settings::default(), &format!("d{n}"), &GenOpts::default()).unwrap();
            for (loc, item) in &g.seed_file.placements {
                let l: LocId = w.loc(loc).unwrap();
                assert!(allowed(&d, &w, &p, w.item(item).unwrap(), l), "{item} at {loc}");
            }
        }
    }

    fn rebuilt(d: &Data, p: &Plan, placements: &BM<String, String>) -> bool {
        let w: World = world_with(d, &p.logic()).unwrap();
        let mut pl: Placement = vanilla_place(&w, d).unwrap();
        for (loc, item) in placements {
            pl.put(w.loc(loc).unwrap(), w.item(item).unwrap());
        }
        crate::search::explore(&w, &pl, &[]).won()
    }

    #[test]
    fn roaming_small_keys_move_between_dungeons_and_every_seed_is_beatable() {
        let d: Data = Data::load().unwrap();
        let mut left: usize = 0;
        for mode in ["any_dungeon", "keysanity"] {
            let s: Settings = with(&[("shuffle_chests", "all"), ("shuffle_smallkeys", mode)]);
            let p: Plan = plan(&s).unwrap();
            let w: World = world_with(&d, &p.logic()).unwrap();
            for n in 0..6 {
                let g: Generated = generate_data(&d, &s, &format!("roam {mode} {n}"), &GenOpts::default()).unwrap();
                assert!(rebuilt(&d, &p, &g.seed_file.placements), "{mode} {n} is not beatable");
                for (loc, item) in &g.seed_file.placements {
                    let Some(i) = d.item(item).filter(|i| i.kind == "SmallKey") else {
                        continue;
                    };
                    let here: Option<&str> = w.regions[w.locs[w.loc(loc).unwrap()].region].dungeon.as_deref();
                    if mode == "any_dungeon" {
                        assert!(here.is_some(), "{item} at {loc} is outside a dungeon");
                    }
                    if here != i.dungeon.as_deref() {
                        left += 1;
                    }
                }
            }
        }
        assert!(left > 10, "keys never left their own dungeon, {left}");
    }

    #[test]
    fn keys_stay_home_when_not_asked_to_roam() {
        let d: Data = Data::load().unwrap();
        let s: Settings = with(&[("shuffle_chests", "all")]);
        let p: Plan = plan(&s).unwrap();
        let w: World = world_with(&d, &p.logic()).unwrap();
        for n in 0..3 {
            let g: Generated = generate_data(&d, &s, &format!("home {n}"), &GenOpts::default()).unwrap();
            for (loc, item) in &g.seed_file.placements {
                if let Some(i) = d.item(item).filter(|i| i.kind == "SmallKey") {
                    let here: Option<&str> = w.regions[w.locs[w.loc(loc).unwrap()].region].dungeon.as_deref();
                    assert_eq!(here, i.dungeon.as_deref(), "{item} at {loc}");
                }
            }
        }
    }

    const FOREST_FIRST: &str = "Forest Temple First Room Chest";

    #[test]
    fn own_dungeon_keys_follow_the_logic_counts() {
        let d: Data = Data::load().unwrap();
        let s: Settings = Settings::default();
        let mut moved: BM<&str, usize> = BM::new();
        for n in 0..6 {
            let g: Generated = generate_data(&d, &s, &format!("own keys {n}"), &GenOpts::default()).unwrap();
            let at = |l: &str| -> Option<String> { g.seed_file.placements.get(l).cloned().or_else(|| d.vanilla_of(l).map(|v| v.to_string())) };
            assert_eq!(at(FOREST_FIRST).as_deref(), Some("Small Key (Forest Temple)"), "own keys {n}");
            for key in ["Small Key (Fire Temple)", "Small Key (Water Temple)", "Small Key (Spirit Temple)"] {
                if g.seed_file.placements.iter().any(|(l, i)| i == key && d.vanilla_of(l) != Some(key)) {
                    *moved.entry(key).or_insert(0) += 1;
                }
            }
            let fire: usize = d.cat.locs.iter().filter(|l| at(&l.name).as_deref() == Some("Small Key (Fire Temple)")).count();
            assert_eq!(fire, 7, "own keys {n}");
            assert!(g.report.contains(crate::start_items::pool::FIRE_NOTE), "own keys {n}");
        }
        assert_eq!(moved.len(), 3, "{moved:?}");
        let p: Plan = plan(&s).unwrap();
        assert!(p.start_flags().contains(&(crate::slice::support::FIRE_LOOP_FLAG.to_string(), true)));
        let mut r: Settings = Settings::default();
        r.set("shuffle_smallkeys", Value::s("keysanity")).unwrap();
        assert!(!plan(&r).unwrap().start_flags().iter().any(|(f, _)| f == crate::slice::support::FIRE_LOOP_FLAG));
    }

    #[test]
    fn chest_modes_and_vanilla_keys_change_the_pool() {
        let d: Data = Data::load().unwrap();
        let mut s: Settings = Settings::default();
        s.set("shuffle_rewards", Value::s("off")).unwrap();
        s.set("shuffle_chests", Value::s("off")).unwrap();
        let off: Generated = generate_data(&d, &s, "x", &GenOpts::default()).unwrap();
        assert!(off.seed_file.placements.values().all(|i| HEARTS.contains(&i.as_str()) || i == "Rutos Letter"));
        assert!(off.seed_file.placements.contains_key(crate::slice::pickups::ALWAYS[0]));
        s.set("shuffle_chests", Value::s("overworld")).unwrap();
        let o: Generated = generate_data(&d, &s, "x", &GenOpts::default()).unwrap();
        assert!(o.counts.randomized >= 2);
        s.set("shuffle_chests", Value::s("dungeons")).unwrap();
        s.set("shuffle_smallkeys", Value::s("vanilla")).unwrap();
        s.set("shuffle_mapcompass", Value::s("vanilla")).unwrap();
        let k: Generated = generate_data(&d, &s, "x", &GenOpts::default()).unwrap();
        assert!(k.counts.randomized >= 1);
        assert!(!k.seed_file.placements.contains_key("Ice Cavern Map Chest"));
        assert!(!k.seed_file.placements.contains_key("Gerudo Training Ground Hidden Ceiling Chest"));
    }

    fn eligible_names(d: &Data) -> Vec<String> {
        d.scan.rows.iter().filter(|r| r.state == crate::slice::data::State::Eligible).map(|r| r.name.clone()).collect()
    }

    #[test]
    fn excluded_locations_hold_only_items_the_seed_does_not_need() {
        let d: Data = Data::load().unwrap();
        let names: Vec<String> = eligible_names(&d).into_iter().filter(|n| n != FOREST_FIRST && !n.starts_with("Fire Temple") && d.vanilla_of(n).and_then(|v| d.item(v)).is_some_and(|i| i.adv)).take(12).collect();
        assert!(names.len() >= 8);
        let list: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
        let mut s: Settings = all_chests();
        s.set("disabled_locations", Value::set(&list)).unwrap();
        for n in 0..3 {
            let g: Generated = generate_data(&d, &s, &format!("ex {n}"), &GenOpts::default()).unwrap();
            let mut moved: usize = 0;
            for nm in &names {
                if let Some(item) = g.seed_file.placements.get(nm) {
                    moved += 1;
                    // Hearts are not progress without a "hearts" goal (crate::slice::logic::spec).
                    let heart: bool = HEARTS.iter().any(|h| item.starts_with(h));
                    assert!(heart || !d.item(item).is_some_and(|i| i.adv), "{nm} holds {item}");
                }
            }
            assert!(moved >= 4, "{moved}");
            assert!(g.report.contains("excluded locations hold only items that are not needed"));
        }
    }

    #[test]
    fn too_many_excluded_locations_stop_the_seed_with_a_reason() {
        let d: Data = Data::load().unwrap();
        let names: Vec<String> = eligible_names(&d).into_iter().filter(|n| n != FOREST_FIRST).collect();
        let list: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
        let mut s: Settings = all_chests();
        s.set("disabled_locations", Value::set(&list)).unwrap();
        let e: String = generate_data(&d, &s, "many", &GenOpts::default()).err().unwrap().to_string();
        // Hearts count as unneeded without a "hearts" goal, so the excluded spots may all find
        // such items and the dungeon items left over are what stop the seed instead.
        let fewer: bool = e.contains("too many locations are excluded") && e.contains("Exclude fewer");
        assert!(fewer || e.contains("cannot all fit the shuffled locations"), "{e}");
    }

    #[test]
    fn an_excluded_location_that_is_not_shuffled_keeps_its_item_and_says_so() {
        let d: Data = Data::load().unwrap();
        let mut s: Settings = with(&[("shuffle_chests", "off")]);
        s.set("disabled_locations", Value::set(&["KF Midos Top Left Chest"])).unwrap();
        let g: Generated = generate_data(&d, &s, "off", &GenOpts::default()).unwrap();
        assert!(g.report.contains("Excluded but not shuffled"), "{}", g.report);
    }

    #[test]
    fn unsupported_settings_stop_generation_with_a_message() {
        let d: Data = Data::load().unwrap();
        let mut s: Settings = Settings::default();
        s.set("shuffle_scrubs", Value::s("low")).unwrap();
        let e: String = generate_data(&d, &s, "x", &GenOpts::default()).err().unwrap().to_string();
        assert!(e.contains("shuffle_scrubs is low"));
    }

    #[test]
    fn spoiler_is_optional() {
        let d: Data = synthetic();
        let a: Generated = generate_data(&d, &all_chests(), "s", &GenOpts::default()).unwrap();
        assert!(a.spoiler.is_none());
        let o: GenOpts = GenOpts { spoiler: true, ..GenOpts::default() };
        let b: Generated = generate_data(&d, &all_chests(), "s", &o).unwrap();
        let t: String = b.spoiler.unwrap();
        assert!(t.contains("Randomized chests") && t.contains("Sphere 1"));
    }

    fn with(pairs: &[(&str, &str)]) -> Settings {
        let mut s: Settings = Settings::default();
        // Seeds made before Ocarina and Kokiri Sword shuffle had both shuffled.
        crate::slice::pick::shuffle_both(&mut s);
        for (k, v) in pairs {
            s.set(k, Value::s(v)).unwrap();
        }
        s
    }

    #[test]
    fn boss_hearts_join_the_pool_only_when_asked() {
        let d: Data = Data::load().unwrap();
        let mut no: Settings = Settings::default();
        no.set("shuffle_boss_hearts", Value::Bool(false)).unwrap();
        let off: Generated = generate_data(&d, &no, "hearts", &GenOpts::default()).unwrap();
        assert!(!off.seed_file.placements.contains_key("Deku Tree Queen Gohma Heart"));
        let mut s: Settings = Settings::default();
        s.set("shuffle_boss_hearts", Value::Bool(true)).unwrap();
        for n in 0..2 {
            let g: Generated = generate_data(&d, &s, &format!("hearts {n}"), &GenOpts::default()).unwrap();
            let hearts: Vec<&String> = g.seed_file.placements.keys().filter(|k| k.ends_with(" Heart")).collect();
            assert_eq!(hearts.len(), 8, "{hearts:?}");
            let held: usize = g.seed_file.placements.values().filter(|v| v.as_str() == "Heart Container").count();
            assert_eq!(held, 8);
        }
    }

    #[test]
    fn songs_stay_among_the_song_locations_in_own_mode() {
        let d: Data = Data::load().unwrap();
        let s: Settings = with(&[("shuffle_songs", "own")]);
        for n in 0..2 {
            let g: Generated = generate_data(&d, &s, &format!("songs {n}"), &GenOpts::default()).unwrap();
            for (loc, item) in &g.seed_file.placements {
                let song_loc: bool = d.song_ok(loc);
                let song_item: bool = d.item(item).and_then(|i| i.actor.as_deref()).is_some_and(is_song);
                assert_eq!(song_loc, song_item, "{loc} holds {item}");
            }
            let n_song: usize = g.seed_file.placements.keys().filter(|k| d.flow.by_name(k).is_some_and(|f| f.cat == "song")).count();
            // Bolero (Sheik in Crater) joined the song shuffle.
            assert_eq!(n_song, 12);
        }
    }

    #[test]
    fn songs_can_move_anywhere_in_anywhere_mode_and_every_seed_is_beatable() {
        let d: Data = Data::load().unwrap();
        let mut s: Settings = with(&[("shuffle_songs", "anywhere"), ("shuffle_rewards", "supported")]);
        s.set("shuffle_boss_hearts", Value::Bool(true)).unwrap();
        let mut out_of_place: usize = 0;
        for n in 0..2 {
            let g: Generated = generate_data(&d, &s, &format!("everything {n}"), &GenOpts::default()).unwrap();
            for (loc, item) in &g.seed_file.placements {
                let song_loc: bool = d.flow.by_name(loc).is_some_and(|f| f.cat == "song");
                let song_item: bool = d.item(item).and_then(|i| i.actor.as_deref()).is_some_and(is_song);
                if song_loc != song_item {
                    out_of_place += 1;
                }
            }
        }
        assert!(out_of_place > 0);
    }

    #[test]
    fn every_item_on_an_event_location_is_one_its_grant_gives() {
        let d: Data = Data::load().unwrap();
        let s: Settings = with(&[("shuffle_songs", "own"), ("shuffle_rewards", "supported")]);
        for n in 0..3 {
            let g: Generated = generate_data(&d, &s, &format!("dungeon items {n}"), &GenOpts::default()).unwrap();
            for (loc, item) in &g.seed_file.placements {
                if d.flow.by_name(loc).is_some() {
                    let fits = |i: &crate::slice::data::Item| crate::slice::flow_rules::gift_takes(&d, i) || d.flow_accepts(i);
                    assert!(d.item(item).is_some_and(fits) || d.vanilla_of(loc) == Some(item.as_str()), "{loc} holds {item}");
                }
            }
        }
    }

    #[test]
    fn a_start_reward_changes_the_logic_and_still_generates() {
        let d: Data = Data::load().unwrap();
        let s: Settings = with(&[("start_reward", "any")]);
        let g: Generated = generate_data(&d, &s, "start reward", &GenOpts::default()).unwrap();
        assert!(g.report.contains("start the game with one random"));
        assert_eq!(g.counts.randomized, g.seed_file.placements.len());
    }

    #[test]
    fn an_upgrade_on_a_boss_heart_stands_as_the_pickup_of_a_fixed_tier() {
        let d: Data = Data::load().unwrap();
        let mut s: Settings = Settings::default();
        s.set("shuffle_boss_hearts", Value::Bool(true)).unwrap();
        for n in 0..2 {
            let g: Generated = generate_data(&d, &s, &format!("up {n}"), &GenOpts::default()).unwrap();
            let order: crate::slice::tier_order::Verdict = crate::slice::tier_order::fixed_actors(&d, &g.seed_file).unwrap();
            for (loc, item) in &g.seed_file.placements {
                if !d.cat.by_name(loc).is_some_and(|c| c.kind == "bossheart") {
                    continue;
                }
                let Some(f) = d.family(item) else {
                    continue;
                };
                assert!(crate::slice::key_spots::placeable(&d, item, crate::slice::key_spots::Spot::Free), "{loc} holds {item}");
                let tier: Option<&String> = order.assign.get(loc);
                assert!(tier.is_some_and(|t| crate::slice::tier_order::rank(f, t).is_some()), "{loc} holds {item} as {tier:?}");
            }
        }
    }

    #[test]
    fn a_boss_heart_takes_a_token() {
        let d: Data = Data::load().unwrap();
        let mut s: Settings = Settings::default();
        s.set("shuffle_boss_hearts", Value::Bool(true)).unwrap();
        let p: Plan = plan(&s).unwrap();
        let w: World = world(&d).unwrap();
        let t: ItemId = w.item("Gold Skulltula Token").unwrap();
        let hearts: Vec<&str> = d.cat.locs.iter().filter(|c| c.kind == "bossheart").map(|c| c.name.as_str()).collect();
        assert!(!hearts.is_empty());
        for n in hearts {
            if let Some(l) = w.loc(n) {
                assert!(allowed(&d, &w, &p, t, l), "{n}");
            }
        }
    }

    fn plan_of(list: &[&str]) -> GenOpts {
        GenOpts { plando: list.iter().map(|x| crate::slice::plando::parse(x).unwrap()).collect(), ..GenOpts::default() }
    }

    #[test]
    fn a_plando_entry_is_honoured_and_every_seed_stays_beatable() {
        let d: Data = Data::load().unwrap();
        for n in 0..4 {
            let o: GenOpts = plan_of(&["KF Midos Top Left Chest=Boomerang", "KF Midos Top Right Chest=Bomb Bag"]);
            let r: Res<Generated> = generate_data(&d, &all_chests(), &format!("plando {n}"), &o);
            let g: Generated = match r {
                Ok(g) => g,
                Err(e) => panic!("{e}"),
            };
            let p = &g.seed_file.placements;
            assert_eq!(p.get("KF Midos Top Left Chest").map(|s| s.as_str()), Some("Boomerang"));
            assert_eq!(p.get("KF Midos Top Right Chest").map(|s| s.as_str()), Some("Bomb Bag"));
            assert!(g.report.contains("Plandomizer: 2"));
        }
    }

    #[test]
    fn bad_plando_entries_are_refused_with_a_reason() {
        let d: Data = Data::load().unwrap();
        let go = |s: &Settings, l: &[&str]| generate_data(&d, s, "p", &plan_of(l)).err().map(|e| e.to_string()).unwrap_or_default();
        assert!(go(&all_chests(), &["KF Midos Top Left Chest=Not An Item"]).contains("no item is named"));
        let m: String = go(&all_chests(), &["Midos Top Left=Boomerang"]);
        assert!(m.contains("no location is named") && m.contains("KF Midos Top Left Chest"), "{m}");
        assert!(go(&all_chests(), &["KF Midos Top Left Chest=Boomerang", "KF Midos Top Right Chest=Boomerang"]).contains("no copy left"));
        assert!(go(&all_chests(), &["KF Midos Top Left Chest=Boomerang", "KF Midos Top Left Chest=Deku Shield"]).contains("more than once"));
        assert!(go(&with(&[("shuffle_chests", "off")]), &["KF Midos Top Left Chest=Boomerang"]).contains("not shuffled"));
    }

    #[test]
    fn the_gerudo_membership_card_follows_its_setting() {
        let d: Data = Data::load().unwrap();
        let free = |s: &Settings| -> bool {
            let p: Plan = plan(s).unwrap();
            let w: World = world_with(&d, &p.logic()).unwrap();
            crate::slice::pick::pick(&d, &w, &p).free.iter().any(|l| w.locs[*l].name == crate::flow_extra::GERUDO_CARD_LOC)
        };
        assert!(free(&Settings::default()));
        let mut off: Settings = Settings::default();
        off.set(crate::flow_extra::GERUDO_CARD, Value::Bool(false)).unwrap();
        assert!(!free(&off));
        let g: Generated = generate_data(&d, &off, "vanilla card", &GenOpts::default()).unwrap();
        assert!(!g.seed_file.placements.contains_key(crate::flow_extra::GERUDO_CARD_LOC));
        let mut fast: Settings = Settings::default();
        fast.set("gerudo_fortress", Value::s("fast")).unwrap();
        assert!(!free(&fast));
    }

    #[test]
    fn the_ocarina_and_kokiri_sword_stay_vanilla_unless_shuffled() {
        let d: Data = Data::load().unwrap();
        let free = |s: &Settings| -> Vec<String> {
            let p: Plan = plan(s).unwrap();
            let w: World = world_with(&d, &p.logic()).unwrap();
            crate::slice::pick::pick(&d, &w, &p).free.iter().map(|l| w.locs[*l].name.clone()).collect()
        };
        let spots = [crate::slice::flow_rules::SARIA, crate::slice::flow_rules::OCARINA_OF_TIME, crate::slice::pick::SWORD_CHEST];
        let off: Vec<String> = free(&Settings::default());
        assert!(spots.iter().all(|x| !off.iter().any(|n| n == x)), "{off:?}");
        let g: Generated = generate_data(&d, &Settings::default(), "vanilla gifts", &GenOpts::default()).unwrap();
        assert!(spots.iter().all(|x| !g.seed_file.placements.contains_key(*x)));
        let mut on: Settings = Settings::default();
        on.set(crate::slice::pick::OCARINAS, Value::Bool(true)).unwrap();
        on.set(crate::slice::pick::SWORD, Value::Bool(true)).unwrap();
        let on: Vec<String> = free(&on);
        assert!(spots.iter().all(|x| on.iter().any(|n| n == x)), "{on:?}");
    }

    #[test]
    fn an_open_forest_generates_and_can_put_progression_past_mido() {
        let d: Data = Data::load().unwrap();
        for kak in ["closed", "open"] {
            let s: Settings = with(&[("open_forest", "open"), ("open_kakariko", kak)]);
            for n in 0..3 {
                let g: Generated = generate_data(&d, &s, &format!("forest open {kak} {n}"), &GenOpts::default()).unwrap();
                assert!(g.report.contains("Kokiri Forest starts open"));
            }
        }
    }

    #[test]
    fn an_open_kakariko_only_places_giveable_items_at_the_skipped_checks() {
        let d: Data = Data::load().unwrap();
        let s: Settings = with(&[("shuffle_rewards", "supported"), ("shuffle_songs", "anywhere"), ("open_forest", "open"), ("open_kakariko", "open")]);
        for n in 0..3 {
            let g: Generated = generate_data(&d, &s, &format!("kak {n}"), &GenOpts::default()).unwrap();
            assert!(g.report.contains("Kakariko gate starts open"));
            let p: Plan = plan(&s).unwrap();
            let locs: Vec<&str> = crate::slice::edit::skipped_locations(&p);
            assert_eq!(locs.len(), 2);
            for loc in &locs {
                if let Some(item) = g.seed_file.placements.get(*loc) {
                    assert!(d.item(item).is_some_and(|i| i.actor.is_some() || !i.tiers.is_empty()), "seed kak {n}: {loc} holds {item}");
                }
            }
            let state = crate::slice::edit::start_state(&d, &g.seed_file, &p).unwrap();
            assert!(state.items.len() >= 1 + 2, "{:?}", state.items);
            // The castle visit skip, plus the Deku Tree quest an open forest moves on.
            assert_eq!(state.steps.len(), 9);
            if let Some(item) = g.seed_file.placements.get("HC Malon Egg") {
                assert!(!d.item(item).is_some_and(|i| i.adv), "seed kak {n}: the egg check holds {item}");
            }
        }
        let off: Settings = with(&[("open_kakariko", "closed")]);
        let g: Generated = generate_data(&d, &off, "kak off", &GenOpts::default()).unwrap();
        assert!(!g.report.contains("Kakariko gate starts open"));
        let p: Plan = plan(&off).unwrap();
        assert!(crate::slice::edit::skipped_locations(&p).is_empty());
        // Only flags remain: the intro skip's first-visit scenes and the always-on child Temple
        // of Time skip (crate::story_skip::temple). No quest step, state or item.
        let off_state: crate::start_state::Ops = crate::slice::edit::start_state(&d, &g.seed_file, &p).unwrap();
        assert!(off_state.steps.is_empty() && off_state.enums.is_empty() && off_state.items.is_empty());
        let seen: usize = off_state.flags.iter().filter(|(n, _)| n.starts_with(crate::start_state::VISIT)).count();
        assert_eq!(seen, 13);
        assert_eq!(off_state.flags.len(), crate::start_state::FIRST_VISIT.len() + crate::story_skip::TEMPLE_FLAGS.len() + crate::story_skip::OWL_FLAGS.len() + crate::key_log::start_ops().flags.len() + seen);
        // Zelda's Letter is in the pool and may go anywhere: a skipped check hands out what the
        // seed put there, an unshuffled one its vanilla item.
        let mut bare: SeedFile = g.seed_file.clone();
        for l in crate::start_state::KAKARIKO_LOCATIONS {
            bare.placements.remove(l);
        }
        let vanilla: Vec<String> = crate::slice::edit::skipped_items(&d, &bare, &crate::start_state::KAKARIKO_LOCATIONS).unwrap();
        assert_eq!(vanilla, vec!["Item_ZeldaLetter", "Item_Melody_ZeldaLullaby"]);
        bare.placements.insert("HC Zeldas Letter".to_string(), "Boomerang".to_string());
        let moved: Vec<String> = crate::slice::edit::skipped_items(&d, &bare, &crate::start_state::KAKARIKO_LOCATIONS).unwrap();
        assert_eq!(moved, vec!["Item_Boomerang", "Item_Melody_ZeldaLullaby"]);
    }

    #[test]
    fn two_progressive_ocarinas_given_at_the_start_become_both_tiers() {
        let d: Data = Data::load().unwrap();
        let mut g: Generated = generate_data(&d, &Settings::default(), "tiers", &GenOpts::default()).unwrap();
        g.seed_file.placements.insert("LW Gift from Saria".to_string(), "Ocarina".to_string());
        g.seed_file.placements.insert("HF Ocarina of Time Item".to_string(), "Ocarina".to_string());
        let given: Vec<String> = crate::slice::edit::skipped_items(&d, &g.seed_file, &["LW Gift from Saria", "HF Ocarina of Time Item"]).unwrap();
        assert_eq!(given, vec!["Item_Ocarina_Fairy", "Item_Ocarina_Time"]);
    }

    #[test]
    fn the_door_of_time_is_closed_by_default_and_can_be_opened() {
        let d: Data = Data::load().unwrap();
        let g: Generated = generate_data(&d, &Settings::default(), "door off", &GenOpts::default()).unwrap();
        assert!(!g.report.contains("Door of Time starts open"));
        let mut s: Settings = Settings::default();
        s.set("open_door_of_time", Value::Bool(true)).unwrap();
        for n in 0..2 {
            let g: Generated = generate_data(&d, &s, &format!("door {n}"), &GenOpts::default()).unwrap();
            assert!(g.report.contains("Door of Time starts open"));
        }
    }

    #[test]
    fn disabled_locations_match_without_case_and_typos_are_refused() {
        let d: Data = Data::load().unwrap();
        let mut s: Settings = all_chests();
        s.set("disabled_locations", Value::set(&["kf midos top left chest"])).unwrap();
        let g: Generated = generate_data(&d, &s, "dis", &GenOpts::default()).unwrap();
        assert!(g.seed_file.placements.get("KF Midos Top Left Chest").is_none_or(|i| HEARTS.iter().any(|h| i.starts_with(h)) || !d.item(i).is_some_and(|x| x.adv)));
        s.set("disabled_locations", Value::set(&["Midos Top Left"])).unwrap();
        let e: String = generate_data(&d, &s, "dis", &GenOpts::default()).err().unwrap().to_string();
        assert!(e.contains("disabled locations: no location is named") && e.contains("KF Midos Top Left Chest"), "{e}");
    }

    fn digest(p: &BM<String, String>) -> u64 {
        let mut h: u64 = 0xCBF2_9CE4_8422_2325;
        for (k, v) in p {
            for b in format!("{k}\t{v}\n").as_bytes() {
                h ^= *b as u64;
                h = h.wrapping_mul(0x0000_0100_0000_01B3);
            }
        }
        h
    }

    #[test]
    fn seeds_that_generated_before_keep_their_exact_placements() {
        let d: Data = Data::load().unwrap();
        let cases: [(&[(&str, &str)], &str); 4] = [
            (&[], "default 0"),
            (&[], "default 1"),
            (&[("shuffle_chests", "overworld")], "chests_ow 0"),
            (&[("shuffle_chests", "dungeons")], "dungeons 0"),
        ];
        let mut got: BM<String, String> = BM::new();
        for (pairs, seed) in cases {
            let mut all: Vec<(&str, &str)> = vec![("shuffle_rewards", "off")];
            all.extend_from_slice(pairs);
            let mut s: Settings = with(&all);
            s.set(crate::open_world::TIME, Value::Bool(false)).unwrap();
            let g: Generated = generate_data(&d, &s, seed, &GenOpts::default()).unwrap();
            got.insert(seed.to_string(), format!("{} {:016x}", g.seed_file.placements.len(), digest(&g.seed_file.placements)));
        }
        crate::slice::pins::verify("old_seeds", &got);
    }

    #[test]
    fn a_setting_that_cannot_fit_fails_at_once_with_a_plain_message() {
        let d: Data = Data::load().unwrap();
        let s: Settings = with(&[("shuffle_chests", "dungeons"), ("shuffle_bosskeys", "overworld")]);
        let t0: std::time::Instant = std::time::Instant::now();
        let e: String = generate_data(&d, &s, "x", &GenOpts::default()).err().unwrap().to_string();
        assert!(e.contains("shuffle_bosskeys is overworld while shuffle_chests is dungeons"), "{e}");
        assert!(e.contains("Set Chests to Everywhere"), "{e}");
        assert!(t0.elapsed() < std::time::Duration::from_secs(20));
    }

    #[test]
    fn boss_keys_anywhere_with_shuffled_pots_generate_because_the_starting_area_keeps_a_spot() {
        let d: Data = Data::load().unwrap();
        let s: Settings = with(&[
            ("shuffle_chests", "all"),
            ("shuffle_pots", "all"),
            ("shuffle_crates", "all"),
            ("shuffle_bosskeys", "keysanity"),
        ]);
        let p: Plan = plan(&s).unwrap();
        let w: World = world_with(&d, &p.logic()).unwrap();
        let o: GenOpts = GenOpts { tries: 3, ..GenOpts::default() };
        for seed in ["a", "b", "c"] {
            let g: Generated = generate_data(&d, &s, seed, &o).unwrap_or_else(|e| panic!("{seed}: {e}"));
            assert!(rebuilt(&d, &p, &g.seed_file.placements));
            for (loc, item) in &g.seed_file.placements {
                assert!(allowed(&d, &w, &p, w.item(item).unwrap(), w.loc(loc).unwrap()), "{item} at {loc}");
            }
        }
    }

    fn xcykrix() -> Settings {
        let mut s: Settings = with(&[
            ("open_forest", "open"),
            ("shuffle_rewards", "supported"),
            ("shuffle_songs", "anywhere"),
            ("shuffle_skulltulas", "matched"),
        ]);
        for k in ["shuffle_boss_hearts", "gossip_hints", "faster_chests"] {
            s.set(k, Value::Bool(true)).unwrap();
        }
        s
    }

    #[test]
    fn the_careful_fill_is_deterministic_and_its_seeds_are_valid() {
        let d: Data = Data::load().unwrap();
        let s: Settings = xcykrix();
        let p: Plan = plan(&s).unwrap();
        let w: World = world_with(&d, &p.logic()).unwrap();
        let o: GenOpts = GenOpts { switch: 0, spoiler: true, ..GenOpts::default() };
        let a: Generated = generate_data(&d, &s, "careful", &o).unwrap();
        let b: Generated = generate_data(&d, &s, "careful", &o).unwrap();
        assert_eq!(a.seed_file.to_text(), b.seed_file.to_text());
        assert_eq!(a.spoiler, b.spoiler);
        assert!(rebuilt(&d, &p, &a.seed_file.placements));
        for (loc, item) in &a.seed_file.placements {
            assert!(allowed(&d, &w, &p, w.item(item).unwrap(), w.loc(loc).unwrap()), "{item} at {loc}");
        }
        assert!(a.spoiler.unwrap().contains("Sphere 1"));
        let c: Generated = generate_data(&d, &s, "careful other", &o).unwrap();
        assert_ne!(a.seed_file.placements, c.seed_file.placements);
    }

    #[test]
    fn the_time_limit_and_a_cancel_end_the_run_with_a_message() {
        let d: Data = Data::load().unwrap();
        let o: GenOpts = GenOpts { limit: Some(Duration::ZERO), ..GenOpts::default() };
        let e: String = generate_data(&d, &xcykrix(), "slow", &o).err().unwrap().to_string();
        assert!(e.contains("time limit of 0 seconds") && e.contains("set Gold Skulltulas to Confident matches"), "{e}");
        let l: Arc<Live> = Arc::new(Live::default());
        l.cancel();
        let c: GenOpts = GenOpts { live: Some(Arc::clone(&l)), ..GenOpts::default() };
        let e: String = generate_data(&d, &Settings::default(), "stop", &c).err().unwrap().to_string();
        assert_eq!(e, crate::slice::watch::STOPPED);
        let r: Generated = generate_data(&d, &Settings::default(), "free", &GenOpts { limit: Some(LIMIT), ..GenOpts::default() }).unwrap();
        let n: Generated = generate_data(&d, &Settings::default(), "free", &GenOpts::default()).unwrap();
        assert_eq!(r.seed_file.to_text(), n.seed_file.to_text());
    }

    #[test]
    fn an_empty_scan_is_refused() {
        let scan: &str = "{\"format\":1,\"game\":\"g\",\"files\":[],\"chests\":[]}";
        let d: Data = Data::new(ITEMS_JSON, VANILLA_JSON, scan, CATALOG_JSON).unwrap().with_remake(REMAKE_JSON).unwrap();
        assert!(generate_data(&d, &all_chests(), "x", &GenOpts::default()).is_err());
    }

    const HELD: [(&str, &str); 9] = [
        ("ToT Reward from Rauru", "Light Medallion"),
        ("Queen Gohma", "Kokiri Emerald"),
        ("King Dodongo", "Goron Ruby"),
        ("Barinade", "Zora Sapphire"),
        ("Phantom Ganon", "Forest Medallion"),
        ("Volvagia", "Fire Medallion"),
        ("Morpha", "Water Medallion"),
        ("Bongo Bongo", "Shadow Medallion"),
        ("Twinrova", "Spirit Medallion"),
    ];

    fn held_loc(name: &str) -> bool {
        HELD.iter().any(|h| h.0 == name)
    }

    fn held_item(name: &str) -> bool {
        HELD.iter().any(|h| h.1 == name)
    }

    #[test]
    fn the_held_locations_are_the_nine_dungeon_reward_events() {
        let d: Data = Data::load().unwrap();
        let mut got: Vec<(&str, &str)> = d
            .flow
            .locs
            .iter()
            .filter(|l| l.cat == crate::slice::support::DUNGEON_REWARD_CAT)
            .map(|l| (l.name.as_str(), l.vanilla.as_str()))
            .collect();
        let mut want: Vec<(&str, &str)> = HELD.to_vec();
        got.sort();
        want.sort();
        assert_eq!(got, want);
        for (l, i) in HELD {
            assert_eq!(d.vanilla_of(l), Some(i), "{l}");
        }
    }

    #[test]
    fn stones_and_medallions_stay_with_their_bosses_by_default() {
        let d: Data = Data::load().unwrap();
        let mut need: usize = 0;
        for (s, tag) in [(Settings::default(), "default"), (with(&[("shuffle_chests", "overworld")]), "overworld")] {
            assert!(plan(&s).unwrap().rewards.on);
            assert!(!plan(&s).unwrap().rewards.dungeon_rewards);
            for n in 0..3 {
                let g: Generated = generate_data(&d, &s, &format!("held {tag} {n}"), &GenOpts { spoiler: true, ..GenOpts::default() }).unwrap();
                assert!(g.seed_file.placements.contains_key("Kak Man on Roof"), "{tag} {n}");
                for (loc, item) in &g.seed_file.placements {
                    assert!(!held_loc(loc), "{tag} {n} shuffled {loc}");
                    assert!(!held_item(item), "{tag} {n} put {item} at {loc}");
                }
                for (loc, item) in g.spheres.iter().flatten() {
                    if held_item(item) {
                        assert!(HELD.contains(&(loc.as_str(), item.as_str())), "{loc} {item}");
                        need += 1;
                    }
                }
            }
        }
        assert!(need > 0);
    }

    #[test]
    fn stones_and_medallions_move_when_asked() {
        let d: Data = Data::load().unwrap();
        let mut s: Settings = Settings::default();
        s.set("shuffle_dungeon_rewards", Value::s("anywhere")).unwrap();
        assert!(plan(&s).unwrap().rewards.dungeon_rewards);
        let mut moved: usize = 0;
        for n in 0..3 {
            let g: Generated = generate_data(&d, &s, &format!("moved {n}"), &GenOpts::default()).unwrap();
            for (l, _) in HELD {
                assert!(g.seed_file.placements.contains_key(l), "{n} {l}");
            }
            moved += g.seed_file.placements.iter().filter(|(l, i)| held_item(i) && !held_loc(l)).count();
        }
        assert!(moved > 0);
    }

    #[test]
    fn end_of_dungeons_shuffles_stones_and_medallions_only_among_their_events() {
        let d: Data = Data::load().unwrap();
        let mut s: Settings = Settings::default();
        s.set("shuffle_dungeon_rewards", Value::s("dungeons")).unwrap();
        let p: Plan = plan(&s).unwrap();
        assert!(p.rewards.dungeon_rewards && p.rewards.dungeon_own);
        let mut moved: usize = 0;
        for n in 0..4 {
            let g: Generated = generate_data(&d, &s, &format!("end of dungeons {n}"), &GenOpts::default()).unwrap();
            let mut got: Vec<&str> = Vec::new();
            for (l, _) in HELD {
                let item: &String = g.seed_file.placements.get(l).unwrap_or_else(|| panic!("{n} {l} is not shuffled"));
                assert!(held_item(item), "{n} {l} holds {item}");
                got.push(item.as_str());
                moved += usize::from(d.vanilla_of(l) != Some(item.as_str()));
            }
            got.sort();
            let mut want: Vec<&str> = HELD.iter().map(|h| h.1).collect();
            want.sort();
            assert_eq!(got, want, "{n}");
            for (l, i) in &g.seed_file.placements {
                assert!(!held_item(i) || held_loc(l), "{n} put {i} at {l}");
            }
        }
        assert!(moved > 0);
    }

    #[test]
    fn rewards_off_and_stones_and_medallions_on_keep_the_old_seeds() {
        let d: Data = Data::load().unwrap();
        let mut on: Settings = Settings::default();
        crate::slice::pick::shuffle_both(&mut on);
        on.set("shuffle_dungeon_rewards", Value::s("anywhere")).unwrap();
        let mut pots: Settings = on.clone();
        pots.set("shuffle_pots", Value::s("all")).unwrap();
        pots.set("shuffle_crates", Value::s("all")).unwrap();
        let cases: [(Settings, &str, &str); 8] = [
            (with(&[("shuffle_rewards", "off")]), "OOTRR1-XZLA", "pin a"),
            (with(&[("shuffle_rewards", "off")]), "OOTRR1-XZLA", "pin b"),
            (with(&[("shuffle_rewards", "off"), ("shuffle_chests", "overworld")]), "OOTRR1-BAAQCQ5V", "pin a"),
            (with(&[("shuffle_rewards", "off"), ("shuffle_chests", "overworld")]), "OOTRR1-BAAQCQ5V", "pin b"),
            (on.clone(), "OOTRR1-EYAQD32I", "pin a"),
            (on.clone(), "OOTRR1-EYAQD32I", "pin b"),
            (pots.clone(), "OOTRR1-BEAQGCQBAMTACAK2JA", "pin a"),
            (pots.clone(), "OOTRR1-BEAQGCQBAMTACAK2JA", "pin b"),
        ];
        let mut got: BM<String, String> = BM::new();
        for (mut s, old, seed) in cases {
            s.set(crate::open_world::TIME, Value::Bool(false)).unwrap();
            s.set(crate::deku_bridge::KEY, Value::Bool(false)).unwrap();
            // The defaults these strings were written with.
            s.set("shuffle_boss_hearts", Value::Bool(false)).unwrap();
            s.set(crate::flow_extra::SETTING, Value::Bool(false)).unwrap();
            s.set(crate::qol::NAVI, Value::s("off")).unwrap();
            let g: Generated = generate_data(&d, &s, seed, &GenOpts::default()).unwrap();
            let mut f: SeedFile = g.seed_file.clone();
            if s.text("shuffle_dungeon_rewards").unwrap() != "vanilla" {
                assert_ne!(f.settings_string, old);
            } else {
                assert_eq!(f.settings_string, old);
            }
            f.settings_string = old.to_string();
            got.insert(format!("{old} {seed}"), f.checksum());
        }
        crate::slice::pins::verify("rewards_off_and_stones", &got);
    }

    #[test]
    fn keysanity_with_crates_places_the_kokiri_sword_once() {
        let d: Data = Data::load().unwrap();
        let s: Settings = with(&[("shuffle_smallkeys", "keysanity"), ("shuffle_crates", "all")]);
        let o: GenOpts = GenOpts { tries: 20, ..GenOpts::default() };
        let g: Generated = generate_data(&d, &s, "a", &o).unwrap_or_else(|e| panic!("{e}"));
        let at: Vec<&String> = g.seed_file.placements.iter().filter(|(_, i)| i.as_str() == "Kokiri Sword").map(|(l, _)| l).collect();
        assert_eq!(at.len(), 1, "{at:?}");
    }

    #[test]
    fn beehives_on_seeds_are_pinned() {
        let d: Data = Data::load().unwrap();
        let mut hives: Settings = Settings::default();
        crate::slice::pick::shuffle_both(&mut hives);
        hives.set(crate::pot_slice::plan::HIVES, Value::Bool(true)).unwrap();
        let mut both: Settings = hives.clone();
        both.set("shuffle_pots", Value::s("all")).unwrap();
        both.set("shuffle_crates", Value::s("all")).unwrap();
        let cases: [(Settings, &str, &str); 4] = [
            (hives.clone(), "hives", "pin a"),
            (hives.clone(), "hives", "pin b"),
            (both.clone(), "hives pots and crates", "pin a"),
            (both.clone(), "hives pots and crates", "pin b"),
        ];
        let mut got: BM<String, String> = BM::new();
        for (s, name, seed) in cases {
            let g: Generated = generate_data(&d, &s, seed, &GenOpts::default()).unwrap();
            let mut f: SeedFile = g.seed_file.clone();
            f.settings_string = "pinned".to_string();
            assert_eq!(f.placements.keys().filter(|k| k.starts_with("Kak Hole")).count(), 7);
            got.insert(format!("{name} {seed}"), f.checksum());
        }
        crate::slice::pins::verify("beehives", &got);
    }

    #[test]
    fn the_pool_keeps_every_ootr_copy_of_the_bow_bomb_bag_and_slingshot() {
        let d: Data = Data::load().unwrap();
        let g: Generated = generate_data(&d, &Settings::default(), "upgrade copies", &GenOpts::default()).unwrap();
        let count = |n: &str| g.seed_file.placements.values().filter(|v| v.as_str() == n).count();
        for (n, k) in [("Bow", 3), ("Bomb Bag", 3), ("Slingshot", 3), ("Magic Meter", 2), ("Progressive Hookshot", 2)] {
            assert_eq!(count(n), k, "{n}");
        }
        for l in ["Kak Shooting Gallery Reward", "GF HBA 1500 Points", "Market Bombchu Bowling First Prize"] {
            assert_eq!(d.vanilla_of(l), Some("Rupees (5)"), "{l}");
        }
        let t: String = generate_data(&d, &Settings::default(), "upgrade copies", &GenOpts { spoiler: true, ..GenOpts::default() }).unwrap().spoiler.unwrap();
        let chests: &str = t.split("Randomized chests").nth(1).unwrap().split("Playthrough").next().unwrap();
        for (n, k) in [("Progressive Bow", 3), ("Progressive Bomb Bag", 3), ("Progressive Slingshot", 3), ("Progressive Magic Meter", 2)] {
            assert_eq!(chests.lines().filter(|x| x.split(" (vanilla").next().is_some_and(|h| h.ends_with(&format!(": {n}")))).count(), k, "{n}");
        }
        for old in [": Bow", ": Bomb Bag", ": Slingshot", ": Magic Meter", "(vanilla Bow)", "(vanilla Magic Meter)"] {
            assert!(!t.lines().any(|x| x.ends_with(old) || x.contains(&format!("{old} ("))), "{old}");
        }
    }
}
