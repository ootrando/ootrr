use std::fmt::Write as W;
use std::fs;
use std::path::{Path as P, PathBuf as PB};

use crate::TITLE_ID;
use crate::error::Res;
use crate::flow_locations::dict_id;
use crate::modpack::Pack;
use crate::romfs::Romfs;
use crate::rsizetable::{self, Rst};
use crate::zs;

pub const MARGIN: u32 = 0x400;
pub const BANC_DICT: u32 = 3;

#[derive(Clone, Debug)]
pub struct Out {
    pub path: String,
    pub z: Vec<u8>,
    pub old: usize,
    pub new: usize,
}

#[derive(Clone, Debug)]
pub struct Committed {
    pub report: String,
    pub files: usize,
    pub grown: usize,
}

pub fn banc_raw(rf: &mut Romfs, f: &str, from: Option<&PB>) -> Res<Vec<u8>> {
    let local: Option<PB> = from.map(|b| b.join(f)).filter(|p| p.is_file());
    match local {
        Some(p) => rf.zs.decompress(&fs::read(p)?),
        None => rf.read_zs(f),
    }
}

pub fn banc_out(rf: &mut Romfs, f: &str, old: usize, bytes: &[u8]) -> Res<Out> {
    let z: Vec<u8> = rf.read(f)?;
    let id: u32 = dict_id(&z).unwrap_or(BANC_DICT);
    Ok(Out { path: f.to_string(), z: zs::compress_raw(bytes, Some(id))?, old, new: bytes.len() })
}

pub fn edit_staged(rf: &mut Romfs, more: &mut Vec<Out>, work: &P, path: &str, packed: bool, f: impl FnOnce(&[u8]) -> Res<Vec<u8>>) -> Res<()> {
    if let Some(i) = more.iter().position(|o| o.path == path) {
        let raw: Vec<u8> = if packed { rf.zs.decompress(&more[i].z)? } else { more[i].z.clone() };
        let out: Vec<u8> = f(&raw)?;
        let old: usize = more[i].old;
        more[i] = if packed { banc_out(rf, path, old, &out)? } else { Out { path: path.to_string(), old, new: out.len(), z: out } };
        return Ok(());
    }
    let local: PB = path.split('/').fold(work.to_path_buf(), |a, s| a.join(s));
    let z: Vec<u8> = if local.is_file() { fs::read(&local)? } else { rf.read(path)? };
    let raw: Vec<u8> = if packed { rf.zs.decompress(&z)? } else { z };
    let out: Vec<u8> = f(&raw)?;
    more.push(if packed { banc_out(rf, path, raw.len(), &out)? } else { Out { path: path.to_string(), old: raw.len(), new: out.len(), z: out } });
    Ok(())
}

pub fn rewrite(rf: &mut Romfs, path: &str, f: impl FnOnce(&[u8]) -> Res<Vec<u8>>) -> Res<Out> {
    let z: Vec<u8> = rf.read(path)?;
    let id: Option<u32> = dict_id(&z);
    let raw: Vec<u8> = rf.zs.decompress(&z)?;
    let out: Vec<u8> = f(&raw)?;
    Ok(Out { path: path.to_string(), z: zs::compress_raw(&out, id)?, old: raw.len(), new: out.len() })
}

/// Grows the size table for an edited file. For an actor pack this also grows the
/// entries of the files inside it: the table budgets each packed file on its own
/// (e.g. AI/Player.action.walk.module.ainb at 1560 bytes), and a graph that outgrows
/// its entry leaves the actor unbuilt (a black screen when loading a save).
pub fn grow_out(rf: &mut Romfs, t: &mut Rst, o: &Out) -> Res<usize> {
    let mut n: usize = 0;
    if o.new > o.old && t.grow(&o.path, o.old as u64, o.new as u64, MARGIN)?.is_some() {
        n += 1;
    }
    if o.old == 0 && t.get(&o.path).is_none() {
        let peer: Option<(String, u32)> = crate::ap::item::size_peer(&o.path).or_else(|| crate::bombchu_bag::size_peer(&o.path)).and_then(|p| t.get(&p).map(|s| (p, s)));
        let size: u64 = match peer {
            Some((p, s)) => s as u64 + rsizetable::align(o.new as u64).saturating_sub(rsizetable::align(rf.read_zs(&p)?.len() as u64)),
            None => rsizetable::align(o.new as u64) + rsizetable::align(MARGIN as u64),
        };
        t.set_size(&o.path, size as u32)?;
        n += 1;
    }
    if o.path.ends_with(".pack.zs") {
        let new: Vec<crate::sarc::Entry> = crate::sarc::parse(&rf.zs.decompress(&o.z)?)?;
        if !rf.root.join(&o.path).is_file() {
            let peers: Vec<(String, usize)> = new.iter().filter(|e| t.get(&e.name).is_some()).map(|e| (e.name.clone(), e.data.len())).collect();
            for e in &new {
                if t.get(&e.name).is_none() {
                    let sz: u32 = crate::pickup_pack::sizes::file_size(t, &peers, &e.name, e.data.len())?;
                    t.set_size(&e.name, sz)?;
                    n += 1;
                }
            }
            return Ok(n);
        }
        let old: Vec<crate::sarc::Entry> = crate::sarc::parse(&rf.read_zs(&o.path)?)?;
        for e in &new {
            let was: Option<usize> = crate::sarc::get(&old, &e.name).map(|x| x.data.len());
            if let Some(w) = was
                && e.data.len() > w
                && t.grow(&e.name, w as u64, e.data.len() as u64, MARGIN)?.is_some()
            {
                n += 1;
            }
        }
    }
    Ok(n)
}

pub fn commit(rf: &mut Romfs, load: &P, name: &str, outs: &[Out], rst_from: Option<&PB>) -> Res<Committed> {
    let pk: Pack = Pack::create(load, TITLE_ID, name)?;
    let mut rst: Rst = match rst_from {
        Some(p) => Rst::parse(&rf.zs.decompress(&fs::read(p)?)?)?,
        None => rsizetable::load(rf)?,
    };
    let mut grown: usize = 0;
    let mut report: String = String::new();
    for x in outs {
        pk.write_romfs(&x.path, &x.z)?;
        let mut note: String = String::new();
        let g: usize = grow_out(rf, &mut rst, x)?;
        if g > 0 {
            grown += g;
            note.push_str(if g == 1 { " (size table grown)" } else { " (size table grown, packed files too)" });
        }
        let _ = writeln!(report, "{}: {} -> {} bytes{note}", x.path, x.old, x.new);
    }
    pk.write_romfs(rsizetable::TABLE_PATH, &rst.to_zs()?)?;
    Ok(Committed { report, files: outs.len() + 1, grown })
}
