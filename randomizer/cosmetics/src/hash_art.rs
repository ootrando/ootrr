//! The seed hash on the title screen: a row of the game's own item icons drawn under the
//! "RANDOMIZER" line of the title logo, picked from the seed's checksum. Everyone who builds the
//! same seed sees the same icons, so a race can check that all its players have the same seed
//! without anyone seeing where an item is.
//!
//! The icons are read from the player's game files at build time (UI/Tex/Icon, the pause menu's
//! item icons, 341 x 341 ASTC). Each one is scaled down to 32 x 32 ASTC blocks (128 x 128 texels)
//! and written into the logo texture as void-extent blocks, one flat colour per 4 x 4 block.

use crate::error::{Error as E, Res};
use crate::romfs::Romfs;
use crate::tex::bntx::{self, Tex};
use crate::tex::image::Img;
use crate::tex::swizzle;

/// Icons in the title-screen hash.
pub const SLOTS: usize = 5;
/// ASTC blocks on an icon's side.
pub const SIDE_BLOCKS: usize = 32;
const BLOCK_PX: usize = 4;
/// Texels on an icon's side.
pub const SIDE: usize = SIDE_BLOCKS * BLOCK_PX;
/// Texels between two icons.
const GAP: usize = 32;
/// Where the row sits in the 1664 x 1280 logo: centred under the RANDOMIZER line, clear of the
/// sword's tip on the left (checked against the game's texture in the tests).
pub const TOP: usize = 1120;
pub const CENTER_X: usize = 1128;
/// The logo texture size this layout was made for.
pub const LOGO_W: usize = 1664;
pub const LOGO_H: usize = 1280;
/// Where the game keeps its item icons, one texture per file named after the file.
pub const ICON_DIR: &str = "UI/Tex/Icon";

/// The icons a hash can show: the game's icon id and the name players call it by. They are all
/// ids the app's tracker already reads (web tracker icon_map OWN), so the app can show them too,
/// and no two look alike at a glance.
pub const ICONS: [(&str, &str); 32] = [
    ("Item_DekuStick", "Deku Stick"),
    ("Item_MagicBean", "Magic Bean"),
    ("Item_Bomb", "Bomb"),
    ("Weapon_Range_SlingShot", "Slingshot"),
    ("Item_Boomerang", "Boomerang"),
    ("Item_Bombchu", "Bombchu"),
    ("Weapon_Range_Bow", "Bow"),
    ("Armor_Child_Gauntlet_Goron", "Goron's Bracelet"),
    ("Item_Dungeon_SmallKey", "Small Key"),
    ("Item_Hammer_PouchIcon", "Megaton Hammer"),
    ("Item_Hookshot", "Hookshot"),
    ("Item_DekuNut", "Deku Nut"),
    ("Item_DinFire", "Din's Fire"),
    ("Item_FaroreWind", "Farore's Wind"),
    ("Item_NayruLove", "Nayru's Love"),
    ("Item_Ocarina_Fairy", "Fairy Ocarina"),
    ("Item_Bottle", "Bottle"),
    ("Weapon_Sword_Kokiri", "Kokiri Sword"),
    ("Weapon_Sword_Master", "Master Sword"),
    ("Item_Wallet_Large", "Giant's Wallet"),
    ("Armor_GlassTruth", "Lens of Truth"),
    ("Item_Scale_Gold", "Golden Scale"),
    ("Weapon_Shield_Deku", "Deku Shield"),
    ("Weapon_Shield_Hylian", "Hylian Shield"),
    ("Weapon_Shield_Mirror", "Mirror Shield"),
    ("Armor_Adult_HeavyBoot", "Iron Boots"),
    ("Armor_Adult_HoverBoot", "Hover Boots"),
    ("Armor_Adult_Goron", "Goron Tunic"),
    ("Armor_Adult_Zora", "Zora Tunic"),
    ("Item_Token_Gold", "Gold Skulltula Token"),
    ("Item_Dungeon_BossKey", "Boss Key"),
    ("Item_Token_Gerudo", "Gerudo Card"),
];

/// The icons a seed checksum (a hex text, such as a seed file's sha256 checksum) shows, in order:
/// five different icons, so the row reads out plainly (32 x 31 x 30 x 29 x 28 rows).
pub fn pick(checksum: &str) -> Vec<usize> {
    let lo: u32 = crate::murmur::murmur3_32(checksum.as_bytes(), 0x4841_5348);
    let hi: u32 = crate::murmur::murmur3_32(checksum.as_bytes(), 0x4943_4F4E);
    let mut v: u64 = (u64::from(hi) << 32) | u64::from(lo);
    let mut left: Vec<usize> = (0..ICONS.len()).collect();
    (0..SLOTS)
        .map(|_| {
            let n: u64 = left.len() as u64;
            let i: usize = left.remove((v % n) as usize);
            v /= n;
            i
        })
        .collect()
}

/// The icon names a seed checksum shows, in order.
pub fn names(checksum: &str) -> Vec<&'static str> {
    pick(checksum).into_iter().map(|i| ICONS[i].1).collect()
}

/// The game icon ids a seed checksum shows, in order.
pub fn ids(checksum: &str) -> Vec<&'static str> {
    pick(checksum).into_iter().map(|i| ICONS[i].0).collect()
}

/// An ASTC void-extent block: every texel of it is `c`.
pub fn flat_block(c: [u8; 4]) -> [u8; 16] {
    let mut b: [u8; 16] = [0xFC, 0xFD, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0, 0, 0, 0, 0, 0, 0, 0];
    for (i, v) in c.iter().enumerate() {
        let w: u16 = u16::from(*v) * 257;
        b[8 + 2 * i..10 + 2 * i].copy_from_slice(&w.to_le_bytes());
    }
    b
}

/// The left texel column of the icon in slot `k`.
pub fn left_of(k: usize) -> usize {
    let width: usize = SLOTS * SIDE + (SLOTS - 1) * GAP;
    CENTER_X - width / 2 + k * (SIDE + GAP)
}

/// The colour of each block of `im` cut into `n` x `n` blocks: the alpha-weighted mean of the
/// pixels each block covers (a pixel on a border counts in part), mostly-clear blocks fully clear.
pub fn blocks(im: &Img, n: usize) -> Vec<[u8; 4]> {
    let mut out: Vec<[u8; 4]> = Vec::with_capacity(n * n);
    let (fw, fh): (f64, f64) = (im.w as f64 / n as f64, im.h as f64 / n as f64);
    for by in 0..n {
        for bx in 0..n {
            let (x0, x1, y0, y1): (f64, f64, f64, f64) = (bx as f64 * fw, (bx + 1) as f64 * fw, by as f64 * fh, (by + 1) as f64 * fh);
            let mut acc: [f64; 4] = [0.0; 4];
            let mut area: f64 = 0.0;
            for y in (y0.floor() as usize)..(y1.ceil() as usize).min(im.h) {
                let wy: f64 = (y1.min(y as f64 + 1.0) - y0.max(y as f64)).max(0.0);
                for x in (x0.floor() as usize)..(x1.ceil() as usize).min(im.w) {
                    let w: f64 = wy * (x1.min(x as f64 + 1.0) - x0.max(x as f64)).max(0.0);
                    let c: [u8; 4] = im.at(x, y);
                    let a: f64 = f64::from(c[3]) * w;
                    for i in 0..3 {
                        acc[i] += f64::from(c[i]) * a;
                    }
                    acc[3] += a;
                    area += w;
                }
            }
            let alpha: f64 = if area > 0.0 { acc[3] / area } else { 0.0 };
            if alpha < 12.0 {
                out.push([0, 0, 0, 0]);
                continue;
            }
            let ch = |i: usize| (acc[i] / acc[3]).round().clamp(0.0, 255.0) as u8;
            out.push([ch(0), ch(1), ch(2), alpha.round().clamp(0.0, 255.0) as u8]);
        }
    }
    out
}

/// The game's icon `id`, decoded.
pub fn read_icon(rf: &mut Romfs, id: &str) -> Res<Img> {
    let f: String = format!("{ICON_DIR}/{id}.bntx.zs");
    let raw: Vec<u8> = rf.read_zs(&f).map_err(|e| E::Msg(format!("seed hash: the game file {f} could not be read ({e})")))?;
    let ts: Vec<Tex> = bntx::parse(&raw)?;
    let t: &Tex = bntx::find(&ts, id).ok_or_else(|| E::Msg(format!("seed hash: the game file {f} has no texture {id}, the game files differ")))?;
    let (im, failed): (Img, usize) = bntx::decode(&raw, t)?;
    if failed > 0 {
        return Err(E::Msg(format!("seed hash: {failed} blocks of the icon {id} could not be read")));
    }
    Ok(im)
}

/// Draws the icons into the texture `t` of the texture file `raw`, left to right.
pub fn draw(raw: &mut [u8], t: &Tex, icons: &[usize], rf: &mut Romfs) -> Res<()> {
    if !bntx::supported(t.fmt) {
        return Err(E::Msg(format!("seed hash: the title logo {} is not ASTC 4x4, the game files differ", t.name)));
    }
    if t.w != LOGO_W || t.h != LOGO_H {
        return Err(E::Msg(format!("seed hash: the title logo is {}x{}, not {LOGO_W}x{LOGO_H}, the game files differ", t.w, t.h)));
    }
    if icons.len() > SLOTS || icons.iter().any(|i| *i >= ICONS.len()) {
        return Err(E::Format("seed hash: an icon is out of range"));
    }
    let wb: usize = t.w.div_ceil(BLOCK_PX);
    for (k, i) in icons.iter().enumerate() {
        let cs: Vec<[u8; 4]> = blocks(&read_icon(rf, ICONS[*i].0)?, SIDE_BLOCKS);
        let x0: usize = left_of(k) / BLOCK_PX;
        let y0: usize = TOP / BLOCK_PX;
        for (n, c) in cs.iter().enumerate() {
            let a: usize = t.off + swizzle::addr(x0 + n % SIDE_BLOCKS, y0 + n / SIDE_BLOCKS, wb, t.gobs);
            let end: usize = a.checked_add(swizzle::BLOCK).filter(|e| *e <= raw.len() && *e <= t.off + t.size).ok_or(E::Format("seed hash: an icon leaves the logo texture"))?;
            raw[a..end].copy_from_slice(&flat_block(*c));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icons_and_names_are_all_different() {
        for (i, a) in ICONS.iter().enumerate() {
            for b in &ICONS[i + 1..] {
                assert_ne!(a.0, b.0);
                assert_ne!(a.1, b.1);
            }
        }
    }

    #[test]
    fn the_pick_is_stable_and_spreads() {
        let a: Vec<usize> = pick("0123abcd");
        assert_eq!(a, pick("0123abcd"));
        assert_eq!(a.len(), SLOTS);
        assert_eq!(names("0123abcd").len(), SLOTS);
        for n in 0..500 {
            let p: Vec<usize> = pick(&format!("{n:064x}"));
            assert!((1..SLOTS).all(|k| !p[..k].contains(&p[k])), "{p:?} repeats an icon");
        }
        let mut seen: std::collections::BTreeSet<Vec<usize>> = std::collections::BTreeSet::new();
        let mut used: [usize; ICONS.len()] = [0; ICONS.len()];
        for n in 0..2000 {
            let p: Vec<usize> = pick(&format!("{n:064x}"));
            for i in &p {
                used[*i] += 1;
            }
            seen.insert(p);
        }
        assert!(seen.len() > 1990, "{} distinct rows of 2000", seen.len());
        assert!(used.iter().all(|u| *u > 200), "{used:?}");
    }

    #[test]
    fn a_flat_block_decodes_to_its_colour() {
        let mut px: [[u8; 4]; 16] = [[0; 4]; 16];
        for c in [[24u8, 18, 14, 255], [250, 210, 60, 255], [0, 0, 0, 0], [255, 255, 255, 255], [10, 20, 30, 128]] {
            assert!(crate::tex::astc::decode_block(&flat_block(c), 4, 4, true, &mut px));
            assert!(px.iter().all(|p| *p == c), "{c:?} {px:?}");
        }
    }

    #[test]
    fn blocks_average_by_area_and_alpha() {
        let mut im: Img = Img::new(8, 8);
        for y in 0..8 {
            for x in 0..4 {
                im.put(x, y, [200, 100, 0, 255]);
            }
        }
        let b: Vec<[u8; 4]> = blocks(&im, 2);
        assert_eq!(b, vec![[200, 100, 0, 255], [0, 0, 0, 0], [200, 100, 0, 255], [0, 0, 0, 0]]);
        let one: Vec<[u8; 4]> = blocks(&im, 1);
        assert_eq!(one, vec![[200, 100, 0, 128]]);
        let odd: Vec<[u8; 4]> = blocks(&im, 3);
        assert_eq!(odd.len(), 9);
        assert_eq!(odd[0], [200, 100, 0, 255]);
        assert_eq!(odd[2], [0, 0, 0, 0]);
    }

    #[test]
    fn the_row_fits_the_logo_on_whole_blocks() {
        let right: usize = left_of(SLOTS - 1) + SIDE;
        assert!(right <= LOGO_W && TOP + SIDE <= LOGO_H);
        for k in 0..SLOTS {
            assert_eq!(left_of(k) % BLOCK_PX, 0, "slot {k}");
        }
        assert_eq!(TOP % BLOCK_PX, 0);
    }

    #[test]
    fn every_icon_is_in_the_game_and_the_row_lands_on_clear_logo() {
        let root: std::path::PathBuf = ootrr_game::test_romfs::path();
        if !root.join("Pack").join("ZsDic.pack.zs").is_file() {
            eprintln!("skipped, no RomFS dump at {}", root.display());
            return;
        }
        let mut rf: Romfs = Romfs::open(&root).unwrap();
        for (id, _) in ICONS {
            let im: Img = read_icon(&mut rf, id).unwrap();
            assert!(blocks(&im, SIDE_BLOCKS).iter().filter(|c| c[3] > 0).count() > 100, "{id} is nearly empty");
        }
        let p = crate::title_logo::patch().unwrap();
        let mut raw: Vec<u8> = rf.read_zs(&p.files[0]).unwrap();
        crate::title_logo::apply_raw(&mut raw, &p).unwrap();
        let t: Tex = bntx::find(&bntx::parse(&raw).unwrap(), &p.texture).cloned().unwrap();
        let (before, _) = bntx::decode(&raw, &t).unwrap();
        let (x0, x1) = (left_of(0) - GAP / 2, left_of(SLOTS - 1) + SIDE + GAP / 2);
        for y in TOP - 8..TOP + SIDE + 8 {
            for x in x0..x1 {
                assert!(before.at(x, y)[3] <= 8, "the logo is not clear at {x},{y}");
            }
        }
        let all: Vec<usize> = (0..SLOTS).collect();
        draw(&mut raw, &t, &all, &mut rf).unwrap();
        let (after, failed) = bntx::decode(&raw, &t).unwrap();
        assert_eq!(failed, 0);
        for y in 0..t.h {
            for x in 0..t.w {
                let inside: bool = y >= TOP && y < TOP + SIDE && x >= left_of(0) && x < left_of(SLOTS - 1) + SIDE;
                if !inside {
                    assert_eq!(after.at(x, y), before.at(x, y), "{x},{y} changed outside the row");
                }
            }
        }
        assert!((TOP..TOP + SIDE).any(|y| after.at(left_of(0) + SIDE / 2, y)[3] > 200));
    }
}
