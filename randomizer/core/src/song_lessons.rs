use crate::error::Res;
use crate::flow_locations::{dict_id, edit_bytes, path_of, Req, StagedFile};
use crate::romfs::Romfs;
use crate::staging::{rewrite, Out};
use crate::zs;

pub const WINDMILL: &str = "Npc_Hylia_009_Adult.bfevfl";
pub const STORM_OWNED: &str = "IsGet.Item_Melody_StormSong";
pub const STORM_TAUGHT: &str = "HavePlayedEvent.Dm_MelodyGet_Storm";
const PARAM: &str = "GameDataName";

fn req(index: usize, event: &str, old: &str, new: &str) -> Req {
    Req { index, event: event.to_string(), param: PARAM.to_string(), old: old.to_string(), new: new.to_string() }
}

pub fn edits() -> Vec<(&'static str, Vec<Req>)> {
    vec![(WINDMILL, vec![req(4, "Event9", STORM_OWNED, STORM_TAUGHT), req(17, "Event31", STORM_OWNED, STORM_TAUGHT), req(51, "Event17", STORM_OWNED, STORM_TAUGHT), req(54, "Event69", STORM_OWNED, STORM_TAUGHT)])]
}

pub fn stage_flow(rf: &mut Romfs, files: &mut Vec<StagedFile>) -> Res<()> {
    for (file, rs) in edits() {
        let path: String = path_of(file);
        match files.iter().position(|x| x.path == path) {
            Some(i) => {
                let id: Option<u32> = dict_id(&files[i].z);
                let raw: Vec<u8> = rf.zs.decompress(&files[i].z)?;
                let out: Vec<u8> = edit_bytes(&raw, &rs)?;
                files[i].new = out.len();
                files[i].z = zs::compress_raw(&out, id)?;
            }
            None => {
                let o: Out = rewrite(rf, &path, |raw: &[u8]| edit_bytes(raw, &rs))?;
                files.push(StagedFile { path: o.path, z: o.z, old: o.old, new: o.new });
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bfevfl::{Bf, Val};
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

    fn name_at(b: &Bf, i: usize) -> Option<String> {
        match b.flows[0].events[i].kind.params()?.get(PARAM)? {
            Val::Str(s) | Val::Actor(s, _) => Some(s.clone()),
            _ => None,
        }
    }

    #[test]
    fn the_windmill_lesson_stops_reading_song_ownership() {
        let Some(root) = real() else {
            return;
        };
        let mut rf: Romfs = Romfs::open(&root).unwrap();
        let mut files: Vec<StagedFile> = Vec::new();
        stage_flow(&mut rf, &mut files).unwrap();
        assert_eq!(files.len(), 1);
        for (file, rs) in edits() {
            let f: &StagedFile = files.iter().find(|x| x.path == path_of(file)).unwrap();
            let b: Bf = Bf::parse(&rf.zs.decompress(&f.z).unwrap()).unwrap();
            for r in &rs {
                assert_eq!(name_at(&b, r.index).as_deref(), Some(r.new.as_str()), "{file} {}", r.event);
            }
            let left: Vec<usize> = (0..b.flows[0].events.len()).filter(|i| name_at(&b, *i).as_deref() == Some(STORM_OWNED)).collect();
            assert!(left.is_empty(), "{file} still reads {STORM_OWNED} at {left:?}");
        }
        let again: usize = files.len();
        assert!(stage_flow(&mut rf, &mut files).is_err());
        assert_eq!(files.len(), again);
    }
}
