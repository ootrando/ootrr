use std::path::{Path as P, PathBuf as PB};

use crate::bombchu_bag::actor::{info_file, COUNT};
use crate::bombchu_bag::{actor_path, is_pack, ACTOR, BUNDLE, CAPACITY};
use crate::byml::{Byml, Node};
use crate::error::{Error as E, Res};
use crate::pickup_pack::actor::{MODEL_REF, POUCH_ACTOR};
use crate::pickup_pack::closure::{local, COMPONENTS};
use crate::pickup_pack::rows::{self, ACTOR_INFO, POUCH_INFO};
use crate::pickup_pack::sizes::pack_size;
use crate::romfs::Romfs;
use crate::rsizetable::{Rst, TABLE_PATH};
use crate::sarc::{self, Entry};
use crate::zs;

pub const ROW_KEYS: [&str; 7] = ["AnimationResources", "FmdbName", "ModelProjectName", "ModelAabbMax", "ModelAabbMin", "PreActorRenderInfoHeapSize", "ModelVariationAnims"];
const CHU_ANIM: &str = "Item_Bombchu_Animation";
const BAG_ANIM: &str = "Item_BombBag_Small_Animation";
const EDITED: [&str; 3] = ["Actor/", "Component/PouchContentParam/", "Component/AnimationParam/"];

fn bad(m: String) -> E {
    E::Msg(format!("bombchu bag: {m}"))
}

fn swap_anim(n: &mut Node) -> usize {
    match n {
        Node::Str(s) if s == CHU_ANIM => {
            *s = BAG_ANIM.to_string();
            1
        }
        Node::Arr(v) => v.iter_mut().map(swap_anim).sum(),
        Node::Map(m) => m.iter_mut().map(|e| swap_anim(&mut e.1)).sum(),
        _ => 0,
    }
}

pub fn repack(raw: &[u8], info: &Entry, name: &str) -> Res<Vec<u8>> {
    let mut es: Vec<Entry> = sarc::parse(raw)?;
    let (mut owner, mut model): (usize, usize) = (0, 0);
    for e in es.iter_mut() {
        let ours: bool = EDITED.iter().any(|d| e.name.starts_with(d));
        if !(ours && e.name.ends_with(".bgyml") && crate::pickup_pack::closure::is_byml(&e.data)) {
            continue;
        }
        let mut y: Byml = Byml::parse(&e.data)?;
        let mut changed: bool = false;
        if y.root.get(POUCH_ACTOR).and_then(|v| v.as_str()) == Some(actor_path(BUNDLE).as_str()) {
            y.root.set(POUCH_ACTOR, Node::Str(actor_path(ACTOR)));
            owner += 1;
            changed = true;
        }
        if y.root.get(COUNT).is_some() && y.root.get("BundleActor").and_then(|v| v.as_str()) == Some(actor_path(crate::bombchu_bag::CHU).as_str()) {
            y.root.set(COUNT, Node::I32(CAPACITY));
            changed = true;
        }
        if let Some(c) = y.root.get_mut(COMPONENTS)
            && c.get(MODEL_REF).is_some()
        {
            c.set(MODEL_REF, Node::Str(local(&info_file())));
            model += 1;
            changed = true;
        }
        if e.name.starts_with("Component/AnimationParam/") && swap_anim(&mut y.root) > 0 {
            changed = true;
        }
        if changed {
            e.data = y.write()?;
        }
    }
    if owner == 0 || model == 0 {
        return Err(bad(format!("{name} does not hand out {BUNDLE} with a model, the pickup build changed")));
    }
    sarc::set(&mut es, &info.name, info.data.clone());
    sarc::write(&es)
}

pub fn actor_row(t: &mut Byml, name: &str) -> Res<()> {
    let item: Node = rows::need(t, ACTOR, ACTOR_INFO)?.clone();
    let list: &mut Vec<Node> = t.root.as_arr_mut().ok_or(E::Format("bombchu bag: the actor table is not a row list"))?;
    let r: &mut Node = list.iter_mut().find(|r| rows::id(r) == Some(name)).ok_or_else(|| bad(format!("{name} has no row in {ACTOR_INFO}")))?;
    r.set(rows::ACTOR_NAME, Node::Str(ACTOR.to_string()));
    for k in ROW_KEYS {
        match item.get(k) {
            Some(v) => {
                r.set(k, v.clone());
            }
            None => {
                r.remove(k);
            }
        }
    }
    Ok(())
}

pub fn pouch_row(t: &mut Byml, name: &str) -> Res<()> {
    let list: &mut Vec<Node> = t.root.as_arr_mut().ok_or(E::Format("bombchu bag: the pouch table is not a row list"))?;
    if let Some(r) = list.iter_mut().find(|r| rows::id(r) == Some(name))
        && r.get(COUNT).is_some()
    {
        r.set(COUNT, Node::I32(CAPACITY));
    }
    Ok(())
}

fn rw(rf: &mut Romfs, p: &PB, f: impl FnOnce(&[u8]) -> Res<Vec<u8>>) -> Res<usize> {
    let z: Vec<u8> = std::fs::read(p)?;
    let out: Vec<u8> = f(&rf.zs.decompress(&z)?)?;
    std::fs::write(p, zs::compress_raw(&out, zs::dict_of(&z))?)?;
    Ok(out.len())
}

pub fn repoint(rf: &mut Romfs, base: &P, names: &[String]) -> Res<Vec<String>> {
    let mine: Vec<&String> = names.iter().filter(|n| is_pack(n)).collect();
    if mine.is_empty() {
        return Ok(Vec::new());
    }
    let item: Vec<Entry> = sarc::parse(&rf.zs.decompress(&std::fs::read(base.join(crate::pickup_pack::pack_of(ACTOR)))?)?)?;
    let info: Entry = sarc::get(&item, &info_file()).cloned().ok_or_else(|| bad(format!("the {ACTOR} pack lost its model info")))?;
    let tp: PB = base.join(TABLE_PATH);
    let mut rst: Rst = Rst::parse(&rf.zs.decompress(&std::fs::read(&tp)?)?)?;
    let mut done: Vec<String> = Vec::new();
    for n in mine {
        let pp: PB = base.join(crate::pickup_pack::pack_of(n));
        let len: usize = rw(rf, &pp, |raw| repack(raw, &info, n))?;
        rst.set_size(&crate::pickup_pack::pack_of(n), pack_size(len)?)?;
        done.push(n.clone());
    }
    rw(rf, &base.join(ACTOR_INFO), |raw| {
        let mut y: Byml = Byml::parse(raw)?;
        for n in &done {
            actor_row(&mut y, n)?;
        }
        y.write()
    })?;
    rw(rf, &base.join(POUCH_INFO), |raw| {
        let mut y: Byml = Byml::parse(raw)?;
        for n in &done {
            pouch_row(&mut y, n)?;
        }
        y.write()
    })?;
    std::fs::write(&tp, rst.to_zs()?)?;
    Ok(done)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(x: &str) -> Node {
        Node::Str(x.to_string())
    }

    fn file(name: &str, root: Node) -> Entry {
        Entry { name: name.to_string(), data: Byml { root, version: 7, big: false }.write().unwrap() }
    }

    #[test]
    fn a_built_pickup_hands_out_the_bag_with_its_model() {
        let actor: Entry = file("Actor/Obj_BombchuBag.engine__actor__ActorParam.bgyml", Node::Map(vec![(COMPONENTS.to_string(), Node::Map(vec![(MODEL_REF.to_string(), s("?Component/ModelInfo/Item_Bombchu.engine__component__ModelInfo.bgyml"))]))]));
        let pouch: Entry = file(
            "Component/PouchContentParam/Obj_BombchuBag.game__component__PouchContentParam.bgyml",
            Node::Map(vec![("BundleActor".to_string(), s(&actor_path("Item_Bombchu"))), (COUNT.to_string(), Node::I32(20)), (POUCH_ACTOR.to_string(), s(&actor_path(BUNDLE)))]),
        );
        let anim: Entry = file("Component/AnimationParam/Obj_BombchuBag.engine__component__AnimationParam.bgyml", Node::Map(vec![("AnimationResources".to_string(), Node::Arr(vec![Node::Map(vec![("ModelProjectName".to_string(), s(CHU_ANIM))])]))]));
        let info: Entry = Entry { name: info_file(), data: vec![1, 2, 3] };
        let raw: Vec<u8> = sarc::write(&[actor, pouch, anim]).unwrap();
        let out: Vec<Entry> = sarc::parse(&repack(&raw, &info, PACK_NAME).unwrap()).unwrap();
        let p: Byml = Byml::parse(&sarc::get(&out, "Component/PouchContentParam/Obj_BombchuBag.game__component__PouchContentParam.bgyml").unwrap().data).unwrap();
        assert_eq!(p.root.get(POUCH_ACTOR).and_then(|v| v.as_str()), Some(actor_path(ACTOR).as_str()));
        assert_eq!(p.root.get(COUNT), Some(&Node::I32(CAPACITY)));
        let a: Byml = Byml::parse(&sarc::get(&out, "Actor/Obj_BombchuBag.engine__actor__ActorParam.bgyml").unwrap().data).unwrap();
        assert_eq!(a.root.get(COMPONENTS).and_then(|c| c.get(MODEL_REF)).and_then(|v| v.as_str()), Some(local(&info_file()).as_str()));
        let n: Byml = Byml::parse(&sarc::get(&out, "Component/AnimationParam/Obj_BombchuBag.engine__component__AnimationParam.bgyml").unwrap().data).unwrap();
        assert!(format!("{:?}", n.root).contains(BAG_ANIM));
        assert_eq!(sarc::get(&out, &info_file()).unwrap().data, vec![1, 2, 3]);
        let plain: Vec<u8> = sarc::write(&[file("Actor/X.engine__actor__ActorParam.bgyml", Node::Map(Vec::new()))]).unwrap();
        assert!(repack(&plain, &Entry { name: info_file(), data: Vec::new() }, PACK_NAME).is_err());
    }

    const PACK_NAME: &str = crate::bombchu_bag::PACK;
}
