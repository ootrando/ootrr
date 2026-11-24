//! Story cutscenes skipped by writing what they would have written.
//!
//! - Deku Tree (with an open forest): the meeting (Dm_Kokiri_0070/0075) and the first arrival
//!   inside the tree (Dm_Deku_0000 plus Navi's dungeon lesson) are replaced by their writes at
//!   new game, so the mouth is open and the quest is where the arrival leaves it.
//! - Saria's gift: the bridge event goes straight to Dm_Kokiri_0155, the short gift
//!   scene that hands over the check's item, the way it does when the long Dm_Kokiri_0150 scene
//!   is skipped with the button. 0150 is marked as played there, which retires the trigger.
//! - Temple of Time as a child: the first entry and the altar scenes are marked as
//!   played at new game, with the flags the altar scene leaves behind.
//! - First Master Sword draw: the draw gives the sword and the item of Rauru's check,
//!   writes what the Rauru and Sheik scenes write, and hands over to the game's own later-draw
//!   event (Market_Child_Field_ChildToAdult), which swaps the equipment, makes Link an adult and
//!   loads the adult temple.
//!
//! Nothing here adds a call into another flow file. Such a call is only loaded when the event's
//! row in RSDB/GameAnalyzedEventInfo lists the file, and even with the row edited a get-item
//! call spliced into Saria's bridge event hung the game (2026-11-15, twice).

use std::fs;

use crate::bfevfl::{Bf, Cont, Event, Flow, Kind, Val};
use crate::error::{Error, Res};
use crate::qol::{edit_zs, Have};
use crate::romfs::Romfs;
use crate::staging::Out;
use crate::start_state::Ops;

pub const NOTE_DEKU: &str = "The Deku Tree is met at new game: its mouth is open, the Deku Tree quest is complete and the Forest Stone quest is where Link's first arrival inside the tree leaves it, so the meeting, the arrival scene and Navi's dungeon lesson are skipped.";
pub const NOTE: &str = "Story cutscenes are skipped. Saria's bridge scenes are cut: crossing the bridge gives the check's item with a plain item get window. The Temple of Time's first entry and altar scenes are marked as played. The first Master Sword draw gives the sword and the item of Rauru's check and then plays the short scene of a later draw, without the sword pull, Ganondorf, Rauru or Sheik scenes. Zelda's escape at the drawbridge is cut, so the Ocarina of Time waits in the moat right away. The Light Arrows scene only plays the short scene that hands over its check, and it no longer takes out the Master Sword, so Link keeps his equipment. The owls that stop Link (Hyrule Field, Zora's River, the Lost Woods) are cut and marked as met; the Death Mountain and Lake Hylia owls still offer their flights when spoken to. The burning village and Sheik's flashback before the Nocturne are cut: the village loads the way the game does, without either scene, into the Nocturne scene, and the scene no longer takes out the Master Sword. Sheik's song scenes go straight to the song: the talk and the three ocarina lessons before it and the talk after it are cut, and the song check is given as usual.";

const TRUE: u32 = 0x8000_0001;
const FALSE: u32 = 0;
const NONE: u16 = 0xFFFF;

const DATA_ACTOR: &str = "GameDataActor";
const SET_BOOL: &str = "EventTriggerSetGameDataBool";
const CHALLENGE_ACTOR: &str = "ChallengeSystemActor";
const PROGRESS: &str = "EventExecuteProgressChallengeStep";

// ---------------------------------------------------------------- Deku Tree (open forest)

/// Steps after the anchor (Meet_DekuTree, which the open forest already moves to Step03):
/// the meeting completes the Deku Tree quest and opens the Forest Stone quest, the first
/// arrival inside the tree moves it to MeetBoss.
pub const DEKU_STEPS: [(&str, &str); 3] = [("Meet_DekuTree", "Complete"), ("Get_StoneOfForest", "GoInside"), ("Get_StoneOfForest", "MeetBoss")];
pub const DEKU_ENUMS: [(&str, &str); 2] = [("Step_Meet_DekuTree", "Complete"), ("Step_Get_StoneOfForest", "MeetBoss")];
/// The mouth flag is what Dm_Kokiri_0075 writes (the tree model reads it). The dungeon entrance
/// plays its arrival only while Dm_Deku_0000 is unplayed; that arrival also runs Navi's dungeon
/// lesson, which gates the D-pad Navi talk, so the lesson's markers are written too (the same
/// four the Kakariko state writes).
pub const DEKU_FLAGS: [&str; 8] = [
    "KokiriForest_Meet_DekuTree_OpenDekuTreeMouth",
    "HavePlayedEvent.Dm_Kokiri_0070",
    "HavePlayedEvent.Dm_Kokiri_0075",
    "HavePlayedEvent.Dm_Deku_0000",
    "HavePlayedEvent.NaviHint_Trial",
    "HaveTalked.Npc_Navi",
    "ActionGuide_IsEnd.Navi",
    "ActionGuide_IsNew.Navi",
];

pub fn deku() -> Ops {
    Ops {
        steps: DEKU_STEPS.iter().map(|(c, s)| ((*c).to_string(), (*s).to_string())).collect(),
        enums: DEKU_ENUMS.iter().map(|(c, s)| ((*c).to_string(), (*s).to_string())).collect(),
        flags: DEKU_FLAGS.iter().map(|n| ((*n).to_string(), true)).collect(),
        ..Ops::default()
    }
}

// ---------------------------------------------------------------- new game state

/// The child Temple of Time: each trigger area exists only while its scene is unplayed. The
/// altar scene's flow clears the first-time light shaft and its area's logic sets the found flag.
pub const TEMPLE_FLAGS: [&str; 4] = [
    "HavePlayedEvent.Dm_TimeTemple_0000",
    "HavePlayedEvent.Dm_TimeTemple_0020",
    "Market_Child_TimeTemple_DeleteLightShaft_FirstTime_00",
    "Market_Child_FoundMasterSword",
];

/// Owls the maps start on their own while the scene is unplayed (Hyrule Field near the castle,
/// toward Gerudo Valley and toward Lake Hylia, Zora's River after Saria's Song). They write nothing
/// but their played flag. The Death Mountain and Lake Hylia flight owls only talk when spoken to
/// and are left alone, as are the dream owl (it opens the dream world) and the owl after the
/// Spirit Temple (other flows branch on its scene).
pub const OWL_FLAGS: [&str; 4] = [
    "HavePlayedEvent.Dm_HrlField_0030",
    "HavePlayedEvent.Dm_HrlField_0040",
    "HavePlayedEvent.Dm_HrlField_0050",
    "HavePlayedEvent.Dm_ZoraRiver_0005",
];

/// The always-on new game flags: the child Temple of Time and the owls.
pub fn start() -> Ops {
    Ops { flags: TEMPLE_FLAGS.iter().chain(OWL_FLAGS.iter()).map(|n| ((*n).to_string(), true)).collect(), ..Ops::default() }
}

// ---------------------------------------------------------------- flow helpers

fn flag(v: bool) -> Val {
    Val::Bool(if v { TRUE } else { FALSE })
}

fn item(k: &str, v: Val) -> (String, Val) {
    (k.to_string(), v)
}

fn idx(n: usize) -> Res<u16> {
    if n >= NONE as usize {
        return Err(Error::Format("story skip: too many events in the flow"));
    }
    Ok(n as u16)
}

fn find(f: &Flow, name: &str) -> Res<usize> {
    f.events
        .iter()
        .position(|e| e.name == name)
        .ok_or_else(|| Error::Msg(format!("story skip: {} has no event {name}, the game files differ", f.name)))
}

fn text<'a>(e: &'a Event, k: &str) -> Option<&'a str> {
    match e.kind.params()?.get(k)? {
        Val::Str(s) => Some(s.as_str()),
        _ => None,
    }
}

/// The action an event runs, as `actor.action`.
fn action(f: &Flow, i: usize) -> Option<String> {
    let Kind::Action { actor, act, .. } = &f.events[i].kind else {
        return None;
    };
    let a = f.actors.get(*actor as usize)?;
    Some(format!("{}.{}", a.name, a.actions.get(*act as usize)?))
}

fn next_of(k: &Kind) -> Option<u16> {
    match k {
        Kind::Action { next, .. } | Kind::Sub { next, .. } | Kind::Join { next } => Some(*next),
        _ => None,
    }
}

fn set_next(k: &mut Kind, to: u16) -> Res<()> {
    match k {
        Kind::Action { next, .. } | Kind::Sub { next, .. } | Kind::Join { next } => {
            *next = to;
            Ok(())
        }
        _ => Err(Error::Format("story skip: the event has no single next")),
    }
}

/// Appends a chain of events, each pointing at the following one and the last at `then`.
/// Returns the index of the first.
fn chain(f: &mut Flow, prefix: &str, kinds: Vec<Kind>, then: u16) -> Res<u16> {
    let base: usize = f.events.len();
    idx(base + kinds.len())?;
    let n: usize = kinds.len();
    for (i, mut k) in kinds.into_iter().enumerate() {
        set_next(&mut k, if i + 1 < n { idx(base + i + 1)? } else { then })?;
        f.events.push(Event { name: format!("{prefix}{}", base + i), kind: k });
    }
    idx(base)
}

fn set_bool(f: &mut Flow, name: &str, v: bool) -> Res<Kind> {
    let (a, x) = crate::start_state::slot(f, DATA_ACTOR, SET_BOOL)?;
    Ok(Kind::Action { next: NONE, actor: a, act: x, params: Some(Cont { items: vec![item("Index", Val::Int(-1)), item("GameDataName", Val::Str(name.to_string())), item("Value", flag(v))] }) })
}

fn step(f: &mut Flow, quest: &str, step: &str) -> Res<Kind> {
    let (a, x) = crate::start_state::slot(f, CHALLENGE_ACTOR, PROGRESS)?;
    Ok(Kind::Action {
        next: NONE,
        actor: a,
        act: x,
        params: Some(Cont {
            items: vec![
                item("ChallengeName", Val::Str(quest.to_string())),
                item("StepName", Val::Str(step.to_string())),
                item("IsIgnoreCompletedChallenge", flag(false)),
                item("IsIgnoreChallengeProgressUI", flag(true)),
                item("IsWaitEnd", flag(true)),
            ],
        }),
    })
}

fn guard(f: &Flow, prefix: &str) -> Res<()> {
    if f.events.iter().any(|e| e.name.starts_with(prefix)) {
        return Err(Error::Msg(format!("story skip: {} was already patched", f.name)));
    }
    Ok(())
}

fn flow_mut<'a>(b: &'a mut Bf, name: &str) -> Res<&'a mut Flow> {
    b.flows.iter_mut().find(|f| f.name == name).ok_or_else(|| Error::Msg(format!("story skip: no flow {name}")))
}

fn rebuild(raw: &[u8], edit: impl FnOnce(&mut Bf) -> Res<()>) -> Res<Vec<u8>> {
    let mut b: Bf = Bf::parse(raw)?;
    edit(&mut b)?;
    let out: Vec<u8> = b.write()?;
    if Bf::parse(&out)? != b {
        return Err(Error::Format("story skip: the patched flow does not parse back to the edited model"));
    }
    Ok(out)
}

/// The string parameter `param` of event `index` (checked by name) in a flow file.
fn param_of(raw: &[u8], index: usize, event: &str, param: &str) -> Res<String> {
    let b: Bf = Bf::parse(raw)?;
    let f: &Flow = b.flows.first().ok_or(Error::Format("story skip: the file holds no flow"))?;
    let e: &Event = f.events.get(index).ok_or_else(|| Error::Msg(format!("story skip: {} has no event {index}", f.name)))?;
    if e.name != event {
        return Err(Error::Msg(format!("story skip: event {index} of {} is {} and not {event}, the game files differ", f.name, e.name)));
    }
    text(e, param).map(str::to_string).ok_or_else(|| Error::Msg(format!("story skip: {event} of {} has no {param}", f.name)))
}

fn copy(f: &Flow, name: &str, want: &str) -> Res<Kind> {
    let i: usize = find(f, name)?;
    let k: Kind = f.events[i].kind.clone();
    let ok: bool = match &k {
        Kind::Sub { entry, .. } => entry == want,
        Kind::Action { .. } => action(f, i).as_deref() == Some(want),
        _ => false,
    };
    if !ok {
        return Err(Error::Msg(format!("story skip: {name} in {} is not {want}, the game files differ", f.name)));
    }
    Ok(k)
}

fn set_param(k: &mut Kind, key: &str, v: Val) -> Res<()> {
    let c: &mut Cont = k.params_mut().ok_or(Error::Format("story skip: the event has no parameters"))?;
    let slot: &mut Val = c.items.iter_mut().find(|(n, _)| n == key).map(|(_, v)| v).ok_or_else(|| Error::Msg(format!("story skip: the event has no {key}")))?;
    *slot = v;
    Ok(())
}

// ---------------------------------------------------------------- Saria's gift

pub const BRIDGE_FILE: &str = "Event/EventFlow/Goto_HyruleCastle.bfevfl.zs";
pub const BRIDGE_FLOW: &str = "Goto_HyruleCastle";
pub const BRIDGE_PREFIX: &str = "OOTRRSaria";
/// KokiriForest_ArriveLostWood: sound ducking -> Dm_Kokiri_0150 -> (skipped? fade out) ->
/// sound release -> Dm_Kokiri_0155 (the gift) -> ...
const DUCKING: &str = "Event58";
const LONG_SCENE: &str = "Event43";
const RELEASE: &str = "Event115";
const GIFT_SCENE: &str = "Event44";
/// The bridge trigger (KokiriForest_Child_Level_All_02_ddcd) is deleted once Dm_Kokiri_0150 has
/// played. Skipping the transition means the game never marks it, so the flow does.
pub const LONG_FLAG: &str = "HavePlayedEvent.Dm_Kokiri_0150";

/// The ducking now leads to a write of the 0150 flag and then to the sound release, which goes
/// on to the gift scene. Both scenes stay in the event's preload row.
pub fn patch_bridge(b: &mut Bf) -> Res<()> {
    let f: &mut Flow = flow_mut(b, BRIDGE_FLOW)?;
    guard(f, BRIDGE_PREFIX)?;
    let duck: usize = find(f, DUCKING)?;
    let long: usize = find(f, LONG_SCENE)?;
    let release: usize = find(f, RELEASE)?;
    let gift: usize = find(f, GIFT_SCENE)?;
    if text(&f.events[long], "EventName") != Some("Dm_Kokiri_0150") || text(&f.events[gift], "EventName") != Some("Dm_Kokiri_0155") {
        return Err(Error::Format("story skip: the bridge scenes are not where they were, the game files differ"));
    }
    if next_of(&f.events[duck].kind) != Some(idx(long)?) || next_of(&f.events[release].kind) != Some(idx(gift)?) {
        return Err(Error::Format("story skip: the bridge flow no longer runs ducking -> 0150 and release -> 0155"));
    }
    let k: Kind = set_bool(f, LONG_FLAG, true)?;
    let first: u16 = chain(f, BRIDGE_PREFIX, vec![k], idx(release)?)?;
    set_next(&mut f.events[duck].kind, first)?;
    Ok(())
}

/// Ported from Ontico's REALAZK/ootrr pull request 11 ("Let Saria's gift hold any pouch item").
///
/// The short gift scene. Its C01 cut shows the get window (Event77) and then, inside a fork,
/// adds the item (Event68); the location patcher points both at the check's item.
pub const GIFT_FILE: &str = "Event/EventFlow/Dm_Kokiri_0155.bfevfl.zs";
pub const GIFT_FLOW: &str = "Dm_Kokiri_0155";
pub const GIFT_ENTRY: &str = "EntryPoint0";
const GIFT_GRANT: &str = "Event68";
const GIFT_SHOW: &str = "Event77";
const POUCH_ADD: &str = "PouchSystemActor.EventTriggerAddOrRemovePouchContent";

/// Turns the gift scene into a plain item get: the entry point runs the grant, then the get
/// window, then ends, with no cuts, camera or animation.
///
/// The gift stays in its own scene file on purpose. A get-item call spliced into the bridge
/// event (Goto_HyruleCastle) hung the game twice, even with its preload row edited. This scene
/// already calls GetPouchContent:DisplayGetByActorName, so nothing new has to be loaded.
///
/// The window runs before the grant, as in the scene's own C01 cut. A progressive item placed
/// here has both events wrapped into tier checks (crate::progressive::wrap_grant, before this
/// patch runs), and both checks then see the pouch before the grant, so they pick the same tier.
pub fn patch_gift(b: &mut Bf) -> Res<()> {
    let f: &mut Flow = flow_mut(b, GIFT_FLOW)?;
    let grant: usize = find(f, GIFT_GRANT)?;
    let show: usize = find(f, GIFT_SHOW)?;
    let wrapped = |i: usize| matches!(f.events[i].kind, Kind::Switch { .. });


    let credited: bool = action(f, grant).is_some_and(|a| a.starts_with("GameDataActor.")) || crate::gift_keys::is_ring_credit(f, grant);
    if !wrapped(grant) && !credited && action(f, grant).as_deref() != Some(POUCH_ADD) {
        return Err(Error::Format("story skip: the gift scene's grant is not a pouch add, the game files differ"));
    }
    if !wrapped(show) && !matches!(&f.events[show].kind, Kind::Sub { flow, entry, .. } if flow == "GetPouchContent" && entry == "DisplayGetByActorName") {
        return Err(Error::Format("story skip: the gift scene's get window is not GetPouchContent:DisplayGetByActorName, the game files differ"));
    }
    let (g, s): (u16, u16) = (idx(grant)?, idx(show)?);
    // C01 holds the window's sub calls (one, or one per tier once wrapped).
    let subs: Vec<u16> = f.entries.iter().find(|e| e.name == "C01").map(|e| e.subs.clone()).ok_or(Error::Format("story skip: the gift scene has no C01 cut"))?;
    let e: &mut crate::bfevfl::Entry = f.entries.iter_mut().find(|e| e.name == GIFT_ENTRY).ok_or(Error::Format("story skip: the gift scene has no EntryPoint0"))?;
    if e.main == s {
        return Err(Error::Format("story skip: the gift scene was already patched"));
    }
    e.main = s;
    e.subs = subs;
    let after: u16 = ends(f, show)?;
    retarget(f, show, after, g)?;
    if !credited {
        retarget(f, grant, ends(f, grant)?, NONE)?;
    }
    Ok(())
}

/// The single `next` every path from `start` ends on: the event's own next, or, through a
/// tier check, the shared next of its tier copies.
fn ends(f: &Flow, start: usize) -> Res<u16> {
    let mut seen: Vec<u16> = Vec::new();
    let mut todo: Vec<usize> = vec![start];
    while let Some(i) = todo.pop() {
        match &f.events[i].kind {
            Kind::Switch { cases, .. } => todo.extend(cases.iter().map(|c| c.ev as usize)),
            k => {
                // A Bow or Slingshot tier is followed by its bag adds (crate::starter_bags::line).
                let n: u16 = crate::starter_bags::skip(f, next_of(k).ok_or(Error::Format("story skip: a gift event has no single next"))?);
                // Ammo given to a player without its bag is followed by its extra rupees
                // (crate::progressive::NO_BAG_EXTRA).
                let n: u16 = crate::progressive::skip_extra_rupees(f, n);
                if !seen.contains(&n) {
                    seen.push(n);
                }
            }
        }
    }
    match seen.as_slice() {
        [n] => Ok(*n),
        _ => Err(Error::Msg(format!("story skip: the gift event {} does not end on one next ({seen:?})", f.events[start].name))),
    }
}

/// Points every path from `start` that ends on `from` at `to` instead.
fn retarget(f: &mut Flow, start: usize, from: u16, to: u16) -> Res<()> {
    let mut todo: Vec<usize> = vec![start];
    while let Some(i) = todo.pop() {
        if let Kind::Switch { cases, .. } = &f.events[i].kind {
            todo.extend(cases.iter().map(|c| c.ev as usize));
        } else if let Some(n) = next_of(&f.events[i].kind) {
            // The end of a Bow or Slingshot tier's bag line is the end of the give.
            let at: usize = crate::starter_bags::line_end(f, n).unwrap_or(i);
            if next_of(&f.events[at].kind) == Some(from) {
                set_next(&mut f.events[at].kind, to)?;
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------- first Master Sword draw

pub const DOOR_FILE: &str = "Event/EventFlow/DoorOfTime.bfevfl.zs";
pub const DOOR_FLOW: &str = "DoorOfTime";
pub const DOOR_PREFIX: &str = "OOTRRDraw";
/// DoorOfTime_Draw: true when Ganondorf's scene has played (a later draw), false on the first.
const DRAW_SWITCH: &str = "Event23";
/// The later-draw branch: a transition to Market_Child_Field_ChildToAdult.
const LATER_DRAW: &str = "Event22";
const GIVE_SWORD: &str = "Event115";
/// The Recovery sub that follows the adult temple load in Rauru's path; door_fix guards the
/// quest completion after it.
const RECOVERY: &str = "Event102";
pub const LATER_EVENT: &str = "Market_Child_Field_ChildToAdult";
/// Rauru's grant, which the location patcher points at the check's item.
pub const RAURU_FILE: &str = "Event/EventFlow/Dm_SageRoom_0005.bfevfl.zs";
pub const RAURU_INDEX: usize = 182;
pub const RAURU_EVENT: &str = "Event48";
pub const RAURU_PARAM: &str = "ActorName";
/// Every later draw tells itself apart from the first by Dm_TimeTemple_0040. The rest are the
/// skipped scenes (Sheik's altar talk 0065 would start while the sages quest is at Step01) and
/// what Market_Adult_AllSage_Start writes after Sheik's scenes.
pub const DRAW_FLAGS: [&str; 9] = [
    "HavePlayedEvent.Dm_TimeTemple_0030",
    "HavePlayedEvent.Dm_TimeTemple_0040",
    "HavePlayedEvent.Dm_SageRoom_0000",
    "HavePlayedEvent.Dm_SageRoom_0005",
    "HavePlayedEvent.Dm_TimeTemple_0050",
    "HavePlayedEvent.Dm_TimeTemple_0055",
    "HavePlayedEvent.Dm_TimeTemple_0065",
    "Market_Child_Firsttime_GotoAdult",
    "HavePlayedEvent.NaviHint_Shield_Hylian",
];
/// The quest steps Market_Adult_AllSage_Start writes, in its order.
pub const DRAW_STEPS: [(&str, &str); 3] = [("SageOfLight", "Complete"), ("AllSage", "Step01"), ("SageOfForest", "Step01")];

/// The first draw went pull scene -> Chamber of Sages (Ganondorf, the sword, the era change)
/// -> Chamber of Sages (Rauru, his check) -> adult temple (Sheik, quest steps). It now writes
/// the flags, gives the sword and `medal` (plus its IsGet flag `got`, which a pouch add without
/// the get event may leave unset), writes the quest steps and transitions to the later-draw
/// event, which the draw's own event table row already lists.
///
/// The transition is the last thing the draw does, as in the later draw. A first version ran
/// Rauru's Recovery sub (a wait and a heal) after it and the game crashed while the child temple
/// was reloaded with Link already an adult (2026-11-15). The sword is not equipped directly
/// either: the era swap equips it for the adult, as on every later draw. The guarded quest
/// completion that followed Recovery (door_fix) runs before the transition instead.
pub fn patch_draw(b: &mut Bf, medal: &str, got: Option<&str>) -> Res<()> {
    let f: &mut Flow = flow_mut(b, DOOR_FLOW)?;
    guard(f, DOOR_PREFIX)?;
    let sw: usize = find(f, DRAW_SWITCH)?;
    let Kind::Switch { cases, params, .. } = &f.events[sw].kind else {
        return Err(Error::Format("story skip: the draw check is not a switch"));
    };
    if params.as_ref().and_then(|c| c.get("EventName")) != Some(&Val::Str("Dm_TimeTemple_0040".to_string())) || cases.len() != 2 {
        return Err(Error::Format("story skip: the draw check does not test Dm_TimeTemple_0040, the game files differ"));
    }
    let later: usize = find(f, LATER_DRAW)?;
    if text(&f.events[later], "EventName") != Some(LATER_EVENT) || action(f, later).as_deref() != Some("EventSystemActor.EventTransition") {
        return Err(Error::Format("story skip: the later draw no longer transitions to Market_Child_Field_ChildToAdult"));
    }
    let mut kinds: Vec<Kind> = Vec::new();
    for n in DRAW_FLAGS {
        kinds.push(set_bool(f, n, true)?);
    }
    let mut sword: Kind = copy(f, GIVE_SWORD, "PouchSystemActor.EventTriggerAddOrRemovePouchContent")?;
    if text(&f.events[find(f, GIVE_SWORD)?], "Actor") != Some("Weapon_Sword_Master") {
        return Err(Error::Format("story skip: the Ganondorf path no longer gives the Master Sword"));
    }
    set_param(&mut sword, "IsEquipDIrect", flag(false))?;
    let mut gift: Kind = sword.clone();
    set_param(&mut gift, "Actor", Val::Str(medal.to_string()))?;
    kinds.push(sword);
    kinds.push(gift);
    if let Some(n) = got {
        kinds.push(set_bool(f, n, true)?);
    }
    for (q, s) in DRAW_STEPS {
        kinds.push(step(f, q, s)?);
    }
    // The tail: [guard ->] [DoorOfTime Complete ->] transition (last, no next).
    let mut go: Kind = f.events[later].kind.clone();
    set_next(&mut go, NONE)?;
    let t: u16 = chain(f, DOOR_PREFIX, vec![go], NONE)?;
    let rec: usize = find(f, RECOVERY)?;
    if !matches!(&f.events[rec].kind, Kind::Sub { entry, .. } if entry == "Recovery") {
        return Err(Error::Format("story skip: Event102 is not the Recovery call, the game files differ"));
    }
    let after: u16 = next_of(&f.events[rec].kind).ok_or(Error::Format("story skip: the Recovery call has no next"))?;
    let is_complete = |f: &Flow, i: usize| text(&f.events[i], "ChallengeName") == Some("DoorOfTime") && text(&f.events[i], "StepName") == Some("Complete");
    let tail: u16 = match f.events.get(after as usize).map(|e| e.kind.clone()) {
        // door_fix's guard: case 0 (step06 or later) asks for Complete, the other case ends.
        Some(Kind::Switch { actor, query, cases, params }) => {
            let yes: u16 = cases.iter().find(|c| c.val == 0).map(|c| c.ev).ok_or(Error::Format("story skip: the quest guard has no true case"))?;
            if !is_complete(f, yes as usize) {
                return Err(Error::Format("story skip: the quest guard does not lead to the DoorOfTime Complete request"));
            }
            let mut done: Kind = f.events[yes as usize].kind.clone();
            set_next(&mut done, t)?;
            let c: u16 = chain(f, DOOR_PREFIX, vec![done], t)?;
            let cases = cases.iter().map(|x| crate::bfevfl::Case { val: x.val, ev: if x.val == 0 { c } else { t } }).collect();
            let g: u16 = idx(f.events.len())?;
            f.events.push(Event { name: format!("{DOOR_PREFIX}{g}"), kind: Kind::Switch { actor, query, cases, params } });
            g
        }
        Some(Kind::Action { .. }) if is_complete(f, after as usize) => {
            let mut done: Kind = f.events[after as usize].kind.clone();
            set_next(&mut done, t)?;
            chain(f, DOOR_PREFIX, vec![done], t)?
        }
        _ => return Err(Error::Format("story skip: Recovery is not followed by the DoorOfTime Complete request")),
    };
    let first: u16 = chain(f, DOOR_PREFIX, kinds, tail)?;
    let Kind::Switch { cases, .. } = &mut f.events[sw].kind else { unreachable!() };
    let c = cases.iter_mut().find(|c| c.val == 1).ok_or(Error::Format("story skip: the draw check has no false case"))?;
    c.ev = first;
    Ok(())
}

// ---------------------------------------------------------------- dungeon endings

pub const NOTE_ENDINGS: &str = "Dungeon endings are cut short. On the first clear, the Spiritual Stone or Medallion check is given in the boss room before Link warps out, and Link lands at the dungeon's exit after one fade: the Chamber of Sages stop, the Deku Tree's farewell, the Goron and Zora celebrations, each sage's awakening and parting words, the forest farewell, the village scene after the Fire Temple, Lake Hylia refilling and the last sage's scene are skipped, and their scenes are marked as played. The quest steps they wrote are still written.";
pub const ENDING_FILE: &str = "Event/EventFlow/Dm_SY_BancChange_EscapeDungeon.bfevfl.zs";
pub const ENDING_FLOW: &str = "Dm_SY_BancChange_EscapeDungeon";
pub const ENDING_PREFIX: &str = "OOTRREnding";
/// The scenes cut from the dungeon ending events in that flow. Each is a pre-rendered timeline
/// (the button skip goes past them too) with no flag, quest or item write of its own, except
/// Dm_Kokiri_0080 and Dm_Kokiri_0162, scripted scenes that only talk. The scene that hands over
/// the reward stays in each chain (Dm_Kokiri_0130, Dm_DeathMtTrail_0025, Dm_ZoraFountain_0030,
/// Dm_Awake_*_Get), as do the quest steps and flags the events write around them.
pub const CUT_SCENES: [&str; 16] = [
    // Deku Tree: his last words, farewell and death, and Link leaving the forest.
    "Dm_Kokiri_0080",
    "Dm_Kokiri_0095",
    "Dm_Kokiri_0090",
    "Dm_Kokiri_0140",
    // Dodongo's Cavern: the Goron celebration.
    "Dm_DeathMtTrail_0020",
    // Jabu-Jabu: Ruto at the fountain.
    "Dm_ZoraFountain_0020",
    // Forest Temple: Saria awakens, then the forest scenes outside.
    "Dm_Awake_Forest",
    "Dm_Kokiri_0158",
    "Dm_Kokiri_0160",
    "Dm_Kokiri_0162",
    // Fire Temple: Kakariko and Darunia awakens.
    "Dm_Kakariko_0025",
    "Dm_Awake_Fire",
    // Water Temple: Ruto awakens, Lake Hylia refills.
    "Dm_Awake_Water",
    "Dm_LakeHrl_0020",
    // Shadow and Spirit Temples: Impa and Nabooru awaken.
    "Dm_Awake_Shadow",
    "Dm_Awake_Spirit",
];

/// Turns every transition to a cut scene into a write of that scene's played flag, keeping its
/// next. Several maps read these flags, so they are set as if the scene had played.
pub fn patch_endings(b: &mut Bf) -> Res<usize> {
    let f: &mut Flow = flow_mut(b, ENDING_FLOW)?;
    guard(f, ENDING_PREFIX)?;
    let mut n: usize = 0;
    for scene in CUT_SCENES {
        let hits: Vec<usize> = (0..f.events.len())
            .filter(|&i| action(f, i).as_deref() == Some("EventSystemActor.EventTransition") && text(&f.events[i], "EventName") == Some(scene))
            .collect();
        if hits.is_empty() {
            return Err(Error::Msg(format!("story skip: no ending plays {scene}, the game files differ")));
        }
        for i in hits {
            let next: u16 = next_of(&f.events[i].kind).ok_or(Error::Format("story skip: a transition has no next"))?;
            let mut k: Kind = set_bool(f, &format!("HavePlayedEvent.{scene}"), true)?;
            set_next(&mut k, next)?;
            f.events[i].kind = k;
            f.events[i].name = format!("{ENDING_PREFIX}{i}");
            n += 1;
        }
    }
    Ok(n)
}

/// After the Fire Temple boss, SageOfFire_ClearEvent plays Dm_FireTemple_0030 and changes to
/// KakarikoVillage_Adult booting FireTemple_SageOfFire_Kakariko, which preloads the Chamber of
/// Sages, plays Dm_Kakariko_0025 (the village scene) and changes to SageRoom booting
/// FireTemple_SageOfFire_SageRoom. With 0025 cut, that boot event reserved the next change while
/// the village was still loading and Link froze at the village entrance. The Kakariko stop is
/// removed: the clear event writes 0025's played flag, preloads the Chamber and changes straight
/// to it with the Chamber boot (a copy of the boot event's own change).
pub const FIRE_PREFIX: &str = "OOTRRFire";
const FIRE_EXIT: (&str, &str) = ("Event19", "Dm_FireTemple_0030");
const FIRE_TO_VILLAGE: (&str, &str) = ("Event30", "FireTemple_SageOfFire_Kakariko");
const FIRE_PRELOAD: (&str, &str) = ("Event298", "GameSystemActor.EventTriggerBancRequestBeforehandLoad");
const FIRE_TO_CHAMBER: (&str, &str) = ("Event35", "FireTemple_SageOfFire_SageRoom");
const BANC_CHANGE: &str = "GameSystemActor.EventReserveBancChange";

pub fn patch_fire_ending(b: &mut Bf) -> Res<()> {
    let f: &mut Flow = flow_mut(b, ENDING_FLOW)?;
    guard(f, FIRE_PREFIX)?;
    let exit: usize = find_action(f, (FIRE_EXIT.0, "EventSystemActor.EventTransition"))?;
    if text(&f.events[exit], "EventName") != Some(FIRE_EXIT.1) {
        return Err(Error::Format("story skip: the Fire Temple exit no longer plays its scene, the game files differ"));
    }
    let village: usize = find_action(f, (FIRE_TO_VILLAGE.0, BANC_CHANGE))?;
    let chamber: usize = find_action(f, (FIRE_TO_CHAMBER.0, BANC_CHANGE))?;
    for (i, boot) in [(village, FIRE_TO_VILLAGE.1), (chamber, FIRE_TO_CHAMBER.1)] {
        if text(&f.events[i], "BootEventName") != Some(boot) {
            return Err(Error::Msg(format!("story skip: {} no longer boots {boot}, the game files differ", f.events[i].name)));
        }
    }
    if next_of(&f.events[exit].kind) != Some(idx(village)?) {
        return Err(Error::Format("story skip: the Fire Temple exit no longer leads to the village change"));
    }
    let preload: Kind = f.events[find_action(f, FIRE_PRELOAD)?].kind.clone();
    let mut to_chamber: Kind = f.events[chamber].kind.clone();
    set_next(&mut to_chamber, NONE)?;
    f.events[village].kind = to_chamber;
    f.events[village].name = format!("{FIRE_PREFIX}{village}");
    let flag: Kind = set_bool(f, "HavePlayedEvent.Dm_Kakariko_0025", true)?;
    let first: u16 = chain(f, FIRE_PREFIX, vec![flag, preload], idx(village)?)?;
    set_next(&mut f.events[exit].kind, first)?;
    Ok(())
}

/// The medallion scenes of the Chamber of Sages. Each gives its check, fades to white, restores
/// the boots, then waits a second, shows the sage's parting line(s) on the white screen and
/// waits about three more seconds (Forest and Fire then set AllSage_AwakeSage_Fire_Or_Forest).
pub const MEDAL_SCENES: [&str; 5] = ["Dm_Awake_Forest_Get", "Dm_Awake_Fire_Get", "Dm_Awake_Water_Get", "Dm_Awake_Shadow_Get", "Dm_Awake_Spirit_Get"];
const RESTORE_BOOTS: &str = "Player_Event.EventTriggerPlayerRestoreBootsChange";

/// Points the boots restore past the waits and talks that follow it, at the first event that is
/// neither (the flag write, or the end). Returns how many events were passed over.
pub fn cut_white_screen(b: &mut Bf) -> Res<usize> {
    let f: &mut Flow = b.flows.first_mut().ok_or(Error::Format("story skip: the medallion scene holds no flow"))?;
    let hits: Vec<usize> = (0..f.events.len()).filter(|&i| action(f, i).as_deref() == Some(RESTORE_BOOTS)).collect();
    let [r] = hits.as_slice() else {
        return Err(Error::Msg(format!("story skip: {} has {} boots restores, expected one", f.name, hits.len())));
    };
    let mut i: u16 = next_of(&f.events[*r].kind).ok_or(Error::Format("story skip: the boots restore has no next"))?;
    let mut cut: usize = 0;
    let mut talks: usize = 0;
    while i != NONE {
        let a: String = action(f, i as usize).unwrap_or_default();
        let skip: bool = a == "EventSystemActor.EventWait" || a.ends_with(".EventTalk");
        if !skip {
            break;
        }
        talks += usize::from(a.ends_with(".EventTalk"));
        cut += 1;
        i = next_of(&f.events[i as usize].kind).ok_or(Error::Format("story skip: a white screen event has no next"))?;
    }
    if talks == 0 {
        return Err(Error::Msg(format!("story skip: {} shows no line after the medallion, the game files differ", f.name)));
    }
    set_next(&mut f.events[*r].kind, i)?;
    Ok(cut)
}

pub fn medal_file(scene: &str) -> String {
    format!("Event/EventFlow/{scene}.bfevfl.zs")
}

/// Darunia's Goron Ruby scene. Its main entry runs its parts in order (sub :C01, :C01-05, :C01-07,
/// :C02 ... :C09); :C01-07 ends with the grant (the check), everything after is Darunia, the Gorons
/// and Navi talking, with no flag or item write, and :C09 closes with a listener reset, a fade out
/// and a one second wait. The grant part is pointed straight at that close.
pub const DARUNIA_FILE: &str = "Event/EventFlow/Dm_DeathMtTrail_0025.bfevfl.zs";
const DARUNIA_GRANT_PART: &str = "C01-07";
const LISTENER: &str = "SoundSystemActor.EventSoundChangeListenerPreset";
const FADE_OUT: &str = "UISystemActor.EventEventFadeOut";

pub fn cut_darunia_talk(b: &mut Bf) -> Res<()> {
    let f: &mut Flow = b.flows.first_mut().ok_or(Error::Format("story skip: Darunia's scene holds no flow"))?;
    let parts: Vec<usize> = (0..f.events.len()).filter(|&i| matches!(&f.events[i].kind, Kind::Sub { flow, entry, .. } if flow.is_empty() && entry == DARUNIA_GRANT_PART)).collect();
    let [part] = parts.as_slice() else {
        return Err(Error::Msg(format!("story skip: Darunia's scene runs {DARUNIA_GRANT_PART} {} times, expected once", parts.len())));
    };
    // The closing listener reset, the one whose next is the fade out.
    let close: Vec<usize> = (0..f.events.len())
        .filter(|&i| action(f, i).as_deref() == Some(LISTENER))
        .filter(|&i| next_of(&f.events[i].kind).is_some_and(|n| n != NONE && action(f, n as usize).as_deref() == Some(FADE_OUT)))
        .collect();
    let [close] = close.as_slice() else {
        return Err(Error::Msg(format!("story skip: Darunia's scene has {} closing fades, expected one", close.len())));
    };
    if next_of(&f.events[*part].kind) == Some(*close as u16) {
        return Err(Error::Format("story skip: Darunia's talk is already cut"));
    }
    set_next(&mut f.events[*part].kind, *close as u16)
}

// ---------------------------------------------------------------- Darunia's Joy

/// Darunia's Joy. Saria's Song to child Darunia runs Get_StoneOfFire entry
/// Get_StoneOfFire_PlayOcarina: when the melody is the one it asks for (EventQueryIsRecommendedMelody
/// case 0) it fades out, plays the dance timeline Dm_Goron_0025 (no flow, no writes), then reaches
/// the switch (Event39) that picks the bracelet scene Dm_Goron_0030 or Darunia's talk.
/// crate::open_world::darunia already makes that switch read HavePlayedEvent.Dm_Goron_0030, so the
/// gift is given once and does not follow the quest step. The melody case goes straight to that
/// switch, so the song leads to the item and a second song to the talk.
pub const STONE_FILE: &str = "Event/EventFlow/Get_StoneOfFire.bfevfl.zs";
const STONE_FLOW: &str = "Get_StoneOfFire";
const MELODY_QUERY: &str = "EventQueryIsRecommendedMelody";
const TRANSITION: &str = "EventSystemActor.EventTransition";
const DANCE: &str = "Dm_Goron_0025";
const BRACELET: &str = "Dm_Goron_0030";

/// The bracelet scene Dm_Goron_0030 runs its parts in a row of forks (sub :C01 to :C09, all talk,
/// dance and camera), then :C10 (the GetInEvent_Goron grant, the check) and :C11 (the bracelet
/// equip pose, Darunia back to his spot, the Get_StoneOfFire step DodongoCavernOpne and its
/// progress window). The main entry starts at the :C10 fork, and :C11 no longer plays the equip
/// pose, which only fits the Goron's Bracelet.
pub const BRACELET_FILE: &str = "Event/EventFlow/Dm_Goron_0030.bfevfl.zs";
const BRACELET_MAIN: &str = "EntryPoint0";
const BRACELET_GRANT_PART: &str = "C10";
const EQUIP_POSE: &str = "Gauntlet_Goron_Equip";

pub fn patch_darunia_song(b: &mut Bf) -> Res<()> {
    let f: &mut Flow = flow_mut(b, STONE_FLOW)?;
    let to = |f: &Flow, scene: &str| -> Vec<usize> { (0..f.events.len()).filter(|&i| action(f, i).as_deref() == Some(TRANSITION) && text(&f.events[i], "EventName") == Some(scene)).collect() };
    let [dance] = to(f, DANCE)[..] else {
        return Err(Error::Msg(format!("story skip: {STONE_FLOW} moves to {DANCE} other than once")));
    };
    let [gift] = to(f, BRACELET)[..] else {
        return Err(Error::Msg(format!("story skip: {STONE_FLOW} moves to {BRACELET} other than once")));
    };
    // The switch that picks the bracelet scene: the gift happens once.
    let picks: Vec<usize> = (0..f.events.len()).filter(|&i| matches!(&f.events[i].kind, Kind::Switch { cases, .. } if cases.iter().any(|c| c.ev as usize == gift))).collect();
    let [pick] = picks[..] else {
        return Err(Error::Msg(format!("story skip: {} switches in {STONE_FLOW} lead to {BRACELET}, expected one", picks.len())));
    };
    let asks: Vec<usize> = (0..f.events.len())
        .filter(|&i| match &f.events[i].kind {
            Kind::Switch { actor, query, .. } => f.actors.get(*actor as usize).and_then(|a| a.queries.get(*query as usize)).is_some_and(|q| q == MELODY_QUERY),
            _ => false,
        })
        .collect();
    // The melody check whose right melody case leads (through a fade) to the dance.
    let mut hit: Option<usize> = None;
    for i in asks {
        let Kind::Switch { cases, .. } = &f.events[i].kind else {
            continue;
        };
        if let Some(c) = cases.iter().find(|c| c.val == 0) {
            let first: usize = c.ev as usize;
            if first == dance || next_of(&f.events[first].kind) == Some(dance as u16) {
                hit = Some(i);
            }
        }
    }
    let i: usize = hit.ok_or_else(|| Error::Msg(format!("story skip: no melody check in {STONE_FLOW} leads to {DANCE}")))?;
    let Kind::Switch { cases, .. } = &mut f.events[i].kind else {
        return Err(Error::Format("story skip: the melody check is no switch"));
    };
    for c in cases.iter_mut().filter(|c| c.val == 0) {
        c.ev = idx(pick)?;
    }
    Ok(())
}

pub fn cut_bracelet_scene(b: &mut Bf) -> Res<()> {
    let f: &mut Flow = flow_mut(b, BRACELET)?;
    let parts: Vec<usize> = (0..f.events.len()).filter(|&i| matches!(&f.events[i].kind, Kind::Sub { flow, entry, .. } if flow.is_empty() && entry == BRACELET_GRANT_PART)).collect();
    let [part] = parts[..] else {
        return Err(Error::Msg(format!("story skip: {BRACELET} runs :{BRACELET_GRANT_PART} other than once")));
    };
    let fork: usize = (0..f.events.len())
        .find(|&i| matches!(&f.events[i].kind, Kind::Fork { forks, .. } if forks.contains(&(part as u16))))
        .ok_or_else(|| Error::Msg(format!("story skip: {BRACELET} runs :{BRACELET_GRANT_PART} outside a fork")))?;
    let e = f.entries.iter_mut().find(|e| e.name == BRACELET_MAIN).ok_or_else(|| Error::Msg(format!("story skip: {BRACELET} has no {BRACELET_MAIN}")))?;
    if e.main == idx(fork)? {
        return Err(Error::Format("story skip: the bracelet scene is already cut"));
    }
    e.main = idx(fork)?;
    let pose: Vec<u16> = (0..f.events.len()).filter(|&i| text(&f.events[i], "ASCommand") == Some(EQUIP_POSE)).map(idx).collect::<Res<Vec<u16>>>()?;
    if pose.is_empty() {
        return Err(Error::Msg(format!("story skip: {BRACELET} has no {EQUIP_POSE} pose")));
    }
    let mut dropped: usize = 0;
    for ev in f.events.iter_mut() {
        if let Kind::Fork { forks, .. } = &mut ev.kind {
            let n: usize = forks.len();
            forks.retain(|x| !pose.contains(x));
            dropped += n - forks.len();
        }
    }
    if dropped == 0 {
        return Err(Error::Msg(format!("story skip: no fork in {BRACELET} starts the {EQUIP_POSE} pose")));
    }
    Ok(())
}

pub fn stage_endings(rf: &mut Romfs, have: Have) -> Res<Vec<Out>> {
    let mut outs: Vec<Out> = vec![edit_zs(rf, have, ENDING_FILE, |raw| {
        rebuild(raw, |b| {
            patch_endings(b)?;
            patch_fire_ending(b)?;
            Ok(())
        })
    })?];
    // The location patcher may have staged these scenes already (their grant holds the check).
    outs.push(edit_zs(rf, have, DARUNIA_FILE, |raw| rebuild(raw, cut_darunia_talk))?);
    outs.push(edit_zs(rf, have, STONE_FILE, |raw| rebuild(raw, patch_darunia_song))?);
    // The location patcher stages the bracelet scene already (its grant holds the check).
    outs.push(edit_zs(rf, have, BRACELET_FILE, |raw| rebuild(raw, cut_bracelet_scene))?);
    for s in MEDAL_SCENES {
        // The location patcher may have staged the scene already (its grant holds the check).
        outs.push(edit_zs(rf, have, &medal_file(s), |raw| {
            rebuild(raw, |b| {
                cut_white_screen(b)?;
                Ok(())
            })
        })?);
    }
    Ok(outs)
}

pub fn worked_endings(rf: &mut Romfs, pk: &crate::modpack::Pack) -> Res<Vec<Out>> {
    let root: std::path::PathBuf = pk.work.join("romfs");
    let have = |p: &str| -> Option<Vec<u8>> { fs::read(p.split('/').fold(root.clone(), |a, s| a.join(s))).ok() };
    stage_endings(rf, &have)
}

// ---------------------------------------------------------------- light arrows

/// The Light Arrows scene lives in AllSage.bfevfl: entry Market_Adult_Field_GetArrowLight (the
/// adult Temple of Time area, AllSage Step02) and GetArrowLight_OcarinaBackTimeTemple (the Song
/// of Time warp, Dm_SY_Ocarina) both reach Event175, which forces the Master Sword out, then play
/// the timelines Dm_TimeTemple_0090/0095/0110/0115/0125 (Sheik is Zelda), the flowchart
/// Dm_TimeTemple_0130 (the Light Arrows check, nothing else) and the timeline 0135 (Ganondorf takes
/// Zelda), then Navi talks and the quests advance. The sword request becomes a flag write, the six
/// timelines become their played flags, and after the closing camera/Navi reset the chain fades in
/// and goes straight to the quest writes.
pub const ARROWS_FILE: &str = "Event/EventFlow/AllSage.bfevfl.zs";
pub const ARROWS_FLOW: &str = "AllSage";
pub const ARROWS_PREFIX: &str = "OOTRRArrows";
const ARROWS_SWORD: (&str, &str) = ("Event175", "Player_Event.EventTriggerEquipmentUserRequestCreateStaticEquipment");
const ARROWS_CUT: [(&str, &str); 6] = [
    ("Event12", "Dm_TimeTemple_0090"),
    ("Event13", "Dm_TimeTemple_0095"),
    ("Event14", "Dm_TimeTemple_0110"),
    ("Event16", "Dm_TimeTemple_0115"),
    ("Event63", "Dm_TimeTemple_0125"),
    ("Event42", "Dm_TimeTemple_0135"),
];
const ARROWS_GRANT: (&str, &str) = ("Event17", "Dm_TimeTemple_0130");
/// After 0135: Event116 (camera, Navi and Link reset fork) -> join -> Event179 (unequip) -> Event97 (fade in) -> Event10 (AllSage Complete).
const ARROWS_RESET: &str = "Event116";
const ARROWS_UNEQUIP: (&str, &str) = ("Event179", "Player_Event.EventPlayerUnequip");
const ARROWS_FADE_IN: (&str, &str) = ("Event97", "UISystemActor.EventEventFadeIn");
const ARROWS_DONE: (&str, &str) = ("Event10", "ChallengeSystemActor.EventExecuteProgressChallengeStep");

fn find_action(f: &Flow, (name, want): (&str, &str)) -> Res<usize> {
    let i: usize = find(f, name)?;
    if action(f, i).as_deref() != Some(want) {
        return Err(Error::Msg(format!("story skip: {name} in {} is not {want}, the game files differ", f.name)));
    }
    Ok(i)
}

/// Turns the transition `name` (checked to play `scene`) into a write of the scene's played flag,
/// keeping its next unless `to` is given.
fn transition_to_flag(f: &mut Flow, prefix: &str, name: &str, scene: &str, to: Option<u16>) -> Res<usize> {
    let i: usize = find_action(f, (name, "EventSystemActor.EventTransition"))?;
    if text(&f.events[i], "EventName") != Some(scene) {
        return Err(Error::Msg(format!("story skip: {name} in {} does not play {scene}, the game files differ", f.name)));
    }
    let next: u16 = match to {
        Some(t) => t,
        None => next_of(&f.events[i].kind).ok_or(Error::Format("story skip: a transition has no next"))?,
    };
    let mut k: Kind = set_bool(f, &format!("HavePlayedEvent.{scene}"), true)?;
    set_next(&mut k, next)?;
    f.events[i].kind = k;
    f.events[i].name = format!("{prefix}{i}");
    Ok(i)
}

pub fn patch_light_arrows(b: &mut Bf) -> Res<()> {
    let f: &mut Flow = flow_mut(b, ARROWS_FLOW)?;
    guard(f, ARROWS_PREFIX)?;
    let grant: usize = find_action(f, (ARROWS_GRANT.0, "EventSystemActor.EventTransition"))?;
    if text(&f.events[grant], "EventName") != Some(ARROWS_GRANT.1) {
        return Err(Error::Format("story skip: AllSage no longer plays the Light Arrows scene"));
    }
    let reset: usize = find(f, ARROWS_RESET)?;
    if !matches!(f.events[reset].kind, Kind::Fork { .. }) {
        return Err(Error::Format("story skip: the Light Arrows reset is not a fork, the game files differ"));
    }
    let unequip: usize = find_action(f, ARROWS_UNEQUIP)?;
    let fade: usize = find_action(f, ARROWS_FADE_IN)?;
    let done: usize = find_action(f, ARROWS_DONE)?;
    // The forced Master Sword: same next, now a harmless flag write (0090 is set again below).
    let sword: usize = find_action(f, ARROWS_SWORD)?;
    let next: u16 = next_of(&f.events[sword].kind).ok_or(Error::Format("story skip: the sword request has no next"))?;
    let mut k: Kind = set_bool(f, "HavePlayedEvent.Dm_TimeTemple_0090", true)?;
    set_next(&mut k, next)?;
    f.events[sword].kind = k;
    f.events[sword].name = format!("{ARROWS_PREFIX}{sword}");
    let reset: u16 = idx(reset)?;
    for (name, scene) in ARROWS_CUT {
        let to: Option<u16> = (scene == "Dm_TimeTemple_0135").then_some(reset);
        transition_to_flag(f, ARROWS_PREFIX, name, scene, to)?;
    }
    set_next(&mut f.events[unequip].kind, idx(fade)?)?;
    set_next(&mut f.events[fade].kind, idx(done)?)?;
    Ok(())
}

/// What a check whose scene is dropped gives: the placed item's actor, its upgrade family when it
/// is one, and its dungeon credit when it is a dungeon item.
#[derive(Debug, Clone, PartialEq)]
pub struct SceneGive {
    pub actor: String,
    pub family: Option<&'static str>,
    pub key: Option<(crate::gift_keys::Item, &'static str)>,
    pub line: Option<String>,
}

/// The checks whose scenes are dropped for the standard item get.
pub const DROPPED: [&str; 3] = ["ToT Light Arrows Cutscene", "LW Gift from Saria", "ToT Reward from Rauru"];

/// The standard item get (the item shown, Link holding it up, the get window and the pouch add)
/// that the minigames run with no NPC.
pub const ITEM_GET: (&str, &str) = ("Dm_SY_GetInEvent", "GetInEvent_NotTurn_NotNpc");

/// Adds the item get for `g` to `f`, ending on `then`, and returns its first event. A dungeon item
/// runs the item get's window-only copy (crate::item_window), then its credit; an upgrade gets its
/// tier checks. The call joins the preload list (subs) of `entries`.
fn item_get(f: &mut Flow, name: &str, g: &SceneGive, then: u16, entries: &[&str]) -> Res<u16> {
    if f.events.iter().any(|e| e.name == name) {
        return Err(Error::Msg(format!("story skip: the item get {name} is already in {}", f.name)));
    }
    let then: u16 = match &g.line {
        Some(id) => {
            let (t, say): (u16, u16) = crate::dungeon_credit::action_slot(f, crate::dungeon_credit::TALKER, crate::dungeon_credit::TALK)?;
            let n: u16 = idx(f.events.len())?;
            f.events.push(Event { name: format!("{name}Dbg"), kind: Kind::Action { next: then, actor: t, act: say, params: Some(crate::dungeon_credit::talk(id.clone())) } });
            n
        }
        None => then,
    };
    let at: u16 = idx(f.events.len())?;
    // A small key is held up as the minigame key through the key copy, whose window names the
    // dungeon key (crate::gift_keys::SMALL_KEY_SHOWN).
    let small: bool = matches!(g.key, Some((crate::gift_keys::Item::Small, _))) && g.actor == crate::gift_keys::SMALL_KEY_ITEM;
    let ring: bool = matches!(g.key, Some((crate::gift_keys::Item::Ring(_), _)));
    let window: String = if small { crate::item_window::key_name(ITEM_GET.1) } else { crate::item_window::name(ITEM_GET.1) };

    let entry: &str = if g.key.is_some() && !ring { &window } else { ITEM_GET.1 };
    let actor: String = if small { crate::gift_keys::SMALL_KEY_SHOWN.to_string() } else { g.actor.clone() };
    f.events.push(Event {
        name: name.to_string(),
        kind: Kind::Sub {
            next: then,
            params: Some(Cont { items: vec![("ActorName".to_string(), Val::Str(actor))] }),
            flow: ITEM_GET.0.to_string(),
            entry: entry.to_string(),
        },
    });
    for e in f.entries.iter_mut().filter(|e| entries.contains(&e.name.as_str())) {
        e.subs.push(at);
    }
    if let Some(family) = g.family {
        crate::progressive::wrap_grant_as(f, at as usize, name, "ActorName", family, &crate::ocarina_hold::forms_of(family))?;
    } else {
        held(f, at as usize, "ActorName")?;
        // Ammo that needs a bag gives a green rupee to a player without one.
        if g.key.is_none() {
            crate::progressive::ammo_guard(f, at as usize, name, "ActorName")?;
            crate::starter_bags::follow(f, at as usize, &g.actor)?;
        }
    }
    if let Some((item, dungeon)) = g.key {
        let first: u16 = crate::gift_keys::credit(f, item, dungeon, then)?;
        set_next(&mut f.events[at as usize].kind, first)?;
    }
    Ok(at)
}

/// The item get at `at` holds up the actor in its `param`. An item with no animation (an Ocarina or
/// a song) is handed out as its pickup actor, then added like Saria's gift (crate::ocarina_hold).
/// An actor the location patcher already turned into the pickup actor gets the add too.
fn held(f: &mut Flow, at: usize, param: &str) -> Res<()> {
    let actor: String = match f.events[at].kind.params().and_then(|c| c.get(param)) {
        Some(Val::Str(s)) => s.clone(),
        _ => return Ok(()),
    };
    let (form, tier): (String, String) = match (crate::ocarina_hold::shown(&actor), crate::ocarina_hold::tier_of(&actor)) {
        (Some(p), _) => (p, actor),
        (None, Some(t)) => (actor, t.to_string()),
        (None, None) => return Ok(()),
    };
    if let Some(c) = f.events[at].kind.params_mut()
        && let Some((_, v)) = c.items.iter_mut().find(|(k, _)| k == param)
    {
        *v = Val::Str(form);
    }
    let then: u16 = next_of(&f.events[at].kind).ok_or(Error::Format("story skip: the item get has no next"))?;
    let added: u16 = crate::ocarina_hold::after(f, &tier, then)?;
    set_next(&mut f.events[at].kind, added)
}

/// The one event of `f` whose GameDataName parameter is `flag`.
fn writes(f: &Flow, flag: &str) -> Res<usize> {
    let hits: Vec<usize> = (0..f.events.len())
        .filter(|&i| f.events[i].kind.params().is_some_and(|c| c.items.iter().any(|(k, v)| k == "GameDataName" && matches!(v, Val::Str(s) if s == flag))))
        .collect();
    match hits.as_slice() {
        [i] => Ok(*i),
        _ => Err(Error::Msg(format!("story skip: {} writes {flag} {} times, expected once", f.name, hits.len()))),
    }
}

const ARROWS_ENTRIES: [&str; 2] = ["Market_Adult_Field_GetArrowLight", "GetArrowLight_OcarinaBackTimeTemple"];
const ARROWS_GIVE: &str = "OOTRRArrowsGive";
/// Market_Adult_Field_GetArrowLight: the event start and the unequip after its fade out.
const ARROWS_START: (&str, &str) = ("Event164", "Player_Event.EventTriggerPlayerEventStartWithDrawn");
const ARROWS_FIRST_UNEQUIP: (&str, &str) = ("Event165", "Player_Event.EventPlayerUnequip");

/// Drops the Light Arrows scene (Dm_TimeTemple_0130) entirely: its transition becomes its played
/// flag and Link runs the standard item get with the placed item, then the quest steps follow as
/// before. No fades or camera moves around it: the event start goes straight to the unequip and
/// the scene flags (skipping the player camera, the slow walk stop and the fade out), and the last
/// scene flag straight to the item get (skipping the camera and Navi reset, the second unequip and
/// the fade in), and the wait for the scenes' actors (Event81) between the scene flags is passed
/// over. The event takes the settings of Saria's bridge event (ARROWS_ROW, plain_row). Runs after
/// patch_light_arrows.
pub fn patch_arrows_give(b: &mut Bf, g: &SceneGive) -> Res<()> {
    let f: &mut Flow = flow_mut(b, ARROWS_FLOW)?;
    let fade: usize = find_action(f, ARROWS_FADE_IN)?;
    let done: u16 = next_of(&f.events[fade].kind).ok_or(Error::Format("story skip: the Light Arrows fade in has no next"))?;
    if action(f, done as usize).as_deref() != Some(ARROWS_DONE.1) {
        return Err(Error::Format("story skip: run patch_light_arrows before the Light Arrows item get"));
    }
    transition_to_flag(f, ARROWS_PREFIX, ARROWS_GRANT.0, ARROWS_GRANT.1, None)?;
    let at: u16 = item_get(f, ARROWS_GIVE, g, done, &ARROWS_ENTRIES)?;
    let start: usize = find_action(f, ARROWS_START)?;
    let unequip: usize = find_action(f, ARROWS_FIRST_UNEQUIP)?;
    set_next(&mut f.events[start].kind, idx(unequip)?)?;
    let last: usize = writes(f, "HavePlayedEvent.Dm_TimeTemple_0135")?;
    set_next(&mut f.events[last].kind, at)?;
    let wait: usize = find_action(f, ARROWS_WAIT)?;
    let after: u16 = next_of(&f.events[wait].kind).ok_or(Error::Format("story skip: the Light Arrows actor wait has no next"))?;
    let w: u16 = idx(wait)?;
    for e in f.events.iter_mut() {
        if let Kind::Action { next, .. } | Kind::Sub { next, .. } | Kind::Join { next } = &mut e.kind
            && *next == w
        {
            *next = after;
        }
    }
    Ok(())
}

pub const ARROWS_ROW: &str = "Market_Adult_Field_GetArrowLight";
const ARROWS_WAIT: (&str, &str) = ("Event81", "GameSystemActor.EventTraverseAroundAndWaitCreateActor");

pub fn stage_arrows_row(rf: &mut Romfs, late: &mut Vec<Out>, work: &std::path::Path) -> Res<()> {
    stage_late_file(rf, late, work, crate::progressive::SETTING_TABLE, |raw| {
        let mut y: crate::byml::Byml = crate::byml::Byml::parse(raw)?;
        plain_row(&mut y, ARROWS_ROW)?;
        y.write()
    })
}

/// Adds the item get to the story skip's AllSage edit in `late` (or to the game's AllSage).
pub fn stage_arrows_give(rf: &mut Romfs, late: &mut Vec<Out>, g: &SceneGive) -> Res<()> {
    crate::dungeon_grant::merge(rf, late, ARROWS_FILE, |raw| rebuild(raw, |b| patch_arrows_give(b, g)))
}

/// The Spiritual Stone and Medallion checks, their reward scenes in the dungeon ending flow
/// (Dm_SY_BancChange_EscapeDungeon), and the flags those scenes write of their own.
pub const REWARD_SCENES: [(&str, &str, &[&str]); 8] = [
    ("Queen Gohma", "Dm_Kokiri_0130", &[]),
    ("King Dodongo", "Dm_DeathMtTrail_0025", &[]),
    ("Barinade", "Dm_ZoraFountain_0030", &[]),
    ("Phantom Ganon", "Dm_Awake_Forest_Get", &["AllSage_AwakeSage_Fire_Or_Forest"]),
    ("Volvagia", "Dm_Awake_Fire_Get", &["AllSage_AwakeSage_Fire_Or_Forest"]),
    ("Morpha", "Dm_Awake_Water_Get", &[]),
    ("Bongo Bongo", "Dm_Awake_Shadow_Get", &[]),
    ("Twinrova", "Dm_Awake_Spirit_Get", &[]),
];
const REWARD_PREFIX: &str = "OOTRRReward";
const FADE_IN: (&str, &str) = ("UISystemActor", "EventEventFadeIn");

/// The entries of `f` whose events reach event `i`.
fn entries_reaching(f: &Flow, i: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for e in &f.entries {
        let mut seen: Vec<bool> = vec![false; f.events.len()];
        let mut todo: Vec<u16> = vec![e.main];
        while let Some(x) = todo.pop() {
            let x: usize = x as usize;
            if x >= f.events.len() || seen[x] {
                continue;
            }
            seen[x] = true;
            match &f.events[x].kind {
                Kind::Action { next, .. } | Kind::Sub { next, .. } | Kind::Join { next } => todo.push(*next),
                Kind::Switch { cases, .. } | Kind::SubSwitch { cases, .. } => todo.extend(cases.iter().map(|c| c.ev)),
                Kind::Fork { join, forks } => {
                    todo.push(*join);
                    todo.extend(forks.iter().copied());
                }
                Kind::Label { .. } => {}
            }
        }
        if seen[i] {
            out.push(e.name.clone());
        }
    }
    out
}

/// Each Spiritual Stone and Medallion check is given in the boss room, before Link warps out:
/// - The reward scene's transition becomes its played flag (and the flags the scene wrote), then a
///   fade in (the landing event may still be black after the skipped scenes before it), and the
///   flow goes on as after the scene.
/// - In the boss room's blue warp event (BOSS_WARPS), the first visit's path (case 1 of its first
///   switch, the dungeon not yet done) runs, after the warp choice and before the warp scene
///   (Link turning into light), Link joining and unequipping and the standard item get with what
///   the check holds.
/// - The Chamber of Sages stop between the boss room and the landing (SAGE_STOPS) is skipped: the
///   boss room's change to it becomes the stop's own writes, its preload and its change to the
///   landing, so Link lands after one fade (as patch_fire_ending does for the Kakariko stop).
/// Runs after patch_endings and patch_fire_ending.
pub fn patch_reward_gives(b: &mut Bf, gives: &[(&str, &[&str], SceneGive)]) -> Res<()> {
    let f: &mut Flow = flow_mut(b, ENDING_FLOW)?;
    for (scene, extra, g) in gives {
        let hits: Vec<usize> = (0..f.events.len())
            .filter(|&i| action(f, i).as_deref() == Some("EventSystemActor.EventTransition") && text(&f.events[i], "EventName") == Some(*scene))
            .collect();
        let [i] = hits.as_slice() else {
            return Err(Error::Msg(format!("story skip: the dungeon endings play {scene} {} times, expected once", hits.len())));
        };
        let i: usize = *i;
        let next: u16 = next_of(&f.events[i].kind).ok_or(Error::Format("story skip: a reward scene has no next"))?;
        let mut kinds: Vec<Kind> = Vec::new();
        for flag in *extra {
            kinds.push(set_bool(f, flag, true)?);
        }
        let (a, x): (u16, u16) = crate::dungeon_credit::action_slot(f, FADE_IN.0, FADE_IN.1)?;
        kinds.push(Kind::Action {
            next: NONE,
            actor: a,
            act: x,
            params: Some(Cont {
                items: vec![
                    ("Color".to_string(), Val::Int(0)),
                    ("IsNoPlayAnime".to_string(), Val::Bool(0)),
                    ("Length".to_string(), Val::Int(1)),
                    ("IsWaitEnd".to_string(), Val::Bool(1)),
                ],
            }),
        });
        let first: u16 = chain(f, &format!("{REWARD_PREFIX}{scene}_"), kinds, next)?;
        let name: String = f.events[i].name.clone();
        transition_to_flag(f, REWARD_PREFIX, &name, scene, Some(first))?;
        let boss: &str = BOSS_WARPS.iter().find(|(s, _)| s == scene).map(|(_, e)| *e).ok_or_else(|| Error::Msg(format!("story skip: no blue warp is known for {scene}")))?;
        give_at_warp(f, boss, scene, g)?;
    }
    for (boot, entry) in SAGE_STOPS {
        skip_stop(f, boot, entry)?;
    }
    Ok(())
}

pub const BOSS_WARPS: [(&str, &str); 8] = [
    ("Dm_Kokiri_0130", "BossArea_WarpIn"),
    ("Dm_DeathMtTrail_0025", "Get_StoneOfFire_EscapeDungeon"),
    ("Dm_ZoraFountain_0030", "Get_StoneOfWater_LeaveFromJabuJabuBelly"),
    ("Dm_Awake_Forest_Get", "ForestTemple_SageOfForest_EscapeFromTemple"),
    ("Dm_Awake_Fire_Get", "SageOfFire_ClearEvent"),
    ("Dm_Awake_Water_Get", "SageOfWater_ClearEvent"),
    ("Dm_Awake_Shadow_Get", "ShadowTemple_BossArea_WarpIn"),
    ("Dm_Awake_Spirit_Get", "SageOfSpirit_Clear"),
];

pub const SAGE_STOPS: [(&str, &str); 5] = [
    ("ForestTemple_SageOfForest_SageRoom", "ForestTemple_SageOfForest_SageRoom"),
    ("FireTemple_SageOfFire_SageRoom", "FireTemple_SageOfFire_SageRoom"),
    ("SageOfWater_SageRoom", "SageOfWater_SageRoom"),
    ("ShadowTemple_SageOfShadow_SageRoom", "ShadowTemple_Warp_ChamberOfSages"),
    ("SpiritTemple_SageOfSpirit_SageRoom", "SpiritTemple_SageOfSpirit_SageRoom"),
];
const LAST_SAGE: (&str, &str) = ("SageRoom_Allsage_LastSage", "SageRoom_AllSage_LastSage");
const PRELOAD: &str = "GameSystemActor.EventTriggerBancRequestBeforehandLoad";

fn leads_to(f: &Flow, to: u16) -> Vec<usize> {
    (0..f.events.len())
        .filter(|&j| match &f.events[j].kind {
            Kind::Action { next, .. } | Kind::Sub { next, .. } | Kind::Join { next } => *next == to,
            Kind::Switch { cases, .. } | Kind::SubSwitch { cases, .. } => cases.iter().any(|c| c.ev == to),
            Kind::Fork { join, forks } => *join == to || forks.contains(&to),
            _ => false,
        })
        .collect()
}

fn redirect(f: &mut Flow, from: usize, to: u16, head: u16) -> Res<()> {
    match &mut f.events[from].kind {
        Kind::Action { next, .. } | Kind::Sub { next, .. } | Kind::Join { next } => *next = head,
        Kind::Switch { cases, .. } | Kind::SubSwitch { cases, .. } => {
            for c in cases.iter_mut().filter(|c| c.ev == to) {
                c.ev = head;
            }
        }
        _ => return Err(Error::Format("story skip: a fork leads to a part of a dungeon ending that changes")),
    }
    Ok(())
}

fn give_at_warp(f: &mut Flow, entry: &str, scene: &str, g: &SceneGive) -> Res<()> {
    let mut at: u16 = f.entries.iter().find(|e| e.name == entry).map(|e| e.main).ok_or_else(|| Error::Msg(format!("story skip: the ending flow has no {entry}")))?;
    let mut from: Option<usize> = None;
    let mut first_switch: bool = true;
    for _ in 0..16 {
        let i: usize = at as usize;
        if i >= f.events.len() {
            break;
        }
        if action(f, i).as_deref() == Some("EventSystemActor.EventTransition") {
            let p: usize = from.ok_or_else(|| Error::Msg(format!("story skip: {entry} starts on its warp scene")))?;
            let k1: Kind = player(f, "EventTriggerParticipateEvent", None)?;
            let k2: Kind = player(f, "EventPlayerUnequip", Some(Cont { items: vec![item("IsWaitEnd", Val::Bool(1))] }))?;
            let give: u16 = item_get(f, &format!("{REWARD_PREFIX}{scene}"), g, at, &[entry])?;
            let head: u16 = chain(f, &format!("{REWARD_PREFIX}{scene}_Join_"), vec![k1, k2], give)?;
            return redirect(f, p, at, head);
        }
        let next: u16 = match &f.events[i].kind {
            Kind::Switch { cases, .. } if first_switch => {
                first_switch = false;
                cases.iter().find(|c| c.val == 1).map(|c| c.ev).ok_or_else(|| Error::Msg(format!("story skip: {entry} has no first visit case")))?
            }
            Kind::SubSwitch { cases, .. } if cases.len() == 1 => cases[0].ev,
            Kind::Action { next, .. } => *next,
            _ => return Err(Error::Msg(format!("story skip: {entry} does not reach its warp scene as expected, the game files differ"))),
        };
        from = Some(i);
        at = next;
    }
    Err(Error::Msg(format!("story skip: {entry} does not reach its warp scene, the game files differ")))
}

fn skip_stop(f: &mut Flow, boot: &str, entry: &str) -> Res<()> {
    let main: u16 = f.entries.iter().find(|e| e.name == entry).map(|e| e.main).ok_or_else(|| Error::Msg(format!("story skip: the ending flow has no {entry}")))?;
    let mut writes: Vec<Kind> = Vec::new();
    let mut preload: Option<Kind> = None;
    let mut at: u16 = main;
    let mut steps: usize = 0;
    let change: Kind = loop {
        steps += 1;
        let i: usize = at as usize;
        if i >= f.events.len() || steps > 64 {
            return Err(Error::Msg(format!("story skip: the {entry} stop does not reach its change, the game files differ")));
        }
        let what: String = action(f, i).unwrap_or_default();
        let k: Kind = f.events[i].kind.clone();
        if what == BANC_CHANGE {
            let mut c: Kind = k;
            set_next(&mut c, NONE)?;
            break c;
        }
        if what == PRELOAD {
            preload = Some(k.clone());
        } else if what.starts_with("GameDataActor.") || what.starts_with("ChallengeSystemActor.") {
            writes.push(k.clone());
        } else if what == "EventSystemActor.EventTransition" && text(&f.events[i], "EventName") == Some(LAST_SAGE.0) {
            writes.push(Kind::Sub { next: NONE, params: None, flow: ARROWS_FLOW.to_string(), entry: LAST_SAGE.1.to_string() });
        } else if !(what == "Player_Event.EventTriggerParticipateEvent" || what.starts_with("UISystemActor.EventEventFade")) {
            return Err(Error::Msg(format!("story skip: the {entry} stop runs {} ({what}), which the skip does not know", f.events[i].name)));
        }
        at = next_of(&k).ok_or_else(|| Error::Msg(format!("story skip: {} in {entry} has no single next", f.events[i].name)))?;
    };
    let changes: Vec<usize> = (0..f.events.len()).filter(|&i| action(f, i).as_deref() == Some(BANC_CHANGE) && text(&f.events[i], "BootEventName") == Some(boot)).collect();
    if changes.is_empty() {
        return Err(Error::Msg(format!("story skip: nothing changes to {boot} any more, the game files differ")));
    }
    for c in changes {
        let mut kinds: Vec<Kind> = writes.clone();
        kinds.extend(preload.clone());
        kinds.push(change.clone());
        let head: u16 = chain(f, &format!("{ENDING_PREFIX}Stop_{entry}_"), kinds, NONE)?;
        let to: u16 = idx(c)?;
        for p in leads_to(f, to) {
            if action(f, p).as_deref() == Some(PRELOAD) {
                let pp: u16 = idx(p)?;
                for q in leads_to(f, pp) {
                    redirect(f, q, pp, head)?;
                }
            } else {
                redirect(f, p, to, head)?;
            }
        }
        for e in f.entries.iter_mut().filter(|e| e.main == to) {
            e.main = head;
        }
        let calls: Vec<u16> = (usize::from(head)..f.events.len()).filter(|&x| matches!(f.events[x].kind, Kind::Sub { .. })).map(idx).collect::<Res<Vec<u16>>>()?;
        let reaching: Vec<String> = entries_reaching(f, usize::from(head));
        for e in f.entries.iter_mut().filter(|e| reaching.contains(&e.name)) {
            for x in &calls {
                if !e.subs.contains(x) {
                    e.subs.push(*x);
                }
            }
        }
    }
    Ok(())
}

pub fn patch_last_sage(b: &mut Bf) -> Res<()> {
    let f: &mut Flow = flow_mut(b, ARROWS_FLOW)?;
    let hits: Vec<String> = (0..f.events.len())
        .filter(|&i| action(f, i).as_deref() == Some("EventSystemActor.EventTransition") && text(&f.events[i], "EventName") == Some("Dm_SageRoom_0010"))
        .map(|i| f.events[i].name.clone())
        .collect();
    let [name] = hits.as_slice() else {
        return Err(Error::Msg(format!("story skip: AllSage plays Dm_SageRoom_0010 {} times, expected once", hits.len())));
    };
    let name: String = name.clone();
    transition_to_flag(f, ENDING_PREFIX, &name, "Dm_SageRoom_0010", None)?;
    Ok(())
}

/// Adds the reward item gets to the endings edit in `late` (or to the game's ending flow).
pub fn stage_reward_gives(rf: &mut Romfs, late: &mut Vec<Out>, gives: &[(&str, &[&str], SceneGive)]) -> Res<()> {
    crate::dungeon_grant::merge(rf, late, ENDING_FILE, |raw| rebuild(raw, |b| patch_reward_gives(b, gives)))
}

/// The Great Fairy checks and their fountain flows.
pub const FAIRIES: [(&str, &str); 6] = [
    ("DMT Great Fairy Reward", "Dm_FairyFountain_Din"),
    ("HC Great Fairy Reward", "Dm_FairyFountain_Power"),
    ("DMC Great Fairy Reward", "Dm_FairyFountain_Wisdom"),
    ("ZF Great Fairy Reward", "Dm_FairyFountain_Frore"),
    ("Colossus Great Fairy Reward", "Dm_FairyFountain_Nayru"),
    ("OGC Great Fairy Reward", "Dm_FairyFountain_Courage"),
];
const FAIRY_PREFIX: &str = "OOTRRFairy";
/// The part of each fairy's gift scene that runs her give (her own item get routine).
const FAIRY_GIVE: &str = "C15-01";

fn wait0(f: &mut Flow, next: u16) -> Res<Kind> {
    let (a, x): (u16, u16) = crate::dungeon_credit::action_slot(f, "EventSystemActor", "EventWait")?;
    Ok(Kind::Action { next, actor: a, act: x, params: Some(Cont { items: vec![("IsWaitEnd".to_string(), Val::Bool(1)), ("Frames".to_string(), Val::Int(0))] }) })
}

fn targets_of(k: &Kind) -> Vec<u16> {
    match k {
        Kind::Action { next, .. } | Kind::Sub { next, .. } | Kind::Join { next } => vec![*next],
        Kind::Switch { cases, .. } | Kind::SubSwitch { cases, .. } => cases.iter().map(|c| c.ev).collect(),
        Kind::Fork { join, forks } => std::iter::once(*join).chain(forks.iter().copied()).collect(),
        Kind::Label { .. } => Vec::new(),
    }
}

fn is_sub_to(k: &Kind, entry: &str) -> bool {
    matches!(k, Kind::Sub { entry: e, .. } if e == entry)
}

/// A Great Fairy, once paid. The talk and the payment stay. The music stop, the skip marker and the
/// fairy rising (GreatFairy_Appear) before her gift scene become zero-frame waits, the gift scene
/// becomes the release of the music mute and only her own give (her C15-01 part: the fairy item get
/// routine, built for this scene's Link, with what the check holds), and her leaving (GreatFairy_Disappear: her parting talk, the "all fairies met" message and the
/// heal) becomes only the heal (TriForcePoint:Recover). Din's MP check before the gift stays.
pub fn patch_fairy_give(b: &mut Bf, flow: &str, g: &SceneGive) -> Res<()> {
    let f: &mut Flow = flow_mut(b, flow)?;
    let gifts: Vec<usize> = (0..f.events.len()).filter(|&i| matches!(&f.events[i].kind, Kind::Sub { flow: fl, entry, .. } if fl.is_empty() && entry == flow)).collect();
    let [gift] = gifts.as_slice() else {
        return Err(Error::Msg(format!("story skip: {flow} runs its gift scene {} times, expected once", gifts.len())));
    };
    let gift: usize = *gift;
    let n: usize = f.events.len();
    let mut pred: Vec<Vec<usize>> = vec![Vec::new(); n];
    for i in 0..n {
        for t in targets_of(&f.events[i].kind) {
            if (t as usize) < n {
                pred[t as usize].push(i);
            }
        }
    }
    // The rising: an Appear call that reaches the gift within three steps.
    let near = |from: usize| -> bool {
        let mut todo: Vec<(usize, u8)> = vec![(from, 0)];
        while let Some((i, d)) = todo.pop() {
            if i == gift {
                return true;
            }
            if d < 3 && i < n {
                todo.extend(targets_of(&f.events[i].kind).into_iter().map(|t| (t as usize, d + 1)));
            }
        }
        false
    };
    let appears: Vec<usize> = (0..n).filter(|&i| is_sub_to(&f.events[i].kind, "GreatFairy_Appear") && near(i)).collect();
    if appears.is_empty() {
        return Err(Error::Msg(format!("story skip: {flow} has no fairy rising before its gift, the game files differ")));
    }
    let mut quiet: Vec<usize> = appears.clone();
    for &a in &appears {
        let mut at: usize = a;
        for want in ["EventSystemActor.EventTriggerSkipAcceptStart", "SoundSystemActor.EventTriggerStopInGameBgm"] {
            if let [p] = pred[at].as_slice()
                && action(f, *p).as_deref() == Some(want)
            {
                quiet.push(*p);
                at = *p;
            }
        }
    }
    for i in quiet {
        let next: u16 = next_of(&f.events[i].kind).ok_or(Error::Format("story skip: a fairy event has no next"))?;
        f.events[i].kind = wait0(f, next)?;
    }
    // The leaving: the Disappear call a few steps after the gift.
    let mut i: u16 = next_of(&f.events[gift].kind).ok_or(Error::Format("story skip: the fairy gift has no next"))?;
    let mut leave: Option<usize> = None;
    for _ in 0..4 {
        if i == NONE {
            break;
        }
        if is_sub_to(&f.events[i as usize].kind, "GreatFairy_Disappear") {
            leave = Some(i as usize);
            break;
        }
        i = next_of(&f.events[i as usize].kind).unwrap_or(NONE);
    }
    let leave: usize = leave.ok_or_else(|| Error::Msg(format!("story skip: {flow} has no fairy leaving after its gift, the game files differ")))?;
    let after: u16 = next_of(&f.events[leave].kind).ok_or(Error::Format("story skip: the fairy leaving has no next"))?;
    f.events[leave].kind = Kind::Sub { next: after, params: None, flow: "TriForcePoint".to_string(), entry: "Recover".to_string() };
    // The gift: the release of the music mute from the gift scene, then the item get.
    let release: Option<Kind> = f
        .entries
        .iter()
        .find(|e| e.name == flow)
        .map(|e| e.main)
        .and_then(|m| {
            let mut seen: Vec<bool> = vec![false; n];
            let mut todo: Vec<u16> = vec![m];
            while let Some(x) = todo.pop() {
                let x: usize = x as usize;
                if x >= n || seen[x] {
                    continue;
                }
                seen[x] = true;
                if action(f, x).as_deref() == Some("SoundSystemActor.EventSoundReleaseDucking") {
                    return Some(f.events[x].kind.clone());
                }
                todo.extend(targets_of(&f.events[x].kind));
            }
            None
        });
    let then: u16 = next_of(&f.events[gift].kind).ok_or(Error::Format("story skip: the fairy gift has no next"))?;
    if !f.entries.iter().any(|e| e.name == FAIRY_GIVE) {
        return Err(Error::Msg(format!("story skip: {flow} has no {FAIRY_GIVE} give part, the game files differ")));
    }
    let _ = g;
    let at: u16 = idx(f.events.len())?;
    f.events.push(Event { name: format!("{FAIRY_PREFIX}Give"), kind: Kind::Sub { next: then, params: None, flow: String::new(), entry: FAIRY_GIVE.to_string() } });
    for e in f.entries.iter_mut().filter(|e| e.subs.contains(&idx(gift).unwrap_or(NONE))) {
        e.subs.push(at);
    }
    f.events[gift].kind = match release {
        Some(mut k) => {
            set_next(&mut k, at)?;
            k
        }
        None => wait0(f, at)?,
    };
    // An entry's preload list (subs) names its sub calls. The rising and the gift scene call are
    // no longer sub calls, and a list that names another event makes the game refuse the event.
    let is_sub: Vec<bool> = f.events.iter().map(|e| matches!(e.kind, Kind::Sub { .. } | Kind::SubSwitch { .. })).collect();
    for e in f.entries.iter_mut() {
        e.subs.retain(|&i| is_sub.get(i as usize).copied().unwrap_or(false));
    }
    Ok(())
}

/// Adds a Great Fairy's item get to her flow in `late`, in the mod's work folder, or the game's.
pub fn stage_fairy_give(rf: &mut Romfs, late: &mut Vec<Out>, work: &std::path::Path, flow: &str, g: &SceneGive) -> Res<()> {
    let path: String = format!("Event/EventFlow/{flow}.bfevfl.zs");
    if late.iter().any(|o| o.path == path) {
        return crate::dungeon_grant::merge(rf, late, &path, |raw| rebuild(raw, |b| patch_fairy_give(b, flow, g)));
    }
    let have = |p: &str| -> Option<Vec<u8>> { fs::read(p.split('/').fold(work.to_path_buf(), |a, s| a.join(s))).ok() };
    late.push(edit_zs(rf, &have, &path, |raw| rebuild(raw, |b| patch_fairy_give(b, flow, g)))?);
    Ok(())
}

// ---------------------------------------------------------------- song lessons

/// A song lesson played from another flow: the check, the flow that plays it, every scene of the
/// lesson that flow plays, the scene that hands over the song, and other checks that scene gives.
pub struct SongTrigger {
    pub loc: &'static str,
    pub flow: &'static str,
    pub cut: &'static [&'static str],
    pub give: &'static str,
    pub also: &'static [&'static str],
    /// Calls the scene makes that carry state (flow, entry), run in its place.
    pub calls: &'static [(&'static str, &'static str)],
}

pub const MALON_GIVEN: &str = "HavePlayedEvent.Test_Npc_Hylia_049_Adult_D_N";
pub const GIVEN_MARKS: [(&str, &str); 1] = [("Song from Malon", MALON_GIVEN)];

pub fn given_mark(loc: &str) -> Option<&'static str> {
    GIVEN_MARKS.iter().find(|m| m.0 == loc).map(|m| m.1)
}

pub const SONG_TRIGGERS: [SongTrigger; 11] = [
    SongTrigger { loc: "Song from Impa", flow: "DoorOfTime", cut: &[], give: "Dm_MelodyGet_Zelda", also: &[], calls: &[] },
    SongTrigger { loc: "Song from Malon", flow: "SubEvent_GetEponaSong", cut: &[], give: "Dm_MelodyGet_Epona", also: &[], calls: &[] },
    SongTrigger { loc: "Song from Saria", flow: "SacredForestMeadow", cut: &[], give: "Dm_MelodyGet_Saria", also: &[], calls: &[] },
    // The lesson, then one of the day or night flips (the time change after each stays), then Sun_03.
    SongTrigger {
        loc: "Song from Royal Familys Tomb",
        flow: "GimmickFlow_RoyalFamilyTomb",
        cut: &["Dm_MelodyGet_Sun", "Dm_MelodyGet_Sun_01", "Dm_MelodyGet_Sun_04", "Dm_MelodyGet_Sun_05", "Dm_MelodyGet_Sun_06", "Dm_MelodyGet_Sun_07", "Dm_MelodyGet_Sun_08", "Dm_MelodyGet_Sun_09", "Dm_MelodyGet_Sun_10"],
        give: "Dm_MelodyGet_Sun_03",
        also: &[],
        calls: &[],
    },
    // The Ocarina of Time scene also hands over the Ocarina.
    SongTrigger { loc: "Song from Ocarina of Time", flow: "DoorOfTime", cut: &[], give: "Dm_MelodyGet_Time", also: &["HF Ocarina of Time Item"], calls: &[] },
    SongTrigger { loc: "Song from Windmill", flow: "Npc_Hylia_009_Adult", cut: &[], give: "Dm_MelodyGet_Storm", also: &[], calls: &[("Npc_Hylia_009_Adult", "After_GetStormSong")] },
    // Sheik's songs, played from the sage quest flows. Running the item get inside Sheik's own scene
    // stalled (the scene's Link uses its own animation set), so like the others the scene is
    // dropped from the flow that plays it. Requiem is started by its map and keeps its cut scene.
    SongTrigger { loc: "Sheik in Forest", flow: "SageOfForest", cut: &[], give: "Dm_MelodyGet_Forest", also: &[], calls: &[("AllSage", "MeetSheik_And_Gone")] },
    SongTrigger { loc: "Sheik in Crater", flow: "SageOfFire", cut: &[], give: "Dm_MelodyGet_Fire", also: &[], calls: &[("AllSage", "MeetSheik_And_Gone")] },
    SongTrigger { loc: "Sheik in Ice Cavern", flow: "SageOfWater", cut: &[], give: "Dm_MelodyGet_Water", also: &[], calls: &[("AllSage", "MeetSheik_And_Gone")] },
    SongTrigger { loc: "Sheik in Kakariko", flow: "Dm_SY_BancChangeArrive", cut: &[], give: "Dm_MelodyGet_Shadow", also: &[], calls: &[] },
    SongTrigger { loc: "Sheik at Temple", flow: "AllSage", cut: &[], give: "Dm_MelodyGet_Shine", also: &[], calls: &[] },
];
const SONG_PREFIX: &str = "OOTRRSong";
pub const BOOT_LESSONS: [(&str, &str); 1] = [("Sheik in Kakariko", "Event329")];
const LOADING_WAIT: (&str, &str) = ("UISystemActor", "EventWaitCloseLoadingFade");

fn fade_in(f: &mut Flow) -> Res<Kind> {
    let (a, x): (u16, u16) = crate::dungeon_credit::action_slot(f, FADE_IN.0, FADE_IN.1)?;
    Ok(Kind::Action {
        next: NONE,
        actor: a,
        act: x,
        params: Some(Cont {
            items: vec![
                ("Color".to_string(), Val::Int(0)),
                ("IsNoPlayAnime".to_string(), Val::Bool(0)),
                ("Length".to_string(), Val::Int(1)),
                ("IsWaitEnd".to_string(), Val::Bool(1)),
            ],
        }),
    })
}

fn give_before(f: &mut Flow, ev: &str, prefix: &str, gives: &[SceneGive]) -> Res<()> {
    let c: usize = find(f, ev)?;
    let ci: u16 = idx(c)?;
    let before: Vec<usize> = (0..f.events.len()).filter(|&j| matches!(&f.events[j].kind, Kind::Action { next, .. } | Kind::Sub { next, .. } | Kind::Join { next } if *next == ci)).collect();
    if before.is_empty() {
        return Err(Error::Msg(format!("story skip: nothing leads to {ev} in {}, the game files differ", f.name)));
    }
    let entries: Vec<String> = entries_reaching(f, c);
    let names: Vec<&str> = entries.iter().map(|s| s.as_str()).collect();
    let mut then: u16 = ci;
    for (k, g) in gives.iter().enumerate().rev() {
        then = item_get(f, &format!("{prefix}_{k}"), g, then, &names)?;
    }
    for j in before {
        set_next(&mut f.events[j].kind, then)?;
    }
    Ok(())
}

fn loading_wait(f: &mut Flow) -> Res<Kind> {
    let (a, x): (u16, u16) = crate::dungeon_credit::action_slot(f, LOADING_WAIT.0, LOADING_WAIT.1)?;
    Ok(Kind::Action { next: NONE, actor: a, act: x, params: Some(Cont { items: vec![("IsWaitEnd".to_string(), Val::Bool(1))] }) })
}

/// A write a scene makes of its own (a flag, a counter, a quest step), to run in place of it.
#[derive(Debug, Clone, PartialEq)]
pub struct Write {
    pub actor: String,
    pub act: String,
    pub params: Option<Cont>,
}

/// The writes `scene` makes besides its item: its GameDataActor and ChallengeSystemActor actions.
/// Whether `w` marks a song as owned (IsGet.Item_Melody_*).
fn grants_song(w: &Write) -> bool {
    matches!(w.params.as_ref().and_then(|c| c.get("GameDataName")), Some(Val::Str(s)) if s.starts_with("IsGet.Item_Melody_"))
}

pub fn scene_writes(rf: &mut Romfs, scene: &str) -> Res<Vec<Write>> {
    let raw: Vec<u8> = rf.read_zs(&format!("Event/EventFlow/{scene}.bfevfl.zs"))?;
    let b: Bf = Bf::parse(&raw)?;
    let f: &Flow = b.flows.first().ok_or_else(|| Error::Msg(format!("story skip: {scene} holds no flow")))?;
    let mut out: Vec<Write> = Vec::new();
    for e in &f.events {
        if let Kind::Action { actor, act, params, .. } = &e.kind {
            let Some(a) = f.actors.get(*actor as usize) else { continue };
            if (a.name == "GameDataActor" || a.name == "ChallengeSystemActor") && a.sub.is_empty() {
                let x: String = a.actions.get(*act as usize).cloned().unwrap_or_default();
                out.push(Write { actor: a.name.clone(), act: x, params: params.clone() });
            }
        }
    }
    Ok(out)
}

/// The transitions of `f` that play `scene`.
fn plays(f: &Flow, scene: &str) -> Vec<usize> {
    (0..f.events.len()).filter(|&i| action(f, i).as_deref() == Some("EventSystemActor.EventTransition") && text(&f.events[i], "EventName") == Some(scene)).collect()
}

/// Drops a song lesson played from `t.flow`: each scene of the lesson becomes its played flag, and
/// the scene that hands over the song becomes its played flag, the writes it made and the standard
/// item get of each of `gives` (the extra checks of `also`, then the song). As with Saria's gift,
/// nothing else plays: the trigger no longer stops the music on its way to the scene.
pub fn patch_song_trigger(b: &mut Bf, t: &SongTrigger, writes: &[Write], gives: &[SceneGive]) -> Res<()> {
    let f: &mut Flow = flow_mut(b, t.flow)?;
    for scene in t.cut {
        for i in plays(f, scene) {
            let name: String = f.events[i].name.clone();
            transition_to_flag(f, SONG_PREFIX, &name, scene, None)?;
        }
    }
    let at: Vec<usize> = plays(f, t.give);
    if at.is_empty() {
        return Err(Error::Msg(format!("story skip: {} no longer plays {}, the game files differ", t.flow, t.give)));
    }
    for i in at {
        let next: u16 = next_of(&f.events[i].kind).ok_or(Error::Format("story skip: a song scene has no next"))?;
        let entries: Vec<String> = entries_reaching(f, i);
        let names: Vec<&str> = entries.iter().map(|s| s.as_str()).collect();
        let mut then: u16 = next;
        let boot: Option<&str> = BOOT_LESSONS.iter().find(|b| b.0 == t.loc).map(|b| b.1);
        match boot {
            None => {
                for (k, g) in gives.iter().enumerate().rev() {
                    then = item_get(f, &format!("{SONG_PREFIX}{}_{i}_{k}", t.give), g, then, &names)?;
                }
            }
            Some(ev) => give_before(f, ev, &format!("{SONG_PREFIX}{}_{i}", t.give), gives)?,
        }
        let mut kinds: Vec<Kind> = Vec::new();
        if boot.is_some() {
            kinds.push(loading_wait(f)?);
            kinds.push(fade_in(f)?);
        }
        if let Some(m) = given_mark(t.loc) {
            kinds.push(set_bool(f, m, true)?);
        }
        // The item gets grant whatever the checks hold, so the scene's own song grant (Bolero's
        // scene sets IsGet.Item_Melody_FireBolero outright) would give the vanilla song twice
        // over a shuffled check.
        for w in writes.iter().filter(|w| !grants_song(w)) {
            let (a, x): (u16, u16) = crate::dungeon_credit::action_slot(f, &w.actor, &w.act)?;
            kinds.push(Kind::Action { next: NONE, actor: a, act: x, params: w.params.clone() });
        }
        for (fl, en) in t.calls {
            let local: bool = *fl == t.flow;
            kinds.push(Kind::Sub { next: NONE, params: None, flow: if local { String::new() } else { fl.to_string() }, entry: en.to_string() });
        }
        let n: usize = kinds.len();
        let first: u16 = if n == 0 { then } else { chain(f, &format!("{SONG_PREFIX}{}_{i}_", t.give), kinds, then)? };
        keep_music(f, i);
        // Each entry lists the sub calls it makes (its preload list). A call missing from it is
        // never prepared, and the game crashed on Sheik's MeetSheik_And_Gone call at Bolero.
        let calls: Vec<u16> = (usize::from(first)..usize::from(first) + n).filter(|&x| matches!(f.events[x].kind, Kind::Sub { .. })).map(idx).collect::<Res<Vec<u16>>>()?;
        for e in f.entries.iter_mut().filter(|e| entries.contains(&e.name)) {
            for c in &calls {
                if !e.subs.contains(c) {
                    e.subs.push(*c);
                }
            }
        }
        let name: String = f.events[i].name.clone();
        transition_to_flag(f, SONG_PREFIX, &name, t.give, Some(first))?;
    }
    Ok(())
}

fn keep_music(f: &mut Flow, to: usize) {
    let mut before: Vec<bool> = vec![false; f.events.len()];
    let mut todo: Vec<usize> = vec![to];
    while let Some(x) = todo.pop() {
        for (j, e) in f.events.iter().enumerate() {
            let leads: bool = match &e.kind {
                Kind::Action { next, .. } | Kind::Sub { next, .. } | Kind::Join { next } => *next as usize == x,
                Kind::Switch { cases, .. } | Kind::SubSwitch { cases, .. } => cases.iter().any(|c| c.ev as usize == x),
                Kind::Fork { join, forks } => *join as usize == x || forks.iter().any(|k| *k as usize == x),
                Kind::Label { .. } => false,
            };
            if leads && !before[j] {
                before[j] = true;
                todo.push(j);
            }
        }
    }
    let stops: Vec<(u16, u16)> = (0..f.events.len())
        .filter(|&j| before[j] && action(f, j).as_deref() == Some("SoundSystemActor.EventTriggerStopInGameBgm"))
        .filter_map(|j| next_of(&f.events[j].kind).map(|n| (j as u16, n)))
        .collect();
    for (s, n) in stops {
        for e in f.events.iter_mut() {
            match &mut e.kind {
                Kind::Action { next, .. } | Kind::Sub { next, .. } | Kind::Join { next } if *next == s => *next = n,
                Kind::Switch { cases, .. } | Kind::SubSwitch { cases, .. } => {
                    for c in cases.iter_mut().filter(|c| c.ev == s) {
                        c.ev = n;
                    }
                }
                Kind::Fork { join, forks } => {
                    if *join == s {
                        *join = n;
                    }
                    for k in forks.iter_mut().filter(|k| **k == s) {
                        *k = n;
                    }
                }
                _ => {}
            }
        }
        for en in f.entries.iter_mut().filter(|en| en.main == s) {
            en.main = n;
        }
    }
}

/// Turns a song scene's grant (the get window and pouch add of GetPouchContentByActorName, or a
/// tier check over several of them) into the standard item get with the same item, so Link holds
/// the song up. Runs on Sheik's six scenes after patch_sheik has cut them to the grant.
pub fn patch_song_grant(b: &mut Bf, scene: &str, grant: &str) -> Res<()> {
    let f: &mut Flow = flow_mut(b, scene)?;
    let g: usize = find(f, grant)?;
    let subs: Vec<usize> = match &f.events[g].kind {
        Kind::Switch { cases, .. } => cases.iter().map(|c| c.ev as usize).collect(),
        _ => vec![g],
    };
    let mut done: usize = 0;
    for i in subs {
        if let Kind::Sub { flow, entry, params, .. } = &mut f.events[i].kind
            && flow == "GetPouchContent"
            && (entry == "GetPouchContentByActorName" || entry == "DisplayGetByActorName")
        {
            let window: bool = entry == "DisplayGetByActorName";
            // A small key is held up as the minigame key through the key copy (crate::gift_keys).
            let key: Option<&mut Val> = params.as_mut().and_then(|c| c.items.iter_mut().find(|(k, v)| k == "ActorName" && matches!(v, Val::Str(s) if s == crate::gift_keys::SMALL_KEY_ITEM))).map(|(_, v)| v);
            let small: bool = window && key.is_some();
            if let Some(v) = key.filter(|_| window) {
                *v = Val::Str(crate::gift_keys::SMALL_KEY_SHOWN.to_string());
            }
            *flow = ITEM_GET.0.to_string();
            *entry = match (window, small) {
                (true, true) => crate::item_window::key_name(ITEM_GET.1),
                (true, false) => crate::item_window::name(ITEM_GET.1),
                _ => ITEM_GET.1.to_string(),
            };
            done += 1;
            if !window {
                held(f, i, "ActorName")?;
            }
        }
    }
    if done == 0 {
        return Err(Error::Msg(format!("story skip: {grant} in {scene} is not a song grant, the game files differ")));
    }
    Ok(())
}

/// Stages an edit of `flow` in `late`, in the mod's work folder, or the game's.
pub fn stage_late(rf: &mut Romfs, late: &mut Vec<Out>, work: &std::path::Path, flow: &str, edit: impl FnOnce(&mut Bf) -> Res<()>) -> Res<()> {
    let path: String = format!("Event/EventFlow/{flow}.bfevfl.zs");
    if late.iter().any(|o| o.path == path) {
        return crate::dungeon_grant::merge(rf, late, &path, |raw| rebuild(raw, edit));
    }
    let have = |p: &str| -> Option<Vec<u8>> { fs::read(p.split('/').fold(work.to_path_buf(), |a, s| a.join(s))).ok() };
    late.push(edit_zs(rf, &have, &path, |raw| rebuild(raw, edit))?);
    Ok(())
}

pub const RAURU_LOC: &str = "ToT Reward from Rauru";
const RAURU_GIVE: &str = "OOTRRRauruGive";

/// The first Master Sword draw (patch_draw) gives Rauru's check with a bare pouch add in its
/// chain. That add becomes a fade in and the standard item get with what the check holds (with
/// its tier checks or its dungeon credit), still before the transition to the adult temple.
/// Runs after patch_draw.
pub fn patch_sun_give(b: &mut Bf, g: &SceneGive) -> Res<()> {
    let f: &mut Flow = flow_mut(b, crate::flow_extra::sun::FLOW)?;
    let named = |f: &Flow, n: &str| -> Res<usize> {
        let hits: Vec<usize> = (0..f.events.len()).filter(|&i| f.events[i].name == n).collect();
        match hits.as_slice() {
            [i] => Ok(*i),
            _ => Err(Error::Msg(format!("lh sun: expected one {n} in {}, found {}", f.name, hits.len()))),
        }
    };
    let step: usize = named(f, "Event1")?;
    let done: usize = named(f, "Event2")?;
    let steps = |f: &Flow, i: usize, want: &str| f.events[i].kind.params().and_then(|c| c.get("StepName")) == Some(&Val::Str(want.to_string()));
    if !steps(f, step, "Step03") || !steps(f, done, "Complete") {
        return Err(Error::Format("lh sun: Event1 and Event2 no longer move the Fire Arrows quest to Step03 and Complete"));
    }
    if next_of(&f.events[step].kind) != Some(NONE) {
        return Err(Error::Format("lh sun: Event1 is no longer the end of FireArrow_Appear"));
    }
    set_next(&mut f.events[done].kind, NONE)?;
    let at: u16 = item_get(f, "OOTRRSunGive", g, idx(done)?, &["FireArrow_Appear"])?;
    set_next(&mut f.events[step].kind, at)
}

pub fn stage_sun_give(rf: &mut Romfs, late: &mut Vec<Out>, work: &std::path::Path, g: &SceneGive) -> Res<()> {
    stage_late(rf, late, work, crate::flow_extra::sun::FLOW, |b| patch_sun_give(b, g))
}

/// The winning spin still plays the pot's Heart Piece scene (cut by patch_goron_pot_scene to
/// the roulette and the camera reset, which stop the pot and end its spin; without them the pot
/// kept spinning, ignored bombs and the HUD never came back). After the scene, the placed item
/// is given once, while the drop flag is still unset.
pub fn patch_goron_pot_give(b: &mut Bf, g: &SceneGive) -> Res<()> {
    use crate::bfevfl::Case;
    use crate::flow_extra::goron_pot as pot;
    let f: &mut Flow = flow_mut(b, pot::FLOW)?;
    guard(f, "OOTRRPot")?;
    let scenes: Vec<usize> = (0..f.events.len()).filter(|&i| action(f, i).as_deref() == Some("EventSystemActor.EventTransition") && text(&f.events[i], "EventName") == Some(pot::SCENE)).collect();
    let [scene] = scenes.as_slice() else {
        return Err(Error::Msg(format!("goron pot: expected one {} scene in {}, found {}", pot::SCENE, pot::FLOW, scenes.len())));
    };
    let scene: usize = *scene;
    let dropped: u16 = next_of(&f.events[scene].kind).filter(|n| *n != NONE).ok_or(Error::Format("goron pot: the Heart Piece scene ends the flow"))?;
    if action(f, dropped as usize).as_deref() != Some("GameDataActor.EventTriggerSetGameDataBool") || text(&f.events[dropped as usize], "GameDataName") != Some(pot::DROPPED) {
        return Err(Error::Format("goron pot: the Heart Piece scene is no longer followed by its drop flag"));
    }
    let taken: Kind = set_bool(f, pot::TAKEN, true)?;
    let after: u16 = chain(f, "OOTRRPotTaken", vec![taken], dropped)?;
    let give: u16 = item_get(f, "OOTRRPotGive", g, after, &[pot::ENTRY])?;
    let (a, q): (u16, u16) = crate::start_state::query_slot(f, DATA_ACTOR, "EventQueryIsGameDataBool")?;
    let check: u16 = idx(f.events.len())?;
    // Case 0 is true: the item was already given.
    f.events.push(Event {
        name: "OOTRRPotCheck".to_string(),
        kind: Kind::Switch {
            actor: a,
            query: q,
            cases: vec![Case { val: 0, ev: NONE }, Case { val: 1, ev: give }],
            params: Some(Cont { items: vec![item("GameDataName", Val::Str(pot::DROPPED.to_string())), item("Index", Val::Int(-1))] }),
        },
    });
    set_next(&mut f.events[scene].kind, check)
}

/// Cuts the Heart Piece out of the pot's win scene: the roulette (which stops the pot on its face
/// and resets its prize holder) runs straight into the after-drop camera reset, so the pot never
/// lets the vanilla Heart Piece out of its holder slot.
pub fn patch_goron_pot_scene(b: &mut Bf) -> Res<()> {
    use crate::flow_extra::goron_pot as pot;
    let f: &mut Flow = flow_mut(b, pot::SCENE)?;
    let sub = |f: &Flow, entry: &str| -> Res<usize> {
        let hits: Vec<usize> = (0..f.events.len()).filter(|&i| matches!(&f.events[i].kind, Kind::Sub { flow, entry: e, .. } if flow == pot::ROULETTE_FLOW && e == entry)).collect();
        match hits.as_slice() {
            [i] => Ok(*i),
            _ => Err(Error::Msg(format!("goron pot: expected one {}:{entry} in {}, found {}", pot::ROULETTE_FLOW, pot::SCENE, hits.len()))),
        }
    };
    let roulette: usize = sub(f, "RoulettePot")?;
    let after: usize = sub(f, "AfterDrop")?;
    let to: u16 = idx(after)?;
    if next_of(&f.events[roulette].kind) == Some(to) {
        return Err(Error::Msg(format!("goron pot: {} was already patched", pot::SCENE)));
    }
    if !(0..f.events.len()).any(|i| action(f, i).as_deref() == Some(pot::DROP_PRIZE)) {
        return Err(Error::Msg(format!("goron pot: {} no longer drops the Heart Piece", pot::SCENE)));
    }
    set_next(&mut f.events[roulette].kind, to)?;
    set_next(&mut f.events[after].kind, NONE)
}

pub fn stage_goron_pot_give(rf: &mut Romfs, late: &mut Vec<Out>, work: &std::path::Path, g: &SceneGive) -> Res<()> {
    stage_late(rf, late, work, crate::flow_extra::goron_pot::SCENE, patch_goron_pot_scene)?;
    stage_late(rf, late, work, crate::flow_extra::goron_pot::FLOW, |b| patch_goron_pot_give(b, g))
}

pub fn patch_rauru_give(b: &mut Bf, g: &SceneGive) -> Res<()> {
    let f: &mut Flow = flow_mut(b, DOOR_FLOW)?;
    let adds: Vec<usize> = (0..f.events.len())
        .filter(|&i| f.events[i].name.starts_with(DOOR_PREFIX) && action(f, i).as_deref() == Some("PouchSystemActor.EventTriggerAddOrRemovePouchContent") && text(&f.events[i], "Actor") != Some("Weapon_Sword_Master"))
        .collect();
    let [gift] = adds.as_slice() else {
        return Err(Error::Msg(format!("story skip: the Master Sword draw gives Rauru's check {} times, expected once (run patch_draw first)", adds.len())));
    };
    let gift: usize = *gift;
    let next: u16 = next_of(&f.events[gift].kind).ok_or(Error::Format("story skip: Rauru's gift has no next"))?;
    let entries: Vec<String> = entries_reaching(f, gift);
    let names: Vec<&str> = entries.iter().map(|s| s.as_str()).collect();
    let at: u16 = item_get(f, RAURU_GIVE, g, next, &names)?;
    let (a, x): (u16, u16) = crate::dungeon_credit::action_slot(f, FADE_IN.0, FADE_IN.1)?;
    f.events[gift].kind = Kind::Action {
        next: at,
        actor: a,
        act: x,
        params: Some(Cont {
            items: vec![
                ("Color".to_string(), Val::Int(0)),
                ("IsNoPlayAnime".to_string(), Val::Bool(0)),
                ("Length".to_string(), Val::Int(1)),
                ("IsWaitEnd".to_string(), Val::Bool(1)),
            ],
        }),
    };
    Ok(())
}

/// Adds Rauru's item get to the story skip's DoorOfTime edit in `late` (or to the game's).
pub fn stage_rauru_give(rf: &mut Romfs, late: &mut Vec<Out>, g: &SceneGive) -> Res<()> {
    crate::dungeon_grant::merge(rf, late, DOOR_FILE, |raw| rebuild(raw, |b| patch_rauru_give(b, g)))
}

/// Saria's gift (KokiriForest_ArriveLostWood in Goto_HyruleCastle), the bridge trigger. Nothing of
/// it plays: the trigger's event bool is set, both scenes (Dm_Kokiri_0150 and the gift scene
/// Dm_Kokiri_0155) are marked as played, the field palette flag is set, and the placed item is
/// given. Link stays on the bridge: no BGM stop, camera, fade, warps or area change to Hyrule
/// Field. The item get needs Link in the event, so the event still starts with Link taken (sword
/// drawn, then rolling), joining and unequipping. Runs after patch_bridge.
pub const SARIA_ENTRY: &str = "KokiriForest_ArriveLostWood";
const SARIA_GIVE: &str = "OOTRRSariaGive";
const SARIA_START: (&str, &str) = ("Event56", "Player_Event.EventTriggerPlayerEventStartWithDrawn");
const SARIA_ROLL: (&str, &str) = ("Event135", "Player_Event.EventTriggerPlayerEventStartWithRolling");
const SARIA_JOIN: (&str, &str) = ("Event18", "Player_Event.EventTriggerParticipateEvent");
const SARIA_UNEQUIP: (&str, &str) = ("Event52", "Player_Event.EventPlayerUnequip");
const SARIA_BOOL: (&str, &str) = ("Event2", "EventRequestActor.EventTriggerSetEventBool");
const SARIA_WAIT: (&str, &str) = ("Event17", "EventSystemActor.EventWait");
const SARIA_AFTER: &str = "HyruleField_ChangeENV";

pub fn patch_saria_give(b: &mut Bf, g: &SceneGive) -> Res<()> {
    let f: &mut Flow = flow_mut(b, BRIDGE_FLOW)?;
    if writes(f, LONG_FLAG).is_err() {
        return Err(Error::Format("story skip: run patch_bridge before Saria's item get"));
    }
    let start: usize = find_action(f, SARIA_START)?;
    let roll: usize = find_action(f, SARIA_ROLL)?;
    let join: usize = find_action(f, SARIA_JOIN)?;
    let unequip: usize = find_action(f, SARIA_UNEQUIP)?;
    let used: usize = find_action(f, SARIA_BOOL)?;
    let wait: usize = find_action(f, SARIA_WAIT)?;
    let links: [(usize, usize); 3] = [(start, roll), (unequip, used), (used, wait)];
    if links.iter().any(|&(a, b)| next_of(&f.events[a].kind) != Some(b as u16)) {
        return Err(Error::Format("story skip: Saria's bridge event steps are not linked as they were, the game files differ"));
    }
    let main: u16 = f.entries.iter().find(|e| e.name == SARIA_ENTRY).map(|e| e.main).ok_or(Error::Format("story skip: Goto_HyruleCastle has no KokiriForest_ArriveLostWood"))?;
    if main != idx(start)? {
        return Err(Error::Format("story skip: Saria's bridge event no longer starts by taking Link, the game files differ"));
    }
    let at: u16 = item_get(f, SARIA_GIVE, g, NONE, &[SARIA_ENTRY])?;
    let k1: Kind = set_bool(f, LONG_FLAG, true)?;
    let k2: Kind = set_bool(f, "HavePlayedEvent.Dm_Kokiri_0155", true)?;
    let k3: Kind = set_bool(f, SARIA_AFTER, true)?;
    let first: u16 = chain(f, SARIA_GIVE, vec![k1, k2, k3], at)?;
    set_next(&mut f.events[wait].kind, first)?;
    set_next(&mut f.events[roll].kind, idx(join)?)?;
    set_next(&mut f.events[join].kind, idx(unequip)?)?;
    Ok(())
}

/// Adds Saria's item get to the story skip's bridge edit in `late` (or to the game's).
pub fn stage_saria_give(rf: &mut Romfs, late: &mut Vec<Out>, g: &SceneGive) -> Res<()> {
    crate::dungeon_grant::merge(rf, late, BRIDGE_FILE, |raw| rebuild(raw, |b| patch_saria_give(b, g)))
}

pub fn stage_arrows(rf: &mut Romfs, have: Have) -> Res<Out> {
    edit_zs(rf, have, ARROWS_FILE, |raw| rebuild(raw, patch_light_arrows))
}

// ---------------------------------------------------------------- zelda escape

/// DoorOfTime entry DoorOfTime_Escape (the drawbridge area while DoorOfTime is Step03) fades out,
/// sets the storm, plays the timeline Dm_HrlField_0060 (Zelda and Impa ride out, the Ocarina falls
/// in the moat, Ganondorf), then restores the bridge and weather, warps Link, writes Step04 and fades
/// in with Navi's hint. Only the timeline is cut; the Ocarina pickup (Step04) and Dm_MelodyGet_Time
/// keep both checks.
const ESCAPE_EVENT: &str = "Event13";
const ESCAPE_SCENE: &str = "Dm_HrlField_0060";
pub const ESCAPE_PREFIX: &str = "OOTRREscape";

const ESCAPE_STEP_EVENT: &str = "Event12";
pub const ESCAPE_STEP: &str = "Step05";

pub fn patch_escape(b: &mut Bf) -> Res<()> {
    let f: &mut Flow = flow_mut(b, DOOR_FLOW)?;
    guard(f, ESCAPE_PREFIX)?;
    transition_to_flag(f, ESCAPE_PREFIX, ESCAPE_EVENT, ESCAPE_SCENE, None)?;
    let i: usize = find_action(f, (ESCAPE_STEP_EVENT, "ChallengeSystemActor.EventExecuteProgressChallengeStep"))?;
    if text(&f.events[i], "ChallengeName") != Some("DoorOfTime") || text(&f.events[i], "StepName") != Some("Step04") {
        return Err(Error::Msg(format!("story skip: {ESCAPE_STEP_EVENT} in {} is not the DoorOfTime Step04 request, the game files differ", f.name)));
    }
    set_param(&mut f.events[i].kind, "StepName", Val::Str(ESCAPE_STEP.to_string()))?;
    Ok(())
}

// ---------------------------------------------------------------- owls in flows

/// Owls played from story flows. Each scene only talks (the field owl's flow writes its quest step,
/// the Threads of Time menu and the amiibo item itself, after the scene), so each transition becomes
/// its played flag and the flow goes on.
pub const OWL_PREFIX: &str = "OOTRROwl";
/// Goto_HyruleCastle entry Goto_HyruleCastle_Kaepora_00, the first field owl (closed Kakariko
/// gate): the owl flies in (Dm_HrlField_0005) and talks (0010). Dm_HrlField_0000 (the field
/// opening and its clock) stays.
const FIELD_OWL: [(&str, &str); 2] = [("Event59", "Dm_HrlField_0005"), ("Event12", "Dm_HrlField_0010")];
/// Get_StoneOfWater entry StoneOfWater_TalkKaepora_ZoraRiver: the owl at the river entrance; the
/// Water Stone step after it stays.
pub const RIVER_FILE: &str = "Event/EventFlow/Get_StoneOfWater.bfevfl.zs";
const RIVER_OWL: [(&str, &str); 1] = [("Event81", "Dm_ZoraRiver_0000")];
/// Dm_SY_BancChangeArrive: the Lost Woods owls (from Kokiri Forest, from Goron City, and after
/// Saria's Song). Their next is the same step logic the button skip lands on.
pub const ARRIVE_FILE: &str = "Event/EventFlow/Dm_SY_BancChangeArrive.bfevfl.zs";
const WOODS_OWL: [(&str, &str); 3] = [("Event214", "Dm_LostWood_0000"), ("Event209", "Dm_LostWood_0000_GoronCity"), ("Event211", "Dm_LostWood_0010")];

fn cut_owls(b: &mut Bf, flow: &str, cuts: &[(&str, &str)]) -> Res<()> {
    let f: &mut Flow = flow_mut(b, flow)?;
    guard(f, OWL_PREFIX)?;
    for (name, scene) in cuts {
        transition_to_flag(f, OWL_PREFIX, name, scene, None)?;
    }
    Ok(())
}

pub fn patch_field_owl(b: &mut Bf) -> Res<()> {
    cut_owls(b, BRIDGE_FLOW, &FIELD_OWL)
}

pub fn patch_river_owl(b: &mut Bf) -> Res<()> {
    cut_owls(b, "Get_StoneOfWater", &RIVER_OWL)
}

pub fn patch_woods_owls(b: &mut Bf) -> Res<()> {
    cut_owls(b, "Dm_SY_BancChangeArrive", &WOODS_OWL)
}

// ---------------------------------------------------------------- sheik songs

/// Sheik's six song scenes (flowcharts, Dm_MelodyGet_*). Each places the actors, talks, plays the
/// song three times (Sheik, Link's input, the duet: subs into Dm_SY_Ocarina), then hands over the
/// song with GetPouchContent (the check) and closes the message, then talks and sends Sheik away.
/// The cut jumps from the placement straight to the grant, and after the message closes runs
/// copies of the departure actions (Sheik offstage, fades, camera reset, the met-Sheik counter,
/// the temple flag of the Prelude) into the scene's own last call.
pub const SHEIK_PREFIX: &str = "OOTRRSheik";
pub struct SheikCut {
    pub scene: &'static str,
    /// The placement event whose next becomes the grant (the first copy of `before`, if any).
    pub start: &'static str,
    pub before: &'static [&'static str],
    pub grant: &'static str,
    /// The message close after the grant.
    pub close: &'static str,
    pub tail: &'static [&'static str],
    /// The original last call the tail ends on, or the end.
    pub end: Option<&'static str>,
    /// Events whose next becomes the end (a talk or the music after the placement).
    pub stop: &'static [&'static str],
}

pub const SHEIK_CUTS: [SheikCut; 6] = [
    // Minuet: Sheik is sent offstage in the placement and never seen.
    SheikCut { scene: "Dm_MelodyGet_Forest", start: "Event307", before: &["Event318"], grant: "Event22", close: "Event17", tail: &["Event60", "Event167"], end: Some("Event330"), stop: &[] },
    // Bolero: Sheik and the harp leave, the camera returns, the counter goes up.
    SheikCut { scene: "Dm_MelodyGet_Fire", start: "Event0", before: &[], grant: "Event192", close: "Event204", tail: &["Event38", "Event43", "Event244", "Event152"], end: Some("Event301"), stop: &[] },
    // Serenade: Sheik leaves behind a fade; the Ice Cavern quest writes stay in the caller.
    SheikCut { scene: "Dm_MelodyGet_Water", start: "Event44", before: &[], grant: "Event29", close: "Event26", tail: &["Event18", "Event66", "Event47", "Event69", "Event19", "Event74", "Event282"], end: Some("Event123"), stop: &[] },
    // Nocturne: ends faded out like vanilla (the caller fades in); the burning village stays.
    SheikCut { scene: "Dm_MelodyGet_Shadow", start: "Event119", before: &[], grant: "Event21", close: "Event18", tail: &["Event76", "Event36"], end: None, stop: &["Event45"] },
    // Requiem: Sheik leaves, camera, counter.
    SheikCut { scene: "Dm_MelodyGet_Spilit", start: "Event253", before: &[], grant: "Event17", close: "Event14", tail: &["Event112", "Event153", "Event63"], end: Some("Event282"), stop: &[] },
    // Prelude: behind a fade, Market_Adult_GetLightPrelude removes the temple Sheik.
    SheikCut { scene: "Dm_MelodyGet_Shine", start: "Event253", before: &[], grant: "Event53", close: "Event220", tail: &["Event133", "Event100", "Event46", "Event103", "Event129", "Event116", "Event189"], end: None, stop: &["Event5"] },
];

pub fn sheik_file(scene: &str) -> String {
    format!("Event/EventFlow/{scene}.bfevfl.zs")
}

fn copy_kind(f: &Flow, name: &str) -> Res<Kind> {
    let k: Kind = f.events[find(f, name)?].kind.clone();
    if next_of(&k).is_none() {
        return Err(Error::Msg(format!("story skip: {name} in {} has no single next, the game files differ", f.name)));
    }
    Ok(k)
}

pub fn patch_sheik(b: &mut Bf, c: &SheikCut) -> Res<()> {
    let f: &mut Flow = flow_mut(b, c.scene)?;
    guard(f, SHEIK_PREFIX)?;
    let grant: usize = find(f, c.grant)?;
    // The grant may be wrapped: a dungeon item turns it into the get window and its credit
    // (crate::gift_keys::edit_flow), an upgrade or ammo into tier and owner checks (flow_wrap).
    // Every such form still runs into the message close.
    let close: usize = find_action(f, (c.close, "GameSystemActor.EventCloseMessageDialog"))?;
    let mut seen: Vec<u16> = Vec::new();
    let mut todo: Vec<u16> = vec![idx(grant)?];
    let mut reached: bool = false;
    while let Some(i) = todo.pop() {
        if i == NONE || seen.contains(&i) || seen.len() > 256 {
            continue;
        }
        if i == idx(close)? {
            reached = true;
            continue;
        }
        seen.push(i);
        if let Some(e) = f.events.get(i as usize) {
            match &e.kind {
                Kind::Switch { cases, .. } | Kind::SubSwitch { cases, .. } => todo.extend(cases.iter().map(|c| c.ev)),
                Kind::Fork { join, forks } => {
                    todo.extend(forks.iter().copied());
                    todo.push(*join);
                }
                k => todo.extend(next_of(k)),
            }
        }
    }
    if !reached {
        return Err(Error::Msg(format!("story skip: the song grant of {} is not followed by its message close", c.scene)));
    }
    let start: usize = find(f, c.start)?;
    let end: u16 = match c.end {
        Some(n) => idx(find(f, n)?)?,
        None => NONE,
    };
    let stops: Vec<usize> = c.stop.iter().map(|n| find(f, n)).collect::<Res<_>>()?;
    let before: Vec<Kind> = c.before.iter().map(|n| copy_kind(f, n)).collect::<Res<_>>()?;
    let tail: Vec<Kind> = c.tail.iter().map(|n| copy_kind(f, n)).collect::<Res<_>>()?;
    let to_grant: u16 = if before.is_empty() { idx(grant)? } else { chain(f, SHEIK_PREFIX, before, idx(grant)?)? };
    set_next(&mut f.events[start].kind, to_grant)?;
    let after: u16 = if tail.is_empty() { end } else { chain(f, SHEIK_PREFIX, tail, end)? };
    set_next(&mut f.events[close].kind, after)?;
    for s in stops {
        set_next(&mut f.events[s].kind, NONE)?;
    }
    // Marks the flow as patched even when nothing was appended.
    f.events[start].name = format!("{SHEIK_PREFIX}{start}");
    Ok(())
}

pub const REQUIEM: &str = "Dm_MelodyGet_Spilit";
pub const REQUIEM_LOC: &str = "Sheik at Colossus";
const REQUIEM_ENTRY: &str = "EntryPoint0";
const REQUIEM_GRANT: &str = "Event17";
const REQUIEM_PREFIX: &str = "OOTRRRequiem";
pub const SARIA_ROW: &str = "KokiriForest_ArriveLostWood";
const ROW_FIELDS: [&str; 3] = ["EventBaseSetting", "EventSettingComponentList", "EventSoundSetting"];

pub fn patch_requiem(b: &mut Bf) -> Res<()> {
    {
        let f: &mut Flow = flow_mut(b, REQUIEM)?;
        guard(f, REQUIEM_PREFIX)?;
        if !f.events.iter().any(|e| e.name.starts_with(SHEIK_PREFIX)) {
            return Err(Error::Format("story skip: run patch_sheik before the Requiem skip"));
        }
    }
    patch_song_grant(b, REQUIEM, REQUIEM_GRANT)?;
    let f: &mut Flow = flow_mut(b, REQUIEM)?;
    let grant: u16 = idx(find(f, REQUIEM_GRANT)?)?;
    let k1: Kind = player(f, "EventTriggerPlayerEventStartWithDrawn", None)?;
    let k2: Kind = player(f, "EventTriggerParticipateEvent", None)?;
    let k3: Kind = player(f, "EventPlayerUnequip", Some(Cont { items: vec![item("IsWaitEnd", Val::Bool(1))] }))?;
    let first: u16 = chain(f, REQUIEM_PREFIX, vec![k1, k2, k3], grant)?;
    let e = f.entries.iter_mut().find(|e| e.name == REQUIEM_ENTRY).ok_or(Error::Format("story skip: Requiem has no EntryPoint0"))?;
    e.main = first;
    if !e.subs.contains(&grant) {
        e.subs.push(grant);
    }
    Ok(())
}

fn player(f: &mut Flow, act: &str, params: Option<Cont>) -> Res<Kind> {
    let (a, x): (u16, u16) = crate::dungeon_credit::action_slot(f, "Player_Event", act)?;
    Ok(Kind::Action { next: NONE, actor: a, act: x, params })
}

pub fn plain_row(y: &mut crate::byml::Byml, row: &str) -> Res<()> {
    use crate::byml::Node;
    let id = |n: &Node| -> String { n.get("__RowId").and_then(|v| v.as_str()).unwrap_or("").to_string() };
    let rows: &mut Vec<Node> = y.root.as_arr_mut().ok_or(Error::Format("story skip: the event settings root is not a list"))?;
    let from: Node = rows.iter().find(|r| id(r) == SARIA_ROW).cloned().ok_or(Error::Format("story skip: the event settings have no row for Saria's bridge"))?;
    let to: &mut Node = rows.iter_mut().find(|r| id(r) == row).ok_or_else(|| Error::Msg(format!("story skip: the event settings have no {row} row")))?;
    for k in ROW_FIELDS {
        let v: Node = from.get(k).cloned().ok_or_else(|| Error::Msg(format!("story skip: Saria's bridge row has no {k}")))?;
        to.set(k, v);
    }
    Ok(())
}

pub fn stage_late_file(rf: &mut Romfs, late: &mut Vec<Out>, work: &std::path::Path, path: &str, edit: impl FnOnce(&[u8]) -> Res<Vec<u8>>) -> Res<()> {
    if late.iter().any(|o| o.path == path) {
        return crate::dungeon_grant::merge(rf, late, path, edit);
    }
    let have = |p: &str| -> Option<Vec<u8>> { fs::read(p.split('/').fold(work.to_path_buf(), |a, s| a.join(s))).ok() };
    late.push(edit_zs(rf, &have, path, edit)?);
    Ok(())
}

pub fn stage_requiem(rf: &mut Romfs, late: &mut Vec<Out>, work: &std::path::Path) -> Res<()> {
    stage_late(rf, late, work, REQUIEM, patch_requiem)?;
    stage_late_file(rf, late, work, crate::progressive::SETTING_TABLE, |raw| {
        let mut y: crate::byml::Byml = crate::byml::Byml::parse(raw)?;
        plain_row(&mut y, REQUIEM)?;
        y.write()
    })
}

pub fn stage_sheik(rf: &mut Romfs, have: Have) -> Res<Vec<Out>> {
    let mut outs: Vec<Out> = Vec::new();
    for c in &SHEIK_CUTS {
        // The location patcher has staged these (their grant holds the song check).
        outs.push(edit_zs(rf, have, &sheik_file(c.scene), |raw| rebuild(raw, |b| patch_sheik(b, c)))?);
    }
    Ok(outs)
}

// ---------------------------------------------------------------- burning kakariko

/// Dm_SY_BancChangeArrive entry KakarikoVillage_Arrive (adult Kakariko before the Shadow Temple):
/// preloads KakarikoVillage_Adult, sets KakarikoVillage_Burning_Start, forces the Master Sword out
/// (Event37), mutes the world sounds (Event498), plays the timeline Dm_Kakariko_0030 (the village
/// burns), then changes to KakarikoVillage_Adult booting KakarikoVillage_FlashbackScenes, whose only
/// job is the timeline Dm_Kakariko_0040 (Sheik's flashback) and a change to
/// KakarikoVillage_Adult_Alternative booting KakarikoVillage_MelodyGet_Shadow (the Nocturne scene
/// and the Shadow quest writes). The arrival itself already runs in KakarikoVillage_Adult_Alternative
/// (the exit from Hyrule Field loads it once Sequence_IsAlternative_KakarikoAdult holds), so both
/// banc changes are kept: a change to the banc Link is already in never finishes, and Link froze at
/// the entrance. The cut: the sword request becomes the 0030 flag, the mute is skipped, the 0030
/// transition becomes the 0040 flag, and the flashback's own transition becomes the 0040 flag too.
const BURN_SWORD: (&str, &str) = ("Event37", "Player_Event.EventTriggerEquipmentUserRequestCreateStaticEquipment");
const BURN_LOG: (&str, &str) = ("Event484", "UISystemActor.EventTriggerSetIsNotClearMessageLog");
const BURN_SCENE: (&str, &str) = ("Event70", "Dm_Kakariko_0030");
const BURN_PRELOAD: (&str, &str) = ("Event327", "GameSystemActor.EventTriggerBancRequestBeforehandLoad");
const BURN_CHANGE: (&str, &str) = ("Event329", "GameSystemActor.EventReserveBancChange");
const BURN_FLASHBACK: (&str, &str) = ("Event50", "Dm_Kakariko_0040");
const BURN_FLASHBACK_CHANGE: (&str, &str) = ("Event379", "GameSystemActor.EventReserveBancChange");
const BURN_ARRIVE_BANC: &str = "KakarikoVillage_Adult_Alternative";
const BURN_FLASHBACK_BOOT: &str = "KakarikoVillage_FlashbackScenes";
const BURN_BOOT: &str = "KakarikoVillage_MelodyGet_Shadow";
pub const BURN_PREFIX: &str = "OOTRRBurn";

pub fn patch_burning(b: &mut Bf) -> Res<()> {
    let f: &mut Flow = flow_mut(b, "Dm_SY_BancChangeArrive")?;
    guard(f, BURN_PREFIX)?;
    if !f.entries.iter().any(|e| e.name == BURN_BOOT) {
        return Err(Error::Format("story skip: the Nocturne boot entry is missing, the game files differ"));
    }
    let sword: usize = find_action(f, BURN_SWORD)?;
    let log: usize = find_action(f, BURN_LOG)?;
    find_action(f, BURN_PRELOAD)?;
    let change: usize = find_action(f, BURN_CHANGE)?;
    if text(&f.events[change], "BootEventName") != Some(BURN_FLASHBACK_BOOT) || text(&f.events[change], "NextBanc") == Some(BURN_ARRIVE_BANC) {
        return Err(Error::Format("story skip: the burning village no longer changes to the plain village for the flashback, the game files differ"));
    }
    let onward: usize = find_action(f, BURN_FLASHBACK_CHANGE)?;
    if text(&f.events[onward], "BootEventName") != Some(BURN_BOOT) {
        return Err(Error::Format("story skip: the flashback no longer boots the Nocturne scene, the game files differ"));
    }
    let next: u16 = next_of(&f.events[sword].kind).ok_or(Error::Format("story skip: the sword request has no next"))?;
    let mut k: Kind = set_bool(f, &format!("HavePlayedEvent.{}", BURN_SCENE.1), true)?;
    set_next(&mut k, next)?;
    f.events[sword].kind = k;
    f.events[sword].name = format!("{BURN_PREFIX}{sword}");
    let scene: usize = transition_to_flag(f, BURN_PREFIX, BURN_SCENE.0, BURN_SCENE.1, None)?;
    // The flag of the flashback in place of the burning scene's transition.
    set_param(&mut f.events[scene].kind, "GameDataName", Val::Str("HavePlayedEvent.Dm_Kakariko_0040".to_string()))?;
    set_next(&mut f.events[log].kind, idx(scene)?)?;
    transition_to_flag(f, BURN_PREFIX, BURN_FLASHBACK.0, BURN_FLASHBACK.1, None)?;
    Ok(())
}

// ---------------------------------------------------------------- staging

fn source(rf: &mut Romfs, have: Have, path: &str) -> Res<Vec<u8>> {
    let z: Vec<u8> = match have(path) {
        Some(b) => b,
        None => rf.read(path)?,
    };
    rf.zs.decompress(&z)
}

/// The item the location patcher left in a grant event (the vanilla one when unshuffled).
pub fn placed(rf: &mut Romfs, have: Have, file: &str, index: usize, event: &str, param: &str) -> Res<String> {
    let raw: Vec<u8> = source(rf, have, file)?;
    param_of(&raw, index, event, param)
}

/// Rewrites the bridge and the draw. `have` returns a file already staged by an earlier step (the door fix,
/// the location patcher), so those edits are kept and Rauru's gift follows the shuffled item.
pub fn stage(rf: &mut Romfs, have: Have) -> Res<Vec<Out>> {
    let medal: String = placed(rf, have, RAURU_FILE, RAURU_INDEX, RAURU_EVENT, RAURU_PARAM)?;
    let l: crate::flags::FlagList = crate::flags_build::load(rf)?;
    let got: String = format!("IsGet.{medal}");
    let got: Option<&str> = l.by_hash(crate::flags::hash_name(&got) as u64).is_some().then_some(got.as_str());
    Ok(vec![
        edit_zs(rf, have, BRIDGE_FILE, |raw| {
            rebuild(raw, |b| {
                patch_bridge(b)?;
                patch_field_owl(b)
            })
        })?,
        edit_zs(rf, have, GIFT_FILE, |raw| rebuild(raw, patch_gift))?,
        edit_zs(rf, have, RIVER_FILE, |raw| rebuild(raw, patch_river_owl))?,
        edit_zs(rf, have, ARRIVE_FILE, |raw| {
            rebuild(raw, |b| {
                patch_woods_owls(b)?;
                patch_burning(b)
            })
        })?,
        edit_zs(rf, have, DOOR_FILE, |raw| {
            rebuild(raw, |b| {
                patch_draw(b, &medal, got)?;
                patch_escape(b)
            })
        })?,
        stage_arrows(rf, have)?,
    ]
    .into_iter()
    .chain(stage_sheik(rf, have)?)
    .collect())
}

pub fn worked(rf: &mut Romfs, pk: &crate::modpack::Pack) -> Res<Vec<Out>> {
    let root: std::path::PathBuf = pk.work.join("romfs");
    let have = |p: &str| -> Option<Vec<u8>> { fs::read(p.split('/').fold(root.clone(), |a, s| a.join(s))).ok() };
    stage(rf, &have)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dump() -> Option<std::path::PathBuf> {
        let p: std::path::PathBuf = ootrr_game::test_romfs::path();
        if p.is_dir() {
            Some(p)
        } else {
            eprintln!("skipped: no romfs at {}, set OOTRR_ROMFS", p.display());
            None
        }
    }

    fn none(_p: &str) -> Option<Vec<u8>> {
        None
    }

    #[test]
    fn every_entry_of_a_patched_song_trigger_preloads_the_calls_it_now_makes() {
        let Some(root) = dump() else { return };
        let mut rf: Romfs = Romfs::open(&root).unwrap();
        for t in &SONG_TRIGGERS {
            let writes: Vec<Write> = scene_writes(&mut rf, t.give).unwrap();
            let gives: Vec<SceneGive> = std::iter::once(t.loc).chain(t.also.iter().copied()).map(|_| SceneGive { actor: "Item_Bottle".to_string(), family: None, key: None, line: None }).collect();
            let mut b: Bf = Bf::parse(&rf.read_zs(&format!("Event/EventFlow/{}.bfevfl.zs", t.flow)).unwrap()).unwrap();
            patch_song_trigger(&mut b, t, &writes, &gives).unwrap();
            let f: &Flow = b.flows.iter().find(|x| x.name == t.flow).unwrap();
            for e in f.events.iter().filter(|e| e.name.starts_with(SONG_PREFIX)) {
                assert!(!text(e, "GameDataName").is_some_and(|n| n.starts_with("IsGet.Item_Melody_")), "{}: {} grants a song outright", t.flow, e.name);
            }
            for i in (0..f.events.len()).filter(|&i| f.events[i].name.starts_with(SONG_PREFIX) && matches!(f.events[i].kind, Kind::Sub { .. })) {
                let ins: Vec<String> = entries_reaching(f, i);
                assert!(!ins.is_empty(), "{}: {} is reached by no entry", t.flow, f.events[i].name);
                for e in f.entries.iter().filter(|e| ins.contains(&e.name)) {
                    assert!(e.subs.contains(&(i as u16)), "{}: entry {} does not preload {}", t.flow, e.name, f.events[i].name);
                }
            }
        }
    }

    #[test]
    fn every_story_call_of_a_dropped_song_scene_still_runs() {
        let Some(root) = dump() else { return };
        let mut rf: Romfs = Romfs::open(&root).unwrap();
        let helpers: [&str; 7] = ["", "Dm_SY_Ocarina", "GetPouchContent", "NaviEventCommon", "NaviChat", "Common", "Common_Camera_Pln"];
        let talk_only: [(&str, &str); 3] = [("SageOfWater", "Dm_MelodyGet_WaterAfter"), ("AllSage", "MelodyOfShine"), ("DoorOfTime", "Dm_MelodyGet_Time_After")];
        for t in &SONG_TRIGGERS {
            let b: Bf = Bf::parse(&rf.read_zs(&format!("Event/EventFlow/{}.bfevfl.zs", t.give)).unwrap()).unwrap();
            for e in b.flows.iter().flat_map(|f| f.events.iter()) {
                let Kind::Sub { flow, entry, .. } = &e.kind else { continue };
                if helpers.contains(&flow.as_str()) || talk_only.contains(&(flow.as_str(), entry.as_str())) {
                    continue;
                }
                assert!(t.calls.contains(&(flow.as_str(), entry.as_str())), "{}: the scene calls {flow}:{entry}, which no longer runs", t.loc);
            }
        }
    }

    #[test]
    fn the_goron_pot_win_plays_the_scene_then_gives_the_placed_item_once() {
        use crate::bfevfl::Case;
        use crate::flow_extra::goron_pot as pot;
        let Some(root) = dump() else { return };
        let mut rf: Romfs = Romfs::open(&root).unwrap();
        let mut b: Bf = Bf::parse(&rf.read_zs(&format!("Event/EventFlow/{}.bfevfl.zs", pot::FLOW)).unwrap()).unwrap();
        let g: SceneGive = SceneGive { actor: "Item_Bow".to_string(), family: None, key: None, line: None };
        patch_goron_pot_give(&mut b, &g).unwrap();
        let b: Bf = Bf::parse(&b.write().unwrap()).unwrap();
        let f: &Flow = b.flows.iter().find(|x| x.name == pot::FLOW).unwrap();
        // The scene still plays (it stops the pot and ends its spin), and the check follows it.
        let scene: &Event = f.events.iter().find(|e| text(e, "EventName") == Some(pot::SCENE)).unwrap();
        let check: u16 = next_of(&scene.kind).unwrap();
        let switch: &Event = &f.events[check as usize];
        assert!(matches!(switch.kind, Kind::Switch { .. }) && text(switch, "GameDataName") == Some(pot::DROPPED), "{switch:?}");
        let Kind::Switch { cases, .. } = &switch.kind else { unreachable!() };
        assert!(cases.contains(&Case { val: 0, ev: NONE }));
        let give: u16 = cases.iter().find(|c| c.val == 1).unwrap().ev;
        let Kind::Sub { next, params, flow, .. } = &f.events[give as usize].kind else { panic!("{:?}", f.events[give as usize]) };
        assert_eq!(flow, ITEM_GET.0);
        assert_eq!(params.as_ref().and_then(|c| c.get("ActorName")), Some(&Val::Str("Item_Bow".to_string())));
        let mut at: u16 = *next;
        let mut set: Vec<&str> = Vec::new();
        while at != NONE {
            let e: &Event = &f.events[at as usize];
            if let Some(n) = text(e, "GameDataName") {
                set.push(n);
            }
            at = next_of(&e.kind).unwrap_or(NONE);
        }
        assert!(set.ends_with(&[pot::TAKEN, pot::DROPPED]), "{set:?}");
        let entry = f.entries.iter().find(|e| e.name == pot::ENTRY).unwrap();
        assert!(entry.subs.contains(&give));
        let mut again: Bf = b.clone();
        assert!(patch_goron_pot_give(&mut again, &g).is_err());
    }

    #[test]
    fn the_goron_pot_scene_keeps_the_roulette_but_never_drops_the_heart_piece() {
        use crate::flow_extra::goron_pot as pot;
        let Some(root) = dump() else { return };
        let mut rf: Romfs = Romfs::open(&root).unwrap();
        let mut b: Bf = Bf::parse(&rf.read_zs(&format!("Event/EventFlow/{}.bfevfl.zs", pot::SCENE)).unwrap()).unwrap();
        patch_goron_pot_scene(&mut b).unwrap();
        let b: Bf = Bf::parse(&b.write().unwrap()).unwrap();
        let f: &Flow = b.flows.iter().find(|x| x.name == pot::SCENE).unwrap();
        let mut seen: Vec<bool> = vec![false; f.events.len()];
        let mut todo: Vec<u16> = f.entries.iter().map(|e| e.main).collect();
        while let Some(at) = todo.pop() {
            if at == NONE || seen[at as usize] {
                continue;
            }
            seen[at as usize] = true;
            match &f.events[at as usize].kind {
                Kind::Switch { cases, .. } => todo.extend(cases.iter().map(|c| c.ev)),
                Kind::Fork { forks, join } => {
                    todo.extend(forks.iter().copied());
                    todo.push(*join);
                }
                k => todo.extend(next_of(k)),
            }
        }
        let runs = |want: &dyn Fn(usize) -> bool| (0..f.events.len()).any(|i| seen[i] && want(i));
        let sub = |i: usize, entry: &str| matches!(&f.events[i].kind, Kind::Sub { flow, entry: e, .. } if flow == pot::ROULETTE_FLOW && e == entry);
        assert!(runs(&|i| sub(i, "RoulettePot")));
        assert!(runs(&|i| sub(i, "AfterDrop")));
        assert!(!runs(&|i| action(f, i).as_deref() == Some(pot::DROP_PRIZE)));
        assert!(!runs(&|i| action(f, i).is_some_and(|a| a.starts_with("Obj_Heart_Piece."))));
        let mut again: Bf = b.clone();
        assert!(patch_goron_pot_scene(&mut again).is_err());
    }

    #[test]
    fn the_skipped_escape_moves_the_quest_past_the_moat_step() {
        let Some(root) = dump() else { return };
        let mut rf: Romfs = Romfs::open(&root).unwrap();
        let mut b: Bf = Bf::parse(&rf.read_zs(DOOR_FILE).unwrap()).unwrap();
        patch_escape(&mut b).unwrap();
        let f: &Flow = b.flows.iter().find(|x| x.name == DOOR_FLOW).unwrap();
        let steps: Vec<&str> = f.events.iter().filter(|e| text(e, "ChallengeName") == Some("DoorOfTime")).filter_map(|e| text(e, "StepName")).collect();
        assert!(!steps.contains(&"Step04"), "{steps:?}");
        let e: &Event = f.events.iter().find(|e| e.name == ESCAPE_STEP_EVENT).unwrap();
        assert_eq!(text(e, "StepName"), Some(ESCAPE_STEP));
        assert!(patch_escape(&mut b).is_err());
    }

    #[test]
    fn the_nocturne_is_given_before_the_burned_village_loads() {
        let Some(root) = dump() else { return };
        let mut rf: Romfs = Romfs::open(&root).unwrap();
        let t: &SongTrigger = SONG_TRIGGERS.iter().find(|t| t.loc == "Sheik in Kakariko").unwrap();
        let writes: Vec<Write> = scene_writes(&mut rf, t.give).unwrap();
        let gives: Vec<SceneGive> = vec![SceneGive { actor: "Item_Bottle".to_string(), family: None, key: None, line: None }];
        let mut b: Bf = Bf::parse(&rf.read_zs(&format!("Event/EventFlow/{}.bfevfl.zs", t.flow)).unwrap()).unwrap();
        patch_burning(&mut b).unwrap();
        patch_song_trigger(&mut b, t, &writes, &gives).unwrap();
        let f: &Flow = b.flows.iter().find(|x| x.name == t.flow).unwrap();
        let boot = f.entries.iter().find(|e| e.name == "KakarikoVillage_MelodyGet_Shadow").unwrap();
        let w: Vec<String> = walk(f, boot.main, 16);
        assert!(!w.iter().any(|x| x.contains("GetInEvent")), "{w:?}");
        let wait: usize = w.iter().position(|x| x.starts_with("UISystemActor.EventWaitCloseLoadingFade")).expect(&format!("{w:?}"));
        let fade: usize = w.iter().position(|x| x.starts_with("UISystemActor.EventEventFadeIn")).expect(&format!("{w:?}"));
        assert!(wait < fade, "{w:?}");
        let unequip: usize = f.events.iter().position(|e| e.name == "Event496").unwrap();
        let a: Vec<String> = walk(f, next_of(&f.events[unequip].kind).unwrap(), 12);
        let get: usize = a.iter().position(|x| x.contains("GetInEvent")).expect(&format!("{a:?}"));
        let change: usize = a.iter().position(|x| x.starts_with("GameSystemActor.EventReserveBancChange")).expect(&format!("{a:?}"));
        assert!(get < change, "{a:?}");
        assert!(entries_reaching(f, f.events.iter().position(|e| e.name == "Event329").unwrap()).contains(&"KakarikoVillage_Arrive".to_string()));
        Bf::parse(&b.write().unwrap()).unwrap();
    }

    /// Follows single nexts from `start`, collecting each event's summary.
    fn walk(f: &Flow, start: u16, limit: usize) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let mut i: u16 = start;
        while i != NONE && out.len() < limit {
            let e: &Event = &f.events[i as usize];
            let what: String = match &e.kind {
                Kind::Action { .. } => {
                    let a: String = action(f, i as usize).unwrap_or_default();
                    let p: String = ["EventName", "GameDataName", "Actor", "ChallengeName", "StepName"].iter().filter_map(|k| text(e, k)).collect::<Vec<_>>().join(",");
                    format!("{a}({p})")
                }
                Kind::Sub { flow, entry, .. } => format!("sub {flow}:{entry}"),
                k => format!("{k:?}"),
            };
            out.push(what);
            i = next_of(&e.kind).unwrap_or(NONE);
        }
        out
    }

    /// Like walk, but a fork continues at its join.
    fn walk_joins(f: &Flow, start: u16, limit: usize) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let mut i: u16 = start;
        while i != NONE && out.len() < limit {
            let w: Vec<String> = walk(f, i, 1);
            out.extend(w);
            i = match &f.events[i as usize].kind {
                Kind::Fork { join, .. } => *join,
                k => next_of(k).unwrap_or(NONE),
            };
        }
        out
    }

    #[test]
    fn the_new_game_state_covers_the_deku_tree_and_the_child_temple() {
        let o: Ops = deku();
        assert_eq!(o.steps[0], ("Meet_DekuTree".to_string(), "Complete".to_string()));
        assert_eq!(o.steps.last().unwrap(), &("Get_StoneOfForest".to_string(), "MeetBoss".to_string()));
        assert!(o.flags.iter().any(|(n, _)| n == "KokiriForest_Meet_DekuTree_OpenDekuTreeMouth"));
        assert!(o.items.is_empty() && o.time.is_none());
        let s: Ops = start();
        assert_eq!(s.flags.len(), TEMPLE_FLAGS.len() + OWL_FLAGS.len());
        assert!(s.flags.iter().all(|(_, v)| *v));
        assert!(!s.flags.iter().any(|(n, _)| n.contains("Kokiri")));
        assert!(s.steps.is_empty() && s.items.is_empty());
    }

    #[test]
    fn the_real_draw_hands_over_to_the_later_draw() {
        let Some(root) = dump() else {
            return;
        };
        let mut rf: Romfs = Romfs::open(&root).unwrap();
        assert_eq!(placed(&mut rf, &none, RAURU_FILE, RAURU_INDEX, RAURU_EVENT, RAURU_PARAM).unwrap(), "Item_Medal_Light");
        let mut b: Bf = Bf::parse(&rf.read_zs(DOOR_FILE).unwrap()).unwrap();
        crate::door_fix::patch_flow(&mut b).unwrap();
        patch_draw(&mut b, "Item_Test", Some("IsGet.Item_Test")).unwrap();
        assert!(patch_draw(&mut b, "Item_Test", None).is_err());
        let f: &Flow = b.flows.iter().find(|f| f.name == DOOR_FLOW).unwrap();
        let Kind::Switch { cases, .. } = &f.events[find(f, DRAW_SWITCH).unwrap()].kind else { panic!() };
        let later: u16 = cases.iter().find(|c| c.val == 0).unwrap().ev;
        assert_eq!(walk(f, later, 2), vec!["EventSystemActor.EventTransition(Market_Child_Field_ChildToAdult)".to_string()]);
        let w: Vec<String> = walk(f, cases.iter().find(|c| c.val == 1).unwrap().ev, 30);
        assert!(w[1].contains("HavePlayedEvent.Dm_TimeTemple_0040"));
        assert_eq!(w[9], "PouchSystemActor.EventTriggerAddOrRemovePouchContent(Weapon_Sword_Master)");
        assert_eq!(w[10], "PouchSystemActor.EventTriggerAddOrRemovePouchContent(Item_Test)");
        assert_eq!(w[11], "GameDataActor.EventTriggerSetGameDataBool(IsGet.Item_Test)");
        assert_eq!(w[12], "ChallengeSystemActor.EventExecuteProgressChallengeStep(SageOfLight,Complete)");
        assert_eq!(w[14], "ChallengeSystemActor.EventExecuteProgressChallengeStep(SageOfForest,Step01)");
        assert!(w[15].starts_with("Switch"), "{}", w[15]);
        assert_eq!(w.len(), 16);
        // The guard: Step06 or later asks for Complete then transitions, else just transitions;
        // the transition is the last event either way.
        let mut i: u16 = cases.iter().find(|c| c.val == 1).unwrap().ev;
        for _ in 0..15 {
            i = next_of(&f.events[i as usize].kind).unwrap();
        }
        let Kind::Switch { cases: g, .. } = &f.events[i as usize].kind else { panic!() };
        let yes: Vec<String> = walk(f, g.iter().find(|c| c.val == 0).unwrap().ev, 5);
        assert_eq!(yes, vec!["ChallengeSystemActor.EventExecuteProgressChallengeStep(DoorOfTime,Complete)".to_string(), "EventSystemActor.EventTransition(Market_Child_Field_ChildToAdult)".to_string()]);
        let no: Vec<String> = walk(f, g.iter().find(|c| c.val != 0).unwrap().ev, 5);
        assert_eq!(no, vec!["EventSystemActor.EventTransition(Market_Child_Field_ChildToAdult)".to_string()]);
        assert!(!w.iter().chain(&yes).any(|s| s.starts_with("sub")));
        assert!(f.events.iter().filter(|e| e.name.starts_with(DOOR_PREFIX)).all(|e| match &e.kind {
            Kind::Action { params, .. } => params.as_ref().and_then(|c| c.get("IsEquipDIrect")) != Some(&Val::Bool(TRUE)),
            _ => true,
        }));
        Bf::parse(&b.write().unwrap()).unwrap();
        assert_eq!(stage(&mut rf, &none).unwrap().len(), 6 + SHEIK_CUTS.len());
    }

    #[test]
    fn the_real_bridge_goes_straight_to_the_gift_scene() {
        let Some(root) = dump() else {
            return;
        };
        let mut rf: Romfs = Romfs::open(&root).unwrap();
        let mut b: Bf = Bf::parse(&rf.read_zs(BRIDGE_FILE).unwrap()).unwrap();
        crate::start_state::keep_field_clock(&mut b).unwrap();
        patch_bridge(&mut b).unwrap();
        assert!(patch_bridge(&mut b).is_err());
        let f: &Flow = b.flows.iter().find(|f| f.name == BRIDGE_FLOW).unwrap();
        let w: Vec<String> = walk(f, find(f, DUCKING).unwrap() as u16, 6);
        assert!(w[0].starts_with("SoundSystemActor.EventSoundRequestDucking"));
        assert_eq!(w[1], "GameDataActor.EventTriggerSetGameDataBool(HavePlayedEvent.Dm_Kokiri_0150)");
        assert!(w[2].starts_with("SoundSystemActor.EventSoundReleaseDucking"));
        assert_eq!(w[3], "EventSystemActor.EventTransition(Dm_Kokiri_0155)");
        assert_eq!(w[4], "GameDataActor.EventTriggerSetGameDataBool(HyruleField_ChangeENV)");
        assert!(w[5].contains("EventReserveBancChange"));
        Bf::parse(&b.write().unwrap()).unwrap();
    }
    #[test]
    fn the_real_dungeon_endings_keep_only_the_reward_scenes() {
        let Some(root) = dump() else {
            return;
        };
        let mut rf: Romfs = Romfs::open(&root).unwrap();
        let mut b: Bf = Bf::parse(&rf.read_zs(ENDING_FILE).unwrap()).unwrap();
        let n: usize = patch_endings(&mut b).unwrap();
        assert!(n >= CUT_SCENES.len());
        assert!(patch_endings(&mut b).is_err());
        let f: &Flow = b.flows.iter().find(|f| f.name == ENDING_FLOW).unwrap();
        let played: Vec<String> = (0..f.events.len()).filter(|&i| action(f, i).as_deref() == Some("EventSystemActor.EventTransition")).filter_map(|i| text(&f.events[i], "EventName").map(str::to_string)).collect();
        for s in CUT_SCENES {
            assert!(!played.iter().any(|p| p == s), "{s} still plays");
        }
        for s in ["Dm_Kokiri_0130", "Dm_DeathMtTrail_0025", "Dm_ZoraFountain_0030", "Dm_Awake_Forest_Get", "Dm_Awake_Fire_Get", "Dm_Awake_Water_Get", "Dm_Awake_Shadow_Get", "Dm_Awake_Spirit_Get"] {
            assert!(played.iter().any(|p| p == s), "{s} no longer plays");
        }
        // The Deku Tree chain: flags in place of the four scenes, the emerald scene kept.
        let e = f.entries.iter().find(|e| e.name == "Warp_Kokiri").unwrap();
        let w: Vec<String> = walk(f, e.main, 12);
        let at: usize = w.iter().position(|s| s == "EventSystemActor.EventTransition(Dm_Kokiri_0130)").unwrap();
        let flag = |n: &str| format!("GameDataActor.EventTriggerSetGameDataBool(HavePlayedEvent.{n})");
        assert_eq!(w[at - 3..at].to_vec(), vec![flag("Dm_Kokiri_0080"), flag("Dm_Kokiri_0095"), flag("Dm_Kokiri_0090")]);
        assert_eq!(w[at + 1], flag("Dm_Kokiri_0140"));
        Bf::parse(&b.write().unwrap()).unwrap();
    }

    #[test]
    fn the_real_dungeon_rewards_are_given_in_the_boss_room_and_the_sage_stops_are_skipped() {
        let Some(root) = dump() else {
            return;
        };
        let mut rf: Romfs = Romfs::open(&root).unwrap();
        let mut b: Bf = Bf::parse(&rf.read_zs(ENDING_FILE).unwrap()).unwrap();
        patch_endings(&mut b).unwrap();
        patch_fire_ending(&mut b).unwrap();
        let gives: Vec<(&str, &[&str], SceneGive)> =
            REWARD_SCENES.iter().map(|(_, scene, extra)| (*scene, *extra, SceneGive { actor: "Item_Bottle".to_string(), family: None, key: None, line: None })).collect();
        patch_reward_gives(&mut b, &gives).unwrap();
        let f: &Flow = b.flows.iter().find(|f| f.name == ENDING_FLOW).unwrap();
        for (_, boss) in BOSS_WARPS {
            let e = f.entries.iter().find(|e| e.name == boss).unwrap();
            assert!(e.subs.iter().any(|&s| matches!(&f.events[s as usize].kind, Kind::Sub { flow, .. } if flow == ITEM_GET.0)), "{boss}");
        }
        let bosses: Vec<&str> = BOSS_WARPS.iter().map(|(_, e)| *e).collect();
        for (boot, _) in SAGE_STOPS {
            let left: usize = (0..f.events.len())
                .filter(|&i| action(f, i).as_deref() == Some(BANC_CHANGE) && text(&f.events[i], "BootEventName") == Some(boot))
                .filter(|&i| entries_reaching(f, i).iter().any(|n| bosses.contains(&n.as_str())))
                .count();
            assert_eq!(left, 0, "a blue warp still changes to {boot}");
        }
        Bf::parse(&b.write().unwrap()).unwrap();
    }

    #[test]
    fn the_real_medallion_scenes_end_without_the_white_screen_lines() {
        let Some(root) = dump() else {
            return;
        };
        let mut rf: Romfs = Romfs::open(&root).unwrap();
        for s in MEDAL_SCENES {
            let mut b: Bf = Bf::parse(&rf.read_zs(&medal_file(s)).unwrap()).unwrap();
            let n: usize = cut_white_screen(&mut b).unwrap();
            assert!(n >= 3, "{s}: {n}");
            assert!(cut_white_screen(&mut b).is_err(), "{s}: a second run finds no line");
            let f: &Flow = &b.flows[0];
            let g: usize = (0..f.events.len()).find(|&i| matches!(&f.events[i].kind, Kind::Sub { entry, .. } if entry == "GetPouchContentByActorName")).unwrap();
            let w: Vec<String> = walk(f, g as u16, 10);
            assert!(w[0].starts_with("sub GetPouchContent:GetPouchContentByActorName"), "{s}");
            assert!(w.iter().any(|x| x.starts_with("UISystemActor.EventEventFadeOut")), "{s}");
            assert!(!w.iter().any(|x| x.ends_with(")") && x.contains(".EventTalk(")), "{s}: {w:?}");
            let tail: Vec<&String> = w.iter().skip_while(|x| !x.starts_with("Player_Event.EventTriggerPlayerRestoreBootsChange")).skip(1).collect();
            if s.contains("Forest") || s.contains("Fire") {
                assert_eq!(tail, vec!["GameDataActor.EventTriggerSetGameDataBool(AllSage_AwakeSage_Fire_Or_Forest)"], "{s}");
            } else {
                assert!(tail.is_empty(), "{s}: {tail:?}");
            }
        }
    }

    #[test]
    fn saria_song_to_darunia_goes_straight_to_the_bracelet_scene_and_it_starts_at_the_gift() {
        let Some(root) = dump() else {
            return;
        };
        let mut rf: Romfs = Romfs::open(&root).unwrap();
        let mut b: Bf = Bf::parse(&rf.read_zs(STONE_FILE).unwrap()).unwrap();
        patch_darunia_song(&mut b).unwrap();
        let f: &Flow = flow_mut(&mut b, STONE_FLOW).unwrap();
        let ask: usize = f.events.iter().position(|e| e.name == "Event35").unwrap();
        let Kind::Switch { cases, .. } = &f.events[ask].kind else { panic!() };
        let to: usize = cases.iter().find(|c| c.val == 0).unwrap().ev as usize;
        assert_eq!(f.events[to].name, "Event39");
        let Kind::Switch { cases: pick, .. } = &f.events[to].kind else { panic!() };
        assert!(pick.iter().any(|c| text(&f.events[c.ev as usize], "EventName") == Some(BRACELET)));
        // The wrong melody case still goes to Darunia's other scene.
        let other: usize = cases.iter().find(|c| c.val == 1).unwrap().ev as usize;
        assert_eq!(text(&f.events[other], "EventName"), Some("Dm_Goron_0028"));
        let mut s: Bf = Bf::parse(&rf.read_zs(BRACELET_FILE).unwrap()).unwrap();
        cut_bracelet_scene(&mut s).unwrap();
        assert!(cut_bracelet_scene(&mut s).is_err());
        let f: &Flow = &s.flows[0];
        let e = f.entries.iter().find(|e| e.name == BRACELET_MAIN).unwrap();
        let Kind::Fork { forks, join } = &f.events[e.main as usize].kind else { panic!("main is no fork") };
        assert!(forks.iter().any(|x| matches!(&f.events[*x as usize].kind, Kind::Sub { entry, .. } if entry == BRACELET_GRANT_PART)));
        // After the gift the scene only runs :C11, which keeps the quest step.
        let Kind::Join { next } = &f.events[*join as usize].kind else { panic!() };
        let Kind::Fork { forks: last, .. } = &f.events[*next as usize].kind else { panic!() };
        assert!(last.iter().any(|x| matches!(&f.events[*x as usize].kind, Kind::Sub { entry, .. } if entry == "C11")));
        assert!(f.events.iter().all(|ev| !matches!(&ev.kind, Kind::Fork { forks, .. } if forks.iter().any(|x| text(&f.events[*x as usize], "ASCommand") == Some(EQUIP_POSE)))));
        assert!(f.events.iter().any(|ev| text(ev, "StepName") == Some("DodongoCavernOpne")));
    }

    #[test]
    fn the_real_darunia_scene_ends_after_the_ruby() {
        let Some(root) = dump() else {
            return;
        };
        let mut rf: Romfs = Romfs::open(&root).unwrap();
        let mut b: Bf = Bf::parse(&rf.read_zs(DARUNIA_FILE).unwrap()).unwrap();
        cut_darunia_talk(&mut b).unwrap();
        assert!(cut_darunia_talk(&mut b).is_err());
        let f: &Flow = &b.flows[0];
        let e = f.entries.iter().find(|e| e.name == "EntryPoint0").unwrap();
        let w: Vec<String> = walk(f, e.main, 20);
        assert_eq!(w, vec!["sub :Ready", "sub :C01", "sub :C01-05", "sub :C01-07", "SoundSystemActor.EventSoundChangeListenerPreset()", "UISystemActor.EventEventFadeOut()", "EventSystemActor.EventWait()"]);
        // The grant is still the end of the part that is kept.
        let g = f.entries.iter().find(|e| e.name == DARUNIA_GRANT_PART).unwrap();
        let mut i: u16 = g.main;
        let mut granted = false;
        for _ in 0..40 {
            if i == NONE {
                break;
            }
            granted |= matches!(&f.events[i as usize].kind, Kind::Sub { entry, .. } if entry == "GetPouchContentByActorName");
            i = match &f.events[i as usize].kind {
                Kind::Fork { join, .. } => *join,
                k => next_of(k).unwrap_or(NONE),
            };
        }
        assert!(granted);
        Bf::parse(&b.write().unwrap()).unwrap();
    }

    #[test]
    fn the_real_light_arrows_scene_keeps_only_the_grant() {
        let Some(root) = dump() else {
            return;
        };
        let mut rf: Romfs = Romfs::open(&root).unwrap();
        let mut b: Bf = Bf::parse(&rf.read_zs(ARROWS_FILE).unwrap()).unwrap();
        patch_light_arrows(&mut b).unwrap();
        assert!(patch_light_arrows(&mut b).is_err());
        let f: &Flow = b.flows.iter().find(|f| f.name == ARROWS_FLOW).unwrap();
        let flag = |n: &str| format!("GameDataActor.EventTriggerSetGameDataBool(HavePlayedEvent.{n})");
        for entry in ["Market_Adult_Field_GetArrowLight", "GetArrowLight_OcarinaBackTimeTemple"] {
            let e = f.entries.iter().find(|e| e.name == entry).unwrap();
            let w: Vec<String> = walk_joins(f, e.main, 60);
            assert!(!w.iter().any(|x| x.contains("Equipment")), "{entry}: {w:?}");
            let at: usize = w.iter().position(|x| x == "EventSystemActor.EventTransition(Dm_TimeTemple_0130)").unwrap();
            assert_eq!(w[at + 1], flag("Dm_TimeTemple_0135"));
            assert_eq!(w.iter().filter(|x| x.starts_with("EventSystemActor.EventTransition")).count(), 1, "{entry}: {w:?}");
            for s in ["Dm_TimeTemple_0090", "Dm_TimeTemple_0095", "Dm_TimeTemple_0110", "Dm_TimeTemple_0115", "Dm_TimeTemple_0125"] {
                assert!(w[..at].contains(&flag(s)), "{entry}: {s}");
            }
            let tail: Vec<&String> = w[at + 2..].iter().filter(|x| !x.starts_with("Fork") && !x.starts_with("Join")).collect();
            assert_eq!(tail, vec!["Player_Event.EventPlayerUnequip()", "UISystemActor.EventEventFadeIn()", "ChallengeSystemActor.EventExecuteProgressChallengeStep(AllSage,Complete)", "ChallengeSystemActor.EventExecuteProgressChallengeStep(Defeat_Ganon,Step01)"], "{entry}");
        }
        Bf::parse(&b.write().unwrap()).unwrap();
    }

    #[test]
    fn the_real_zelda_escape_moves_the_step_past_the_moat_and_keeps_the_fade() {
        let Some(root) = dump() else {
            return;
        };
        let mut rf: Romfs = Romfs::open(&root).unwrap();
        let mut b: Bf = Bf::parse(&rf.read_zs(DOOR_FILE).unwrap()).unwrap();
        patch_escape(&mut b).unwrap();
        assert!(patch_escape(&mut b).is_err());
        let f: &Flow = b.flows.iter().find(|f| f.name == DOOR_FLOW).unwrap();
        let e = f.entries.iter().find(|e| e.name == "DoorOfTime_Escape").unwrap();
        let w: Vec<String> = walk_joins(f, e.main, 80);
        assert!(w.contains(&"GameDataActor.EventTriggerSetGameDataBool(HavePlayedEvent.Dm_HrlField_0060)".to_string()), "{w:?}");
        assert!(!w.iter().any(|x| x.starts_with("EventSystemActor.EventTransition")), "{w:?}");
        assert!(w.contains(&"ChallengeSystemActor.EventExecuteProgressChallengeStep(DoorOfTime,Step05)".to_string()), "{w:?}");
        Bf::parse(&b.write().unwrap()).unwrap();
    }

    #[test]
    fn the_real_owls_become_flags() {
        let Some(root) = dump() else {
            return;
        };
        let mut rf: Romfs = Romfs::open(&root).unwrap();
        for (file, patch) in [(BRIDGE_FILE, patch_field_owl as fn(&mut Bf) -> Res<()>), (RIVER_FILE, patch_river_owl), (ARRIVE_FILE, patch_woods_owls)] {
            let mut b: Bf = Bf::parse(&rf.read_zs(file).unwrap()).unwrap();
            patch(&mut b).unwrap();
            assert!(patch(&mut b).is_err(), "{file}");
            let f: &Flow = &b.flows[0];
            for s in ["Dm_HrlField_0005", "Dm_HrlField_0010", "Dm_ZoraRiver_0000", "Dm_LostWood_0000", "Dm_LostWood_0000_GoronCity", "Dm_LostWood_0010"] {
                assert!(!(0..f.events.len()).any(|i| action(f, i).as_deref() == Some("EventSystemActor.EventTransition") && text(&f.events[i], "EventName") == Some(s)), "{file}: {s}");
            }
            Bf::parse(&b.write().unwrap()).unwrap();
        }
        // The field owl's flow still writes its quest step, menu and item after the cut.
        let mut b: Bf = Bf::parse(&rf.read_zs(BRIDGE_FILE).unwrap()).unwrap();
        patch_field_owl(&mut b).unwrap();
        let f: &Flow = &b.flows[0];
        let e = f.entries.iter().find(|e| e.name == "Goto_HyruleCastle_Kaepora_00").unwrap();
        let w: Vec<String> = walk_joins(f, e.main, 40);
        for want in ["EventSystemActor.EventTransition(Dm_HrlField_0000)", "GameDataActor.EventTriggerSetGameDataBool(UI_IsFinishChallengeMenuOpenEvent)", "PouchSystemActor.EventTriggerAddOrRemovePouchContent(Pouch_Amiibo)", "ChallengeSystemActor.EventExecuteProgressChallengeStep(Goto_HyruleCastle,Step02)"] {
            assert!(w.iter().any(|x| x == want), "{want}: {w:?}");
        }
    }

    #[test]
    fn the_real_sheik_scenes_go_from_the_placement_to_the_song() {
        let Some(root) = dump() else {
            return;
        };
        let mut rf: Romfs = Romfs::open(&root).unwrap();
        for c in &SHEIK_CUTS {
            let mut b: Bf = Bf::parse(&rf.read_zs(&sheik_file(c.scene)).unwrap()).unwrap();
            patch_sheik(&mut b, c).unwrap();
            assert!(patch_sheik(&mut b, c).is_err(), "{}", c.scene);
            let f: &Flow = &b.flows[0];
            let e = f.entries.iter().find(|e| e.name == "EntryPoint0").unwrap();
            let w: Vec<String> = walk_joins(f, e.main, 80);
            let at: usize = w.iter().position(|x| x == "sub GetPouchContent:GetPouchContentByActorName").unwrap_or_else(|| panic!("{}: {w:?}", c.scene));
            // No ocarina lesson, no talk on the way in or out.
            assert!(!w.iter().any(|x| x.contains("Dm_SY_Ocarina") || x.contains(".EventTalk(")), "{}: {w:?}", c.scene);
            assert_eq!(w[at + 1], "GameSystemActor.EventCloseMessageDialog()", "{}", c.scene);
            let rest: &[String] = &w[at + 2..];
            if !["Dm_MelodyGet_Shadow", "Dm_MelodyGet_Forest", "Dm_MelodyGet_Shine"].contains(&c.scene) {
                assert!(rest.iter().any(|x| x.contains("Sheik_Adult.EventGoOffstageForUser")), "{}: {rest:?}", c.scene);
            }
            if c.scene == "Dm_MelodyGet_Shine" {
                assert!(rest.iter().any(|x| x.ends_with("(Market_Adult_GetLightPrelude)")), "{rest:?}");
            }
            if c.scene != "Dm_MelodyGet_Shadow" {
                assert!(rest.iter().any(|x| x.ends_with("(AllSage_MeetSheikCount)")), "{}: {rest:?}", c.scene);
            }
            Bf::parse(&b.write().unwrap()).unwrap();
        }
    }

    #[test]
    fn the_real_burning_village_skips_both_scenes_and_keeps_both_loads() {
        let Some(root) = dump() else {
            return;
        };
        let mut rf: Romfs = Romfs::open(&root).unwrap();
        let mut b: Bf = Bf::parse(&rf.read_zs(ARRIVE_FILE).unwrap()).unwrap();
        patch_woods_owls(&mut b).unwrap();
        patch_burning(&mut b).unwrap();
        assert!(patch_burning(&mut b).is_err());
        let f: &Flow = &b.flows[0];
        // From the preload, past the horse check at the start of KakarikoVillage_Arrive.
        let w: Vec<String> = walk_joins(f, find(f, BURN_PRELOAD.0).unwrap() as u16, 40);
        assert!(!w.iter().any(|x| x.contains("RequestCreateStaticEquipment") || x.starts_with("EventSystemActor.EventTransition") || x.contains("Ducking")), "{w:?}");
        for want in ["GameDataActor.EventTriggerSetGameDataBool(KakarikoVillage_Burning_Start)", "GameDataActor.EventTriggerSetGameDataBool(HavePlayedEvent.Dm_Kakariko_0030)", "GameDataActor.EventTriggerSetGameDataBool(HavePlayedEvent.Dm_Kakariko_0040)"] {
            assert!(w.iter().any(|x| x == want), "{want}: {w:?}");
        }
        assert_eq!(w.last().unwrap(), "Player_Event.EventWaitCreationFinishedEquipment()");
        let change: usize = find(f, BURN_CHANGE.0).unwrap();
        assert_eq!(text(&f.events[change], "NextBanc"), Some("KakarikoVillage_Adult"));
        assert_ne!(text(&f.events[change], "NextBanc"), Some(BURN_ARRIVE_BANC), "the arrival runs in the burned village, a change to it never finishes");
        assert_eq!(text(&f.events[change], "BootEventName"), Some(BURN_FLASHBACK_BOOT));
        let flashback: usize = f.entries.iter().find(|e| e.name == BURN_FLASHBACK_BOOT).unwrap().main as usize;
        let w2: Vec<String> = walk_joins(f, flashback as u16, 40);
        assert!(!w2.iter().any(|x| x.starts_with("EventSystemActor.EventTransition")), "{w2:?}");
        assert!(w2.iter().any(|x| x == "GameDataActor.EventTriggerSetGameDataBool(HavePlayedEvent.Dm_Kakariko_0040)"), "{w2:?}");
        let onward: usize = find(f, BURN_FLASHBACK_CHANGE.0).unwrap();
        assert_eq!((text(&f.events[onward], "NextBanc"), text(&f.events[onward], "BootEventName")), (Some(BURN_ARRIVE_BANC), Some(BURN_BOOT)));
        Bf::parse(&b.write().unwrap()).unwrap();
    }

    #[test]
    fn the_real_gift_scene_is_a_plain_item_get() {
        let Some(root) = dump() else {
            return;
        };
        let mut rf: Romfs = Romfs::open(&root).unwrap();
        let mut b: Bf = Bf::parse(&rf.read_zs(GIFT_FILE).unwrap()).unwrap();
        patch_gift(&mut b).unwrap();
        assert!(patch_gift(&mut b).is_err());
        let f: &Flow = b.flows.iter().find(|f| f.name == GIFT_FLOW).unwrap();
        let e = f.entries.iter().find(|e| e.name == GIFT_ENTRY).unwrap();
        let w: Vec<String> = walk(f, e.main, 5);
        assert_eq!(w, vec!["sub GetPouchContent:DisplayGetByActorName".to_string(), "PouchSystemActor.EventTriggerAddOrRemovePouchContent(Item_Ocarina_Fairy)".to_string()]);
        assert_eq!(e.subs, vec![find(f, GIFT_SHOW).unwrap() as u16]);
        // With a progressive item placed, both events are tier checks first; every window
        // copy then leads to the grant check, and every grant copy ends the flow.
        let mut b: Bf = Bf::parse(&rf.read_zs(GIFT_FILE).unwrap()).unwrap();
        let fl: &mut Flow = flow_mut(&mut b, GIFT_FLOW).unwrap();
        let (g, s): (usize, usize) = (find(fl, GIFT_GRANT).unwrap(), find(fl, GIFT_SHOW).unwrap());
        crate::progressive::wrap_grant(fl, g, GIFT_GRANT, "Actor", "Ocarina").unwrap();
        crate::progressive::wrap_grant(fl, s, GIFT_SHOW, "ActorName", "Ocarina").unwrap();
        patch_gift(&mut b).unwrap();
        let f: &Flow = b.flows.iter().find(|f| f.name == GIFT_FLOW).unwrap();
        assert_eq!(f.entries.iter().find(|e| e.name == GIFT_ENTRY).unwrap().main as usize, s);
        assert_eq!(ends(f, s).unwrap() as usize, g);
        assert_eq!(ends(f, g).unwrap(), NONE);
        let out: Vec<u8> = b.write().unwrap();
        assert_eq!(Bf::parse(&out).unwrap(), b);
    }

    #[test]
    fn the_real_fire_temple_goes_straight_to_the_chamber() {
        let Some(root) = dump() else {
            return;
        };
        let mut rf: Romfs = Romfs::open(&root).unwrap();
        let mut b: Bf = Bf::parse(&rf.read_zs(ENDING_FILE).unwrap()).unwrap();
        patch_endings(&mut b).unwrap();
        patch_fire_ending(&mut b).unwrap();
        assert!(patch_fire_ending(&mut b).is_err());
        let f: &Flow = b.flows.iter().find(|f| f.name == ENDING_FLOW).unwrap();
        let w: Vec<String> = walk(f, find(f, FIRE_EXIT.0).unwrap() as u16, 10);
        assert_eq!(w, vec!["EventSystemActor.EventTransition(Dm_FireTemple_0030)", "GameDataActor.EventTriggerSetGameDataBool(HavePlayedEvent.Dm_Kakariko_0025)", "GameSystemActor.EventTriggerBancRequestBeforehandLoad()", "GameSystemActor.EventReserveBancChange()"]);
        let last: &Event = f.events.iter().find(|e| e.name.starts_with(FIRE_PREFIX) && text(e, "NextBanc").is_some()).unwrap();
        assert_eq!(text(last, "NextBanc"), Some("SageRoom"));
        assert_eq!(text(last, "BootEventName"), Some("FireTemple_SageOfFire_SageRoom"));
        Bf::parse(&b.write().unwrap()).unwrap();
    }
}
