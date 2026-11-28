use std::collections::BTreeSet as BS;
use std::path::Path as P;

use crate::ainb::start;
use crate::ap::item::logo;
use crate::bomb_lift;
use crate::fast;
use crate::flags_build::LIST_PATH;
use crate::flow_locations::path_of;
use crate::hints;
use crate::intro_skip;
use crate::pickup_pack::model;
use crate::progressive::{ANALYZED_TABLE, BIG_FLOW, SETTING_TABLE, SMALL_FLOW};
use crate::rsizetable::TABLE_PATH;
use crate::slice::data::Data;
use crate::song_models;
use crate::songs;
use crate::special::SPOTS;

pub const LANGUAGE: &str = "Mals/USen.Product.110.sarc.zs";

pub const POSED: [&str; 5] = ["Item_Dungeon_Map", "Item_Medal_Light", "Item_Ocarina_Fairy", "Item_Ocarina_Time", "Item_Stone_Forest"];

fn poses() -> impl Iterator<Item = String> {
    POSED.iter().map(|m| model::pose_file(&model::pose_of(m)))
}

fn ap_look() -> impl Iterator<Item = String> {
    std::iter::once(logo::new_model()).chain(logo::SUFFIXES.into_iter().map(logo::tex_file)).filter_map(|f| logo::source_of(&f))
}

pub fn list(d: &Data) -> Vec<String> {
    let mut out: BS<String> = BS::new();
    out.extend(d.scan.files.iter().cloned());
    out.extend(d.pots.files.iter().cloned());
    out.extend(d.pots.rock_files.iter().cloned());
    for l in &d.flow.locs {
        for e in &l.edits {
            if crate::cow_clone::of_file(&e.file).is_none() {
                out.insert(path_of(&e.file));
            }
            out.extend(e.also.iter().map(|a| path_of(a)));
        }
    }
    for c in crate::cow_clone::all() {
        out.extend(c.spots.iter().map(|s| s.file.clone()));
    }
    out.extend(crate::cow_clone::stage::sources());
    for s in [BIG_FLOW, SMALL_FLOW, SETTING_TABLE, ANALYZED_TABLE, TABLE_PATH, LIST_PATH, start::MODULE, songs::FLOW, hints::FLOW, LANGUAGE] {
        out.insert(s.to_string());
    }
    out.extend(fast::FILES.iter().map(|s| s.to_string()));
    out.insert(intro_skip::FILE.to_string());
    out.extend(crate::qol::files());
    out.extend(SPOTS.iter().map(|s| s.flow.to_string()));
    out.extend(song_models::SONGS.iter().map(|s| song_models::pack_of(s)));
    out.extend(bomb_lift::PACKS.iter().map(|s| bomb_lift::pack_of(s)));
    out.insert(crate::dungeon_credit::POUCH_TABLE.to_string());
    out.insert("Pack/ZsDic.pack.zs".to_string());
    out.into_iter().collect()
}

/// Folders a build reads whole: it lists them, or picks files in them by names that come
/// from the seed or the data files (actor packs, event flows, map and language files).
pub const TREES: [&str; 16] = [
    "AoC1",
    "Banc",
    "Component",
    "Event/EventFlow",
    "GameData",
    "Logic",
    "Mals",
    "Pack",
    "Phive/NavMesh",
    "Phive/StitchedNavMesh",
    "RSDB",
    "Sequence",
    "SLink2",
    "Sound/Bgm",
    "System/CombinationDataTableData",
    "System/Resource",
];

/// Music and sound effect shuffle list and size every sound bank and stream.
pub const SOUND: &str = "Sound/Resource";

/// The part of the game a build reads: the files in `files` and everything under the
/// folders in `trees`. Used to unpack only that part of a .nup or .nx (about 300 MB, or
/// about 2.3 GB with music or sound effect shuffle) instead of the whole 25 GB game.
pub struct Unpack {
    pub files: BS<String>,
    pub trees: BS<String>,
}

impl Unpack {
    pub fn wants(&self, r: &str) -> bool {
        self.files.contains(r) || self.trees.iter().any(|t| r.len() > t.len() && r.starts_with(t.as_str()) && r.as_bytes()[t.len()] == b'/')
    }
}

pub fn unpack(d: &Data, sound: bool) -> Unpack {
    let mut files: BS<String> = list(d).into_iter().collect();
    files.extend(crate::qol::low_health::FILES.iter().map(|s| s.to_string()));
    files.extend(crate::colors::files());
    files.insert(crate::silence::GROUP_FILE.to_string());
    files.extend(crate::gerudo_card::MEMBERS.iter().map(|n| crate::gerudo_card::schedule_path(n)));
    files.extend(crate::open_world::fortress::CARPENTERS.iter().map(|n| crate::open_world::fortress::own_schedule(n)));
    files.insert(crate::gerudo_card::schedule_path(crate::open_world::rolling_goron::OWN));
    files.extend(crate::open_world::kokiri::NPCS.iter().chain([&crate::open_world::kokiri::MIDO]).map(|n| crate::open_world::kokiri::schedule_path(n)));
    // Read only with some settings (no heart drops, harp colours, enemy effects, adult trade
    // shuffle), so missing them from a .nup or .nx unpack only shows up in those seeds.
    files.insert(crate::difficulty::lottery::FILE.to_string());
    files.insert(crate::song_models::HARP_FILE.to_string());
    files.insert(crate::song_models::tex_file(crate::song_models::ALBEDO));
    files.insert(crate::difficulty::elink::FILE.to_string());
    files.insert(crate::open_world::trade_shuffle::POUCH_UI.to_string());
    if let Ok(p) = crate::title_logo::patch() {
        files.extend(crate::title_logo::touched(&p));
    }
    files.extend(poses());
    files.extend(ap_look());
    let mut trees: BS<String> = TREES.iter().map(|s| s.to_string()).collect();
    if sound {
        trees.insert(SOUND.to_string());
    }
    Unpack { files, trees }
}

pub fn missing(root: &P, d: &Data) -> Vec<String> {
    list(d)
        .into_iter()
        .filter(|r| !r.split('/').fold(root.to_path_buf(), |a, s| a.join(s)).is_file())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_list_names_the_files_a_default_build_reads() {
        let d: Data = Data::load().unwrap();
        let l: Vec<String> = list(&d);
        assert!(l.iter().any(|x| x == TABLE_PATH));
        assert!(l.iter().any(|x| x.starts_with("Banc/")));
        assert!(l.iter().any(|x| x.contains("Enemy_Scrub_Business_Common")));
        assert!(l.windows(2).all(|w| w[0] < w[1]));
    }

    fn files(root: &P) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let mut stack: Vec<std::path::PathBuf> = vec![root.to_path_buf()];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).unwrap() {
                let p: std::path::PathBuf = e.unwrap().path();
                if p.is_dir() {
                    stack.push(p);
                } else {
                    out.push(p.strip_prefix(root).unwrap().to_string_lossy().replace(std::path::MAIN_SEPARATOR, "/"));
                }
            }
        }
        out.sort();
        out
    }

    fn at(root: &P, r: &str) -> std::path::PathBuf {
        r.split('/').fold(root.to_path_buf(), |a, s| a.join(s))
    }

    fn cut_to(root: &P, u: &Unpack, cut: &P) {
        for r in files(root).iter().filter(|r| u.wants(r)) {
            let (a, b): (std::path::PathBuf, std::path::PathBuf) = (at(root, r), at(cut, r));
            std::fs::create_dir_all(b.parent().unwrap()).unwrap();
            if std::fs::hard_link(&a, &b).is_err() {
                std::fs::copy(&a, &b).unwrap();
            }
        }
    }

    #[test]
    fn a_more_enemies_build_from_the_unpack_set_has_the_folder_build_s_maps() {
        let root: std::path::PathBuf = ootrr_game::test_romfs::path();
        if !root.join("Pack").join("ZsDic.pack.zs").is_file() {
            eprintln!("skipped: no romfs at {}, set OOTRR_ROMFS", root.display());
            return;
        }
        let d: Data = Data::load().unwrap();
        let u: Unpack = unpack(&d, false);
        let all: Vec<String> = files(&root);
        let meshes: Vec<&String> = all.iter().filter(|r| r.starts_with("Phive/NavMesh/") || r.starts_with("Phive/StitchedNavMesh/")).collect();
        assert_eq!((meshes.len(), meshes.iter().filter(|r| !u.wants(r)).count()), (846, 0));
        let tmp: std::path::PathBuf = std::env::temp_dir().join(format!("ootrr_unpack_set_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let cut: std::path::PathBuf = tmp.join("romfs");
        cut_to(&root, &u, &cut);
        let mut s: crate::settings::Settings = crate::settings::Settings::default();
        s.set(crate::difficulty::MORE, crate::settings::Value::Int(1000)).unwrap();
        let f: crate::settings::seed::SeedFile = crate::slice::generate::generate_data(&d, &s, "unpack set", &crate::slice::generate::GenOpts::default()).unwrap().seed_file;
        let full: std::path::PathBuf = crate::slice::edit::build(&f, &root, &tmp.join("full"), "unpack set").unwrap().root;
        let some: std::path::PathBuf = crate::slice::edit::build(&f, &cut, &tmp.join("part"), "unpack set").unwrap().root;
        let maps = |r: &P| -> Vec<String> { files(r).into_iter().filter(|x| x.contains("Banc/")).collect() };
        let (a, b): (Vec<String>, Vec<String>) = (maps(&full), maps(&some));
        assert!(a.len() > 30);
        assert_eq!(a, b);
        let differ: Vec<&String> = a.iter().filter(|r| std::fs::read(at(&full, r)).unwrap() != std::fs::read(at(&some, r)).unwrap()).collect();
        assert!(differ.is_empty(), "differ from the folder build: {differ:?}");
        std::fs::rename(at(&cut, crate::difficulty::ground::NAV_DIR), tmp.join("nav")).unwrap();
        let e: String = crate::slice::edit::build(&f, &cut, &tmp.join("bare"), "unpack set").err().unwrap().to_string();
        assert!(e.contains(crate::difficulty::ground::NAV_DIR), "{e}");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn a_build_from_the_unpack_set_poses_the_pickups_as_a_folder_build_does() {
        let root: std::path::PathBuf = ootrr_game::test_romfs::path();
        if !root.join("Pack").join("ZsDic.pack.zs").is_file() {
            eprintln!("skipped: no romfs at {}, set OOTRR_ROMFS", root.display());
            return;
        }
        let d: Data = Data::load().unwrap();
        let u: Unpack = unpack(&d, false);
        let mut rf: crate::romfs::Romfs = crate::romfs::Romfs::open(&root).unwrap();
        let mut looks: Vec<crate::pickup_pack::Layers> = vec![crate::pickup_pack::Layers { model: crate::dungeon_packs::Kind::Map.look().map(str::to_string), ..crate::pickup_pack::Layers::default() }];
        for e in crate::pickup_pack::catalog::load().unwrap() {
            for (args, _) in e.builds() {
                looks.push(crate::pickup_pack::catalog::spec(&e.item, args).unwrap().1);
            }
        }
        let mut posed: BS<String> = BS::new();
        for l in &looks {
            if let Some(p) = model::layers(&mut rf, l).unwrap().pose {
                posed.insert(model::pose_file(&p));
            }
        }
        assert_eq!(posed, poses().collect());
        assert!(posed.iter().all(|f| u.wants(f)));
        let tmp: std::path::PathBuf = std::env::temp_dir().join(format!("ootrr_unpack_poses_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let cut: std::path::PathBuf = tmp.join("romfs");
        cut_to(&root, &u, &cut);
        let mut s: crate::settings::Settings = crate::settings::Settings::default();
        s.set(crate::slice::pick::OCARINAS, crate::settings::Value::Bool(true)).unwrap();
        let f: crate::settings::seed::SeedFile = crate::slice::generate::generate_data(&d, &s, "unpack poses", &crate::slice::generate::GenOpts::default()).unwrap().seed_file;
        let full: std::path::PathBuf = crate::slice::edit::build(&f, &root, &tmp.join("full"), "unpack poses").unwrap().root;
        let differ = |to: &str| -> Vec<String> {
            let some: std::path::PathBuf = crate::slice::edit::build(&f, &cut, &tmp.join(to), "unpack poses").unwrap().root;
            assert_eq!(files(&full), files(&some));
            files(&full).into_iter().filter(|r| std::fs::read(at(&full, r)).unwrap() != std::fs::read(at(&some, r)).unwrap()).collect()
        };
        assert_eq!(differ("part"), Vec::<String>::new());
        for p in &posed {
            std::fs::remove_file(at(&cut, p)).unwrap();
        }
        let without: Vec<String> = differ("bare");
        for pack in ["Obj_Ocarina_Fairy.pack.zs", "Obj_Ocarina_TimeLand.pack.zs"] {
            assert!(without.iter().any(|r| r.ends_with(pack)), "{without:?}");
        }
        println!("without the pose files {} files differ: {without:?}", without.len());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn the_unpack_set_holds_the_kokiri_schedules_and_the_ap_item_look_sources() {
        let u: Unpack = unpack(&Data::load().unwrap(), false);
        for f in crate::open_world::kokiri::NPCS.into_iter().chain([crate::open_world::kokiri::MIDO]).map(crate::open_world::kokiri::schedule_path) {
            assert!(u.wants(&f), "{f}");
        }
        let look: Vec<String> = ap_look().collect();
        assert_eq!(look.len(), 1 + logo::SUFFIXES.len());
        for f in look {
            assert!(u.wants(&f), "{f}");
        }
    }

    #[test]
    fn the_unpack_set_holds_the_build_list_the_colour_files_and_its_trees() {
        let d: Data = Data::load().unwrap();
        let u: Unpack = unpack(&d, false);
        assert!(list(&d).iter().all(|f| u.wants(f)));
        assert!(crate::colors::files().iter().all(|f| u.wants(f)));
        assert!(crate::qol::low_health::FILES.iter().all(|f| u.wants(f)));
        assert!(u.wants("Pack/Actor/Anything.pack.zs") && u.wants("Banc/X/y.zs"));
        assert!(!u.wants("Packed/x") && !u.wants("Pack") && !u.wants("Sound/Resource/Stream/SE_A.bwav"));
        assert!(unpack(&d, true).wants("Sound/Resource/Stream/SE_A.bwav"));
    }

    /// Files a .nup build failed on because only the plain folder build read them: the rolling
    /// Goron's own schedule, the drop table (no heart drops), the harp model and texture, the effect
    /// links, the pouch menu order and the extra scenes enemy edits list.
    #[test]
    fn the_unpack_set_holds_the_setting_dependent_files() {
        let d: Data = Data::load().unwrap();
        let u: Unpack = unpack(&d, false);
        for f in [
            crate::gerudo_card::schedule_path(crate::open_world::rolling_goron::OWN),
            crate::difficulty::lottery::FILE.to_string(),
            crate::song_models::HARP_FILE.to_string(),
            crate::song_models::tex_file(crate::song_models::ALBEDO),
            crate::song_models::NAMES.to_string(),
            crate::difficulty::elink::FILE.to_string(),
            crate::silence::GROUP_FILE.to_string(),
            crate::open_world::trade_shuffle::POUCH_UI.to_string(),
            format!("{}/Banc/BancBasedResource/StartPos.byml.zs", crate::difficulty::extras::MORE_SCENES),
        ] {
            assert!(u.wants(&f), "{f}");
        }
    }

    #[test]
    fn an_empty_folder_misses_everything() {
        let d: Data = Data::load().unwrap();
        let dir = std::env::temp_dir().join(format!("ootrr_required_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(missing(&dir, &d).len(), list(&d).len());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
