use crate::ainb::build::Spec;
use crate::ainb::{audit, Ainb};
use crate::error::{Error as E, Res};

pub const DONOR: &str = "AI/PouchContent.event.root.ainb";
pub const DONOR_PACK: &str = "Pack/AI.Global.Product.111.pack.zs";
const SET_FLAG: &str = "OneShotSetNonStructGameDataBool";

pub fn donor(rf: &mut crate::romfs::Romfs) -> Res<Vec<u8>> {
    let raw: Vec<u8> = rf.read_zs(DONOR_PACK)?;
    let es: Vec<crate::sarc::Entry> = crate::sarc::parse(&raw)?;
    crate::sarc::get(&es, DONOR)
        .map(|e: &crate::sarc::Entry| e.data.clone())
        .ok_or_else(|| E::Msg(format!("ring event: {DONOR} is missing from {DONOR_PACK}")))
}

fn ring(actor: &str) -> Res<&'static crate::key_rings::Ring> {
    crate::key_rings::all_with_hideout()
        .into_iter()
        .find(|r| r.actor == actor)
        .ok_or_else(|| E::Msg(format!("ring event: unknown actor {actor}")))
}

fn no_direct_credit(a: &Ainb, actor: &str) -> Res<()> {
    let r: &crate::key_rings::Ring = ring(actor)?;
    let applied: String = crate::key_rings::applied_flag(r.folder);
    for kind in ["OneShotSetNonStructGameDataInt", "OneShotSetStructGameDataInt"] {
        for i in a.find_els(kind) {
            if let Some(name) = a.in_str(i, "GameDataFullName") {
                if name.starts_with("DungeonInfo.") {
                    return Err(E::Format("ring event: the AINB must not write dungeon keys"));
                }
            }
        }
    }
    for i in a.find_els(SET_FLAG) {
        if a.in_str(i, "GameDataFullName") == Some(applied.as_str()) {
            return Err(E::Format("ring event: the AINB must not set the applied flag"));
        }
    }
    Ok(())
}

fn patch(a: &mut Ainb, flag: &str, event: &str) -> Res<()> {
    let pf: u32 = a.pool.add(flag);
    let pe: u32 = a.pool.add(event);
    let f: Vec<u8> = pf.to_le_bytes().to_vec();
    let e: Vec<u8> = pe.to_le_bytes().to_vec();
    let z: Vec<u8> = 0u32.to_le_bytes().to_vec();
    let seq: usize = a.add_node(&Spec::composite(2, "ring#chain").kid(35, ""))?;
    let set: usize = a.add_node(
        &Spec::node("OneShotSetNonStructGameDataBool", "ring#save")
            .constant(0, "GameDataIndex", z.clone())
            .constant(1, "Value", e.clone())
            .constant(3, "GameDataFullName", f.clone()),
    )?;
    let blank: u32 = a.pool.add("");
    a.params[seq].add_plug(2, 1, crate::ainb::Plug { a: set as u32, b: blank, extra: Vec::new() })?;
    let q: usize = a.add_node(
        &Spec::query("QueryGetNonStructGameDataBool", "ring#check")
            .idle(0, "GameDataIndex", z.clone())
            .idle(1, "IsDirect", e.clone())
            .constant(3, "GameDataFullName", f.clone()),
    )?;
    let mut sel: Spec = Spec::composite(7, "ring#select").linked(1, "Input", q, 0).kid(77, "True").kid(seq, "False");
    sel.link_extra = 8;
    let id: usize = a.add_node(&sel)?;
    let g: usize = a.find_in(id, 1, "Input").ok_or_else(|| E::Format("ring event: the selector lost its input"))?;
    a.set_in_val(1, g, &e)?;
    let p: &mut crate::ainb::Param = &mut a.params[58];
    let [n, b]: [u8; 2] = p.pl[2];
    let old: &mut crate::ainb::Plug = p
        .plugs
        .get_mut(b as usize..b as usize + n as usize)
        .and_then(|g: &mut [crate::ainb::Plug]| g.iter_mut().find(|q| q.a == 35))
        .ok_or_else(|| E::Format("ring event: the get flow lost its first child"))?;
    old.a = id as u32;
    Ok(())
}

pub fn make(actor: &str, donor: &[u8]) -> Res<Vec<u8>> {
    ring(actor)?;
    let flag: String = crate::key_rings::got_flag(actor);
    let event: String = format!("{actor}.event.root");
    let mut a: Ainb = Ainb::parse(donor)?;
    patch(&mut a, &flag, &event)?;
    let set: Vec<usize> = a
        .find_els(SET_FLAG)
        .into_iter()
        .filter(|&i| a.in_str(i, "GameDataFullName") == Some(flag.as_str()))
        .collect();
    let get: Vec<usize> = a
        .find_els("QueryGetNonStructGameDataBool")
        .into_iter()
        .filter(|&i| a.in_str(i, "GameDataFullName") == Some(flag.as_str()))
        .collect();
    if set.len() != 1 || get.len() != 1 {
        return Err(E::Format("ring event: the template lost its saved flag nodes"));
    }
    no_direct_credit(&a, actor)?;
    let bad: Vec<String> = audit(&a);
    if !bad.is_empty() {
        return Err(E::Msg(format!("ring event: {actor}: {}", bad.join("; "))));
    }
    a.write()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::romfs::Romfs;

    fn donor() -> Option<Vec<u8>> {
        let p: std::path::PathBuf = ootrr_game::test_romfs::path();
        if !p.exists() {
            eprintln!("skipped: no romfs at {}, set OOTRR_ROMFS", p.display());
            return None;
        }
        let mut rf: Romfs = Romfs::open(&p).unwrap();
        super::donor(&mut rf).ok()
    }

    #[test]
    fn every_ring_event_sets_its_own_saved_flag() {
        let Some(raw) = donor() else {
            return;
        };
        for r in crate::key_rings::all_with_hideout() {
            let out: Vec<u8> = make(r.actor, &raw).unwrap();
            let a: Ainb = Ainb::parse(&out).unwrap();
            let flag: String = crate::key_rings::got_flag(r.actor);
            let set: Vec<usize> = a
                .find_els(SET_FLAG)
                .into_iter()
                .filter(|&i| a.in_str(i, "GameDataFullName") == Some(flag.as_str()))
                .collect();
            let get: Vec<usize> = a
                .find_els("QueryGetNonStructGameDataBool")
                .into_iter()
                .filter(|&i| a.in_str(i, "GameDataFullName") == Some(flag.as_str()))
                .collect();
            assert_eq!((set.len(), get.len()), (1, 1), "{}", r.actor);
            assert!(!String::from_utf8_lossy(&out).contains("Item_KeyRing_"), "{}", r.actor);
        }
    }

    #[test]
    fn every_ring_event_leaves_key_credit_to_the_flows() {
        let Some(raw) = donor() else {
            return;
        };
        for r in crate::key_rings::all_with_hideout() {
            let out: Vec<u8> = make(r.actor, &raw).unwrap();
            let a: Ainb = Ainb::parse(&out).unwrap();
            for name in [format!("DungeonInfo.{}.SmallKeyNum", r.folder), format!("DungeonInfo.{}.SmallKeyTotalNum", r.folder)] {
                for kind in ["OneShotSetNonStructGameDataInt", "OneShotSetStructGameDataInt"] {
                    let set: Vec<usize> = a
                        .find_els(kind)
                        .into_iter()
                        .filter(|&i| a.in_str(i, "GameDataFullName") == Some(name.as_str()))
                        .collect();
                    assert!(set.is_empty(), "{}: {} must not be written by the AINB", r.actor, name);
                }
            }
            let applied: String = crate::key_rings::applied_flag(r.folder);
            let set: Vec<usize> = a
                .find_els(SET_FLAG)
                .into_iter()
                .filter(|&i| a.in_str(i, "GameDataFullName") == Some(applied.as_str()))
                .collect();
            assert!(set.is_empty(), "{}: {} must not be written by the AINB", r.actor, applied);
            assert!(audit(&a).is_empty(), "{}: bad AINB", r.actor);
            let again: Ainb = Ainb::parse(&a.write().unwrap()).unwrap();
            assert!(audit(&again).is_empty(), "{}: bad AINB after round trip", r.actor);
        }
    }
}
