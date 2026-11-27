use crate::error::{Error, Res};
use crate::fres::bin::{f32_at, name, ptr_at, u16_at, u32_at};
use crate::fres::space::{euler, mul, quat, srt, Mat};

pub const MODELS: usize = 0x28;
pub const GPU_SIZE: usize = 0x16c;
pub const GPU_AT: usize = 0x170;
pub const FVTX: usize = 0x58;
pub const FSHP: usize = 0x60;
pub const FMAT: usize = 0xb0;
pub const MESH: usize = 0x38;
pub const BONE: usize = 0x58;
pub const BOX: usize = 24;
pub const SPHERE: usize = 16;
pub const MAT_INDEX: usize = 0x338;

#[derive(Clone, Debug)]
pub struct Bone {
    pub name: String,
    pub parent: Option<usize>,
    pub rigid: Option<usize>,
    pub world: Mat,
}

#[derive(Clone, Debug)]
pub struct Attr {
    pub name: String,
    pub format: u32,
    pub offset: usize,
    pub buffer: usize,
}

#[derive(Clone, Debug)]
pub struct Vtx {
    pub at: usize,
    pub attrs: Vec<Attr>,
    pub offset: usize,
    pub count: usize,
    pub stride: usize,
    pub skin: u8,
}

#[derive(Clone, Debug)]
pub struct Mesh {
    pub at: usize,
    pub offset: usize,
    pub format: u32,
    pub count: usize,
    pub first: usize,
}

#[derive(Clone, Debug)]
pub struct Shape {
    pub at: usize,
    pub name: String,
    pub vtx: usize,
    pub skin_at: usize,
    pub boxes: usize,
    pub spheres: usize,
    pub mat: u16,
    pub vbi: u16,
    pub meshes: Vec<Mesh>,
}

#[derive(Clone, Debug)]
pub struct Model {
    pub fmdl: usize,
    pub fvtx: usize,
    pub fshp: usize,
    pub fmat: usize,
    pub nv: usize,
    pub ns: usize,
    pub nm: usize,
    pub bones: Vec<Bone>,
    pub rigid: Vec<usize>,
    pub shapes: Vec<Shape>,
    pub vtx: Vec<Vtx>,
    pub gpu: usize,
    pub gpu_size: usize,
}

fn magic(b: &[u8], at: usize, m: &[u8; 4], what: &'static str) -> Res<()> {
    if b.get(at..at + 4) != Some(m.as_slice()) {
        return Err(Error::Format(what));
    }
    Ok(())
}

fn byte(b: &[u8], o: usize) -> Res<u8> {
    b.get(o).copied().ok_or(Error::Format("fres: the file ends inside a field"))
}

fn bones(b: &[u8], sk: usize) -> Res<(Vec<Bone>, Vec<usize>)> {
    magic(b, sk, b"FSKL", "fres: the model has no skeleton")?;
    let flags: u32 = u32_at(b, sk + 4)?;
    let arr: usize = ptr_at(b, sk + 0x10)?;
    let list: usize = ptr_at(b, sk + 0x18)?;
    let n: usize = u16_at(b, sk + 0x38)? as usize;
    let mats: usize = u16_at(b, sk + 0x3a)? as usize + u16_at(b, sk + 0x3c)? as usize;
    let by_quat: bool = (flags >> 12) & 3 == 0;
    let mut out: Vec<Bone> = Vec::new();
    for i in 0..n {
        let a: usize = arr + BONE * i;
        let par: u16 = u16_at(b, a + 0x22)?;
        let rig: u16 = u16_at(b, a + 0x26)?;
        let f = |k: usize| -> Res<f64> { Ok(f32_at(b, a + k)? as f64) };
        let s: [f64; 3] = [f(0x30)?, f(0x34)?, f(0x38)?];
        let r: [f64; 4] = [f(0x3c)?, f(0x40)?, f(0x44)?, f(0x48)?];
        let t: [f64; 3] = [f(0x4c)?, f(0x50)?, f(0x54)?];
        let turn: [[f64; 3]; 3] = if by_quat { quat(r) } else { euler([r[0], r[1], r[2]]) };
        let local: Mat = srt(s, turn, t);
        let parent: Option<usize> = (par != u16::MAX).then_some(par as usize);
        let world: Mat = match parent {
            Some(p) => mul(&out.get(p).ok_or(Error::Format("fres: a bone comes before its parent"))?.world, &local),
            None => local,
        };
        out.push(Bone { name: name(b, ptr_at(b, a)?)?, parent, rigid: (rig != u16::MAX).then_some(rig as usize), world });
    }
    let rigid: Vec<usize> = (0..mats).map(|i| u16_at(b, list + 2 * i).map(|v| v as usize)).collect::<Res<Vec<usize>>>()?;
    Ok((out, rigid))
}

fn vtx(b: &[u8], at: usize) -> Res<Vtx> {
    magic(b, at, b"FVTX", "fres: a vertex buffer has no FVTX magic")?;
    let aa: usize = ptr_at(b, at + 8)?;
    let na: usize = byte(b, at + 0x4c)? as usize;
    let mut attrs: Vec<Attr> = Vec::new();
    for k in 0..na {
        let q: usize = aa + 0x10 * k;
        attrs.push(Attr { name: name(b, ptr_at(b, q)?)?, format: u32_at(b, q + 8)?, offset: u16_at(b, q + 12)? as usize, buffer: u16_at(b, q + 14)? as usize });
    }
    let strides: usize = ptr_at(b, at + 0x38)?;
    Ok(Vtx { at, attrs, offset: u32_at(b, at + 0x48)? as usize, count: u32_at(b, at + 0x50)? as usize, stride: u32_at(b, strides)? as usize, skin: byte(b, at + 0x54)? })
}

fn shape(b: &[u8], at: usize) -> Res<Shape> {
    magic(b, at, b"FSHP", "fres: a shape has no FSHP magic")?;
    let ma: usize = ptr_at(b, at + 0x18)?;
    let n: usize = byte(b, at + 0x5b)? as usize;
    let mut meshes: Vec<Mesh> = Vec::new();
    for k in 0..n {
        let m: usize = ma + MESH * k;
        meshes.push(Mesh { at: m, offset: u32_at(b, m + 0x20)? as usize, format: u32_at(b, m + 0x28)?, count: u32_at(b, m + 0x2c)? as usize, first: u32_at(b, m + 0x30)? as usize });
    }
    Ok(Shape {
        at,
        name: name(b, ptr_at(b, at + 8)?)?,
        vtx: ptr_at(b, at + 0x10)?,
        skin_at: ptr_at(b, at + 0x20)?,
        boxes: ptr_at(b, at + 0x38)?,
        spheres: ptr_at(b, at + 0x40)?,
        mat: u16_at(b, at + 0x52)?,
        vbi: u16_at(b, at + 0x56)?,
        meshes,
    })
}

pub fn read(b: &[u8]) -> Res<Model> {
    magic(b, 0, b"FRES", "fres: bad magic")?;
    let fmdl: usize = ptr_at(b, MODELS)?;
    magic(b, fmdl, b"FMDL", "fres: the first model has no FMDL magic")?;
    let (bones, rigid): (Vec<Bone>, Vec<usize>) = bones(b, ptr_at(b, fmdl + 0x18)?)?;
    let fvtx: usize = ptr_at(b, fmdl + 0x20)?;
    let fshp: usize = ptr_at(b, fmdl + 0x28)?;
    let fmat: usize = ptr_at(b, fmdl + 0x38)?;
    let (nv, ns, nm): (usize, usize, usize) = (u16_at(b, fmdl + 0x68)? as usize, u16_at(b, fmdl + 0x6a)? as usize, u16_at(b, fmdl + 0x6c)? as usize);
    let vtxs: Vec<Vtx> = (0..nv).map(|i| vtx(b, fvtx + FVTX * i)).collect::<Res<Vec<Vtx>>>()?;
    let shapes: Vec<Shape> = (0..ns).map(|i| shape(b, fshp + FSHP * i)).collect::<Res<Vec<Shape>>>()?;
    for i in 0..nm {
        magic(b, fmat + FMAT * i, b"FMAT", "fres: a material has no FMAT magic")?;
    }
    Ok(Model { fmdl, fvtx, fshp, fmat, nv, ns, nm, bones, rigid, shapes, vtx: vtxs, gpu: ptr_at(b, GPU_AT)?, gpu_size: u32_at(b, GPU_SIZE)? as usize })
}

pub fn mat_name(b: &[u8], m: &Model, i: usize) -> Res<String> {
    name(b, ptr_at(b, m.fmat + FMAT * i + 8)?)
}

impl Model {
    pub fn bone(&self, n: &str) -> Option<usize> {
        self.bones.iter().position(|x| x.name == n)
    }

    pub fn attr(v: &Vtx, n: &str) -> Option<Attr> {
        v.attrs.iter().find(|a| a.name == n).cloned()
    }

    pub fn first_vertex(&self) -> usize {
        self.vtx.iter().map(|v| v.offset).min().unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_without_the_magic_is_refused() {
        assert!(read(&[0u8; 0x200]).is_err());
        let mut b: Vec<u8> = vec![0u8; 0x200];
        b[..4].copy_from_slice(b"FRES");
        assert!(read(&b).is_err());
    }
}
