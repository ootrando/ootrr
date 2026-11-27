use std::path::Path as P;

use crate::bombchu_bag::{actor_path, CAPACITY, CHU};
use crate::byml::{Byml, Node};
use crate::error::{Error as E, Res};
use crate::pickup_pack::pack_of;
use crate::pickup_pack::rows::{self, POUCH_INFO};
use crate::romfs::Romfs;
use crate::sarc::{self, Entry};
use crate::staging::Out;

pub const KIND: &str = "MaxNumGameData";
pub const SHARED: &str = "Bomb";
pub const GATE: &str = "SmallBagActor";
pub const FIXED: &str = "FixedMaxNum";
pub const FILL: &str = "ActorFullOnlyIfIsGetWhenGetBag";
pub const RAISE: &str = "ExpansionMaxNumActor";
pub const RAISED: &str = "Actor";
pub const BOMB: &str = "Item_Bomb";
pub const CHU_PARAM: &str = "Component/PouchContentParam/Item_Bombchu.game__component__PouchContentParam.bgyml";
pub const CHU_PACKS: [&str; 11] = [
    "Item_Bombchu",
    "Obj_Bombchu",
    "Pouch_Bombchu",
    "Shop_BombchuBundle_01",
    "Shop_BombchuBundle_02",
    "Shop_BombchuBundle_03",
    "Shop_BombchuBundle_04",
    "Shop_BombchuBundle_05",
    "Shop_BombchuBundle_06",
    "Shop_BombchuBundle_07",
    "Shop_BombchuBundle_08",
];
pub const TIERS: [&str; 3] = ["Small", "Medium", "Large"];
pub const CHU_ROWS: [&str; 2] = ["Item_Bombchu", "Shop_BombchuBundle_03"];

fn bad(m: String) -> E {
    E::Msg(format!("bombchu bag: {m}"))
}

pub fn bag_packs() -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for t in TIERS {
        let f: String = format!("GameParameter/ExpansionMaxNumActor/Item_BombBag_{t}.game__actor__ExpansionMaxNumActor.bgyml");
        for p in ["Item", "Pouch"] {
            out.push((format!("{p}_BombBag_{t}"), f.clone()));
        }
    }
    out
}

pub fn own_max(n: &mut Node, who: &str) -> Res<()> {
    if n.get(KIND).and_then(|v| v.as_str()) != Some(SHARED) {
        return Err(bad(format!("{who} does not share the {SHARED} maximum, the game files differ")));
    }
    n.remove(KIND);
    n.remove(GATE);
    n.set(FIXED, Node::I32(CAPACITY));
    Ok(())
}

pub fn no_chus(n: &mut Node, who: &str) -> Res<()> {
    let chu: String = actor_path(CHU);
    let list: &mut Vec<Node> = n.get_mut(RAISED).and_then(|v| v.as_arr_mut()).ok_or_else(|| bad(format!("{who} raises no actor list")))?;
    let before: usize = list.len();
    list.retain(|v| v.as_str() != Some(chu.as_str()));
    if list.len() + 1 != before || !list.iter().any(|v| v.as_str() == Some(actor_path(BOMB).as_str())) {
        return Err(bad(format!("{who} does not raise both bombs and Bombchus, the game files differ")));
    }
    n.remove(FILL);
    Ok(())
}

fn edit_file(raw: &[u8], file: &str, who: &str, f: impl FnOnce(&mut Node, &str) -> Res<()>) -> Res<Vec<u8>> {
    let mut es: Vec<Entry> = sarc::parse(raw)?;
    let e: &Entry = sarc::get(&es, file).ok_or_else(|| bad(format!("{who} has no {file}")))?;
    let mut y: Byml = Byml::parse(&e.data)?;
    f(&mut y.root, who)?;
    sarc::set(&mut es, file, y.write()?);
    sarc::write(&es)
}

pub fn rows_of(raw: &[u8]) -> Res<Vec<u8>> {
    let mut y: Byml = Byml::parse(raw)?;
    let list: &mut Vec<Node> = y.root.as_arr_mut().ok_or(E::Format("bombchu bag: the pouch table is not a row list"))?;
    for name in CHU_ROWS {
        let r: &mut Node = list.iter_mut().find(|r| rows::id(r) == Some(name)).ok_or_else(|| bad(format!("{name} has no row in {POUCH_INFO}")))?;
        own_max(r, name)?;
    }
    for (name, _) in bag_packs() {
        let r: &mut Node = list.iter_mut().find(|r| rows::id(r) == Some(name.as_str())).ok_or_else(|| bad(format!("{name} has no row in {POUCH_INFO}")))?;
        let was: String = r.get(RAISE).and_then(|v| v.as_str()).unwrap_or("").to_string();
        let kept: Vec<&str> = was.split(',').filter(|a| *a != actor_path(CHU)).collect();
        if kept.len() + 1 != was.split(',').count() || !kept.contains(&actor_path(BOMB).as_str()) {
            return Err(bad(format!("the {name} row does not raise both bombs and Bombchus, the game files differ")));
        }
        r.set(RAISE, Node::Str(kept.join(",")));
        r.remove(FILL);
    }
    y.write()
}

pub fn stage(rf: &mut Romfs, late: &mut Vec<Out>, work: &P) -> Res<()> {
    for a in CHU_PACKS {
        crate::story_skip::stage_late_file(rf, late, work, &pack_of(a), |raw| edit_file(raw, CHU_PARAM, a, own_max))?;
    }
    for (a, f) in bag_packs() {
        crate::story_skip::stage_late_file(rf, late, work, &pack_of(&a), |raw| edit_file(raw, &f, &a, no_chus))?;
    }
    crate::story_skip::stage_late_file(rf, late, work, POUCH_INFO, rows_of)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(x: &str) -> Node {
        Node::Str(x.to_string())
    }

    #[test]
    fn the_chu_maximum_becomes_its_own_fixed_forty() {
        let mut n: Node = Node::Map(vec![(KIND.to_string(), s(SHARED)), (GATE.to_string(), s(&actor_path("Item_BombBag_Small"))), ("EnableStock".to_string(), Node::Bool(true))]);
        own_max(&mut n, "x").unwrap();
        assert!(n.get(KIND).is_none() && n.get(GATE).is_none());
        assert_eq!(n.get(FIXED), Some(&Node::I32(40)));
        assert_eq!(n.get("EnableStock"), Some(&Node::Bool(true)));
        assert!(own_max(&mut n, "x").is_err());
    }

    #[test]
    fn a_bomb_bag_stops_raising_and_filling_bombchus() {
        let mut n: Node = Node::Map(vec![(RAISED.to_string(), Node::Arr(vec![s(&actor_path(BOMB)), s(&actor_path(CHU))])), (FILL.to_string(), s(&actor_path(CHU))), ("MaxNum".to_string(), Node::I32(20))]);
        no_chus(&mut n, "x").unwrap();
        assert_eq!(n.get(RAISED), Some(&Node::Arr(vec![s(&actor_path(BOMB))])));
        assert!(n.get(FILL).is_none());
        assert_eq!(n.get("MaxNum"), Some(&Node::I32(20)));
        assert!(no_chus(&mut n, "x").is_err());
    }

    #[test]
    fn six_bag_packs_share_three_files() {
        let b: Vec<(String, String)> = bag_packs();
        assert_eq!(b.len(), 6);
        assert!(b.iter().any(|(p, f)| p == "Pouch_BombBag_Large" && f.ends_with("Item_BombBag_Large.game__actor__ExpansionMaxNumActor.bgyml")));
    }

    #[test]
    fn the_real_packs_and_rows_take_the_edit() {
        let Some(root) = std::env::var_os("OOTRR_ROMFS").map(std::path::PathBuf::from).filter(|p| p.join("Pack").is_dir()) else {
            return;
        };
        let mut rf: Romfs = Romfs::open(&root).unwrap();
        for a in CHU_PACKS {
            let raw: Vec<u8> = rf.read_zs(&pack_of(a)).unwrap();
            let out: Vec<u8> = edit_file(&raw, CHU_PARAM, a, own_max).unwrap();
            let y: Byml = Byml::parse(&sarc::get(&sarc::parse(&out).unwrap(), CHU_PARAM).unwrap().data).unwrap();
            assert_eq!(y.root.get(FIXED), Some(&Node::I32(CAPACITY)), "{a}");
        }
        for (a, f) in bag_packs() {
            edit_file(&rf.read_zs(&pack_of(&a)).unwrap(), &f, &a, no_chus).unwrap();
        }
        let t: Vec<u8> = rows_of(&rf.read_zs(POUCH_INFO).unwrap()).unwrap();
        let y: Byml = Byml::parse(&t).unwrap();
        let chu: &Node = rows::find(&y, CHU).unwrap();
        assert_eq!(chu.get(FIXED), Some(&Node::I32(CAPACITY)));
        assert!(chu.get(KIND).is_none() && chu.get(GATE).is_none());
        let bag: &Node = rows::find(&y, "Item_BombBag_Large").unwrap();
        assert_eq!(bag.get(RAISE).and_then(|v| v.as_str()), Some(actor_path(BOMB).as_str()));
        assert!(bag.get(FILL).is_none());
        assert_eq!(bag.get(KIND).and_then(|v| v.as_str()), Some(SHARED));
    }
}
