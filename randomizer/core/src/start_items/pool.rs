use std::collections::BTreeSet as BS;

use crate::error::{Error as E, Res};
use crate::logic_items::ItemId;
use crate::rng::Rng;
use crate::settings::Settings;
use crate::slice::data::Data;
use crate::slice::support::{Keep, Plan, Songs};
use crate::slice::tier_order::{native, Give};
use crate::start_items::give::Take;
use crate::start_items::table::Strip;
use crate::world::{LocId, Placement, World};

pub const JUNK: &str = "Rupee (1)";
pub(crate) const PIECES: [&str; 3] = ["Piece of Heart", "Piece of Heart (Out of Logic)", "Piece of Heart (Treasure Chest Game)"];
pub(crate) const CONTAINER: &str = "Heart Container";
const RUPEES: [&str; 5] = [JUNK, "Rupees (5)", "Rupees (20)", "Rupees (50)", "Rupees (200)"];

pub fn heart_adjusted_item(item: &str) -> bool {
    PIECES.contains(&item) || item == CONTAINER || RUPEES.contains(&item)
}

pub fn adjust_hearts(w: &World, s: &Settings, seed: &str, base: &mut Placement, free: &[LocId]) -> Res<Option<String>> {
    let delta: i64 = s.int("starting_hearts")? - crate::start_items::give::VANILLA_HEARTS;
    if delta == 0 {
        return Ok(None);
    }
    let mode: &str = s.text("heart_pool")?;
    let mut rng: Rng = Rng::from_text(&format!("{seed} heart pool"));
    let mut pieces: Vec<LocId> = Vec::new();
    let mut containers: Vec<LocId> = Vec::new();
    let mut junk: Vec<LocId> = Vec::new();
    for &l in free {
        if let Some(i) = base.get(l) {
            let name: &str = &w.items.names[i];
            if PIECES.contains(&name) {
                pieces.push(l);
            } else if name == CONTAINER {
                containers.push(l);
            } else if RUPEES.contains(&name) {
                junk.push(l);
            }
        }
    }
    let n: usize = delta.unsigned_abs() as usize;
    let mut choices: Vec<(usize, usize)> = (0..=n)
        .map(|c| (c, n - c))
        .filter(|(c, p)| if delta > 0 { *c <= containers.len() && 4 * p <= pieces.len() } else { c + 4 * p <= junk.len() })
        .collect();
    if choices.is_empty() {
        return Err(E::Msg(format!(
            "starting hearts: cannot adjust the shuffled heart pool by {n} hearts ({} containers, {} pieces, {} rupees available). Shuffle more heart or rupee checks, or choose starting hearts closer to 3",
            containers.len(), pieces.len(), junk.len()
        )));
    }
    match mode {
        "pieces" => choices.sort_by_key(|(c, _)| *c),
        "containers" => choices.sort_by_key(|(c, _)| std::cmp::Reverse(*c)),
        "random" => rng.shuffle(&mut choices),
        _ => return Err(E::Msg(format!("heart_pool is {mode}, which is not a known choice"))),
    }
    let (c, p) = choices[0];
    if delta > 0 {
        let replacement: ItemId = w.item(JUNK).ok_or_else(|| E::Msg(format!("{JUNK} is not in the logic item list")))?;
        rng.shuffle(&mut pieces);
        rng.shuffle(&mut containers);
        for l in containers.into_iter().take(c).chain(pieces.into_iter().take(4 * p)) {
            base.put(l, replacement);
        }
    } else {
        let piece: ItemId = w.item(PIECES[0]).ok_or_else(|| E::Msg(format!("{} is not in the logic item list", PIECES[0])))?;
        let container: ItemId = w.item(CONTAINER).ok_or_else(|| E::Msg(format!("{CONTAINER} is not in the logic item list")))?;
        rng.shuffle(&mut junk);
        for &l in junk.iter().take(c) {
            base.put(l, container);
        }
        for &l in junk.iter().skip(c).take(4 * p) {
            base.put(l, piece);
        }
    }
    let verb: &str = if delta > 0 { "Removed" } else { "Added" };
    Ok(Some(format!("{verb} {c} Heart Containers and {} Pieces of Heart in the shuffled pool to compensate for starting hearts.", 4 * p)))
}

fn gone_kinds(p: &Plan) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    if p.keys.map == Keep::Start {
        out.extend(["Map", "Compass"]);
    }
    if p.keys.boss == Keep::Start {
        out.push("BossKey");
    }
    if p.keys.ganon == Keep::Start {
        out.push("GanonBossKey");
    }
    out
}

pub fn active(p: &Plan) -> bool {
    p.fire_loop_open() || p.open.beans || !gone_kinds(p).is_empty() || p.kit.take.iter().any(|t| t.strip != Strip::Keep && t.count > 0)
}

fn kind_of<'a>(d: &'a Data, w: &World, it: ItemId) -> &'a str {
    d.item(&w.items.names[it]).map(|i| i.kind.as_str()).unwrap_or("")
}

fn is_song(d: &Data, w: &World, it: ItemId) -> bool {
    d.item(&w.items.names[it]).and_then(|i| i.actor.as_deref()).is_some_and(crate::slice::generate::is_song)
}

fn held_tier(d: &Data, w: &World, it: ItemId, l: LocId) -> Option<usize> {
    let f: &str = d.family(&w.items.names[it])?;
    match native(d, f, &w.locs[l].name) {
        Give::Fixed(k) => Some(k),
        _ => None,
    }
}

struct Ctx<'a> {
    d: &'a Data,
    w: &'a World,
    p: &'a Plan,
    free: BS<LocId>,
    junk: ItemId,
}

fn strip_one(c: &Ctx, t: &Take, base: &mut Placement, notes: &mut Vec<String>) -> Res<()> {
    let (d, w, p, free, junk): (&Data, &World, &Plan, &BS<LocId>, ItemId) = (c.d, c.w, c.p, &c.free, c.junk);
    let it: ItemId = w.item(t.logic).ok_or_else(|| E::Msg(format!("{} is not in the logic item list", t.logic)))?;
    let mut left: u32 = t.count;
    if matches!(t.strip, Strip::Unique | Strip::Family) {
        for l in w.real.iter().copied().filter(|l| !free.contains(l)) {
            if left == 0 {
                break;
            }
            if base.get(l) != Some(it) {
                continue;
            }
            let tier: Option<usize> = if t.strip == Strip::Unique { Some(0) } else { held_tier(d, w, it, l) };
            if tier.is_some_and(|k| (k as u32) < t.count) {
                base.put(l, junk);
                left -= 1;
            }
        }
    }
    if p.songs == Songs::Own && is_song(d, w, it) {
        if left > 0 {
            notes.push(format!("{} stays among the song lessons because songs only shuffle among themselves, so it can be learned a second time.", t.logic));
        }
        return Ok(());
    }
    for l in free.iter().copied() {
        if left == 0 {
            break;
        }
        if base.get(l) == Some(it) {
            base.put(l, junk);
            left -= 1;
        }
    }
    Ok(())
}

pub fn fit_actors(d: &Data, item: &str, want: usize, acts: &mut Vec<String>) {
    if (item == JUNK || PIECES.contains(&item) || item == CONTAINER)
        && let Some(a) = d.item(item).and_then(|i| i.actor.clone())
    {
        while acts.len() < want {
            acts.push(a.clone());
        }
    }
    if acts.len() > want {
        let f: Option<&str> = d.family(item);
        acts.sort_by_key(|a| f.and_then(|f| crate::slice::tier_order::rank(f, a)).unwrap_or(0));
        acts.truncate(want);
    }
}

pub fn base(d: &Data, w: &World, p: &Plan, vanilla: &Placement, free: &[LocId]) -> Res<(Placement, Vec<String>)> {
    let mut out: Placement = vanilla.clone();
    let mut notes: Vec<String> = Vec::new();
    if !active(p) {
        return Ok((out, notes));
    }
    let junk: ItemId = w.item(JUNK).ok_or_else(|| E::Msg(format!("{JUNK} is not in the logic item list")))?;
    let set: BS<LocId> = free.iter().copied().collect();
    let kinds: Vec<&str> = gone_kinds(p);
    if !kinds.is_empty() {
        let mut n: usize = 0;
        for l in free {
            if out.get(*l).is_some_and(|i| kinds.contains(&kind_of(d, w, i))) {
                out.put(*l, junk);
                n += 1;
            }
        }
        if n > 0 {
            notes.push(format!("{n} removed dungeon items (Maps, Compasses or Boss Keys whose setting is Removed or Start with) were taken out of the shuffled locations and replaced by {JUNK}."));
        }
    }
    if p.fire_loop_open() {
        spare_fire_key(w, free, &mut out, junk, &mut notes);
    }
    if p.open.beans {
        let beans: Vec<ItemId> = crate::open_world::skips::BEAN_ITEMS.iter().filter_map(|n| w.item(n)).collect();
        let mut n: usize = 0;
        for l in free {
            if out.get(*l).is_some_and(|i| beans.contains(&i)) {
                out.put(*l, junk);
                n += 1;
            }
        }
        if n > 0 {
            notes.push(format!("The Magic Bean is not needed with planted beans, so it was taken out of the shuffled locations and replaced by {JUNK}."));
        }
    }
    let c: Ctx = Ctx { d, w, p, free: set, junk };
    for t in p.kit.take.iter().filter(|t| t.strip != Strip::Keep && t.count > 0) {
        strip_one(&c, t, &mut out, &mut notes)?;
    }
    Ok((out, notes))
}

pub const FIRE_NOTE: &str = "The Fire Temple Boss Key loop door starts unlocked, so the Hammer alone opens it, and one Fire Temple small key was taken out of the shuffled locations and replaced by Rupee (1).";

fn spare_fire_key(w: &World, free: &[LocId], out: &mut Placement, junk: ItemId, notes: &mut Vec<String>) {
    let Some(k) = w.item("Small Key (Fire Temple)") else {
        return;
    };
    let spare: Option<LocId> = w.loc(crate::slice::support::FIRE_SPARE_KEY).filter(|l| free.contains(l) && out.get(*l) == Some(k));
    let Some(l) = spare.or_else(|| free.iter().copied().find(|l| out.get(*l) == Some(k))) else {
        return;
    };
    out.put(l, junk);
    notes.push(FIRE_NOTE.to_string());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{Settings, Value};
    use crate::slice::fill::vanilla_place;
    use crate::slice::logic::world_with;
    use crate::slice::pick::{pick, Pick};
    use crate::slice::support::plan;

    fn with(pairs: &[(&str, Value)]) -> Settings {
        let mut s: Settings = Settings::default();
        for (k, v) in pairs {
            s.set(k, v.clone()).unwrap();
        }
        s
    }

    fn count(w: &World, pl: &Placement, ls: &[LocId], name: &str) -> usize {
        let it: ItemId = w.item(name).unwrap();
        ls.iter().filter(|l| pl.get(**l) == Some(it)).count()
    }

    fn setup(s: &Settings) -> (Data, World, Plan, Placement, Pick) {
        let d: Data = Data::load().unwrap();
        let p: Plan = plan(s).unwrap();
        let mut w: World = world_with(&d, &p.logic()).unwrap();
        crate::slice::starts::add(&d, &p, &mut w, "pool").unwrap();
        let v: Placement = vanilla_place(&w, &d).unwrap();
        let pk: Pick = pick(&d, &w, &p);
        (d, w, p, v, pk)
    }

    #[test]
    fn defaults_only_take_out_the_spare_fire_key() {
        let (d, w, p, v, pk) = setup(&Settings::default());
        assert!(active(&p));
        let (b, n): (Placement, Vec<String>) = base(&d, &w, &p, &v, &pk.free).unwrap();
        let changed: Vec<&str> = w.real.iter().filter(|l| b.get(**l) != v.get(**l)).map(|l| w.locs[*l].name.as_str()).collect();
        assert_eq!(changed, vec![crate::slice::support::FIRE_SPARE_KEY]);
        assert_eq!(b.get(w.loc(crate::slice::support::FIRE_SPARE_KEY).unwrap()), w.item(JUNK));
        assert_eq!(n, vec![FIRE_NOTE.to_string()]);
        let (d, w, p, v, pk) = setup(&with(&[("shuffle_smallkeys", Value::s("vanilla"))]));
        assert!(!active(&p));
        let (b, n): (Placement, Vec<String>) = base(&d, &w, &p, &v, &pk.free).unwrap();
        assert_eq!(b.item, v.item);
        assert!(n.is_empty());
    }

    #[test]
    fn heart_adjustments_use_whole_hearts_and_prefer_the_selected_type() {
        for (start, mode, containers, pieces) in [(1, "pieces", 0, 8), (2, "containers", 1, 0), (5, "pieces", 0, 8), (5, "containers", 2, 0)] {
            let s = with(&[("starting_hearts", Value::Int(start)), ("heart_pool", Value::s(mode))]);
            let (_, w, _, mut b, pk) = setup(&s);
            let before = b.clone();
            adjust_hearts(&w, &s, "hearts", &mut b, &pk.free).unwrap();
            let count_pieces = |pl: &Placement| PIECES.iter().map(|name| count(&w, pl, &pk.free, name)).sum::<usize>();
            let bc = count(&w, &before, &pk.free, CONTAINER);
            let ac = count(&w, &b, &pk.free, CONTAINER);
            let bp = count_pieces(&before);
            let ap = count_pieces(&b);
            assert_eq!(bc.abs_diff(ac), containers);
            assert_eq!(bp.abs_diff(ap), pieces);
            assert_eq!((ac as i64 - bc as i64) * 4 + ap as i64 - bp as i64, (3 - start) * 4);
        }
    }

    #[test]
    fn heart_checks_follow_shuffle_settings_without_changing_fixed_items() {
        let cases = [
            ("defaults", vec![], false, false, false, true),
            ("treasure", vec![("shuffle_treasure_reward", Value::Bool(true))], true, false, false, true),
            ("frog bug", vec![("shuffle_frog_bug_heart", Value::Bool(true))], false, true, false, true),
            ("crater scrub", vec![("shuffle_scrubs_a", Value::Bool(true))], false, false, true, true),
            ("optional checks", vec![("shuffle_treasure_reward", Value::Bool(true)), ("shuffle_frog_bug_heart", Value::Bool(true)), ("shuffle_scrubs_a", Value::Bool(true))], true, true, true, true),
            ("frog songs", vec![("shuffle_frog_songs", Value::Bool(true))], false, false, false, true),
            ("rewards off", vec![("shuffle_rewards", Value::s("off")), ("shuffle_frog_bug_heart", Value::Bool(true)), ("shuffle_scrubs_a", Value::Bool(true))], false, false, false, false),
            ("freestanding dungeons", vec![("shuffle_freestanding_items", Value::s("dungeons"))], false, false, false, true),
            ("freestanding overworld", vec![("shuffle_freestanding_items", Value::s("overworld"))], false, false, false, true),
            ("freestanding all", vec![("shuffle_freestanding_items", Value::s("all"))], false, false, false, true),
        ];
        for (tag, settings, treasure, bug, crater, rewards) in cases {
            for start in [1, 3, 5] {
                let mut s = with(&settings);
                s.set("starting_hearts", Value::Int(start)).unwrap();
                s.set("heart_pool", Value::s("pieces")).unwrap();
                let (_, w, _, mut b, pk) = setup(&s);
                let before = b.clone();
                for (name, selected) in [
                    ("Market Treasure Chest Game Reward", treasure),
                    ("ZR Frogs Bug Game", bug),
                    ("ZR Frogs Ocarina Game", rewards),
                    ("DMC Deku Scrub", crater),
                    ("HF Deku Scrub Grotto", rewards),
                    ("LLR Freestanding PoH", true),
                    ("Kak Windmill Freestanding PoH", true),
                    ("HF Tektite Grotto Freestanding PoH", true),
                ] {
                    let l = w.loc(name).unwrap();
                    assert_eq!(pk.free.contains(&l), selected, "{tag}: {name}");
                    assert!(PIECES.contains(&w.items.names[before.get(l).unwrap()].as_str()), "{tag}: {name}");
                }
                for name in crate::flow_extra::frogs::SONGS {
                    let l = w.loc(name).unwrap();
                    assert_eq!(pk.free.contains(&l), tag == "frog songs", "{tag}: {name}");
                    assert!(!PIECES.contains(&w.items.names[before.get(l).unwrap()].as_str()), "{tag}: {name}");
                }
                adjust_hearts(&w, &s, tag, &mut b, &pk.free).unwrap();
                for &l in &w.real {
                    if !pk.free.contains(&l) {
                        assert_eq!(b.get(l), before.get(l), "{tag}: {}", w.locs[l].name);
                    }
                }
                let pieces = |pl: &Placement| PIECES.iter().map(|name| count(&w, pl, &pk.free, name) as i64).sum::<i64>();
                assert_eq!(pieces(&b) - pieces(&before), 4 * (3 - start), "{tag}");
            }
        }
    }

    #[test]
    fn newly_added_heart_actors_match_the_item() {
        let d = Data::load().unwrap();
        for (item, expected) in [(JUNK, "TBox_Rupee_Green"), ("Piece of Heart", "Obj_Heart_Piece"), ("Heart Container", "Obj_Heart_Container")] {
            let mut actors = Vec::new();
            fit_actors(&d, item, 1, &mut actors);
            assert_eq!(actors, vec![expected], "{item}");
        }
    }

    #[test]
    fn added_hearts_get_the_correct_actor_in_seed_build_targets() {
        let d = Data::load().unwrap();
        let s = with(&[("starting_hearts", Value::Int(1))]);
        let loc = "KF Midos Top Left Chest";
        assert!(RUPEES.contains(&d.pool_item(loc).unwrap()));
        for (item, expected) in [("Piece of Heart", "Obj_Heart_Piece"), ("Heart Container", "Obj_Heart_Container")] {
            let placements = [(loc.to_string(), item.to_string())].into_iter().collect();
            let seed = crate::settings::seed::SeedFile::new("added hearts", &s, "game", placements).unwrap();
            let actors = crate::slice::edit::actors_for(&d, &seed).unwrap();
            assert_eq!(actors.get(loc).map(String::as_str), Some(expected), "{item}");
        }
    }

    #[test]
    fn generated_seeds_respect_optional_heart_check_settings() {
        let d = Data::load().unwrap();
        for enabled in [false, true] {
            for start in [1, 5] {
                let s = with(&[
                    ("starting_hearts", Value::Int(start)),
                    ("heart_pool", Value::s("pieces")),
                    ("shuffle_treasure_reward", Value::Bool(enabled)),
                    ("shuffle_frog_songs", Value::Bool(enabled)),
                    ("shuffle_frog_bug_heart", Value::Bool(enabled)),
                    ("shuffle_scrubs_a", Value::Bool(enabled)),
                ]);
                let g = beatable(&d, &s, &format!("heart checks {enabled} {start}"));
                for loc in ["Market Treasure Chest Game Reward", "ZR Frogs Bug Game", "DMC Deku Scrub"] {
                    assert_eq!(g.seed_file.placements.contains_key(loc), enabled, "{loc}");
                }
                for loc in ["ZR Frogs Ocarina Game", "HF Deku Scrub Grotto", "LLR Freestanding PoH"] {
                    assert!(g.seed_file.placements.contains_key(loc), "{loc}");
                }
                let (w, placed) = crate::slice::starts::rebuild(&d, &g.seed_file).unwrap();
                let vanilla = vanilla_place(&w, &d).unwrap();
                let pieces = |pl: &Placement| PIECES.iter().map(|name| count(&w, pl, &w.real, name) as i64).sum::<i64>();
                assert_eq!(pieces(&placed) - pieces(&vanilla), 4 * (3 - start));
                let build_data = crate::slice::treasure::with(d.clone(), enabled);
                crate::slice::chest_targets::targets(&build_data, &g.seed_file).unwrap();
            }
        }
    }

    #[test]
    fn heart_adjustment_falls_back_and_reports_insufficient_pool() {
        let s = with(&[("starting_hearts", Value::Int(12)), ("heart_pool", Value::s("containers"))]);
        let (_, w, _, mut b, pk) = setup(&s);
        let before = b.clone();
        adjust_hearts(&w, &s, "fallback", &mut b, &pk.free).unwrap();
        assert_eq!(count(&w, &before, &pk.free, CONTAINER) - count(&w, &b, &pk.free, CONTAINER), 8);
        assert_eq!(count(&w, &before, &pk.free, PIECES[0]) - count(&w, &b, &pk.free, PIECES[0]), 4);

        let mut no_room = before.clone();
        let error = adjust_hearts(&w, &s, "fallback", &mut no_room, &[]).unwrap_err();
        assert!(error.to_string().contains("Shuffle more heart or rupee checks"));
        assert_eq!(no_room.item, before.item);
    }

    #[test]
    fn randomized_heart_adjustment_is_seeded_and_preserves_the_total() {
        let s = with(&[("starting_hearts", Value::Int(7)), ("heart_pool", Value::s("random"))]);
        let (_, w, _, b, pk) = setup(&s);
        let mut outcomes = BS::new();
        for n in 0..20 {
            let mut first = b.clone();
            let mut second = b.clone();
            let seed = format!("heart random {n}");
            adjust_hearts(&w, &s, &seed, &mut first, &pk.free).unwrap();
            adjust_hearts(&w, &s, &seed, &mut second, &pk.free).unwrap();
            assert_eq!(first.item, second.item);
            let removed_containers = count(&w, &b, &pk.free, CONTAINER) - count(&w, &first, &pk.free, CONTAINER);
            let removed_pieces: usize = PIECES.iter().map(|name| count(&w, &b, &pk.free, name) - count(&w, &first, &pk.free, name)).sum();
            assert_eq!(removed_pieces % 4, 0);
            assert_eq!(removed_containers * 4 + removed_pieces, 16);
            outcomes.insert(removed_containers);
        }
        assert!(outcomes.len() > 1);
    }

    #[test]
    fn every_starting_value_adjusts_exactly_or_reports_a_shortage() {
        for start in 1..=20 {
            for mode in ["pieces", "containers", "random"] {
                let s = with(&[("starting_hearts", Value::Int(start)), ("heart_pool", Value::s(mode))]);
                let (_, w, _, mut b, pk) = setup(&s);
                let before = b.clone();
                let weight = |pl: &Placement| -> i64 {
                    4 * count(&w, pl, &pk.free, CONTAINER) as i64
                        + PIECES.iter().map(|name| count(&w, pl, &pk.free, name) as i64).sum::<i64>()
                };
                match adjust_hearts(&w, &s, "range", &mut b, &pk.free) {
                    Ok(_) => assert_eq!(weight(&b) - weight(&before), 4 * (3 - start), "{start} {mode}"),
                    Err(e) => {
                        assert!(start > 3, "{start} {mode}: {e}");
                        assert!(e.to_string().contains("cannot adjust the shuffled heart pool"), "{start} {mode}: {e}");
                        assert_eq!(b.item, before.item);
                    }
                }
            }
        }
    }

    #[test]
    fn twenty_starting_hearts_reports_unshuffled_heart_shortage() {
        let s = with(&[("starting_hearts", Value::Int(20))]);
        let d = Data::load().unwrap();
        let error = crate::slice::generate::generate_data(&d, &s, "twenty hearts", &crate::slice::generate::GenOpts::default()).unwrap_err();
        assert!(error.to_string().contains("cannot adjust the shuffled heart pool by 17 hearts"));
    }

    #[test]
    fn generated_heart_adjustments_build_targets_and_rebuild() {
        let d = Data::load().unwrap();
        for (start, mode) in [(1, "pieces"), (1, "containers"), (7, "random"), (12, "containers")] {
            let s = with(&[("starting_hearts", Value::Int(start)), ("heart_pool", Value::s(mode))]);
            let g = beatable(&d, &s, &format!("heart generation {start} {mode}"));
            let (w, placed) = crate::slice::starts::rebuild(&d, &g.seed_file).unwrap();
            let vanilla = vanilla_place(&w, &d).unwrap();
            let weight = |pl: &Placement| -> i64 {
                4 * count(&w, pl, &w.real, CONTAINER) as i64
                    + PIECES.iter().map(|name| count(&w, pl, &w.real, name) as i64).sum::<i64>()
            };
            assert_eq!(weight(&placed) - weight(&vanilla), 4 * (3 - start));
            let actors = crate::slice::edit::actors_for(&d, &g.seed_file).unwrap();
            for (loc, actor) in actors {
                match g.seed_file.placements[&loc].as_str() {
                    "Piece of Heart" | "Piece of Heart (Out of Logic)" | "Piece of Heart (Treasure Chest Game)" => assert_eq!(actor, "Obj_Heart_Piece", "{loc}"),
                    "Heart Container" => assert_eq!(actor, "Obj_Heart_Container", "{loc}"),
                    _ => {}
                }
            }
            crate::slice::chest_targets::targets(&d, &g.seed_file).unwrap();
            assert!(g.report.contains("to compensate for starting hearts"));
        }
        let s = with(&[
            ("starting_hearts", Value::Int(1)),
            ("heart_pool", Value::s("pieces")),
            ("shuffle_pots", Value::s("all")),
            ("shuffle_shops", Value::s("deals_1")),
        ]);
        let g = beatable(&d, &s, "heart pots and shops");
        crate::slice::chest_targets::targets(&d, &g.seed_file).unwrap();
    }

    #[test]
    fn removed_maps_and_boss_keys_leave_the_shuffled_locations() {
        let s: Settings = with(&[("shuffle_mapcompass", Value::s("remove")), ("shuffle_bosskeys", Value::s("remove")), ("shuffle_ganon_bosskey", Value::s("remove")), ("shuffle_smallkeys", Value::s("vanilla"))]);
        let (d, w, p, v, pk) = setup(&s);
        let (b, n): (Placement, Vec<String>) = base(&d, &w, &p, &v, &pk.free).unwrap();
        let gone = |pl: &Placement| pk.free.iter().filter(|l| pl.get(**l).is_some_and(|i| ["Map", "Compass", "BossKey", "GanonBossKey"].contains(&kind_of(&d, &w, i)))).count();
        assert!(gone(&v) > 20);
        assert_eq!(gone(&b), 0);
        assert_eq!(count(&w, &b, &pk.free, JUNK), count(&w, &v, &pk.free, JUNK) + gone(&v));
        assert_eq!(n.len(), 1);
    }

    #[test]
    fn a_starting_item_loses_one_pool_copy_and_tiers_lose_as_many() {
        let s: Settings = with(&[("starting_items", Value::set(&["hookshot", "boomerang", "bow"]))]);
        let (d, w, p, v, pk) = setup(&s);
        let (b, _): (Placement, Vec<String>) = base(&d, &w, &p, &v, &pk.free).unwrap();
        let all: Vec<LocId> = w.real.clone();
        assert_eq!(count(&w, &b, &all, "Boomerang") + 1, count(&w, &v, &all, "Boomerang"));
        assert_eq!(count(&w, &b, &all, "Progressive Hookshot") + 1, count(&w, &v, &all, "Progressive Hookshot"));
        assert_eq!(count(&w, &b, &all, "Bow") + 1, count(&w, &v, &all, "Bow"));
        let l: Settings = with(&[("starting_items", Value::set(&["longshot"]))]);
        let (d, w, p, v, pk) = setup(&l);
        let (b, _): (Placement, Vec<String>) = base(&d, &w, &p, &v, &pk.free).unwrap();
        assert_eq!(count(&w, &b, &w.real, "Progressive Hookshot"), count(&w, &v, &w.real, "Progressive Hookshot") - 2);
    }

    fn beatable(d: &Data, s: &Settings, seed: &str) -> crate::slice::generate::Generated {
        use crate::slice::generate::{generate_data, GenOpts};
        let g = generate_data(d, s, seed, &GenOpts::default()).unwrap_or_else(|e| panic!("{seed}: {e}"));
        let (w, pl): (World, Placement) = crate::slice::starts::rebuild(d, &g.seed_file).unwrap();
        assert!(crate::search::explore(&w, &pl, &[]).won(), "{seed} is not beatable");
        g
    }

    #[test]
    fn seeds_with_many_starting_items_generate_and_stay_beatable() {
        let d: Data = Data::load().unwrap();
        let spread: Vec<&str> = vec!["kokiri_sword", "big_bomb_bag", "longshot", "double_magic", "bottle", "iron_boots", "ocarina", "biggest_quiver", "golden_scale", "silver_gauntlets", "giant_wallet", "lens_of_truth", "deku_nuts", "big_stick_bag", "bombchus", "magic_beans", "gerudo_card"];
        let cases: Vec<(&str, Settings)> = vec![
            ("spread", with(&[("starting_items", Value::set(&spread)), ("starting_songs", Value::set(&["song_of_time", "zeldas_lullaby"])), ("starting_hearts", Value::Int(6))])),
            ("all", with(&[("starting_items", Value::set(&crate::start_items::table::OPTS.iter().map(|o| o.id).collect::<Vec<&str>>())), ("starting_songs", Value::set(&crate::start_items::table::SONGS.iter().map(|o| o.0).collect::<Vec<&str>>()))])),
            ("songs anywhere", with(&[("starting_items", Value::set(&["hookshot", "bow"])), ("starting_songs", Value::set(&["eponas_song"])), ("shuffle_songs", Value::s("anywhere"))])),
            ("songs own", with(&[("starting_songs", Value::set(&["sarias_song", "suns_song"])), ("shuffle_songs", Value::s("own"))])),
            ("chests off", with(&[("starting_items", Value::set(&["hookshot", "boomerang", "magic_meter"])), ("shuffle_chests", Value::s("off"))])),
        ];
        for (tag, s) in &cases {
            let p: Plan = plan(s).unwrap();
            for n in 0..3 {
                let g = beatable(&d, s, &format!("start {tag} {n}"));
                let w: World = world_with(&d, &p.logic()).unwrap();
                for (loc, item) in &g.seed_file.placements {
                    assert!(w.loc(loc).is_some() && w.item(item).is_some(), "{loc} {item}");
                }
                if *tag == "all" {
                    for x in ["Progressive Hookshot", "Boomerang", "Megaton Hammer", "Lens of Truth", "Bomb Bag", "Bow", "Progressive Strength Upgrade"] {
                        assert!(!g.seed_file.placements.values().any(|v| v == x), "{tag} {n} still places {x}");
                    }
                }
            }
        }
    }

    #[test]
    fn removed_dungeon_items_never_show_up_in_a_seed() {
        let d: Data = Data::load().unwrap();
        for v in ["remove", "startwith"] {
            let s: Settings = with(&[("shuffle_mapcompass", Value::s(v)), ("shuffle_bosskeys", Value::s("remove")), ("shuffle_ganon_bosskey", Value::s("remove"))]);
            for n in 0..3 {
                let g = beatable(&d, &s, &format!("gone {v} {n}"));
                for (loc, item) in &g.seed_file.placements {
                    let k: &str = d.item(item).map(|i| i.kind.as_str()).unwrap_or("");
                    assert!(!["Map", "Compass", "BossKey", "GanonBossKey"].contains(&k), "{v} {n} {loc} {item}");
                }
                assert!(g.report.contains("replaced by Rupee (1)"));
            }
        }
    }

    #[test]
    fn an_unshuffled_duplicate_is_junk_for_the_logic_and_a_shuffled_higher_tier_stays() {
        let s: Settings = with(&[("starting_items", Value::set(&["hookshot", "boomerang"])), ("shuffle_chests", Value::s("off")), ("shuffle_rewards", Value::s("off"))]);
        let (d, w, p, v, pk) = setup(&s);
        let (b, _): (Placement, Vec<String>) = base(&d, &w, &p, &v, &pk.free).unwrap();
        assert_eq!(count(&w, &b, &w.real, "Boomerang"), 0);
        let hook: LocId = w.loc("Graveyard Dampe Race Hookshot Chest").unwrap();
        let long: LocId = w.loc("Water Temple Longshot Chest").unwrap();
        assert_eq!(b.get(hook), w.item(JUNK));
        assert_eq!(b.get(long), w.item("Progressive Hookshot"));
    }
}
