//! Quest order: checks that live inside one window of a story quest stay obtainable when the
//! player moves the quest past that window first, and early step requests never move a quest back.
//!
//! The remake hands several gifts out only while a quest sits on one step: King Zora while the red
//! ice still holds him (until SageOfWater is complete), and the fortress guard gives the Membership
//! Card in the scene that moves SageOfSpirit from Step01 to Step02. A randomizer seed can push those
//! quests on first: clearing the Water Temple, or reaching the Colossus or the Spirit Temple
//! (SageOfSpirit Step04 and on, which also clears the fortress, mends the bridge and calms the
//! Gerudo). Each [`LATE`] hook adds a branch to the talk
//! the NPC still has in those later states: while the gift has not been given, the talk runs the
//! game's own gift part (the location patcher's shuffled item is inside it) instead.
//!
//! A gift part also asks for its quest step (MoveStatue), and some places ask for early steps of a
//! quest that may already be further on (the wasteland gate guard asks for SageOfSpirit Step02, the
//! Colossus arrival for Step04). The game moves a quest back when asked for an earlier step, which
//! would undo Fire Temple or Spirit Temple progress, so each [`ORDER`] request runs only while the
//! quest is not yet past that step.

use crate::bfevfl::flow::NONE;
use crate::bfevfl::{Bf, Case, Cont, Event, Flow, Kind, Val};
use crate::error::{Error, Res};
use crate::romfs::Romfs;
use crate::start_state::{query_slot, slot};

use super::guard::{self, Run};

pub const TRUE: u32 = 0x8000_0001;
pub const DATA: &str = "GameDataActor";
pub const BOOL_QUERY: &str = "EventQueryIsGameDataBool";
pub const EXP_QUERY: &str = "EventQueryIsGameDataBoolExp";
pub const SET_BOOL: &str = "EventTriggerSetGameDataBool";
pub const MOVER: &str = "EventSystemActor";
pub const MOVE: &str = "EventTransition";
/// True while Link is a child; every hook here is for adult NPCs and adult scenes.
pub const CHILD: &str = "Sequence_IsChildhood";

/// How a hook hands out the gift.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Give {
    /// A sub call into the entry (of another flow file, or this one when the flow is empty) that
    /// holds the whole gift: talk, item get and the gift's own flag.
    Sub(&'static str, &'static str),
    /// A transition to the gift's own scene, which marks itself played.
    Scene(&'static str),
}

/// A gift kept for later: the entries of `flow` that the NPC talks through once the quest has moved
/// past the gift's window. While `done` is false (and `when`, if set, is true) an adult talk runs
/// `give` instead of the entry's own talk.
#[derive(Clone, Copy, Debug)]
pub struct Late {
    pub loc: &'static str,
    pub flow: &'static str,
    pub entries: &'static [&'static str],
    pub when: Option<&'static str>,
    pub done: &'static str,
    pub give: Give,
    pub prefix: &'static str,
}

pub const ZORA_DONE: &str = "SageOfWater_KingZora_adult_Talk_Event198";
pub const CARD_DONE: &str = "HavePlayedEvent.Dm_Gerudo_0025";
pub const CARD_SCENE: &str = "Dm_Gerudo_0025";
pub const CARD_ONCE_PREFIX: &str = "OOTRRCardOnce";

pub const LATE: [Late; 2] = [
    // After the Water Temple, King Zora stands free in the thawed domain and talks through his own
    // flow; without a Zora Tunic in the pouch he only points at the shop. Safety_Entry is the part
    // his frozen-era talk ends with when the thaw gift is still owed (SageOfWater event Event278).
    Late {
        loc: "ZD King Zora Thawed",
        flow: "Npc_Zora_KingZora_adult",
        entries: &["Talk"],
        when: Some("SageOfWater_IsCompleted_Exp"),
        done: ZORA_DONE,
        give: Give::Sub("SageOfWater", "Safety_Entry"),
        prefix: "OOTRRLateZora",
    },
    // The jail-side guard while the quest sits at Step02 without the card scene (an open fortress).
    // A cleared fortress (open_world::fortress: holding the card) otherwise gets its card scene from
    // the rescue, which no longer follows the quest or the card.
    Late {
        loc: "Hideout Gerudo Membership Card",
        flow: "SageOfSpirit",
        entries: &["Step02_Npc_Gerudo_001_Talk"],
        when: None,
        done: CARD_DONE,
        give: Give::Scene(CARD_SCENE),
        prefix: "OOTRRLateCard",
    },
];

/// (flow, quest, step, prefix): requests that run only while `<quest>_IsAfter_<step>_Exp` is false.
pub const ORDER: [(&str, &str, &str, &str); 7] = [
    // Link the Goron's intro (also played on a late first visit), his stop and his gift.
    ("SageOfFire", "SageOfFire", "StopDaruniaJr", "OOTRROrderStop"),
    ("SageOfFire", "SageOfFire", "TalkDaruniaJr", "OOTRROrderTalk"),
    ("SageOfFire", "SageOfFire", "MoveStatue", "OOTRROrderMove"),
    // The carpenter boss in the valley, the wasteland gate guard, the card scene's clear event and
    // the Colossus arrival from the wasteland.
    ("Npc_Hylia_012_Adult", "SageOfSpirit", "Step01", "OOTRROrderBoss"),
    ("Npc_Gerudo_006", "SageOfSpirit", "Step02", "OOTRROrderGate"),
    ("SageOfSpirit", "SageOfSpirit", "Step02", "OOTRROrderClear"),
    ("Dm_SY_BancChangeArrive", "SageOfSpirit", "Step04", "OOTRROrderColossus"),
];

/// Entering Goron City from the Crater plays no intro, so Link the Goron (created by
/// SageOfFire_GenerateDaruniaJr, which only the intro sets) would be missing for a player who never
/// came in from the trail or the woods. The Crater exit now sets it for an adult.
pub const GENERATE: (&str, &str, &str) = ("Dm_SY_BancChangeGo", "DeathMountainCrater_ToGoronCity", "SageOfFire_GenerateDaruniaJr");
pub const GENERATE_PREFIX: &str = "OOTRRLateGoronMake";

pub fn exp_after(quest: &str, step: &str) -> String {
    format!("{quest}_IsAfter_{step}_Exp")
}

fn push(f: &mut Flow, name: String, kind: Kind) -> Res<u16> {
    let at: u16 = guard::idx(f.events.len())?;
    f.events.push(Event { name, kind });
    Ok(at)
}

/// A switch on a bool (`BOOL_QUERY`) or an expression (`EXP_QUERY`): case 0 (true) goes to `yes`,
/// case 1 (false) to `no`.
pub fn ask(f: &mut Flow, query: &str, name: &str, yes: u16, no: u16, label: String) -> Res<u16> {
    let (actor, q): (u16, u16) = query_slot(f, DATA, query)?;
    let cases: Vec<Case> = [(0, yes), (1, no)].into_iter().filter(|(_, ev)| *ev != NONE).map(|(val, ev)| Case { val, ev }).collect();
    let mut items: Vec<(String, Val)> = Vec::new();
    if query == BOOL_QUERY {
        items.push(("Index".to_string(), Val::Int(-1)));
    }
    items.push(("GameDataName".to_string(), Val::Str(name.to_string())));
    push(f, label, Kind::Switch { actor, query: q, cases, params: Some(Cont { items }) })
}

/// Adds the late gift branch in front of every entry `l` names. Returns how many entry starts were
/// hooked (entries sharing a start share one branch).
pub fn hook(b: &mut Bf, l: &Late) -> Res<usize> {
    let f: &mut Flow = guard::flow_mut(b, l.flow)?;
    if f.events.iter().any(|e| e.name.starts_with(l.prefix)) {
        return Err(Error::Msg(format!("quest order: {} already has the late gift for {}", l.flow, l.loc)));
    }
    let mut mains: Vec<u16> = Vec::new();
    for name in l.entries {
        let e = f.entries.iter().find(|e| e.name == *name).ok_or_else(|| Error::Msg(format!("quest order: {} has no {name} entry, the game files differ", l.flow)))?;
        if !mains.contains(&e.main) {
            mains.push(e.main);
        }
    }
    for (n, old) in mains.iter().copied().enumerate() {
        let tag: String = format!("{}{n}", l.prefix);
        let give: u16 = match l.give {
            Give::Sub(flow, entry) => push(f, format!("{tag}Give"), Kind::Sub { next: NONE, params: None, flow: flow.to_string(), entry: entry.to_string() })?,
            Give::Scene(scene) => {
                let (actor, act): (u16, u16) = slot(f, MOVER, MOVE)?;
                let items: Vec<(String, Val)> = vec![
                    ("IsWaitEnd".to_string(), Val::Bool(TRUE)),
                    ("SpecialEventActionType".to_string(), Val::Int(1)),
                    ("EventName".to_string(), Val::Str(scene.to_string())),
                    ("FadeInTypeEventEnd".to_string(), Val::Int(0)),
                ];
                push(f, format!("{tag}Give"), Kind::Action { next: NONE, actor, act, params: Some(Cont { items }) })?
            }
        };
        let owed: u16 = ask(f, BOOL_QUERY, l.done, old, give, format!("{tag}Done"))?;
        let then: u16 = match l.when {
            Some(x) => ask(f, EXP_QUERY, x, owed, old, format!("{tag}When"))?,
            None => owed,
        };
        let top: u16 = ask(f, BOOL_QUERY, CHILD, old, then, format!("{tag}Adult"))?;
        for e in f.entries.iter_mut().filter(|e| e.main == old && l.entries.contains(&e.name.as_str())) {
            e.main = top;
            if matches!(l.give, Give::Sub(..)) {
                e.subs.push(give);
            }
        }
    }
    Ok(mains.len())
}

/// Guards every request for `step` of `quest` in `flow` so it runs only while the quest is not past
/// that step.
pub fn order(b: &mut Bf, flow: &str, quest: &str, step: &str, prefix: &str) -> Res<usize> {
    Ok(guard::guard_steps(b, flow, quest, &[step], &exp_after(quest, step), Run::WhenFalse, prefix)?.len())
}

/// The Crater exit to Goron City sets Link the Goron's creation flag for an adult, before its own
/// first event.
pub fn generate(b: &mut Bf) -> Res<()> {
    let (flow, entry, flag) = GENERATE;
    let f: &mut Flow = guard::flow_mut(b, flow)?;
    if f.events.iter().any(|e| e.name.starts_with(GENERATE_PREFIX)) {
        return Err(Error::Msg(format!("quest order: {flow} already makes Link the Goron")));
    }
    let i: usize = f.entries.iter().position(|e| e.name == entry).ok_or_else(|| Error::Msg(format!("quest order: {flow} has no {entry} entry, the game files differ")))?;
    let old: u16 = f.entries[i].main;
    let (actor, act): (u16, u16) = slot(f, DATA, SET_BOOL)?;
    let items: Vec<(String, Val)> = vec![
        ("Index".to_string(), Val::Int(-1)),
        ("Value".to_string(), Val::Bool(TRUE)),
        ("GameDataName".to_string(), Val::Str(flag.to_string())),
    ];
    let set: u16 = push(f, format!("{GENERATE_PREFIX}Set"), Kind::Action { next: old, actor, act, params: Some(Cont { items }) })?;
    let top: u16 = ask(f, BOOL_QUERY, CHILD, old, set, format!("{GENERATE_PREFIX}Adult"))?;
    f.entries[i].main = top;
    Ok(())
}

/// The fortress clear event (SageOfSpirit entry Meet_FourCarpenter_ClearEvent: card scene, then the
/// Step02 request) and the flag that marks the fortress cleared (open_world::fortress::MARK).
pub const CLEAR: (&str, &str) = ("SageOfSpirit", "Step02");
pub const CLEAR_PREFIX: &str = "OOTRRFortressCleared";

/// Writes the cleared mark right after the clear event's Step02 request, so it is set whether the
/// request runs or is skipped by its ORDER guard (staged after this). The mark is what the
/// fortress reads (open_world::fortress), so it must not depend on the Spirit quest's step.
pub fn mark_clear(b: &mut Bf) -> Res<()> {
    let (flow, step) = CLEAR;
    let f: &mut Flow = guard::flow_mut(b, flow)?;
    if f.events.iter().any(|e| e.name.starts_with(CLEAR_PREFIX)) {
        return Err(Error::Msg(format!("quest order: {flow} already marks the fortress cleared")));
    }
    let hits: Vec<usize> = (0..f.events.len()).filter(|&i| guard::step_of(f, &f.events[i], flow) == Some(step)).collect();
    let [at] = hits[..] else {
        return Err(Error::Msg(format!("quest order: {flow} does not ask for {step} once, the game files differ")));
    };
    let Kind::Action { next, .. } = f.events[at].kind else {
        return Err(Error::Msg(format!("quest order: the {step} request of {flow} is not an action")));
    };
    let (actor, act): (u16, u16) = slot(f, DATA, SET_BOOL)?;
    let items: Vec<(String, Val)> = vec![
        ("Index".to_string(), Val::Int(-1)),
        ("Value".to_string(), Val::Bool(TRUE)),
        ("GameDataName".to_string(), Val::Str(super::fortress::MARK.to_string())),
    ];
    let set: u16 = push(f, format!("{CLEAR_PREFIX}Set"), Kind::Action { next, actor, act, params: Some(Cont { items }) })?;
    if let Kind::Action { next, .. } = &mut f.events[at].kind {
        *next = set;
    }
    Ok(())
}

fn card_transition(f: &Flow, i: usize) -> bool {
    let Kind::Action { actor, act, params, .. } = &f.events[i].kind else {
        return false;
    };
    let Some(a) = f.actors.get(usize::from(*actor)) else {
        return false;
    };
    if a.name != MOVER || a.actions.get(usize::from(*act)).map(|s| s.as_str()) != Some(MOVE) {
        return false;
    }
    matches!(params.as_ref().and_then(|p| p.get("EventName")), Some(Val::Str(s)) if s == CARD_SCENE)
}




pub fn card_once(b: &mut Bf) -> Res<usize> {
    let f: &mut Flow = guard::flow_mut(b, CLEAR.0)?;
    if f.events.iter().any(|e| e.name.starts_with(CARD_ONCE_PREFIX)) {
        return Err(Error::Msg(format!("quest order: {} already guards the card scene", CLEAR.0)));
    }
    let targets: Vec<usize> = (0..f.events.len()).filter(|&i| card_transition(f, i)).collect();
    if targets.is_empty() {
        return Err(Error::Msg(format!("quest order: {} has no transition to {CARD_SCENE}", CLEAR.0)));
    }
    let (qa, qi): (u16, u16) = query_slot(f, DATA, BOOL_QUERY)?;
    let mut out: Vec<String> = Vec::new();
    for (n, t) in targets.iter().enumerate() {
        let old: u16 = guard::idx(*t)?;
        let next: u16 = match &f.events[*t].kind {
            Kind::Action { next, .. } => *next,
            _ => unreachable!(),
        };
        let to: u16 = guard::idx(f.events.len())?;
        guard::retarget(f, old, to);
        let cases: Vec<Case> = [(0, next), (1, old)].into_iter().filter(|(_, ev)| *ev != NONE).map(|(val, ev)| Case { val, ev }).collect();
        let name: String = format!("{CARD_ONCE_PREFIX}{n}");
        let params: Cont = Cont {
            items: vec![
                ("Index".to_string(), Val::Int(-1)),
                ("GameDataName".to_string(), Val::Str(CARD_DONE.to_string())),
            ],
        };
        f.events.push(Event { name: name.clone(), kind: Kind::Switch { actor: qa, query: qi, cases, params: Some(params) } });
        out.push(f.events[*t].name.clone());
    }
    Ok(out.len())
}

/// Every flow this module edits, each with its edit.
pub fn edits() -> Vec<(&'static str, Box<dyn Fn(&mut Bf) -> Res<()>>)> {
    let mut flows: Vec<&'static str> = Vec::new();
    for l in &LATE {
        if !flows.contains(&l.flow) {
            flows.push(l.flow);
        }
    }
    for (flow, ..) in &ORDER {
        if !flows.contains(flow) {
            flows.push(flow);
        }
    }
    if !flows.contains(&GENERATE.0) {
        flows.push(GENERATE.0);
    }
    flows
        .into_iter()
        .map(|flow| {
            let edit: Box<dyn Fn(&mut Bf) -> Res<()>> = Box::new(move |b: &mut Bf| {
                if flow == CLEAR.0 {
                    card_once(b)?;
                }
                for l in LATE.iter().filter(|l| l.flow == flow) {
                    hook(b, l)?;
                }
                if flow == CLEAR.0 {
                    mark_clear(b)?;
                }
                for (_, quest, step, prefix) in ORDER.iter().filter(|o| o.0 == flow) {
                    order(b, flow, quest, step, prefix)?;
                }
                if flow == GENERATE.0 {
                    generate(b)?;
                }
                Ok(())
            });
            (flow, edit)
        })
        .collect()
}

/// Stages every edit as a late file, on top of what the location patcher and the earlier stages
/// wrote to the same flows.
pub fn stage_late(rf: &mut Romfs, late: &mut Vec<crate::staging::Out>, work: &std::path::Path) -> Res<()> {
    for (flow, edit) in edits() {
        crate::story_skip::stage_late(rf, late, work, flow, |b| edit(b))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bfevfl::Entry;
    use std::path::PathBuf as PB;

    fn real() -> Option<PB> {
        let p: PB = ootrr_game::test_romfs::path();
        if p.join("Pack").join("ZsDic.pack.zs").is_file() {
            Some(p)
        } else {
            eprintln!("skipped, no RomFS dump at {} (set OOTRR_ROMFS)", p.display());
            None
        }
    }

    fn load(rf: &mut Romfs, flow: &str) -> Bf {
        Bf::parse(&rf.read_zs(&format!("Event/EventFlow/{flow}.bfevfl.zs")).unwrap()).unwrap()
    }

    fn flow<'a>(b: &'a Bf, name: &str) -> &'a Flow {
        b.flows.iter().find(|f| f.name == name).unwrap()
    }

    fn entry<'a>(f: &'a Flow, name: &str) -> &'a Entry {
        f.entries.iter().find(|e| e.name == name).unwrap()
    }

    fn cases(f: &Flow, at: u16) -> Vec<(u32, u16)> {
        let Kind::Switch { cases, .. } = &f.events[at as usize].kind else { panic!("event {at} is not a switch") };
        cases.iter().map(|c| (c.val, c.ev)).collect()
    }

    fn name_of(f: &Flow, at: u16) -> String {
        match f.events[at as usize].kind.params().and_then(|p| p.get("GameDataName")) {
            Some(Val::Str(s)) => s.clone(),
            _ => String::new(),
        }
    }

    fn sample() -> Bf {
        let actor = |n: &str, acts: &[&str], qs: &[&str]| crate::bfevfl::Actor {
            name: n.to_string(),
            sub: String::new(),
            arg: String::new(),
            actions: acts.iter().map(|s| s.to_string()).collect(),
            queries: qs.iter().map(|s| s.to_string()).collect(),
            params: None,
            ep: 0xFFFF,
            cut: 1,
        };
        let talk = |next: u16| Kind::Action { next, actor: 0, act: 0, params: None };
        let f: Flow = Flow {
            name: "Npc".to_string(),
            actors: vec![actor("Npc_EventStarter", &["EventTalk"], &[])],
            events: vec![Event { name: "Event0".to_string(), kind: talk(1) }, Event { name: "Event1".to_string(), kind: talk(NONE) }],
            entries: vec![
                Entry { name: "Talk".to_string(), subs: Vec::new(), vars: Vec::new(), main: 0, pad: 0 },
                Entry { name: "Talk_Meeting".to_string(), subs: Vec::new(), vars: Vec::new(), main: 0, pad: 0 },
                Entry { name: "Near".to_string(), subs: Vec::new(), vars: Vec::new(), main: 1, pad: 0 },
            ],
        };
        Bf { ver: [0, 4, 0, 0], bom: 0xFEFF, shift: 3, pad: 0, name: "Npc".to_string(), flows: vec![f], tls: Vec::new() }
    }

    #[test]
    fn a_hook_asks_adult_then_when_then_done_and_shares_one_branch_per_start() {
        let mut b: Bf = sample();
        let l: Late = Late { loc: "X", flow: "Npc", entries: &["Talk", "Talk_Meeting"], when: Some("Q_Exp"), done: "Done", give: Give::Sub("Other", "Gift"), prefix: "OOTRRLateX" };
        assert_eq!(hook(&mut b, &l).unwrap(), 1);
        let f: &Flow = &b.flows[0];
        let top: u16 = entry(f, "Talk").main;
        assert_eq!(entry(f, "Talk_Meeting").main, top);
        assert_eq!(entry(f, "Near").main, 1);
        assert_eq!(name_of(f, top), CHILD);
        let [(0, child), (1, adult)] = cases(f, top)[..] else { panic!() };
        assert_eq!(child, 0);
        assert_eq!(name_of(f, adult), "Q_Exp");
        let [(0, after), (1, before)] = cases(f, adult)[..] else { panic!() };
        assert_eq!(before, 0);
        assert_eq!(name_of(f, after), "Done");
        let [(0, given), (1, owed)] = cases(f, after)[..] else { panic!() };
        assert_eq!(given, 0);
        let Kind::Sub { next, flow, entry: e, .. } = &f.events[owed as usize].kind else { panic!() };
        assert_eq!((*next, flow.as_str(), e.as_str()), (NONE, "Other", "Gift"));
        assert_eq!(entry(f, "Talk").subs, vec![owed]);
        assert_eq!(entry(f, "Talk_Meeting").subs, vec![owed]);
        assert!(hook(&mut b, &l).is_err());
        let back: Bf = Bf::parse(&b.write().unwrap()).unwrap();
        assert_eq!(back, b);
    }

    #[test]
    fn a_scene_hook_transitions_and_lists_no_sub() {
        let mut b: Bf = sample();
        let l: Late = Late { loc: "X", flow: "Npc", entries: &["Near"], when: None, done: "Done", give: Give::Scene("Dm_X"), prefix: "OOTRRLateX" };
        hook(&mut b, &l).unwrap();
        let f: &Flow = &b.flows[0];
        let top: u16 = entry(f, "Near").main;
        let [(0, _), (1, done)] = cases(f, top)[..] else { panic!() };
        assert_eq!(name_of(f, done), "Done");
        let [(0, given), (1, owed)] = cases(f, done)[..] else { panic!() };
        assert_eq!(given, 1);
        let Kind::Action { actor, act, params, next } = &f.events[owed as usize].kind else { panic!() };
        assert_eq!(*next, NONE);
        assert_eq!(f.actors[*actor as usize].name, MOVER);
        assert_eq!(f.actors[*actor as usize].actions[*act as usize], MOVE);
        assert_eq!(params.as_ref().and_then(|p| p.get("EventName")), Some(&Val::Str("Dm_X".to_string())));
        assert!(entry(f, "Near").subs.is_empty());
        let back: Bf = Bf::parse(&b.write().unwrap()).unwrap();
        assert_eq!(back, b);
    }

    fn card_sample() -> Bf {
        let actor = |n: &str, acts: &[&str], qs: &[&str]| crate::bfevfl::Actor {
            name: n.to_string(),
            sub: String::new(),
            arg: String::new(),
            actions: acts.iter().map(|s| s.to_string()).collect(),
            queries: qs.iter().map(|s| s.to_string()).collect(),
            params: None,
            ep: 0xFFFF,
            cut: 1,
        };
        let f: Flow = Flow {
            name: CLEAR.0.to_string(),
            actors: vec![actor(MOVER, &[MOVE], &[]), actor(DATA, &[], &[BOOL_QUERY])],
            events: vec![
                Event { name: "Start".to_string(), kind: Kind::Action { next: 1, actor: 0, act: 0, params: None } },
                Event {
                    name: "Event145".to_string(),
                    kind: Kind::Action {
                        next: 2,
                        actor: 0,
                        act: 0,
                        params: Some(Cont { items: vec![("EventName".to_string(), Val::Str(CARD_SCENE.to_string()))] }),
                    },
                },
                Event { name: "After".to_string(), kind: Kind::Action { next: NONE, actor: 0, act: 0, params: None } },
            ],
            entries: vec![
                Entry { name: "Meet_FourCarpenter_ClearEvent".to_string(), subs: Vec::new(), vars: Vec::new(), main: 0, pad: 0 },
            ],
        };
        Bf { ver: [0, 4, 0, 0], bom: 0xFEFF, shift: 3, pad: 0, name: CLEAR.0.to_string(), flows: vec![f], tls: Vec::new() }
    }

    #[test]
    fn the_card_scene_guard_skips_later_unlocks_but_keeps_the_first_grant() {
        let mut b: Bf = card_sample();
        assert_eq!(card_once(&mut b).unwrap(), 1);
        let f: &Flow = &b.flows[0];
        let Kind::Action { next, .. } = &f.events[0].kind else { panic!() };
        assert_eq!(*next, 3);
        assert_eq!(cases(f, 3), vec![(0, 2), (1, 1)]);
        assert_eq!(name_of(f, 3), CARD_DONE);
        let Kind::Switch { actor, query, params, .. } = &f.events[3].kind else { panic!() };
        assert_eq!(f.actors[*actor as usize].name, DATA);
        assert_eq!(f.actors[*actor as usize].queries[*query as usize], BOOL_QUERY);
        assert_eq!(params.as_ref().and_then(|p| p.get("Index")), Some(&Val::Int(-1)));
        assert!(card_once(&mut b).is_err());
        let back: Bf = Bf::parse(&b.write().unwrap()).unwrap();
        assert_eq!(back, b);
    }

    #[test]
    fn every_edit_applies_to_the_game_flows_and_reads_back() {
        let Some(root) = real() else {
            return;
        };
        let mut rf: Romfs = Romfs::open(&root).unwrap();
        let all = edits();
        assert_eq!(all.len(), 7);
        for (name, edit) in &all {
            let mut b: Bf = load(&mut rf, name);
            edit(&mut b).unwrap();
            let back: Bf = Bf::parse(&b.write().unwrap()).unwrap();
            assert_eq!(back, b, "{name}");
            assert!(edit(&mut b).is_err(), "{name} took the edit twice");
        }
    }

    #[test]
    fn the_late_gifts_reach_the_game_gift_parts_of_their_locations() {
        let Some(root) = real() else {
            return;
        };
        let mut rf: Romfs = Romfs::open(&root).unwrap();
        let d: crate::flow_locations::Data = crate::flow_locations::Data::load().unwrap();
        for l in &LATE {
            assert!(d.by_name(l.loc).is_some(), "{} is not a flow location", l.loc);
            match l.give {
                Give::Sub(flow, entry) => {
                    let b: Bf = load(&mut rf, flow);
                    assert!(flow_named(&b, flow).entries.iter().any(|e| e.name == entry), "{flow} has no {entry}");
                    let loc = d.by_name(l.loc).unwrap();
                    assert!(loc.edits.iter().any(|x| x.file == format!("{flow}.bfevfl") && x.entries.iter().any(|e| e == entry)), "{entry} is not where {} grants", l.loc);
                }
                Give::Scene(scene) => {
                    let loc = d.by_name(l.loc).unwrap();
                    assert!(loc.edits.iter().any(|x| x.file == format!("{scene}.bfevfl")), "{scene} does not grant {}", l.loc);
                }
            }
        }
    }

    fn flow_named<'a>(b: &'a Bf, name: &str) -> &'a Flow {
        flow(b, name)
    }

    #[test]
    fn the_guarded_requests_are_the_only_early_ones_and_their_expressions_exist() {
        let Some(root) = real() else {
            return;
        };
        let mut rf: Romfs = Romfs::open(&root).unwrap();
        let names: String = std::fs::read_to_string(PB::from(env!("CARGO_MANIFEST_DIR")).join("../../data/flags.json")).unwrap();
        for (flow, quest, step, prefix) in &ORDER {
            assert!(names.contains(&format!("\"{}\"", exp_after(quest, step))), "{}", exp_after(quest, step));
            let mut b: Bf = load(&mut rf, flow);
            assert!(order(&mut b, flow, quest, step, prefix).unwrap() >= 1);
        }
        for l in &LATE {
            for n in [Some(l.done), l.when].into_iter().flatten() {
                assert!(names.contains(&format!("\"{n}\"")), "{n}");
            }
        }
        assert!(names.contains(&format!("\"{}\"", GENERATE.2)));
        assert!(names.contains(&format!("\"{CHILD}\"")));
    }

    #[test]
    fn the_crater_exit_sets_the_goron_flag_before_its_own_start() {
        let Some(root) = real() else {
            return;
        };
        let mut rf: Romfs = Romfs::open(&root).unwrap();
        let mut b: Bf = load(&mut rf, GENERATE.0);
        let old: u16 = entry(flow(&b, GENERATE.0), GENERATE.1).main;
        generate(&mut b).unwrap();
        let f: &Flow = flow(&b, GENERATE.0);
        let top: u16 = entry(f, GENERATE.1).main;
        let [(0, child), (1, set)] = cases(f, top)[..] else { panic!() };
        assert_eq!(child, old);
        let Kind::Action { next, params, .. } = &f.events[set as usize].kind else { panic!() };
        assert_eq!(*next, old);
        assert_eq!(params.as_ref().and_then(|p| p.get("GameDataName")), Some(&Val::Str(GENERATE.2.to_string())));
    }
}
