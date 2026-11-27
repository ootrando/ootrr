use std::collections::BTreeMap as BM;

use crate::bfevfl::{Bf, Flow};
use crate::error::{Error as E, Res};
use crate::flow_locations::build::{plan, Change, Choice, Opts, Stage, StagedFile, FRAME_DICT};
use crate::flow_locations::data::{path_of, Data};
use crate::flow_locations::edit::{set_param, Req};
use crate::flow_extra::{apply_op, ops_for, Op};
use crate::progressive::Grant;
use crate::romfs::Romfs;
use crate::zs;

#[derive(Default)]
struct Work<'a> {
    reqs: Vec<Req>,
    ops: Vec<Op>,
    grants: Vec<&'a Grant>,
    cow_grants: Vec<&'a crate::flow_extra::cows::ProgressiveGrant>,
    ammo: Vec<(usize, String, String)>,
    bags: Vec<(usize, String)>,
}

/// The roles whose change hands the placed item to the player.
const GIVES: [&str; 2] = ["grant", "give"];

pub fn stage_merged(rf: &mut Romfs, d: &Data, cs: &[Choice], gs: &[Grant], cows: &[crate::flow_extra::cows::ProgressiveGrant], o: &Opts) -> Res<Stage> {
    let (changes, same, untested): (Vec<Change>, usize, Vec<String>) = plan(d, cs, o)?;
    let mut by: BM<String, Work> = BM::new();
    for c in changes.iter().filter(|c| crate::flow_extra::trade::plain(c)) {
        by.entry(c.file.clone()).or_default().reqs.push(Req {
            index: c.index,
            event: c.event.clone(),
            param: c.param.clone(),
            old: c.old.clone(),
            new: c.new.clone(),
        });
        if GIVES.contains(&c.role.as_str()) && crate::progressive::guarded(&c.new) && !d.by_name(&c.location).is_some_and(crate::scrubs_b::is_b) && !crate::scrubs_b::flow::is_sale(c.index, &c.event) {
            by.entry(c.file.clone()).or_default().ammo.push((c.index, c.event.clone(), c.param.clone()));
        }
        if GIVES.contains(&c.role.as_str()) && crate::starter_bags::bag_for(&c.new).is_some() {
            by.entry(c.file.clone()).or_default().bags.push((c.index, c.new.clone()));
        }
    }
    for (file, os) in ops_for(&changes)? {
        by.entry(file).or_default().ops.extend(os);
    }
    for g in gs {
        by.entry(g.file.clone()).or_default().grants.push(g);
    }
    for g in cows {
        by.entry(g.op.file.clone()).or_default().cow_grants.push(g);
    }
    let mut files: Vec<StagedFile> = Vec::new();
    for (file, w) in &by {
        let path: String = path_of(file);
        let (raw, id): (Vec<u8>, Option<u32>) = crate::cow_clone::read(rf, file)?;
        let mut b: Bf = Bf::parse(&raw)?;
        for r in &w.reqs {
            set_param(&mut b, r)?;
        }
        for x in &w.ops {
            apply_op(&mut b, x).map_err(|e| E::Msg(format!("{} edit on {}: {e}", x.kind(), x.file)))?;
        }
        for g in &w.cow_grants {
            crate::flow_extra::progressive_once(&mut b, &g.op, g.family, &g.forms)
                .map_err(|e| E::Msg(format!("progressive cow edit on {}: {e}", g.op.file)))?;
        }
        for g in &w.grants {
            let f: &mut Flow = b.flows.first_mut().ok_or(E::Format("flow_locations: the file holds no flow"))?;
            g.apply(f)?;
        }
        // Ammo that needs a bag gives a green rupee to a player without one (progressive::ammo_guard).
        for (index, event, param) in &w.ammo {
            let f: &mut Flow = b.flows.first_mut().ok_or(E::Format("flow_locations: the file holds no flow"))?;
            crate::progressive::ammo_guard(f, *index, event, param)?;
        }
        // A plain give of the Bow or Slingshot then adds its starting bag (crate::starter_bags).
        for (index, tier) in &w.bags {
            let f: &mut Flow = b.flows.first_mut().ok_or(E::Format("flow_locations: the file holds no flow"))?;
            crate::starter_bags::follow(f, *index, tier)?;
        }
        let out: Vec<u8> = b.write()?;
        if Bf::parse(&out)? != b {
            return Err(E::Format("flow_locations: the written flow does not parse back to the edited model"));
        }
        let dict: Option<u32> = if w.reqs.is_empty() && w.ops.is_empty() && w.cow_grants.is_empty() && w.ammo.is_empty() && w.bags.is_empty() { id } else { Some(id.unwrap_or(FRAME_DICT)) };
        files.push(StagedFile { path, z: zs::compress_raw(&out, dict)?, old: raw.len(), new: out.len() });
    }
    Ok(Stage { files, changes, same, untested })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bfevfl::{Actor, Cont, Entry, Event, Kind, Val};
    use crate::progressive::wrap_grant;

    fn sub(n: &str, item: &str) -> Event {
        Event {
            name: n.to_string(),
            kind: Kind::Sub {
                next: 0xFFFF,
                params: Some(Cont { items: vec![("ActorName".to_string(), Val::Str(item.to_string()))] }),
                flow: "Dm_SY_GetInEvent".to_string(),
                entry: "GetInEvent_NotTurn".to_string(),
            },
        }
    }

    fn sample() -> Bf {
        Bf {
            ver: [0, 4, 0, 0],
            bom: 0xFEFF,
            shift: 3,
            pad: 0,
            name: "Test".to_string(),
            flows: vec![Flow {
                name: "Test".to_string(),
                actors: vec![Actor {
                    name: "GameSystemActor".to_string(),
                    sub: String::new(),
                    arg: String::new(),
                    actions: Vec::new(),
                    queries: Vec::new(),
                    params: None,
                    ep: 0xFFFF,
                    cut: 1,
                }],
                events: vec![sub("Event0", "Item_Wallet_Medium"), sub("Event1", "Obj_Heart_Piece")],
                entries: vec![Entry { name: "Start".to_string(), subs: vec![0, 1], vars: Vec::new(), main: 0, pad: 0 }],
            }],
            tls: Vec::new(),
        }
    }

    #[test]
    fn a_parameter_edit_and_a_wrap_share_one_pass_and_both_survive() {
        let raw: Vec<u8> = sample().write().unwrap();
        let mut b: Bf = Bf::parse(&raw).unwrap();
        set_param(&mut b, &Req { index: 1, event: "Event1".to_string(), param: "ActorName".to_string(), old: "Obj_Heart_Piece".to_string(), new: "Item_Longshot".to_string() }).unwrap();
        wrap_grant(&mut b.flows[0], 0, "Event0", "ActorName", "Wallet").unwrap();
        let back: Bf = Bf::parse(&b.write().unwrap()).unwrap();
        assert_eq!(back, b);
        assert!(matches!(back.flows[0].events[0].kind, Kind::Switch { .. }));
        let p: Option<Val> = back.flows[0].events[1].kind.params().and_then(|c| c.get("ActorName").cloned());
        assert_eq!(p, Some(Val::Str("Item_Longshot".to_string())));
    }

    #[test]
    fn progressive_cow_grants_are_staged_without_replacing_the_milk() {
        let r = ootrr_game::test_romfs::path();
        if !r.join("Pack").join("ZsDic.pack.zs").is_file() {
            eprintln!("skipped: no romfs at {}, set OOTRR_ROMFS", r.display());
            return;
        }
        let d = crate::slice::data::Data::load().unwrap();
        let placed: BM<String, &'static str> = crate::flow_extra::cows::LOCS.iter()
            .chain(crate::flow_extra::cows::OTHER.iter())
            .filter(|name| **name != "GF Cow")
            .map(|name| (name.to_string(), "BombBag"))
            .collect();
        let cows = crate::flow_extra::cows::progressive_grants(&d, &placed).unwrap();
        let cs = vec![Choice { location: "GF Cow".to_string(), item: "Item_HoverBoot".to_string() }];
        let mut rf = Romfs::open(&r).unwrap();
        let stage = stage_merged(&mut rf, &d.flow, &cs, &[], &cows, &Opts { untested: true, ..Opts::default() }).unwrap();
        for loc in placed.keys() {
            let l = d.flow.by_name(loc).unwrap();
            let e = &l.edits[0];
            let original = Bf::parse(&crate::cow_clone::read(&mut rf, &e.file).unwrap().0).unwrap();
            let path = path_of(&e.file);
            let staged = stage.files.iter().find(|f| f.path == path).unwrap();
            let changed = Bf::parse(&rf.zs.decompress(&staged.z).unwrap()).unwrap();
            let f = &changed.flows[0];
            assert_eq!(f.events[e.index], original.flows[0].events[e.index], "{loc} milk");
            let flag = crate::flow_extra::cows::flag_of(loc).unwrap();
            assert!(f.events.iter().any(|e| e.kind.params().and_then(|p| p.get("GameDataName")) == Some(&Val::Str(flag.clone()))), "{loc} gate");
            for tier in crate::progressive::tiers("BombBag").unwrap() {
                assert!(f.events.iter().any(|e| matches!(&e.kind, Kind::Sub { flow, entry, params, .. }
                    if flow == "Dm_SY_GetInEvent" && entry == "GetInEvent_NotTurn_NotNpc"
                    && params.as_ref().and_then(|p| p.get("ActorName")) == Some(&Val::Str(tier.to_string())))), "{loc} {tier}");
            }
        }
    }

    #[test]
    fn gated_ops_param_edits_and_wraps_share_the_real_files() {
        use std::path::PathBuf as PB;
        let r: PB = ootrr_game::test_romfs::path();
        if !r.join("Event/EventFlow").is_dir() {
            eprintln!("skipped: no romfs at {}, set OOTRR_ROMFS", r.display());
            return;
        }
        let sd: crate::slice::data::Data = crate::slice::data::Data::load().unwrap();
        let mut rf: Romfs = Romfs::open(&r).unwrap();
        let cs: Vec<Choice> = ["Kak 20 Gold Skulltula Reward", "DMT Biggoron", "GC Medigoron", "Kak 30 Gold Skulltula Reward"]
            .iter()
            .map(|n| Choice { location: n.to_string(), item: "Item_Longshot".to_string() })
            .collect();
        let o: Opts = Opts::default();
        let plain: Stage = crate::flow_locations::stage(&mut rf, &sd.flow, &cs, &o).unwrap();
        let merged: Stage = stage_merged(&mut rf, &sd.flow, &cs, &[], &[], &o).unwrap();
        assert_eq!(plain.files.len(), merged.files.len());
        for (a, b) in plain.files.iter().zip(&merged.files) {
            assert_eq!(a.path, b.path);
            assert_eq!(rf.zs.decompress(&a.z).unwrap(), rf.zs.decompress(&b.z).unwrap(), "{}", a.path);
        }
        let fam: &'static str = sd.family("Progressive Wallet").unwrap();
        let gs: Vec<Grant> = sd.grants(fam).into_iter().filter(|g| g.file == "GatheringTokenGold.bfevfl").collect();
        assert!(!gs.is_empty());
        let both: Stage = stage_merged(&mut rf, &sd.flow, &cs, &gs, &[], &o).unwrap();
        assert_eq!(both.files.len(), merged.files.len());
        let at: usize = both.files.iter().position(|f| f.path.contains("GatheringTokenGold")).unwrap();
        let raw: Vec<u8> = rf.zs.decompress(&both.files[at].z).unwrap();
        let b: Bf = Bf::parse(&raw).unwrap();
        let before: Bf = Bf::parse(&rf.zs.decompress(&merged.files[at].z).unwrap()).unwrap();
        assert!(b.flows[0].events.len() > before.flows[0].events.len());
        for (i, e) in before.flows[0].events.iter().enumerate() {
            if gs.iter().any(|g| g.index == i) {
                continue;
            }
            assert_eq!(&b.flows[0].events[i], e, "event {i}");
        }
    }
}
