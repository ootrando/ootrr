pub fn to_f32(h: u16) -> f32 {
    let sign: u32 = ((h >> 15) as u32) << 31;
    let exp: u32 = ((h >> 10) & 0x1f) as u32;
    let man: u32 = (h & 0x3ff) as u32;
    let bits: u32 = match (exp, man) {
        (0, 0) => sign,
        (0, m) => {
            let v: f32 = m as f32 / 1024.0 / 16384.0;
            return if sign != 0 { -v } else { v };
        }
        (0x1f, m) => sign | 0x7f80_0000 | (m << 13),
        (e, m) => sign | ((e + 112) << 23) | (m << 13),
    };
    f32::from_bits(bits)
}

pub fn from_f32(v: f32) -> u16 {
    let bits: u32 = v.to_bits();
    let sign: u16 = ((bits >> 16) & 0x8000) as u16;
    let a: f32 = v.abs();
    if a.is_nan() {
        return sign | 0x7e00;
    }
    if a >= 65520.0 {
        return sign | 0x7c00;
    }
    if a < 6.103_515_6e-5 {
        let m: u16 = (a * 16384.0 * 1024.0).round() as u16;
        return sign | m;
    }
    let e: i32 = ((a.to_bits() >> 23) & 0xff) as i32 - 127;
    let man: u32 = a.to_bits() & 0x7f_ffff;
    let mut h: u32 = (((e + 15) as u32) << 10) | (man >> 13);
    let rest: u32 = man & 0x1fff;
    if rest > 0x1000 || (rest == 0x1000 && (h & 1) == 1) {
        h += 1;
    }
    sign | h as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_halves_decode() {
        assert_eq!(to_f32(0x3c00), 1.0);
        assert_eq!(to_f32(0xc000), -2.0);
        assert_eq!(to_f32(0x3555), 0.333_251_95);
        assert_eq!(to_f32(0x0000), 0.0);
        assert_eq!(to_f32(0x0001), 5.960_464_5e-8);
        assert!(to_f32(0x7c00).is_infinite());
    }

    #[test]
    fn every_finite_half_round_trips() {
        for h in 0..=u16::MAX {
            let e: u16 = (h >> 10) & 0x1f;
            if e == 0x1f || h == 0x8000 {
                continue;
            }
            assert_eq!(from_f32(to_f32(h)), h, "{h:#06x}");
        }
    }

    #[test]
    fn encoding_rounds_to_the_nearest_half() {
        assert_eq!(from_f32(0.1), 0x2e66);
        assert_eq!(from_f32(-0.0459), 0xa9e0);
        assert_eq!(from_f32(1e6), 0x7c00);
    }
}
