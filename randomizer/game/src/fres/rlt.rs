use crate::error::{Error, Res};
use crate::fres::bin::{u16_at, u32_at, u64_at};

pub const MAGIC: &[u8; 4] = b"_RLT";
pub const AT: usize = 0x18;
pub const SIZE_AT: usize = 0x1c;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Section {
    pub pos: usize,
    pub size: usize,
    pub slots: Vec<usize>,
}

pub fn offset(b: &[u8]) -> Res<usize> {
    let o: usize = u32_at(b, AT)? as usize;
    if b.get(o..o + 4) != Some(MAGIC.as_slice()) {
        return Err(Error::Format("fres: no relocation table where the header says"));
    }
    Ok(o)
}

pub fn read(b: &[u8]) -> Res<Vec<Section>> {
    let o: usize = offset(b)?;
    let n: usize = u32_at(b, o + 8)? as usize;
    let mut heads: Vec<(usize, usize, usize, usize)> = Vec::new();
    for i in 0..n {
        let p: usize = o + 16 + 24 * i;
        heads.push((u32_at(b, p + 8)? as usize, u32_at(b, p + 12)? as usize, u32_at(b, p + 16)? as usize, u32_at(b, p + 20)? as usize));
    }
    let ents: usize = o + 16 + 24 * n;
    let mut out: Vec<Section> = Vec::new();
    for (pos, size, first, count) in heads {
        let mut slots: Vec<usize> = Vec::new();
        for k in first..first + count {
            let q: usize = ents + 8 * k;
            let at: usize = u32_at(b, q)? as usize;
            let structs: usize = u16_at(b, q + 4)? as usize;
            let ptrs: usize = b.get(q + 6).copied().ok_or(Error::Format("fres: the relocation table ends early"))? as usize;
            let pad: usize = b.get(q + 7).copied().ok_or(Error::Format("fres: the relocation table ends early"))? as usize;
            let mut a: usize = at;
            for _ in 0..structs {
                for j in 0..ptrs {
                    slots.push(a + 8 * j);
                }
                a += 8 * (ptrs + pad);
            }
        }
        out.push(Section { pos, size, slots });
    }
    Ok(out)
}

pub fn write(at: usize, secs: &[Section]) -> Res<Vec<u8>> {
    let mut ents: Vec<(u32, u8)> = Vec::new();
    let mut heads: Vec<(u32, u32, u32, u32)> = Vec::new();
    for s in secs {
        let mut sl: Vec<usize> = s.slots.clone();
        sl.sort_unstable();
        sl.dedup();
        let first: usize = ents.len();
        let mut i: usize = 0;
        while i < sl.len() {
            let mut j: usize = i;
            while j + 1 < sl.len() && sl[j + 1] == sl[j] + 8 && j + 1 - i < 255 {
                j += 1;
            }
            ents.push((u32::try_from(sl[i]).map_err(|_| Error::Format("fres: a pointer slot is past 4 GiB"))?, (j - i + 1) as u8));
            i = j + 1;
        }
        let w = |x: usize| u32::try_from(x).map_err(|_| Error::Format("fres: a section is past 4 GiB"));
        heads.push((w(s.pos)?, w(s.size)?, w(first)?, w(ents.len() - first)?));
    }
    let mut out: Vec<u8> = MAGIC.to_vec();
    out.extend(u32::try_from(at).map_err(|_| Error::Format("fres: the table is past 4 GiB"))?.to_le_bytes());
    out.extend((secs.len() as u32).to_le_bytes());
    out.extend(0u32.to_le_bytes());
    for (pos, size, first, n) in heads {
        out.extend(0u64.to_le_bytes());
        for v in [pos, size, first, n] {
            out.extend(v.to_le_bytes());
        }
    }
    for (pos, n) in ents {
        out.extend(pos.to_le_bytes());
        out.extend(1u16.to_le_bytes());
        out.push(n);
        out.push(0);
    }
    Ok(out)
}

pub fn targets(b: &[u8], s: &Section) -> Res<Vec<u64>> {
    s.slots.iter().map(|p| u64_at(b, *p)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(secs: &[Section], len: usize) -> Vec<u8> {
        let mut b: Vec<u8> = vec![0; len];
        let t: Vec<u8> = write(len, secs).unwrap();
        b[AT..AT + 4].copy_from_slice(&(len as u32).to_le_bytes());
        b.extend(t);
        b
    }

    #[test]
    fn written_sections_read_back_with_runs_grouped() {
        let secs: Vec<Section> = vec![
            Section { pos: 0, size: 0x200, slots: vec![0x20, 0x28, 0x30, 0x58, 0x40] },
            Section { pos: 0x200, size: 0x100, slots: vec![0x170] },
            Section { pos: 0x300, size: 0, slots: Vec::new() },
        ];
        let b: Vec<u8> = file(&secs, 0x300);
        let back: Vec<Section> = read(&b).unwrap();
        assert_eq!(back.len(), 3);
        assert_eq!(back[0].slots, vec![0x20, 0x28, 0x30, 0x40, 0x58]);
        assert_eq!(back[1], Section { pos: 0x200, size: 0x100, slots: vec![0x170] });
        assert!(back[2].slots.is_empty());
        let o: usize = offset(&b).unwrap();
        let first: usize = o + 16 + 24 * 3;
        assert_eq!(b[first + 6], 3);
    }

    #[test]
    fn strided_entries_expand_to_every_struct() {
        let mut b: Vec<u8> = vec![0; 0x100];
        b[AT..AT + 4].copy_from_slice(&0x100u32.to_le_bytes());
        b.extend(MAGIC);
        b.extend(0x100u32.to_le_bytes());
        b.extend(1u32.to_le_bytes());
        b.extend(0u32.to_le_bytes());
        b.extend(0u64.to_le_bytes());
        for v in [0u32, 0x100, 0, 1] {
            b.extend(v.to_le_bytes());
        }
        b.extend(0x10u32.to_le_bytes());
        b.extend(3u16.to_le_bytes());
        b.push(2);
        b.push(1);
        let s: Vec<Section> = read(&b).unwrap();
        assert_eq!(s[0].slots, vec![0x10, 0x18, 0x28, 0x30, 0x40, 0x48]);
    }

    #[test]
    fn a_missing_table_is_refused() {
        assert!(read(&[0u8; 0x40]).is_err());
    }
}
