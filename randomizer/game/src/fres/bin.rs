use crate::error::{Error, Res};

fn short() -> Error {
    Error::Format("fres: the file ends inside a field")
}

pub fn get<const N: usize>(b: &[u8], o: usize) -> Res<[u8; N]> {
    let s: &[u8] = b.get(o..o.checked_add(N).ok_or_else(short)?).ok_or_else(short)?;
    let mut out: [u8; N] = [0; N];
    out.copy_from_slice(s);
    Ok(out)
}

pub fn u16_at(b: &[u8], o: usize) -> Res<u16> {
    Ok(u16::from_le_bytes(get::<2>(b, o)?))
}

pub fn u32_at(b: &[u8], o: usize) -> Res<u32> {
    Ok(u32::from_le_bytes(get::<4>(b, o)?))
}

pub fn u64_at(b: &[u8], o: usize) -> Res<u64> {
    Ok(u64::from_le_bytes(get::<8>(b, o)?))
}

pub fn f32_at(b: &[u8], o: usize) -> Res<f32> {
    Ok(f32::from_le_bytes(get::<4>(b, o)?))
}

pub fn ptr_at(b: &[u8], o: usize) -> Res<usize> {
    let v: u64 = u64_at(b, o)?;
    usize::try_from(v).ok().filter(|p| *p < b.len()).ok_or(Error::Format("fres: a pointer leaves the file"))
}

pub fn put(b: &mut [u8], o: usize, v: &[u8]) -> Res<()> {
    let s: &mut [u8] = b.get_mut(o..o.checked_add(v.len()).ok_or_else(short)?).ok_or_else(short)?;
    s.copy_from_slice(v);
    Ok(())
}

pub fn put16(b: &mut [u8], o: usize, v: u16) -> Res<()> {
    put(b, o, &v.to_le_bytes())
}

pub fn put32(b: &mut [u8], o: usize, v: u32) -> Res<()> {
    put(b, o, &v.to_le_bytes())
}

pub fn put64(b: &mut [u8], o: usize, v: u64) -> Res<()> {
    put(b, o, &v.to_le_bytes())
}

pub fn put_f32(b: &mut [u8], o: usize, v: f32) -> Res<()> {
    put(b, o, &v.to_le_bytes())
}

pub fn name(b: &[u8], o: usize) -> Res<String> {
    let n: usize = u16_at(b, o)? as usize;
    let s: &[u8] = b.get(o + 2..o + 2 + n).ok_or(Error::Format("fres: a name leaves the file"))?;
    Ok(String::from_utf8_lossy(s).into_owned())
}

pub fn align(x: usize, a: usize) -> usize {
    x.div_ceil(a) * a
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fields_read_back_what_was_written_and_refuse_the_end() {
        let mut b: Vec<u8> = vec![0; 16];
        put16(&mut b, 0, 0x1234).unwrap();
        put32(&mut b, 2, 0xdead_beef).unwrap();
        put64(&mut b, 8, 7).unwrap();
        assert_eq!(u16_at(&b, 0).unwrap(), 0x1234);
        assert_eq!(u32_at(&b, 2).unwrap(), 0xdead_beef);
        assert_eq!(ptr_at(&b, 8).unwrap(), 7);
        assert!(u32_at(&b, 14).is_err());
        assert!(put64(&mut b, 12, 1).is_err());
        assert_eq!(align(0x658c, 0x100), 0x6600);
        assert_eq!(align(0x100, 0x100), 0x100);
    }

    #[test]
    fn a_name_is_a_length_then_its_bytes() {
        let b: Vec<u8> = vec![3, 0, b'_', b'p', b'0', 0];
        assert_eq!(name(&b, 0).unwrap(), "_p0");
        assert!(name(&[9, 0, b'a'], 0).is_err());
    }
}
