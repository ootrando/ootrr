use std::collections::{BTreeMap as BM, BTreeSet as BS};

use crate::error::{Error, Res};
use crate::fres::bin::{align, name, put, put16, put32, put64, put_f32, u16_at, u32_at, u64_at};
use crate::fres::dict::{build, Node};
use crate::fres::half;
use crate::fres::model::{self, Model, Shape, Vtx, BOX, FMAT, FSHP, FVTX, GPU_AT, GPU_SIZE, MAT_INDEX, SPHERE};
use crate::fres::rlt::{self, Section};
use crate::fres::space::{dir, invert, mul, pack10, point, snorm10, Mat};

pub const CPU_ALIGN: usize = 0x100;
pub const BUF_ALIGN: usize = 0x100;
pub const GPU_ALIGN: usize = 0x1000;
pub const POOL: usize = 0x120;
pub const HEADER: usize = 0xf0;
const SECTIONS: usize = 6;
const CPU: usize = 0;
const IDX: usize = 1;
const POOL_SEC: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Place {
    pub scale: [f64; 3],
    pub at: [f64; 3],
}

impl Place {
    pub fn mat(&self) -> Mat {
        let s: [f64; 3] = self.scale;
        [[s[0], 0.0, 0.0, self.at[0]], [0.0, s[1], 0.0, self.at[1]], [0.0, 0.0, s[2], self.at[2]], [0.0, 0.0, 0.0, 1.0]]
    }
}

fn normal_mat(m: &Mat) -> Mat {
    let inv: Mat = invert(m).unwrap_or(*m);
    let mut out: Mat = [[0.0; 4]; 4];
    for (i, row) in out.iter_mut().enumerate().take(3) {
        for (j, v) in row.iter_mut().enumerate().take(3) {
            *v = inv[j][i];
        }
    }
    out[3][3] = 1.0;
    out
}

#[derive(Clone, Debug)]
pub struct Merged {
    pub bytes: Vec<u8>,
    pub min: [f64; 3],
    pub max: [f64; 3],
    pub shapes: Vec<String>,
}

fn bad(m: &str) -> Error {
    Error::Msg(format!("fres merge: {m}"))
}

struct Out {
    b: Vec<u8>,
    slots: BM<usize, usize>,
}

impl Out {
    fn alloc(&mut self, n: usize, a: usize) -> usize {
        let at: usize = align(self.b.len(), a);
        self.b.resize(at + n, 0);
        at
    }

    fn copy(&mut self, from: usize, to: usize, n: usize) {
        let part: Vec<u8> = self.b[from..from + n].to_vec();
        self.b[to..to + n].copy_from_slice(&part);
        let moved: Vec<(usize, usize)> = self.slots.range(from..from + n).map(|(p, s)| (*p - from + to, *s)).collect();
        self.slots.extend(moved);
    }
}

fn sections(b: &[u8]) -> Res<Vec<Section>> {
    let s: Vec<Section> = rlt::read(b)?;
    if s.len() != SECTIONS {
        return Err(bad(&format!("expected {SECTIONS} relocation sections, found {}", s.len())));
    }
    Ok(s)
}

fn index_of(b: &[u8], m: &model::Mesh, base: usize, i: usize) -> Res<usize> {
    match m.format {
        1 => Ok(u16_at(b, base + 2 * i)? as usize),
        2 => Ok(u32_at(b, base + 4 * i)? as usize),
        f => Err(bad(&format!("index format {f} is not known"))),
    }
}

fn array(o: &mut Out, bag: (usize, usize), chu: (usize, usize), size: usize, remap: &mut BM<u64, u64>) -> usize {
    let dst: usize = o.alloc(size * (bag.1 + chu.1), 0x10);
    for (k, (at, n)) in [bag, chu].into_iter().enumerate() {
        for i in 0..n {
            let to: usize = dst + size * (i + if k == 0 { 0 } else { bag.1 });
            o.copy(at + size * i, to, size);
            remap.insert((at + size * i) as u64, to as u64);
        }
    }
    dst
}

fn dict(o: &mut Out, keys: &[(String, u64)], empty: u64) -> Res<usize> {
    let names: Vec<&str> = keys.iter().map(|k| k.0.as_str()).collect();
    let nodes: Vec<Node> = build(&names);
    let at: usize = o.alloc(8 + 16 * nodes.len(), 8);
    put32(&mut o.b, at + 4, keys.len() as u32)?;
    for (i, n) in nodes.iter().enumerate() {
        let q: usize = at + 8 + 16 * i;
        put32(&mut o.b, q, n.bit)?;
        put16(&mut o.b, q + 4, n.left)?;
        put16(&mut o.b, q + 6, n.right)?;
        put64(&mut o.b, q + 8, if i == 0 { empty } else { keys[i - 1].1 })?;
        o.slots.insert(q + 8, CPU);
    }
    Ok(at)
}

fn names_of(o: &Out, arr: usize, size: usize, n: usize) -> Res<Vec<(String, u64)>> {
    (0..n)
        .map(|i| {
            let p: u64 = u64_at(&o.b, arr + size * i + 8)?;
            Ok((name(&o.b, p as usize)?, p))
        })
        .collect()
}

struct Moved {
    pos: Vec<[f64; 3]>,
    min: [f64; 3],
    max: [f64; 3],
}

fn move_vertices(o: &mut Out, chu: &Model, v: &Vtx, base: usize, to_bag: &Mat, place: &Mat, rigid: u8) -> Res<Moved> {
    let p: model::Attr = Model::attr(v, "_p0").ok_or_else(|| bad("a chu buffer has no position"))?;
    let i0: model::Attr = Model::attr(v, "_i0").ok_or_else(|| bad("a chu buffer has no bone index, it is not rigid"))?;
    if p.format != 0x1505 || i0.format != 0x0203 || v.skin != 1 {
        return Err(bad("the chu vertex layout differs from the one this merge knows"));
    }
    let turns: Vec<model::Attr> = ["_n0", "_t0"].iter().filter_map(|n| Model::attr(v, n)).filter(|a| a.format == 0x0e02).collect();
    let mut pos: Vec<[f64; 3]> = Vec::new();
    let (mut min, mut max): ([f64; 3], [f64; 3]) = ([f64::MAX; 3], [f64::MIN; 3]);
    for i in 0..v.count {
        let q: usize = base + v.stride * i;
        let bi: usize = o.b[q + i0.offset] as usize;
        let bone: usize = *chu.rigid.get(bi).ok_or_else(|| bad("a chu vertex names a missing bone"))?;
        let world: Mat = mul(place, &chu.bones[bone].world);
        let m: Mat = mul(to_bag, &world);
        let src: [f64; 3] = [0, 1, 2].map(|k| half::to_f32(u16::from_le_bytes([o.b[q + p.offset + 2 * k], o.b[q + p.offset + 2 * k + 1]])) as f64);
        let local: [f64; 3] = point(&m, src);
        for (k, x) in local.iter().enumerate() {
            put16(&mut o.b, q + p.offset + 2 * k, half::from_f32(*x as f32))?;
        }
        let stored: [f64; 3] = [0, 1, 2].map(|k| half::to_f32(half::from_f32(local[k] as f32)) as f64);
        pos.push(stored);
        let w: [f64; 3] = point(&world, src);
        for k in 0..3 {
            min[k] = min[k].min(w[k]);
            max[k] = max[k].max(w[k]);
        }
        let nm: Mat = normal_mat(&m);
        for a in &turns {
            let x: u32 = u32_at(&o.b, q + a.offset)?;
            let by: &Mat = if a.name == "_n0" { &nm } else { &m };
            let n: [f64; 3] = dir(by, [snorm10(x, 0), snorm10(x, 10), snorm10(x, 20)]);
            put32(&mut o.b, q + a.offset, pack10(n, x))?;
        }
        o.b[q + i0.offset] = rigid;
    }
    Ok(Moved { pos, min, max })
}

fn bounds(o: &mut Out, s: &Shape, idx: usize, pos: &[[f64; 3]]) -> Res<()> {
    for (mi, m) in s.meshes.iter().enumerate() {
        let base: usize = idx + m.offset;
        let mut used: BS<usize> = BS::new();
        for j in 0..m.count {
            used.insert(index_of(&o.b, m, base, j)? + m.first);
        }
        let pts: Vec<[f64; 3]> = used.iter().map(|i| pos.get(*i).copied().ok_or_else(|| bad("an index points past the chu vertices"))).collect::<Res<Vec<[f64; 3]>>>()?;
        if pts.is_empty() {
            continue;
        }
        let lo: [f64; 3] = [0, 1, 2].map(|k| pts.iter().map(|p| p[k]).fold(f64::MAX, f64::min));
        let hi: [f64; 3] = [0, 1, 2].map(|k| pts.iter().map(|p| p[k]).fold(f64::MIN, f64::max));
        let c: [f64; 3] = [0, 1, 2].map(|k| (lo[k] + hi[k]) / 2.0);
        let e: [f64; 3] = [0, 1, 2].map(|k| (hi[k] - lo[k]) / 2.0);
        for bx in [2 * mi, 2 * mi + 1] {
            for (k, v) in c.iter().chain(e.iter()).enumerate() {
                put_f32(&mut o.b, s.boxes + BOX * bx + 4 * k, *v as f32)?;
            }
        }
        let r: f64 = pts.iter().map(|p| ((p[0] - c[0]).powi(2) + (p[1] - c[1]).powi(2) + (p[2] - c[2]).powi(2)).sqrt()).fold(0.0, f64::max);
        for (k, v) in c.iter().chain([r].iter()).enumerate() {
            put_f32(&mut o.b, s.spheres + SPHERE * mi + 4 * k, *v as f32)?;
        }
    }
    Ok(())
}

pub fn merge(bag_b: &[u8], chu_b: &[u8], place: Place, bone: &str) -> Res<Merged> {
    let bag: Model = model::read(bag_b)?;
    let chu: Model = model::read(chu_b)?;
    let bs: Vec<Section> = sections(bag_b)?;
    let cs: Vec<Section> = sections(chu_b)?;
    let (bag_cpu, chu_cpu): (usize, usize) = (bs[CPU].size, cs[CPU].size);
    if bs[CPU].pos != 0 || cs[CPU].pos != 0 || bag.gpu < bag_cpu || chu.gpu < chu_cpu {
        return Err(bad("the CPU section does not start the file"));
    }
    let (ib, ic): (usize, usize) = (bag.first_vertex(), chu.first_vertex());
    let x0: usize = align(bag_cpu, CPU_ALIGN);
    let mut o: Out = Out { b: bag_b[..bag_cpu].to_vec(), slots: BM::new() };
    o.b.resize(x0, 0);
    o.b.extend_from_slice(&chu_b[..chu_cpu]);
    for (si, s) in bs.iter().enumerate() {
        for p in &s.slots {
            o.slots.insert(*p, si);
        }
    }
    for (si, s) in cs.iter().enumerate() {
        for p in &s.slots {
            if si == IDX || *p < HEADER || (si != CPU && si != POOL_SEC) {
                continue;
            }
            let v: u64 = u64_at(chu_b, *p)?;
            if si == CPU && v != 0 {
                put64(&mut o.b, x0 + p, v + x0 as u64)?;
            }
            o.slots.insert(x0 + p, si);
        }
    }
    let at = |p: usize| x0 + p;
    let root: usize = bag.bone(bone).ok_or_else(|| bad(&format!("the bag has no bone {bone}")))?;
    let rigid: u8 = bag.bones[root].rigid.and_then(|r| u8::try_from(r).ok()).ok_or_else(|| bad(&format!("the bag bone {bone} has no rigid matrix")))?;
    let to_bag: Mat = invert(&bag.bones[root].world).ok_or_else(|| bad("the bag bone cannot be inverted"))?;
    for (k, s) in chu.shapes.iter().enumerate() {
        put16(&mut o.b, at(s.at) + 0x50, (bag.ns + k) as u16)?;
        put16(&mut o.b, at(s.at) + 0x52, bag.nm as u16 + s.mat)?;
        put16(&mut o.b, at(s.at) + 0x54, 0)?;
        put16(&mut o.b, at(s.at) + 0x56, bag.nv as u16 + s.vbi)?;
        put16(&mut o.b, at(s.at) + 0x58, 1)?;
        put16(&mut o.b, at(s.skin_at), root as u16)?;
    }
    for k in 0..chu.nm {
        put16(&mut o.b, at(chu.fmat + FMAT * k + MAT_INDEX), (bag.nm + k) as u16)?;
    }
    let mut remap: BM<u64, u64> = BM::new();
    let fv: usize = array(&mut o, (bag.fvtx, bag.nv), (at(chu.fvtx), chu.nv), FVTX, &mut remap);
    let fs: usize = array(&mut o, (bag.fshp, bag.ns), (at(chu.fshp), chu.ns), FSHP, &mut remap);
    let fm: usize = array(&mut o, (bag.fmat, bag.nm), (at(chu.fmat), chu.nm), FMAT, &mut remap);
    for k in 0..chu.nv {
        put16(&mut o.b, fv + FVTX * (bag.nv + k) + 0x4e, (bag.nv + k) as u16)?;
    }
    let cpu_slots: Vec<usize> = o.slots.iter().filter(|(_, s)| **s == CPU).map(|(p, _)| *p).collect();
    for p in cpu_slots {
        let v: u64 = u64_at(&o.b, p)?;
        if let Some(n) = remap.get(&v) {
            put64(&mut o.b, p, *n)?;
        }
    }
    let (nv, ns, nm): (usize, usize, usize) = (bag.nv + chu.nv, bag.ns + chu.ns, bag.nm + chu.nm);
    let empty: u64 = u64_at(bag_b, model_dict(bag_b, &bag)? + 16)?;
    let sk: Vec<(String, u64)> = names_of(&o, fs, FSHP, ns)?;
    let mk: Vec<(String, u64)> = names_of(&o, fm, FMAT, nm)?;
    let sd: usize = dict(&mut o, &sk, empty)?;
    let md: usize = dict(&mut o, &mk, empty)?;
    let f: usize = bag.fmdl;
    for (k, v) in [(0x20, fv), (0x28, fs), (0x30, sd), (0x38, fm), (0x40, md)] {
        put64(&mut o.b, f + k, v as u64)?;
    }
    for (k, v) in [(0x68, nv), (0x6a, ns), (0x6c, nm)] {
        put16(&mut o.b, f + k, v as u16)?;
    }
    let cpu_end: usize = o.b.len();
    let idx_chu: usize = align(ib, BUF_ALIGN);
    let vtx_bag: usize = align(idx_chu + ic, BUF_ALIGN);
    let vtx_chu: usize = align(vtx_bag + (bag.gpu_size - ib), BUF_ALIGN);
    let gsz: usize = align(vtx_chu + (chu.gpu_size - ic), GPU_ALIGN);
    let gpu: usize = align(cpu_end, GPU_ALIGN);
    o.b.resize(gpu + gsz, 0);
    let part = |b: &[u8], from: usize, to: usize| -> Res<Vec<u8>> { b.get(from..to).map(|s| s.to_vec()).ok_or_else(|| bad("a buffer leaves the file")) };
    put(&mut o.b, gpu, &part(bag_b, bag.gpu, bag.gpu + ib)?)?;
    put(&mut o.b, gpu + idx_chu, &part(chu_b, chu.gpu, chu.gpu + ic)?)?;
    put(&mut o.b, gpu + vtx_bag, &part(bag_b, bag.gpu + ib, bag.gpu + bag.gpu_size)?)?;
    put(&mut o.b, gpu + vtx_chu, &part(chu_b, chu.gpu + ic, chu.gpu + chu.gpu_size)?)?;
    for i in 0..bag.nv {
        let a: usize = fv + FVTX * i + 0x48;
        let v: u32 = u32_at(&o.b, a)?;
        put32(&mut o.b, a, (v as usize - ib + vtx_bag) as u32)?;
    }
    for k in 0..chu.nv {
        let a: usize = fv + FVTX * (bag.nv + k) + 0x48;
        let v: u32 = u32_at(&o.b, a)?;
        put32(&mut o.b, a, (v as usize - ic + vtx_chu) as u32)?;
    }
    for s in &chu.shapes {
        for m in &s.meshes {
            let a: usize = at(m.at) + 0x20;
            let v: u32 = u32_at(&o.b, a)?;
            put32(&mut o.b, a, (v as usize + idx_chu) as u32)?;
        }
    }
    let (mut min, mut max): ([f64; 3], [f64; 3]) = ([f64::MAX; 3], [f64::MIN; 3]);
    let pm: Mat = place.mat();
    for s in &chu.shapes {
        let v: &Vtx = chu.vtx.get(s.vbi as usize).ok_or_else(|| bad("a chu shape names a missing buffer"))?;
        let base: usize = gpu + vtx_chu + (v.offset - ic);
        let moved: Moved = move_vertices(&mut o, &chu, v, base, &to_bag, &pm, rigid)?;
        for k in 0..3 {
            min[k] = min[k].min(moved.min[k]);
            max[k] = max[k].max(moved.max[k]);
        }
        let mut shifted: Shape = s.clone();
        shifted.boxes = at(s.boxes);
        shifted.spheres = at(s.spheres);
        bounds(&mut o, &shifted, gpu + idx_chu, &moved.pos)?;
    }
    let pool: usize = o.b.len();
    o.b.resize(pool + POOL, 0);
    let slots: Vec<(usize, usize)> = o.slots.iter().map(|(p, s)| (*p, *s)).collect();
    for (p, s) in &slots {
        if *s == POOL_SEC {
            put64(&mut o.b, *p, pool as u64)?;
        }
    }
    o.slots.insert(GPU_AT, IDX);
    put64(&mut o.b, GPU_AT, gpu as u64)?;
    put32(&mut o.b, GPU_SIZE, gsz as u32)?;
    let rp: usize = o.b.len();
    let mut secs: Vec<Section> = vec![
        Section { pos: 0, size: cpu_end, slots: Vec::new() },
        Section { pos: gpu, size: vtx_bag, slots: Vec::new() },
        Section { pos: gpu + vtx_bag, size: gsz - vtx_bag, slots: Vec::new() },
        Section { pos: pool, size: 0, slots: Vec::new() },
        Section { pos: pool, size: POOL, slots: Vec::new() },
        Section { pos: rp, size: 0, slots: Vec::new() },
    ];
    for (p, s) in o.slots.iter() {
        secs[*s].slots.push(*p);
    }
    let t: Vec<u8> = rlt::write(rp, &secs)?;
    o.b.extend(t);
    let total: usize = o.b.len();
    put32(&mut o.b, rlt::AT, rp as u32)?;
    put32(&mut o.b, rlt::SIZE_AT, total as u32)?;
    let sp: usize = u16_at(&o.b, 0x16)? as usize;
    if o.b.get(sp..sp + 4) != Some(b"_STR".as_slice()) {
        return Err(bad("the bag string table is not where the header says"));
    }
    put32(&mut o.b, sp + 8, (rp - sp) as u32)?;
    let shapes: Vec<String> = sk.into_iter().map(|k| k.0).collect();
    Ok(Merged { bytes: o.b, min, max, shapes })
}

fn model_dict(b: &[u8], m: &Model) -> Res<usize> {
    let p: u64 = u64_at(b, m.fmdl + 0x30)?;
    usize::try_from(p).map_err(|_| bad("the shape dictionary is out of range"))
}

pub fn check(b: &[u8]) -> Res<()> {
    let secs: Vec<Section> = rlt::read(b)?;
    for (i, s) in secs.iter().enumerate() {
        for p in &s.slots {
            let v: u64 = u64_at(b, *p)?;
            if v == 0 {
                continue;
            }
            let v: usize = v as usize;
            let inside: bool = v >= s.pos && (v < s.pos + s.size || (s.size == 0 && v == s.pos) || v == s.pos);
            if !inside {
                return Err(bad(&format!("pointer at {p:#x} of section {i} points to {v:#x}, outside {:#x}+{:#x}", s.pos, s.size)));
            }
        }
    }
    let m: Model = model::read(b)?;
    for s in &m.shapes {
        let v: &Vtx = m.vtx.get(s.vbi as usize).ok_or_else(|| bad("a shape names a missing buffer"))?;
        if v.at != s.vtx {
            return Err(bad(&format!("shape {} points to another buffer than its index", s.name)));
        }
        if v.offset + v.stride * v.count > m.gpu_size {
            return Err(bad(&format!("the buffer of {} leaves the GPU section", s.name)));
        }
        for x in &s.meshes {
            let w: usize = if x.format == 1 { 2 } else { 4 };
            if x.offset + w * x.count > m.first_vertex() {
                return Err(bad(&format!("an index buffer of {} runs into the vertex buffers", s.name)));
            }
            let base: usize = m.gpu + x.offset;
            for j in 0..x.count {
                if index_of(b, x, base, j)? + x.first >= v.count {
                    return Err(bad(&format!("an index of {} points past its buffer", s.name)));
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fres::model::read;

    fn romfs() -> Option<std::path::PathBuf> {
        std::env::var_os("OOTRR_ROMFS").map(std::path::PathBuf::from).filter(|p| p.join("Model").is_dir())
    }

    fn load(root: &std::path::Path, f: &str) -> Vec<u8> {
        let mut rf: crate::romfs::Romfs = crate::romfs::Romfs::open(root).unwrap();
        rf.read_zs(&format!("Model/{f}.bfres.zs")).unwrap()
    }

    const PLACE: Place = Place { scale: [0.27, 0.27, 0.27], at: [0.0, 0.045, 0.05] };

    #[test]
    fn the_merged_bag_keeps_every_shape_and_passes_the_checks() {
        let Some(root) = romfs() else {
            return;
        };
        let bag: Vec<u8> = load(&root, "Item_BombBag_Small.Item_BombBag_Small");
        let chu: Vec<u8> = load(&root, "Item_Bombchu.Item_Bombchu");
        check(&bag).unwrap();
        check(&chu).unwrap();
        let m: Merged = merge(&bag, &chu, PLACE, "Skl_Root").unwrap();
        check(&m.bytes).unwrap();
        let back: Model = read(&m.bytes).unwrap();
        let old: Model = read(&bag).unwrap();
        let c: Model = read(&chu).unwrap();
        assert_eq!(back.ns, old.ns + c.ns);
        assert_eq!(back.nm, old.nm + c.nm);
        assert_eq!(back.nv, old.nv + c.nv);
        assert_eq!(back.shapes.iter().map(|s| s.name.clone()).collect::<Vec<String>>(), m.shapes);
        for (i, s) in back.shapes.iter().enumerate() {
            assert_eq!(s.vbi as usize, i);
            assert_eq!(s.mat as usize, if i < old.ns { old.shapes[i].mat as usize } else { old.nm + c.shapes[i - old.ns].mat as usize });
        }
        for i in 0..old.nv {
            let (a, b): (&Vtx, &Vtx) = (&old.vtx[i], &back.vtx[i]);
            let n: usize = a.stride * a.count;
            assert_eq!(bag[old.gpu + a.offset..old.gpu + a.offset + n], m.bytes[back.gpu + b.offset..back.gpu + b.offset + n]);
        }
        for (s, t) in old.shapes.iter().zip(&back.shapes) {
            for (x, y) in s.meshes.iter().zip(&t.meshes) {
                let n: usize = 2 * x.count;
                assert_eq!(bag[old.gpu + x.offset..old.gpu + x.offset + n], m.bytes[back.gpu + y.offset..back.gpu + y.offset + n]);
            }
        }
        assert!(m.max[2] > 0.1 && m.max[2] < 0.2, "{:?}", m.max);
        assert!(m.min[1] > 0.0 && m.max[1] < 0.2, "{:?} {:?}", m.min, m.max);
        assert!(model::mat_name(&m.bytes, &back, 3).unwrap().starts_with("Mt_Item_Bombchu"));
    }

    #[test]
    fn the_chu_vertices_land_in_the_bag_bone_space() {
        let Some(root) = romfs() else {
            return;
        };
        let bag: Vec<u8> = load(&root, "Item_BombBag_Small.Item_BombBag_Small");
        let chu: Vec<u8> = load(&root, "Item_Bombchu.Item_Bombchu");
        let m: Merged = merge(&bag, &chu, PLACE, "Skl_Root").unwrap();
        let back: Model = read(&m.bytes).unwrap();
        let c: Model = read(&chu).unwrap();
        let root_bone: usize = back.bone("Skl_Root").unwrap();
        let rigid: u8 = back.bones[root_bone].rigid.unwrap() as u8;
        let v: &Vtx = &back.vtx[back.nv - 1];
        let old: &Vtx = &c.vtx[c.nv - 1];
        for i in (0..v.count).step_by(97) {
            let q: usize = back.gpu + v.offset + v.stride * i;
            assert_eq!(m.bytes[q + 20], rigid);
            let local: [f64; 3] = [0, 1, 2].map(|k| half::to_f32(u16_at(&m.bytes, q + 2 * k).unwrap()) as f64);
            let world: [f64; 3] = point(&back.bones[root_bone].world, local);
            let oq: usize = c.gpu + old.offset + old.stride * i;
            let src: [f64; 3] = [0, 1, 2].map(|k| half::to_f32(u16_at(&chu, oq + 2 * k).unwrap()) as f64);
            let bone: usize = c.rigid[chu[oq + 20] as usize];
            let want: [f64; 3] = point(&PLACE.mat(), point(&c.bones[bone].world, src));
            for k in 0..3 {
                assert!((world[k] - want[k]).abs() < 2e-3, "{i} {world:?} {want:?}");
            }
        }
    }

    #[test]
    fn normals_follow_the_inverse_transpose() {
        let m: Mat = Place { scale: [2.0, 1.0, 0.5], at: [0.0; 3] }.mat();
        let n: [f64; 3] = dir(&normal_mat(&m), [1.0, 0.0, 1.0]);
        assert!((n[0] - 0.242_535_625).abs() < 1e-6 && (n[2] - 0.970_142_5).abs() < 1e-6, "{n:?}");
    }

    #[test]
    fn a_missing_bone_is_refused() {
        let Some(root) = romfs() else {
            return;
        };
        let bag: Vec<u8> = load(&root, "Item_BombBag_Small.Item_BombBag_Small");
        let chu: Vec<u8> = load(&root, "Item_Bombchu.Item_Bombchu");
        assert!(merge(&bag, &chu, PLACE, "Nope").unwrap_err().to_string().contains("no bone Nope"));
        assert!(merge(&bag, &chu, PLACE, "Root").unwrap_err().to_string().contains("no rigid matrix"));
    }
}
