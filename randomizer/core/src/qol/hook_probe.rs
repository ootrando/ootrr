//! Probe mods for mimicking Hookshot storage in the player's AI graphs (no code patch).
//!
//! 1.1.1 fixed the storage glitch in code, so the 1.0 behaviour cannot come back. These probes
//! answer the one question a data-only mimic rests on: can a graph node move Link while the
//! hookshot action runs, and does that move ignore walls?
//!
//! `AI/Player.action.hookshotshoot.module.ainb` switches on the hook state from
//! QueryEquipmentUserTryGetHookshotHookState: 2 flying out, 3 hooked (the pull, which is
//! ExecutePlayerClimb with ClimbType 3), 4 coming back after a miss or a wall hit, 0 idle.
//! The state 4 branch is a SelectorGenericForceResult over the recoil animation pick. Every
//! probe but `Pull` puts a parallel under that selector, holding the animation pick and one
//! move node, so the forced result keeps the action ending as it does in the game.
//!
//! - `Force`: ExecuteActorForceSetPos to a point 3 m ahead of where Link stood when the hook
//!   started back. The point is read once (OutputOnlyOnEnter), so Link is held there.
//! - `Warp`: OneshotWarpToTargetPosAndYRot to the same point. It also turns Link to angle 0.
//! - `ToHook`: ExecuteActorForceSetPos to the hook itself plus 1 m ahead. The hook comes from
//!   QueryGetShootableActor by actor name (Shootable_Hook for the Hookshot, Shootable_HookLong
//!   for the Longshot, as the player's ShooterParam lists them with no key).
//! - `Pull`: the state 4 case runs the state 3 branch, so a hook that only hit a wall still
//!   asks for the pull.
//!
//! Player_Child and Player_Adult carry the same graph under the same path and the game runs
//! whichever it loaded first, so both packs get the same edit (see qol::bunny).

use std::fs;
use std::path::{Path as P, PathBuf as PB};

use crate::ainb::build::{Spec, QUERY};
use crate::ainb::{audit, Ainb};
use crate::error::{Error, Res};
use crate::qol::{bunny, edit_zs};
use crate::romfs::Romfs;
use crate::rsizetable::{self, Rst};
use crate::sarc;
use crate::staging::{self, Out};

pub const GRAPH: &str = "AI/Player.action.hookshotshoot.module.ainb";
pub const STATE: &str = "QueryEquipmentUserTryGetHookshotHookState";
pub const HOOKED: i32 = 3;
pub const RETURNING: i32 = 4;
pub const FORCE: &str = "ExecuteActorForceSetPos";
pub const WARP: &str = "OneshotWarpToTargetPosAndYRot";
pub const GET_HOOK: &str = "QueryGetShootableActor";
pub const HOOK: &str = "Shootable_Hook";
pub const HOOK_LONG: &str = "Shootable_HookLong";
/// How far ahead of Link the forward probes put him, and how far past the hook.
pub const AHEAD: f32 = 3.0;
pub const PAST_HOOK: f32 = 1.0;

const SWITCH: u16 = 1;
const PARALLEL: u16 = 3;
const FORCE_RESULT: &str = "SelectorGenericForceResult";
const LINK: &str = "game::actor::ActorLinkBase";
const S32: usize = 0;
const BOOL: usize = 1;
const F32: usize = 2;
const STR: usize = 3;
const VEC3: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Variant {
    Force,
    Warp,
    ToHook(&'static str),
    Pull,
}

pub const ALL: [(Variant, &str, &str); 5] = [
    (Variant::Force, "A1-force-forward", "A hook that misses or hits a wall moves Link 3 m ahead (force set position)."),
    (Variant::Warp, "A2-warp-forward", "Same as A1 with the one-shot warp node; Link also turns to a fixed direction."),
    (Variant::ToHook(HOOK), "B1-to-hook-hookshot", "Hookshot only: a hook that hits a wall moves Link to the hook plus 1 m."),
    (Variant::ToHook(HOOK_LONG), "B2-to-hook-longshot", "Longshot only: a hook that hits a wall moves Link to the hook plus 1 m."),
    (Variant::Pull, "C-pull-on-wall", "A hook that misses or hits a wall still starts the pull."),
];

fn flag(v: bool) -> Vec<u8> {
    (v as u32).to_le_bytes().to_vec()
}

fn int(v: i32) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

fn num(v: f32) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

/// A query with no inputs and one vec3 output, read once when its reader starts.
fn once<'a>(name: &'a str, seed: &'a str, out: &'a str) -> Spec<'a> {
    let mut s: Spec = Spec::node(name, seed).prop(BOOL, "OutputOnlyOnEnter", flag(true)).out(VEC3, out);
    s.role = QUERY;
    s
}

/// The case value a switch child plug carries in its extra bytes.
fn case_of(extra: &[u8]) -> Option<i32> {
    extra.get(4..8).map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// The plug position, in `plugs`, of the child of switch `sw` for case `v`.
fn case_plug(a: &Ainb, sw: usize, v: i32) -> Option<usize> {
    let p = &a.params[sw];
    let b: usize = p.pl[2][1] as usize;
    p.group(2).iter().position(|x| case_of(&x.extra) == Some(v)).map(|j| b + j)
}

/// The hook state switch whose case 4 runs the forced-result selector, and that selector.
pub fn site(a: &Ainb) -> Res<(usize, usize)> {
    let state: usize = match a.find_els(STATE).as_slice() {
        [one] => *one,
        _ => return Err(Error::Format("hook probe: the graph does not have exactly one hook state query")),
    };
    let mut hits: Vec<(usize, usize)> = Vec::new();
    for i in 0..a.els.len() {
        if a.el_type(i) != SWITCH || !a.find_in(i, S32, "Input").and_then(|g| a.input(S32, g)).is_some_and(|x| x.src == state as i16) {
            continue;
        }
        if let Some(at) = case_plug(a, i, RETURNING) {
            let kid: usize = a.params[i].plugs[at].a as usize;
            if a.el_type(kid) == 0 && a.el_name(kid) == FORCE_RESULT {
                hits.push((i, kid));
            }
        }
    }
    match hits.as_slice() {
        [one] => Ok(*one),
        _ => Err(Error::Format("hook probe: no single hook state switch with a forced-result case 4")),
    }
}

/// The node that moves Link, with the queries it reads.
fn mover(a: &mut Ainb, v: Variant) -> Res<usize> {
    let (s1, s2, s3, s4, s5, s6, s7) = ("ootrr hook probe pos", "ootrr hook probe forward", "ootrr hook probe step", "ootrr hook probe dest", "ootrr hook probe move", "ootrr hook probe hook", "ootrr hook probe hook pos");
    let fwd: usize = a.add_node(&once("QueryActorGetForwardDir", s2, "Forward"))?;
    let (from, far): (usize, f32) = match v {
        Variant::ToHook(actor) => {
            let name: Vec<u8> = a.pool.add(actor).to_le_bytes().to_vec();
            let hook: usize = a.add_node(&Spec::query(GET_HOOK, s6).constant(S32, "Index", int(0)).constant(STR, "ActorName", name).out_ptr("Actor", LINK))?;
            let mut at: Spec = Spec::node("QueryActorGetPosFromGameActorLink", s7).prop(BOOL, "OutputOnlyOnEnter", flag(true)).linked_ptr("ActorLink", LINK, hook).out(VEC3, "Position");
            at.role = QUERY;
            (a.add_node(&at)?, PAST_HOOK)
        }
        _ => (a.add_node(&once("QueryActorGetPosition", s1, "Position"))?, AHEAD),
    };
    let step: usize = a.add_node(&Spec::query("QueryMathMulF32AndVector3f", s3).constant(F32, "F32", num(far)).linked(VEC3, "Vector", fwd, 0).out(VEC3, "Vector"))?;
    let dest: usize = a.add_node(&Spec::query("QueryMathAddVector3f", s4).linked(VEC3, "InputA", from, 0).linked(VEC3, "InputB", step, 0).out(VEC3, "Result"))?;
    match v {
        Variant::Warp => a.add_node(&Spec::node(WARP, s5).prop(BOOL, "ExecuteEveryFrame", flag(false)).constant(F32, "AngleXZ", num(0.0)).linked(VEC3, "Position", dest, 0)),
        _ => a.add_node(&Spec::node(FORCE, s5).linked(VEC3, "Destination", dest, 0)),
    }
}

pub fn patch_graph(raw: &[u8], v: Variant) -> Res<Vec<u8>> {
    let mut a: Ainb = Ainb::parse(raw)?;
    let (sw, forced): (usize, usize) = site(&a)?;
    if v == Variant::Pull {
        let pull: usize = a.params[sw].plugs[case_plug(&a, sw, HOOKED).ok_or(Error::Format("hook probe: the hook state switch has no case 3"))?].a as usize;
        let at: usize = case_plug(&a, sw, RETURNING).ok_or(Error::Format("hook probe: the hook state switch has no case 4"))?;
        a.params[sw].plugs[at].a = pull as u32;
    } else {
        let at: usize = a.params[forced].pl[2][1] as usize;
        if a.params[forced].pl[2][0] != 1 {
            return Err(Error::Format("hook probe: the forced-result selector does not have one child"));
        }
        let pick: usize = a.params[forced].plugs[at].a as usize;
        if a.el_type(pick) == PARALLEL {
            return Err(Error::Format("hook probe: the graph is already patched"));
        }
        let mv: usize = mover(&mut a, v)?;
        let both: usize = a.add_node(&Spec::composite(PARALLEL, "ootrr hook probe parallel").prop(S32, "EndPolicy", int(0)).prop(S32, "ResultPolicy", int(0)).kid(pick, "").kid(mv, ""))?;
        a.params[forced].plugs[at].a = both as u32;
    }
    let bad: Vec<String> = audit(&a);
    if !bad.is_empty() {
        return Err(Error::Msg(format!("hook probe: {}", bad.join("; "))));
    }
    let out: Vec<u8> = a.write()?;
    let back: Ainb = Ainb::parse(&out)?;
    if back.write()? != out || back.params != a.params || back.ins != a.ins || back.outs != a.outs || back.qids != a.qids {
        return Err(Error::Format("hook probe: the written graph does not read back"));
    }
    Ok(out)
}

pub fn patch_pack(raw: &[u8], v: Variant) -> Res<Vec<u8>> {
    let mut es: Vec<sarc::Entry> = sarc::parse(raw)?;
    let g: Vec<u8> = sarc::get(&es, GRAPH).ok_or(Error::Msg(format!("hook probe: the player pack has no {GRAPH}")))?.data.clone();
    sarc::set(&mut es, GRAPH, patch_graph(&g, v)?);
    sarc::write(&es)
}

/// Both player packs with the probe, and the actor table with room for the added nodes.
/// `base` is the romfs folder of a mod to build on (a seed), so its own edits are kept.
pub fn stage(rf: &mut Romfs, v: Variant, base: Option<&P>) -> Res<Vec<Out>> {
    let have = |p: &str| -> Option<Vec<u8>> { base.and_then(|b| fs::read(p.split('/').fold(b.to_path_buf(), |a, s| a.join(s))).ok()) };
    let mut outs: Vec<Out> = Vec::new();
    for (file, _) in bunny::PACKS {
        outs.push(edit_zs(rf, &have, file, |raw| patch_pack(raw, v))?);
    }
    outs.push(edit_zs(rf, &have, bunny::ACTOR_INFO, bunny::grow_heap)?);
    Ok(outs)
}

/// Writes one folder per probe under `out`, each with a romfs holding the changed files and
/// the size table grown for them. Returns the files written.
pub fn probe(rf: &mut Romfs, out: &P, base: Option<&P>) -> Res<Vec<PB>> {
    let mut done: Vec<PB> = Vec::new();
    for (v, name, _) in ALL {
        let root: PB = out.join(name).join("romfs");
        let mut rst: Rst = match base.map(|b| rsizetable::TABLE_PATH.split('/').fold(b.to_path_buf(), |a, s| a.join(s))).filter(|p| p.is_file()) {
            Some(p) => Rst::parse(&rf.zs.decompress(&fs::read(p)?)?)?,
            None => rsizetable::load(rf)?,
        };
        let mut put = |path: &str, data: &[u8]| -> Res<()> {
            let f: PB = path.split('/').fold(root.clone(), |a, s| a.join(s));
            if let Some(d) = f.parent() {
                fs::create_dir_all(d)?;
            }
            fs::write(&f, data)?;
            done.push(f);
            Ok(())
        };
        for o in stage(rf, v, base)? {
            staging::grow_out(rf, &mut rst, &o)?;
            put(&o.path, &o.z)?;
        }
        put(rsizetable::TABLE_PATH, &rst.to_zs()?)?;
    }
    Ok(done)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn real() -> Option<Romfs> {
        let p: PB = ootrr_game::test_romfs::path();
        if p.is_dir() {
            Romfs::open(&p).ok()
        } else {
            eprintln!("skipped: no romfs at {}, set OOTRR_ROMFS", p.display());
            None
        }
    }

    fn graph(rf: &mut Romfs, file: &str) -> Vec<u8> {
        let es: Vec<sarc::Entry> = sarc::parse(&rf.read_zs(file).unwrap()).unwrap();
        sarc::get(&es, GRAPH).unwrap().data.clone()
    }

    #[test]
    fn case_values_sit_in_the_second_word() {
        assert_eq!(case_of(&[0, 0, 0, 0, 4, 0, 0, 0]), Some(4));
        assert_eq!(case_of(&[]), None);
    }

    #[test]
    fn every_probe_has_its_own_folder() {
        let mut names: Vec<&str> = ALL.iter().map(|x| x.1).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), ALL.len());
    }

    #[test]
    fn the_move_probes_hang_one_node_beside_the_recoil_pick() {
        let Some(mut rf) = real() else { return };
        let raw: Vec<u8> = graph(&mut rf, bunny::ADULT_FILE);
        assert_eq!(raw, graph(&mut rf, bunny::FILE), "both player packs carry the same graph");
        let before: Ainb = Ainb::parse(&raw).unwrap();
        let (sw, forced) = site(&before).unwrap();
        let pick: usize = before.params[forced].group(2)[0].a as usize;
        for (v, node, queries) in [(Variant::Force, FORCE, 4), (Variant::Warp, WARP, 4), (Variant::ToHook(HOOK), FORCE, 5), (Variant::ToHook(HOOK_LONG), FORCE, 5)] {
            let a: Ainb = Ainb::parse(&patch_graph(&raw, v).unwrap()).unwrap();
            assert!(audit(&a).is_empty());
            assert_eq!(a.els.len(), before.els.len() + queries + 2, "{v:?}: the queries, the move node and the parallel");
            assert_eq!(site(&a).unwrap(), (sw, forced), "{v:?}: the switch still reaches the selector");
            let both: usize = a.params[forced].group(2)[0].a as usize;
            assert_eq!(a.el_type(both), PARALLEL);
            let mv: usize = a.find_els(node)[0];
            assert_eq!(a.params[both].group(2).iter().map(|x| x.a as usize).collect::<Vec<_>>(), vec![pick, mv]);
            if let Variant::ToHook(actor) = v {
                assert_eq!(a.in_str(a.find_els(GET_HOOK)[0], "ActorName"), Some(actor));
            } else {
                assert!(a.find_els(GET_HOOK).is_empty());
            }
            assert!(patch_graph(&a.write().unwrap(), v).is_err(), "{v:?}: a second pass is refused");
        }
    }

    #[test]
    fn the_pull_probe_sends_case_4_to_the_pull_branch() {
        let Some(mut rf) = real() else { return };
        let raw: Vec<u8> = graph(&mut rf, bunny::ADULT_FILE);
        let before: Ainb = Ainb::parse(&raw).unwrap();
        let (sw, _) = site(&before).unwrap();
        let pull: u32 = before.params[sw].plugs[case_plug(&before, sw, HOOKED).unwrap()].a;
        let a: Ainb = Ainb::parse(&patch_graph(&raw, Variant::Pull).unwrap()).unwrap();
        assert_eq!(a.els.len(), before.els.len(), "no node is added");
        assert_eq!(a.params[sw].plugs[case_plug(&a, sw, RETURNING).unwrap()].a, pull);
        assert_eq!(a.params[sw].plugs[case_plug(&a, sw, HOOKED).unwrap()].a, pull);
        assert!(site(&a).is_err(), "case 4 no longer runs the recoil selector");
    }

    #[test]
    fn the_probe_folders_hold_both_packs_and_a_grown_size_table() {
        let Some(mut rf) = real() else { return };
        let dir: PB = std::env::temp_dir().join(format!("ootrr-hook-probe-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let files: Vec<PB> = probe(&mut rf, &dir, None).unwrap();
        assert_eq!(files.len(), ALL.len() * 4, "two packs, the actor table and the size table per probe");
        let game: Rst = rsizetable::load(&mut rf).unwrap();
        for (v, name, _) in ALL {
            let root: PB = dir.join(name).join("romfs");
            let rst: Rst = Rst::parse(&rf.zs.decompress(&fs::read(rsizetable::TABLE_PATH.split('/').fold(root.clone(), |a, s| a.join(s))).unwrap()).unwrap()).unwrap();
            let grew: bool = rst.get(GRAPH).unwrap() > game.get(GRAPH).unwrap();
            assert_eq!(grew, v != Variant::Pull, "{name}: the graph entry grows with the added nodes");
            for (file, _) in bunny::PACKS {
                let z: Vec<u8> = fs::read(file.split('/').fold(root.clone(), |a, s| a.join(s))).unwrap();
                let es: Vec<sarc::Entry> = sarc::parse(&rf.zs.decompress(&z).unwrap()).unwrap();
                let a: Ainb = Ainb::parse(&sarc::get(&es, GRAPH).unwrap().data).unwrap();
                assert!(audit(&a).is_empty(), "{name} {file}");
            }
        }
        let _ = fs::remove_dir_all(&dir);
    }
}
