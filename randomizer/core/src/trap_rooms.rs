use crate::start_defaults::Edit;

pub const FOREST_BOW_ROOM: &str = "Gimmick_DefeatEnemy_Open_ForestTemple_03";
pub const SPIRIT_STATUE_DOOR: &str = "SpiritTemple_Room07_Unlocked";
pub const FLAGS: [&str; 2] = [FOREST_BOW_ROOM, SPIRIT_STATUE_DOOR];

pub fn edits() -> Vec<Edit> {
    FLAGS.iter().map(|f| Edit::Bool(f.to_string(), true)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::byml::{Byml, Node};
    use crate::romfs::Romfs;
    use std::path::PathBuf as PB;

    fn real() -> Option<PB> {
        let p: PB = ootrr_game::test_romfs::path();
        if p.join("Pack").join("ZsDic.pack.zs").is_file() {
            Some(p)
        } else {
            eprintln!("skipped, no RomFS dump at {} (set OOTRR_ROMFS)", p.display());
            None
        }
    }

    fn default_of(y: &Byml, name: &str) -> Option<Node> {
        let h: u32 = crate::start_defaults::key(name);
        y.root
            .get("Data")
            .and_then(|d| d.get("Bool"))
            .and_then(|v| v.as_arr())?
            .iter()
            .find(|r| match r.get("Hash") {
                Some(Node::U32(x)) => *x == h,
                Some(Node::I32(x)) => *x as u32 == h,
                _ => false,
            })
            .and_then(|r| r.get("DefaultValue").cloned())
    }

    #[test]
    fn every_edit_starts_a_room_flag_true() {
        let e: Vec<Edit> = edits();
        assert_eq!(e.len(), FLAGS.len());
        for f in FLAGS {
            assert!(e.contains(&Edit::Bool(f.to_string(), true)), "{f}");
        }
    }

    #[test]
    fn both_room_flags_go_from_false_to_true_on_a_new_file() {
        let Some(root) = real() else {
            return;
        };
        let mut rf: Romfs = Romfs::open(&root).unwrap();
        let raw: Vec<u8> = rf.read_zs(crate::chest_slot::LIST).unwrap();
        let mut y: Byml = Byml::parse(&raw).unwrap();
        for f in FLAGS {
            assert_eq!(default_of(&y, f), Some(Node::Bool(false)), "{f}");
        }
        assert_eq!(crate::start_defaults::apply(&mut y, &edits()).unwrap(), FLAGS.len());
        for f in FLAGS {
            assert_eq!(default_of(&y, f), Some(Node::Bool(true)), "{f}");
        }
        assert_eq!(crate::start_defaults::apply(&mut y, &edits()).unwrap(), 0);
        let again: Byml = Byml::parse(&y.write().unwrap()).unwrap();
        assert_eq!(again.root, y.root);
    }
}
