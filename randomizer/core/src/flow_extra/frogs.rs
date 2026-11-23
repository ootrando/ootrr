use crate::error::{Error as E, Res};
use crate::flow_extra::data::{Extra, Spec};
use crate::flow_extra::op::How;
use crate::flow_locations::{Change, Loc};

pub const SONGS_SETTING: &str = "shuffle_frog_songs";
pub const BUG_SETTING: &str = "shuffle_frog_bug_heart";
pub const ROLE: &str = "frog";
pub const FILE: &str = "MiniGame_FrogTenors.bfevfl";
pub const CALLER: &str = ":TeachNewMusic";
pub const PURPLE: &str = "Obj_Rupee_Purple";

pub const SONGS: [&str; 5] = ["ZR Frogs Zeldas Lullaby", "ZR Frogs Eponas Song", "ZR Frogs Sarias Song", "ZR Frogs Suns Song", "ZR Frogs Song of Time"];
pub const BUG: &str = "ZR Frogs Bug Game";

pub const SONGS_NOTE: &str = "The Zora's River frogs give one shuffled item for the first time each of Zelda's Lullaby, Epona's Song, Saria's Song, the Sun's Song and the Song of Time is played to them, in place of the rupee payout. The separately mapped ZR Frogs Ocarina Game reward is a check of its own.";
pub const BUG_NOTE: &str = "The first win of the Zora's River frog bug game gives a shuffled item in place of the Heart Piece. Later wins still give the vanilla Blue Rupee.";

pub fn is_song(name: &str) -> bool {
    SONGS.contains(&name)
}

pub fn is_bug(name: &str) -> bool {
    name == BUG
}

pub fn is(name: &str) -> bool {
    is_song(name) || is_bug(name)
}

fn spec(name: &str) -> Res<Spec> {
    Extra::load()?.specs.into_iter().find(|s| s.name == name).ok_or_else(|| E::Msg(format!("{name} has no frog edit in data/flow_extra.json")))
}

fn own(name: &str, how: &How) -> Option<String> {
    match how {
        How::SwapSub { from, params, .. } if is_song(name) && from == CALLER => params.iter().find(|(k, _)| k == "ActorName").map(|(k, _)| k.clone()),
        How::Set { param, .. } if is_bug(name) => Some(param.clone()),
        _ => None,
    }
}

pub fn changes(l: &Loc, item: &str) -> Res<Vec<Change>> {
    let s: Spec = spec(&l.name)?;
    let mut out: Vec<Change> = Vec::new();
    for o in &s.ops {
        if let Some(param) = own(&l.name, &o.how) {
            out.push(Change {
                location: l.name.clone(),
                file: o.file.clone(),
                index: o.index,
                event: o.event.clone(),
                param,
                role: ROLE.to_string(),
                old: l.item.clone().unwrap_or_default(),
                new: item.to_string(),
            });
        }
    }
    if out.is_empty() {
        return Err(E::Msg(format!("{} has no frog edit in data/flow_extra.json", l.name)));
    }
    Ok(out)
}

pub fn plain(c: &Change) -> bool {
    c.role != ROLE
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap as HM;
    use std::path::{Path as P, PathBuf as PB};

    use super::*;
    use crate::bfevfl::{Flow, Kind, Val};
    use crate::flow_extra::sim::{beyond_always, broken, called, edited, edited_many, granted, run, sim, Scene};
    use crate::flow_locations::{Choice, Data as FlowData};
    use crate::flow_sim::run::{End, Fin, Sim};
    use crate::romfs::Romfs;
    use crate::settings::{Settings, Value};
    use crate::slice::data::Data;
    use crate::slice::generate::{allowed, generate_data, GenOpts, Generated};
    use crate::slice::support::{plan as plan_of, Plan};
    use crate::world::World;

    const FLAGS: &str = include_str!("../../../../data/flags.json");
    const FLOW: &str = "MiniGame_FrogTenors";
    const MELODY: &str = "PlayOcarinaForFrogMiniGame_Melody";
    const COUNT: &str = "MiniGame_FrogTenors_GetCount";
    const RUPEE: &str = "PlayerStatus.CurrentRupee";
    const REPEL: &str = "MiniGame_FrogTenors_InsectRepellent";
    const GRANT: &str = "Dm_SY_GetInEvent:GetInEvent_NotTurn_NotNpc";
    const ITEMS: [&str; 5] = ["Item_Longshot", "Item_HoverBoot", "Item_HeavyBoot", "Item_Bottle", "Item_MagicBean"];
    const SONG_FLAGS: [&str; 5] = [
        "MiniGame_FrogTenors_ZeldaLullaby",
        "MiniGame_FrogTenors_EponaSong",
        "MiniGame_FrogTenors_SariaSong",
        "MiniGame_FrogTenors_SunSong",
        "MiniGame_FrogTenors_TimeSong",
    ];
    const BUG_ITEM: &str = "Item_Arrow_Ice";

    fn root() -> Option<PB> {
        let p: PB = ootrr_game::test_romfs::path();
        if p.join("Event/EventFlow").is_dir() {
            Some(p)
        } else {
            eprintln!("skipped: no romfs at {}, set OOTRR_ROMFS", p.display());
            None
        }
    }

    fn scene(entry: &str, set: &[(String, String)]) -> Scene {
        Scene { flow: FLOW.to_string(), entry: entry.to_string(), set: set.to_vec(), own: Vec::new() }
    }

    fn pairs(rows: &[(&str, &str)]) -> Vec<(String, String)> {
        rows.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    fn all_songs() -> Vec<Choice> {
        SONGS.iter().zip(ITEMS).map(|(l, i)| Choice { location: l.to_string(), item: i.to_string() }).collect()
    }

    fn song_state(done: &[usize], count: usize) -> Vec<(String, String)> {
        let mut v: Vec<(String, String)> = pairs(&[("MiniGame_FrogTenors_SongClear", "false"), ("MiniGame_FrogTenors_StormSong", "true"), (RUPEE, "0")]);
        v.push((COUNT.to_string(), count.to_string()));
        for (i, f) in SONG_FLAGS.iter().enumerate() {
            v.push((f.to_string(), done.contains(&i).to_string()));
        }
        v
    }

    fn gave_any(f: &Fin) -> i64 {
        ITEMS.iter().map(|i| granted(f, i)).sum()
    }

    fn write<'a>(f: &'a Fin, k: &str) -> Option<&'a str> {
        f.m.writes.get(k).map(|s| s.as_str())
    }

    fn sims(r: &P, fl: HM<String, Flow>, sc: &Scene) -> (Vec<Fin>, Vec<Fin>) {
        let mut a: Sim = sim(r, HM::new()).unwrap();
        let mut b: Sim = sim(r, fl).unwrap();
        (run(&mut a, sc).unwrap(), run(&mut b, sc).unwrap())
    }

    #[test]
    fn each_song_pays_its_own_item_once_and_the_count_still_rises_by_one() {
        let Some(r) = root() else { return };
        let mut rf: Romfs = Romfs::open(&r).unwrap();
        let fl: HM<String, Flow> = edited_many(&mut rf, &all_songs()).unwrap();
        assert_eq!(beyond_always(&fl), vec![FLOW.to_string()]);
        for count in [0usize, 2, 4] {
            let done: Vec<usize> = (0..count).collect();
            let sc: Scene = scene(MELODY, &song_state(&done, count));
            let (a, b): (Vec<Fin>, Vec<Fin>) = sims(&r, fl.clone(), &sc);
            assert!(a.iter().all(|f| gave_any(f) == 0), "{count}");
            assert!(a.iter().any(|f| write(f, RUPEE).is_some_and(|v| v != "0")), "{count} vanilla pays rupees");
            assert!(b.iter().all(|f| write(f, RUPEE).is_none_or(|v| v == "0")), "{count} no rupee payout is left");
            assert!(b.iter().all(|f| gave_any(f) <= 1), "{count}");
            for (i, item) in ITEMS.iter().enumerate() {
                let got: Vec<&Fin> = b.iter().filter(|f| granted(f, item) == 1).collect();
                if done.contains(&i) {
                    assert!(got.is_empty(), "{count} {item} a played song pays again");
                    continue;
                }
                assert!(!got.is_empty(), "{count} {item}");
                for f in got {
                    assert_eq!(write(f, SONG_FLAGS[i]), Some("true"), "{count} {item}");
                    assert_eq!(write(f, COUNT), Some((count + 1).to_string().as_str()), "{count} {item}");
                    assert!(called(f, GRANT), "{count} {item}");
                    assert!(!called(f, ":GetRupy_FrogsTenors"), "{count} {item}");
                }
            }
            for f in &b {
                let paid: bool = gave_any(f) == 1;
                let counted: bool = write(f, COUNT).is_some();
                assert_eq!(paid, counted, "{count} the count rises exactly when an item is paid");
            }
            assert!(broken(&b) <= broken(&a), "{count}");
        }
    }

    #[test]
    fn the_storm_heart_piece_and_the_other_songs_keep_their_paths() {
        let Some(r) = root() else { return };
        let mut rf: Romfs = Romfs::open(&r).unwrap();
        let fl: HM<String, Flow> = edited_many(&mut rf, &all_songs()).unwrap();
        let mut set: Vec<(String, String)> = song_state(&[0, 1, 2, 3, 4], 5);
        set.retain(|(k, _)| k != "MiniGame_FrogTenors_StormSong");
        set.push(("MiniGame_FrogTenors_StormSong".to_string(), "false".to_string()));
        let sc: Scene = scene(MELODY, &set);
        let (a, b): (Vec<Fin>, Vec<Fin>) = sims(&r, fl, &sc);
        assert!(a.iter().any(|f| granted(f, "Obj_Heart_Piece") == 1));
        assert!(b.iter().any(|f| granted(f, "Obj_Heart_Piece") == 1));
        assert!(b.iter().all(|f| gave_any(f) == 0));
        assert_eq!(a.len(), b.len());
        assert_eq!(broken(&b), broken(&a));
    }

    fn bug_state(repel: bool, retry: bool) -> (Scene, Vec<(String, String)>) {
        let set: Vec<(String, String)> = pairs(&[
            ("MiniGame_FrogTenors_SongClear", "true"),
            (REPEL, if repel { "true" } else { "false" }),
            ("MiniGame_FrogTenors_InsectRepellent_Explanation", "true"),
            ("MiniGame_FrogTenors_InsectRepellent_Clearnumber", if repel { "1" } else { "0" }),
        ]);
        let entry: &str = if retry { "FrogMiniGame_RetryFlow" } else { "FrogMiniGame_Insectrepellent" };
        (scene(entry, &set), set)
    }

    fn first_bug_win(part: usize) {
        let Some(r) = root() else { return };
        let mut rf: Romfs = Romfs::open(&r).unwrap();
        let fl: HM<String, Flow> = edited(&mut rf, BUG, BUG_ITEM).unwrap();
        for retry in [[false, true][part]] {
            let (first, _) = bug_state(false, retry);
            let (a, b): (Vec<Fin>, Vec<Fin>) = sims(&r, fl.clone(), &first);
            assert!(a.iter().any(|f| granted(f, "Obj_Heart_Piece") == 1), "{retry}");
            assert!(b.iter().all(|f| granted(f, "Obj_Heart_Piece") == 0 && granted(f, "Obj_Rupee_Blue") == 0), "{retry}");
            let won: Vec<&Fin> = b.iter().filter(|f| granted(f, BUG_ITEM) == 1).collect();
            assert!(!won.is_empty(), "{retry}");
            let done: Vec<&&Fin> = won.iter().filter(|f| f.end == End::Done).collect();
            assert!(!done.is_empty() && done.iter().all(|f| write(f, REPEL) == Some("true")), "{retry} the first win still sets the flag");
            let van = |v: &[Fin], it: &str| v.iter().filter(|f| granted(f, it) == 1).map(|f| (format!("{:?}", f.end), write(f, REPEL).map(|s| s.to_string()))).collect::<Vec<_>>();
            assert_eq!(van(&a, "Obj_Heart_Piece"), van(&b, BUG_ITEM), "{retry} the edited wins end like the vanilla heart piece wins");
            assert!(b.iter().all(|f| granted(f, BUG_ITEM) <= 1), "{retry}");
            assert_eq!(broken(&b), broken(&a), "{retry}");
            let (later, _) = bug_state(true, retry);
            let (a, b): (Vec<Fin>, Vec<Fin>) = sims(&r, fl.clone(), &later);
            assert!(b.iter().all(|f| granted(f, BUG_ITEM) == 0 && granted(f, "Obj_Heart_Piece") == 0), "{retry}");
            assert!(b.iter().any(|f| granted(f, "Obj_Rupee_Blue") == 1), "{retry}");
            assert_eq!(a.len(), b.len(), "{retry}");
            assert_eq!(broken(&b), broken(&a), "{retry}");
        }
    }

    parts!(
        first_bug_win,
        the_first_bug_win_pays_the_item_and_sets_the_flag_and_later_wins_stay_blue_rupees = 0,
        the_first_bug_win_on_a_retry_pays_the_item_and_sets_the_flag_and_later_wins_stay_blue_rupees = 1,
    );

    #[test]
    fn the_bug_edit_touches_only_the_two_heart_piece_callers() {
        let Some(r) = root() else { return };
        let mut rf: Romfs = Romfs::open(&r).unwrap();
        let fl: HM<String, Flow> = edited(&mut rf, BUG, BUG_ITEM).unwrap();
        let raw: Vec<u8> = rf.read_zs(&crate::flow_locations::path_of(FILE)).unwrap();
        let van: crate::bfevfl::Bf = crate::bfevfl::Bf::parse(&raw).unwrap();
        let (f, v): (&Flow, &Flow) = (&fl[FLOW], &van.flows[0]);
        assert_eq!(f.events.len(), v.events.len());
        let diff: Vec<usize> = (0..v.events.len()).filter(|i| f.events[*i] != v.events[*i]).collect();
        assert_eq!(diff, vec![71, 91]);
        for i in [71, 91] {
            assert_eq!(f.events[i].kind.params().and_then(|c| c.get("ActorName")), Some(&Val::Str(BUG_ITEM.to_string())));
        }
        assert!(beyond_always(&edited(&mut rf, BUG, "Obj_Heart_Piece").unwrap()).is_empty());
    }

    #[test]
    fn the_song_edit_changes_only_the_callers_the_payouts_and_the_rupee_display() {
        let Some(r) = root() else { return };
        let mut rf: Romfs = Romfs::open(&r).unwrap();
        let fl: HM<String, Flow> = edited_many(&mut rf, &all_songs()).unwrap();
        let raw: Vec<u8> = rf.read_zs(&crate::flow_locations::path_of(FILE)).unwrap();
        let van: crate::bfevfl::Bf = crate::bfevfl::Bf::parse(&raw).unwrap();
        let (f, v): (&Flow, &Flow) = (&fl[FLOW], &van.flows[0]);
        assert_eq!(f.events.len(), v.events.len());
        let diff: Vec<usize> = (0..v.events.len()).filter(|i| f.events[*i] != v.events[*i]).collect();
        assert_eq!(diff, vec![79, 80, 81, 82, 83, 84, 203, 236, 239, 240, 241, 242]);
        for (i, item) in [80usize, 81, 82, 83, 84].iter().zip(ITEMS) {
            assert_eq!(f.events[*i].kind.params().and_then(|c| c.get("ActorName")), Some(&Val::Str(item.to_string())));
        }
        for i in [236usize, 239, 240, 241, 242] {
            let Kind::Sub { next, flow, entry, params } = &f.events[i].kind else { panic!("sub") };
            assert_eq!((*next, flow.as_str(), entry.as_str()), (243, "Dm_SY_GetInEvent", "GetInEvent_NotTurn_NotNpc"));
            assert_eq!(params.as_ref().and_then(|c| c.get("ActorName")), Some(&Val::Arg("ActorName".to_string())));
        }
        let one: HM<String, Flow> = edited(&mut rf, SONGS[1], ITEMS[1]).unwrap();
        let d1: Vec<usize> = (0..v.events.len()).filter(|i| one[FLOW].events[*i] != v.events[*i]).collect();
        assert_eq!(d1, diff);
        for i in [80usize, 82, 83, 84] {
            assert_eq!(one[FLOW].events[i].kind.params().and_then(|c| c.get("ActorName")), Some(&Val::Str(PURPLE.to_string())), "{i}");
        }
    }

    fn with(songs: bool, bug: bool) -> Settings {
        let mut s: Settings = Settings::default();
        s.set("shuffle_rewards", Value::s("supported")).unwrap();
        s.set(SONGS_SETTING, Value::Bool(songs)).unwrap();
        s.set(BUG_SETTING, Value::Bool(bug)).unwrap();
        s
    }

    #[test]
    fn both_toggles_are_off_by_default_need_supported_rewards_and_keep_the_string() {
        let p = plan_of(&Settings::default()).unwrap();
        assert!(!p.rewards.frog_songs && !p.rewards.frog_bug);
        let p = plan_of(&with(true, false)).unwrap();
        assert!(p.rewards.frog_songs && !p.rewards.frog_bug);
        let p = plan_of(&with(false, true)).unwrap();
        assert!(!p.rewards.frog_songs && p.rewards.frog_bug);
        let mut s: Settings = Settings::default();
        s.set("shuffle_rewards", Value::s("off")).unwrap();
        s.set(SONGS_SETTING, Value::Bool(true)).unwrap();
        s.set(BUG_SETTING, Value::Bool(true)).unwrap();
        let p = plan_of(&s).unwrap();
        assert!(!p.rewards.frog_songs && !p.rewards.frog_bug);
        let mut d: Settings = Settings::default();
        d.set(SONGS_SETTING, Value::Bool(false)).unwrap();
        d.set(BUG_SETTING, Value::Bool(false)).unwrap();
        assert_eq!(Settings::default().encode().unwrap(), d.encode().unwrap());
    }

    #[test]
    fn the_frog_rows_are_gated_flow_locations_that_open_only_with_their_toggle() {
        let d: Data = Data::load().unwrap();
        let off = plan_of(&Settings::default()).unwrap();
        let songs = plan_of(&with(true, false)).unwrap();
        let bug = plan_of(&with(false, true)).unwrap();
        for n in SONGS {
            let l: &Loc = d.flow.by_name(n).unwrap();
            assert!(l.gated && !l.supported && d.flow_ok(n), "{n}");
            assert_eq!(d.vanilla_of(n), Some("Rupees (50)"), "{n}");
            assert!(!crate::flow_extra::open(l, &off) && crate::flow_extra::open(l, &songs) && !crate::flow_extra::open(l, &bug), "{n}");
        }
        let l: &Loc = d.flow.by_name(BUG).unwrap();
        assert!(l.gated && !l.supported && d.flow_ok(BUG));
        assert_eq!(d.vanilla_of(BUG), Some("Piece of Heart"));
        assert!(!crate::flow_extra::open(l, &off) && !crate::flow_extra::open(l, &songs) && crate::flow_extra::open(l, &bug));
        assert!(!crate::flow_extra::bound(&d, "Rupees (50)") && !crate::flow_extra::bound(&d, "Piece of Heart"));
        assert!(d.flow.by_name("ZR Frogs Ocarina Game").is_some_and(|l| l.supported));
    }

    #[test]
    fn each_song_row_has_its_own_caller_and_the_bug_row_two_heart_piece_callers() {
        let d: FlowData = FlowData::load().unwrap();
        for (n, i) in SONGS.iter().zip([80usize, 81, 82, 83, 84]) {
            let ch: Vec<Change> = changes(d.by_name(n).unwrap(), "Item_Longshot").unwrap();
            assert_eq!(ch.len(), 1, "{n}");
            assert_eq!((ch[0].index, ch[0].file.as_str(), ch[0].role.as_str()), (i, FILE, ROLE), "{n}");
            assert!(!plain(&ch[0]));
        }
        let ch: Vec<Change> = changes(d.by_name(BUG).unwrap(), "Item_Longshot").unwrap();
        assert_eq!(ch.iter().map(|c| c.index).collect::<Vec<usize>>(), vec![71, 91]);
        assert!(ch.iter().all(|c| c.old == "Obj_Heart_Piece" && c.param == "ActorName"));
        assert!(changes(d.by_name("GV Cow").unwrap(), "Item_Longshot").is_err());
    }

    #[test]
    fn a_vanilla_rupee_at_a_song_still_rewires_the_song() {
        let d: FlowData = FlowData::load().unwrap();
        let cs: Vec<Choice> = vec![Choice { location: SONGS[0].to_string(), item: "Obj_Rupee_Purple".to_string() }];
        let (c, same, _) = crate::flow_locations::plan(&d, &cs, &crate::flow_locations::Opts::default()).unwrap();
        assert_eq!((c.len(), same), (5, 0));
        assert!(c.iter().all(|x| x.new == PURPLE));
        let cs: Vec<Choice> = vec![Choice { location: BUG.to_string(), item: "Obj_Heart_Piece".to_string() }];
        let (c, same, _) = crate::flow_locations::plan(&d, &cs, &crate::flow_locations::Opts::default()).unwrap();
        assert_eq!((c.len(), same), (0, 1));
    }

    #[test]
    fn the_frog_flags_are_persistent() {
        for f in SONG_FLAGS.iter().chain([&COUNT, &REPEL]) {
            let row: &str = FLAGS.lines().find(|l| l.contains(&format!("\"name\":\"{f}\""))).unwrap();
            assert!(row.contains("\"save\":0") && row.contains("\"reset\":8"), "{f}");
        }
    }

    #[test]
    fn toggles_off_place_no_frog_row_and_on_add_exactly_their_rows() {
        let d: Data = Data::load().unwrap();
        let off: Generated = generate_data(&d, &with(false, false), "frogs", &GenOpts::default()).unwrap();
        assert!(SONGS.iter().chain([&BUG]).all(|l| !off.seed_file.placements.contains_key(*l)));
        // Every item may go to a frog: its grant is the generic pouch get and a dungeon item there
        // credits its dungeon (crate::debug_names), so only crate::slice::generate::allowed judges it.
        let p: Plan = plan_of(&with(true, true)).unwrap();
        let w: World = crate::slice::logic::world_with(&d, &p.logic()).unwrap();
        for n in 0..2 {
            let g: Generated = generate_data(&d, &with(true, true), &format!("frogs {n}"), &GenOpts::default()).unwrap();
            assert_eq!(g.seed_file.placements.len(), off.seed_file.placements.len() + SONGS.len() + 1);
            for l in SONGS.iter().chain([&BUG]) {
                let it: &str = &g.seed_file.placements[*l];
                assert!(allowed(&d, &w, &p, w.item(it).unwrap(), w.loc(l).unwrap()) || Some(it) == d.vanilla_of(l), "{l} holds {it}");
            }
            let s: Generated = generate_data(&d, &with(true, false), &format!("frog songs {n}"), &GenOpts::default()).unwrap();
            assert_eq!(s.seed_file.placements.len(), off.seed_file.placements.len() + SONGS.len());
            assert!(!s.seed_file.placements.contains_key(BUG));
        }
    }

    #[test]
    fn a_built_mod_with_both_toggles_loads_every_flow_its_edited_events_call() {
        let Some(r) = root() else { return };
        let d: Data = Data::load().unwrap();
        let g: Generated = generate_data(&d, &with(true, true), "frogs build", &GenOpts::default()).unwrap();
        let load: PB = PB::from(env!("CARGO_MANIFEST_DIR")).join("../../target/frogs_mods");
        let _ = std::fs::remove_dir_all(&load);
        let b: crate::slice::edit::Built = crate::slice::edit::build_for(&g.seed_file, &r, &load, "ootrr-frogs", crate::modpack::Host::Eden).unwrap();
        let mut rf: Romfs = Romfs::open(&r).unwrap();
        let romfs: PB = b.root.join("romfs");
        assert!(romfs.join(crate::flow_locations::path_of(FILE)).is_file());
        assert!(crate::flow_extra::analyzed::worked(&mut rf, &romfs).unwrap().is_none(), "an edited frog event calls a flow its row does not load");
        let _ = std::fs::remove_dir_all(&load);
    }
}
