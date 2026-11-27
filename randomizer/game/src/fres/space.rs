pub type Mat = [[f64; 4]; 4];

pub const ID: Mat = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]];

pub fn mul(a: &Mat, b: &Mat) -> Mat {
    let mut out: Mat = [[0.0; 4]; 4];
    for (i, row) in out.iter_mut().enumerate() {
        for (j, v) in row.iter_mut().enumerate() {
            *v = (0..4).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    out
}

pub fn point(m: &Mat, p: [f64; 3]) -> [f64; 3] {
    [0, 1, 2].map(|i| m[i][0] * p[0] + m[i][1] * p[1] + m[i][2] * p[2] + m[i][3])
}

pub fn dir(m: &Mat, v: [f64; 3]) -> [f64; 3] {
    let r: [f64; 3] = [0, 1, 2].map(|i| m[i][0] * v[0] + m[i][1] * v[1] + m[i][2] * v[2]);
    let n: f64 = (r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt();
    if n > 0.0 { r.map(|x| x / n) } else { r }
}

pub fn euler(r: [f64; 3]) -> [[f64; 3]; 3] {
    let (sx, cx): (f64, f64) = r[0].sin_cos();
    let (sy, cy): (f64, f64) = r[1].sin_cos();
    let (sz, cz): (f64, f64) = r[2].sin_cos();
    [[cy * cz, sx * sy * cz - cx * sz, cx * sy * cz + sx * sz], [cy * sz, sx * sy * sz + cx * cz, cx * sy * sz - sx * cz], [-sy, sx * cy, cx * cy]]
}

pub fn quat(q: [f64; 4]) -> [[f64; 3]; 3] {
    let [x, y, z, w]: [f64; 4] = q;
    [
        [1.0 - 2.0 * (y * y + z * z), 2.0 * (x * y - z * w), 2.0 * (x * z + y * w)],
        [2.0 * (x * y + z * w), 1.0 - 2.0 * (x * x + z * z), 2.0 * (y * z - x * w)],
        [2.0 * (x * z - y * w), 2.0 * (y * z + x * w), 1.0 - 2.0 * (x * x + y * y)],
    ]
}

pub fn srt(s: [f64; 3], r: [[f64; 3]; 3], t: [f64; 3]) -> Mat {
    let mut out: Mat = ID;
    for i in 0..3 {
        for j in 0..3 {
            out[i][j] = r[i][j] * s[j];
        }
        out[i][3] = t[i];
    }
    out
}

pub fn invert(m: &Mat) -> Option<Mat> {
    let r: [[f64; 3]; 3] = [0, 1, 2].map(|i| [m[i][0], m[i][1], m[i][2]]);
    let det: f64 = r[0][0] * (r[1][1] * r[2][2] - r[1][2] * r[2][1]) - r[0][1] * (r[1][0] * r[2][2] - r[1][2] * r[2][0]) + r[0][2] * (r[1][0] * r[2][1] - r[1][1] * r[2][0]);
    if det.abs() < 1e-12 {
        return None;
    }
    let mut out: Mat = ID;
    for i in 0..3 {
        for j in 0..3 {
            out[i][j] = (r[(j + 1) % 3][(i + 1) % 3] * r[(j + 2) % 3][(i + 2) % 3] - r[(j + 1) % 3][(i + 2) % 3] * r[(j + 2) % 3][(i + 1) % 3]) / det;
        }
    }
    let t: [f64; 3] = [m[0][3], m[1][3], m[2][3]];
    for i in 0..3 {
        out[i][3] = -(0..3).map(|k| out[i][k] * t[k]).sum::<f64>();
    }
    Some(out)
}

pub fn snorm10(w: u32, shift: u32) -> f64 {
    let v: i32 = ((w >> shift) & 0x3ff) as i32;
    let s: i32 = if v & 0x200 != 0 { v - 0x400 } else { v };
    (s as f64 / 511.0).max(-1.0)
}

pub fn pack10(v: [f64; 3], keep: u32) -> u32 {
    let e = |x: f64| ((x.clamp(-1.0, 1.0) * 511.0).round() as i32 as u32) & 0x3ff;
    e(v[0]) | (e(v[1]) << 10) | (e(v[2]) << 20) | (keep & 0xc000_0000)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: [f64; 3], b: [f64; 3]) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-9)
    }

    #[test]
    fn an_inverse_undoes_scale_turn_and_move() {
        let m: Mat = srt([0.5, 0.5, 0.5], euler([0.3, 1.2, -0.7]), [1.0, -2.0, 0.25]);
        let inv: Mat = invert(&m).unwrap();
        let p: [f64; 3] = [0.1, 0.2, 0.3];
        assert!(near(point(&inv, point(&m, p)), p));
        assert!(near(point(&mul(&inv, &m), p), p));
        assert!(invert(&[[0.0; 4]; 4]).is_none());
    }

    #[test]
    fn a_quarter_turn_quaternion_matches_the_euler_turn() {
        let h: f64 = std::f64::consts::FRAC_1_SQRT_2;
        let q: [[f64; 3]; 3] = quat([0.0, 0.0, h, h]);
        let e: [[f64; 3]; 3] = euler([0.0, 0.0, std::f64::consts::FRAC_PI_2]);
        for i in 0..3 {
            assert!(near(q[i], e[i]));
        }
    }

    #[test]
    fn packed_normals_round_trip_and_keep_the_top_bits() {
        let w: u32 = pack10([1.0, -1.0, 0.5], 0xc000_0000);
        assert_eq!(w >> 30, 3);
        assert!((snorm10(w, 0) - 1.0).abs() < 1e-9);
        assert!((snorm10(w, 10) + 1.0).abs() < 1e-9);
        assert!((snorm10(w, 20) - 0.5).abs() < 2e-3);
        assert_eq!(snorm10(0x200, 0), -1.0);
        assert!(near(dir(&ID, [0.0, 3.0, 4.0]), [0.0, 0.6, 0.8]));
    }
}
