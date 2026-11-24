











use std::collections::BTreeMap as BM;

use crate::bfevfl::{Bf, Kind, replace_str};
use crate::byml::{Byml, Node};
use crate::chest_slot::LIST;
use crate::error::{Error as E, Res};
use crate::flow_locations::{StagedFile, dict_id};
use crate::placement::hash_of;
use crate::romfs::Romfs;
use crate::settings::seed::SeedFile;
use crate::slice::data::Data;
use crate::slice::edit::Edited;
use crate::slice::support::{Keep, Plan};
use crate::staging::{Out, rewrite};
use crate::zs;

pub const FLOW: &str = "Event/EventFlow/SageOfSpirit_RescueCarpenter.bfevfl.zs";
pub const RUPEE: &str = "Obj_Rupee_Green";
pub const NATIVE: [&str; 4] = [
    "Hideout 1 Torch Jail Gerudo Key",
    "Hideout 2 Torches Jail Gerudo Key",
    "Hideout 3 Torches Jail Gerudo Key",
    "Hideout 4 Torches Jail Gerudo Key",
];

const FLOW_NAME: &str = "SageOfSpirit_RescueCarpenter";
const WARP: &str = "EventWarpToActorIncludedRayCheck";




fn orig_hash(i: usize) -> u64 {
    [
        14727159573379044362u64,
        5303541637929200213,
        1406667554698159711,
        10731814783987785199,
    ][i]
}

pub fn is_native(loc: &str) -> bool {
    NATIVE.contains(&loc)
}

fn old_actor(d: &Data, loc: &str) -> Res<String> {
    d.cat
        .by_name(loc)
        .map(|l| l.item.clone())
        .ok_or_else(|| E::Msg(format!("{loc} is not in the location catalog")))
}


pub fn edit_banc(
    file: &str,
    raw: &[u8],
    edited: Option<Edited>,
    d: &Data,
    p: &Plan,
    seed: &SeedFile,
    fixed: &BM<String, String>,
) -> Res<Option<Edited>> {
    if p.hideout == Keep::Vanilla {
        return Ok(edited);
    }
    let actors: BM<String, String> = crate::gerudo_drops::actors(d, seed, fixed)?;
    let mut jobs: Vec<(usize, String, Vec<(u64, String, String)>)> = Vec::new();
    for (i, name) in NATIVE.iter().copied().enumerate() {
        let Some(l) = d.cat.by_name(name) else {
            return Err(E::Msg(format!("{name} is not in the location catalog")));
        };
        let old: String = l.item.clone();
        let new: String = actors
            .get(name)
            .cloned()
            .ok_or_else(|| E::Msg(format!("{name} has no pickup pack actor")))?;
        let mut refs: Vec<(u64, String, String)> = Vec::new();
        for r in &l.refs {
            if r.files.iter().any(|f| f == file) {
                refs.push((r.hash, old.clone(), new.clone()));
            }
        }
        if !refs.is_empty() {
            jobs.push((i, name.to_string(), refs));
        }
    }
    if jobs.is_empty() {
        return Ok(edited);
    }
    let bytes: &[u8] = edited.as_ref().map_or(raw, |e| &e.bytes);
    let mut y: Byml = Byml::parse(bytes)?;
    let Some(list) = y.root.get_mut("Actors").and_then(|a| a.as_arr_mut()) else {
        return Err(E::Msg(format!("{file} has no Actors array")));
    };
    for (_i, loc, refs) in jobs {
        let mut flow: Option<usize> = None;
        let mut spare: Option<usize> = None;
        for (hash, _, _) in &refs {
            let Some(at) = list.iter().position(|a| hash_of(a) == Some(*hash)) else {
                continue;
            };
            let name: Option<&str> = list[at].get("Name").and_then(|v| v.as_str());
            if name.is_some_and(|s| s.starts_with("SmallKeyForEvent_")) {
                flow = Some(at);
            } else if spare.is_none() {
                spare = Some(at);
            }
        }
        let Some(flow_i) = flow else {
            return Err(E::Msg(format!("{loc} has no flow-bound drop actor in {file}")));
        };
        let Some(spare_i) = spare else {
            return Err(E::Msg(format!("{loc} has no spare drop actor in {file}")));
        };
        let flow_hash: u64 = hash_of(&list[flow_i]).unwrap_or(0);
        let (_, old, new) = refs
            .iter()
            .find(|(h, _, _)| *h == flow_hash)
            .ok_or_else(|| E::Msg(format!("{loc} flow actor is not a catalog ref")))?;


        if list[flow_i].get("Gyaml").and_then(|v| v.as_str()) != Some(old.as_str()) {
            return Err(E::Msg(format!("{loc} flow actor does not hold {old} in {file}")));
        }
        list[flow_i].set("Gyaml", Node::Str(RUPEE.to_string()));

        if list[spare_i].get("Gyaml").and_then(|v| v.as_str()) != Some(old.as_str()) {
            return Err(E::Msg(format!("{loc} spare actor does not hold {old} in {file}")));
        }


        list[spare_i].set("Gyaml", Node::Str(new.clone()));
    }
    let out: Vec<u8> = y.write()?;
    let mut e: Edited = edited.unwrap_or(Edited {
        file: file.to_string(),
        old_len: raw.len(),
        bytes: Vec::new(),
        hits: Vec::new(),
        used: Vec::new(),
        hearts: Vec::new(),
        wraps: Vec::new(),
        touched: Vec::new(),
    });
    e.bytes = out;
    Ok(Some(e))
}

fn add_rows(y: &mut Byml, hashes: &[u64]) -> Res<usize> {
    let rows: &mut Vec<Node> = y
        .root
        .get_mut("Data")
        .and_then(|d| d.get_mut("Bool64bitKey"))
        .and_then(|t| t.as_arr_mut())
        .ok_or_else(|| E::Format("hideout keys: the flag list has no Bool64bitKey table"))?;
    let mut n: usize = 0;
    for h in hashes {
        if rows.iter().any(|r| hash_of(r) == Some(*h)) {
            continue;
        }



        let at: usize = rows.partition_point(|r| hash_of(r).is_some_and(|x| x < *h));
        rows.insert(
            at,
            Node::Map(vec![
                ("Hash".to_string(), Node::U64(*h)),
                ("ResetTypeValue".to_string(), Node::I32(24)),
                ("SaveFileIndex".to_string(), Node::I32(0)),
            ]),
        );
        n += 1;
    }
    Ok(n)
}




pub fn stage_rows(rf: &mut Romfs, more: &mut Vec<Out>, active: bool) -> Res<()> {
    if !active {
        return Ok(());
    }
    let mut hashes: Vec<u64> = (0..NATIVE.len()).map(orig_hash).collect();
    hashes.sort_unstable();
    hashes.dedup();
    match more.iter().position(|o| o.path == LIST) {
        Some(i) => {
            let id: Option<u32> = dict_id(&more[i].z);
            let raw: Vec<u8> = rf.zs.decompress(&more[i].z)?;
            let mut y: Byml = Byml::parse(&raw)?;
            add_rows(&mut y, &hashes)?;
            let out: Vec<u8> = y.write()?;
            more[i].new = out.len();
            more[i].z = zs::compress_raw(&out, id)?;
        }
        None => {
            more.push(rewrite(rf, LIST, |raw| {
                let mut y: Byml = Byml::parse(raw)?;
                add_rows(&mut y, &hashes)?;
                y.write()
            })?);
        }
    }
    Ok(())
}

fn patch_flow(raw: &[u8], d: &Data, actors: &BM<String, String>) -> Res<Vec<u8>> {
    let mut b: Bf = Bf::parse(raw)?;
    for name in NATIVE {
        let old: String = old_actor(d, name)?;
        actors
            .get(name)
            .ok_or_else(|| E::Msg(format!("{name} has no pickup pack actor")))?;
        let n = replace_str(&mut b, &old, RUPEE);
        if n.total() == 0 {
            return Err(E::Msg(format!("the carpenter rescue flow has no {old} to turn into a harmless rupee")));
        }
    }
    let f: &mut crate::bfevfl::Flow = b
        .flows
        .iter_mut()
        .find(|f| f.name == FLOW_NAME)
        .ok_or_else(|| E::Msg(format!("the carpenter rescue flow has no {FLOW_NAME}")))?;





    let drop: usize = f
        .actors
        .iter()
        .position(|a| a.name == "Obj_Dungeon_SmallKey" && a.sub == "GerudoThiefDropKey(Obj_SmallKey)")
        .ok_or_else(|| E::Format("the carpenter rescue flow has no GerudoThiefDropKey key actor"))?;
    f.actors[drop].name = RUPEE.to_string();
    let drop_actor: u16 = crate::dungeon_credit::idx(drop)?;
    let warp_action: u16 = crate::dungeon_credit::idx(
        f.actors[drop]
            .actions
            .iter()
            .position(|x| x == WARP)
            .ok_or_else(|| E::Format("the carpenter rescue key actor has no warp action"))?,
    )?;

    let warp: usize = f
        .events
        .iter()
        .position(|e| e.name == "Event160")
        .ok_or_else(|| E::Format("the carpenter rescue flow has no Event160 key warp"))?;
    if let Kind::Action { actor, act, .. } = &mut f.events[warp].kind {
        *actor = drop_actor;
        *act = warp_action;
    } else {
        return Err(E::Format("the carpenter rescue key warp is not an action"));
    }
    let out: Vec<u8> = b.write()?;
    if Bf::parse(&out)? != b {
        return Err(E::Format("the carpenter rescue flow does not parse back after the pickup actor edit"));
    }
    Ok(out)
}



pub fn stage_flow(
    rf: &mut Romfs,
    files: &mut Vec<StagedFile>,
    d: &Data,
    p: &Plan,
    seed: &SeedFile,
    fixed: &BM<String, String>,
) -> Res<()> {
    if p.hideout == Keep::Vanilla {
        return Ok(());
    }
    let actors: BM<String, String> = crate::gerudo_drops::actors(d, seed, fixed)?;
    let at: Option<usize> = files.iter().position(|f| f.path == FLOW);
    let compressed: Vec<u8> = match at {
        Some(i) => files[i].z.clone(),
        None => rf.read(FLOW)?,
    };
    let id: Option<u32> = dict_id(&compressed);
    let raw: Vec<u8> = rf.zs.decompress(&compressed)?;
    let out: Vec<u8> = patch_flow(&raw, d, &actors)?;
    let old: usize = at.map_or(raw.len(), |i| files[i].old);
    let z: Vec<u8> = zs::compress_raw(&out, id)?;
    let file: StagedFile = StagedFile { path: FLOW.to_string(), z, old, new: out.len() };
    match at {
        Some(i) => files[i] = file,
        None => files.push(file),
    }
    Ok(())
}
