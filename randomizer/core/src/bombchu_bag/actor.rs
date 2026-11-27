use std::path::Path as P;

use crate::bombchu_bag::look::Look;
use crate::bombchu_bag::{ACTOR, BAG, BUNDLE, CAPACITY, PROJECT};
use crate::byml::{Byml, Node};
use crate::error::{Error as E, Res};
use crate::pickup_pack::actor::{own_name_ref, pouch_file, ANIM_REF, AS_REF, MODEL_REF, NAME_REF, POUCH_REF};
use crate::pickup_pack::closure::{actor_file, local, norm, COMPONENTS};
use crate::pickup_pack::rows::{self, ACTOR_INFO, GAME_ACTOR_INFO, HEAP, POUCH_INFO, VARIATION};
use crate::pickup_pack::{pack_of, PACK_DICT};
use crate::romfs::Romfs;
use crate::sarc::{self, Entry};
use crate::staging::Out;
use crate::zs;

pub const COUNT: &str = "BundleActorNum";
pub const OFFSET: &str = "GetDemoOffsetPos";
pub const RENDER_HEAP: &str = "PreActorRenderInfoHeapSize";
pub const MODEL_KEYS: [&str; 4] = ["AnimationResources", "CalcRadius", "DisplayRadius", "LoadRadius"];
pub const TABLES: [&str; 3] = [ACTOR_INFO, GAME_ACTOR_INFO, POUCH_INFO];

fn bad(m: String) -> E {
    E::Msg(format!("bombchu bag: {m}"))
}

pub fn info_file() -> String {
    format!("Component/ModelInfo/{PROJECT}.engine__component__ModelInfo.bgyml")
}

fn s(x: &str) -> Node {
    Node::Str(x.to_string())
}

fn byml(es: &[Entry], path: &str, who: &str) -> Res<Byml> {
    let e: &Entry = sarc::get(es, path).ok_or_else(|| bad(format!("the {who} pack has no {path}")))?;
    Byml::parse(&e.data)
}

fn comp(y: &Byml, k: &str, who: &str) -> Res<String> {
    y.root.get(COMPONENTS).and_then(|c| c.get(k)).and_then(|v| v.as_str()).map(|v| v.to_string()).ok_or_else(|| bad(format!("the {who} actor has no {k}")))
}

fn file_of(r: &str, who: &str) -> Res<String> {
    norm(r).ok_or_else(|| bad(format!("the {who} reference {r} is not a pack file")))
}

pub fn pack(bundle: &[Entry], bag: &[Entry]) -> Res<Vec<Entry>> {
    let (ba, ga): (String, String) = (actor_file(BUNDLE), actor_file(BAG));
    let mut a: Byml = byml(bundle, &ba, BUNDLE)?;
    let g: Byml = byml(bag, &ga, BAG)?;
    let old_pouch: String = file_of(&comp(&a, POUCH_REF, BUNDLE)?, BUNDLE)?;
    let old_anim: String = file_of(&comp(&a, ANIM_REF, BUNDLE)?, BUNDLE)?;
    let bag_anim: String = file_of(&comp(&g, ANIM_REF, BAG)?, BAG)?;
    let bag_info: String = file_of(&comp(&g, MODEL_REF, BAG)?, BAG)?;
    let bag_pouch: String = file_of(&comp(&g, POUCH_REF, BAG)?, BAG)?;
    let mut p: Byml = byml(bundle, &old_pouch, BUNDLE)?;
    if p.root.get(COUNT).is_none() {
        return Err(bad(format!("{old_pouch} has no {COUNT}, the bundle is not a bundle")));
    }
    p.root.set(COUNT, Node::I32(CAPACITY));
    if let Some(o) = byml(bag, &bag_pouch, BAG)?.root.get(OFFSET) {
        p.root.set(OFFSET, o.clone());
    }
    let mut info: Byml = byml(bag, &bag_info, BAG)?;
    for (k, v) in [("FmdbName", BAG), ("ModelProjectName", PROJECT)] {
        if !info.root.set(k, s(v)) {
            return Err(bad(format!("{bag_info} is not a map")));
        }
    }
    info.root.remove(VARIATION);
    let c: &mut Node = a.root.get_mut(COMPONENTS).ok_or_else(|| bad(format!("{ba} has no components")))?;
    if c.get(NAME_REF).is_none() {
        return Err(bad(format!("{ba} has no {NAME_REF}")));
    }
    c.set(NAME_REF, s(&own_name_ref(ACTOR)));
    c.set(MODEL_REF, s(&local(&info_file())));
    c.set(ANIM_REF, s(&local(&bag_anim)));
    c.set(AS_REF, s(&comp(&g, AS_REF, BAG)?));
    c.set(POUCH_REF, s(&local(&pouch_file(ACTOR))));
    let gone: [&str; 2] = [ba.as_str(), old_anim.as_str()];
    let mut out: Vec<Entry> = bundle.iter().filter(|e| !gone.contains(&e.name.as_str())).cloned().collect();
    let anim: Entry = sarc::get(bag, &bag_anim).cloned().ok_or_else(|| bad(format!("the {BAG} pack has no {bag_anim}")))?;
    out.push(anim);
    out.push(Entry { name: info_file(), data: info.write()? });
    out.push(Entry { name: pouch_file(ACTOR), data: p.write()? });
    out.push(Entry { name: actor_file(ACTOR), data: a.write()? });
    Ok(out)
}

fn int(n: &Node, k: &str) -> i64 {
    match n.get(k) {
        Some(Node::I32(v)) => *v as i64,
        Some(Node::U32(v)) => *v as i64,
        _ => 0,
    }
}

fn same_int(like: Option<&Node>, v: i64) -> Node {
    match like {
        Some(Node::U32(_)) => Node::U32(v.max(0) as u32),
        _ => Node::I32(v as i32),
    }
}

fn vec3(n: Option<&Node>) -> Option<[f64; 3]> {
    let n: &Node = n?;
    let f = |k: &str| match n.get(k) {
        Some(Node::F32(v)) => Some(*v as f64),
        _ => None,
    };
    Some([f("X")?, f("Y")?, f("Z")?])
}

fn put3(v: [f64; 3]) -> Node {
    Node::Map(vec![("X".to_string(), Node::F32(v[0] as f32)), ("Y".to_string(), Node::F32(v[1] as f32)), ("Z".to_string(), Node::F32(v[2] as f32))])
}

pub fn row(t: &mut Byml, i: usize, look: (&[f64; 3], &[f64; 3])) -> Res<()> {
    let name: &str = TABLES[i];
    if rows::find(t, ACTOR).is_some() {
        return Err(bad(format!("{ACTOR} already has a row in {name}")));
    }
    let mut r: Node = rows::pouch_row(rows::need(t, BUNDLE, name)?, ACTOR);
    match i {
        0 => {
            let g: Node = rows::need(t, BAG, name)?.clone();
            r.set(rows::ACTOR_NAME, s(ACTOR));
            for k in MODEL_KEYS {
                if let Some(v) = g.get(k) {
                    r.set(k, v.clone());
                }
            }
            r.set("FmdbName", s(BAG));
            r.set("ModelProjectName", s(PROJECT));
            let lo: [f64; 3] = vec3(g.get("ModelAabbMin")).ok_or_else(|| bad(format!("the {BAG} row has no model box")))?;
            let hi: [f64; 3] = vec3(g.get("ModelAabbMax")).ok_or_else(|| bad(format!("the {BAG} row has no model box")))?;
            r.set("ModelAabbMin", put3([0, 1, 2].map(|k| lo[k].min(look.0[k]))));
            r.set("ModelAabbMax", put3([0, 1, 2].map(|k| hi[k].max(look.1[k]))));
            let render: i64 = int(&g, RENDER_HEAP) + int(&r, RENDER_HEAP);
            r.set(RENDER_HEAP, same_int(g.get(RENDER_HEAP), render));
            let heap: i64 = (int(&g, HEAP).max(int(&r, HEAP)) + rows::HEAP_MARGIN as i64 + 7) / 8 * 8;
            r.set(HEAP, same_int(r.get(HEAP), heap));
            r.remove(VARIATION);
        }
        2 => {
            if r.get(COUNT).is_none() {
                return Err(bad(format!("the {BUNDLE} pouch row has no {COUNT}")));
            }
            r.set(COUNT, Node::I32(CAPACITY));
        }
        _ => {
            r.remove(VARIATION);
        }
    }
    rows::insert(t, r)?;
    Ok(())
}

pub fn stage(rf: &mut Romfs, late: &mut Vec<Out>, work: &P, look: &Look) -> Res<()> {
    let p: String = pack_of(ACTOR);
    if rf.root.join(&p).is_file() || late.iter().any(|o| o.path == p) {
        return Err(bad(format!("{p} exists already")));
    }
    let bz: Vec<u8> = rf.read(&pack_of(BUNDLE))?;
    let bundle: Vec<Entry> = sarc::parse(&rf.zs.decompress(&bz)?)?;
    let bag: Vec<Entry> = sarc::parse(&rf.read_zs(&pack_of(BAG))?)?;
    let raw: Vec<u8> = sarc::write(&pack(&bundle, &bag)?)?;
    let id: u32 = crate::flow_locations::dict_id(&bz).unwrap_or(PACK_DICT);
    late.push(Out { path: p, z: zs::compress_raw(&raw, Some(id))?, old: 0, new: raw.len() });
    for (i, t) in TABLES.iter().enumerate() {
        crate::story_skip::stage_late_file(rf, late, work, t, |b| {
            let mut y: Byml = Byml::parse(b)?;
            row(&mut y, i, (&look.min, &look.max))?;
            y.write()
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn romfs() -> Option<Romfs> {
        let root: std::path::PathBuf = std::env::var_os("OOTRR_ROMFS").map(std::path::PathBuf::from).filter(|p| p.join("Pack").is_dir())?;
        Romfs::open(&root).ok()
    }

    #[test]
    fn the_pack_is_the_bundle_with_the_bag_look_and_forty_chus() {
        let Some(mut rf) = romfs() else {
            return;
        };
        let bundle: Vec<Entry> = sarc::parse(&rf.read_zs(&pack_of(BUNDLE)).unwrap()).unwrap();
        let bag: Vec<Entry> = sarc::parse(&rf.read_zs(&pack_of(BAG)).unwrap()).unwrap();
        let es: Vec<Entry> = pack(&bundle, &bag).unwrap();
        assert!(sarc::get(&es, &actor_file(BUNDLE)).is_none());
        let a: Byml = byml(&es, &actor_file(ACTOR), ACTOR).unwrap();
        assert_eq!(comp(&a, NAME_REF, ACTOR).unwrap(), own_name_ref(ACTOR));
        assert_eq!(comp(&a, AS_REF, ACTOR).unwrap(), "?AS/Item_BombBag.root.asb");
        for k in [MODEL_REF, ANIM_REF, POUCH_REF] {
            let f: String = file_of(&comp(&a, k, ACTOR).unwrap(), ACTOR).unwrap();
            assert!(sarc::get(&es, &f).is_some(), "{k} {f}");
        }
        let p: Byml = byml(&es, &pouch_file(ACTOR), ACTOR).unwrap();
        assert_eq!(p.root.get(COUNT), Some(&Node::I32(CAPACITY)));
        assert_eq!(p.root.get("BundleActor").and_then(|v| v.as_str()), Some(crate::bombchu_bag::actor_path(crate::bombchu_bag::CHU).as_str()));
        let i: Byml = byml(&es, &info_file(), ACTOR).unwrap();
        assert_eq!(i.root.get("ModelProjectName").and_then(|v| v.as_str()), Some(PROJECT));
        assert_eq!(i.root.get("FmdbName").and_then(|v| v.as_str()), Some(BAG));
        assert!(i.root.get(VARIATION).is_none());
    }

    #[test]
    fn the_rows_join_every_table_sorted() {
        let Some(mut rf) = romfs() else {
            return;
        };
        let lo: [f64; 3] = [-0.06, 0.04, -0.07];
        let hi: [f64; 3] = [0.06, 0.15, 0.13];
        for (i, t) in TABLES.iter().enumerate() {
            let mut y: Byml = rf.read_byml(t).unwrap();
            row(&mut y, i, (&lo, &hi)).unwrap();
            assert!(rows::sorted(&y).unwrap());
            let r: &Node = rows::find(&y, ACTOR).unwrap();
            if i == 0 {
                assert_eq!(r.get("ModelProjectName").and_then(|v| v.as_str()), Some(PROJECT));
                assert_eq!(r.get(rows::ACTOR_NAME).and_then(|v| v.as_str()), Some(ACTOR));
                assert!(vec3(r.get("ModelAabbMax")).unwrap()[2] >= 0.13 - 1e-6);
                assert!(r.get(VARIATION).is_none());
            }
            if i == 2 {
                assert_eq!(r.get(COUNT), Some(&Node::I32(CAPACITY)));
            }
            assert!(row(&mut y, i, (&lo, &hi)).is_err());
        }
    }
}
