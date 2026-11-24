










use crate::bfevfl::flow::NONE;
use crate::bfevfl::{Bf, Case, Cont, Entry, Event, Flow, Kind, Val};
use crate::byml::{Byml, Node};
use crate::chest_slot::LIST;
use crate::error::{Error as E, Res};
use crate::flow_locations::dict_id;
use crate::romfs::Romfs;
use crate::staging::{rewrite, Out};
use crate::zs;

const PREFIX: &str = "OOTRRKeyRing";
const FLOW: &str = "ActionGuide";


const WORLD_ENTRY: &str = "OOTRRRingWorld";
const SHOP_ENTRY: &str = "ActionGuideForShop";
const GET_ENTRY: &str = "ActionGuide";
const WAIT_ACTOR: &str = "EventSystemActor";
const WAIT_ACT: &str = "EventWait";
const TRUE: u32 = 0x8000_0001;
const HOLDER: &str = "GameDataActor";
const ASK_BOOL: &str = "EventQueryIsGameDataBool";
const ADD: &str = "EventTriggerAddOrSubGameDataInt";
const SET: &str = "EventTriggerSetGameDataBool";
const BASE_ROW: u32 = 0x154ef588;

fn idx(n: usize) -> Res<u16> {
    u16::try_from(n).map_err(|_| E::Format("key rings: too many events in the flow"))
}

fn push(f: &mut Flow, kind: Kind) -> Res<u16> {
    let i: u16 = idx(f.events.len())?;
    f.events.push(Event { name: format!("{PREFIX}{i}"), kind });
    Ok(i)
}



fn switch(f: &mut Flow, actor: &str, query: &str, params: Cont, on_true: u16, on_false: u16) -> Res<Kind> {
    let (a, q): (u16, u16) = crate::start_state::query_slot(f, actor, query)?;
    Ok(Kind::Switch { actor: a, query: q, cases: vec![Case { val: 0, ev: on_true }, Case { val: 1, ev: on_false }], params: Some(params) })
}

fn add_int(name: String, count: u32) -> Cont {
    Cont {
        items: vec![
            ("Index".to_string(), Val::Int(-1)),
            ("GameDataName".to_string(), Val::Str(name)),
            ("Value".to_string(), Val::Int(count as i32)),
        ],
    }
}




fn ring(f: &mut Flow, r: &'static crate::key_rings::Ring, next: u16) -> Res<u16> {
    let applied: String = crate::key_rings::applied_flag(r.folder);
    let (a, add): (u16, u16) = crate::dungeon_credit::action_slot(f, HOLDER, ADD)?;
    let (_, set): (u16, u16) = crate::dungeon_credit::action_slot(f, HOLDER, SET)?;
    let (t, say): (u16, u16) = crate::dungeon_credit::action_slot(f, crate::dungeon_credit::TALKER, crate::dungeon_credit::TALK)?;

    let say_it: u16 = push(f, Kind::Action {
        next,
        actor: t,
        act: say,
        params: Some(crate::dungeon_credit::talk(crate::dungeon_text::message_id(r.folder))),
    })?;
    let mark: u16 = push(f, Kind::Action { next: say_it, actor: a, act: set, params: Some(crate::dungeon_credit::set_bool(applied.clone())) })?;
    let once: u16 = push(f, Kind::Action {
        next: mark,
        actor: a,
        act: set,
        params: Some(crate::dungeon_credit::set_bool(crate::dungeon_credit::flag(r.folder, "IsGetSmallKeyOnce"))),
    })?;
    let total: u16 = push(f, Kind::Action {
        next: once,
        actor: a,
        act: add,
        params: Some(add_int(crate::dungeon_credit::flag(r.folder, "SmallKeyTotalNum"), r.count)),
    })?;
    let num: u16 = push(f, Kind::Action {
        next: total,
        actor: a,
        act: add,
        params: Some(add_int(crate::dungeon_credit::flag(r.folder, "SmallKeyNum"), r.count)),
    })?;



    let held: Kind = switch(
        f,
        HOLDER,
        ASK_BOOL,
        Cont { items: vec![("GameDataName".to_string(), Val::Str(crate::key_rings::got_flag(r.actor))), ("Index".to_string(), Val::Int(-1))] },
        num,
        next,
    )?;
    let held_q: u16 = push(f, held)?;

    let asked: Kind = switch(
        f,
        HOLDER,
        ASK_BOOL,
        Cont { items: vec![("GameDataName".to_string(), Val::Str(applied)), ("Index".to_string(), Val::Int(-1))] },
        next,
        held_q,
    )?;
    push(f, asked)
}



pub fn patch(f: &mut Flow) -> Res<()> {
    patch_with(f, &crate::key_rings::RINGS.iter().collect::<Vec<_>>())
}

pub fn patch_with(f: &mut Flow, rings: &[&'static crate::key_rings::Ring]) -> Res<()> {
    if rings.is_empty() {
        return Ok(());
    }
    if f.events.iter().any(|e| e.name.starts_with(PREFIX)) {
        return Ok(());
    }
    let template: &crate::bfevfl::Entry = f.entries.iter().find(|e| e.name == GET_ENTRY).ok_or_else(|| E::Msg(format!("key rings: no {GET_ENTRY} entry in the {FLOW} flow")))?;
    let pad: u16 = template.pad;
    let (a, x): (u16, u16) = crate::start_state::slot(f, WAIT_ACTOR, WAIT_ACT)?;
    let wait: u16 = push(f, Kind::Action {
        next: NONE,
        actor: a,
        act: x,
        params: Some(Cont {
            items: vec![("IsWaitEnd".to_string(), Val::Bool(TRUE)), ("Frames".to_string(), Val::Int(1))],
        }),
    })?;
    let mut first: u16 = wait;
    for r in rings.iter().rev() {
        first = ring(f, r, first)?;
    }
    f.entries.push(Entry { name: WORLD_ENTRY.to_string(), subs: Vec::new(), vars: Vec::new(), main: first, pad });
    let shop: &mut crate::bfevfl::Entry = f.entries.iter_mut().find(|e| e.name == SHOP_ENTRY).ok_or_else(|| E::Msg(format!("key rings: no {SHOP_ENTRY} entry in the {FLOW} flow")))?;
    shop.main = first;
    Ok(())
}

pub fn patch_bf(b: &mut Bf, rings: &[&'static crate::key_rings::Ring]) -> Res<()> {
    let f: &mut Flow = b.flows.iter_mut().find(|x| x.name == FLOW).ok_or_else(|| E::Msg(format!("key rings: no {FLOW} flow")))?;
    patch_with(f, rings)
}


fn redirect(f: &mut Flow, file: &str) -> Res<usize> {
    let mut n: usize = 0;
    for e in &mut f.events {
        if let Kind::Sub { entry, flow, .. } = &mut e.kind
            && flow == FLOW
            && entry == GET_ENTRY
        {
            *entry = WORLD_ENTRY.to_string();
            n += 1;
        }
    }
    if n != 1 {
        return Err(E::Msg(format!("key rings: {file} calls {FLOW}:{GET_ENTRY} {n} times, expected once")));
    }
    Ok(n)
}

pub fn redirect_bf(b: &mut Bf, file: &str) -> Res<()> {
    let f: &mut Flow = b.flows.iter_mut().find(|x| x.name == file).ok_or_else(|| E::Msg(format!("key rings: no {file} flow")))?;
    redirect(f, file)?;
    Ok(())
}



pub fn stage_flows(rf: &mut Romfs, late: &mut Vec<Out>, work: &std::path::Path, rings: &[&'static crate::key_rings::Ring]) -> Res<()> {
    if rings.is_empty() {
        return Ok(());
    }
    crate::story_skip::stage_late(rf, late, work, FLOW, |b| patch_bf(b, rings))?;
    for file in ["Dm_SY_Get", "Dm_SY_Get_WithoutPlayer"] {
        crate::story_skip::stage_late(rf, late, work, file, |b| redirect_bf(b, file))?;
    }
    Ok(())
}

fn flag_pairs_with(rings: &[&'static crate::key_rings::Ring]) -> Vec<(String, u32)> {
    let mut out: Vec<(String, u32)> = Vec::new();
    for r in rings {
        let got: String = crate::key_rings::got_flag(r.actor);
        let applied: String = crate::key_rings::applied_flag(r.folder);
        out.push((got.clone(), crate::flags::hash_name(&got)));
        out.push((applied.clone(), crate::flags::hash_name(&applied)));
    }
    out
}

fn row_hash(r: &Node) -> Option<u32> {
    match r.get("Hash") {
        Some(Node::U32(h)) => Some(*h),
        _ => None,
    }
}

fn is_our_row(r: &Node, h: u32) -> bool {
    row_hash(r) == Some(h)
        && r.get("DefaultValue") == Some(&Node::Bool(false))
        && r.get("ResetTypeValue") == Some(&Node::I32(8))
        && r.get("SaveFileIndex") == Some(&Node::I32(0))
}




fn add_isget_members_with(y: &mut Byml, rings: &[&'static crate::key_rings::Ring]) -> Res<()> {
    let isget: u32 = crate::flags::hash_name("IsGet");
    let structs: &mut Vec<Node> = y
        .root
        .get_mut("Data")
        .and_then(|d| d.get_mut("Struct"))
        .and_then(|s| s.as_arr_mut())
        .ok_or_else(|| E::Format("key rings: the game data list has no Struct table"))?;
    let members: &mut Vec<Node> = structs
        .iter_mut()
        .find(|s| row_hash(s) == Some(isget))
        .and_then(|s| s.get_mut("DefaultValue"))
        .and_then(|v| v.as_arr_mut())
        .ok_or_else(|| E::Format("key rings: the game data list has no IsGet struct"))?;
    for r in rings {
        let member: u32 = crate::flags::hash_name(r.actor);
        let flag: u32 = crate::flags::hash_name(&crate::key_rings::got_flag(r.actor));
        if let Some(row) = members.iter().find(|m| row_hash(m) == Some(member)) {
            if row.get("Value") != Some(&Node::U32(flag)) {
                return Err(E::Msg(format!("key rings: the IsGet member for {} differs from the game data", r.actor)));
            }
            continue;
        }
        members.push(Node::Map(vec![
            ("Hash".to_string(), Node::U32(member)),
            ("Value".to_string(), Node::U32(flag)),
        ]));
    }
    Ok(())
}

fn add_rows_with(y: &mut Byml, rings: &[&'static crate::key_rings::Ring]) -> Res<()> {
    let rows: &mut Vec<Node> = y
        .root
        .get_mut("Data")
        .and_then(|d| d.get_mut("Bool"))
        .and_then(|b| b.as_arr_mut())
        .ok_or_else(|| E::Format("key rings: the game data list has no Bool table"))?;
    let base: Node = rows
        .iter()
        .find(|r| row_hash(r) == Some(BASE_ROW))
        .cloned()
        .ok_or_else(|| E::Format("key rings: the game data list has no saved Bool template"))?;
    for (name, h) in flag_pairs_with(rings) {
        if let Some(row) = rows.iter().find(|r| row_hash(r) == Some(h)) {
            if !is_our_row(row, h) {
                return Err(E::Msg(format!("key rings: the flag {name} clashes with a game flag")));
            }
            continue;
        }
        let mut row: Node = base.clone();
        row.set("Hash", Node::U32(h));
        row.set("DefaultValue", Node::Bool(false));
        row.set("ResetTypeValue", Node::I32(8));
        row.set("SaveFileIndex", Node::I32(0));
        rows.push(row);
    }
    Ok(())
}

pub fn add_runtime_rows(raw: &[u8], rings: &[&'static crate::key_rings::Ring]) -> Res<Vec<u8>> {
    let mut y: Byml = Byml::parse(raw)?;
    add_rows_with(&mut y, rings)?;
    add_isget_members_with(&mut y, rings)?;
    y.write()
}

fn edit_raw(raw: &[u8], rings: &[&'static crate::key_rings::Ring]) -> Res<Vec<u8>> {
    add_runtime_rows(raw, rings)
}



pub fn stage_rows(rf: &mut Romfs, more: &mut Vec<Out>, rings: &[&'static crate::key_rings::Ring]) -> Res<()> {
    if rings.is_empty() {
        return Ok(());
    }
    match more.iter().position(|o| o.path == LIST) {
        Some(i) => {
            let id: Option<u32> = dict_id(&more[i].z);
            let raw: Vec<u8> = rf.zs.decompress(&more[i].z)?;
            let out: Vec<u8> = edit_raw(&raw, rings)?;
            more[i].new = out.len();
            more[i].z = zs::compress_raw(&out, id)?;
        }
        None => more.push(rewrite(rf, LIST, |raw| edit_raw(raw, rings))?),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bfevfl::flow::NONE;
    use crate::bfevfl::{Actor, Entry};
    use std::collections::{BTreeMap as BM, BTreeSet as BS};

    fn flag_pairs() -> Vec<(String, u32)> {
        flag_pairs_with(&crate::key_rings::RINGS.iter().collect::<Vec<_>>())
    }

    fn add_isget_members(y: &mut Byml) -> Res<()> {
        add_isget_members_with(y, &crate::key_rings::RINGS.iter().collect::<Vec<_>>())
    }

    fn add_rows(y: &mut Byml) -> Res<()> {
        add_rows_with(y, &crate::key_rings::RINGS.iter().collect::<Vec<_>>())
    }

    const EXPECTED: [(&str, u32); 21] = [
        ("test", 0xba6bd213),
        ("DungeonInfo.ForestTemple.SmallKeyNum", 0x90237092),
        ("IsGet.Item_Dungeon_SmallKey", 0x154ef588),
        ("IsGet.Item_Ring_Forest", 0x2299468c),
        ("IsGet.Item_Ring_Fire", 0x223b086e),
        ("IsGet.Item_Ring_Water", 0xd8db874b),
        ("IsGet.Item_Ring_Shadow", 0x92d9860a),
        ("IsGet.Item_Ring_Spirit", 0xb28490ba),
        ("IsGet.Item_Ring_Well", 0xd5e3c18d),
        ("IsGet.Item_Ring_GTG", 0x5f62ae19),
        ("IsGet.Item_Ring_Ganon", 0x685bed59),
        ("IsGet.Item_Ring_Hideout", 0x56844297),
        ("OOTRR_KeyRing.ForestTemple", 0x2cffcc4f),
        ("OOTRR_KeyRing.FireTemple", 0x4e79f9b5),
        ("OOTRR_KeyRing.WaterTemple", 0xeba9f19f),
        ("OOTRR_KeyRing.ShadowTemple", 0xc8d7d030),
        ("OOTRR_KeyRing.SpiritTemple", 0x64576046),
        ("OOTRR_KeyRing.BottomOfTheWell", 0x0607308e),
        ("OOTRR_KeyRing.GerudoTrainingGround", 0x26517f00),
        ("OOTRR_KeyRing.InsideGanonCastle", 0x97150d07),
        ("OOTRR_KeyRing.GerudoFortress", 0x22c11c3d),
    ];

    fn actor(name: &str, acts: &[&str], queries: &[&str]) -> Actor {
        Actor {
            name: name.to_string(),
            sub: String::new(),
            arg: String::new(),
            actions: acts.iter().map(|s| s.to_string()).collect(),
            queries: queries.iter().map(|s| s.to_string()).collect(),
            params: None,
            ep: 0,
            cut: 0,
        }
    }

    fn entries() -> [(&'static str, &'static str); 2] {
        [(WORLD_ENTRY, "the world pickup"), (SHOP_ENTRY, "the shop get")]
    }

    fn guide_wait(f: &Flow) -> usize {
        f.events
            .iter()
            .position(|e| matches!(&e.kind, Kind::Action { params: Some(c), .. } if c.get("Frames") == Some(&Val::Int(1)) && c.get("IsWaitEnd") == Some(&Val::Bool(TRUE))))
            .expect("the ring chain has a one-frame wait")
    }

    fn action_guide() -> Flow {
        Flow {
            name: FLOW.to_string(),
            actors: vec![actor("EventSystemActor", &["EventWait"], &[])],
            events: vec![
                Event {
                    name: "OOTRRNoGuide0".to_string(),
                    kind: Kind::Action { next: NONE, actor: 0, act: 0, params: None },
                },
                Event {
                    name: "OOTRRRingCallItemGet".to_string(),
                    kind: Kind::Action { next: 0, actor: 0, act: 0, params: None },
                },
                Event {
                    name: "OOTRRRingCallForShop".to_string(),
                    kind: Kind::Action { next: 0, actor: 0, act: 0, params: None },
                },
            ],
            entries: vec![
                Entry { name: "ActionGuide".to_string(), subs: Vec::new(), vars: Vec::new(), main: 1, pad: 0 },
                Entry { name: "ActionGuideForShop".to_string(), subs: Vec::new(), vars: Vec::new(), main: 2, pad: 0 },
            ],
        }
    }

    #[test]
    fn only_world_and_shop_gets_check_each_held_ring_before_their_guide_wait() {
        let mut f: Flow = action_guide();
        patch(&mut f).unwrap();
        let wait: usize = guide_wait(&f);
        for (entry_name, label) in entries() {
            let entry: &Entry = f.entries.iter().find(|e| e.name == entry_name).unwrap();
            let mut at: usize = entry.main as usize;
            assert_ne!(at, wait, "{label} starts at the ring checks, not the wait");
            let mut applied_seen: BS<String> = BS::new();
            let mut held_seen: BS<String> = BS::new();
            let mut reached_wait: bool = false;
            for _ in 0..f.events.len() {
                if at == wait {
                    reached_wait = true;
                    break;
                }
                let e: &Event = &f.events[at];
                match &e.kind {
                    Kind::Switch { params, cases, .. } => {
                        let c: &Cont = params.as_ref().unwrap();
                        let Some(Val::Str(name)) = c.get("GameDataName") else { panic!("a ring switch needs a flag") };
                        if name.starts_with("OOTRR_KeyRing.") {
                            applied_seen.insert(name.clone());
                        } else if name.starts_with("IsGet.") {
                            held_seen.insert(name.clone());
                        }


                        let answer: u32 = if name.starts_with("OOTRR_KeyRing.") { 1 } else { 0 };
                        at = cases.iter().find(|c| c.val == answer).unwrap().ev as usize;
                    }
                    Kind::Action { next, .. } => at = *next as usize,
                    _ => panic!("unexpected ring credit event"),
                }
            }
            assert!(reached_wait, "{label} reaches its wait");
            assert_eq!(applied_seen.len(), 8, "{label} has an applied-flag guard per ring");
            assert_eq!(held_seen.len(), 8, "{label} checks every saved IsGet bool");
            assert!(applied_seen.contains(&crate::key_rings::applied_flag("ForestTemple")));
            assert!(held_seen.contains(&crate::key_rings::got_flag("Item_Ring_Forest")));
        }
        let ordinary: &Entry = f.entries.iter().find(|e| e.name == GET_ENTRY).unwrap();
        assert_eq!(ordinary.main, 1, "chests and gifts keep the untouched item-get entry");
        assert!(f.events.iter().any(|e| e.name.starts_with(PREFIX)));
        let before: Flow = f.clone();
        patch(&mut f).unwrap();
        assert_eq!(f, before, "the credit is added once");
    }

    #[test]
    fn the_hideout_ring_is_checked_and_credited_like_a_standard_ring() {
        let mut f: Flow = action_guide();
        let rings: Vec<&'static crate::key_rings::Ring> = vec![&crate::key_rings::HIDEOUT_RING];
        patch_with(&mut f, &rings).unwrap();
        let wait: usize = guide_wait(&f);
        let entry: &Entry = f.entries.iter().find(|e| e.name == WORLD_ENTRY).unwrap();
        let mut at: usize = entry.main as usize;
        let mut applied: BS<String> = BS::new();
        let mut held: BS<String> = BS::new();
        let mut four: bool = false;
        for _ in 0..f.events.len() {
            if at == wait {
                break;
            }
            match &f.events[at].kind {
                Kind::Switch { params, cases, .. } => {
                    let c: &Cont = params.as_ref().unwrap();
                    let Some(Val::Str(name)) = c.get("GameDataName") else { panic!("a ring switch needs a flag") };
                    if name.starts_with("OOTRR_KeyRing.") {
                        applied.insert(name.clone());
                    } else if name.starts_with("IsGet.") {
                        held.insert(name.clone());
                    }
                    let answer: u32 = if name.starts_with("OOTRR_KeyRing.") { 1 } else { 0 };
                    at = cases.iter().find(|c| c.val == answer).unwrap().ev as usize;
                }
                Kind::Action { next, params, .. } => {
                    if let Some(c) = params
                        && c.get("GameDataName") == Some(&Val::Str("DungeonInfo.GerudoFortress.SmallKeyNum".to_string()))
                    {
                        four = c.get("Value") == Some(&Val::Int(4));
                    }
                    at = *next as usize;
                }
                _ => panic!("unexpected ring credit event"),
            }
        }
        assert_eq!(applied.len(), 1);
        assert_eq!(held.len(), 1);
        assert!(applied.contains(&crate::key_rings::applied_flag("GerudoFortress")));
        assert!(held.contains(&crate::key_rings::got_flag("Item_Ring_Hideout")));
        assert!(four, "the Hideout ring writes four keys");
    }

    #[test]
    fn only_the_world_get_flows_call_the_checked_entry() {
        let sub = |flow: &str, entry: &str| Event {
            name: format!("call {flow} {entry}"),
            kind: Kind::Sub { next: NONE, params: None, flow: flow.to_string(), entry: entry.to_string() },
        };
        let mut b: Bf = Bf {
            ver: [0, 4, 0, 0],
            bom: 0xFEFF,
            shift: 3,
            pad: 0,
            name: "flow".to_string(),
            tls: Vec::new(),
            flows: vec![Flow {
                name: "Dm_SY_Get".to_string(),
                actors: Vec::new(),
                events: vec![sub(FLOW, GET_ENTRY), sub("Other", GET_ENTRY)],
                entries: Vec::new(),
            }],
        };
        redirect_bf(&mut b, "Dm_SY_Get").unwrap();
        let Kind::Sub { entry, .. } = &b.flows[0].events[0].kind else { panic!("sub") };
        assert_eq!(entry, WORLD_ENTRY, "the world pickup calls the checked entry");
        let Kind::Sub { entry, .. } = &b.flows[0].events[1].kind else { panic!("sub") };
        assert_eq!(entry, GET_ENTRY, "other callers are left alone");
        assert!(redirect_bf(&mut b, "Dm_SY_Get").is_err(), "a second redirect is refused");
    }

    #[test]
    fn a_picked_up_ring_writes_its_full_count_and_its_applied_flag() {
        let mut f: Flow = action_guide();
        patch(&mut f).unwrap();
        let forest: &crate::key_rings::Ring = crate::key_rings::by_folder("ForestTemple").unwrap();
        let applied: String = crate::key_rings::applied_flag(forest.folder);
        let wait: usize = guide_wait(&f);
        let entry_name: &str = WORLD_ENTRY;
        let mut at: usize = f.entries.iter().find(|e| e.name == entry_name).unwrap().main as usize;
        let mut credit: Option<u16> = None;
        for _ in 0..f.events.len() {
            if at == wait {
                break;
            }
            let e: &Event = &f.events[at];
            if let Kind::Switch { cases, params, .. } = &e.kind
                && params.as_ref().and_then(|c| c.get("GameDataName")) == Some(&Val::Str(applied.clone()))
            {
                let skip: u16 = cases.iter().find(|c| c.val == 0).unwrap().ev;
                credit = Some(cases.iter().find(|c| c.val == 1).unwrap().ev);
                assert_ne!(skip, credit.unwrap(), "an applied flag has its own branch");
                break;
            }
            let Kind::Action { next, .. } = &e.kind else { panic!("expected a ring check") };
            at = *next as usize;
        }
        let mut at: usize = credit.expect("the Forest applied flag is checked once per chain") as usize;
        let mut writes: BM<String, i32> = BM::new();
        for _ in 0..f.events.len() {
            let e: &Event = &f.events[at];
            match &e.kind {
                Kind::Switch { cases, params, .. } => {


                    let name: String = match params.as_ref().and_then(|c| c.get("GameDataName")) {
                        Some(Val::Str(n)) => n.clone(),
                        _ => panic!("a ring switch needs a flag"),
                    };
                    let answer: u32 = if name.starts_with("OOTRR_KeyRing.") { 1 } else { 0 };
                    at = cases.iter().find(|c| c.val == answer).unwrap().ev as usize;
                }
                Kind::Action { next, params, .. } => {
                    if let Some(c) = params
                        && let Some(Val::Str(name)) = c.get("GameDataName")
                    {
                        let value: Option<i32> = match c.get("Value") {
                            Some(Val::Int(v)) => Some(*v),
                            Some(Val::Bool(v)) => Some(*v as i32),
                            _ => None,
                        };
                        if let Some(v) = value {
                            writes.insert(name.clone(), v);
                        }
                    }
                    if usize::from(*next) == wait {
                        break;
                    }
                    at = *next as usize;
                }
                _ => break,
            }
        }
        assert_eq!(writes.get("DungeonInfo.ForestTemple.SmallKeyNum"), Some(&5));
        assert_eq!(writes.get("DungeonInfo.ForestTemple.SmallKeyTotalNum"), Some(&5));
        assert_eq!(writes.get("DungeonInfo.ForestTemple.IsGetSmallKeyOnce"), Some(&1));
        assert_eq!(writes.get(applied.as_str()), Some(&1));
    }

    #[test]
    fn every_flag_the_ring_credit_uses_has_a_staged_game_data_row() {
        let names: BS<String> = flag_pairs().into_iter().map(|(n, _)| n).collect();
        for r in crate::key_rings::RINGS {
            assert!(names.contains(&crate::key_rings::got_flag(r.actor)));
            assert!(names.contains(&crate::key_rings::applied_flag(r.folder)));
        }
        for (n, h) in EXPECTED {
            assert_eq!(crate::flags::hash_name(n), h, "{n}");
        }
    }

    #[test]
    fn the_pouch_add_can_record_each_ring_through_the_is_get_struct() {
        let mut y: Byml = Byml {
            root: Node::Map(vec![(
                "Data".to_string(),
                Node::Map(vec![
                    ("Bool".to_string(), Node::Arr(vec![Node::Map(vec![("Hash".to_string(), Node::U32(BASE_ROW))])])),
                    (
                        "Struct".to_string(),
                        Node::Arr(vec![Node::Map(vec![
                            ("Hash".to_string(), Node::U32(crate::flags::hash_name("IsGet"))),
                            ("DefaultValue".to_string(), Node::Arr(Vec::new())),
                        ])]),
                    ),
                ]),
            )]),
            version: 2,
            big: false,
        };
        add_rows(&mut y).unwrap();
        add_isget_members(&mut y).unwrap();
        let bools: &[Node] = y.root.get("Data").unwrap().get("Bool").unwrap().as_arr().unwrap();
        assert_eq!(bools.len(), 1 + 16, "each ring has a held flag and an applied flag");
        let isget: u32 = crate::flags::hash_name("IsGet");
        let members: &[Node] = y
            .root
            .get("Data")
            .unwrap()
            .get("Struct")
            .unwrap()
            .as_arr()
            .unwrap()
            .iter()
            .find(|s| row_hash(s) == Some(isget))
            .unwrap()
            .get("DefaultValue")
            .unwrap()
            .as_arr()
            .unwrap();
        assert_eq!(members.len(), 8, "each generated ring actor gets one IsGet member");
        for r in crate::key_rings::RINGS {
            let member: u32 = crate::flags::hash_name(r.actor);
            let flag: u32 = crate::flags::hash_name(&crate::key_rings::got_flag(r.actor));
            assert_eq!(
                members.iter().find(|m| row_hash(m) == Some(member)).and_then(|m| m.get("Value")),
                Some(&Node::U32(flag)),
                "{}",
                r.actor
            );
        }
        let before: Byml = y.clone();
        add_rows(&mut y).unwrap();
        add_isget_members(&mut y).unwrap();
        assert_eq!(y, before, "the rows are added once");
    }
}
