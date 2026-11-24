use std::collections::{BTreeMap as BM, BTreeSet as BS};
use std::fmt::Write as W;
use std::fs;
use std::path::{Path as P, PathBuf as PB};

use crate::byml::{Byml, Node};
use crate::error::{Error as E, Res};
use crate::flow_locations::dict_id;
use crate::modpack::{Host, Pack};
use crate::pickup_pack::actor::pouch_file;
use crate::pickup_pack::ammo;
use crate::pickup_pack::build::{extra_roots, plan_with_files, pool_of, Plan};
use crate::pickup_pack::closure::{actor_file, chain, select_with, texts, Pool};
use crate::pickup_pack::guard;
use crate::pickup_pack::magic;
use crate::pickup_pack::meter;
use crate::pickup_pack::place;
use crate::pickup_pack::rows::{self, ACTOR_INFO, GAME_ACTOR_INFO, POUCH_INFO};
use crate::pickup_pack::sizes;
use crate::pickup_pack::tag::{self, Table, TAG};
use crate::pickup_pack::{is_actor_name, pack_of, Layers, Spec, BASE, DONOR, PACK_DICT};
use crate::chest_slot::LIST;
use crate::placement::banc_files;
use crate::pots::KEPT;
use crate::slice::data::Data;
use crate::romfs::Romfs;
use crate::rsizetable::{self, Rst, TABLE_PATH};
use crate::sarc::{self, Entry};
use crate::staging::{BANC_DICT, MARGIN};
use crate::zs;
use crate::TITLE_ID;

#[derive(Clone, Debug)]
pub struct Opts {
    pub host: Host,
    pub base: Option<PB>,
    pub rst_from: Option<PB>,
    pub places: Vec<u64>,
    pub dry: bool,
}

#[derive(Clone, Debug)]
pub struct File {
    pub path: String,
    pub raw: Vec<u8>,
    pub z: Vec<u8>,
    pub old: usize,
    pub fresh: bool,
}

#[derive(Clone, Debug)]
pub struct Done {
    pub report: String,
    pub root: Option<PB>,
    pub files: Vec<File>,
    pub carried: Vec<File>,
    pub heap: i32,
    pub kept: Vec<File>,
}

pub const CARRY: [&str; 1] = [TAG];

pub(crate) fn base_dir(o: &Opts) -> Res<Option<PB>> {
    let Some(b) = o.base.as_ref() else {
        return Ok(None);
    };
    if !b.is_dir() {
        return Err(E::Msg(format!("the base folder {} does not exist, it must be the romfs folder of a mod", b.display())));
    }
    let inner: PB = b.join("romfs");
    if inner.is_dir() && !b.join("RSDB").is_dir() && !b.join("Pack").is_dir() {
        return Ok(Some(inner));
    }
    Ok(Some(b.clone()))
}

fn base_file(base: Option<&PB>, path: &str) -> Option<PB> {
    base.map(|b| b.join(path)).filter(|p| p.is_file())
}

pub(crate) fn start(rf: &mut Romfs, base: Option<&PB>, path: &str) -> Res<Vec<u8>> {
    match base_file(base, path) {
        Some(p) => rf.zs.decompress(&fs::read(p)?),
        None => rf.read_zs(path),
    }
}

fn old_len(rf: &mut Romfs, modded: bool, path: &str, first: &[u8]) -> Res<usize> {
    if modded {
        return Ok(first.len());
    }
    Ok(rf.read_zs(path)?.len())
}

pub(crate) fn table(rf: &mut Romfs, o: &Opts, modded: bool, path: &str, edit: impl FnOnce(&mut Byml) -> Res<()>) -> Res<File> {
    let first: Vec<u8> = start(rf, o.base.as_ref(), path)?;
    let mut y: Byml = Byml::parse(&first)?;
    edit(&mut y)?;
    let raw: Vec<u8> = y.write()?;
    let id: Option<u32> = dict_id(&rf.read(path)?);
    Ok(File { path: path.to_string(), z: zs::compress_raw(&raw, id)?, old: old_len(rf, modded, path, &first)?, raw, fresh: false })
}

pub(crate) fn tables(rf: &mut Romfs, o: &Opts, modded: bool, s: &Spec, l: &Layers, identity: &str) -> Res<(Vec<File>, i32)> {
    let source: &str = s.source();
    let seen: Byml = Byml::parse(&start(rf, o.base.as_ref(), ACTOR_INFO)?)?;
    let own: i32 = rows::own_heap(&seen, source)?;
    let heap: i32 = match s.heap {
        Some(h) if h < own => return Err(E::Msg(format!("the heap {h} is below the {own} that {source} needs by itself"))),
        Some(h) => h,
        None => rows::heap(&seen, source)?,
    };
    let mut at: [usize; 2] = [0, 0];
    let a: File = table(rf, o, modded, ACTOR_INFO, |y: &mut Byml| {
        let mut r: Node = rows::actor_row(rows::need(y, source, ACTOR_INFO)?, &s.name, heap);
        if let Some(m) = &l.model {
            crate::pickup_pack::model::row(&mut r, rows::need(y, m, ACTOR_INFO)?, l.pose.as_deref())?;
        }
        if l.ring.is_some() {




            r.set("ActorName", Node::Str(identity.to_string()));
            if let Some(elink) = l.pouch_from.as_deref() {
                r.set("ELinkUserName", Node::Str(elink.to_string()));
            }
        } else if l.own {
            r.set(rows::ACTOR_NAME, Node::Str(s.name.clone()));
        }
        at[0] = rows::insert(y, r)?;
        Ok(())
    })?;
    let g: File = table(rf, o, modded, GAME_ACTOR_INFO, |y: &mut Byml| {
        let r: Node = rows::game_row(rows::need(y, BASE, GAME_ACTOR_INFO)?, rows::need(y, source, GAME_ACTOR_INFO)?, &s.name);
        at[1] = rows::insert(y, r)?;
        Ok(())
    })?;
    if at[0] != at[1] {
        return Err(E::Msg(format!(
            "the new rows sit at {} in the actor table and at {} in the game actor table, the two tables no longer line up",
            at[0], at[1]
        )));
    }
    let q: File = table(rf, o, modded, POUCH_INFO, |y: &mut Byml| {
        let pouch_source: &str = l.pouch_from.as_deref().unwrap_or(source);
        let mut r: Node = rows::pouch_row(rows::need(y, pouch_source, POUCH_INFO)?, &s.name);
        if l.own {
            rows::quiet(&mut r);
        } else if s.name.starts_with(crate::slice::key_spots::DUNGEON_PREFIX) {
            r.remove("PouchSpecialDeal");
            r.set("PouchStockable", Node::Bool(true));
        }
        rows::insert(y, r)?;
        if identity != s.name && rows::find(y, identity).is_none() {
            let id: Node = rows::pouch_row(rows::need(y, pouch_source, POUCH_INFO)?, identity);
            rows::insert(y, id)?;
        }
        if l.ammo {
            ammo::apply(y, &s.name, identity)?;
        }
        if meter::applies(identity) {
            meter::apply(y, &s.name, identity)?;
        }
        Ok(())
    })?;
    Ok((vec![a, g, q], heap))
}

fn tag_file(rf: &mut Romfs, o: &Opts, modded: bool, s: &Spec, l: &Layers) -> Res<Option<File>> {
    if l.tags.is_empty() {
        return Ok(None);
    }
    let first: Vec<u8> = start(rf, o.base.as_ref(), TAG)?;
    let old: Table = Table::parse(&first)?;
    let raw: Vec<u8> = tag::add(&old, s.source(), &s.name, &l.tags)?.write()?;
    let id: Option<u32> = dict_id(&rf.read(TAG)?);
    Ok(Some(File { path: TAG.to_string(), z: zs::compress_raw(&raw, id)?, old: old_len(rf, modded, TAG, &first)?, raw, fresh: false }))
}

pub(crate) fn kept(rf: &mut Romfs, base: Option<&PB>, files: &[File]) -> Res<Vec<File>> {
    let mut out: Vec<File> = Vec::new();
    for path in CARRY {
        if files.iter().any(|f| f.path == path) {
            continue;
        }
        if let Some(p) = base_file(base, path) {
            let z: Vec<u8> = fs::read(p)?;
            let raw: Vec<u8> = rf.zs.decompress(&z)?;
            out.push(File { path: path.to_string(), old: raw.len(), raw, z, fresh: false });
        }
    }
    Ok(out)
}

fn spots(rf: &mut Romfs, o: &Opts, modded: bool, name: &str, note: &mut String) -> Res<Vec<File>> {
    let mut out: Vec<File> = Vec::new();
    if o.places.is_empty() {
        return Ok(out);
    }
    let list: Byml = Byml::parse(&start(rf, o.base.as_ref(), LIST)?)?;
    let d: Data = Data::load()?;
    for h in &o.places {
        place::single(&d, *h)?;
        place::permanent(&list, *h)?;
        let _ = writeln!(note, "spot {h} has a permanent saved row (reset type {KEPT}, no extra byte)");
    }
    let mut hit: BM<u64, usize> = BM::new();
    for f in banc_files(rf)? {
        let first: Vec<u8> = start(rf, o.base.as_ref(), &f)?;
        if !o.places.iter().any(|h| place::has_hash(&first, *h)) {
            continue;
        }
        let mut y: Byml = Byml::parse(&first)?;
        let mut n: usize = 0;
        for h in &o.places {
            if let Some(old) = place::swap(&mut y, *h, name)? {
                *hit.entry(*h).or_insert(0) += 1;
                n += 1;
                let _ = writeln!(note, "spot {h} in {f}: {old} -> {name}");
            }
        }
        if n == 0 {
            continue;
        }
        let raw: Vec<u8> = y.write()?;
        let id: u32 = dict_id(&rf.read(&f)?).unwrap_or(BANC_DICT);
        out.push(File { path: f.clone(), z: zs::compress_raw(&raw, Some(id))?, old: old_len(rf, modded, &f, &first)?, raw, fresh: false });
    }
    if let Some(h) = o.places.iter().find(|h| !hit.contains_key(h)) {
        return Err(E::Msg(format!("no placed actor has the hash {h}")));
    }
    Ok(out)
}

pub(crate) fn earlier(rf: &mut Romfs, base: Option<&PB>, own: &str) -> Res<Vec<File>> {
    let mut out: Vec<File> = Vec::new();
    let Some(dir) = base.map(|b| b.join("Pack").join("Actor")).filter(|p| p.is_dir()) else {
        return Ok(out);
    };
    let mut names: Vec<String> = Vec::new();
    for e in fs::read_dir(&dir)? {
        names.push(e?.file_name().to_string_lossy().to_string());
    }
    names.sort();
    for n in names.iter().filter(|n| n.ends_with(".pack.zs")) {
        let path: String = format!("Pack/Actor/{n}");
        if path == own || rf.root.join("Pack").join("Actor").join(n).is_file() {
            continue;
        }
        let z: Vec<u8> = fs::read(dir.join(n))?;
        let raw: Vec<u8> = rf.zs.decompress(&z)?;
        out.push(File { path, old: raw.len(), raw, z, fresh: false });
    }
    Ok(out)
}

pub fn actor_of(path: &str) -> &str {
    path.trim_start_matches("Pack/Actor/").trim_end_matches(".pack.zs")
}

pub(crate) fn size_table(rf: &mut Romfs, o: &Opts) -> Res<(Rst, bool)> {
    let from: Option<PB> = o.rst_from.clone().or_else(|| base_file(o.base.as_ref(), TABLE_PATH));
    match from {
        Some(p) => Ok((Rst::parse(&rf.zs.decompress(&fs::read(p)?)?)?, true)),
        None => Ok((rsizetable::load(rf)?, false)),
    }
}

pub(crate) fn sized(rst: &mut Rst, pl: &Plan, files: &[File], carried: &[File], note: &mut String) -> Res<()> {
    for c in carried {
        let n: u32 = sizes::pack_size(c.raw.len())?;
        rst.set_size(&c.path, n)?;
        let _ = writeln!(note, "{}: earlier pack carried from the base, {} bytes, size table entry {n}", c.path, c.raw.len());
    }
    for f in files {
        if f.fresh {
            let n: u32 = sizes::pack_size(f.raw.len())?;
            rst.set_size(&f.path, n)?;
            let _ = writeln!(note, "{}: new, {} bytes, size table entry {n}", f.path, f.raw.len());
            continue;
        }
        let was: Option<u32> = rst.get(&f.path);
        let now: Option<u32> = rst.grow(&f.path, f.old as u64, f.raw.len() as u64, MARGIN)?;
        let _ = writeln!(note, "{}: {} -> {} bytes, size table entry {:?} -> {:?}", f.path, f.old, f.raw.len(), was, now);
    }
    for (f, len) in &pl.fresh {
        let n: u32 = if sizes::kind(f) == "event.root.ainb" {
            sizes::event_size(*len)?
        } else {
            sizes::file_size(rst, &pl.peers, f, *len)?
        };
        rst.set_size(f, n)?;
        let _ = writeln!(note, "{f}: new file in the pack, {len} bytes, size table entry {n}");
    }
    Ok(())
}

fn text<'a>(n: &'a Node, k: &str) -> &'a str {
    n.get(k).and_then(|v| v.as_str()).unwrap_or("")
}

fn audit_carried(rst: &Rst, tables: &[Byml], carried: &[File]) -> Res<Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    for c in carried {
        let name: &str = actor_of(&c.path);
        if rst.get(&c.path) != Some(sizes::pack_size(c.raw.len())?) {
            return Err(E::Msg(format!("audit: the earlier pack {name} has the wrong size table entry")));
        }
        let held: Vec<String> = sarc::parse(&c.raw)?.into_iter().map(|e| e.name).collect();
        let pouch: bool = held.contains(&pouch_file(name));
        if let Some(f) = [actor_file(name), pouch_file(name)].iter().filter(|f| held.contains(f)).find(|f| rst.get(f).is_none()) {
            return Err(E::Msg(format!("audit: {f} of the earlier pack {name} has no size table entry, the size table does not come from the mod that made that pack")));
        }
        if tables.iter().take(if pouch { tables.len() } else { 2 }).any(|y| rows::find(y, name).is_none()) {
            return Err(E::Msg(format!("audit: the earlier pack {name} has lost a table row")));
        }
        out.push(format!("earlier pack {name} keeps its three rows and its size table entries"));
    }
    Ok(out)
}

pub struct Scope<'a> {
    pub carried: &'a [File],
    pub places: &'a [u64],
    pub base: Option<&'a PB>,
    pub layers: &'a Layers,
    pub kept: &'a [File],
}

fn audit_layers(rf: &mut Romfs, s: &Spec, pl: &Plan, files: &[File], rst: &Rst, p: &Pool, sc: &Scope) -> Res<Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    let l: &Layers = sc.layers;
    let written: Option<&File> = files.iter().find(|f| f.path == TAG);
    match (l.tags.is_empty(), written) {
        (true, Some(_)) => return Err(E::Format("audit: a tag table was written with no tag asked for")),
        (false, None) => return Err(E::Format("audit: the tag table was not written")),
        (false, Some(f)) => {
            let now: Table = Table::parse(&rf.zs.decompress(&f.z)?)?;
            let old: Table = Table::parse(&start(rf, sc.base, TAG)?)?;
            if now.write()? != f.raw {
                return Err(E::Format("audit: the tag table does not read back to its own bytes"));
            }
            if (rst.get(TAG).unwrap_or(0) as usize) < f.raw.len() {
                return Err(E::Format("audit: the tag table has a size table entry below its size"));
            }
            out.push(tag::audit(&old, &now, &s.name, &l.tags)?);
        }
        (true, None) => {}
    }
    for k in sc.kept {
        if start(rf, sc.base, &k.path)? != k.raw || (rst.get(&k.path).unwrap_or(0) as usize) < k.raw.len() {
            return Err(E::Msg(format!("audit: {} from the base is not carried as it was, or the size table does not come from the mod that made it", k.path)));
        }
        out.push(format!("{} carried from the base unchanged, {} bytes", k.path, k.raw.len()));
    }
    let own: String = magic::table_file(&s.name);
    let points: bool = chain(p, &s.name)?.comps.get(magic::TABLE_REF).map(|v| v.as_str()) == Some(format!("?{own}").as_str());
    let held: bool = p.get(&own).is_some() && pl.fresh.iter().any(|f| f.0 == own);
    if l.magic != points || l.magic != held {
        return Err(E::Format("audit: the game parameter table of the pack does not match the magic layer"));
    }
    if l.magic {
        let y: Byml = p.byml(&own)?;
        let r: &str = y.root.get("Components").and_then(|c| c.get(magic::ABILITY)).and_then(|v| v.as_str()).unwrap_or("");
        out.push(format!("magic layer: {own} inherits the pickup base table and adds {r}"));
    }
    Ok(out)
}

pub fn audit(rf: &mut Romfs, s: &Spec, pl: &Plan, files: &[File], rst_z: &[u8], sc: &Scope) -> Res<Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    let get = |p: &str| files.iter().find(|f| f.path == p).ok_or_else(|| E::Msg(format!("audit: {p} was not written")));
    let rst: Rst = Rst::parse(&rf.zs.decompress(rst_z)?)?;
    let pk: &File = get(&pack_of(&s.name))?;
    let raw: Vec<u8> = rf.zs.decompress(&pk.z)?;
    let es: Vec<Entry> = sarc::parse(&raw)?;
    let p: Pool = pool_of(&[es.clone()])?;
    let mut names: Vec<String> = es.iter().map(|e| e.name.clone()).collect();
    names.sort();
    if select_with(&p, &s.name, &extra_roots(&p, sc.layers)?)? != names {
        return Err(E::Format("audit: the written pack holds a file its actor does not reach"));
    }
    for e in &es {
        if texts(&e.data).is_err() && !crate::pickup_pack::kit::leaf(&e.data) {
            return Err(E::Msg(format!("audit: {} does not read back", e.name)));
        }
    }
    if rst.get(&pk.path) != Some(sizes::pack_size(raw.len())?) {
        return Err(E::Format("audit: the pack has the wrong size table entry"));
    }
    out.push(format!("pack reads back: {} files, {} bytes, dictionary {:?}, size entry {:?}", es.len(), raw.len(), dict_id(&pk.z), rst.get(&pk.path)));
    for (f, len) in &pl.fresh {
        let n: u32 = rst.get(f).ok_or_else(|| E::Msg(format!("audit: {f} has no size table entry")))?;
        if (n as usize) < *len {
            return Err(E::Msg(format!("audit: {f} has a size table entry below its size")));
        }
    }
    let mut now: Vec<Byml> = Vec::new();
    for t in [ACTOR_INFO, GAME_ACTOR_INFO, POUCH_INFO] {
        let f: &File = get(t)?;
        let y: Byml = Byml::parse(&rf.zs.decompress(&f.z)?)?;
        let was: Byml = Byml::parse(&start(rf, sc.base, t)?)?;
        let n: usize = y.root.as_arr().map(|a| a.len()).unwrap_or(0);
        let m: usize = was.root.as_arr().map(|a| a.len()).unwrap_or(0);
        let had: usize = usize::from(rows::find(&was, &s.name).is_some());



        let extra: usize = usize::from(t == POUCH_INFO && pl.identity != s.name && rows::find(&was, &pl.identity).is_none());
        let r: &Node = rows::need(&y, &s.name, t)?;
        if !rows::sorted(&y)? || n + had != m + 1 + extra {
            let want: &str = if extra == 0 { "plus one sorted row" } else { "plus the expected sorted rows" };
            return Err(E::Msg(format!("audit: {t} is not the table it started from {want}")));
        }
        let index: std::collections::HashMap<&str, &Node> = rows::index(&y);
        let eased = |x: &Node| t == POUCH_INFO && ((sc.layers.ammo && rows::id(x) == Some(pl.identity.as_str())) || (meter::applies(&pl.identity) && meter::loses(x)));
        let kept: usize = was
            .root
            .as_arr()
            .unwrap_or(&[])
            .iter()
            .filter(|x| rows::id(x) != Some(s.name.as_str()))
            .filter(|x| rows::id(x).and_then(|i| index.get(i).copied()) == Some(*x) || eased(x))
            .count();
        if kept + had != m {
            return Err(E::Msg(format!("audit: {t} changed {} rows it started with", m - had - kept)));
        }
        if (rst.get(t).unwrap_or(0) as usize) < f.raw.len() {
            return Err(E::Msg(format!("audit: {t} has a size table entry below its size")));
        }
        let fact: String = match t {
            ACTOR_INFO => format!("ActorName {} heap {:?} model {}", text(r, "ActorName"), r.get(rows::HEAP), text(r, "FmdbName")),
            GAME_ACTOR_INFO => format!("IsRespawn {:?} layer {}", r.get(rows::RESPAWN), text(r, rows::LAYER)),
            _ => format!("deal {} category {}", text(r, "PouchSpecialDeal"), text(r, "PouchCategory")),
        };
        out.push(format!("{t}: {m} -> {n} rows, {kept} earlier rows unchanged, new row {fact}"));
        if t == POUCH_INFO && sc.layers.ammo {
            out.push(ammo::audit(&was, &y, &s.name, &pl.identity)?);
        }
        if t == POUCH_INFO && meter::applies(&pl.identity) {
            out.push(meter::audit(&was, &y, &s.name, &pl.identity)?);
        }
        now.push(y);
    }
    out.extend(audit_layers(rf, s, pl, files, &rst, &p, sc)?);
    out.extend(audit_carried(&rst, &now, sc.carried)?);
    for h in sc.places {
        let n: usize = files
            .iter()
            .filter(|f| f.path.starts_with("Banc/"))
            .filter(|f| Byml::parse(&f.raw).ok().and_then(|y| place::current(&y, *h)).as_deref() == Some(s.name.as_str()))
            .count();
        if n == 0 {
            return Err(E::Msg(format!("audit: spot {h} does not hold {}", s.name)));
        }
        out.push(format!("spot {h} holds {} in {n} file(s)", s.name));
    }
    Ok(out)
}

pub fn run(romfs: &P, load: &P, mod_name: &str, s: &Spec, o: &Opts) -> Res<Done> {
    run_with(romfs, load, mod_name, s, o, &Layers::default())
}

pub fn run_with(romfs: &P, load: &P, mod_name: &str, s: &Spec, o: &Opts, l: &Layers) -> Res<Done> {
    if !is_actor_name(&s.name) || !is_actor_name(&s.item) || !is_actor_name(s.source()) || s.identity.as_deref().is_some_and(|id| !is_actor_name(id)) {
        return Err(E::Msg(format!("{}, {} or an override is not a plain actor name", s.name, s.item)));
    }
    let source: &str = s.source();
    let o: &Opts = &Opts { base: base_dir(o)?, ..o.clone() };
    let mut rf: Romfs = Romfs::open(romfs)?;
    if rf.root.join(pack_of(&s.name)).is_file() {
        return Err(E::Msg(format!("the game already ships {}, use that actor", s.name)));
    }
    if !rf.root.join(pack_of(source)).is_file() {
        return Err(E::Msg(format!("the game has no actor pack for {source}")));
    }
    let posed: Layers = crate::pickup_pack::model::layers(&mut rf, l)?;
    let lifted: Layers = crate::pickup_pack::lift::layers(&mut rf, source, &posed)?;
    let l: &Layers = &lifted;
    let mut packs: Vec<Vec<Entry>> = Vec::new();
    let mut seen: BS<&str> = BS::new();
    for n in [BASE, DONOR, source]
        .into_iter()
        .chain(l.pouch_from.as_deref())



        .chain(l.ring.as_deref().filter(|_| !l.drop_ring).map(|_| crate::pickup_pack::actor::EVENT_BASE))
    {
        if seen.insert(n) {
            packs.push(sarc::parse(&start(&mut rf, o.base.as_ref(), &pack_of(n))?)?);
        }
    }


    if let Some(m) = l.model.as_deref() {
        if seen.insert(m) {
            let es: Vec<Entry> = sarc::parse(&start(&mut rf, o.base.as_ref(), &pack_of(m))?)?;
            packs.push(es.into_iter().filter(|e| crate::pickup_pack::closure::is_byml(&e.data)).collect());
        }
    }
    let mut extra: Vec<(String, Vec<u8>)> = Vec::new();
    if let Some(who) = l.ring.as_deref() {
        if !crate::pickup_pack::is_actor_name(who) || s.identity.as_deref() != Some(who) {
            return Err(E::Msg(format!("{who} must be a plain actor name and the pack identity")));
        }


        if !l.drop_ring {
            let donor: Vec<u8> = crate::ring_event::donor(&mut rf)?;
            extra.push((format!("AI/{who}.event.root.ainb"), crate::ring_event::make(who, &donor)?));
        }
    }
    let pl: Plan = plan_with_files(&packs, s, l, &extra)?;
    let (mut rst, modded): (Rst, bool) = size_table(&mut rf, o)?;
    let mut note: String = String::new();
    let id: u32 = dict_id(&rf.read(&pack_of(BASE))?).unwrap_or(PACK_DICT);
    let mut files: Vec<File> = vec![File { path: pack_of(&s.name), z: zs::compress_raw(&pl.sarc, Some(id))?, raw: pl.sarc.clone(), old: 0, fresh: true }];
    let (ts, heap): (Vec<File>, i32) = tables(&mut rf, o, modded, s, l, &pl.identity)?;
    rows::need(&Byml::parse(&ts[2].raw)?, &pl.identity, POUCH_INFO)?;
    let _ = writeln!(note, "pouch identity {}, the name the save and the get window use", pl.identity);
    files.extend(ts);
    files.extend(tag_file(&mut rf, o, modded, s, l)?);
    files.extend(spots(&mut rf, o, modded, &s.name, &mut note)?);
    let carried: Vec<File> = earlier(&mut rf, o.base.as_ref(), &pack_of(&s.name))?;
    let kept: Vec<File> = kept(&mut rf, o.base.as_ref(), &files)?;
    sized(&mut rst, &pl, &files, &carried, &mut note)?;
    let rst_z: Vec<u8> = rst.to_zs()?;
    let sc: Scope = Scope { carried: &carried, places: &o.places, base: o.base.as_ref(), layers: l, kept: &kept };
    for line in audit(&mut rf, s, &pl, &files, &rst_z, &sc)? {
        let _ = writeln!(note, "{line}");
    }


    let guard_line: String = if l.ring.is_some() && !l.drop_ring {
        format!("{} is a saved key item, it does not carry a world pickup death table", s.name)
    } else {
        guard::check(&mut rf, &s.name, &files, &carried)?
    };
    let _ = writeln!(note, "{guard_line}");
    let mut root: Option<PB> = None;
    if !o.dry {
        let pk: Pack = Pack::create_for(load, TITLE_ID, mod_name, o.host)?;
        for f in files.iter().chain(carried.iter()).chain(kept.iter()) {
            pk.write_romfs(&f.path, &f.z)?;
        }
        pk.write_romfs(TABLE_PATH, &rst_z)?;
        let _ = writeln!(note, "wrote {} files to {}", files.len() + carried.len() + kept.len() + 1, pk.root.display());
        root = Some(pk.root);
    }
    Ok(Done { report: note, root, files, carried, heap, kept })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pickup_pack::build::plan_with;
    use crate::pickup_pack::Style;

    fn real() -> Option<PB> {
        let p: PB = ootrr_game::test_romfs::path();
        if p.join("Pack").join("ZsDic.pack.zs").is_file() {
            Some(p)
        } else {
            eprintln!("skipped, no RomFS dump at {} (set OOTRR_ROMFS)", p.display());
            None
        }
    }

    fn hookshot() -> Spec {
        Spec { item: "Item_Hookshot".to_string(), name: "Obj_Hookshot".to_string(), style: Style::Touch, heap: None, source: None, identity: None }
    }

    fn opts(places: Vec<u64>) -> Opts {
        Opts { host: Host::Hookshot, base: None, rst_from: None, places, dry: true }
    }

    const SPOT: u64 = 11219803099333988363;

    #[test]
    fn the_three_real_tables_round_trip_and_stay_in_step() {
        let Some(root) = real() else {
            return;
        };
        let mut rf: Romfs = Romfs::open(&root).unwrap();
        let mut ids: Vec<Vec<String>> = Vec::new();
        for t in [ACTOR_INFO, GAME_ACTOR_INFO, POUCH_INFO] {
            let raw: Vec<u8> = rf.read_zs(t).unwrap();
            let y: Byml = Byml::parse(&raw).unwrap();
            assert_eq!(y.write().unwrap(), raw, "{t}");
            assert!(rows::sorted(&y).unwrap(), "{t}");
            ids.push(y.root.as_arr().unwrap().iter().filter_map(rows::id).map(|s| s.to_string()).collect());
        }
        assert_eq!(ids[0].len(), 8363);
        assert_eq!(ids[0], ids[1]);
        assert_eq!(ids[2].len(), 395);
        assert!(ids[2].iter().all(|i| ids[0].contains(i)));
    }

    #[test]
    fn every_real_pack_entry_follows_the_pack_size_rule() {
        let Some(root) = real() else {
            return;
        };
        let mut rf: Romfs = Romfs::open(&root).unwrap();
        let rst: Rst = rsizetable::load(&mut rf).unwrap();
        let all: Vec<String> = rf.list("Pack/Actor").unwrap();
        let mut n: usize = 0;
        for f in all.iter().filter(|f| f.ends_with(".pack.zs")).step_by(7) {
            let len: usize = rf.read_zs(f).unwrap().len();
            assert_eq!(rst.get(f), Some(sizes::pack_size(len).unwrap()), "{f}");
            n += 1;
        }
        assert!(n > 1000, "{n}");
    }

    #[test]
    fn the_real_hookshot_build_adds_one_row_per_table_and_changes_nothing_else() {
        let Some(root) = real() else {
            return;
        };
        let d: Done = run(&root, P::new("unused"), "unused", &hookshot(), &opts(vec![SPOT])).unwrap();
        assert!(d.root.is_none());
        let mut rf: Romfs = Romfs::open(&root).unwrap();
        let paths: Vec<&str> = d.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(
            paths,
            vec!["Pack/Actor/Obj_Hookshot.pack.zs", ACTOR_INFO, GAME_ACTOR_INFO, POUCH_INFO, "Banc/KokiriForest/KokiriForest_Child_Dynamic.bcett.byml.zs"]
        );
        for f in &d.files {
            assert_eq!(rf.zs.decompress(&f.z).unwrap(), f.raw, "{}", f.path);
        }
        assert_eq!(dict_id(&d.files[0].z), Some(PACK_DICT));
        for (i, t) in [ACTOR_INFO, GAME_ACTOR_INFO, POUCH_INFO].iter().enumerate() {
            let was: Byml = rf.read_byml(t).unwrap();
            let mut now: Byml = Byml::parse(&d.files[i + 1].raw).unwrap();
            let at: usize = now.root.as_arr().unwrap().iter().position(|r| rows::id(r) == Some("Obj_Hookshot")).unwrap();
            let new: Node = now.root.as_arr_mut().unwrap().remove(at);
            assert_eq!(now, was, "{t}");
            let item: &Node = rows::find(&was, "Item_Hookshot").unwrap();
            match i {
                0 => {
                    assert_eq!(text(&new, "ActorName"), "Item_Hookshot");
                    assert_eq!(text(&new, "FmdbName"), "Item_Hookshot_Body");
                    assert_eq!(new.get(rows::HEAP), Some(&Node::I32(d.heap)));
                    assert_eq!(d.heap, 167160 + rows::HEAP_MARGIN);
                    assert_eq!(new.get("ModelAabbMax"), item.get("ModelAabbMax"));
                    assert_eq!(new.get("PreActorRenderInfoHeapSize"), item.get("PreActorRenderInfoHeapSize"));
                }
                1 => {
                    assert_eq!(new.get(rows::RESPAWN), Some(&Node::Bool(true)));
                    assert_eq!(text(&new, rows::LAYER), "SmallObject");
                    assert_eq!(new.get(rows::VARIATION), item.get(rows::VARIATION));
                    assert!(new.get(rows::VARIATION).is_some());
                }
                _ => {
                    let mut same: Node = new.clone();
                    same.set(rows::ROW_ID, Node::Str("Item_Hookshot".to_string()));
                    assert_eq!(&same, item);
                }
            }
        }
        let was: Byml = rf.read_byml(&d.files[4].path).unwrap();
        let now: Byml = Byml::parse(&d.files[4].raw).unwrap();
        let a: &[Node] = was.root.get("Actors").unwrap().as_arr().unwrap();
        let b: &[Node] = now.root.get("Actors").unwrap().as_arr().unwrap();
        assert_eq!(a.len(), b.len());
        let diff: Vec<usize> = (0..a.len()).filter(|i| a[*i] != b[*i]).collect();
        assert_eq!(diff.len(), 1);
        assert_eq!(text(&a[diff[0]], "Gyaml"), "Obj_Rupee_Green");
        assert_eq!(text(&b[diff[0]], "Gyaml"), "Obj_Hookshot");
        let mut back: Node = b[diff[0]].clone();
        back.set("Gyaml", Node::Str("Obj_Rupee_Green".to_string()));
        assert_eq!(back, a[diff[0]]);
        let mut rest: Byml = now.clone();
        rest.root.get_mut("Actors").unwrap().as_arr_mut().unwrap()[diff[0]] = a[diff[0]].clone();
        assert_eq!(rest, was);
        assert!(d.report.contains("pack reads back: 40 files"));
    }

    #[test]
    fn the_real_ring_build_uses_one_row_per_table_for_the_ring_identity() {
        let Some(root) = real() else {
            return;
        };
        let s: Spec = Spec {
            item: "Item_Ring_Forest".to_string(),
            name: "Item_Ring_Forest".to_string(),
            style: Style::Touch,
            heap: None,
            source: Some("Item_Dungeon_SmallKey".to_string()),
            identity: Some("Item_Ring_Forest".to_string()),
        };
        let l: Layers = Layers {
            pouch_from: Some("Item_Medal_Forest".to_string()),
            ring: Some("Item_Ring_Forest".to_string()),
            ..Layers::default()
        };
        let mut rf: Romfs = Romfs::open(&root).unwrap();
        let o: Opts = opts(Vec::new());
        let d: Done = run_with(&root, P::new("unused"), "unused", &s, &o, &l).unwrap();
        for t in [ACTOR_INFO, GAME_ACTOR_INFO, POUCH_INFO] {
            let was: Byml = rf.read_byml(t).unwrap();
            let now: Byml = Byml::parse(&file(&d, t).raw).unwrap();
            let was_n: usize = was.root.as_arr().unwrap().len();
            let now_n: usize = now.root.as_arr().unwrap().len();
            assert_eq!(now_n, was_n + 1, "{t}");
            assert!(rows::find(&now, "Item_Ring_Forest").is_some(), "{t}");
        }
        let actors: Byml = Byml::parse(&file(&d, ACTOR_INFO).raw).unwrap();
        let actor: &Node = rows::find(&actors, "Item_Ring_Forest").unwrap();
        assert_eq!(text(actor, "ActorName"), "Item_Ring_Forest");
        assert_eq!(text(actor, "ELinkUserName"), "Item_Medal_Forest");

        let es: Vec<Entry> = sarc::parse(&file(&d, &pack_of("Item_Ring_Forest")).raw).unwrap();
        let names: BS<&str> = es.iter().map(|e| e.name.as_str()).collect();
        for f in [
            "Actor/Item_Ring_Forest.engine__actor__ActorParam.bgyml",
            "Component/EventPerformerParam/Item_Ring_Forest.engine__component__EventPerformerParam.bgyml",
            "Component/EventPerformerParam/PouchContent.engine__component__EventPerformerParam.bgyml",
            "Component/ModelInfo/Item_Ring_Forest.engine__component__ModelInfo.bgyml",
            "Component/PouchContentParam/Item_Ring_Forest.game__component__PouchContentParam.bgyml",
            "AI/Item_Ring_Forest.event.root.ainb",
        ] {
            assert!(names.contains(f), "{f} is missing from the ring pack");
        }
        for f in [
            "Component/ASInfo/Item_Ring_Forest.engine__component__ASInfo.bgyml",
            "Component/CaptureParam/Item_Ring_Forest.game__component__CaptureParam.bgyml",
            "Component/ELink/Item_Ring_Forest.engine__component__ELinkParam.bgyml",
            "Component/Physics/Item_Ring_Forest.engine__component__PhysicsParam.bgyml",
        ] {
            assert!(!names.contains(f), "{f} belongs to a world pickup, not a ring");
        }

        let actor_file_data: Byml = in_pack(&d, "Item_Ring_Forest", &actor_file("Item_Ring_Forest")).unwrap();
        assert_eq!(actor_file_data.root.get("$parent").and_then(Node::as_str), Some("Work/Actor/Item_EventObject_Base.engine__actor__ActorParam.gyml"));
        assert_eq!(comp(&d, "Item_Ring_Forest", "ActorNameRef").as_deref(), Some("?ActorSystem/ActorName/Item_Ring_Forest.engine__actor__ActorName.bgyml"));
        assert_eq!(comp(&d, "Item_Ring_Forest", "EventPerformerRef").as_deref(), Some("?Component/EventPerformerParam/Item_Ring_Forest.engine__component__EventPerformerParam.bgyml"));
        assert_eq!(comp(&d, "Item_Ring_Forest", "ModelInfoRef").as_deref(), Some("?Component/ModelInfo/Item_Ring_Forest.engine__component__ModelInfo.bgyml"));
        assert_eq!(comp(&d, "Item_Ring_Forest", "PouchContentRef").as_deref(), Some("?Component/PouchContentParam/Item_Ring_Forest.game__component__PouchContentParam.bgyml"));
        assert!(comp(&d, "Item_Ring_Forest", "ELinkRef").unwrap().contains("Component/ELink/"), "{}", comp(&d, "Item_Ring_Forest", "ELinkRef").unwrap());
        assert!(comp(&d, "Item_Ring_Forest", "XLinkRef").is_none());
        assert!(comp(&d, "Item_Ring_Forest", "ASInfoRef").is_none());
        assert!(comp(&d, "Item_Ring_Forest", "AttentionSenderRef").is_none());

        let performer: Byml = in_pack(&d, "Item_Ring_Forest", "Component/EventPerformerParam/Item_Ring_Forest.engine__component__EventPerformerParam.bgyml").unwrap();
        assert_eq!(performer.root.get("EventUniqueAin").and_then(Node::as_str), Some("Work/AI/Root/Event/Item_Ring_Forest.event.root.ain"));
        assert_eq!(performer.root.get("UseEventUniqueAin"), Some(&Node::Bool(true)));
        let model: Byml = in_pack(&d, "Item_Ring_Forest", "Component/ModelInfo/Item_Ring_Forest.engine__component__ModelInfo.bgyml").unwrap();
        assert_eq!(model.root.get("FmdbName").and_then(Node::as_str), Some("Obj_Dungeon_SmallKey"));
        assert_eq!(model.root.get("ModelProjectName").and_then(Node::as_str), Some("Obj_Dungeon_SmallKey"));
        let pouch: Byml = in_pack(&d, "Item_Ring_Forest", "Component/PouchContentParam/Item_Ring_Forest.game__component__PouchContentParam.bgyml").unwrap();
        assert_eq!(pouch.root.get("$parent").and_then(Node::as_str), Some("Work/Component/PouchContentParam/Default.game__component__PouchContentParam.gyml"));
        assert!(pouch.root.get("PouchActor").is_none());
        assert!(pouch.root.get("IsCountCheckOnFirstGet").is_none());
        let ain: &[u8] = &sarc::get(&es, "AI/Item_Ring_Forest.event.root.ainb").unwrap().data;
        let ain: String = String::from_utf8_lossy(ain).into_owned();
        assert!(ain.contains("IsGet.Item_Ring_Forest"));
        assert!(!ain.contains("Item_KeyRing_"));
    }

    #[test]
    fn all_real_ring_packs_pass_the_persistence_guard() {
        let Some(root) = real() else {
            return;
        };
        let catalog: Vec<crate::pickup_pack::catalog::Entry> = crate::pickup_pack::catalog::load().unwrap();
        for r in crate::key_rings::all_with_hideout() {
            let e: &crate::pickup_pack::catalog::Entry = catalog.iter().find(|e| e.item == r.actor).unwrap();
            let (s, mut l): (Spec, Layers) = crate::pickup_pack::catalog::spec(r.actor, &e.args).unwrap();
            l.spin = true;
            let d: Done = run_with(&root, P::new("unused"), "unused", &s, &opts(Vec::new()), &l).unwrap();
            assert!(d.report.contains("is a saved key item"), "{}: {}", r.actor, d.report);
        }
    }

    #[test]
    fn the_drop_ring_build_uses_the_generic_get_flow_on_a_world_pickup_body() {
        let Some(root) = real() else {
            return;
        };
        let r: &'static crate::key_rings::Ring = &crate::key_rings::RINGS[0];
        let catalog: Vec<crate::pickup_pack::catalog::Entry> = crate::pickup_pack::catalog::load().unwrap();
        let e: &crate::pickup_pack::catalog::Entry = catalog.iter().find(|e| e.item == r.drop_item).unwrap();
        let (s, mut l): (Spec, Layers) = crate::pickup_pack::catalog::spec(&e.item, &e.args).unwrap();
        l.spin = true;
        let d: Done = run_with(&root, P::new("unused"), "unused", &s, &opts(Vec::new()), &l).unwrap();

        for t in [ACTOR_INFO, GAME_ACTOR_INFO, POUCH_INFO] {
            let now: Byml = Byml::parse(&file(&d, t).raw).unwrap();
            assert!(rows::find(&now, &s.name).is_some(), "{t} has no {}", s.name);
        }
        let pouch: Byml = Byml::parse(&file(&d, POUCH_INFO).raw).unwrap();
        assert!(rows::find(&pouch, r.actor).is_some(), "the saved ring identity has no pouch row");
        let actors: Byml = Byml::parse(&file(&d, ACTOR_INFO).raw).unwrap();
        let actor: &Node = rows::find(&actors, &s.name).unwrap();
        assert_eq!(text(actor, "ActorName"), r.actor);
        assert_eq!(text(actor, "ELinkUserName"), "Item_Medal_Forest");

        let es: Vec<Entry> = sarc::parse(&file(&d, &pack_of(&s.name)).raw).unwrap();
        let names: BS<&str> = es.iter().map(|e| e.name.as_str()).collect();
        for f in [
            format!("Actor/{}.engine__actor__ActorParam.bgyml", s.name),
            format!("Component/ASInfo/{}.engine__component__ASInfo.bgyml", s.name),
            format!("Component/PouchContentParam/{}.game__component__PouchContentParam.bgyml", s.name),
        ] {
            assert!(names.contains(f.as_str()), "{f} is missing from the drop ring pack");
        }
        let actor_file_data: Byml = in_pack(&d, &s.name, &actor_file(&s.name)).unwrap();
        assert_eq!(actor_file_data.root.get("$parent").and_then(Node::as_str), Some("Work/Actor/Obj_Item_Base.engine__actor__ActorParam.gyml"));
        assert!(comp(&d, &s.name, "ActorNameRef").unwrap().contains(r.actor));
        assert!(comp(&d, &s.name, "ASInfoRef").unwrap().contains(&s.name));
        assert!(comp(&d, &s.name, "ELinkRef").is_none());


        assert!(comp(&d, &s.name, "EventPerformerRef").is_none(), "a drop ring must not override the generic get performer");
        assert!(
            !names.contains(format!("Component/EventPerformerParam/{}.engine__component__EventPerformerParam.bgyml", s.name).as_str()),
            "a drop ring must not ship a private get performer"
        );
        assert!(!names.contains(format!("AI/{}.event.root.ainb", r.actor).as_str()), "a drop ring must not ship a private get event");

        let line: String = format!("{} stays collected once taken", s.name);
        assert!(d.report.contains(&line), "{}", d.report);
        let mut rf: Romfs = Romfs::open(&root).unwrap();
        assert!(guard::check(&mut rf, &s.name, &d.files, &d.carried).is_ok());
    }

    #[test]
    fn the_real_build_refuses_a_shipped_name_a_missing_item_and_a_chest() {
        let Some(root) = real() else {
            return;
        };
        let taken: Spec = Spec { name: "Obj_Arrow_Fire".to_string(), ..hookshot() };
        assert!(run(&root, P::new("unused"), "unused", &taken, &opts(Vec::new())).is_err());
        let none: Spec = Spec { item: "Item_Nothing_Here".to_string(), ..hookshot() };
        assert!(run(&root, P::new("unused"), "unused", &none, &opts(Vec::new())).is_err());
        assert!(run(&root, P::new("unused"), "unused", &hookshot(), &opts(vec![12786193094758345086])).is_err());
        assert!(run(&root, P::new("unused"), "unused", &hookshot(), &opts(vec![1])).is_err());
        let mut lost: Opts = opts(Vec::new());
        lost.base = Some(PB::from("no/such/mod/romfs"));
        assert!(run(&root, P::new("unused"), "unused", &hookshot(), &lost).unwrap_err().to_string().contains("does not exist"));
    }

    #[test]
    fn a_second_run_on_top_of_the_first_mod_is_stable() {
        let Some(root) = real() else {
            return;
        };
        let load: PB = std::env::temp_dir().join(format!("ootrr_pack_add_{}", std::process::id()));
        let _ = fs::remove_dir_all(&load);
        let mut o: Opts = opts(vec![SPOT]);
        o.dry = false;
        let one: Done = run(&root, &load, "one", &hookshot(), &o).unwrap();
        let first: PB = one.root.clone().unwrap();
        assert!(first.join("romfs").join("Pack").join("Actor").join("Obj_Hookshot.pack.zs").is_file());
        assert!(first.join("romfs").join(TABLE_PATH).is_file());
        assert!(first.ends_with("one"));
        o.base = Some(first.join("romfs"));
        let two: Done = run(&root, &load, "two", &hookshot(), &o).unwrap();
        for (a, b) in one.files.iter().zip(two.files.iter()) {
            assert_eq!(a.path, b.path);
            assert_eq!(a.raw, b.raw, "{}", a.path);
        }
        let ta: Vec<u8> = fs::read(first.join("romfs").join(TABLE_PATH)).unwrap();
        let tb: Vec<u8> = fs::read(two.root.unwrap().join("romfs").join(TABLE_PATH)).unwrap();
        assert_eq!(ta, tb);
        let _ = fs::remove_dir_all(&load);
    }
    fn snapshot(d: &P) -> BM<String, Vec<u8>> {
        let mut out: BM<String, Vec<u8>> = BM::new();
        let mut stack: Vec<PB> = vec![d.to_path_buf()];
        while let Some(x) = stack.pop() {
            for e in fs::read_dir(&x).unwrap() {
                let p: PB = e.unwrap().path();
                if p.is_dir() {
                    stack.push(p);
                } else {
                    out.insert(p.strip_prefix(d).unwrap().to_string_lossy().replace('\\', "/"), fs::read(&p).unwrap());
                }
            }
        }
        out
    }

    fn ids(rf: &mut Romfs, dir: &P, t: &str) -> (Byml, Vec<String>) {
        let y: Byml = Byml::parse(&rf.zs.decompress(&fs::read(dir.join(t)).unwrap()).unwrap()).unwrap();
        let v: Vec<String> = y.root.as_arr().unwrap().iter().filter_map(rows::id).map(|s| s.to_string()).collect();
        (y, v)
    }

    #[test]
    fn three_packs_chain_on_top_of_each_other_and_every_earlier_pack_stays_untouched() {
        let Some(root) = real() else {
            return;
        };
        let load: PB = std::env::temp_dir().join(format!("ootrr_pack_chain_{}", std::process::id()));
        let _ = fs::remove_dir_all(&load);
        let mut rf: Romfs = Romfs::open(&root).unwrap();
        let chain: [(&str, &str); 3] = [("Item_Hookshot", "Obj_Hookshot"), ("Item_Boomerang", "Obj_Boomerang"), ("Item_Hammer", "Obj_Hammer")];
        let mut base: Option<PB> = None;
        let mut first: BM<String, Vec<u8>> = BM::new();
        let mut sizes_seen: BM<String, u32> = BM::new();
        let mut rows_seen: Vec<BM<String, Node>> = vec![BM::new(), BM::new(), BM::new()];
        let mut frozen: Vec<(PB, BM<String, Vec<u8>>)> = Vec::new();
        for (i, (item, name)) in chain.iter().enumerate() {
            let s: Spec = Spec { item: item.to_string(), name: name.to_string(), style: Style::Touch, heap: None, source: None, identity: None };
            let places: Vec<u64> = if i == 0 { vec![SPOT] } else { Vec::new() };
            let o: Opts = Opts { host: Host::Hookshot, base: base.clone(), rst_from: None, places, dry: false };
            let d: Done = run(&root, &load, &format!("step{i}"), &s, &o).unwrap();
            let out: PB = d.root.clone().unwrap().join("romfs");
            assert_eq!(d.carried.len(), i, "{name}");
            assert!(d.report.contains(&format!("{name} stays collected once taken")), "{}", d.report);
            first.insert(pack_of(name), fs::read(out.join(pack_of(name))).unwrap());
            for (path, z) in &first {
                assert_eq!(&fs::read(out.join(path)).unwrap(), z, "{path} in step {i}");
            }
            let rst: Rst = Rst::parse(&rf.zs.decompress(&fs::read(out.join(TABLE_PATH)).unwrap()).unwrap()).unwrap();
            let stock: Rst = rsizetable::load(&mut rf).unwrap();
            for f in [pack_of(name), actor_file(name), pouch_file(name)] {
                assert!(stock.get(&f).is_none(), "{f}");
                sizes_seen.insert(f.clone(), rst.get(&f).unwrap());
            }
            for (f, n) in &sizes_seen {
                assert_eq!(rst.get(f), Some(*n), "{f} in step {i}");
            }
            for path in first.keys() {
                let len: usize = rf.zs.decompress(&first[path]).unwrap().len();
                assert_eq!(rst.get(path), Some(sizes::pack_size(len).unwrap()), "{path}");
            }
            let mut all: Vec<Vec<String>> = Vec::new();
            for (k, t) in [ACTOR_INFO, GAME_ACTOR_INFO, POUCH_INFO].iter().enumerate() {
                let (y, v): (Byml, Vec<String>) = ids(&mut rf, &out, t);
                let was: Byml = rf.read_byml(t).unwrap();
                assert!(rows::sorted(&y).unwrap(), "{t}");
                assert_eq!(v.len(), was.root.as_arr().unwrap().len() + i + 1, "{t} in step {i}");
                rows_seen[k].insert(name.to_string(), rows::find(&y, name).unwrap().clone());
                for (n, r) in &rows_seen[k] {
                    assert_eq!(rows::find(&y, n), Some(r), "{n} in {t} in step {i}");
                }
                let mut rest: Byml = y.clone();
                rest.root.as_arr_mut().unwrap().retain(|r| !rows_seen[k].contains_key(rows::id(r).unwrap_or("")));
                assert_eq!(rest, was, "{t} in step {i}");
                assert!(rst.get(t).unwrap() as usize >= rf.zs.decompress(&fs::read(out.join(t)).unwrap()).unwrap().len(), "{t}");
                all.push(v);
            }
            assert_eq!(all[0], all[1]);
            let seen: crate::pickup_persist::Extra = crate::pickup_persist::Extra::load(&mut rf, &out).unwrap();
            assert_eq!(seen.packs.len(), i + 1);
            for (n, snap) in &frozen {
                assert_eq!(&snapshot(n), snap, "step {i} changed {}", n.display());
            }
            frozen.push((out.clone(), snapshot(&out)));
            base = Some(out);
        }
        let banc: &str = "Banc/KokiriForest/KokiriForest_Child_Dynamic.bcett.byml.zs";
        assert!(frozen[0].1.contains_key(banc));
        assert!(!frozen[2].1.contains_key(banc));
        assert_eq!(frozen[2].1.keys().filter(|k| k.starts_with("Pack/Actor/")).count(), 3);
        let again: Spec = Spec { item: "Item_Boomerang".to_string(), name: "Obj_Boomerang".to_string(), style: Style::Touch, heap: None, source: None, identity: None };
        let o: Opts = Opts { host: Host::Hookshot, base: base.clone(), rst_from: None, places: Vec::new(), dry: false };
        let d: Done = run(&root, &load, "step2", &again, &o).unwrap();
        assert_eq!(d.carried.len(), 2);
        assert_eq!(snapshot(&d.root.unwrap().join("romfs")), frozen[2].1);
        let _ = fs::remove_dir_all(&load);
    }

    #[test]
    fn a_base_table_that_lost_or_changed_a_row_fails_the_audit() {
        let Some(root) = real() else {
            return;
        };
        let load: PB = std::env::temp_dir().join(format!("ootrr_pack_audit_{}", std::process::id()));
        let _ = fs::remove_dir_all(&load);
        let mut o: Opts = opts(Vec::new());
        o.dry = false;
        let one: Done = run(&root, &load, "one", &hookshot(), &o).unwrap();
        let dir: PB = one.root.unwrap().join("romfs");
        let mut rf: Romfs = Romfs::open(&root).unwrap();
        let boom: Spec = Spec { item: "Item_Boomerang".to_string(), name: "Obj_Boomerang".to_string(), style: Style::Touch, heap: None, source: None, identity: None };
        o.base = Some(dir.clone());
        o.dry = true;
        let pl: Plan = {
            let mut packs: Vec<Vec<Entry>> = Vec::new();
            for n in [BASE, DONOR, "Item_Boomerang"] {
                packs.push(sarc::parse(&rf.read_zs(&pack_of(n)).unwrap()).unwrap());
            }
            plan_with(&packs, &boom, &Layers::default()).unwrap()
        };
        let good: Done = run(&root, &load, "two", &boom, &o).unwrap();
        let rst_z: Vec<u8> = fs::read(dir.join(TABLE_PATH)).unwrap();
        let mut rst: Rst = Rst::parse(&rf.zs.decompress(&rst_z).unwrap()).unwrap();
        sized(&mut rst, &pl, &good.files, &good.carried, &mut String::new()).unwrap();
        let z: Vec<u8> = rst.to_zs().unwrap();
        let plain: Layers = Layers::default();
        let sc: Scope = Scope { carried: &good.carried, places: &[], base: Some(&dir), layers: &plain, kept: &[] };
        assert!(audit(&mut rf, &boom, &pl, &good.files, &z, &sc).is_ok());
        let stock: Scope = Scope { carried: &good.carried, places: &[], base: None, layers: &plain, kept: &[] };
        let e: String = audit(&mut rf, &boom, &pl, &good.files, &z, &stock).unwrap_err().to_string();
        assert!(e.contains("plus one sorted row"), "{e}");
        let mut bad: Vec<File> = good.files.clone();
        let at: usize = bad.iter().position(|f| f.path == ACTOR_INFO).unwrap();
        let mut y: Byml = Byml::parse(&bad[at].raw).unwrap();
        let row: &mut Node = y.root.as_arr_mut().unwrap().iter_mut().find(|r| rows::id(r) == Some("Obj_Hookshot")).unwrap();
        row.set(rows::HEAP, Node::I32(8));
        bad[at].raw = y.write().unwrap();
        bad[at].z = zs::compress_raw(&bad[at].raw, dict_id(&bad[at].z)).unwrap();
        let e: String = audit(&mut rf, &boom, &pl, &bad, &z, &sc).unwrap_err().to_string();
        assert!(e.contains("changed 1 rows"), "{e}");
        let _ = fs::remove_dir_all(&load);
    }

    fn spec_of(item: &str, name: &str) -> Spec {
        Spec { item: item.to_string(), name: name.to_string(), style: Style::Touch, heap: None, source: None, identity: None }
    }

    fn file<'a>(d: &'a Done, path: &str) -> &'a File {
        d.files.iter().find(|f| f.path == path).unwrap()
    }

    fn in_pack(d: &Done, name: &str, f: &str) -> Option<Byml> {
        let es: Vec<Entry> = sarc::parse(&file(d, &pack_of(name)).raw).unwrap();
        sarc::get(&es, f).map(|e| Byml::parse(&e.data).unwrap())
    }

    fn comp(d: &Done, name: &str, k: &str) -> Option<String> {
        in_pack(d, name, &actor_file(name)).unwrap().root.get("Components").and_then(|c| c.get(k)).and_then(|v| v.as_str()).map(|v| v.to_string())
    }

    fn changed(rf: &mut Romfs, d: &Done, t: &str) -> Vec<String> {
        let was: Byml = rf.read_byml(t).unwrap();
        let now: Byml = Byml::parse(&file(d, t).raw).unwrap();
        let mut out: Vec<String> = Vec::new();
        for r in now.root.as_arr().unwrap() {
            let i: &str = rows::id(r).unwrap();
            if rows::find(&was, i) != Some(r) {
                out.push(i.to_string());
            }
        }
        assert!(was.root.as_arr().unwrap().iter().all(|r| rows::find(&now, rows::id(r).unwrap()).is_some()), "{t}");
        out
    }

    #[test]
    fn the_proven_hookshot_pack_keeps_its_bytes_with_every_layer_off() {
        let Some(root) = real() else {
            return;
        };
        let d: Done = run(&root, P::new("unused"), "unused", &hookshot(), &opts(vec![SPOT])).unwrap();
        assert_eq!(d.files[0].raw.len(), 36132);
        assert_eq!(crate::murmur::murmur3_32(&d.files[0].raw, 0), 0x63dc446d);
        assert_eq!(d.files[0].z.len(), 36145);
        assert_eq!(crate::murmur::murmur3_32(&d.files[0].z, 0), 0x48e36fa4);
        assert_eq!(d.files.len(), 5);
        assert!(d.kept.is_empty());
        assert!(!d.report.contains("layer:"));
        let same: Done = run_with(&root, P::new("unused"), "unused", &hookshot(), &opts(vec![SPOT]), &Layers::default()).unwrap();
        for (a, b) in d.files.iter().zip(same.files.iter()) {
            assert_eq!((a.path.as_str(), &a.raw, &a.z), (b.path.as_str(), &b.raw, &b.z));
        }
    }

    #[test]
    fn the_tier_items_with_their_own_animation_set_build_on_the_shared_one() {
        let Some(root) = real() else {
            return;
        };
        let mut rf: Romfs = Romfs::open(&root).unwrap();
        for (item, name, n) in [("Item_Longshot", "Obj_Longshot", 40), ("Item_BombBag_Small", "Obj_BombBag_Small", 41), ("Item_BombBag_Medium", "Obj_BombBag_Medium", 41), ("Item_BombBag_Large", "Obj_BombBag_Large", 41)] {
            let d: Done = run(&root, P::new("unused"), "unused", &spec_of(item, name), &opts(Vec::new())).unwrap();
            assert!(d.report.contains(&format!("pack reads back: {n} files")), "{item} {}", d.report);
            assert!(d.report.contains(&format!("{name} stays collected once taken")), "{item}");
            assert_eq!(comp(&d, name, "ASRef").as_deref(), Some("?AS/Item_Common.root.asb"), "{item}");
            assert_eq!(comp(&d, name, "ASInfoRef").as_deref(), Some("?Component/ASInfo/Item_Obj.engine__component__ASInfo.bgyml"), "{item}");
            assert_eq!(d.heap, 199928, "{item}");
            assert_eq!(changed(&mut rf, &d, POUCH_INFO), vec![name.to_string()], "{item}");
        }
        let hook: Done = run(&root, P::new("unused"), "unused", &hookshot(), &opts(Vec::new())).unwrap();
        let long: Done = run(&root, P::new("unused"), "unused", &spec_of("Item_Longshot", "Obj_Longshot"), &opts(Vec::new())).unwrap();
        let a: Vec<Entry> = sarc::parse(&hook.files[0].raw).unwrap();
        let b: Vec<Entry> = sarc::parse(&long.files[0].raw).unwrap();
        assert_eq!(b.iter().filter(|e| sarc::get(&a, &e.name).map(|x| &x.data) == Some(&e.data)).count(), 35);
        let g: Byml = Byml::parse(&file(&long, GAME_ACTOR_INFO).raw).unwrap();
        assert_eq!(rows::find(&g, "Obj_Longshot").unwrap().get(rows::VARIATION), rows::find(&g, "Item_Longshot").unwrap().get(rows::VARIATION));
        assert!(rows::find(&g, "Obj_Longshot").unwrap().get(rows::VARIATION).is_some());
        for (item, why) in [("Item_Bomb", "animation set"), ("Item_Ocarina_Time_Holder", "animation set"), ("Item_Medal_Fire", "animation set"), ("Item_NutBag_Small", "has no AnimationRef")] {
            let mut packs: Vec<Vec<Entry>> = Vec::new();
            for n in [BASE, DONOR, item] {
                packs.push(sarc::parse(&rf.read_zs(&pack_of(n)).unwrap()).unwrap());
            }
            let bad: Vec<String> = crate::pickup_pack::actor::fits(&pool_of(&packs).unwrap(), &spec_of(item, "Obj_Refused")).unwrap();
            assert!(bad.iter().any(|m| m.contains(why)), "{item} {bad:?}");
            assert!(run(&root, P::new("unused"), "unused", &spec_of(item, "Obj_Refused"), &opts(Vec::new())).is_err(), "{item}");
        }
    }

    #[test]
    fn the_heavy_items_get_a_heap_above_their_own_and_a_heap_below_it_is_refused() {
        let Some(root) = real() else {
            return;
        };
        for (item, name, heap, who) in [("Item_Gauntlet_Silver", "Obj_Gauntlet_Silver", 203416, "Armor_Adult_Gauntlet_Silver"), ("Item_Gauntlet_Gold", "Obj_Gauntlet_Gold", 205936, "Armor_Adult_Gauntlet_Gold"), ("Item_Bottle_Bug", "Obj_Bottle_Bug", 199928, "Item_Bottle_Bug")] {
            let d: Done = run(&root, P::new("unused"), "unused", &spec_of(item, name), &opts(Vec::new())).unwrap();
            assert_eq!(d.heap, heap, "{item}");
            let y: Byml = Byml::parse(&file(&d, ACTOR_INFO).raw).unwrap();
            assert_eq!(rows::find(&y, name).unwrap().get(rows::HEAP), Some(&Node::I32(heap)), "{item}");
            assert!(d.report.contains(&format!("pouch identity {who},")), "{item}");
        }
        let bug: Spec = Spec { heap: Some(224624), ..spec_of("Item_Bottle_Bug", "Obj_Bottle_Bug") };
        let d: Done = run(&root, P::new("unused"), "unused", &bug, &opts(Vec::new())).unwrap();
        assert_eq!(d.heap, 224624);
        let y: Byml = Byml::parse(&file(&d, ACTOR_INFO).raw).unwrap();
        assert_eq!(rows::find(&y, "Obj_Bottle_Bug").unwrap().get(rows::HEAP), Some(&Node::I32(224624)));
        assert_eq!(rows::own_heap(&y, "Item_Bottle_Bug").unwrap(), 150056);
        for h in [150055, 0, -8] {
            let low: Spec = Spec { heap: Some(h), ..spec_of("Item_Bottle_Bug", "Obj_Bottle_Bug") };
            assert!(run(&root, P::new("unused"), "unused", &low, &opts(Vec::new())).unwrap_err().to_string().contains("below the 150056"), "{h}");
        }
        let edge: Spec = Spec { heap: Some(150056), ..spec_of("Item_Bottle_Bug", "Obj_Bottle_Bug") };
        assert!(run(&root, P::new("unused"), "unused", &edge, &opts(Vec::new())).is_ok());
    }

    #[test]
    fn the_land_letter_gets_one_tag_row_and_a_later_pack_carries_the_tag_table() {
        let Some(root) = real() else {
            return;
        };
        let load: PB = std::env::temp_dir().join(format!("ootrr_pack_tag_{}", std::process::id()));
        let _ = fs::remove_dir_all(&load);
        let mut rf: Romfs = Romfs::open(&root).unwrap();
        let letter: Spec = spec_of("Item_Bottle_Letter", "Obj_Bottle_LetterLand");
        let tags: Layers = Layers { tags: vec!["LetterBottle".to_string()], ..Layers::default() };
        let mut o: Opts = opts(vec![SPOT]);
        o.dry = false;
        let bare: Done = run(&root, &load, "bare", &letter, &o).unwrap();
        assert!(!bare.files.iter().any(|f| f.path == TAG));
        assert!(!bare.root.clone().unwrap().join("romfs").join(TAG).exists());
        let one: Done = run_with(&root, &load, "one", &letter, &o, &tags).unwrap();
        let dir: PB = one.root.clone().unwrap().join("romfs");
        for (a, b) in bare.files.iter().zip(one.files.iter().filter(|f| f.path != TAG)) {
            assert_eq!((a.path.as_str(), &a.raw), (b.path.as_str(), &b.raw));
        }
        let t: &File = file(&one, TAG);
        assert_eq!(dict_id(&t.z), dict_id(&rf.read(TAG).unwrap()));
        assert_eq!(fs::read(dir.join(TAG)).unwrap(), t.z);
        let old: Table = Table::parse(&rf.read_zs(TAG).unwrap()).unwrap();
        let now: Table = Table::parse(&rf.zs.decompress(&t.z).unwrap()).unwrap();
        assert_eq!(now.paths.len(), old.paths.len() + 1);
        assert_eq!(now.tags_of("Obj_Bottle_LetterLand"), vec!["LetterBottle".to_string()]);
        assert_eq!(now.tags_of("Obj_Bottle_Letter"), old.tags_of("Obj_Bottle_Letter"));
        assert_eq!(now.tags_of("Obj_Arrow_Fire"), vec!["Obj_Fire_Arrow".to_string()]);
        for q in tag::QUEST {
            let n = |x: &Table| x.paths.iter().filter(|p| x.tags_of(&p[1]).contains(&q.to_string())).count();
            assert_eq!(n(&now), n(&old), "{q}");
        }
        let rst: Rst = Rst::parse(&rf.zs.decompress(&fs::read(dir.join(TABLE_PATH)).unwrap()).unwrap()).unwrap();
        assert!(rst.get(TAG).unwrap() as usize >= t.raw.len());
        assert!(one.report.contains("new row Obj_Bottle_LetterLand carries [\"LetterBottle\"]"), "{}", one.report);
        assert!(one.report.contains("Obj_Bottle_LetterLand stays collected once taken"));
        let g: Byml = Byml::parse(&file(&one, GAME_ACTOR_INFO).raw).unwrap();
        assert!(rows::find(&g, "Obj_Bottle_LetterLand").unwrap().get("HoldAnimationType").is_none());
        assert_eq!(rows::find(&g, "Obj_Bottle_LetterLand").unwrap().get(rows::RESPAWN), Some(&Node::Bool(true)));
        assert!(rows::find(&g, "Obj_Bottle_Letter").unwrap().get(rows::RESPAWN).is_none());
        o.base = Some(dir.clone());
        o.places = Vec::new();
        let again: Done = run_with(&root, &load, "again", &letter, &o, &tags).unwrap();
        assert_eq!(file(&again, TAG).raw, t.raw);
        assert!(again.kept.is_empty());
        let boom: Done = run(&root, &load, "two", &spec_of("Item_Boomerang", "Obj_Boomerang"), &o).unwrap();
        assert_eq!(boom.kept.len(), 1);
        assert_eq!(boom.carried.len(), 1);
        let next: PB = boom.root.clone().unwrap().join("romfs");
        assert_eq!(fs::read(next.join(TAG)).unwrap(), t.z);
        assert!(boom.report.contains("carried from the base unchanged"), "{}", boom.report);
        o.base = one.root.clone();
        let by_root: Done = run(&root, &load, "three", &spec_of("Item_Boomerang", "Obj_Boomerang"), &o).unwrap();
        assert_eq!(snapshot(&by_root.root.unwrap().join("romfs")), snapshot(&next));
        o.base = None;
        o.dry = true;
        assert!(run_with(&root, &load, "x", &hookshot(), &o, &tags).unwrap_err().to_string().contains("no evidence"));
        let fire: Layers = Layers { tags: vec!["Obj_Fire_Arrow".to_string()], ..Layers::default() };
        assert!(run_with(&root, &load, "x", &letter, &o, &fire).unwrap_err().to_string().contains("quest"));
        assert!(run_with(&root, &load, "x", &spec_of("Item_Bottle_Letter", "Obj_Bottle_Letter"), &o, &tags).unwrap_err().to_string().contains("already ships"));
        let _ = fs::remove_dir_all(&load);
    }

    #[test]
    fn the_ammo_layer_changes_the_new_row_and_the_identity_row_and_nothing_else() {
        let Some(root) = real() else {
            return;
        };
        let load: PB = std::env::temp_dir().join(format!("ootrr_pack_ammo_{}", std::process::id()));
        let _ = fs::remove_dir_all(&load);
        let mut rf: Romfs = Romfs::open(&root).unwrap();
        let on: Layers = Layers { ammo: true, ..Layers::default() };
        for (item, name, who, bag, kind) in [("Item_Range_Bow", "Obj_Range_Bow", "Weapon_Range_Bow", "Item_Quiver_Small", "Arrow"), ("Item_Range_SlingShot", "Obj_Range_SlingShot", "Weapon_Range_SlingShot", "Item_BulletBag_Small", "DekuSeed")] {
            let s: Spec = spec_of(item, name);
            let plain: Done = run(&root, P::new("unused"), "unused", &s, &opts(Vec::new())).unwrap();
            assert_eq!(changed(&mut rf, &plain, POUCH_INFO), vec![name.to_string()], "{item}");
            let d: Done = run_with(&root, P::new("unused"), "unused", &s, &opts(Vec::new()), &on).unwrap();
            assert_eq!(changed(&mut rf, &d, POUCH_INFO), vec![name.to_string(), who.to_string()], "{item}");
            assert_eq!(changed(&mut rf, &d, ACTOR_INFO), vec![name.to_string()], "{item}");
            assert_eq!(changed(&mut rf, &d, GAME_ACTOR_INFO), vec![name.to_string()], "{item}");
            for f in plain.files.iter().filter(|f| f.path != POUCH_INFO) {
                assert_eq!(file(&d, &f.path).raw, f.raw, "{}", f.path);
            }
            let was: Byml = rf.read_byml(POUCH_INFO).unwrap();
            let now: Byml = Byml::parse(&file(&d, POUCH_INFO).raw).unwrap();
            let from: &Node = rows::find(&was, bag).unwrap();
            for r in [name, who] {
                let row: &Node = rows::find(&now, r).unwrap();
                for k in ammo::COLS {
                    assert_eq!(row.get(k), from.get(k), "{r} {k}");
                }
                assert_eq!(row.get(ammo::KIND).and_then(|v| v.as_str()), Some(kind));
                assert_eq!(row.get(ammo::MAX), Some(&Node::I32(30)));
            }
            assert!(ammo::grown(rows::find(&was, who).unwrap(), rows::find(&now, who).unwrap(), from));
            assert_eq!(rows::find(&now, item), rows::find(&was, item));
            assert!(d.report.contains("ammo layer: one shipped row changed"), "{}", d.report);
            assert!(d.files.len() == plain.files.len() && d.kept.is_empty());
        }
        let mut o: Opts = opts(Vec::new());
        o.dry = false;
        let bow: Spec = spec_of("Item_Range_Bow", "Obj_Range_Bow");
        let one: Done = run_with(&root, &load, "one", &bow, &o, &on).unwrap();
        o.base = Some(one.root.clone().unwrap().join("romfs"));
        let two: Done = run_with(&root, &load, "two", &bow, &o, &on).unwrap();
        assert_eq!(snapshot(&two.root.clone().unwrap().join("romfs")), snapshot(&one.root.clone().unwrap().join("romfs")));
        let sling: Done = run_with(&root, &load, "three", &spec_of("Item_Range_SlingShot", "Obj_Range_SlingShot"), &o, &on).unwrap();
        let y: Byml = Byml::parse(&file(&sling, POUCH_INFO).raw).unwrap();
        assert_eq!(y.root.as_arr().unwrap().len(), 397);
        assert!(rows::find(&y, "Weapon_Range_Bow").unwrap().get(ammo::GATE).is_some());
        assert!(rows::find(&y, "Weapon_Range_SlingShot").unwrap().get(ammo::GATE).is_some());
        o.base = None;
        o.dry = true;
        assert!(run_with(&root, &load, "x", &hookshot(), &o, &on).unwrap_err().to_string().contains("no starting bag"));
        assert!(run_with(&root, &load, "x", &spec_of("Item_BombBag_Small", "Obj_BombBag_Small"), &o, &on).unwrap_err().to_string().contains("its own starting bag"));
        let _ = fs::remove_dir_all(&load);
    }

    #[test]
    fn the_magic_layer_adds_one_child_table_and_is_off_by_default() {
        let Some(root) = real() else {
            return;
        };
        let on: Layers = Layers { magic: true, ..Layers::default() };
        for (item, name, n, raise) in [
            ("Item_FairyBless_MP", "Obj_FairyBless_MP", 41, "Item_FairyBless_MP"),
            ("Item_FairyBless_MPDouble", "Obj_FairyBless_MPDouble", 41, "Item_FairyBless_MPDouble"),
        ] {
            let s: Spec = spec_of(item, name);
            let own: String = magic::table_file(name);
            let plain: Done = run(&root, P::new("unused"), "unused", &s, &opts(Vec::new())).unwrap();
            assert!(plain.report.contains(&format!("pack reads back: {n} files")), "{item} {}", plain.report);
            assert!(in_pack(&plain, name, &own).is_none(), "{item}");
            assert_eq!(comp(&plain, name, magic::TABLE_REF), None, "{item}");
            let d: Done = run_with(&root, P::new("unused"), "unused", &s, &opts(Vec::new()), &on).unwrap();
            assert!(d.report.contains(&format!("pack reads back: {} files", n + 2)), "{item} {}", d.report);
            assert_eq!(comp(&d, name, magic::TABLE_REF), Some(format!("?{own}")), "{item}");
            let t: Byml = in_pack(&d, name, &own).unwrap();
            assert_eq!(t.root.get("$parent").and_then(|v| v.as_str()), Some("Work/GameParameter/GameParameterTable/Obj_Item_Base.engine__actor__GameParameterTable.gyml"), "{item}");
            let want: String = format!("?GameParameter/ExpansionAbilityActor/{raise}.game__actor__ExpansionAbilityActor.bgyml");
            assert_eq!(t.root.get("Components").and_then(|c| c.get(magic::ABILITY)).and_then(|v| v.as_str()), Some(want.as_str()), "{item}");
            assert!(in_pack(&d, name, &magic::table_file(BASE)).is_some(), "{item}");
            assert!(in_pack(&d, name, &want[1..]).is_some(), "{item}");
            assert!(in_pack(&d, name, &magic::table_file(item)).is_none(), "{item}");
            for f in plain.files.iter().filter(|f| !f.path.starts_with("Pack/")) {
                assert_eq!(file(&d, &f.path).raw, f.raw, "{}", f.path);
            }
            assert!(d.report.contains("magic layer:"), "{item}");
            assert!(d.report.contains(&format!("{name} stays collected once taken")), "{item}");
        }
        assert!(run_with(&root, P::new("unused"), "unused", &hookshot(), &opts(Vec::new()), &on).unwrap_err().to_string().contains("only for the magic meter"));
    }

    #[test]
    fn the_seven_magic_items_give_no_magic_meter_and_the_meter_items_do() {
        let Some(root) = real() else {
            return;
        };
        let mut rf: Romfs = Romfs::open(&root).unwrap();
        let was: Byml = rf.read_byml(POUCH_INFO).unwrap();
        let cut: Vec<String> = was.root.as_arr().unwrap().iter().filter(|r| meter::loses(r)).map(|r| rows::id(r).unwrap().to_string()).collect();
        assert_eq!(cut.len(), 21);
        assert_eq!(meter::left(&was).len(), 25);
        let on: Layers = Layers { magic: true, ..Layers::default() };
        for (item, name) in [
            ("Item_GlassTruth", "Obj_GlassTruth"),
            ("Item_DinFire", "Obj_DinFire"),
            ("Item_FaroreWind", "Obj_FaroreWind"),
            ("Item_NayruLove", "Obj_NayruLove"),
            ("Item_Arrow_Fire", "Obj_Arrow_Fire2"),
            ("Item_Arrow_Ice", "Obj_Arrow_Ice"),
            ("Item_Arrow_Light", "Obj_Arrow_Light"),
        ] {
            let s: Spec = spec_of(item, name);
            let d: Done = run(&root, P::new("unused"), "unused", &s, &opts(Vec::new())).unwrap();
            let mut rows_changed: Vec<String> = changed(&mut rf, &d, POUCH_INFO);
            rows_changed.sort();
            let mut want: Vec<String> = cut.clone();
            want.push(name.to_string());
            want.sort();
            assert_eq!(rows_changed, want, "{item}");
            let now: Byml = Byml::parse(&file(&d, POUCH_INFO).raw).unwrap();
            assert_eq!(meter::left(&now), vec!["Item_FairyBless_MP", "Item_FairyBless_MPDouble", "Pouch_FairyBless_MP", "Pouch_FairyBless_MPDouble"], "{item}");
            for r in now.root.as_arr().unwrap() {
                if !meter::keeps(rows::id(r).unwrap()) {
                    assert!(!meter::raises(r), "{item} {:?}", rows::id(r));
                    assert_eq!(r.get(meter::AMOUNT), None, "{item} {:?}", rows::id(r));
                }
            }
            assert!(d.report.contains("magic raise: 21 rows"), "{item} {}", d.report);
            let again: Done = run(&root, P::new("unused"), "unused", &s, &opts(Vec::new())).unwrap();
            assert_eq!(file(&again, POUCH_INFO).raw, file(&d, POUCH_INFO).raw, "{item}");
            let e: String = run_with(&root, P::new("unused"), "unused", &s, &opts(Vec::new()), &on).unwrap_err().to_string();
            assert!(e.contains("gives no magic meter"), "{item} {e}");
        }
        for (item, name) in [("Item_FairyBless_MP", "Obj_FairyBless_MP"), ("Item_FairyBless_MPDouble", "Obj_FairyBless_MPDouble")] {
            let d: Done = run(&root, P::new("unused"), "unused", &spec_of(item, name), &opts(Vec::new())).unwrap();
            assert_eq!(changed(&mut rf, &d, POUCH_INFO), vec![name.to_string()], "{item}");
            let now: Byml = Byml::parse(&file(&d, POUCH_INFO).raw).unwrap();
            assert!(meter::raises(rows::find(&now, name).unwrap()), "{item}");
            assert!(meter::raises(rows::find(&now, item).unwrap()), "{item}");
            assert_eq!(meter::left(&now).len(), 26, "{item}");
            assert!(!d.report.contains("magic raise:"), "{item}");
        }
    }
}
