//! Turns off the game's tutorials (action guides). Ported from Ontico's REALAZK/ootrr pull
//! request 11. Always on, like the cutscene skips.
//!
//! Every tutorial an event flow shows goes through Event/EventFlow/ActionGuide: about 25
//! callers (the item get and chest flows, signs, Kokiri children, Navi's hints, shops, the
//! fishing pond, the Great Fairy, the first Hyrule Field arrival). Its five entries only open
//! the guide screens, request tutorials, clear their own "guide still to show" flags, switch
//! the pause menu tab and, for the ocarina, start the Dm_SY_Ocarina playing lesson. Nothing
//! in it matters to the game, so each entry now runs a one frame wait and ends.
//!
//! This also keeps the ocarina lesson from starting when Link has no ocarina, which a shuffled
//! Saria's gift allows.
//!
//! The game code also opens some guides itself, without an event flow (for example "press RT
//! to use Zelda's Letter" when an assignable item is first picked up). Each of the 53 guides
//! has a save flag, ActionGuide_IsEnd.<guide>, that marks it as done, so all of them are set
//! at new game ([`start_flags`]).

use crate::bfevfl::{Bf, Cont, Event, Flow, Kind, Val};
use crate::error::{Error, Res};
use crate::romfs::Romfs;
use crate::staging::{self, Out};

pub const FILE: &str = "Event/EventFlow/ActionGuide.bfevfl.zs";
pub const FLOW: &str = "ActionGuide";
pub const PREFIX: &str = "OOTRRNoGuide";


pub const RING_PREFIX: &str = "OOTRRRingCall";
pub const ENTRIES: [&str; 5] = ["OpenActionGuideSceen", "OpenActionGuideSceenForce", "ActionGuide", "ActionGuideForShop", "OpenActionGuideSceenForOcarina"];
const NONE: u16 = 0xFFFF;
const TRUE: u32 = 0x8000_0001;

/// Every action guide, as named by its ActionGuide_IsEnd flag (and in LayoutMsg/ActionGuide_00).
pub const GUIDES: [&str; 53] = [
    "ActivationAmiibo", "AgonyStone", "AlarmCucco", "Amiibo", "Backflip", "Bomb", "Bombchu", "Boomerang", "BootShortcut", "Bottle", "Bow", "DekuNut", "DekuStick", "DinFire",
    "EquipmentMenu", "EquipmentMenuForBoot", "EquipmentMenuForHeadArmor", "FaroreWind", "FireArrow", "FishingRod", "GoronGauntlet", "Hammer", "HeadShortcut", "Hookshot",
    "HorizontalSlash", "IceArrow", "ItemMenu", "ItemShortcut", "JumpSlash", "LightArrow", "LockOn", "LockOnShield", "LockOnSideMove", "Longshot", "MagicBean", "Navi",
    "NayruLove", "Ocarina", "OcarinaSongEstimate", "RollAttack", "RotarySlash", "Save", "ScrubBusinessSeed", "SideJump", "SinkingLure", "SlingShot", "SystemMinus",
    "Thrust", "TimeTrail", "VerticalSlash", "Weapon_Shield_Hylian", "Weapon_Shield_Mirror", "ZeldaLetter",
];

/// The game's "has picked up a dungeon map (compass) once" marks and their first get scene. The
/// "-" map and the minimap stayed locked on every file until a map was picked up in a dungeon:
/// the per-dungeon DungeonInfo.*.IsGetMap flags that Start With writes (and the map guide being
/// done) do not unlock them, the first pickup does. They are set at new game whatever the map
/// setting, so the map works from the start and its tutorial counts as seen.
pub const MAP_SEEN: [&str; 4] = ["IsGet.Item_Dungeon_Map", "HavePlayedGetEvent.Item_Dungeon_Map", "IsGet.Item_Dungeon_Compass", "HavePlayedGetEvent.Item_Dungeon_Compass"];

pub const SHIELD_HINT: [&str; 2] = ["HavePlayedEvent.NaviHint_Shield_Hylian", "HavePlayedEvent.NaviHint_Shield_Hylian_BuyShop"];

/// New game flags that mark every guide as already done, the map as unlocked and the shield hint
/// as played.
pub fn start_flags() -> Vec<(String, bool)> {
    GUIDES
        .iter()
        .map(|g| (format!("ActionGuide_IsEnd.{g}"), true))
        .chain(MAP_SEEN.iter().chain(SHIELD_HINT.iter()).map(|n| (n.to_string(), true)))
        .collect()
}




pub fn patch(b: &mut Bf, rings: bool) -> Res<usize> {
    let f: &mut Flow = b.flows.iter_mut().find(|f| f.name == FLOW).ok_or(Error::Format("no tutorials: no ActionGuide flow"))?;
    if f.events.iter().any(|e| e.name.starts_with(PREFIX) || e.name.starts_with(RING_PREFIX)) {
        return Err(Error::Format("no tutorials: ActionGuide was already patched"));
    }
    let names: Vec<&str> = f.entries.iter().map(|e| e.name.as_str()).collect();
    if names != ENTRIES {
        return Err(Error::Msg(format!("no tutorials: ActionGuide's entries are {names:?}, the game files differ")));
    }
    let (a, x): (u16, u16) = crate::start_state::slot(f, "EventSystemActor", "EventWait")?;
    let end: usize = f.events.len();
    let calls: usize = usize::from(rings) * 2;
    if end + calls >= NONE as usize {
        return Err(Error::Format("no tutorials: too many events in the flow"));
    }
    let wait = |next, name| Event {
        name,
        kind: Kind::Action {
            next,
            actor: a,
            act: x,
            params: Some(Cont { items: vec![("IsWaitEnd".to_string(), Val::Bool(TRUE)), ("Frames".to_string(), Val::Int(1))] }),
        },
    };
    f.events.push(wait(NONE, format!("{PREFIX}{end}")));
    let (item_get, shop): (u16, u16) = if rings {
        let at: u16 = (end + 1) as u16;
        f.events.push(wait(end as u16, format!("{RING_PREFIX}ItemGet")));
        f.events.push(wait(end as u16, format!("{RING_PREFIX}ForShop")));
        (at, at + 1)
    } else {
        (end as u16, end as u16)
    };
    for e in &mut f.entries {
        e.main = match (rings, e.name.as_str()) {
            (true, "ActionGuide") => item_get,
            (true, "ActionGuideForShop") => shop,
            _ => end as u16,
        };
        e.subs.clear();
    }
    Ok(f.entries.len())
}

pub fn stage(rf: &mut Romfs, rings: bool) -> Res<Out> {
    staging::rewrite(rf, FILE, |raw| {
        let mut b: Bf = Bf::parse(raw)?;
        patch(&mut b, rings)?;
        let out: Vec<u8> = b.write()?;
        if Bf::parse(&out)? != b {
            return Err(Error::Format("no tutorials: the patched flow does not parse back"));
        }
        Ok(out)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bfevfl::Actor;

    #[test]
    fn a_new_game_marks_the_map_as_picked_up_once_and_every_guide_as_done() {
        let f: Vec<(String, bool)> = start_flags();
        assert_eq!(f.len(), GUIDES.len() + MAP_SEEN.len() + SHIELD_HINT.len());
        for n in MAP_SEEN.iter().copied().chain(["ActionGuide_IsEnd.SystemMinus"]) {
            assert!(f.contains(&(n.to_string(), true)), "{n}");
        }
        let rows: String = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/flags.json")).unwrap();
        for n in MAP_SEEN.iter().chain(SHIELD_HINT.iter()) {
            let row: &str = rows.split('\n').find(|l| l.contains(&format!("\"name\":\"{n}\""))).unwrap_or_else(|| panic!("{n} is not a game flag"));
            assert!(row.contains("\"type\":\"Bool\"") && row.contains("\"save\":0"), "{n}: {row}");
        }
    }

    #[test]
    fn every_entry_of_the_real_action_guide_ends_at_once() {
        let p: std::path::PathBuf = ootrr_game::test_romfs::path();
        if !p.is_dir() {
            eprintln!("skipped: no romfs at {}, set OOTRR_ROMFS", p.display());
            return;
        }
        let mut rf: Romfs = Romfs::open(&p).unwrap();
        let mut b: Bf = Bf::parse(&rf.read_zs(FILE).unwrap()).unwrap();
        assert_eq!(patch(&mut b, false).unwrap(), ENTRIES.len());
        assert!(patch(&mut b, false).is_err());
        let f: &Flow = &b.flows[0];
        let end: u16 = f.entries[0].main;
        assert!(f.entries.iter().all(|e| e.main == end && e.subs.is_empty()));
        let Kind::Action { next, .. } = &f.events[end as usize].kind else { panic!() };
        assert_eq!(*next, NONE);
        assert_eq!(Bf::parse(&b.write().unwrap()).unwrap(), b);
    }

    fn synthetic() -> Bf {
        let waiter: Actor = Actor {
            name: "EventSystemActor".to_string(),
            sub: String::new(),
            arg: String::new(),
            actions: vec!["EventWait".to_string()],
            queries: Vec::new(),
            params: None,
            ep: 0,
            cut: 0,
        };
        let entries: Vec<crate::bfevfl::Entry> = ENTRIES
            .iter()
            .map(|e| crate::bfevfl::Entry {
                name: e.to_string(),
                subs: Vec::new(),
                vars: Vec::new(),
                main: NONE,
                pad: 0,
            })
            .collect();
        Bf {
            ver: [0; 4],
            bom: 0,
            shift: 0,
            pad: 0,
            name: "test".to_string(),
            flows: vec![Flow {
                name: FLOW.to_string(),
                actors: vec![waiter],
                events: Vec::new(),
                entries,
            }],
            tls: Vec::new(),
        }
    }

    fn next(f: &Flow, at: u16) -> u16 {
        match f.events[at as usize].kind {
            Kind::Action { next, .. } => next,
            _ => panic!("{} is not an action", f.events[at as usize].name),
        }
    }

    fn patched(rings: bool) -> Bf {
        let mut b: Bf = synthetic();
        assert_eq!(patch(&mut b, rings).unwrap(), ENTRIES.len());
        b
    }

    #[test]
    fn without_rings_every_entry_still_shares_one_wait() {
        let b: Bf = patched(false);
        let f: &Flow = &b.flows[0];
        assert_eq!(f.events.len(), 1);
        assert!(f.events[0].name.starts_with(PREFIX));
        let end: u16 = f.entries[0].main;
        assert!(f.entries.iter().all(|e| e.main == end && e.subs.is_empty()));
        assert_eq!(next(f, end), NONE);
        assert_eq!(Bf::parse(&b.write().unwrap()).unwrap(), b);
    }

    #[test]
    fn rings_keep_separate_item_get_and_shop_call_sites() {
        let mut b: Bf = patched(true);
        let f: &Flow = &b.flows[0];
        let end: u16 = f.entries.iter().find(|e| e.name == "OpenActionGuideSceen").unwrap().main;
        let item: u16 = f.entries.iter().find(|e| e.name == "ActionGuide").unwrap().main;
        let shop: u16 = f.entries.iter().find(|e| e.name == "ActionGuideForShop").unwrap().main;
        assert_ne!(item, shop);
        assert_ne!(item, end);
        assert_ne!(shop, end);
        assert!(f.events[item as usize].name.starts_with(RING_PREFIX), "{}", f.events[item as usize].name);
        assert!(f.events[shop as usize].name.starts_with(RING_PREFIX), "{}", f.events[shop as usize].name);
        assert_eq!(next(f, item), end);
        assert_eq!(next(f, shop), end);
        assert_eq!(next(f, end), NONE);
        assert!(f.entries.iter().all(|e| e.subs.is_empty()));
        for e in &f.entries {
            if !matches!(e.name.as_str(), "ActionGuide" | "ActionGuideForShop") {
                assert_eq!(e.main, end, "{} stayed off its own call site", e.name);
            }
        }
        assert_eq!(Bf::parse(&b.write().unwrap()).unwrap(), b);
        assert!(patch(&mut b, true).is_err());
    }
}
