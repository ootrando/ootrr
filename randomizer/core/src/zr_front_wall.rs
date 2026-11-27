//! The invisible wall at the front of Zora's River.
//!
//! Seven FldObj_AirWall_Player_GenerateNavMesh pieces run along the two breakable boulders
//! (Obj_BrokenRock_A_01) between the Hyrule Field entrance and the river, in the child and the
//! adult static maps alike. They have no presence flag, so they stand in every quest state, and
//! they stop the player going around the boulders: a child who came in through the Lost Woods
//! dive could not leave toward Hyrule Field, and an adult could not come in, without a blast or
//! a smash. The logic already reads the front that way (ZR Front -> Zora River is open to an
//! adult, Zora River -> ZR Front is always open), so each piece now gets a presence that reads
//! the always-true expression negated and never appears, whatever the settings. The boulders
//! stay.

use std::path::Path as P;

use crate::byml::{Byml, Node};
use crate::error::{Error, Res};
use crate::romfs::Romfs;
use crate::staging::Out;

pub const CHILD: &str = "Banc/ZoraRiver/ZoraRiver_Child_Static.bcett.byml.zs";
pub const ADULT: &str = "Banc/ZoraRiver/ZoraRiver_Adult_Static.bcett.byml.zs";
pub const WALL_GYAML: &str = "FldObj_AirWall_Player_GenerateNavMesh";
pub const CHILD_WALLS: [u64; 7] = [
    4942866467657729468,
    2863011381430310501,
    8855423962460166888,
    12374977777450469396,
    15623121253783701078,
    14519305071033520687,
    1671256346243721333,
];
pub const ADULT_WALLS: [u64; 7] = [
    7982339630050931435,
    13781426553798151043,
    7850395794748438732,
    14077907143773739344,
    9371429508151251838,
    11266001672277786960,
    4565321854739336115,
];
pub const NOTE: &str = "The invisible wall along the boulders at the front of Zora's River (FldObj_AirWall_Player_GenerateNavMesh, seven pieces in the child and the adult maps) never appears. A child who dove in from the Lost Woods can leave toward Hyrule Field, and an adult can come in from Hyrule Field, without a blast or a smash. A child coming in from Hyrule Field still needs one, as the logic says.";

fn presence() -> Node {
    Node::Map(vec![
        ("FlagName".to_string(), Node::Str(crate::always::NAME.to_string())),
        ("IsCheckLazy".to_string(), Node::Bool(false)),
        ("IsNegation".to_string(), Node::Bool(true)),
    ])
}

pub fn patch(y: &mut Byml, walls: &[u64]) -> Res<()> {
    let actors: &mut Vec<Node> = y.root.get_mut("Actors").and_then(|n| n.as_arr_mut()).ok_or(Error::Format("zora river wall: the river map has no actors"))?;
    for h in walls {
        let a: &mut Node = actors
            .iter_mut()
            .find(|a| matches!(a.get("Hash"), Some(Node::U64(x)) if x == h))
            .ok_or(Error::Format("zora river wall: the river map lacks a piece of the front wall"))?;
        if a.get("Gyaml").and_then(|g| g.as_str()) != Some(WALL_GYAML) {
            return Err(Error::Format("zora river wall: a front wall piece is not a player air wall"));
        }
        if a.get("Presence").is_some() {
            return Err(Error::Format("zora river wall: a front wall piece already has a presence"));
        }
        a.set("Presence", presence());
    }
    Ok(())
}

pub fn bytes(raw: &[u8], walls: &[u64]) -> Res<Vec<u8>> {
    let mut y: Byml = Byml::parse(raw)?;
    patch(&mut y, walls)?;
    let out: Vec<u8> = y.write()?;
    if Byml::parse(&out)?.root != y.root {
        return Err(Error::Format("zora river wall: the patched map does not read back"));
    }
    Ok(out)
}

pub fn stage(rf: &mut Romfs, more: &mut Vec<Out>, work: &P) -> Res<()> {
    crate::staging::edit_staged(rf, more, work, CHILD, true, |r| bytes(r, &CHILD_WALLS))?;
    crate::staging::edit_staged(rf, more, work, ADULT, true, |r| bytes(r, &ADULT_WALLS))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rom() -> Option<Romfs> {
        let p: std::path::PathBuf = ootrr_game::test_romfs::path();
        if !p.join("Pack").join("ZsDic.pack.zs").is_file() {
            eprintln!("skipped, no RomFS dump at {} (set OOTRR_ROMFS)", p.display());
            return None;
        }
        Some(Romfs::open(&p).unwrap())
    }

    fn actors(b: &[u8]) -> Vec<Node> {
        Byml::parse(b).unwrap().root.get("Actors").and_then(|n| n.as_arr()).unwrap().to_vec()
    }

    fn x_z(a: &Node) -> (f32, f32) {
        let t: &[Node] = a.get("Translate").and_then(|n| n.as_arr()).unwrap();
        match (&t[0], &t[2]) {
            (Node::F32(x), Node::F32(z)) => (*x, *z),
            _ => panic!("translate"),
        }
    }

    #[test]
    fn every_front_wall_piece_never_appears_and_nothing_else_changes() {
        let Some(mut rf) = rom() else { return };
        for (file, walls) in [(CHILD, &CHILD_WALLS), (ADULT, &ADULT_WALLS)] {
            let raw: Vec<u8> = rf.read_zs(file).unwrap();
            let out: Vec<u8> = bytes(&raw, walls).unwrap();
            assert!(bytes(&out, walls).is_err(), "{file}: a second patch is refused");
            let before: Vec<Node> = actors(&raw);
            let after: Vec<Node> = actors(&out);
            assert_eq!(before.len(), after.len());
            let mut changed: usize = 0;
            for (x, y) in before.iter().zip(after.iter()) {
                if x == y {
                    continue;
                }
                changed += 1;
                let Some(Node::U64(h)) = y.get("Hash") else { panic!() };
                assert!(walls.contains(h));
                let p: &Node = y.get("Presence").unwrap();
                assert_eq!(p.get("FlagName").and_then(|n| n.as_str()), Some(crate::always::NAME));
                assert_eq!(p.get("IsNegation"), Some(&Node::Bool(true)));
                let mut plain: Node = y.clone();
                let Node::Map(m) = &mut plain else { panic!() };
                m.retain(|e| e.0 != "Presence");
                assert_eq!(&plain, x, "{file}: only the presence is added");
            }
            assert_eq!(changed, walls.len());
        }
    }

    #[test]
    fn the_pieces_are_the_whole_wall_along_the_boulders_in_both_ages() {
        let Some(mut rf) = rom() else { return };
        for (file, walls) in [(CHILD, &CHILD_WALLS), (ADULT, &ADULT_WALLS)] {
            let acts: Vec<Node> = actors(&rf.read_zs(file).unwrap());
            let front = |a: &&Node| -> bool {
                let (x, z) = x_z(a);
                (-90.0..=-55.0).contains(&x) && (0.0..=12.0).contains(&z)
            };
            let mut found: Vec<u64> = acts
                .iter()
                .filter(|a| a.get("Gyaml").and_then(|g| g.as_str()) == Some(WALL_GYAML))
                .filter(front)
                .map(|a| match a.get("Hash") {
                    Some(Node::U64(h)) => *h,
                    _ => panic!(),
                })
                .collect();
            found.sort();
            let mut want: Vec<u64> = walls.to_vec();
            want.sort();
            assert_eq!(found, want, "{file}");
            let rocks: usize = acts.iter().filter(|a| a.get("Gyaml").and_then(|g| g.as_str()) == Some("Obj_BrokenRock_A_01")).filter(front).count();
            assert_eq!(rocks, 2, "{file}: the boulders stay");
        }
    }
}
