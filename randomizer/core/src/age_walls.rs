use std::path::Path as P;

use crate::byml::{Byml, Node};
use crate::error::{Error, Res};
use crate::romfs::Romfs;
use crate::staging::Out;

pub const CRATER: &str = "Banc/DeathMountainCrater/DeathMountainCrater_Child_Dynamic.bcett.byml.zs";
pub const RIVER_CHILD: &str = "Banc/ZoraRiver/ZoraRiver_Child_Static.bcett.byml.zs";
pub const RIVER_ADULT: &str = "Banc/ZoraRiver/ZoraRiver_Adult_Static.bcett.byml.zs";
pub const ROCK: &str = "Obj_BrokenRock_A_02";
pub const WALL: &str = "FldObj_AirWall_Player";
pub const RIVER_WALL: &str = "FldObj_AirWall_Player_GenerateNavMesh";
pub const CRATER_ROCKS: [u64; 3] = [15044423826661592135, 5416806354710087510, 13991526616466298561];
pub const CRATER_WALLS: [u64; 4] = [7605608966338212464, 13689726361166707049, 12486026435797633509, 12293367376072468601];
pub const CHILD_RIVER: [u64; 7] = [8855423962460166888, 4942866467657729468, 2863011381430310501, 1671256346243721333, 12374977777450469396, 15623121253783701078, 14519305071033520687];
pub const ADULT_RIVER: [u64; 7] = [7850395794748438732, 7982339630050931435, 13781426553798151043, 4565321854739336115, 14077907143773739344, 9371429508151251838, 11266001672277786960];
pub const SPOTS: [(&str, &str, &[u64]); 4] = [(CRATER, ROCK, &CRATER_ROCKS), (CRATER, WALL, &CRATER_WALLS), (RIVER_CHILD, RIVER_WALL, &CHILD_RIVER), (RIVER_ADULT, RIVER_WALL, &ADULT_RIVER)];
pub const NOTE: &str = "Child can walk from Death Mountain Crater to the Fire Temple, and either age can swim down Zora's River from the Lost Woods shortcut. The child crater path was blocked by three breakable rocks (Obj_BrokenRock_A_02) with an invisible wall (FldObj_AirWall_Player) behind them, and the river had seven invisible walls (FldObj_AirWall_Player_GenerateNavMesh) across the waterfall in both ages. Each now has a presence that reads Get_StoneOfFire_IsAfter_Ready_Exp (always true) negated, so none of them appear.";

fn never() -> Node {
    Node::Map(vec![("FlagName".to_string(), Node::Str(crate::always::NAME.to_string())), ("IsCheckLazy".to_string(), Node::Bool(false)), ("IsNegation".to_string(), Node::Bool(true))])
}

pub fn patch(y: &mut Byml, gyaml: &str, hashes: &[u64]) -> Res<()> {
    let actors: &mut Vec<Node> = y.root.get_mut("Actors").and_then(|n| n.as_arr_mut()).ok_or(Error::Format("age walls: the map has no actors"))?;
    for h in hashes {
        let a: &mut Node = actors
            .iter_mut()
            .find(|a| matches!(a.get("Hash"), Some(Node::U64(x)) if x == h))
            .ok_or(Error::Format("age walls: a wall is missing from its map"))?;
        if a.get("Gyaml").and_then(|g| g.as_str()) != Some(gyaml) {
            return Err(Error::Format("age walls: a wall is not the expected actor"));
        }
        if a.get("Presence").is_some() {
            return Err(Error::Format("age walls: a wall already has a presence"));
        }
        a.set("Presence", never());
    }
    Ok(())
}

pub fn bytes(raw: &[u8], gyaml: &str, hashes: &[u64]) -> Res<Vec<u8>> {
    let mut y: Byml = Byml::parse(raw)?;
    patch(&mut y, gyaml, hashes)?;
    let out: Vec<u8> = y.write()?;
    if Byml::parse(&out)?.root != y.root {
        return Err(Error::Format("age walls: the patched map does not read back"));
    }
    Ok(out)
}

pub fn stage(rf: &mut Romfs, more: &mut Vec<Out>, work: &P) -> Res<()> {
    for (file, gyaml, hashes) in SPOTS {
        crate::staging::edit_staged(rf, more, work, file, true, |r| bytes(r, gyaml, hashes))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dump() -> Option<Romfs> {
        let p: std::path::PathBuf = ootrr_game::test_romfs::path();
        if !p.join("Pack").join("ZsDic.pack.zs").is_file() {
            eprintln!("skipped, no RomFS dump at {} (set OOTRR_ROMFS)", p.display());
            return None;
        }
        Some(Romfs::open(&p).unwrap())
    }

    fn hidden(out: &[u8], h: u64) -> bool {
        let y: Byml = Byml::parse(out).unwrap();
        let acts: &[Node] = y.root.get("Actors").and_then(|n| n.as_arr()).unwrap();
        let a: &Node = acts.iter().find(|a| a.get("Hash") == Some(&Node::U64(h))).unwrap();
        a.get("Presence") == Some(&never())
    }

    #[test]
    fn the_walls_never_appear() {
        let Some(mut rf) = dump() else { return };
        for (file, gyaml, hashes) in SPOTS {
            let raw: Vec<u8> = rf.read_zs(file).unwrap();
            let out: Vec<u8> = bytes(&raw, gyaml, hashes).unwrap();
            for h in hashes {
                assert!(!hidden(&raw, *h));
                assert!(hidden(&out, *h));
            }
            assert!(bytes(&out, gyaml, hashes).is_err());
        }
    }

    #[test]
    fn only_the_listed_walls_change() {
        let Some(mut rf) = dump() else { return };
        for (file, gyaml, hashes) in SPOTS {
            let raw: Vec<u8> = rf.read_zs(file).unwrap();
            let out: Vec<u8> = bytes(&raw, gyaml, hashes).unwrap();
            let a: Byml = Byml::parse(&raw).unwrap();
            let b: Byml = Byml::parse(&out).unwrap();
            let before: &[Node] = a.root.get("Actors").and_then(|n| n.as_arr()).unwrap();
            let after: &[Node] = b.root.get("Actors").and_then(|n| n.as_arr()).unwrap();
            assert_eq!(before.len(), after.len());
            let mut changed: usize = 0;
            for (x, y) in before.iter().zip(after.iter()) {
                if x != y {
                    changed += 1;
                    assert!(matches!(x.get("Hash"), Some(Node::U64(h)) if hashes.contains(h)));
                }
            }
            assert_eq!(changed, hashes.len());
        }
    }

    #[test]
    fn staged_edits_land_in_one_file_per_map() {
        let Some(mut rf) = dump() else { return };
        let work: std::path::PathBuf = std::env::temp_dir().join("ootrr-age-walls-test");
        let mut more: Vec<Out> = Vec::new();
        stage(&mut rf, &mut more, &work).unwrap();
        for file in [CRATER, RIVER_CHILD, RIVER_ADULT] {
            assert_eq!(more.iter().filter(|o| o.path == file).count(), 1);
        }
        let z: Vec<u8> = more.iter().find(|o| o.path == CRATER).unwrap().z.clone();
        let raw: Vec<u8> = rf.zs.decompress(&z).unwrap();
        for h in CRATER_ROCKS.iter().chain(CRATER_WALLS.iter()) {
            assert!(hidden(&raw, *h));
        }
    }

    #[test]
    fn a_default_build_stages_the_walls() {
        let found: bool = crate::slice::stages::STAGES.iter().any(|(n, _)| *n == "crater and river walls");
        assert!(found);
    }
}
