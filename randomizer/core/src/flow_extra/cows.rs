use std::collections::BTreeMap as BM;

use crate::error::{Error as E, Res};
use crate::flow_extra::data::{Extra, Spec};
use crate::flow_extra::op::{How, Op};
use crate::flow_locations::{Change, Loc};

pub const SETTING: &str = "shuffle_cows";
pub const SETTING_OTHER: &str = "shuffle_cows_other";
pub const ROLE: &str = "once";
pub const PARAM: &str = "ActorName";
pub const VANILLA: &str = "Milk";

pub const LOCS: [&str; 2] = ["KF Links House Cow", "GV Cow"];
pub const OTHER: [&str; 6] = ["GF Cow", "DMT Cow Grotto Cow", "Kak Impas House Cow", "HF Cow Grotto Cow", "LLR Stables Left Cow", "LLR Stables Right Cow"];

pub const NOTE: &str = "Link's House cow in Kokiri Forest and the Gerudo Valley cow each give one extra shuffled item the first time their milk scene starts, with or without a bottle. The milk itself stays vanilla and stays repeatable. The other cows are not shuffled.";

pub const NOTE_OTHER: &str = "The other six cows (Gerudo Fortress, the Death Mountain Trail and Hyrule Field grottos, Impa's house in Kakariko and the two Lon Lon Ranch stable cows) each give one extra shuffled item the first time their milk scene starts. The five ordinary ones become actor classes of their own. Untested in the game.";

pub fn is(name: &str) -> bool {
    LOCS.contains(&name) || OTHER.contains(&name)
}

pub fn is_other(name: &str) -> bool {
    OTHER.contains(&name)
}

fn spec(name: &str) -> Res<Spec> {
    Extra::load()?.specs.into_iter().find(|s| s.name == name).ok_or_else(|| E::Msg(format!("{name} has no once grant edit in data/flow_extra.json")))
}

pub fn flag_of(name: &str) -> Option<String> {
    let s: Spec = spec(name).ok()?;
    s.ops.iter().find_map(|o| match &o.how {
        How::Once(w) => Some(w.flag.clone()),
        _ => None,
    })
}

pub fn changes(l: &Loc, item: &str) -> Res<Vec<Change>> {
    let s: Spec = spec(&l.name)?;
    let mut out: Vec<Change> = Vec::new();
    for o in &s.ops {
        if matches!(o.how, How::Once(_)) {
            out.push(Change {
                location: l.name.clone(),
                file: o.file.clone(),
                index: o.index,
                event: o.event.clone(),
                param: PARAM.to_string(),
                role: ROLE.to_string(),
                old: l.item.clone().unwrap_or_default(),
                new: item.to_string(),
            });
        }
    }
    if out.is_empty() {
        return Err(E::Msg(format!("{} has no once grant edit in data/flow_extra.json", l.name)));
    }
    Ok(out)
}

pub fn plain(c: &Change) -> bool {
    c.role != ROLE
}

pub struct ProgressiveGrant {
    pub op: Op,
    pub family: &'static str,
    pub forms: Vec<String>,
}

pub fn progressive_grants(d: &crate::slice::data::Data, placed: &BM<String, &'static str>) -> Res<Vec<ProgressiveGrant>> {
    let mut out: Vec<ProgressiveGrant> = Vec::new();
    for (loc, family) in placed.iter().filter(|(loc, _)| is(loc)) {
        let s: Spec = spec(loc)?;
        let [op] = s.ops.as_slice() else {
            return Err(E::Msg(format!("{loc} must have exactly one once grant edit")));
        };
        if !matches!(op.how, How::Once(_)) {
            return Err(E::Msg(format!("{loc} has no once grant edit")));
        }
        let tier: &str = crate::progressive::tiers(family)?[0];
        out.push(ProgressiveGrant { op: op.bind(tier), family, forms: crate::ocarina_hold::forms(d, loc, family) });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeSet as BS, HashMap as HM};
    use std::path::{Path as P, PathBuf as PB};

    use super::*;
    use crate::bfevfl::{Bf, Flow};
    use crate::flow_extra::sim::{beyond_always, broken, called, edited, granted, run, sim, Scene};
    use crate::flow_locations::Data as FlowData;
    use crate::flow_sim::run::{End, Fin, Sim};
    use crate::romfs::Romfs;
    use crate::settings::{Settings, Value};
    use crate::slice::data::Data;
    use crate::slice::generate::{generate_data, GenOpts, Generated};
    use crate::slice::generate::allowed;
    use crate::slice::support::{plan as plan_of, Plan};
    use crate::world::World;

    const FLAGS: &str = include_str!("../../../../data/flags.json");
    const NEW: &str = "Item_Longshot";
    const BOTTLE: &str = "Dm_SY_GetInEvent:GetInEventInBottle";
    const CHILD: &str = "Sequence_IsChildhood";

    fn root() -> Option<PB> {
        let p: PB = ootrr_game::test_romfs::path();
        if p.join("Event/EventFlow").is_dir() {
            Some(p)
        } else {
            eprintln!("skipped: no romfs at {}, set OOTRR_ROMFS", p.display());
            None
        }
    }

    fn scene_in(flow: &str, entry: &str, set: &[(&str, &str)]) -> Scene {
        Scene { flow: flow.to_string(), entry: entry.to_string(), set: set.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(), own: Vec::new() }
    }

    fn flow_of(loc: &str) -> String {
        crate::cow_clone::by_location(loc).map(|c| c.class).unwrap_or_else(|| "Animal_Cow".to_string())
    }

    fn plain_of(rf: &mut Romfs, loc: &str) -> HM<String, Flow> {
        let mut out: HM<String, Flow> = HM::new();
        if let Some(c) = crate::cow_clone::by_location(loc) {
            let b: Bf = Bf::parse(&crate::cow_clone::base(rf, &c.flow_file()).unwrap().unwrap()).unwrap();
            out.insert(c.class, b.flows.into_iter().next().unwrap());
        }
        out
    }

    fn both(r: &P, loc: &str, sc: &Scene) -> (Vec<Fin>, Vec<Fin>) {
        let mut rf: Romfs = Romfs::open(r).unwrap();
        let fl: HM<String, Flow> = edited(&mut rf, loc, NEW).unwrap();
        assert_eq!(beyond_always(&fl).len(), 1, "{loc}");
        let mut a: Sim = sim(r, plain_of(&mut rf, loc)).unwrap();
        let mut b: Sim = sim(r, fl).unwrap();
        (run(&mut a, sc).unwrap(), run(&mut b, sc).unwrap())
    }

    fn shape(f: &Fin, flag: &str) -> String {
        let gr: Vec<String> = f.m.gr.iter().filter(|(k, _)| k.as_str() != NEW).map(|(k, v)| format!("{k}={v}")).collect();
        let wr: Vec<String> = f.m.writes.iter().filter(|(k, _)| k.as_str() != flag).map(|(k, v)| format!("{k}={v}")).collect();
        format!("{:?} | {gr:?} | {:?} | {wr:?}", f.end, f.m.reqs)
    }

    fn shapes(fins: &[Fin], flag: &str) -> BS<String> {
        fins.iter().map(|f| shape(f, flag)).collect()
    }

    fn gave(f: &Fin, flag: &str) -> bool {
        f.m.writes.get(flag).map(|s| s.as_str()) == Some("true")
    }

    fn check_once(r: &P, loc: &str, entry: &str, flag: &str, set: &[(&str, &str)]) {
        let (a, b): (Vec<Fin>, Vec<Fin>) = both(r, loc, &scene_in(&flow_of(loc), entry, set));
        assert!(a.iter().all(|f| granted(f, NEW) == 0 && !f.m.writes.contains_key(flag)), "{loc} vanilla");
        assert!(a.iter().any(|f| called(f, BOTTLE)), "{loc} vanilla milk");
        assert_eq!(shapes(&a, flag), shapes(&b, flag), "{loc} the edited paths differ from vanilla beyond the one grant");
        assert!(b.iter().any(|f| granted(f, NEW) == 1), "{loc}");
        for f in &b {
            assert_eq!(gave(f, flag), granted(f, NEW) == 1, "{loc} the grant and the flag write go together");
            assert!(granted(f, NEW) <= 1, "{loc}");
        }
        assert_eq!(broken(&b), broken(&a), "{loc}");
        assert!(b.iter().filter(|f| f.end == End::Done).any(|f| called(f, BOTTLE)), "{loc} the milk path survives");
    }

    fn check_quiet(r: &P, loc: &str, entry: &str, flag: &str, set: &[(&str, &str)]) {
        let (a, b): (Vec<Fin>, Vec<Fin>) = both(r, loc, &scene_in(&flow_of(loc), entry, set));
        assert!(b.iter().all(|f| granted(f, NEW) == 0 && !f.m.writes.contains_key(flag)), "{loc} {set:?}");
        assert_eq!(shapes(&a, flag), shapes(&b, flag), "{loc} {set:?}");
        assert_eq!(a.len(), b.len(), "{loc} {set:?}");
    }

    #[test]
    fn the_house_cow_hands_out_the_item_once_and_the_milk_path_runs_as_vanilla() {
        let Some(r) = root() else { return };
        let flag: String = flag_of("KF Links House Cow").unwrap();
        check_once(&r, "KF Links House Cow", "PushToBottleMilk_LinkHouse", &flag, &[]);
        check_quiet(&r, "KF Links House Cow", "PushToBottleMilk_LinkHouse", &flag, &[(flag.as_str(), "true")]);
    }

    #[test]
    fn the_valley_cow_asks_for_a_child_and_the_fortress_adult_keeps_vanilla() {
        let Some(r) = root() else { return };
        let flag: String = flag_of("GV Cow").unwrap();
        check_once(&r, "GV Cow", "PushToBottleMilk_LinkHouse0", &flag, &[(CHILD, "true")]);
        check_quiet(&r, "GV Cow", "PushToBottleMilk_LinkHouse0", &flag, &[(CHILD, "true"), (flag.as_str(), "true")]);
        check_quiet(&r, "GV Cow", "PushToBottleMilk_LinkHouse0", &flag, &[(CHILD, "false")]);
    }

    fn with(on: bool) -> Settings {
        let mut s: Settings = Settings::default();
        s.set("shuffle_rewards", Value::s("supported")).unwrap();
        s.set(SETTING, Value::Bool(on)).unwrap();
        s
    }

    #[test]
    fn the_toggle_is_off_by_default_needs_supported_rewards_and_keeps_the_string() {
        assert!(!plan_of(&Settings::default()).unwrap().rewards.cows);
        assert!(!plan_of(&with(false)).unwrap().rewards.cows);
        assert!(plan_of(&with(true)).unwrap().rewards.cows);
        let mut s: Settings = Settings::default();
        s.set("shuffle_rewards", Value::s("off")).unwrap();
        s.set(SETTING, Value::Bool(true)).unwrap();
        assert!(!plan_of(&s).unwrap().rewards.cows);
        let mut d: Settings = Settings::default();
        d.set(SETTING, Value::Bool(false)).unwrap();
        assert_eq!(Settings::default().encode().unwrap(), d.encode().unwrap());
    }

    #[test]
    fn the_cows_are_flow_ok_but_closed_until_the_toggle_is_on() {
        let d: Data = Data::load().unwrap();
        let off = plan_of(&Settings::default()).unwrap();
        let on = plan_of(&with(true)).unwrap();
        for n in LOCS {
            let l: &Loc = d.flow.by_name(n).unwrap();
            assert!(d.flow_ok(n), "{n}");
            assert!(!crate::flow_extra::open(l, &off) && crate::flow_extra::open(l, &on), "{n}");
            assert_eq!(d.vanilla_of(n), Some("Rupees (5)"), "{n}");
            assert!(crate::slice::edit::is_sale(&d, n), "{n}");
        }
        assert!(!crate::flow_extra::bound(&d, "Rupees (5)") && !crate::flow_extra::bound(&d, "Milk"));
        assert!(!d.flow_ok("LLR Tower Left Cow"));
    }

    /// The plan and logic world `crate::slice::generate::allowed` judges a cow's item with. Every
    /// item may go to a cow: its once grant is the generic pouch get, and a dungeon item there
    /// credits its dungeon through a location credit (crate::debug_names), so only the setting and
    /// safety rules and the build checks remain.
    fn rules(d: &Data, s: &Settings) -> (Plan, World) {
        let p: Plan = plan_of(s).unwrap();
        let w: World = crate::slice::logic::world_with(d, &p.logic()).unwrap();
        (p, w)
    }

    #[test]
    fn toggle_off_places_no_cow_and_toggle_on_adds_all_eight_with_items_the_flows_hand_out() {
        let d: Data = Data::load().unwrap();
        let off: Generated = generate_data(&d, &with(false), "cows", &GenOpts::default()).unwrap();
        for l in LOCS {
            assert!(!off.seed_file.placements.contains_key(l), "{l}");
        }
        let (p, w): (Plan, World) = rules(&d, &with(true));
        for n in 0..3 {
            let g: Generated = generate_data(&d, &with(true), &format!("cows {n}"), &GenOpts::default()).unwrap();
            assert_eq!(g.seed_file.placements.len(), off.seed_file.placements.len() + LOCS.len() + OTHER.len());
            for l in LOCS {
                let it: &str = &g.seed_file.placements[l];
                assert!(allowed(&d, &w, &p, w.item(it).unwrap(), w.loc(l).unwrap()), "{l} holds {it}");
                assert_ne!(it, "Milk", "{l}");
            }
            assert!(g.seed_file.placements.values().all(|i| i != "Milk"));
        }
    }

    #[test]
    fn a_built_mod_carries_the_once_gate_for_both_cows_and_leaves_the_others_alone() {
        let Some(r) = root() else { return };
        let d: Data = Data::load().unwrap();
        // Every item may go to a cow now, and an upgrade, ammo or a dungeon item may add tier, bag
        // or credit events of its own, so the other two Animal_Cow cows (GV and GF) get plain items
        // too: the event count below is the three once gates alone.
        let o: GenOpts = GenOpts {
            plando: ["KF Links House Cow=Hover Boots", "GV Cow=Iron Boots", "GF Cow=Lens of Truth"].iter().map(|x| crate::slice::plando::parse(x).unwrap()).collect(),
            ..GenOpts::default()
        };
        let g: Generated = generate_data(&d, &with(true), "cows build", &o).unwrap();
        assert_eq!(g.seed_file.placements.get("KF Links House Cow").map(|s| s.as_str()), Some("Hover Boots"));
        let load: PB = PB::from(env!("CARGO_MANIFEST_DIR")).join("../../target/cows_mods");
        let _ = std::fs::remove_dir_all(&load);
        let b: crate::slice::edit::Built = crate::slice::edit::build_for(&g.seed_file, &r, &load, "ootrr-cows", crate::modpack::Host::Eden).unwrap();
        let mut rf: Romfs = Romfs::open(&r).unwrap();
        let z: Vec<u8> = std::fs::read(b.root.join("romfs").join(crate::flow_locations::path_of("Animal_Cow.bfevfl"))).unwrap();
        let out: crate::bfevfl::Bf = crate::bfevfl::Bf::parse(&rf.zs.decompress(&z).unwrap()).unwrap();
        let van: crate::bfevfl::Bf = crate::bfevfl::Bf::parse(&rf.read_zs(&crate::flow_locations::path_of("Animal_Cow.bfevfl")).unwrap()).unwrap();
        let (f, v): (&Flow, &Flow) = (&out.flows[0], &van.flows[0]);
        let main = |x: &Flow, n: &str| x.entries.iter().find(|e| e.name == n).map(|e| e.main).unwrap();
        assert_ne!(main(f, "PushToBottleMilk_LinkHouse"), main(v, "PushToBottleMilk_LinkHouse"));
        assert_ne!(main(f, "PushToBottleMilk_LinkHouse0"), main(v, "PushToBottleMilk_LinkHouse0"));
        assert_eq!(main(f, "PushToBottleMilk"), main(v, "PushToBottleMilk"));
        assert_eq!(f.events.len(), v.events.len() + 10);
        let gave: bool = f.events.iter().any(|e| matches!(&e.kind, crate::bfevfl::Kind::Sub { entry, params, .. } if entry == "GetInEvent_NotTurn_NotNpc" && params.as_ref().and_then(|c| c.get("ActorName")).is_some_and(|x| *x != crate::bfevfl::Val::Str("Item_Bottle_Milk".to_string()))));
        assert!(gave);
        let romfs: PB = b.root.join("romfs");
        assert!(crate::flow_extra::analyzed::worked(&mut rf, &romfs).unwrap().is_none(), "an edited cow event calls a flow its row does not load");
        let t: crate::byml::Byml = crate::byml::Byml::parse(&rf.zs.decompress(&std::fs::read(romfs.join(crate::progressive::ANALYZED_TABLE)).unwrap()).unwrap()).unwrap();
        let subs = |id: &str| -> Vec<String> {
            let n = t.root.as_arr().unwrap().iter().find(|n| n.get("__RowId").and_then(|v| v.as_str()) == Some(id)).unwrap();
            n.get(crate::flow_extra::analyzed::SUBS).and_then(|v| v.as_arr()).unwrap().iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect()
        };
        for id in ["GiveMilk_GerudoValley", "GiveMilk_GerudoFortress", "Animal_Cow_PushToBottleMilk_LinkHouse"] {
            assert!(subs(id).contains(&"Dm_SY_Get".to_string()), "{id}");
        }
        assert!(!subs("Animal_Cow_PushToBottleMilk").contains(&"Dm_SY_Get".to_string()));
        let rst: crate::rsizetable::Rst = crate::rsizetable::Rst::parse(&rf.zs.decompress(&std::fs::read(romfs.join(crate::rsizetable::TABLE_PATH)).unwrap()).unwrap()).unwrap();
        let van: crate::rsizetable::Rst = crate::rsizetable::load(&mut rf).unwrap();
        assert!(rst.get(crate::progressive::ANALYZED_TABLE) > van.get(crate::progressive::ANALYZED_TABLE));
        let _ = std::fs::remove_dir_all(&load);
    }

    #[test]
    fn the_two_cows_edit_one_file_together_and_add_seven_events() {
        let Some(r) = root() else { return };
        let mut rf: Romfs = Romfs::open(&r).unwrap();
        let d: FlowData = FlowData::load().unwrap();
        let ch: Vec<Change> = LOCS.iter().flat_map(|l| changes(d.by_name(l).unwrap(), NEW).unwrap()).collect();
        let os = crate::flow_extra::ops_for(&ch).unwrap();
        assert_eq!(os.len(), 1 + crate::flow_extra::always_ops().unwrap().len());
        assert_eq!(os["Animal_Cow.bfevfl"].len(), 2);
        let raw: Vec<u8> = rf.read_zs(&crate::flow_locations::path_of("Animal_Cow.bfevfl")).unwrap();
        let b: crate::bfevfl::Bf = crate::flow_extra::edit_flow(&raw, &[], &os["Animal_Cow.bfevfl"]).unwrap();
        assert_eq!(crate::bfevfl::Bf::parse(&b.write().unwrap()).unwrap(), b);
        let vanilla: crate::bfevfl::Bf = crate::bfevfl::Bf::parse(&raw).unwrap();
        assert_eq!(b.flows[0].events.len(), vanilla.flows[0].events.len() + 7);
    }

    #[test]
    fn both_cows_are_gated_flow_locations_with_one_once_edit_each() {
        let d: FlowData = FlowData::load().unwrap();
        for n in LOCS {
            let l: &Loc = d.by_name(n).unwrap();
            assert!(l.gated && !l.supported, "{n}");
            assert_eq!(l.cat, "cow", "{n}");
            assert_eq!(l.item.as_deref(), Some("Item_Bottle_Milk"), "{n}");
            let ch: Vec<Change> = changes(l, "Item_Longshot").unwrap();
            assert_eq!(ch.len(), 1, "{n}");
            assert!(ch.iter().all(|c| c.role == ROLE && c.new == "Item_Longshot" && c.old == "Item_Bottle_Milk" && !plain(c)), "{n}");
            assert_eq!(ch[0].file, "Animal_Cow.bfevfl");
        }
        assert!(changes(d.by_name("LW Trade Cojiro").unwrap(), "Item_Longshot").is_err());
    }

    #[test]
    fn the_once_flags_are_distinct_unused_persistent_bools() {
        let a: String = flag_of("KF Links House Cow").unwrap();
        let b: String = flag_of("GV Cow").unwrap();
        assert_ne!(a, b);
        for f in [&a, &b] {
            let row: &str = FLAGS.lines().find(|l| l.contains(&format!("\"name\":\"{f}\""))).unwrap();
            assert!(row.contains("\"type\":\"Bool\"") && row.contains("\"save\":0") && row.contains("\"reset\":8"), "{f}");
            assert!(f.starts_with("HavePlayedEvent."), "{f}");
        }
        assert!(flag_of("LW Trade Cojiro").is_none());
    }

    #[test]
    fn the_gerudo_valley_edit_asks_for_a_child_and_the_house_edit_does_not() {
        for (n, child, entry) in [("KF Links House Cow", false, "PushToBottleMilk_LinkHouse"), ("GV Cow", true, "PushToBottleMilk_LinkHouse0")] {
            let s: Spec = spec(n).unwrap();
            assert_eq!(s.ops.len(), 1, "{n}");
            let How::Once(w) = &s.ops[0].how else { panic!("once") };
            assert_eq!((w.child, w.entry.as_str()), (child, entry), "{n}");
            assert_eq!(w.sub, "Dm_SY_GetInEvent:GetInEvent_NotTurn_NotNpc");
        }
    }

    fn with_other(on: bool) -> Settings {
        let mut s: Settings = Settings::default();
        s.set("shuffle_rewards", Value::s("supported")).unwrap();
        s.set(SETTING_OTHER, Value::Bool(on)).unwrap();
        s
    }

    #[test]
    fn every_clone_cow_hands_out_its_item_once_from_its_own_flow() {
        let Some(r) = root() else { return };
        for c in crate::cow_clone::all() {
            let flag: String = flag_of(&c.location).unwrap();
            check_once(&r, &c.location, crate::cow_clone::flow::PUSH, &flag, &[]);
            check_quiet(&r, &c.location, crate::cow_clone::flow::PUSH, &flag, &[(flag.as_str(), "true")]);
        }
    }

    #[test]
    fn an_unedited_clone_runs_the_house_milk_path_with_its_own_name() {
        let Some(r) = root() else { return };
        let mut rf: Romfs = Romfs::open(&r).unwrap();
        let mut v: Sim = sim(&r, HM::new()).unwrap();
        let house: Vec<Fin> = run(&mut v, &scene_in("Animal_Cow", "PushToBottleMilk_LinkHouse", &[])).unwrap();
        let norm = |fins: &[Fin], class: &str| -> BS<String> { shapes(fins, "").into_iter().map(|s| s.replace(class, "Animal_Cow_LinkHouse")).collect() };
        for c in crate::cow_clone::all() {
            let mut a: Sim = sim(&r, plain_of(&mut rf, &c.location)).unwrap();
            let fins: Vec<Fin> = run(&mut a, &scene_in(&c.class, crate::cow_clone::flow::PUSH, &[])).unwrap();
            assert_eq!(norm(&fins, &c.class), norm(&house, "Animal_Cow_LinkHouse"), "{}", c.class);
            assert_eq!(fins.len(), house.len(), "{}", c.class);
            let give: Vec<Fin> = run(&mut a, &scene_in(&c.class, crate::cow_clone::flow::GIVE, &[])).unwrap();
            assert!(give.iter().any(|f| called(f, "Animal_Cow:Cow_GiveMilk")), "{}", c.class);
            let talk: Vec<Fin> = run(&mut a, &scene_in(&c.class, crate::cow_clone::flow::TALK, &[])).unwrap();
            assert!(talk.iter().any(|f| called(f, "Animal_Cow:CowTalk")), "{}", c.class);
            assert_eq!(broken(&give) + broken(&talk) + broken(&fins), 0, "{}", c.class);
        }
    }

    #[test]
    fn the_fortress_cow_asks_for_an_adult_and_the_valley_child_keeps_vanilla() {
        let Some(r) = root() else { return };
        let flag: String = flag_of("GF Cow").unwrap();
        check_once(&r, "GF Cow", "PushToBottleMilk_LinkHouse0", &flag, &[(CHILD, "false")]);
        check_quiet(&r, "GF Cow", "PushToBottleMilk_LinkHouse0", &flag, &[(CHILD, "false"), (flag.as_str(), "true")]);
        check_quiet(&r, "GF Cow", "PushToBottleMilk_LinkHouse0", &flag, &[(CHILD, "true")]);
    }

    #[test]
    fn with_both_gerudo_gates_a_child_gets_the_valley_item_and_an_adult_the_fortress_item() {
        let Some(r) = root() else { return };
        let mut rf: Romfs = Romfs::open(&r).unwrap();
        let d: FlowData = FlowData::load().unwrap();
        let (gv, gf): (String, String) = (flag_of("GV Cow").unwrap(), flag_of("GF Cow").unwrap());
        for order in [["GV Cow", "GF Cow"], ["GF Cow", "GV Cow"]] {
            let ch: Vec<Change> = order.iter().flat_map(|l| changes(d.by_name(l).unwrap(), NEW).unwrap()).collect();
            let os = crate::flow_extra::ops_for(&ch).unwrap();
            let raw: Vec<u8> = rf.read_zs(&crate::flow_locations::path_of("Animal_Cow.bfevfl")).unwrap();
            let b: Bf = crate::flow_extra::edit_flow(&raw, &[], &os["Animal_Cow.bfevfl"]).unwrap();
            assert_eq!(Bf::parse(&b.write().unwrap()).unwrap(), b);
            let mut fl: HM<String, Flow> = HM::new();
            fl.insert("Animal_Cow".to_string(), b.flows[0].clone());
            let mut s: Sim = sim(&r, fl).unwrap();
            for (child, want, other) in [("true", &gv, &gf), ("false", &gf, &gv)] {
                let fins: Vec<Fin> = run(&mut s, &scene_in("Animal_Cow", "PushToBottleMilk_LinkHouse0", &[(CHILD, child)])).unwrap();
                assert!(fins.iter().any(|f| granted(f, NEW) == 1 && gave(f, want)), "{order:?} {child}");
                assert!(fins.iter().all(|f| !f.m.writes.contains_key(other.as_str())), "{order:?} {child}");
                let done: Vec<Fin> = run(&mut s, &scene_in("Animal_Cow", "PushToBottleMilk_LinkHouse0", &[(CHILD, child), (want.as_str(), "true")])).unwrap();
                assert!(done.iter().all(|f| granted(f, NEW) == 0), "{order:?} {child}");
            }
        }
    }

    #[test]
    fn one_cow_toggle_covers_all_eight_and_the_old_other_toggle_only_keeps_the_string() {
        assert!(!plan_of(&Settings::default()).unwrap().rewards.cows_other);
        assert!(!plan_of(&with_other(false)).unwrap().rewards.cows_other);
        assert!(!plan_of(&with_other(true)).unwrap().rewards.cows_other && !plan_of(&with_other(true)).unwrap().rewards.cows);
        assert!(plan_of(&with(true)).unwrap().rewards.cows_other && plan_of(&with(true)).unwrap().rewards.cows);
        let mut s: Settings = Settings::default();
        s.set("shuffle_rewards", Value::s("off")).unwrap();
        s.set(SETTING, Value::Bool(true)).unwrap();
        assert!(!plan_of(&s).unwrap().rewards.cows_other);
        let mut d: Settings = Settings::default();
        d.set(SETTING_OTHER, Value::Bool(false)).unwrap();
        assert_eq!(Settings::default().encode().unwrap(), d.encode().unwrap());
        assert_eq!(crate::settings::schema().get(SETTING_OTHER).map(|x| x.num), Some(66));
        assert_eq!(crate::settings::schema().get(SETTING).map(|x| x.num), Some(65));
    }

    #[test]
    fn the_other_cows_are_flow_ok_and_open_with_the_cow_toggle() {
        let d: Data = Data::load().unwrap();
        let (off, one, two) = (plan_of(&Settings::default()).unwrap(), plan_of(&with(true)).unwrap(), plan_of(&with_other(true)).unwrap());
        for n in OTHER {
            let l: &Loc = d.flow.by_name(n).unwrap();
            assert!(d.flow_ok(n) && is(n) && is_other(n), "{n}");
            assert!(l.gated && !l.supported && l.cat == "cow", "{n}");
            assert!(!crate::flow_extra::open(l, &off) && crate::flow_extra::open(l, &one) && !crate::flow_extra::open(l, &two), "{n}");
            assert_eq!(d.vanilla_of(n), Some("Rupees (5)"), "{n}");
            assert!(crate::slice::edit::is_sale(&d, n), "{n}");
            assert_eq!(changes(l, NEW).unwrap().len(), 1, "{n}");
        }
        for n in LOCS {
            assert!(!crate::flow_extra::open(d.flow.by_name(n).unwrap(), &two) && !is_other(n), "{n}");
        }
    }

    #[test]
    fn the_clone_data_points_at_the_events_the_builder_makes() {
        let x: Extra = Extra::load().unwrap();
        let d: FlowData = FlowData::load().unwrap();
        for c in crate::cow_clone::all() {
            let s: &Spec = x.by_name(&c.location).unwrap();
            assert_eq!(s.ops.len(), 1, "{}", c.class);
            let o = &s.ops[0];
            assert_eq!((o.file.as_str(), o.index, o.event.as_str()), (c.flow_file().as_str(), usize::from(crate::cow_clone::flow::HEAD), "Event2"), "{}", c.class);
            let How::Once(w) = &o.how else { panic!("once") };
            assert!(!w.child && !w.adult && w.entry == crate::cow_clone::flow::PUSH, "{}", c.class);
            let l: &Loc = d.by_name(&c.location).unwrap();
            assert_eq!(l.edits.len(), 1);
            assert_eq!((l.edits[0].file.as_str(), l.edits[0].index, l.edits[0].event.as_str()), (c.flow_file().as_str(), 16, "Event74"), "{}", c.class);
        }
        let Some(r) = root() else { return };
        let mut rf: Romfs = Romfs::open(&r).unwrap();
        let (cow, _): (Bf, Bf) = crate::cow_clone::flow::sources(&mut rf).unwrap();
        assert_eq!(crate::cow_clone::flow::mapped(&cow, 45).unwrap(), 16);
        let gf = x.by_name("GF Cow").unwrap();
        let How::Once(w) = &gf.ops[0].how else { panic!("once") };
        assert!(w.adult && !w.child && w.entry == "PushToBottleMilk_LinkHouse0");
    }

    #[test]
    fn every_cow_flag_is_a_distinct_persistent_bool_that_no_game_file_names() {
        let all: Vec<String> = LOCS.iter().chain(OTHER.iter()).map(|n| flag_of(n).unwrap()).collect();
        let set: BS<&String> = all.iter().collect();
        assert_eq!(set.len(), 8);
        for f in &all {
            let row: &str = FLAGS.lines().find(|l| l.contains(&format!("\"name\":\"{f}\""))).unwrap();
            assert!(row.contains("\"type\":\"Bool\"") && row.contains("\"save\":0") && row.contains("\"reset\":8"), "{f}");
            assert!(f.starts_with("HavePlayedEvent.Dm_StaffCredit_00"), "{f}");
        }
        for bad in ["0022", "0003", "0004", "0020", "0021"] {
            assert!(!all.iter().any(|f| f.ends_with(bad)), "{bad}");
        }
        let Some(r) = root() else { return };
        let mut rf: Romfs = Romfs::open(&r).unwrap();
        let names: Vec<String> = all.iter().map(|f| f.trim_start_matches("HavePlayedEvent.").to_string()).collect();
        let es: crate::byml::Byml = rf.read_byml(crate::progressive::SETTING_TABLE).unwrap();
        let rows: String = format!("{:?}", es.root);
        for n in &names {
            assert!(!rows.contains(&format!("\"{n}\"")), "{n} has an event row");
        }
        let flows: Vec<String> = rf.list("Event/EventFlow").unwrap();
        assert!(flows.len() > 800);
        for f in flows.iter().filter(|f| f.ends_with(".bfevfl.zs")) {
            let raw: Vec<u8> = rf.read_zs(f).unwrap();
            for n in &names {
                assert!(!raw.windows(n.len()).any(|w| w == n.as_bytes()), "{f} names {n}");
            }
        }
    }

    #[test]
    fn the_cow_toggle_adds_the_six_other_cows_with_items_the_flows_hand_out() {
        let d: Data = Data::load().unwrap();
        let off: Generated = generate_data(&d, &with(false), "cows other", &GenOpts::default()).unwrap();
        for l in OTHER {
            assert!(!off.seed_file.placements.contains_key(l), "{l}");
        }
        let (p, w): (Plan, World) = rules(&d, &with(true));
        for n in 0..2 {
            let g: Generated = generate_data(&d, &with(true), &format!("cows other {n}"), &GenOpts::default()).unwrap();
            assert_eq!(g.seed_file.placements.len(), off.seed_file.placements.len() + LOCS.len() + OTHER.len());
            for l in OTHER {
                let it: &str = &g.seed_file.placements[l];
                assert!(allowed(&d, &w, &p, w.item(it).unwrap(), w.loc(l).unwrap()), "{l} holds {it}");
            }
        }
    }

    #[test]
    fn a_built_mod_makes_each_ordinary_cow_its_own_class_in_every_age_file() {
        let Some(r) = root() else { return };
        let d: Data = Data::load().unwrap();
        let o: GenOpts = GenOpts { plando: vec![crate::slice::plando::parse("DMT Cow Grotto Cow=Hover Boots").unwrap()], ..GenOpts::default() };
        let g: Generated = generate_data(&d, &with(true), "cows other build", &o).unwrap();
        let load: PB = PB::from(env!("CARGO_MANIFEST_DIR")).join("../../target/cows_other_mods");
        let _ = std::fs::remove_dir_all(&load);
        let b: crate::slice::edit::Built = crate::slice::edit::build_for(&g.seed_file, &r, &load, "ootrr-cows-other", crate::modpack::Host::Eden).unwrap();
        let romfs: PB = b.root.join("romfs");
        let mut rf: Romfs = Romfs::open(&r).unwrap();
        let rd = |rf: &mut Romfs, p: &str| -> Vec<u8> { rf.zs.decompress(&std::fs::read(p.split('/').fold(romfs.clone(), |a, s| a.join(s))).unwrap()).unwrap() };
        let rst: crate::rsizetable::Rst = crate::rsizetable::Rst::parse(&rd(&mut rf, crate::rsizetable::TABLE_PATH)).unwrap();
        let info: crate::byml::Byml = crate::byml::Byml::parse(&rd(&mut rf, crate::pickup_pack::rows::ACTOR_INFO)).unwrap();
        let es: crate::byml::Byml = crate::byml::Byml::parse(&rd(&mut rf, crate::progressive::SETTING_TABLE)).unwrap();
        let an: crate::byml::Byml = crate::byml::Byml::parse(&rd(&mut rf, crate::progressive::ANALYZED_TABLE)).unwrap();
        let tag: crate::pickup_pack::tag::Table = crate::pickup_pack::tag::Table::parse(&rd(&mut rf, crate::pickup_pack::tag::TAG)).unwrap();
        let row = |y: &crate::byml::Byml, id: &str| y.root.as_arr().unwrap().iter().find(|n| n.get("__RowId").and_then(|v| v.as_str()) == Some(id)).cloned();
        for c in crate::cow_clone::all() {
            let pk: String = crate::cow_clone::pack::pack_of(&c.class);
            let raw: Vec<u8> = rd(&mut rf, &pk);
            assert_eq!(rst.get(&pk), Some(crate::pickup_pack::sizes::pack_size(raw.len()).unwrap()), "{}", c.class);
            let fp: String = crate::flow_locations::path_of(&c.flow_file());
            let fl: Vec<u8> = rd(&mut rf, &fp);
            assert!(rst.get(&fp).is_some_and(|n| n as usize > fl.len()), "{}", c.class);
            let sp: String = crate::cow_clone::pack::path(crate::cow_clone::pack::SCHEDULE, &c.class);
            assert!(romfs.join(&sp).is_file() && rst.get(&sp).is_some(), "{}", c.class);
            assert!(row(&info, &c.class).is_some(), "{}", c.class);
            for id in [crate::cow_clone::flow::talk_row(&c.class), crate::cow_clone::flow::give_row(&c.class), crate::cow_clone::flow::push_row(&c.class)] {
                assert!(row(&es, &id).is_some() && row(&an, &id).is_some(), "{id}");
            }
            let push = row(&an, &crate::cow_clone::flow::push_row(&c.class)).unwrap();
            let subs: Vec<&str> = push.get("SubFlowList").unwrap().as_arr().unwrap().iter().filter_map(|x| x.as_str()).collect();
            assert!(subs.contains(&"Dm_SY_GetInEvent"), "{} {subs:?}", c.class);
            assert_eq!(tag.tags_of(&c.class), tag.tags_of("Animal_Cow"));
            for s in &c.spots {
                let y: crate::byml::Byml = crate::byml::Byml::parse(&rd(&mut rf, &s.file)).unwrap();
                assert_eq!(crate::pickup_pack::place::current(&y, s.hash).as_deref(), Some(c.class.as_str()), "{} {}", c.class, s.file);
            }
        }
        assert!(crate::flow_extra::analyzed::worked(&mut rf, &romfs).unwrap().is_none());
        let gf = row(&an, "GiveMilk_GerudoFortress").unwrap();
        assert!(gf.get("SubFlowList").unwrap().as_arr().unwrap().iter().any(|x| x.as_str() == Some("Dm_SY_Get")));
        assert!(b.report.contains("actor classes of their own"), "{}", b.report);
        let _ = std::fs::remove_dir_all(&load);
    }
}
