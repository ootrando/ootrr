//! Dungeon items (small keys, boss keys, maps, compasses) from gift scenes: cutscenes, Great
//! Fairies, the dungeon reward scenes and Saria's gift (crate::slice::flow_rules::gift_scene).
//!
//! A gift scene hands out its item through the game's generic pouch get: the get window, then the
//! pouch add. The pouch add does not know which dungeon a key belongs to, so for a dungeon item the
//! grant keeps only the get window (GetPouchContent:DisplayGetByActorName, the window Saria's gift
//! shows) and then makes the writes a credited chest makes (crate::dungeon_credit for small keys,
//! crate::dungeon_grant for the rest): the dungeon's key count and flags, then the line that names
//! the dungeon. The location patcher has already pointed the grant's ActorName at the item's actor.

use std::collections::BTreeSet as BS;

use crate::bfevfl::{Cont, Flow, Kind, Val};
use crate::dungeon_credit::{action_slot, add_int, flag, idx, push, set_bool, talk, HOLDER, SET, TALK, TALKER};
use crate::dungeon_grant::{struct_of, Kind as Grant};
use crate::error::{Error as E, Res};
use crate::romfs::Romfs;
use crate::settings::seed::SeedFile;
use crate::flow_locations::StagedFile;
use crate::slice::data::Data;

const POUCH: &str = "GetPouchContent";
const WINDOW: &str = "DisplayGetByActorName";
const ADD: &str = "PouchSystemActor.EventTriggerAddOrRemovePouchContent";
const ADD_INT: &str = "EventTriggerAddOrSubGameDataInt";
const PARAM: &str = "ActorName";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Item {
    Small,

    Ring(&'static crate::key_rings::Ring),
    Other(Grant),
}

impl Item {
    pub fn of(kind: &str) -> Option<Item> {
        match kind {
            "SmallKey" | "HideoutSmallKey" => Some(Item::Small),
            k => Grant::of(k).map(Item::Other),
        }
    }


    pub fn of_item(name: &str, kind: &str) -> Option<Item> {
        crate::key_rings::by_item_any(name).map(Item::Ring).or_else(|| Item::of(kind))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gift {
    pub location: String,
    pub file: String,
    pub index: usize,
    pub event: String,
    pub item: Item,
    pub dungeon: &'static str,
}

/// True for a flow location whose grant is the generic pouch get.
/// A flow location that hands its dungeon item out through its grant edit: every reward event
/// with a grant, not only the gift scenes. A grant routine without a window-only copy stops the
/// build with its name (edit_flow), so it can be added to crate::item_window.
pub fn spot(d: &Data, name: &str) -> bool {
    d.flow.by_name(name).is_some_and(|l| {
        l.name == crate::slice::flow_rules::SARIA || crate::slice::flow_rules::any_item(l) || (!crate::slice::flow_rules::own_module(l) && l.edits.iter().any(|e| e.role == "grant"))
    })
}

/// The dungeon items the seed puts in gift scenes, one per grant event.
pub fn gifts(d: &Data, seed: &SeedFile) -> Res<Vec<Gift>> {
    let mut out: Vec<Gift> = Vec::new();
    for (loc, item) in &seed.placements {
        if !spot(d, loc) {
            continue;
        }
        let Some(i) = d.item(item) else { continue };
        let Some(k) = Item::of_item(item, &i.kind) else { continue };
        let dungeon: &'static str = match k {
            Item::Ring(r) => r.folder,
            _ => i
                .dungeon
                .as_deref()
                .and_then(struct_of)
                .ok_or_else(|| E::Msg(format!("{item} at {loc} belongs to no dungeon this build can credit")))?,
        };
        let l = d.flow.by_name(loc).ok_or_else(|| E::Msg(format!("{loc} is not a flow location")))?;
        for e in l.edits.iter().filter(|e| e.role == "grant") {
            for file in std::iter::once(&e.file).chain(e.also.iter()) {
                out.push(Gift { location: loc.clone(), file: file.clone(), index: e.index, event: e.event.clone(), item: k, dungeon });
            }
        }
    }
    Ok(out)
}

/// The boss key, map and compass lines the gifts need (crate::dungeon_grant::stage_text).
pub fn marked(gs: &[Gift]) -> BS<(Grant, &'static str)> {
    gs.iter()
        .filter_map(|g| match g.item {
            Item::Other(k) => Some((k, g.dungeon)),
            Item::Small | Item::Ring(_) => None,
        })
        .collect()
}

/// The dungeons whose small key line the gifts need (crate::hints::stage keys).
pub fn small_dungeons(gs: &[Gift]) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = gs
        .iter()
        .filter(|g| matches!(g.item, Item::Small | Item::Ring(_)))
        .map(|g| g.dungeon)
        .collect();
    out.sort();
    out.dedup();
    out
}

fn actor_of(c: &Cont) -> Option<String> {
    c.items.iter().find(|(k, _)| k == PARAM || k == "Actor").and_then(|(_, v)| match v {
        Val::Str(s) | Val::Actor(s, _) => Some(s.clone()),
        _ => None,
    })
}

fn action_name(f: &Flow, i: usize) -> Option<String> {
    match &f.events.get(i)?.kind {
        Kind::Action { actor, act, .. } => {
            let a = f.actors.get(*actor as usize)?;
            Some(format!("{}.{}", a.name, a.actions.get(*act as usize)?))
        }
        _ => None,
    }
}

fn add_count(name: String, count: u32) -> Cont {
    Cont {
        items: vec![
            ("Index".to_string(), Val::Int(-1)),
            ("GameDataName".to_string(), Val::Str(name)),
            ("Value".to_string(), Val::Int(count as i32)),
        ],
    }
}






pub fn is_ring_credit(f: &Flow, index: usize) -> bool {
    let Some(next) = (match f.events.get(index).map(|e| &e.kind) {
        Some(Kind::Action { next, .. } | Kind::Sub { next, .. }) => Some(*next),
        _ => None,
    }) else {
        return false;
    };
    let Some(c) = f.events.get(usize::from(next)).and_then(|e| e.kind.params()) else {
        return false;
    };
    let Some(Val::Str(name)) = c.get("GameDataName") else {
        return false;
    };
    let Some(folder) = name.strip_prefix("DungeonInfo.").and_then(|s| s.strip_suffix(".SmallKeyNum")) else {
        return false;
    };
    let Some(r) = crate::key_rings::by_folder_any(folder) else {
        return false;
    };
    c.get("Value") == Some(&Val::Int(r.count as i32))
}

/// The credit writes and the dungeon line, ending on `end`. Returns the first event.
pub fn credit(f: &mut Flow, item: Item, d: &'static str, end: u16) -> Res<u16> {
    let (a, set): (u16, u16) = action_slot(f, HOLDER, SET)?;
    let (t, say): (u16, u16) = action_slot(f, TALKER, TALK)?;
    let at: u16 = idx(f.events.len())?;
    match item {
        Item::Small => {
            let (_, add): (u16, u16) = action_slot(f, HOLDER, ADD_INT)?;
            push(f, Kind::Action { next: at + 1, actor: a, act: add, params: Some(add_int(flag(d, "SmallKeyNum"))) })?;
            push(f, Kind::Action { next: at + 2, actor: a, act: add, params: Some(add_int(flag(d, "SmallKeyTotalNum"))) })?;
            push(f, Kind::Action { next: at + 3, actor: a, act: set, params: Some(set_bool(flag(d, "IsGetSmallKeyOnce"))) })?;
            push(f, Kind::Action { next: end, actor: t, act: say, params: Some(talk(crate::dungeon_text::message_id(d))) })?;
        }
        Item::Ring(r) => {
            let (_, add): (u16, u16) = action_slot(f, HOLDER, ADD_INT)?;
            push(f, Kind::Action { next: at + 1, actor: a, act: add, params: Some(add_count(flag(d, "SmallKeyNum"), r.count)) })?;
            push(f, Kind::Action { next: at + 2, actor: a, act: add, params: Some(add_count(flag(d, "SmallKeyTotalNum"), r.count)) })?;
            push(f, Kind::Action { next: at + 3, actor: a, act: set, params: Some(set_bool(flag(d, "IsGetSmallKeyOnce"))) })?;
            push(f, Kind::Action { next: at + 4, actor: a, act: set, params: Some(set_bool(crate::key_rings::applied_flag(d))) })?;
            push(f, Kind::Action { next: end, actor: t, act: say, params: Some(talk(crate::dungeon_text::message_id(d))) })?;
        }
        Item::Other(k) => {
            push(f, Kind::Action { next: at + 1, actor: a, act: set, params: Some(set_bool(flag(d, k.member()))) })?;
            push(f, Kind::Action { next: end, actor: t, act: say, params: Some(talk(crate::dungeon_grant::message_id(k, d))) })?;
        }
    }
    Ok(at)
}

/// Turns one grant into the get window plus the credit. A call into a give routine (an NPC
/// handover, a Dm_SY_GetInEvent item get, a local reward entry) calls the routine's window-only copy
/// `window` (crate::item_window), so the item is still shown and Link still holds it up. A direct
/// pouch get becomes the get window; a bare pouch add (Saria's, whose window is its own event)
/// becomes the credit.
pub fn edit_flow(f: &mut Flow, index: usize, event: &str, item: Item, d: &'static str, window: Option<&str>) -> Res<()> {
    let e = f.events.get(index).ok_or_else(|| E::Msg(format!("gift keys: event index {index} is out of range")))?;
    if e.name != event {
        return Err(E::Msg(format!("gift keys: event {index} is named {} and not {event}, the game files differ", e.name)));
    }
    if action_name(f, index).as_deref() == Some(ADD) {
        let Kind::Action { next, .. } = f.events[index].kind else { unreachable!() };
        let first: u16 = credit(f, item, d, next)?;
        if matches!(item, Item::Ring(_)) {

            if let Kind::Action { next, .. } = &mut f.events[index].kind {
                *next = first;
            }
            return Ok(());
        }
        // The add event itself becomes a jump to the credit: copy the first credit write in place.
        let k: Kind = f.events[first as usize].kind.clone();
        f.events[index].kind = k;
        return Ok(());
    }
    let next: u16 = match &f.events[index].kind {
        Kind::Sub { next, .. } => *next,
        _ => return Err(E::Msg(format!("gift keys: {event} in {} is not a give, the game files differ", f.name))),
    };
    let first: u16 = credit(f, item, d, next)?;
    if matches!(item, Item::Ring(_)) {

        if let Kind::Sub { next, .. } = &mut f.events[index].kind {
            *next = first;
        }
        return Ok(());
    }
    let k: &mut Kind = &mut f.events[index].kind;
    let Kind::Sub { next, params, flow, entry } = k else { unreachable!() };
    *next = first;
    if flow == POUCH {
        let a: String = params.as_ref().and_then(actor_of).ok_or(E::Format("gift keys: the grant names no actor"))?;
        *params = Some(Cont { items: vec![(PARAM.to_string(), Val::Str(a))] });
        *entry = WINDOW.to_string();
    } else {
        *entry = window.ok_or_else(|| E::Msg(format!("gift keys: {event} calls {flow}:{entry}, which has no window-only copy")))?.to_string();
        if item == Item::Small {
            hold_up_key(params);
        }
    }
    Ok(())
}

/// A give routine that holds the item up waits on the shown item's Rotate_GetDemo spin. Neither
/// Item_Dungeon_SmallKey (the generic item spin, on the ground key's model) nor Obj_Dungeon_SmallKey
/// (the ground pickup's animation info, which plays nothing) can play it, so Link raised his hands,
/// nothing appeared and the scene waited forever (GC Rolling Goron as Adult, in game, with both).
/// The one key the game holds up itself is Obj_MiniGame_SmallKey, whose own ASInfo spins the key
/// model (the Treasure Chest Game key shop, through GetInEvent_NotTurn_DisplayGet, a window-only
/// hold-up like these). So a small key is held up as that actor, through the routine's key copy
/// (crate::item_window::key_window), whose get window names the dungeon key. Nothing is added to
/// the pouch, and the credit after it names the dungeon.
pub const SMALL_KEY_ITEM: &str = "Item_Dungeon_SmallKey";
pub const SMALL_KEY_SHOWN: &str = "Obj_MiniGame_SmallKey";

fn hold_up_key(params: &mut Option<Cont>) {
    if let Some(c) = params.as_mut() {
        for (k, v) in c.items.iter_mut() {
            if k == PARAM && matches!(v, Val::Str(s) if s == SMALL_KEY_ITEM) {
                *v = Val::Str(SMALL_KEY_SHOWN.to_string());
            }
        }
    }
}

fn stem(file: &str) -> &str {
    file.strip_suffix(".bfevfl").unwrap_or(file)
}

/// Edits the scenes in `files` (the location patcher's staged flows), or the game's if not staged,
/// adding the window-only copies of the give routines they call. `extra` names more routines
/// (flow, entry) whose copies are wanted (the Light Arrows item get).
pub fn apply(rf: &mut Romfs, files: &mut Vec<StagedFile>, gs: &[Gift], extra: &[(&str, &str)]) -> Res<()> {
    let mut c: crate::item_window::Cache = crate::item_window::Cache::new();
    for g in gs {
        let here: &str = stem(&g.file);
        let call: Option<(String, String)> = {
            let f: &mut Flow = c.flow_mut(rf, files, here)?;
            match f.events.get(g.index).map(|e| &e.kind) {
                Some(Kind::Sub { flow, entry, .. }) if flow != POUCH => Some((if flow.is_empty() { here.to_string() } else { flow.clone() }, entry.clone())),
                _ => None,
            }
        };
        let w: Option<String> = match &call {
            Some(_) if matches!(g.item, Item::Ring(_)) => None,
            Some((fl, en)) if g.item == Item::Small => Some(crate::item_window::key_window(&mut c, rf, files, fl, en)?),
            Some((fl, en)) => Some(crate::item_window::window(&mut c, rf, files, fl, en)?),
            None => None,
        };
        let f: &mut Flow = c.flow_mut(rf, files, here)?;
        edit_flow(f, g.index, &g.event, g.item, g.dungeon, w.as_deref())?;
    }
    for (fl, en) in extra {
        crate::item_window::window(&mut c, rf, files, fl, en)?;
        if gs.iter().any(|g| g.item == Item::Small) {
            crate::item_window::key_window(&mut c, rf, files, fl, en)?;
        }
    }
    c.store(rf, files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bfevfl::Bf;

    fn sage_of_fire() -> Option<(Romfs, Bf)> {
        let p: std::path::PathBuf = ootrr_game::test_romfs::path();
        if !p.is_dir() {
            eprintln!("skipped: no romfs at {}, set OOTRR_ROMFS", p.display());
            return None;
        }
        let mut rf: Romfs = Romfs::open(&p).unwrap();
        let z: Vec<u8> = rf.read("Event/EventFlow/SageOfFire.bfevfl.zs").unwrap();
        let b: Bf = Bf::parse(&rf.zs.decompress(&z).unwrap()).unwrap();
        Some((rf, b))
    }

    fn gift_scene() -> Option<(Romfs, Bf)> {
        let p: std::path::PathBuf = ootrr_game::test_romfs::path();
        if !p.is_dir() {
            eprintln!("skipped: no romfs at {}, set OOTRR_ROMFS", p.display());
            return None;
        }
        let mut rf: Romfs = Romfs::open(&p).unwrap();
        let z: Vec<u8> = rf.read("Event/EventFlow/Dm_Kokiri_0155.bfevfl.zs").unwrap();
        let b: Bf = Bf::parse(&rf.zs.decompress(&z).unwrap()).unwrap();
        Some((rf, b))
    }

    fn actor(f: &Flow, i: usize) -> String {
        f.events[i].kind.params().and_then(|c| c.get(PARAM)).and_then(|v| match v {
            Val::Str(s) => Some(s.clone()),
            _ => None,
        }).unwrap()
    }

    /// GC Rolling Goron as Adult (SageOfFire Event18) holds its item up through
    /// Dm_SY_GetInEvent:GetInEvent_NotTurn. A small key there froze the game with Link's hands up.
    #[test]
    fn a_small_key_held_up_by_an_npc_is_shown_as_the_minigame_key() {
        let Some((_, mut b)) = sage_of_fire() else { return };
        let f: &mut Flow = b.flows.first_mut().unwrap();
        let (i, ev): (usize, String) = (177, "Event18".to_string());
        assert_eq!(actor(f, i), "Obj_Armor_Goron");
        let mut small: Flow = f.clone();
        if let Some((_, v)) = small.events[i].kind.params_mut().and_then(|c| c.items.iter_mut().find(|(k, _)| k == PARAM)) {
            *v = Val::Str(SMALL_KEY_ITEM.to_string());
        }
        let mut boss: Flow = small.clone();
        edit_flow(&mut small, i, &ev, Item::Small, "GerudoTrainingGround", Some("OOTRR_K_GetInEvent_NotTurn")).unwrap();
        assert_eq!(actor(&small, i), SMALL_KEY_SHOWN);
        assert!(matches!(&small.events[i].kind, Kind::Sub { entry, .. } if entry == "OOTRR_K_GetInEvent_NotTurn"));
        // Other dungeon items keep their own actor, which has a model of its own.
        if let Some((_, v)) = boss.events[i].kind.params_mut().and_then(|c| c.items.iter_mut().find(|(k, _)| k == PARAM)) {
            *v = Val::Str("Item_Dungeon_BossKey".to_string());
        }
        edit_flow(&mut boss, i, &ev, Item::Other(Grant::Boss), "FireTemple", Some("OOTRR_W_GetInEvent_NotTurn")).unwrap();
        assert_eq!(actor(&boss, i), "Item_Dungeon_BossKey");
    }

    /// The key copy holds up the actor it is given (the minigame key) and its window names the
    /// dungeon key. The game holds up Obj_MiniGame_SmallKey itself, through a window-only hold-up.
    #[test]
    fn the_key_copy_holds_up_the_minigame_key_and_names_the_dungeon_key() {
        let Some((mut rf, _)) = sage_of_fire() else { return };
        assert!(rf.root.join(format!("Pack/Actor/{SMALL_KEY_SHOWN}.pack.zs")).is_file());
        let mut c: crate::item_window::Cache = crate::item_window::Cache::new();
        let files: Vec<StagedFile> = Vec::new();
        let k: String = crate::item_window::key_window(&mut c, &mut rf, &files, "Dm_SY_GetInEvent", "GetInEvent_NotTurn").unwrap();
        assert_eq!(k, "OOTRR_K_GetInEvent_NotTurn");
        let w: String = crate::item_window::window(&mut c, &mut rf, &files, "Dm_SY_GetInEvent", "GetInEvent_NotTurn").unwrap();
        assert_ne!(k, w);
        let f: &mut Flow = c.flow_mut(&mut rf, &files, "Dm_SY_GetInEvent").unwrap();
        let main: u16 = f.entries.iter().find(|e| e.name == k).unwrap().main;
        let mut seen: BS<u16> = BS::new();
        let mut todo: Vec<u16> = vec![main];
        let (mut windows, mut shown): (Vec<Val>, bool) = (Vec::new(), false);
        while let Some(i) = todo.pop() {
            if i == 0xFFFF || !seen.insert(i) {
                continue;
            }
            let ev = &f.events[i as usize].kind;
            match ev {
                Kind::Sub { flow, entry, params, .. } => {
                    assert!(!(flow == POUCH && entry.starts_with("GetPouchContent")), "the key copy still adds to the pouch");
                    if flow == POUCH && entry == WINDOW {
                        windows.push(params.as_ref().and_then(|p| p.get(PARAM)).cloned().unwrap());
                    }
                    if entry == "GetInEvent_NotTurn_Sub" {
                        shown = params.as_ref().and_then(|p| p.get(PARAM)).is_some_and(|v| !matches!(v, Val::Str(_)));
                    }
                }
                _ => {}
            }
            match ev {
                Kind::Action { next, .. } | Kind::Sub { next, .. } | Kind::Join { next } => todo.push(*next),
                Kind::Switch { cases, .. } | Kind::SubSwitch { cases, .. } => todo.extend(cases.iter().map(|c| c.ev)),
                Kind::Fork { join, forks } => todo.extend(std::iter::once(*join).chain(forks.iter().copied())),
                Kind::Label { .. } => {}
            }
        }
        assert_eq!(windows, vec![Val::Str(SMALL_KEY_ITEM.to_string())]);
        // The NPC the caller passes binds to the copy's own argument actor (sub and entry index),
        // or its hand over never ends and the scene waits forever with Link's hands up.
        let at: u16 = f.entries.iter().position(|e| e.name == k).unwrap() as u16;
        let npc: Vec<&crate::bfevfl::Actor> = seen
            .iter()
            .filter_map(|&i| match &f.events[i as usize].kind {
                Kind::Action { actor, .. } | Kind::Switch { actor, .. } => Some(&f.actors[*actor as usize]),
                _ => None,
            })
            .filter(|a| !a.arg.is_empty())
            .collect();
        assert!(!npc.is_empty());
        for a in npc {
            assert_eq!(a.sub, format!("{k}({})", a.arg));
            assert_eq!(a.ep, at, "{}", a.sub);
        }
        let orig: u16 = f.entries.iter().position(|e| e.name == "GetInEvent_NotTurn").unwrap() as u16;
        assert!(f.actors.iter().any(|a| a.sub == "GetInEvent_NotTurn(Npc_EventStarter)" && a.ep == orig), "the original entry keeps its own NPC");
        // The shown item still comes from the caller's ActorName (an argument, not a literal).
        assert!(shown);
    }



    #[test]
    fn a_ring_gift_survives_the_shortened_gift_scene() {
        for ring in crate::key_rings::all_with_hideout() {
            let Some((_, mut b)) = gift_scene() else { return };
            let f: &mut Flow = b.flows.first_mut().unwrap();
            edit_flow(f, 51, "Event68", Item::Ring(ring), ring.folder, None).unwrap();
            assert!(is_ring_credit(f, 51), "{}", ring.item);
            crate::story_skip::patch_gift(&mut b).unwrap();
            let f: &mut Flow = b.flows.first_mut().unwrap();
            assert!(is_ring_credit(f, 51), "the shortened gift no longer runs the {} credit", ring.item);
        }
    }
}
