use std::collections::HashMap as Map;

use crate::error::{Error, Res};
use crate::rng::Rng;
use crate::romfs::Romfs;

pub const NAV_DIR: &str = "Phive/NavMesh";
pub const STITCHED_DIR: &str = "Phive/StitchedNavMesh";
pub const NAV_EXT: &str = ".Nin_Ounce_NVN2.bphnm.zs";
pub const TILE: f32 = 250.0;
pub const TILE_MID: i32 = 8;
pub const CELL: f32 = 8.0;
pub const WALL_BAND: f32 = 3.0;
pub const CLEAR_MAX: f32 = 12.0;
pub const MESH: &str = "hkaiNavMesh";
pub const FIELDS: [&str; 3] = ["faces", "edges", "vertices"];
pub const SIZES: [usize; 3] = [12, 24, 16];
pub const GRAIN: u64 = 1 << 20;
pub const SHORE_STEP: f32 = 0.5;
pub const SHORE_RAYS: usize = 16;
pub const SHORE_UP: f32 = 1.5;
pub const SHORE_DOWN: f32 = 3.0;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mesh {
    pub verts: Vec<[f32; 3]>,
    pub faces: Vec<Vec<u32>>,
    pub open: Vec<Vec<bool>>,
    pub next: Vec<Vec<i32>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Face {
    pub pts: Vec<[f32; 3]>,
    pub slope: f32,
    pub area: f32,
    pub patch: u32,
}

#[derive(Clone, Debug, Default)]
pub struct Ground {
    pub faces: Vec<Face>,
    pub walls: Vec<[[f32; 3]; 2]>,
    pub patch_area: Vec<f32>,
    pub total: f32,
    pub tiles: bool,
    sums: Vec<f32>,
    cells: Map<(i32, i32), Vec<u32>>,
    wcells: Map<(i32, i32), Vec<u32>>,
}

struct Rd<'a> {
    b: &'a [u8],
    o: usize,
    e: usize,
}

impl Rd<'_> {
    fn byte(&self, k: usize) -> Res<u64> {
        if self.o + k >= self.e {
            return Err(Error::Format("navmesh: a number runs past its section"));
        }
        Ok(self.b[self.o + k] as u64)
    }

    fn v(&mut self) -> Res<u64> {
        let b0: u64 = self.byte(0)?;
        let (n, mask): (usize, u64) = match b0 {
            x if x & 0x80 == 0 => (1, 0x7f),
            x if x >> 6 == 2 => (2, 0x3fff),
            x if x >> 5 == 6 => (3, 0x1f_ffff),
            x if x >> 3 == 0x1c => (4, 0x7ff_ffff),
            x if x >> 3 == 0x1d => (5, 0x7_ffff_ffff),
            x if x >> 3 == 0x1e => (8, 0x7ff_ffff_ffff_ffff),
            _ => return Err(Error::Format("navmesh: unknown number prefix")),
        };
        let mut x: u64 = 0;
        for k in 0..n {
            x = (x << 8) | self.byte(k)?;
        }
        self.o += n;
        Ok(x & mask)
    }
}

fn u32_at(b: &[u8], o: usize) -> Res<u32> {
    b.get(o..o + 4).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]])).ok_or(Error::Format("navmesh: read past the end"))
}

fn i32_at(b: &[u8], o: usize) -> Res<i32> {
    u32_at(b, o).map(|x| x as i32)
}

fn f32_at(b: &[u8], o: usize) -> Res<f32> {
    u32_at(b, o).map(f32::from_bits)
}

fn sections(b: &[u8], mut off: usize, end: usize, out: &mut Vec<([u8; 4], usize, usize)>) -> Res<()> {
    while off + 8 <= end {
        let h: u32 = u32::from_be_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]]);
        let size: usize = (h & 0x3fff_ffff) as usize;
        if size < 8 || off + size > end {
            return Err(Error::Format("navmesh: a section has a bad size"));
        }
        let tag: [u8; 4] = [b[off + 4], b[off + 5], b[off + 6], b[off + 7]];
        out.push((tag, off + 8, off + size));
        if h & 0x4000_0000 == 0 {
            sections(b, off + 8, off + size, out)?;
        }
        off += size;
    }
    Ok(())
}

fn strings(b: &[u8]) -> Vec<String> {
    b.split(|c| *c == 0).map(|s| String::from_utf8_lossy(s).to_string()).collect()
}

struct Body {
    size: usize,
    members: Vec<(String, usize)>,
}

pub fn parse(b: &[u8], shift: [f32; 3]) -> Res<Mesh> {
    let start: usize = b.windows(4).position(|w| w == b"TAG0").ok_or(Error::Format("navmesh: no tag file"))?.checked_sub(4).ok_or(Error::Format("navmesh: tag file at the start"))?;
    let h: u32 = u32::from_be_bytes([b[start], b[start + 1], b[start + 2], b[start + 3]]);
    let end: usize = (start + (h & 0x3fff_ffff) as usize).min(b.len());
    let mut secs: Vec<([u8; 4], usize, usize)> = Vec::new();
    sections(b, start, end, &mut secs)?;
    let sec = |t: &[u8; 4]| -> Res<(usize, usize)> { secs.iter().find(|s| &s.0 == t).map(|s| (s.1, s.2)).ok_or(Error::Format("navmesh: a section is missing")) };
    let data: usize = sec(b"DATA")?.0;
    let (ts, te): (usize, usize) = sec(b"TST1")?;
    let tst: Vec<String> = strings(&b[ts..te]);
    let (fs, fe): (usize, usize) = sec(b"FST1")?;
    let fst: Vec<String> = strings(&b[fs..fe]);
    let (na, ne): (usize, usize) = sec(b"TNA1")?;
    let mut r: Rd = Rd { b, o: na, e: ne };
    let n: u64 = r.v()?;
    let mut names: Vec<String> = vec![String::new()];
    for _ in 1..n {
        let ni: usize = r.v()? as usize;
        let nt: u64 = r.v()?;
        for _ in 0..nt {
            r.v()?;
            r.v()?;
        }
        names.push(tst.get(ni).cloned().unwrap_or_default());
    }
    let (ba, be): (usize, usize) = sec(b"TBDY")?;
    let mut r: Rd = Rd { b, o: ba, e: be };
    let mut bodies: Map<usize, Body> = Map::new();
    while r.o < r.e {
        let ti: usize = r.v()? as usize;
        if ti == 0 {
            continue;
        }
        r.v()?;
        let fl: u64 = r.v()?;
        let mut body: Body = Body { size: 0, members: Vec::new() };
        if fl & 1 != 0 {
            r.v()?;
        }
        if fl & 2 != 0 {
            r.v()?;
        }
        if fl & 4 != 0 {
            r.v()?;
        }
        if fl & 8 != 0 {
            body.size = r.v()? as usize;
            r.v()?;
        }
        if fl & 0x10 != 0 {
            r.v()?;
        }
        if fl & 0x20 != 0 {
            let nm: u64 = r.v()? & 0xffff;
            for _ in 0..nm {
                let name: usize = r.v()? as usize;
                r.v()?;
                let off: usize = r.v()? as usize;
                r.v()?;
                body.members.push((fst.get(name).cloned().unwrap_or_default(), off));
            }
        }
        if fl & 0x40 != 0 {
            let ni: u64 = r.v()?;
            for _ in 0..ni {
                r.v()?;
                r.v()?;
            }
        }
        if fl & 0x80 != 0 {
            r.v()?;
        }
        bodies.insert(ti, body);
    }
    let (ia, ie): (usize, usize) = sec(b"ITEM")?;
    let mut items: Vec<(usize, usize, usize)> = Vec::new();
    let mut o: usize = ia;
    while o + 12 <= ie {
        items.push(((u32_at(b, o)? & 0xff_ffff) as usize, data + u32_at(b, o + 4)? as usize, u32_at(b, o + 8)? as usize));
        o += 12;
    }
    let mt: usize = names.iter().position(|n| n == MESH).ok_or(Error::Format("navmesh: no navmesh type"))?;
    let obj: usize = items.iter().find(|i| i.0 == mt).ok_or(Error::Format("navmesh: no navmesh object"))?.1;
    let body: &Body = bodies.get(&mt).ok_or(Error::Format("navmesh: no navmesh layout"))?;
    let mut arrays: Vec<(usize, usize)> = Vec::new();
    for (k, f) in FIELDS.iter().enumerate() {
        let off: usize = body.members.iter().find(|m| m.0 == *f).ok_or(Error::Format("navmesh: a navmesh field is missing"))?.1;
        let ix: usize = u32_at(b, obj + off)? as usize;
        let it: (usize, usize, usize) = match ix {
            0 => (0, 0, 0),
            _ => *items.get(ix).ok_or(Error::Format("navmesh: a field points past the items"))?,
        };
        if it.2 > 0 && bodies.get(&it.0).map(|x| x.size).is_some_and(|s| s != 0 && s != SIZES[k]) {
            return Err(Error::Format("navmesh: an array has an unexpected layout"));
        }
        arrays.push((it.1, it.2));
    }
    let mut m: Mesh = Mesh::default();
    for i in 0..arrays[2].1 {
        let p: usize = arrays[2].0 + 16 * i;
        m.verts.push([f32_at(b, p)? + shift[0], f32_at(b, p + 4)? + shift[1], f32_at(b, p + 8)? + shift[2]]);
    }
    let edge = |k: usize| -> Res<(u32, bool, i32)> {
        if k >= arrays[1].1 {
            return Err(Error::Format("navmesh: a face points past the edges"));
        }
        let p: usize = arrays[1].0 + 24 * k;
        let face: i32 = i32_at(b, p + 16)?;
        Ok((u32_at(b, p)?, face < 0 && i32_at(b, p + 8)? < 0, face))
    };
    for i in 0..arrays[0].1 {
        let p: usize = arrays[0].0 + 12 * i;
        let s: usize = i32_at(b, p)?.max(0) as usize;
        let n: usize = (u32_at(b, p + 8)? & 0xffff) as usize;
        let mut vs: Vec<u32> = Vec::new();
        let mut op: Vec<bool> = Vec::new();
        let mut nx: Vec<i32> = Vec::new();
        for k in 0..n {
            let (v, w, f): (u32, bool, i32) = edge(s + k)?;
            if v as usize >= m.verts.len() {
                return Err(Error::Format("navmesh: an edge points past the vertices"));
            }
            vs.push(v);
            op.push(w);
            nx.push(f);
        }
        m.faces.push(vs);
        m.open.push(op);
        m.next.push(nx);
    }
    Ok(m)
}

pub fn tile_shift(name: &str) -> Option<[f32; 3]> {
    let s: &str = name.rsplit('/').next()?;
    let x: i32 = s.strip_prefix('X')?.split('_').next()?.parse().ok()?;
    let z: i32 = s.split("_Z").nth(1)?.split('.').next()?.parse().ok()?;
    Some([(x - TILE_MID) as f32 * TILE + TILE / 2.0, 0.0, (z - TILE_MID) as f32 * TILE + TILE / 2.0])
}

pub fn filler(m: &Mesh) -> bool {
    m.faces.len() <= 1 && m.verts.iter().all(|v| v[1] == 0.0)
}

fn key(x: f32, z: f32) -> (i32, i32) {
    ((x / CELL).floor() as i32, (z / CELL).floor() as i32)
}

fn normal(p: &[[f32; 3]]) -> [f32; 3] {
    let mut n: [f32; 3] = [0.0; 3];
    for i in 0..p.len() {
        let (a, b): ([f32; 3], [f32; 3]) = (p[i], p[(i + 1) % p.len()]);
        n[0] += (a[1] - b[1]) * (a[2] + b[2]);
        n[1] += (a[2] - b[2]) * (a[0] + b[0]);
        n[2] += (a[0] - b[0]) * (a[1] + b[1]);
    }
    n
}

pub fn inside(p: &[[f32; 3]], x: f32, z: f32) -> bool {
    let mut c: bool = false;
    for i in 0..p.len() {
        let (a, b): ([f32; 3], [f32; 3]) = (p[i], p[(i + 1) % p.len()]);
        if (a[2] > z) != (b[2] > z) {
            let t: f32 = (z - a[2]) / (b[2] - a[2]);
            if x < a[0] + t * (b[0] - a[0]) {
                c = !c;
            }
        }
    }
    c
}

pub fn height(p: &[[f32; 3]], x: f32, z: f32) -> f32 {
    let n: [f32; 3] = normal(p);
    if n[1].abs() < 1e-6 {
        return p[0][1];
    }
    p[0][1] - (n[0] * (x - p[0][0]) + n[2] * (z - p[0][2])) / n[1]
}

fn seg(p: [f32; 3], a: [f32; 3], b: [f32; 3]) -> f32 {
    let (dx, dz): (f32, f32) = (b[0] - a[0], b[2] - a[2]);
    let l: f32 = dx * dx + dz * dz;
    let t: f32 = if l > 0.0 { (((p[0] - a[0]) * dx + (p[2] - a[2]) * dz) / l).clamp(0.0, 1.0) } else { 0.0 };
    (p[0] - a[0] - t * dx).hypot(p[2] - a[2] - t * dz)
}

fn root(up: &mut [u32], mut i: u32) -> u32 {
    while up[i as usize] != i {
        up[i as usize] = up[up[i as usize] as usize];
        i = up[i as usize];
    }
    i
}

fn unit(rng: &mut Rng) -> f32 {
    rng.below(GRAIN) as f32 / GRAIN as f32
}

impl Ground {
    pub fn build(meshes: &[Mesh]) -> Ground {
        let mut g: Ground = Ground::default();
        let mut up: Vec<u32> = Vec::new();
        for m in meshes {
            let base: u32 = g.faces.len() as u32;
            for (i, vs) in m.faces.iter().enumerate() {
                let pts: Vec<[f32; 3]> = vs.iter().map(|v| m.verts[*v as usize]).collect();
                let n: [f32; 3] = normal(&pts);
                let l: f32 = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-9);
                let slope: f32 = (n[1].abs() / l).min(1.0).acos().to_degrees();
                g.faces.push(Face { pts: pts.clone(), slope, area: l / 2.0, patch: 0 });
                up.push(base + i as u32);
                for k in 0..pts.len() {
                    if m.open[i][k] {
                        g.walls.push([pts[k], pts[(k + 1) % pts.len()]]);
                    }
                }
            }
            for (i, nx) in m.next.iter().enumerate() {
                for f in nx.iter().filter(|f| **f >= 0 && (**f as usize) < m.faces.len()) {
                    let (a, b): (u32, u32) = (root(&mut up, base + i as u32), root(&mut up, base + *f as u32));
                    if a != b {
                        up[a as usize] = b;
                    }
                }
            }
        }
        let mut ids: Map<u32, u32> = Map::new();
        for i in 0..g.faces.len() {
            let r: u32 = root(&mut up, i as u32);
            let n: u32 = ids.len() as u32;
            let p: u32 = *ids.entry(r).or_insert(n);
            g.faces[i].patch = p;
            if g.patch_area.len() <= p as usize {
                g.patch_area.resize(p as usize + 1, 0.0);
            }
            g.patch_area[p as usize] += g.faces[i].area;
        }
        let mut sum: f32 = 0.0;
        for (i, f) in g.faces.iter().enumerate() {
            sum += f.area;
            g.sums.push(sum);
            let (lo, hi): ((i32, i32), (i32, i32)) = Ground::span(&f.pts);
            for x in lo.0..=hi.0 {
                for z in lo.1..=hi.1 {
                    g.cells.entry((x, z)).or_default().push(i as u32);
                }
            }
        }
        g.total = sum;
        for (i, w) in g.walls.iter().enumerate() {
            let (lo, hi): ((i32, i32), (i32, i32)) = Ground::span(&w[..]);
            for x in lo.0..=hi.0 {
                for z in lo.1..=hi.1 {
                    g.wcells.entry((x, z)).or_default().push(i as u32);
                }
            }
        }
        g
    }

    fn span(p: &[[f32; 3]]) -> ((i32, i32), (i32, i32)) {
        let lo: (i32, i32) = key(p.iter().map(|v| v[0]).fold(f32::MAX, f32::min), p.iter().map(|v| v[2]).fold(f32::MAX, f32::min));
        let hi: (i32, i32) = key(p.iter().map(|v| v[0]).fold(f32::MIN, f32::max), p.iter().map(|v| v[2]).fold(f32::MIN, f32::max));
        (lo, hi)
    }

    pub fn floors(&self, x: f32, z: f32) -> Vec<(f32, u32)> {
        let mut out: Vec<(f32, u32)> = Vec::new();
        for i in self.cells.get(&key(x, z)).map(|v| v.as_slice()).unwrap_or(&[]) {
            let f: &Face = &self.faces[*i as usize];
            if inside(&f.pts, x, z) {
                out.push((height(&f.pts, x, z), *i));
            }
        }
        out
    }

    pub fn floor_near(&self, x: f32, y: f32, z: f32, up: f32, down: f32) -> Option<(f32, u32)> {
        self.floors(x, z).into_iter().filter(|f| f.0 <= y + up && y - f.0 <= down).min_by(|a, b| (a.0 - y).abs().total_cmp(&(b.0 - y).abs()))
    }

    pub fn shore(&self, p: [f32; 3], max: f32) -> f32 {
        let mut best: f32 = max + SHORE_STEP;
        for k in 0..SHORE_RAYS {
            let a: f32 = k as f32 * std::f32::consts::TAU / SHORE_RAYS as f32;
            let mut r: f32 = SHORE_STEP;
            while r < best.min(max + SHORE_STEP) {
                if self.floor_near(p[0] + r * a.cos(), p[1], p[2] + r * a.sin(), SHORE_UP, SHORE_DOWN).is_some() {
                    best = best.min(r);
                    break;
                }
                r += SHORE_STEP;
            }
        }
        best
    }

    pub fn roof(&self, p: [f32; 3], within: f32) -> bool {
        self.floors(p[0], p[2]).into_iter().any(|f| f.0 > p[1] + SHORE_UP && f.0 <= p[1] + within)
    }

    pub fn clear(&self, p: [f32; 3]) -> f32 {
        let mut best: f32 = CLEAR_MAX;
        let r: i32 = (CLEAR_MAX / CELL).ceil() as i32;
        let c: (i32, i32) = key(p[0], p[2]);
        let mut seen: Vec<u32> = Vec::new();
        for x in c.0 - r..=c.0 + r {
            for z in c.1 - r..=c.1 + r {
                for i in self.wcells.get(&(x, z)).map(|v| v.as_slice()).unwrap_or(&[]) {
                    if seen.contains(i) {
                        continue;
                    }
                    seen.push(*i);
                    let w: [[f32; 3]; 2] = self.walls[*i as usize];
                    if (w[0][1].min(w[1][1]) - WALL_BAND..=w[0][1].max(w[1][1]) + WALL_BAND).contains(&p[1]) {
                        best = best.min(seg(p, w[0], w[1]));
                    }
                }
            }
        }
        best
    }

    pub fn steep(&self, p: [f32; 3], r: f32) -> Option<f32> {
        if r <= 0.0 {
            return Some(0.0);
        }
        let mut worst: f32 = 0.0;
        for k in 0..8 {
            let a: f32 = k as f32 * std::f32::consts::FRAC_PI_4;
            let (x, z): (f32, f32) = (p[0] + r * a.cos(), p[2] + r * a.sin());
            let (y, _): (f32, u32) = self.floor_near(x, p[1], z, r + 0.5, r + 0.5)?;
            worst = worst.max(((y - p[1]).abs() / r).atan().to_degrees());
        }
        Some(worst)
    }

    pub fn pick(&self, rng: &mut Rng) -> Option<([f32; 3], u32)> {
        if self.total <= 0.0 {
            return None;
        }
        let t: f32 = unit(rng) * self.total;
        let i: usize = self.sums.partition_point(|s| *s < t).min(self.faces.len() - 1);
        Some((self.point_in(i, rng), i as u32))
    }

    pub fn point_in(&self, i: usize, rng: &mut Rng) -> [f32; 3] {
        let f: &Face = &self.faces[i];
        let k: usize = 1 + rng.below((f.pts.len().max(3) - 2) as u64) as usize;
        let (a, b, c): ([f32; 3], [f32; 3], [f32; 3]) = (f.pts[0], f.pts[k.min(f.pts.len() - 2)], f.pts[(k + 1).min(f.pts.len() - 1)]);
        let (mut u, mut v): (f32, f32) = (unit(rng), unit(rng));
        if u + v > 1.0 {
            u = 1.0 - u;
            v = 1.0 - v;
        }
        let (x, z): (f32, f32) = (a[0] + u * (b[0] - a[0]) + v * (c[0] - a[0]), a[2] + u * (b[2] - a[2]) + v * (c[2] - a[2]));
        [x, height(&f.pts, x, z), z]
    }

    pub fn patch_of(&self, face: u32) -> f32 {
        self.faces.get(face as usize).map(|f| self.patch_area[f.patch as usize]).unwrap_or(0.0)
    }
}

pub fn load(rf: &mut Romfs, map: &str) -> Res<Option<Ground>> {
    rf.require(NAV_DIR)?;
    rf.require(STITCHED_DIR)?;
    let one: String = format!("{NAV_DIR}/{map}{NAV_EXT}");
    let mut meshes: Vec<Mesh> = Vec::new();
    if rf.root.join(&one).is_file() {
        let z: Vec<u8> = rf.read(&one)?;
        meshes.push(parse(&rf.zs.decompress(&z)?, [0.0; 3])?);
    } else if rf.root.join(STITCHED_DIR).join(map).is_dir() {
        for f in rf.list(&format!("{STITCHED_DIR}/{map}"))? {
            let Some(shift) = tile_shift(&f) else {
                continue;
            };
            let m: Mesh = parse(&rf.zs.decompress(&rf.read(&f)?)?, shift)?;
            if !filler(&m) {
                meshes.push(m);
            }
        }
    }
    if meshes.is_empty() {
        return Ok(None);
    }
    let mut g: Ground = Ground::build(&meshes);
    g.tiles = !rf.root.join(&one).is_file();
    Ok(Some(g))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(at: [f32; 3], s: f32, rise: f32) -> Mesh {
        let verts: Vec<[f32; 3]> = vec![at, [at[0] + s, at[1] + rise, at[2]], [at[0] + s, at[1] + rise, at[2] + s], [at[0], at[1], at[2] + s]];
        Mesh { verts, faces: vec![vec![0, 1, 2, 3]], open: vec![vec![true; 4]], next: vec![vec![-1; 4]] }
    }

    #[test]
    fn a_game_folder_without_walk_meshes_is_an_error_and_not_an_empty_map() {
        let dir: std::path::PathBuf = std::env::temp_dir().join(format!("ootrr_no_meshes_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut rf: Romfs = Romfs { root: dir.clone(), zs: crate::zs::Zs::new() };
        let e: String = load(&mut rf, "DodongoCavern").err().unwrap().to_string();
        assert!(e.contains(NAV_DIR), "{e}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_flat_square_reads_its_height_slope_and_room() {
        let g: Ground = Ground::build(&[square([0.0, 10.0, 0.0], 20.0, 0.0)]);
        assert_eq!(g.faces.len(), 1);
        assert!(g.faces[0].slope < 0.01 && (g.faces[0].area - 400.0).abs() < 0.01);
        let (y, _): (f32, u32) = g.floor_near(5.0, 10.5, 5.0, 1.0, 1.0).unwrap();
        assert!((y - 10.0).abs() < 1e-4);
        assert!(g.floor_near(25.0, 10.0, 5.0, 1.0, 1.0).is_none());
        assert!((g.clear([10.0, 10.0, 10.0]) - 10.0).abs() < 1e-3);
        assert!((g.clear([2.0, 10.0, 10.0]) - 2.0).abs() < 1e-3);
        assert!(g.steep([10.0, 10.0, 10.0], 2.0).unwrap() < 0.01);
        assert!(g.steep([0.5, 10.0, 10.0], 2.0).is_none());
        let mut rng: Rng = Rng::from_text("pick");
        for _ in 0..100 {
            let (p, _): ([f32; 3], u32) = g.pick(&mut rng).unwrap();
            assert!((0.0..=20.0).contains(&p[0]) && (0.0..=20.0).contains(&p[2]) && (p[1] - 10.0).abs() < 1e-4);
        }
    }

    #[test]
    fn a_ramp_is_steep_and_two_floors_are_told_apart() {
        let g: Ground = Ground::build(&[square([0.0, 0.0, 0.0], 10.0, 10.0), square([0.0, 30.0, 0.0], 10.0, 0.0)]);
        assert!((g.faces[0].slope - 45.0).abs() < 0.1);
        assert!((g.steep([5.0, 5.0, 5.0], 1.0).unwrap() - 45.0).abs() < 1.0);
        assert_eq!(g.floors(5.0, 5.0).len(), 2);
        assert!((g.floor_near(5.0, 31.0, 5.0, 2.0, 2.0).unwrap().0 - 30.0).abs() < 1e-4);
        assert_eq!(g.patch_area.len(), 2);
    }

    #[test]
    fn shore_and_roof_tell_a_pool_from_the_void() {
        let g: Ground = Ground::build(&[square([0.0, 10.0, 0.0], 10.0, 0.0), square([0.0, 18.0, 30.0], 10.0, 0.0), square([0.0, 15.0, 0.0], 4.0, 0.0)]);
        assert!(g.shore([14.0, 10.0, 5.0], 9.0) <= 4.5);
        assert!(g.shore([30.0, 10.0, 5.0], 9.0) > 9.0);
        assert!(g.shore([5.0, 10.0, 25.0], 9.0) > 9.0);
        assert!(g.roof([2.0, 10.0, 2.0], 10.0));
        assert!(!g.roof([2.0, 10.0, 2.0], 4.0));
        assert!(!g.roof([8.0, 10.0, 8.0], 10.0));
    }

    #[test]
    fn tiles_are_shifted_from_their_names() {
        assert_eq!(tile_shift("Phive/StitchedNavMesh/HyruleField_Child/X8_Z8.Nin_Ounce_NVN2.bphnm.zs"), Some([125.0, 0.0, 125.0]));
        assert_eq!(tile_shift("X0_Z15.Nin_Ounce_NVN2.bphnm.zs"), Some([-1875.0, 0.0, 1875.0]));
        assert_eq!(tile_shift("KokiriForest_Child.Nin_Ounce_NVN2.bphnm.zs"), None);
    }

    fn dump() -> Option<std::path::PathBuf> {
        let p: std::path::PathBuf = std::env::var_os("OOTRR_ROMFS").map(std::path::PathBuf::from)?;
        if p.is_dir() { Some(p) } else { None }
    }

    #[test]
    fn the_game_navmeshes_put_placed_enemies_on_the_ground() {
        let Some(root) = dump() else {
            eprintln!("skipped: set OOTRR_ROMFS");
            return;
        };
        let mut rf: Romfs = Romfs::open(&root).unwrap();
        for (map, file, kind) in [("KokiriForest_Child", "Banc/KokiriForest/KokiriForest_Child_Dynamic.bcett.byml.zs", "Enemy_Dekubaba_Withered"), ("HyruleField_Adult", "Banc/HyruleField/HyruleField_Adult_Static.bcett.byml.zs", "Enemy_Tektite_Blue"), ("DodongoCavern", "Banc/DodongoCavern/DodongoCavern_Dynamic.bcett.byml.zs", "Enemy_Dodongo")] {
            let g: Ground = load(&mut rf, map).unwrap().unwrap();
            assert!(g.faces.len() > 100 && g.total > 1000.0, "{map}");
            let y: crate::byml::Byml = rf.read_byml(file).unwrap();
            let mut on: usize = 0;
            let mut all: usize = 0;
            for a in y.root.get("Actors").unwrap().as_arr().unwrap() {
                if a.get("Gyaml").and_then(|v| v.as_str()) != Some(kind) {
                    continue;
                }
                let p: [f32; 3] = crate::difficulty::extras::pos(a).unwrap();
                all += 1;
                if g.floor_near(p[0], p[1], p[2], 1.0, 1.0).is_some() {
                    on += 1;
                }
            }
            assert!(all > 0 && on * 2 >= all, "{map}: {on} of {all} {kind} stand on the navmesh");
        }
        let mut bad: Vec<String> = Vec::new();
        for f in rf.list(NAV_DIR).unwrap().into_iter().filter(|f| !f.contains("Template")) {
            if let Err(e) = parse(&rf.zs.decompress(&rf.read(&f).unwrap()).unwrap(), [0.0; 3]) {
                bad.push(format!("{f}: {e}"));
            }
        }
        assert!(bad.is_empty(), "{bad:?}");
    }
}
