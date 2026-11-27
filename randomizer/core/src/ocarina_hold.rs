//! Lets Link hold up the items that have no animation: the two Ocarinas and the songs.
//!
//! Every gift that has Link hold the item up (each Dm_SY_GetInEvent entry reaches :Item_Position)
//! shows the item with Player_Event.EventIgnitionShowItem, IsAsPlay true and ASCommand
//! Rotate_GetDemo, which plays that animation on the actor it was handed. The Ocarina item actors
//! (equipment accessories) and the song actors have no animation at all, so the game crashed.
//! Giving the Ocarina actors the item animation components still crashed in the game.
//!
//! The game's own hold up gifts also hand out pickup actors: the Death Mountain Crater scrub's
//! sale gives Obj_Heart_Piece. A pickup actor is built on Obj_Item_Base with the shared Item_Common
//! animation, and its PouchContentParam names the item it adds (PouchActor). So where Link holds
//! one of these items up, he is handed its pickup actor from the pickup catalog (Obj_Ocarina_Fairy,
//! Obj_Ocarina_TimeLand, Obj_Melody_* on Sheik's harp model). A progressive chain still asks about
//! the item actors and each branch hands out the matching pickup actor.
//!
//! Adding the pickup actor puts the item in the save, but the running game only sets an Ocarina up
//! for the D-pad after a reload. Saria's gift adds the Ocarina item itself, which sets it up at
//! once, so the gift then adds the item the same way (no window) and, for an Ocarina, writes the
//! equip slot as the starting items do ([`after`]).
//!
//! This covers the NPC gifts that hold the item up (slice::flow_rules::holds_up) and the reward
//! scenes the story skips turn into the standard item get (story_skip::item_get, Sheik's songs
//! included through the song triggers), for a placed item or the scene's own. The Great Fairies give
//! with their own routine and Requiem keeps its scene's window, so the NPC gift rule covers them. The seed build writes the pickup packs
//! all of those can hand out.

use std::sync::OnceLock;

use crate::bfevfl::{Cont, Flow, Kind, Val};
use crate::error::{Error, Res};
use crate::settings::seed::SeedFile;
use crate::slice::data::Data;
use crate::slice::key_spots::{pick, Pick};

/// The Ocarina item actors.
pub const OCARINAS: [&str; 2] = ["Item_Ocarina_Fairy", "Item_Ocarina_Time"];
const SONG: &str = "Item_Melody_";

/// The player's equipped Ocarina. Adding the pickup actor gives the Ocarina but leaves this empty,
/// so the D-pad cannot play it; the starting items write it the same way (start_items::give).
pub const EQUIP: &str = "PlayerStatus.EquipOcarina";

pub const NOTE: &str = "An Ocarina or a song is handed to Link with the item get that holds the item up, so it is handed out as its pickup actor (those items have no item animation), then added like Saria's gift, and its pickup pack is written into the mod. Experimental.";

/// Whether `actor` has no animation of its own, so a hold up cannot show it.
pub fn no_anim(actor: &str) -> bool {
    OCARINAS.contains(&actor) || actor.starts_with(SONG)
}

/// The game data the gift writes after handing out `tier`'s pickup actor, if any.
pub fn equip_key(tier: &str) -> Option<&'static str> {
    OCARINAS.contains(&tier).then_some(EQUIP)
}

/// The pickup actor a hold up hands out in place of `actor`, if `actor` cannot be shown.
pub fn shown(actor: &str) -> Option<String> {
    if !no_anim(actor) {
        return None;
    }
    pick(actor).map(|p| p.pack)
}

/// Every item actor with no animation that has a pickup pack, and that pack.
fn table() -> &'static Vec<(String, String)> {
    static T: OnceLock<Vec<(String, String)>> = OnceLock::new();
    T.get_or_init(|| {
        crate::pickup_pack::catalog::load()
            .unwrap_or_default()
            .into_iter()
            .filter(|e| no_anim(&e.item))
            .filter_map(|e| shown(&e.item).map(|p| (e.item, p)))
            .collect()
    })
}

pub fn chest_form(actor: &str) -> String {
    if OCARINAS.contains(&actor) {
        return shown(actor).unwrap_or_else(|| actor.to_string());
    }
    actor.to_string()
}

pub fn in_chest(d: &Data, loc: &str) -> bool {
    loc != crate::slice::treasure::LOC && !crate::slice::chest_targets::is_sale(d, loc) && d.cat.by_name(loc).is_some_and(|l| l.kind == "chest")
}

/// The item actor whose pickup actor `form` is.
pub fn tier_of(form: &str) -> Option<&'static str> {
    table().iter().find(|(_, p)| p == form).map(|(i, _)| i.as_str())
}

/// Whether `actor` is one of the pickup actors handed out in place of such an item.
pub fn is_form(actor: &str) -> bool {
    tier_of(actor).is_some()
}

fn holds_up(d: &Data, loc: &str) -> bool {
    // Cow metadata describes the vanilla bottle call, not the shuffled hold-up grant.
    crate::flow_extra::cows::is(loc) || d.flow.by_name(loc).is_some_and(crate::slice::flow_rules::holds_up)
}

/// The actor `loc` hands out for `actor`: its pickup actor at a hold up gift.
pub fn shown_at(d: &Data, loc: &str, actor: &str) -> Option<String> {
    if !holds_up(d, loc) {
        return None;
    }
    shown(actor)
}

/// The actors each tier's branch of a progressive chain that holds the item up hands out, or
/// nothing when the tiers themselves can be shown.
pub fn forms_of(family: &str) -> Vec<String> {
    let Ok(t) = crate::progressive::tiers(family) else {
        return Vec::new();
    };
    if !t.iter().any(|a| no_anim(a)) {
        return Vec::new();
    }
    t.iter().map(|a| shown(a).unwrap_or_else(|| a.to_string())).collect()
}

/// forms_of, for the chain of an NPC gift at `loc` (nothing where the gift shows a window).
pub fn forms(d: &Data, loc: &str, family: &str) -> Vec<String> {
    if !holds_up(d, loc) {
        return Vec::new();
    }
    forms_of(family)
}

/// Adds, after the gift handed out the pickup actor of `tier`, the tier item like Saria's gift
/// (no window, no equip) and, for an Ocarina, the equip slot. Returns the first added event,
/// whose chain ends at `then`.
pub fn after(f: &mut Flow, tier: &str, then: u16) -> Res<u16> {
    let push = |f: &mut Flow, k: Kind| -> Res<u16> {
        let i: u16 = u16::try_from(f.events.len()).map_err(|_| Error::Format("ocarina: the flow has too many events"))?;
        f.events.push(crate::bfevfl::Event { name: format!("OOTRRHeldAdd{i}"), kind: k });
        Ok(i)
    };
    let mut next: u16 = then;
    if let Some(key) = equip_key(tier) {
        let (da, dx): (u16, u16) = crate::start_state::slot(f, "GameDataActor", "EventTriggerSetGameDataString64")?;
        let items: Vec<(String, Val)> = vec![("GameDataName".to_string(), Val::Str(key.to_string())), ("Value".to_string(), Val::Str(tier.to_string())), ("Index".to_string(), Val::Int(-1))];
        next = push(f, Kind::Action { next, actor: da, act: dx, params: Some(Cont { items }) })?;
    }
    let (pa, px): (u16, u16) = crate::start_state::slot(f, "PouchSystemActor", "EventTriggerAddOrRemovePouchContent")?;
    let add: Vec<(String, Val)> = vec![
        ("Num".to_string(), Val::Int(1)),
        ("IsEquipDIrect".to_string(), Val::Bool(0)),
        ("IsSetMessageTargetActorName".to_string(), Val::Bool(0)),
        ("Actor".to_string(), Val::Str(tier.to_string())),
        ("IsGetEvent".to_string(), Val::Bool(0)),
    ];
    push(f, Kind::Action { next, actor: pa, act: px, params: Some(Cont { items: add }) })
}

const PICK_PREFIX: &str = "OOTRRPickOcarina";

fn push_pick(f: &mut Flow, k: Kind) -> Res<u16> {
    let i: u16 = u16::try_from(f.events.len()).ok().filter(|i| *i != crate::bfevfl::flow::NONE).ok_or(Error::Format("ocarina: the flow has too many events"))?;
    f.events.push(crate::bfevfl::Event { name: format!("{PICK_PREFIX}{i}"), kind: k });
    Ok(i)
}

fn ask_name(f: &mut Flow, value: &str, hit: u16, miss: u16) -> Res<u16> {
    let (a, q): (u16, u16) = crate::progressive::ensure(f, "GameDataActor", "EventQueryIsGameDataString64")?;
    let c: Cont = Cont { items: vec![("GameDataName".to_string(), Val::Str(crate::pickup_tiers::NAME.to_string())), ("Index".to_string(), Val::Int(-1)), ("Value".to_string(), Val::Str(value.to_string()))] };
    push_pick(f, Kind::Switch { actor: a, query: q, cases: vec![crate::bfevfl::Case { val: 0, ev: hit }, crate::bfevfl::Case { val: 1, ev: miss }], params: Some(c) })
}

fn owns(f: &mut Flow, actor: &str, yes: u16, no: u16) -> Res<u16> {
    let (a, q): (u16, u16) = crate::progressive::ensure(f, "GameDataActor", "EventQueryCheckIsGet")?;
    let c: Cont = Cont { items: vec![("ActorName".to_string(), Val::Str(actor.to_string()))] };
    push_pick(f, Kind::Switch { actor: a, query: q, cases: vec![crate::bfevfl::Case { val: 0, ev: yes }, crate::bfevfl::Case { val: 1, ev: no }], params: Some(c) })
}

fn settle(f: &mut Flow, next: u16) -> Res<u16> {
    let (a, x): (u16, u16) = crate::start_state::slot(f, "EventSystemActor", "EventWait")?;
    let c: Cont = Cont { items: vec![("IsWaitEnd".to_string(), Val::Bool(1)), ("Frames".to_string(), Val::Int(crate::pickup_tiers::SETTLE))] };
    push_pick(f, Kind::Action { next, actor: a, act: x, params: Some(c) })
}

pub fn picked_up(d: &Data, seed: &SeedFile) -> bool {
    seed.placements.iter().any(|(loc, item)| d.family(item) == Some(FAMILY) && crate::slice::tier_order::fixed_spot(d, loc) && !crate::shops::is_loc(loc))
}

const FAMILY: &str = "Ocarina";

pub fn patch_get(f: &mut Flow, i: usize, name: &str) -> Res<bool> {
    let then: u16 = match f.events.get(i) {
        Some(crate::bfevfl::Event { name: n, kind: Kind::Action { next, .. } }) if n == name => *next,
        _ => return Err(Error::Msg(format!("ocarina: event {i} of {} is not the pouch add {name}, the game files differ", f.name))),
    };
    if f.events.iter().any(|e| e.name.starts_with(PICK_PREFIX)) {
        return Ok(false);
    }
    let best: &str = OCARINAS[1];
    let to_best: u16 = after(f, best, then)?;
    let to_first: u16 = after(f, OCARINAS[0], then)?;
    let first_or_best: u16 = owns(f, best, to_best, to_first)?;
    let hits: [u16; 2] = [settle(f, first_or_best)?, settle(f, to_best)?];
    let mut miss: u16 = then;
    for (tier, hit) in OCARINAS.iter().zip(hits).rev() {
        let idx: usize = crate::progressive::tiers(FAMILY)?.iter().position(|t| t == tier).ok_or(Error::Format("ocarina: the Ocarina chain lost a tier"))?;
        for n in crate::pickup_tiers::names(FAMILY, idx)?.iter().rev() {
            miss = ask_name(f, n, hit, miss)?;
        }
    }
    if let Kind::Action { next, .. } = &mut f.events[i].kind {
        *next = miss;
    }
    Ok(true)
}

pub fn stage(rf: &mut crate::romfs::Romfs, late: &mut Vec<crate::staging::Out>, work: &std::path::Path, on: bool) -> Res<()> {
    if !on {
        return Ok(());
    }
    for (flow, i, name) in crate::pickup_tiers::GETS {
        crate::story_skip::stage_late(rf, late, work, flow, |b| {
            let f: &mut Flow = b.flows.iter_mut().find(|x| x.name == flow).ok_or_else(|| Error::Msg(format!("ocarina: no {flow} flow")))?;
            patch_get(f, i, name).map(|_| ())
        })?;
    }
    Ok(())
}

fn forms_in(d: &Data, item: &str) -> Vec<String> {
    d.item(item).map(|i| i.actor.iter().chain(i.tiers.iter()).filter(|a| no_anim(a)).cloned().collect()).unwrap_or_default()
}

/// The reward scenes the story skips turn into the standard item get (story_skip), which holds the
/// item up whatever the check holds.
pub fn scene_locs() -> Vec<String> {
    let mut out: Vec<String> = vec![crate::lacs_key::LOC.to_string(), crate::slice::flow_rules::SARIA.to_string(), crate::story_skip::RAURU_LOC.to_string(), crate::story_skip::REQUIEM_LOC.to_string()];
    out.extend(crate::story_skip::REWARD_SCENES.iter().map(|x| x.0.to_string()));
    for t in &crate::story_skip::SONG_TRIGGERS {
        out.push(t.loc.to_string());
        out.extend(t.also.iter().map(|x| x.to_string()));
    }
    out.sort();
    out.dedup();
    out
}

/// The locations that hand out an item with no animation by holding it up, with the item actors
/// they can hand out: placed items at NPC gifts that hold them up, and placed or vanilla items at
/// the reward scenes turned into the item get.
pub fn spots(d: &Data, seed: &SeedFile) -> Vec<(String, Vec<String>)> {
    let mut out: Vec<(String, Vec<String>)> = Vec::new();
    for (loc, item) in &seed.placements {
        if holds_up(d, loc) && d.vanilla_of(loc) != Some(item.as_str()) {
            let fs: Vec<String> = forms_in(d, item);
            if !fs.is_empty() {
                out.push((loc.clone(), fs));
            }
        }
    }
    for (loc, item) in &seed.placements {
        if d.family(item) == Some(FAMILY) && in_chest(d, loc) && !out.iter().any(|x| &x.0 == loc) {
            let mut fs: Vec<String> = forms_in(d, item).into_iter().filter(|a| OCARINAS.contains(&a.as_str())).collect();
            fs.dedup();
            if !fs.is_empty() {
                out.push((loc.clone(), fs));
            }
        }
    }
    for loc in scene_locs() {
        if out.iter().any(|x| x.0 == loc) {
            continue;
        }
        let Some(item) = seed.placements.get(&loc).map(|s| s.as_str()).or_else(|| d.vanilla_of(&loc)) else {
            continue;
        };
        let fs: Vec<String> = forms_in(d, item);
        if !fs.is_empty() {
            out.push((loc, fs));
        }
    }
    out
}

/// Whether the seed hands out such an item by holding it up anywhere.
pub fn needed(d: &Data, seed: &SeedFile) -> bool {
    !spots(d, seed).is_empty()
}

/// The pickup packs the seed build writes for those locations.
pub fn wanted(d: &Data, seed: &SeedFile) -> Vec<(String, Pick)> {
    let mut out: Vec<(String, Pick)> = Vec::new();
    for (loc, actors) in spots(d, seed) {
        for a in actors {
            if let Some(p) = pick(&a) {
                out.push((loc.clone(), p));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hold_up_gift_hands_out_the_ocarina_pickup_actors() {
        let d: Data = Data::load().unwrap();
        assert_eq!(shown("Item_Ocarina_Fairy").as_deref(), Some("Obj_Ocarina_Fairy"));
        assert_eq!(shown("Item_Ocarina_Time").as_deref(), Some("Obj_Ocarina_TimeLand"));
        assert_eq!(shown("Item_Hookshot"), None);
        assert!(is_form("Obj_Ocarina_TimeLand") && !is_form("Obj_Heart_Piece"));
        assert_eq!(forms(&d, "Kak Man on Roof", "Ocarina"), vec!["Obj_Ocarina_Fairy".to_string(), "Obj_Ocarina_TimeLand".to_string()]);
        assert!(forms(&d, "Kak Man on Roof", "Hookshot").is_empty());
        // Saria's gift shows a get window, no hold up, until the story skips turn it into the item get.
        assert!(forms(&d, "LW Gift from Saria", "Ocarina").is_empty());
        assert_eq!(forms_of("Ocarina"), vec!["Obj_Ocarina_Fairy".to_string(), "Obj_Ocarina_TimeLand".to_string()]);
        assert_eq!(shown("Item_Melody_ForestMinuet").as_deref(), Some("Obj_Melody_ForestMinuet"));
        assert_eq!(tier_of("Obj_Melody_ForestMinuet"), Some("Item_Melody_ForestMinuet"));
        assert_eq!(tier_of("Obj_Ocarina_TimeLand"), Some("Item_Ocarina_Time"));
        assert!(equip_key("Item_Melody_ForestMinuet").is_none() && equip_key("Item_Ocarina_Time") == Some(EQUIP));
        let scenes: Vec<String> = scene_locs();
        for l in ["LW Gift from Saria", "Song from Impa", "ToT Reward from Rauru", "Sheik in Forest", "Queen Gohma"] {
            assert!(scenes.iter().any(|x| x == l), "{l}");
        }
        assert!(scenes.iter().any(|l| l.starts_with("Sheik")), "{scenes:?}");
        assert_eq!(crate::slice::flow_pool::dest_form(&d, "Ocarina", "Kak Anju as Child", "Item_Ocarina_Fairy".to_string()), "Obj_Ocarina_Fairy");
    }

    #[test]
    fn a_shuffled_ocarina_of_time_spot_hands_out_its_placed_item_in_the_song_skip() {
        let root: std::path::PathBuf = std::env::var_os("OOTRR_ROMFS").map(std::path::PathBuf::from).unwrap_or_else(|| std::path::PathBuf::from("D:/Oot/romfs"));
        if !root.join("Pack").join("ZsDic.pack.zs").is_file() {
            eprintln!("skipped: no romfs at {}, set OOTRR_ROMFS", root.display());
            return;
        }
        let d: Data = Data::load().unwrap();
        let mut s: crate::settings::Settings = crate::settings::Settings::default();
        crate::slice::pick::shuffle_both(&mut s);
        let spot: &str = crate::slice::flow_rules::OCARINA_OF_TIME;
        let mut g = (0..20)
            .map(|n| crate::slice::generate::generate_data(&d, &s, &format!("ocarina of time spot {n}"), &crate::slice::GenOpts::default()).unwrap())
            .find(|g| g.seed_file.placements.iter().all(|(l, i)| i != "Progressive Hookshot" || !crate::slice::tier_order::fixed_spot(&d, l)))
            .unwrap();
        assert!(g.seed_file.placements.contains_key(spot), "the Ocarina of Time spot is not shuffled");
        // Put the Hookshot there; only the build is tested here.
        let at: String = g.seed_file.placements.iter().find(|(_, i)| i.as_str() == "Progressive Hookshot").map(|(l, _)| l.clone()).unwrap();
        let there: String = g.seed_file.placements[spot].clone();
        g.seed_file.placements.insert(at, there);
        g.seed_file.placements.insert(spot.to_string(), "Progressive Hookshot".to_string());
        let load: std::path::PathBuf = std::env::temp_dir().join(format!("ootrr_ocarina_of_time_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&load);
        let b = crate::slice::edit::build(&g.seed_file, &root, &load, "ocarina of time").unwrap();
        let romfs: std::path::PathBuf = b.root.join("romfs");
        let mut rf: crate::romfs::Romfs = crate::romfs::Romfs::open(&root).unwrap();
        let z: Vec<u8> = std::fs::read(crate::story_skip::DOOR_FILE.split('/').fold(romfs.clone(), |x, s| x.join(s))).unwrap();
        let bf = crate::bfevfl::Bf::parse(&rf.zs.decompress(&z).unwrap()).unwrap();
        let gives = |n: &str| {
            bf.flows.iter().flat_map(|f| f.events.iter()).any(|e| e.name.starts_with("OOTRRSongDm_MelodyGet_Time") && e.kind.params().and_then(|c| c.get("ActorName")) == Some(&crate::bfevfl::Val::Str(n.to_string())))
        };
        assert!(gives("Item_Hookshot"), "the song skip does not hand out the placed Hookshot");
        assert!(!gives("Item_Ocarina_Time"), "the song skip still hands out the vanilla Ocarina");
        let _ = std::fs::remove_dir_all(&load);
    }

    #[test]
    fn a_seed_with_the_ocarina_at_the_man_on_the_roof_builds_both_pickup_packs_and_hands_them_out() {
        let root: std::path::PathBuf = std::env::var_os("OOTRR_ROMFS").map(std::path::PathBuf::from).unwrap_or_else(|| std::path::PathBuf::from("D:/Oot/romfs"));
        if !root.join("Pack").join("ZsDic.pack.zs").is_file() {
            eprintln!("skipped: no romfs at {}, set OOTRR_ROMFS", root.display());
            return;
        }
        let d: Data = Data::load().unwrap();
        let mut s: crate::settings::Settings = crate::settings::Settings::default();
        crate::slice::pick::shuffle_both(&mut s);
        let mut g = crate::slice::generate::generate_data(&d, &s, "ocarina held", &crate::slice::GenOpts::default()).unwrap();
        // Trade one Ocarina copy with the man on the roof's item; only the build is tested here.
        let roof: &str = "Kak Man on Roof";
        let at: String = g.seed_file.placements.iter().find(|(_, i)| i.as_str() == "Ocarina").map(|(l, _)| l.clone()).unwrap();
        let there: String = g.seed_file.placements[roof].clone();
        g.seed_file.placements.insert(at, there);
        g.seed_file.placements.insert(roof.to_string(), "Ocarina".to_string());
        assert!(needed(&d, &g.seed_file));
        let load: std::path::PathBuf = std::env::temp_dir().join(format!("ootrr_ocarina_hold_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&load);
        let b = crate::slice::edit::build(&g.seed_file, &root, &load, "ocarina").unwrap();
        let romfs: std::path::PathBuf = b.root.join("romfs");
        for a in OCARINAS {
            let p: std::path::PathBuf = format!("Pack/Actor/{}.pack.zs", shown(a).unwrap()).split('/').fold(romfs.clone(), |x, s| x.join(s));
            assert!(p.is_file(), "{a}'s pickup pack not written");
            let item: std::path::PathBuf = format!("Pack/Actor/{a}.pack.zs").split('/').fold(romfs.clone(), |x, s| x.join(s));
            assert!(!item.is_file(), "{a}'s own pack stays vanilla");
        }
        // The man on the roof's gift hands out the pickup actors.
        let l = d.flow.by_name(roof).unwrap();
        let file: String = crate::flow_locations::path_of(&l.edits[0].file);
        let mut rf: crate::romfs::Romfs = crate::romfs::Romfs::open(&root).unwrap();
        let z: Vec<u8> = std::fs::read(file.split('/').fold(romfs.clone(), |x, s| x.join(s))).unwrap();
        let raw: Vec<u8> = rf.zs.decompress(&z).unwrap();
        let bf = crate::bfevfl::Bf::parse(&raw).unwrap();
        // Hand out events only: the chain's ownership questions still ask about the item actors.
        let named = |n: &str| {
            bf.flows.iter().flat_map(|f| f.events.iter()).filter(|e| !matches!(e.kind, crate::bfevfl::Kind::Switch { .. })).any(|e| e.kind.params().and_then(|c| c.get(&l.edits[0].param)) == Some(&crate::bfevfl::Val::Str(n.to_string())))
        };
        assert!(named("Obj_Ocarina_Fairy") && named("Obj_Ocarina_TimeLand"));
        // Each pickup branch then adds its Ocarina tier like Saria's gift and equips it.
        for t in OCARINAS {
            let adds = bf.flows.iter().flat_map(|f| f.events.iter()).any(|e| e.kind.params().is_some_and(|c| c.get("Actor") == Some(&crate::bfevfl::Val::Str(t.to_string())) && c.get("IsGetEvent") == Some(&crate::bfevfl::Val::Bool(0))));
            assert!(adds, "{t} is not added after its pickup actor");
        }
        for t in OCARINAS {
            let equips = bf.flows.iter().flat_map(|f| f.events.iter()).any(|e| {
                e.kind.params().is_some_and(|c| c.get("GameDataName") == Some(&crate::bfevfl::Val::Str(EQUIP.to_string())) && c.get("Value") == Some(&crate::bfevfl::Val::Str(t.to_string())))
            });
            assert!(equips, "{t} is not equipped after its pickup actor");
        }
        assert!(!named("Item_Ocarina_Fairy"));
        let _ = std::fs::remove_dir_all(&load);
    }
}

#[cfg(test)]
mod pickup_tests {
    use crate::settings::{Settings, Value};
    use crate::slice::data::Data;

    fn walk(f: &crate::bfevfl::Flow, from: u16, named: &str, owned: &[&str]) -> Vec<String> {
        use crate::bfevfl::{Kind, Val};
        let mut out: Vec<String> = Vec::new();
        let mut at: u16 = from;
        let mut steps: usize = 0;
        while at != crate::bfevfl::flow::NONE && steps < 128 {
            steps += 1;
            let e = &f.events[usize::from(at)];
            at = match &e.kind {
                Kind::Switch { actor, query, cases, params } => {
                    let q: &str = &f.actors[usize::from(*actor)].queries[usize::from(*query)];
                    let yes: bool = match (q, params.as_ref().and_then(|c| c.get("Value").or_else(|| c.get("ActorName")))) {
                        ("EventQueryIsGameDataString64", Some(Val::Str(v))) => v == named,
                        ("EventQueryCheckIsGet", Some(Val::Str(v))) => owned.contains(&v.as_str()),
                        _ => false,
                    };
                    cases.iter().find(|c| c.val == if yes { 0 } else { 1 }).map(|c| c.ev).unwrap_or(crate::bfevfl::flow::NONE)
                }
                Kind::Action { next, actor, act, params } => {
                    let what: &str = &f.actors[usize::from(*actor)].actions[usize::from(*act)];
                    let arg: String = match params.as_ref().and_then(|c| c.get("Actor").or_else(|| c.get("Value"))) {
                        Some(Val::Str(s)) => s.clone(),
                        _ => String::new(),
                    };
                    out.push(format!("{what} {arg}").trim().to_string());
                    *next
                }
                Kind::Sub { next, flow, entry, .. } => {
                    out.push(format!("{flow}:{entry}"));
                    *next
                }
                _ => crate::bfevfl::flow::NONE,
            };
        }
        out
    }

    #[test]
    fn an_ocarina_in_a_chest_drops_its_pickup_and_the_chest_flow_equips_it() {
        let root: std::path::PathBuf = ootrr_game::test_romfs::path();
        if !root.join("Pack").join("ZsDic.pack.zs").is_file() {
            eprintln!("skipped: no romfs at {}, set OOTRR_ROMFS", root.display());
            return;
        }
        let d: Data = Data::load().unwrap();
        let mut s: Settings = Settings::default();
        crate::slice::pick::shuffle_both(&mut s);
        let chest: &str = "KF Midos Top Left Chest";
        // Swapping the Ocarina into the chest can push a key item out of tier order on some
        // seeds, so take the first of a few seeds where the swap stays valid.
        let (g, (ts, _, _)) = (0..8)
            .find_map(|n| {
                let mut g = crate::slice::generate::generate_data(&d, &s, &format!("ocarina chest {n}"), &crate::slice::GenOpts::default()).unwrap();
                let at: String = g.seed_file.placements.iter().find(|(_, i)| i.as_str() == "Ocarina").map(|(l, _)| l.clone()).unwrap();
                if at != chest {
                    let there: String = g.seed_file.placements[chest].clone();
                    g.seed_file.placements.insert(at, there);
                    g.seed_file.placements.insert(chest.to_string(), "Ocarina".to_string());
                }
                if !crate::slice::tier_order::fixed_actors(&d, &g.seed_file).ok()?.bad.is_empty() {
                    return None;
                }
                let t = crate::slice::chest_targets::targets(&d, &g.seed_file).ok()?;
                Some((g, t))
            })
            .unwrap();
        assert!(super::in_chest(&d, chest));
        let t = ts.iter().find(|t| t.name == chest).unwrap();
        assert_eq!(t.family, Some("Ocarina"));
        assert!(super::is_form(&t.new_actor), "{}", t.new_actor);
        let packs: Vec<String> = super::wanted(&d, &g.seed_file).into_iter().filter(|(l, _)| l == chest).map(|(_, p)| p.pack).collect();
        assert_eq!(packs, ["Obj_Ocarina_Fairy", "Obj_Ocarina_TimeLand"]);
        let load: std::path::PathBuf = std::env::temp_dir().join(format!("ootrr_ocarina_chest_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&load);
        let b = crate::slice::edit::build(&g.seed_file, &root, &load, "ocarina chest").unwrap();
        let romfs: std::path::PathBuf = b.root.join("romfs");
        for p in &packs {
            assert!(romfs.join("Pack").join("Actor").join(format!("{p}.pack.zs")).is_file(), "{p}");
        }
        let mut rf: crate::romfs::Romfs = crate::romfs::Romfs::open(&root).unwrap();
        let mut seen: usize = 0;
        for flow in [crate::progressive::SMALL_FLOW, crate::progressive::BIG_FLOW] {
            let Ok(z) = std::fs::read(flow.split('/').fold(romfs.clone(), |x, s| x.join(s))) else {
                continue;
            };
            let bf = crate::bfevfl::Bf::parse(&rf.zs.decompress(&z).unwrap()).unwrap();
            let f = &bf.flows[0];
            let Some(e) = f.entries.iter().find(|e| e.name == crate::progressive::entry_name("Ocarina")) else {
                continue;
            };
            seen += 1;
            let shown: Vec<String> = e.subs.iter().filter_map(|i| f.events[usize::from(*i)].kind.params().and_then(|c| c.get("ContentName")).and_then(|v| match v {
                crate::bfevfl::Val::Str(s) => Some(s.clone()),
                _ => None,
            })).collect();
            assert_eq!(shown, ["Obj_Ocarina_Fairy", "Obj_Ocarina_TimeLand"], "{flow}");
            for t in super::OCARINAS {
                assert!(f.events.iter().any(|e| e.kind.params().is_some_and(|c| c.get("GameDataName") == Some(&crate::bfevfl::Val::Str(super::EQUIP.to_string())) && c.get("Value") == Some(&crate::bfevfl::Val::Str(t.to_string())))), "{flow} {t}");
            }
        }
        assert!(seen > 0, "no chest flow routes the Ocarina");
        let _ = std::fs::remove_dir_all(&load);
    }

    #[test]
    fn a_picked_up_ocarina_is_added_like_saria_s_gift_and_equipped() {
        let root: std::path::PathBuf = ootrr_game::test_romfs::path();
        if !root.join("Pack").join("ZsDic.pack.zs").is_file() {
            eprintln!("skipped: no romfs at {}, set OOTRR_ROMFS", root.display());
            return;
        }
        let mut rf: crate::romfs::Romfs = crate::romfs::Romfs::open(&root).unwrap();
        for (flow, i, name) in crate::pickup_tiers::GETS {
            let mut b = crate::bfevfl::Bf::parse(&rf.read_zs(&format!("Event/EventFlow/{flow}.bfevfl.zs")).unwrap()).unwrap();
            let f = b.flows.iter_mut().find(|x| x.name == flow).unwrap();
            let before: Vec<String> = walk(f, i as u16, "Obj_Rupee_Green", &[]);
            assert!(super::patch_get(f, i, name).unwrap());
            assert!(!super::patch_get(f, i, name).unwrap());
            let b = crate::bfevfl::Bf::parse(&b.write().unwrap()).unwrap();
            let f = b.flows.iter().find(|x| x.name == flow).unwrap();
            assert_eq!(walk(f, i as u16, "Obj_Rupee_Green", &[]), before, "{flow}");
            let equip = |t: &str| format!("EventTriggerSetGameDataString64 {t}");
            let add = |t: &str| format!("EventTriggerAddOrRemovePouchContent {t}");
            for (n, owned, tier) in [
                ("Item_Ocarina_Fairy", &[][..], "Item_Ocarina_Fairy"),
                ("Obj_Ocarina_Fairy", &[][..], "Item_Ocarina_Fairy"),
                ("Item_Ocarina_Fairy", &["Item_Ocarina_Time"][..], "Item_Ocarina_Time"),
                ("Obj_Ocarina_TimeLand", &[][..], "Item_Ocarina_Time"),
                ("Item_Ocarina_Time", &["Item_Ocarina_Fairy"][..], "Item_Ocarina_Time"),
            ] {
                let w: Vec<String> = walk(f, i as u16, n, owned);
                assert_eq!(w.get(1).map(|s| s.as_str()), Some("EventWait"), "{flow} {n} {w:?}");
                assert_eq!(&w[2..4], [add(tier), equip(tier)], "{flow} {n} {w:?}");
                assert_eq!(&w[4..], &before[1..], "{flow} {n}");
            }
        }
    }

    #[test]
    fn a_generated_seed_with_an_ocarina_at_a_pickup_spot_builds_its_pickup_pack() {
        let root: std::path::PathBuf = std::env::var_os("OOTRR_ROMFS").map(std::path::PathBuf::from).unwrap_or_else(|| std::path::PathBuf::from("D:/Oot/romfs"));
        if !root.join("Pack").join("ZsDic.pack.zs").is_file() {
            eprintln!("skipped: no romfs at {}, set OOTRR_ROMFS", root.display());
            return;
        }
        let d: Data = Data::load().unwrap();
        let mut s: Settings = Settings::default();
        crate::slice::pick::shuffle_both(&mut s);
        s.set("shuffle_freestanding_items", Value::s("all")).unwrap();
        s.set("shuffle_pots", Value::s("all")).unwrap();
        let spot = |loc: &str| d.pots.has(loc) || d.cat.by_name(loc).is_some_and(crate::slice::pickups::is_world);
        let mut found = None;
        for n in 0..30 {
            let g = crate::slice::generate::generate_data(&d, &s, &format!("ocarina spot {n}"), &crate::slice::GenOpts::default()).unwrap();
            if let Some((l, _)) = g.seed_file.placements.iter().find(|(l, i)| i.as_str() == "Ocarina" && spot(l)) {
                found = Some((l.clone(), g));
                break;
            }
        }
        let Some((loc, g)) = found else {
            panic!("no seed put an Ocarina at a freestanding spot or a pot in 30 tries");
        };
        let load: std::path::PathBuf = std::env::temp_dir().join(format!("ootrr_ocarina_spot_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&load);
        let b = crate::slice::edit::build(&g.seed_file, &root, &load, "ocarina spot").unwrap_or_else(|e| panic!("{loc}: {e}"));
        let actor = b.root.join("romfs").join("Pack").join("Actor");
        assert!(actor.join("Obj_Ocarina_Fairy.pack.zs").is_file() || actor.join("Obj_Ocarina_TimeLand.pack.zs").is_file(), "{loc}");
        let packs: std::collections::BTreeSet<String> = std::fs::read_dir(&actor).unwrap().filter_map(|e| e.ok()?.file_name().to_str()?.strip_suffix(".pack.zs").map(|s| s.to_string())).collect();
        let mut rf: crate::romfs::Romfs = crate::romfs::Romfs::open(&root).unwrap();
        let mut placed: usize = 0;
        for dir in std::fs::read_dir(b.root.join("romfs").join("Banc")).unwrap().filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.is_dir()) {
            for f in std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.to_string_lossy().ends_with(".bcett.byml.zs")) {
                let y = crate::byml::Byml::parse(&rf.zs.decompress(&std::fs::read(&f).unwrap()).unwrap()).unwrap();
                for a in y.root.get("Actors").and_then(|v| v.as_arr()).into_iter().flatten() {
                    if a.get("Gyaml").and_then(|v| v.as_str()).is_some_and(|g| packs.contains(g)) {
                        placed += 1;
                        assert!(a.get("Scale").is_none(), "{} in {} is scaled", a.get("Gyaml").and_then(|v| v.as_str()).unwrap_or(""), f.display());
                    }
                }
            }
        }
        assert!(placed > 0);
        let _ = std::fs::remove_dir_all(&load);
    }
}
