use std::collections::BTreeSet as BS;
use std::path::PathBuf as PB;

use crate::byml::{Byml, Node};
use crate::error::{Error as E, Res};
use crate::pickup_pack::actor::{identity, pouch_file, AUTO_GET, FIRST_CHECK, POUCH_ACTOR};
use crate::pickup_pack::closure::{actor_file, chain, dangling_with, local, norm, select, work, Chain, Pool, COMPONENTS, PARENT};
use crate::pickup_pack::rows::{self, ACTOR_INFO, GAME_ACTOR_INFO, POUCH_INFO};
use crate::pickup_pack::write::start;
use crate::pickup_pack::{pack_of, Spec, Style};
use crate::romfs::Romfs;
use crate::sarc::{self, Entry};
use crate::shelf::{BASE, PREFIX, TEMPLATE};

#[derive(Debug, Clone)]
pub struct Shelf {
    pub name: String,
    pub item: String,
    pub look: String,
    pub price: i32,
    pub pack: Vec<u8>,
    pub fresh: Vec<(String, usize)>,
    pub peers: Vec<(String, usize)>,
    pub who: String,
    /// A song shelf with harp colours on: the coloured harp's model project for its actor row.
    pub project: Option<String>,
}

pub const IDENTITY: [(&str, &str); 1] = [("Item_Gauntlet_Goron", "Armor_Child_Gauntlet_Goron")];

pub fn shelf_identity(who: &str) -> &str {
    IDENTITY.iter().find(|x| x.0 == who).map(|x| x.1).unwrap_or(who)
}

pub fn price_file(name: &str) -> String {
    format!("GameParameter/PriceParam/{name}.game__pouchcontent__PriceParam.bgyml")
}

pub fn table_file(name: &str) -> String {
    format!("GameParameter/GameParameterTable/{name}.engine__actor__GameParameterTable.bgyml")
}

fn s(x: &str) -> Node {
    Node::Str(x.to_string())
}

fn need<'a>(c: &'a Chain, who: &str, k: &str) -> Res<&'a str> {
    c.comps.get(k).map(|v| v.as_str()).ok_or_else(|| E::Msg(format!("{who} has no {k}, it cannot stand on a shelf")))
}

pub fn int(n: &Node, k: &str) -> Option<i64> {
    match n.get(k) {
        Some(Node::I32(v)) => Some(*v as i64),
        Some(Node::U32(v)) => Some(*v as i64),
        Some(Node::I64(v)) => Some(*v),
        _ => None,
    }
}

/// A song shelf shows the shared harp (crate::shops song_models donor). When the seed mod's song
/// pack already points its model info at the song's coloured harp (crate::song_models::stage), the
/// shelf takes that model info and project too, as the song pickups do (song_models::color_packs).
fn tinted_harp(p: &Pool, item: &str, look: &str) -> Option<(String, String)> {
    let song: &str = crate::song_models::SONGS.iter().copied().find(|s| item == format!("Item_Melody_{s}"))?;
    if look != crate::song_models::DONOR {
        return None;
    }
    let info: String = crate::song_models::info_of(song);
    let project: String = crate::song_models::project_of(song);
    let y: Byml = p.byml(&info).ok()?;
    (y.root.get("ModelProjectName").and_then(|v| v.as_str()) == Some(project.as_str())).then_some((format!("?{info}"), project))
}

fn load(rf: &mut Romfs, base: Option<&PB>, name: &str) -> Res<Vec<Entry>> {
    sarc::parse(&start(rf, base, &pack_of(name))?)
}

pub fn make(rf: &mut Romfs, base: Option<&PB>, item: &str, model: Option<&str>, name: &str, price: i32) -> Res<Shelf> {
    if !crate::pickup_pack::is_actor_name(item) || !crate::pickup_pack::is_actor_name(name) {
        return Err(E::Msg(format!("{item} or {name} is not a plain actor name")));
    }
    if rf.root.join(pack_of(name)).is_file() {
        return Err(E::Msg(format!("the game already ships {name}")));
    }
    let mut p: Pool = Pool::default();
    for n in [BASE, TEMPLATE, item].into_iter().chain(model) {
        let es: Vec<Entry> = load(rf, base, n)?;
        p.add_pack(&es)?;
    }
    let peers: Vec<(String, usize)> = p.files.iter().map(|e| (e.0.clone(), e.1.len())).collect();
    let it: Chain = chain(&p, item)?;
    let tpl: Chain = chain(&p, TEMPLATE)?;
    let look_name: &str = model.unwrap_or(item);
    let look: Chain = chain(&p, look_name)?;
    let tinted: Option<(String, String)> = tinted_harp(&p, item, look_name);
    let item_pouch: String = norm(need(&it, item, "PouchContentRef")?).ok_or(E::Format("the item pouch reference is not a file"))?;
    let version: u16 = p.byml(&actor_file(TEMPLATE))?.version;
    let base_table: String = norm(need(&chain(&p, BASE)?, BASE, "GameParameterTableRef")?).ok_or(E::Format("the shelf base table reference is not a file"))?;
    let mut comps: Node = Node::Map(Vec::new());
    comps.set("ASInfoRef", s(need(&tpl, TEMPLATE, "ASInfoRef")?));
    comps.set("ASRef", s(need(&tpl, TEMPLATE, "ASRef")?));
    comps.set("ActorNameRef", s(need(&it, item, "ActorNameRef")?));
    comps.set("AnimationRef", s(need(&look, look_name, "AnimationRef")?));
    match &tinted {
        Some((info, _)) => comps.set("ModelInfoRef", s(info)),
        None => comps.set("ModelInfoRef", s(need(&look, look_name, "ModelInfoRef")?)),
    };
    comps.set("PouchContentRef", s(&local(&pouch_file(name))));
    comps.set("GameParameterTableRef", s(&local(&table_file(name))));
    let mut tc: Node = Node::Map(Vec::new());
    tc.set("PriceParam", s(&local(&price_file(name))));
    let mut t: Node = Node::Map(Vec::new());
    t.set(PARENT, s(&work(&base_table)));
    t.set("Category", s("GameParameterTableDefault"));
    t.set(COMPONENTS, tc);
    let mut pp: Node = Node::Map(Vec::new());
    pp.set("BuyingPrice", Node::I32(price));
    pp.set("SellingPrice", Node::I32(1));
    let tv: u16 = p.byml(&base_table)?.version;
    let extra: Vec<(String, Vec<u8>)> = vec![
        (table_file(name), Byml { root: t, version: tv, big: false }.write()?),
        (price_file(name), Byml { root: pp, version: tv, big: false }.write()?),
    ];
    let mut actor: Node = Node::Map(Vec::new());
    actor.set(PARENT, s(&work(&actor_file(BASE))));
    actor.set(COMPONENTS, comps);
    let sp: Spec = Spec { item: item.to_string(), name: name.to_string(), style: Style::Touch, heap: None, source: None, identity: None };
    let who: String = shelf_identity(&identity(&p, &sp, &it, &item_pouch)?).to_string();
    let mut pouch: Node = Node::Map(Vec::new());
    pouch.set(PARENT, s(&work(&item_pouch)));
    pouch.set(POUCH_ACTOR, s(&work(&actor_file(&who))));
    pouch.set(FIRST_CHECK, Node::Bool(false));
    pouch.set(AUTO_GET, Node::Bool(true));
    let a: Vec<u8> = Byml { root: actor, version, big: false }.write()?;
    let q: Vec<u8> = Byml { root: pouch, version, big: false }.write()?;
    let mut fresh: Vec<(String, usize)> = vec![(actor_file(name), a.len()), (pouch_file(name), q.len())];
    p.add(&actor_file(name), &a)?;
    p.add(&pouch_file(name), &q)?;
    for (f, d) in &extra {
        fresh.push((f.clone(), d.len()));
        p.add(f, d)?;
    }
    let files: Vec<String> = select(&p, name)?;
    let c: Chain = chain(&p, name)?;
    let known: BS<String> = p.files.keys().cloned().collect();
    if let Some(f) = dangling_with(&p, &files, &known, &c)?.first() {
        return Err(E::Msg(format!("{f} is referenced by the shelf actor {name} but left out")));
    }
    for (f, _) in &fresh {
        if !files.contains(f) {
            return Err(E::Msg(format!("the shelf actor {name} does not reach its own file {f}")));
        }
    }
    let es: Vec<Entry> = files.iter().map(|f| Entry { name: f.clone(), data: p.get(f).unwrap_or(&[]).to_vec() }).collect();
    Ok(Shelf { name: name.to_string(), item: item.to_string(), look: look_name.to_string(), price, pack: sarc::write(&es)?, fresh, peers, who, project: tinted.map(|t| t.1) })
}

pub fn heap(t: &Byml, item: &str) -> Res<i32> {
    let mut top: i64 = int(rows::need(t, item, ACTOR_INFO)?, rows::HEAP).unwrap_or(0);
    for r in t.root.as_arr().unwrap_or(&[]) {
        if rows::id(r).is_some_and(|i| i.starts_with("Shop_") && !i.starts_with("Shop_Board") && !i.starts_with(PREFIX)) {
            top = top.max(int(r, rows::HEAP).unwrap_or(0));
        }
    }
    let n: i64 = top + rows::HEAP_MARGIN as i64;
    i32::try_from((n + 7) / 8 * 8).map_err(|_| E::Format("the shelf heap does not fit in 32 bits"))
}

pub fn add_rows(tables: &mut [Byml; 3], sh: &Shelf) -> Res<()> {
    let h: i32 = heap(&tables[0], &sh.look)?.max(heap(&tables[0], &sh.item).unwrap_or(0));
    let mut a: Node = rows::actor_row(rows::need(&tables[0], &sh.look, ACTOR_INFO)?, &sh.name, h);
    if let Some(project) = &sh.project {
        a.set("ModelProjectName", Node::Str(project.clone()));
    }
    if sh.look != sh.item {
        if let Some(n) = rows::find(&tables[0], &sh.item).and_then(|r| r.get("ActorName")).cloned() {
            a.set("ActorName", n);
        }
    }
    let g: Node = rows::game_row(rows::need(&tables[1], BASE, GAME_ACTOR_INFO)?, rows::need(&tables[1], &sh.look, GAME_ACTOR_INFO)?, &sh.name);
    let mut q: Node = rows::pouch_row(rows::need(&tables[2], &sh.who, POUCH_INFO)?, &sh.name);
    q.set("BuyingPrice", Node::I32(sh.price));
    let i: usize = rows::insert(&mut tables[0], a)?;
    let j: usize = rows::insert(&mut tables[1], g)?;
    if i != j {
        return Err(E::Msg(format!("the shelf row {} sits at {i} in the actor table and at {j} in the game actor table", sh.name)));
    }
    rows::insert(&mut tables[2], q)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(project: &str) -> Vec<u8> {
        let root: Node = Node::Map(vec![("FmdbName".to_string(), s("Item_Harp_Sheik")), ("ModelProjectName".to_string(), s(project))]);
        Byml { root, version: 2, big: false }.write().unwrap()
    }

    #[test]
    fn a_song_shelf_shows_the_coloured_harp_once_the_song_pack_carries_it() {
        use crate::song_models::{info_of, project_of, DONOR};
        let mut p: Pool = Pool::default();
        p.add(&info_of("SunSong"), &info(&project_of("SunSong"))).unwrap();
        assert_eq!(tinted_harp(&p, "Item_Melody_SunSong", DONOR), Some((format!("?{}", info_of("SunSong")), "Item_Harp_SunSong".to_string())));
        // Another model donor, another item, or a song pack still showing Sheik's harp: no change.
        assert_eq!(tinted_harp(&p, "Item_Melody_SunSong", "Obj_Ocarina_Time"), None);
        assert_eq!(tinted_harp(&p, "Item_Hookshot", DONOR), None);
        let mut plain: Pool = Pool::default();
        plain.add(&info_of("SunSong"), &info("Item_Harp_Sheik")).unwrap();
        assert_eq!(tinted_harp(&plain, "Item_Melody_SunSong", DONOR), None);
    }

    #[test]
    fn the_price_and_table_files_follow_the_shipped_shelf_names() {
        assert_eq!(shelf_identity("Item_Gauntlet_Goron"), "Armor_Child_Gauntlet_Goron");
        assert_eq!(shelf_identity("Armor_Adult_Gauntlet_Silver"), "Armor_Adult_Gauntlet_Silver");
        assert_eq!(price_file("Shop_OOTRR_Item_Boomerang_20"), "GameParameter/PriceParam/Shop_OOTRR_Item_Boomerang_20.game__pouchcontent__PriceParam.bgyml");
        assert!(table_file("X").ends_with("X.engine__actor__GameParameterTable.bgyml"));
    }

    #[test]
    fn the_heap_is_the_largest_shelf_heap_plus_the_margin() {
        let row = |id: &str, h: i32| Node::Map(vec![(rows::ROW_ID.to_string(), s(id)), (rows::HEAP.to_string(), Node::I32(h))]);
        let t: Byml = Byml { root: Node::Arr(vec![row("Item_Boomerang", 1000), row("Shop_Heart", 5000), row("Shop_Board_SoldOut", 90000), row("Shop_OOTRR_X_5", 80000)]), version: 7, big: false };
        assert_eq!(heap(&t, "Item_Boomerang").unwrap(), 5000 + rows::HEAP_MARGIN);
        assert!(heap(&t, "Item_None").is_err());
    }
}
