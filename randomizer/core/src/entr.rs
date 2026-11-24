pub mod build;
pub mod data;
pub mod layout;
pub mod logic;
pub mod mode;
pub mod pools;
pub mod story;
pub mod text;

use std::collections::BTreeMap as BM;

use crate::error::Res;
use crate::settings::Settings;
use crate::world::{LocId, Placement, World};

pub use layout::Layout;
pub use mode::Mode;

pub fn choose(s: &Settings, w: &mut World, seed: &str, vanilla: &Placement, free: &[LocId]) -> Res<Option<Layout>> {
    match mode::read(s)? {
        Mode::Off => Ok(None),
        m => {
            let h: logic::Have = logic::have(w, vanilla, free);
            Ok(Some(logic::pick(w, m, seed, &h)?))
        }
    }
}

pub fn restore(w: &mut World, m: &BM<String, String>) -> Res<()> {
    if m.is_empty() {
        return Ok(());
    }
    let l: Layout = Layout::from_map(m)?;
    logic::apply(w, &l)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Value;
    use crate::settings::seed::SeedFile;
    use crate::slice::data::Data;
    use crate::slice::generate::{generate_data, GenOpts, Generated};

    fn with(pairs: &[(&str, &str)]) -> Settings {
        let mut s: Settings = Settings::default();
        for (k, v) in pairs {
            s.set(k, Value::s(v)).unwrap();
        }
        s
    }

    fn sound(d: &Data, g: &Generated) -> Layout {
        let back: SeedFile = SeedFile::parse(&g.seed_file.to_text()).unwrap();
        assert_eq!(back, g.seed_file);
        let l: Layout = Layout::from_map(&back.entrances).unwrap();
        assert!(layout::ages_fit(&data::load().unwrap(), &l).unwrap().is_empty());
        let (w, place): (World, Placement) = crate::slice::starts::rebuild(d, &back).unwrap();
        assert!(crate::search::explore(&w, &place, &[]).won(), "{}", back.seed);
        l
    }

    fn run(pairs: &[(&str, &str)], n: usize, tag: &str) -> (usize, usize) {
        let d: Data = Data::load().unwrap();
        let s: Settings = with(pairs);
        let o: GenOpts = GenOpts { spoiler: true, ..GenOpts::default() };
        let mut moved: usize = 0;
        let mut failed: usize = 0;
        for k in 0..n {
            match generate_data(&d, &s, &format!("{tag} {k}"), &o) {
                Ok(g) => {
                    let l: Layout = sound(&d, &g);
                    moved += l.moved();
                    let t: String = g.spoiler.unwrap();
                    assert!(t.starts_with("Entrances (dungeons, coupled)"));
                    assert!(g.report.contains(text::NOTE));
                }
                Err(e) => {
                    failed += 1;
                    eprintln!("{tag} {k}: {e}");
                }
            }
        }
        (moved, failed)
    }

    #[test]
    fn dungeon_entrance_seeds_are_beatable_and_rebuild_the_same_world() {
        let (moved, failed): (usize, usize) = run(&[(mode::SETTING, "dungeons")], 4, "gates");
        assert_eq!(failed, 0);
        assert!(moved >= 8, "{moved}");
    }

    #[test]
    fn shuffled_hideout_keys_do_not_block_dungeon_entrances() {
        for keys in ["any_dungeon", "keysanity"] {
            let (_, failed): (usize, usize) = run(
                &[
                    (mode::SETTING, "dungeons"),
                    ("gerudo_fortress", "open"),
                    ("shuffle_hideoutkeys", keys),
                    ("shuffle_chests", "all"),
                ],
                3,
                &format!("hideout {keys}"),
            );
            assert_eq!(failed, 0, "{keys}");
        }
    }

    #[test]
    fn the_setting_off_leaves_the_seed_file_without_entrances() {
        let d: Data = Data::load().unwrap();
        let g: Generated = generate_data(&d, &Settings::default(), "plain", &GenOpts::default()).unwrap();
        assert!(g.seed_file.entrances.is_empty());
        assert!(!g.seed_file.to_text().contains("entrances"));
        assert!(!g.report.contains(text::NOTE));
    }

    #[test]
    fn unsupported_modes_refuse_to_generate() {
        let d: Data = Data::load().unwrap();
        for v in ["overworld", "everything"] {
            let e: String = generate_data(&d, &with(&[(mode::SETTING, v)]), "x", &GenOpts::default()).err().unwrap().to_string();
            assert!(e.contains(&format!("{} is {v}", mode::SETTING)), "{e}");
        }
    }

    #[test]
    #[ignore]
    fn two_hundred_combined_seeds() {
        let n: usize = std::env::var("OOTRR_ENTR_SEEDS").ok().and_then(|v| v.parse().ok()).unwrap_or(200);
        let from: usize = std::env::var("OOTRR_ENTR_FROM").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
        let pairs: [(&str, &str); 5] = [
            (mode::SETTING, "dungeons"),
            ("shuffle_chests", "all"),
            ("shuffle_rewards", "supported"),
            ("shuffle_smallkeys", "keysanity"),
            ("shuffle_skulltulas", "matched"),
        ];
        let (moved, failed): (usize, usize) = run(&pairs, n, &format!("combo{from}"));
        eprintln!("seeds {n} failed {failed} moved {moved}");
        assert_eq!(failed, 0);
    }
}
