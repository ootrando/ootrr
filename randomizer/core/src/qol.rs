pub mod anju;
pub mod blocks;
pub mod chest_anim;
pub mod dampe_dig;
pub mod dampe_race;
pub mod fishing;
pub mod gear_cycle;
pub mod hook_probe;
pub mod low_health;
pub mod night_skulls;
pub mod scarecrow;
pub mod skull_kid_session;
pub mod tower_escape;
pub mod bunny;
pub mod navi_return;
pub mod owl;
pub mod poe;
pub mod skip;

use std::fs;
use std::path::Path as P;

use crate::error::{Error, Res};
use crate::flow_locations::dict_id;
use crate::romfs::Romfs;
use crate::settings::Settings;
use crate::staging::{self, Out};
use crate::zs;

pub const SKIP: &str = "skip_cutscenes";
pub const ANJU: &str = "anju_one_cucco";
pub const POE: &str = "big_poe_one";
pub const NAVI: &str = "navi_return";
pub const BUNNY: &str = "bunny_hood";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Skip {
    #[default]
    Off,
    Story,
    All,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Opts {
    pub skip: Skip,
    pub anju: bool,
    pub poe: bool,
    pub navi: navi_return::Mode,
    pub bunny: bunny::Mode,
    pub big_chests: bool,
    pub blocks: bool,
    pub owls: bool,
    pub suns_skulls: bool,
    pub any_fish: bool,
    pub free_scarecrow: bool,
    pub skip_escape: bool,
    pub fast_session: bool,
    pub one_race: bool,
    pub first_dig: bool,
    pub gear_cycle: bool,
    pub low_health: bool,
}

pub type Have<'a> = &'a dyn Fn(&str) -> Option<Vec<u8>>;

impl Opts {
    pub fn any(&self) -> bool {
        self.skip != Skip::Off || self.anju || self.poe || self.navi != navi_return::Mode::Off || self.bunny != bunny::Mode::Off || self.big_chests || self.blocks || self.owls || self.low_health
    }

    pub fn logic(&self) -> Vec<(&'static str, &'static str)> {
        let mut out: Vec<(&'static str, &'static str)> = Vec::new();
        if self.anju {
            out.push((anju::LOGIC_KEY, "1"));
        }
        if self.poe {
            out.push((poe::LOGIC_KEY, "1"));
        }
        if self.suns_skulls {
            out.push((night_skulls::LOGIC_SUNS, "true"));
        }
        if self.free_scarecrow {
            out.push((scarecrow::LOGIC_KEY, scarecrow::LOGIC_ON));
        }
        if self.skip_escape {
            out.push((tower_escape::SETTING, "true"));
        }
        out
    }

    pub fn notes(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        match self.skip {
            Skip::Off => {}
            Skip::Story => out.push(skip::NOTE_STORY.to_string()),
            Skip::All => out.push(skip::NOTE_ALL.to_string()),
        }
        if self.anju {
            out.push(anju::NOTE.to_string());
        }
        if self.poe {
            out.push(poe::NOTE.to_string());
        }
        match self.navi {
            navi_return::Mode::Off => {}
            navi_return::Mode::Dungeons => out.push(navi_return::NOTE_DUNGEONS.to_string()),
            navi_return::Mode::Everywhere => out.push(navi_return::NOTE_EVERYWHERE.to_string()),
        }
        match self.bunny {
            bunny::Mode::Off => {}
            bunny::Mode::Speed => out.push(bunny::NOTE_SPEED.to_string()),
            bunny::Mode::Jump => out.push(bunny::NOTE_JUMP.to_string()),
        }
        if self.big_chests {
            out.push(chest_anim::NOTE.to_string());
        }
        if self.blocks {
            out.push(blocks::NOTE.to_string());
        }
        if self.owls {
            out.push(owl::NOTE.to_string());
        }
        if self.suns_skulls {
            out.push(night_skulls::NOTE_SUNS.to_string());
        }
        if self.any_fish {
            out.push(fishing::NOTE.to_string());
        }
        if self.free_scarecrow {
            out.push(scarecrow::NOTE.to_string());
        }
        if self.skip_escape {
            out.push(tower_escape::NOTE.to_string());
        }
        if self.fast_session {
            out.push(skull_kid_session::NOTE.to_string());
        }
        if self.one_race {
            out.push(dampe_race::NOTE.to_string());
        }
        if self.first_dig {
            out.push(dampe_dig::NOTE.to_string());
        }
        if self.gear_cycle {
            out.push(gear_cycle::NOTE.to_string());
        }
        if self.low_health {
            out.push(low_health::NOTE.to_string());
        }
        out
    }

    pub fn maps(&self) -> bool {
        self.skip_escape
    }
}

pub fn read(s: &Settings, bad: &mut Vec<String>) -> Opts {
    let skip: Skip = match s.text(SKIP) {
        Ok("off") => Skip::Off,
        Ok("story") => Skip::Story,
        Ok("all") => Skip::All,
        Ok(v) => {
            bad.push(format!("{SKIP} is {v}, which is not a known choice"));
            Skip::Off
        }
        Err(e) => {
            bad.push(e.to_string());
            Skip::Off
        }
    };
    let mut flag = |id: &str| -> bool {
        s.flag(id).unwrap_or_else(|e| {
            bad.push(e.to_string());
            false
        })
    };
    let anju: bool = flag(ANJU);
    let poe: bool = flag(POE);
    let big_chests: bool = flag(chest_anim::SETTING);
    let blocks: bool = flag(blocks::SETTING);
    let owls: bool = flag(owl::SETTING);
    let suns_skulls: bool = flag(night_skulls::SUNS);
    let any_fish: bool = true;
    let free_scarecrow: bool = flag(scarecrow::SETTING);
    let skip_escape: bool = flag(tower_escape::SETTING);
    let fast_session: bool = true;
    let one_race: bool = true;
    let first_dig: bool = true;
    let gear_cycle: bool = true;
    let low_health: bool = flag(low_health::SETTING);
    let navi: navi_return::Mode = match s.text(NAVI) {
        Ok("off") => navi_return::Mode::Off,
        Ok("dungeons") => navi_return::Mode::Dungeons,
        Ok("everywhere") => navi_return::Mode::Everywhere,
        Ok(v) => {
            bad.push(format!("{NAVI} is {v}, which is not a known choice"));
            navi_return::Mode::Off
        }
        Err(e) => {
            bad.push(e.to_string());
            navi_return::Mode::Off
        }
    };
    let bunny: bunny::Mode = match s.text(BUNNY) {
        Ok("off") => bunny::Mode::Off,
        Ok("speed") => bunny::Mode::Speed,
        Ok("jump") => bunny::Mode::Jump,
        Ok(v) => {
            bad.push(format!("{BUNNY} is {v}, which is not a known choice"));
            bunny::Mode::Off
        }
        Err(e) => {
            bad.push(e.to_string());
            bunny::Mode::Off
        }
    };
    Opts { skip, anju, poe, navi, bunny, big_chests, blocks, owls, suns_skulls, any_fish, free_scarecrow, skip_escape, fast_session, one_race, first_dig, gear_cycle, low_health }
}

pub fn settings() -> Vec<crate::settings::Setting> {
    use crate::settings::Setting;
    vec![
        Setting::toggle(night_skulls::SUNS_NUM, night_skulls::SUNS, "world", "Night Gold Skulltulas expect Sun's Song", false)
            .tip("The logic only expects you to get a night Gold Skulltula once you can play Sun's Song, so no check is placed behind one before that. Like the OoTR trick option of the same name. Has no effect with Skulltulas by day."),
        Setting::toggle(scarecrow::NUM, scarecrow::SETTING, "world", "Free Scarecrow's Song", false)
            .tip("Start with Scarecrow's Song and an instant scarecrow: take out the Ocarina at one of Pierre's spots and he appears there, with no song to record or play. The logic counts the song as known, like the OoTR option of the same name. Tested in the game."),
        Setting::toggle(tower_escape::NUM, tower_escape::SETTING, "goal", "Skip the tower escape", false)
            .tip("After Ganondorf falls, Link goes straight to the field below the collapsing tower and the Ganon fight, with no escape run down the tower, so the logic no longer needs the Longshot to finish. Without this, the logic needs the Longshot for the escape. Tested in the game."),
        Setting::toggle(low_health::NUM, low_health::SETTING, "world", "Disable low-health warnings", false)
            .tip("Suppress the low-health beep, Link's critical-health pose and expression, flashing hearts, and the red pulse on Link. Health and damage are unchanged. The red pulse change still needs playtesting."),
    ]
}

pub fn files() -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    out.push(skip::FILE.to_string());
    out.extend(skip::LISTS.iter().map(|s| s.to_string()));
    out.push(anju::FILE.to_string());
    out.push(poe::FILE.to_string());
    out.push(navi_return::FILE.to_string());
    out.push(navi_return::SEQ.to_string());
    out.push(bunny::FILE.to_string());
    out.push(bunny::ADULT_FILE.to_string());
    out.extend(chest_anim::files().iter().map(|s| s.to_string()));
    out.extend(blocks::files());
    out
}

fn source(rf: &Romfs, have: Have, path: &str) -> Res<Vec<u8>> {
    match have(path) {
        Some(b) => Ok(b),
        None => rf.read(path),
    }
}

pub fn edit_plain(rf: &Romfs, have: Have, path: &str, f: impl FnOnce(&[u8]) -> Res<Vec<u8>>) -> Res<Out> {
    let raw: Vec<u8> = source(rf, have, path)?;
    let out: Vec<u8> = f(&raw)?;
    Ok(Out { path: path.to_string(), old: raw.len(), new: out.len(), z: out })
}

pub fn edit_zs(rf: &mut Romfs, have: Have, path: &str, f: impl FnOnce(&[u8]) -> Res<Vec<u8>>) -> Res<Out> {
    let z: Vec<u8> = source(rf, have, path)?;
    let id: Option<u32> = dict_id(&z);
    let raw: Vec<u8> = rf.zs.decompress(&z)?;
    let out: Vec<u8> = f(&raw)?;
    Ok(Out { path: path.to_string(), old: raw.len(), new: out.len(), z: zs::compress_raw(&out, id)? })
}

pub fn stage(rf: &mut Romfs, o: &Opts, have: Have) -> Res<Vec<Out>> {
    let mut outs: Vec<Out> = Vec::new();
    outs.extend(skip::stage(rf, o.skip, have)?);
    if o.anju {
        outs.push(anju::stage(rf, have)?);
    }
    if o.poe {
        outs.push(poe::stage(rf, have)?);
    }
    outs.extend(navi_return::stage(rf, o.navi, have)?);
    outs.extend(bunny::stage(rf, o.bunny, have)?);
    if o.big_chests {
        outs.extend(chest_anim::stage(rf, have)?);
    }
    if o.blocks {
        outs.extend(blocks::stage(rf, have)?);
    }
    if o.owls {
        owl::stage(rf, have, &mut outs)?;
    }
    if o.gear_cycle {
        outs.extend(gear_cycle::stage(rf, have)?);
        // On top of the Bunny Hood edit when that option changed the same packs.
        for (file, _) in bunny::PACKS {
            stack(rf, &mut outs, have, file, gear_cycle::patch_boots_hop)?;
        }
    }
    if o.low_health {
        outs.extend(low_health::stage(rf, have)?);
        for file in low_health::PACKS {
            stack(rf, &mut outs, have, file, low_health::pulse_pack)?;
        }
    }
    Ok(outs)
}

/// Edits `path` on top of an earlier edit of it in `outs`, keeping that edit's original size
/// for the size table, or on top of the worked or game file when nothing edited it yet.
fn stack(rf: &mut Romfs, outs: &mut Vec<Out>, have: Have, path: &str, f: impl FnOnce(&[u8]) -> Res<Vec<u8>>) -> Res<()> {
    match outs.iter().position(|o| o.path == path) {
        Some(i) => {
            let id: Option<u32> = dict_id(&outs[i].z);
            let raw: Vec<u8> = rf.zs.decompress(&outs[i].z)?;
            let b: Vec<u8> = f(&raw)?;
            outs[i].new = b.len();
            outs[i].z = zs::compress_raw(&b, id)?;
        }
        None => outs.push(edit_zs(rf, have, path, f)?),
    }
    Ok(())
}

pub fn build(romfs: &P, load: &P, name: &str, o: &Opts) -> Res<String> {
    if !o.any() && !o.gear_cycle {
        return Err(Error::Format("qol: nothing selected"));
    }
    let mut rf: Romfs = Romfs::open(romfs)?;
    let outs: Vec<Out> = stage(&mut rf, o, &|_: &str| None)?;
    let c: staging::Committed = staging::commit(&mut rf, load, name, &outs, None)?;
    Ok(c.report)
}

pub fn worked(rf: &mut Romfs, pk: &crate::modpack::Pack, o: &Opts) -> Res<Vec<Out>> {
    let root: std::path::PathBuf = pk.work.join("romfs");
    let have = |p: &str| -> Option<Vec<u8>> { fs::read(p.split('/').fold(root.clone(), |a, s| a.join(s))).ok() };
    stage(rf, o, &have)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Value;

    fn always() -> Opts {
        Opts { any_fish: true, fast_session: true, one_race: true, first_dig: true, gear_cycle: true, ..Opts::default() }
    }

    const ALWAYS_NOTES: usize = 5;

    /// The defaults with Return with Navi off, the one option on by default.
    fn quiet() -> Settings {
        let mut s: Settings = Settings::default();
        s.set(NAVI, Value::s("off")).unwrap();
        s
    }

    #[test]
    fn only_return_with_navi_is_on_by_default() {
        let mut bad: Vec<String> = Vec::new();
        let o: Opts = read(&Settings::default(), &mut bad);
        assert!(bad.is_empty());
        assert_eq!(o, Opts { navi: navi_return::Mode::Everywhere, ..always() });
        assert!(o.any());
        assert!(o.logic().is_empty());
        assert_eq!(o.notes().len(), 1 + ALWAYS_NOTES);
        let q: Opts = read(&quiet(), &mut bad);
        assert_eq!(q, always());
        assert!(!q.any() && q.notes().len() == ALWAYS_NOTES);
    }

    #[test]
    fn each_setting_is_read_and_adds_its_logic_and_note() {
        let mut s: Settings = quiet();
        s.set(SKIP, Value::s("story")).unwrap();
        s.set(ANJU, Value::Bool(true)).unwrap();
        s.set(POE, Value::Bool(true)).unwrap();
        let mut bad: Vec<String> = Vec::new();
        let o: Opts = read(&s, &mut bad);
        assert!(bad.is_empty());
        assert_eq!(o, Opts { skip: Skip::Story, anju: true, poe: true, navi: navi_return::Mode::Off, bunny: bunny::Mode::Off, big_chests: false, blocks: false, owls: false, ..always() });
        s.set(BUNNY, Value::s("jump")).unwrap();
        assert_eq!(read(&s, &mut bad).bunny, bunny::Mode::Jump);
        assert_eq!(read(&s, &mut bad).notes().len(), 4 + ALWAYS_NOTES);
        s.set(BUNNY, Value::s("off")).unwrap();
        assert_eq!(o.logic(), vec![("chicken_count", "1"), ("big_poe_count", "1")]);
        assert_eq!(o.notes().len(), 3 + ALWAYS_NOTES);
        s.set(SKIP, Value::s("all")).unwrap();
        assert_eq!(read(&s, &mut bad).skip, Skip::All);
    }

    #[test]
    fn faster_pickups_give_big_chests_the_small_animation() {
        let mut s: Settings = quiet();
        let mut bad: Vec<String> = Vec::new();
        assert!(!read(&s, &mut bad).big_chests);
        s.set(chest_anim::SETTING, Value::Bool(true)).unwrap();
        let o: Opts = read(&s, &mut bad);
        assert!(bad.is_empty());
        assert!(o.big_chests && o.any());
        assert_eq!(o.notes()[0], chest_anim::NOTE);
        assert_eq!(o.notes().len(), 1 + ALWAYS_NOTES);
    }

    fn dump() -> Option<std::path::PathBuf> {
        let p: std::path::PathBuf = ootrr_game::test_romfs::path();
        if p.is_dir() {
            Some(p)
        } else {
            eprintln!("skipped: no romfs at {}, set OOTRR_ROMFS", p.display());
            None
        }
    }

    fn with(pairs: &[(&str, Value)]) -> Settings {
        let mut s: Settings = Settings::default();
        for (k, v) in pairs {
            s.set(k, v.clone()).unwrap();
        }
        s
    }

    fn built(root: &P, s: &Settings, seed: &str, name: &str) -> std::path::PathBuf {
        let d: crate::slice::data::Data = crate::slice::data::Data::load().unwrap();
        let g: crate::slice::generate::Generated = crate::slice::generate::generate_data(&d, s, seed, &crate::slice::generate::GenOpts::default()).unwrap();
        let load: std::path::PathBuf = std::env::temp_dir().join("ootrr_qol_mods");
        crate::slice::edit::build(&g.seed_file, root, &load, name).unwrap().root.join("romfs")
    }

    fn at(out: &P, r: &str) -> std::path::PathBuf {
        r.split('/').fold(out.to_path_buf(), |a, s| a.join(s))
    }

    #[test]
    fn the_plan_carries_each_option_into_logic_and_notes() {
        use crate::slice::support::plan;
        let p: crate::slice::support::Plan = plan(&quiet()).unwrap();
        assert!(!p.qol.any());
        assert_eq!(p.logic(), [&crate::slice::support::OWN_KEYS[..], &[(crate::open_world::EPONA, "1"), crate::open_world::trade::LOGIC[0]]].concat());
        let base: usize = p.notes.len();
        assert_eq!(plan(&Settings::default()).unwrap().notes.len(), base + 1, "Return with Navi adds its note");
        for (k, v, key) in [(ANJU, Value::Bool(true), "chicken_count"), (POE, Value::Bool(true), "big_poe_count")] {
            let p: crate::slice::support::Plan = plan(&with(&[(NAVI, Value::s("off")), (k, v)])).unwrap();
            assert!(p.qol.any());
            assert_eq!(p.logic(), [&crate::slice::support::OWN_KEYS[..], &[(key, "1"), (crate::open_world::EPONA, "1"), crate::open_world::trade::LOGIC[0]]].concat());
            assert_eq!(p.notes.len(), base + 1);
        }
        let p: crate::slice::support::Plan = plan(&with(&[(SKIP, Value::s("all"))])).unwrap();
        assert_eq!(p.qol.skip, Skip::All);
        assert_eq!(p.logic(), [&crate::slice::support::OWN_KEYS[..], &[(crate::open_world::EPONA, "1"), crate::open_world::trade::LOGIC[0]]].concat());
        assert!(Settings::default().set(SKIP, Value::s("everything")).is_err());
    }

    #[test]
    fn the_logic_override_turns_into_an_int_and_changes_the_two_rules() {
        use crate::logic_items::ItemId;
        use crate::logic_val::Val;
        use crate::slice::data::Data;
        use crate::slice::logic::{spec, world_with};
        use crate::world::{Spec, World};
        let d: Data = Data::load().unwrap();
        let before: Spec = spec(&d, &[]).unwrap();
        let after: Spec = spec(&d, &[("chicken_count", "1"), ("big_poe_count", "1")]).unwrap();
        assert_eq!(before.settings.get("chicken_count"), Some(&Val::Int(7)));
        assert_eq!(after.settings.get("chicken_count"), Some(&Val::Int(1)));
        assert_eq!(after.settings.get("big_poe_count"), Some(&Val::Int(1)));
        let w0: World = world_with(&d, &[("deadly_bonks", "ohko")]).unwrap();
        let w1: World = world_with(&d, &[("deadly_bonks", "ohko"), ("chicken_count", "1"), ("big_poe_count", "1")]).unwrap();
        let rule = |w: &World, n: &str| -> (Vec<ItemId>, String) {
            let r: &crate::rules::Rule = &w.locs[w.loc(n).unwrap()].rule;
            (r.deps.clone(), format!("{:?}", r.node))
        };
        let (a0, n0): (Vec<ItemId>, String) = rule(&w0, "Kak Anju as Child");
        let (a1, n1): (Vec<ItemId>, String) = rule(&w1, "Kak Anju as Child");
        assert!(a1.len() < a0.len(), "{} {}", a1.len(), a0.len());
        assert!(n1.len() < n0.len(), "{n0} {n1}");
        let (_, p0): (Vec<ItemId>, String) = rule(&w0, "Market 5 Big Poes");
        let (_, p1): (Vec<ItemId>, String) = rule(&w1, "Market 5 Big Poes");
        let poes: String = format!("Has({}, 5)", w0.item("Bottle with Big Poe").unwrap());
        assert!(p0.contains(&poes), "{p0}");
        assert!(p1.contains("Any(") && !p1.contains(&poes), "{p1}");
        let (q, _): (Vec<ItemId>, String) = rule(&world_with(&d, &[]).unwrap(), "Kak Anju as Child");
        assert!(q.is_empty());
    }

    #[test]
    fn seeds_generate_with_the_logic_overrides_and_the_two_toggles_never_change_the_default_seed() {
        use crate::slice::data::Data;
        use crate::slice::generate::{generate_data, GenOpts, Generated};
        let d: Data = Data::load().unwrap();
        let on: Settings = with(&[(ANJU, Value::Bool(true)), (POE, Value::Bool(true)), (SKIP, Value::s("story"))]);
        for seed in ["qol a", "qol b", "qol c"] {
            let a: Generated = generate_data(&d, &Settings::default(), seed, &GenOpts::default()).unwrap();
            let b: Generated = generate_data(&d, &on, seed, &GenOpts::default()).unwrap();
            assert!(!a.seed_file.placements.is_empty());
            assert_eq!(a.seed_file.placements.len(), b.seed_file.placements.len());
        }
        let skip_only: Settings = with(&[(SKIP, Value::s("all"))]);
        let a: Generated = generate_data(&d, &Settings::default(), "qol a", &GenOpts::default()).unwrap();
        let b: Generated = generate_data(&d, &skip_only, "qol a", &GenOpts::default()).unwrap();
        assert_eq!(a.seed_file.placements, b.seed_file.placements);
    }

    #[test]
    fn the_boots_hop_edit_stacks_on_the_bunny_hood_edit() {
        let Some(root) = dump() else {
            return;
        };
        let mut rf: Romfs = Romfs::open(&root).unwrap();
        let o: Opts = Opts { bunny: bunny::Mode::Speed, ..always() };
        let outs: Vec<Out> = stage(&mut rf, &o, &|_: &str| None).unwrap();
        for p in [bunny::FILE, bunny::ADULT_FILE] {
            let hits: Vec<&Out> = outs.iter().filter(|x| x.path == p).collect();
            assert_eq!(hits.len(), 1, "{p} is staged once");
            let es: Vec<crate::sarc::Entry> = crate::sarc::parse(&rf.zs.decompress(&hits[0].z).unwrap()).unwrap();
            let game: Vec<u8> = rf.read_zs(p).unwrap();
            let bunny_only: Vec<crate::sarc::Entry> = crate::sarc::parse(&bunny::patch_pack(&game, bunny::Mode::Speed).unwrap()).unwrap();
            let hop_only: Vec<crate::sarc::Entry> = crate::sarc::parse(&gear_cycle::patch_boots_hop(&game).unwrap()).unwrap();
            assert_eq!(crate::sarc::get(&es, bunny::WALK.file).unwrap().data, crate::sarc::get(&bunny_only, bunny::WALK.file).unwrap().data, "{p} keeps the Bunny Hood edit");
            assert_eq!(crate::sarc::get(&es, gear_cycle::SCHEDULER).unwrap().data, crate::sarc::get(&hop_only, gear_cycle::SCHEDULER).unwrap().data, "{p} drops the hop");
            assert_eq!(hits[0].old, game.len(), "{p} keeps the game size for the size table");
        }
        assert!(outs.iter().any(|x| x.path == gear_cycle::COUNTER_FILE));
    }

    #[test]
    fn default_builds_write_none_of_the_option_files() {
        let Some(root) = dump() else {
            return;
        };
        let out: std::path::PathBuf = built(&root, &with(&[("shuffle_rewards", Value::s("off")), (NAVI, Value::s("off"))]), "qol default", "OOTRR Qol Default Test");
        let mut rf: Romfs = Romfs::open(&root).unwrap();
        // The gear cycle is always on: the player packs always lose the boots-change hop, and
        // nothing else in them changes without the Bunny Hood option.
        let packs: [&str; 2] = [bunny::FILE, bunny::ADULT_FILE];
        for f in files().into_iter().filter(|f| !chest_anim::files().contains(&f.as_str()) && !packs.contains(&f.as_str())) {
            assert!(!at(&out, &f).exists(), "{f}");
        }
        for p in packs {
            let built: Vec<crate::sarc::Entry> = crate::sarc::parse(&rf.zs.decompress(&std::fs::read(at(&out, p)).unwrap()).unwrap()).unwrap();
            let game: Vec<crate::sarc::Entry> = crate::sarc::parse(&rf.read_zs(p).unwrap()).unwrap();
            let hop: Vec<u8> = gear_cycle::patch_boots_hop(&rf.read_zs(p).unwrap()).unwrap();
            let want: Vec<crate::sarc::Entry> = crate::sarc::parse(&hop).unwrap();
            assert_eq!(crate::sarc::get(&built, gear_cycle::SCHEDULER).unwrap().data, crate::sarc::get(&want, gear_cycle::SCHEDULER).unwrap().data, "{p}");
            assert_eq!(crate::sarc::get(&built, bunny::WALK.file).unwrap().data, crate::sarc::get(&game, bunny::WALK.file).unwrap().data, "{p}");
        }
        let out: std::path::PathBuf = built(&root, &with(&[(NAVI, Value::s("off"))]), "qol default", "OOTRR Qol Rewards Default Test");
        for f in files().into_iter().filter(|f| !packs.contains(&f.as_str())) {
            if f == poe::FILE && at(&out, &f).is_file() {
                let z: Vec<u8> = std::fs::read(at(&out, &f)).unwrap();
                let b: crate::bfevfl::Bf = crate::bfevfl::Bf::parse(&rf.zs.decompress(&z).unwrap()).unwrap();
                assert_eq!(poe::sites(&b.flows[0], poe::OLD).len(), poe::SITES);
            } else if f == chest_anim::files()[0] && at(&out, &f).is_file() {
                let z: Vec<u8> = std::fs::read(at(&out, &f)).unwrap();
                let b: crate::bfevfl::Bf = crate::bfevfl::Bf::parse(&rf.zs.decompress(&z).unwrap()).unwrap();
                assert!(!b.flows[0].entries.iter().any(|e| e.name.starts_with(chest_anim::PREFIX)));
            } else if !chest_anim::files().contains(&f.as_str()) {
                assert!(!at(&out, &f).exists(), "{f}");
            }
        }
    }

    #[test]
    fn skip_builds_write_the_lock_edits_and_nothing_for_the_title_demos() {
        use crate::byml::{Byml, Node};
        let Some(root) = dump() else {
            return;
        };
        let out: std::path::PathBuf = built(&root, &with(&[(SKIP, Value::s("story"))]), "qol skip", "OOTRR Qol Skip Story Test");
        assert!(!at(&out, skip::FILE).exists());
        for l in skip::LISTS {
            let y: Byml = Byml::parse(&std::fs::read(at(&out, l)).unwrap()).unwrap();
            assert!(y.root.get("Components").unwrap().get(skip::COMPONENT).is_none());
        }
        let out: std::path::PathBuf = built(&root, &with(&[(SKIP, Value::s("all"))]), "qol skip", "OOTRR Qol Skip All Test");
        let y: Byml = Byml::parse(&std::fs::read(at(&out, skip::FILE)).unwrap()).unwrap();
        assert_eq!(y.root.get(skip::KEY), Some(&Node::Bool(false)));
        for l in skip::LISTS {
            assert!(!at(&out, l).exists());
        }
    }

    #[test]
    fn the_anju_build_changes_one_byte_of_one_module() {
        let Some(root) = dump() else {
            return;
        };
        let out: std::path::PathBuf = built(&root, &with(&[(ANJU, Value::Bool(true))]), "qol anju", "OOTRR Qol Anju Test");
        let rf: Romfs = Romfs::open(&root).unwrap();
        let a: Vec<u8> = rf.read(anju::FILE).unwrap();
        let b: Vec<u8> = std::fs::read(at(&out, anju::FILE)).unwrap();
        assert_eq!(a.len(), b.len());
        assert_eq!((0..a.len()).filter(|&i| a[i] != b[i]).count(), 1);
        let m: crate::ainb::Ainb = crate::ainb::Ainb::parse(&b).unwrap();
        assert_eq!(m.in_s32(17, anju::INPUT), Some(anju::NEW));
    }

    #[test]
    fn the_poe_build_merges_with_the_reward_shuffle_instead_of_replacing_it() {
        use crate::bfevfl::{Bf, Flow};
        let Some(root) = dump() else {
            return;
        };
        let mut rf: Romfs = Romfs::open(&root).unwrap();
        let mut load = |p: &std::path::Path| -> Bf {
            let z: Vec<u8> = std::fs::read(at(p, poe::FILE)).unwrap();
            Bf::parse(&rf.zs.decompress(&z).unwrap()).unwrap()
        };
        let base: Vec<(&str, Value)> = vec![("shuffle_rewards", Value::s("supported")), ("shuffle_dungeon_rewards", Value::s("anywhere"))];
        let mut on: Vec<(&str, Value)> = base.clone();
        on.push((POE, Value::Bool(true)));
        let a: std::path::PathBuf = built(&root, &with(&base), "qol poe", "OOTRR Qol Poe Off Test");
        let b: std::path::PathBuf = built(&root, &with(&on), "qol poe", "OOTRR Qol Poe On Test");
        let off: Bf = load(&a);
        let onn: Bf = load(&b);
        let (x, y): (&Flow, &Flow) = (&off.flows[0], &onn.flows[0]);
        assert_eq!(poe::sites(x, poe::OLD).len(), poe::SITES);
        assert!(poe::sites(y, poe::OLD).is_empty());
        let diff: Vec<usize> = (0..x.events.len()).filter(|&i| x.events[i] != y.events[i]).collect();
        assert_eq!(diff, vec![242, 243, 244, 246, 248, 250, 252]);
    }


    #[test]
    fn all_three_options_build_together_with_the_other_stagers() {
        use crate::bfevfl::Bf;
        let Some(root) = dump() else {
            return;
        };
        let s: Settings = with(&[
            (SKIP, Value::s("story")),
            (ANJU, Value::Bool(true)),
            (POE, Value::Bool(true)),
            (NAVI, Value::s("everywhere")),
            (BUNNY, Value::s("jump")),
            ("shuffle_rewards", Value::s("supported")),
            ("faster_chests", Value::Bool(true)),
            (blocks::SETTING, Value::Bool(true)),
            ("skip_intro", Value::Bool(true)),
        ]);
        let out: std::path::PathBuf = built(&root, &s, "qol all", "OOTRR Qol All Test");
        let mut rf: Romfs = Romfs::open(&root).unwrap();
        for f in files() {
            if f == skip::FILE {
                assert!(!at(&out, &f).exists(), "{f}");
            } else {
                assert!(at(&out, &f).is_file(), "{f}");
            }
        }
        let z: Vec<u8> = std::fs::read(at(&out, poe::FILE)).unwrap();
        let b: Bf = Bf::parse(&rf.zs.decompress(&z).unwrap()).unwrap();
        assert!(poe::sites(&b.flows[0], poe::OLD).is_empty());
        assert!(at(&out, crate::intro_skip::FILE).is_file());
    }

    #[test]
    fn the_file_list_is_complete_and_unique() {
        let v: Vec<String> = files();
        assert_eq!(v.len(), 26);
        let mut w: Vec<String> = v.clone();
        w.sort();
        w.dedup();
        assert_eq!(w.len(), v.len());
    }
}
