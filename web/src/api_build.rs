use std::fs;
use std::path::PathBuf as PB;
use std::sync::Arc;

use ootrr_core::TITLE_ID;
use ootrr_core::colors::{self, Colors};
use ootrr_core::cosmetic::{self, Cosmetics, Music};
use ootrr_core::sfx::Sfx;
use ootrr_core::json::Json;
use ootrr_core::modpack::Host;
use ootrr_core::settings::seed::SeedFile;
use ootrr_core::slice;

use crate::api_validate::{self, Report};
use crate::game_source::{self, Prepared, Side, Source};
use crate::body;
use crate::error::{Fail, Hr};
use crate::jobs::{Ctl, Jobs};
use crate::pathsafe;

fn refuse(m: &str) -> Fail {
    Fail::new(422, m)
}

fn inside(inner: &PB, outer: &PB) -> bool {
    match (fs::canonicalize(inner), fs::canonicalize(outer)) {
        (Ok(a), Ok(b)) => a.starts_with(b),
        _ => false,
    }
}

pub struct Plan {
    /// The full seed; for a race file, made again from it (crate::race) and never written out.
    pub seed: SeedFile,
    /// The race file, for a race seed: the only seed file a race build writes.
    pub race: Option<ootrr_core::race::RaceFile>,
    pub load: PB,
    pub name: String,
    pub target: PB,
    pub host: Host,
    pub cos: Cosmetics,
    pub spoiler: Option<String>,
    /// Where the game files come from; `romfs` is only its display path.
    pub source: Source,
    /// Add the v1.1.1 update's own files to the mod.
    pub with_update: bool,
    pub delete_others: bool,
}

pub const SPOILER_FILE: &str = "Spoiler.txt";

pub fn delete_others(root: &std::path::Path) -> std::io::Result<Vec<String>> {
    let Some(parent) = root.parent() else {
        return Ok(Vec::new());
    };
    let own = root.file_name();
    let mut gone: Vec<String> = Vec::new();
    let mut folders: Vec<&std::path::Path> = vec![parent];
    if parent.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.eq_ignore_ascii_case(TITLE_ID)) {
        if let Some(up) = parent.parent() {
            folders.push(up);
        }
    }
    for folder in folders {
        for e in fs::read_dir(folder)? {
            let p: PB = e?.path();
            if !p.is_dir() || p == root {
                continue;
            }
            if (p.join(SPOILER_FILE).is_file() || p.join(SEED_FILE).is_file()) && p.join("romfs").is_dir() {
                fs::remove_dir_all(&p)?;
                gone.push(p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default());
            }
        }
    }
    let _ = own;
    gone.sort();
    Ok(gone)
}
const SPOILER_MAX: usize = 4 * 1024 * 1024;

pub fn put_spoiler(root: &PB, text: Option<&str>) -> std::io::Result<()> {
    let p: PB = root.join(SPOILER_FILE);
    match text {
        Some(t) => fs::write(&p, t),
        None if p.is_file() => fs::remove_file(&p),
        None => Ok(()),
    }
}

pub const SEED_FILE: &str = "Seed.ootrr";

pub fn put_seed_text(root: &PB, text: Option<&str>) -> std::io::Result<()> {
    let p: PB = root.join(SEED_FILE);
    match text {
        Some(t) => fs::write(&p, t),
        None if p.is_file() => fs::remove_file(&p),
        None => Ok(()),
    }
}

fn mods_slot(md: &PB, game: &PB, romfs: &PB) -> bool {
    let own: PB = game.join("mods");
    if md == &own || inside(md, &own) {
        return true;
    }
    romfs.parent().is_some_and(|p| {
        let m: PB = p.join("mods");
        (md == &m || inside(md, &m)) && !inside(md, romfs)
    })
}

/// Where a build reads the game and writes the mod, checked: shared by seed and Archipelago builds.
pub struct Place {
    pub load: PB,
    pub name: String,
    pub target: PB,
    pub host: Host,
    pub source: Source,
    pub with_update: bool,
    pub delete_others: bool,
}

pub fn plan(b: &Json) -> Hr<Plan> {
    let pl: Place = place(b)?;
    let cos: Cosmetics = cosmetics(b)?;
    let (seed, race): (SeedFile, Option<ootrr_core::race::RaceFile>) = ootrr_core::race::open(body::text(b, "seed_text")?)?;
    let spoiler: Option<String> = match body::opt_text(b, "spoiler_text")? {
        // A race seed has no spoiler log, whatever the request carries.
        _ if race.is_some() || ootrr_core::race::is_race(&seed) => None,
        None | Some("") => None,
        Some(t) if t.len() > SPOILER_MAX => return Err(refuse("Spoiler: the spoiler log is too large.")),
        Some(t) => Some(t.to_string()),
    };
    Ok(Plan { seed, race, load: pl.load, name: pl.name, target: pl.target, host: pl.host, cos, spoiler, source: pl.source, with_update: pl.with_update, delete_others: pl.delete_others })
}

/// The cosmetic choices of a build request: music, sound effects, colours and the title line.
/// Shared by seed builds and Archipelago builds.
pub fn cosmetics(b: &Json) -> Hr<Cosmetics> {
    let music: Music = match body::opt_text(b, "music")? {
        None | Some("") => Music::Off,
        Some(s) => Music::parse(s).ok_or_else(|| refuse("Music: use off, grouped or anywhere."))?,
    };
    let sfx: Sfx = match body::opt_text(b, "sfx")? {
        None | Some("") => Sfx::Off,
        Some(s) => Sfx::parse(s).ok_or_else(|| refuse("Sound effects: use off, grouped or anywhere."))?,
    };
    let colors: Colors = match body::opt_text(b, "colors")? {
        None | Some("") => Colors::Off,
        Some(s) => Colors::parse(s).ok_or_else(|| refuse("Colours: use off, custom, random or shared."))?,
    };
    let seed_text: String = body::opt_text(b, "cosmetic_seed")?.unwrap_or("").trim().to_string();
    if seed_text.len() > 64 {
        return Err(refuse("Cosmetic seed: at most 64 characters."));
    }
    let title: bool = b.get("title_logo").and_then(|x| x.as_bool()) != Some(false);
    let mut pairs: Vec<(String, String)> = Vec::new();
    if let Some(v) = b.get("color_parts") {
        let o: &[(String, Json)] = v.as_obj().ok_or_else(|| refuse("Colours: the part colours must be a list of part names and colours."))?;
        if o.len() > colors::ITEMS.len() {
            return Err(refuse("Colours: too many part colours."));
        }
        for (k, x) in o {
            let t: &str = x.as_str().ok_or_else(|| refuse("Colours: each part colour is follow, original, random or a #rrggbb colour."))?;
            pairs.push((k.clone(), t.to_string()));
        }
    }
    let parts: colors::Picks = colors::parse_picks(&pairs).map_err(|e| refuse(&format!("Colours: {e}")))?;
    Ok(Cosmetics { music, sfx, colors, seed: seed_text, title, parts })
}

pub fn place(b: &Json) -> Hr<Place> {
    let game: PB = pathsafe::abs(body::short(b, "game_dir")?)?;
    let upd: Option<PB> = api_validate::opt_path(body::opt_text(b, "update_dir")?)?;
    let rep: Report = api_validate::inspect_with(&game, upd.as_deref());
    if !rep.ok {
        let mut why: Vec<String> = rep.problems.clone();
        if !rep.update_problems.is_empty() {
            why.push(format!("Update folder: {}", rep.update_problems.join(" ")));
        }
        return Err(refuse(&format!("Game folder: {}", why.join(" "))));
    }
    let source: Source = rep.source.clone().ok_or_else(|| refuse("Game folder: it could not be checked."))?;
    let with_update: bool = b.get("include_update").and_then(|x| x.as_bool()) == Some(true);
    if with_update && !source.has_update() {
        return Err(refuse("Update folder: there is no separate v1.1.1 update to add to the mod."));
    }
    let host: Host = match body::opt_text(b, "host")? {
        None | Some("") => Host::Eden,
        Some(s) => Host::parse(s).ok_or_else(|| refuse("Host: use eden, hookshot, longshot or nx2navi."))?,
    };
    let md: PB = pathsafe::abs(body::short(b, "mod_dir")?)?;
    let real: PB = rep.romfs.clone().unwrap_or_else(|| game.clone());
    if host.flat() && !md.is_dir() && md.file_name().and_then(|n| n.to_str()) == Some("mods") {
        fs::create_dir_all(&md).map_err(|_| refuse("Mod folder: could not create the mods folder."))?;
    }
    if !md.is_dir() {
        return Err(refuse("Mod folder: that folder does not exist. Create it first or pick another one."));
    }
    let name: &str = body::short(b, "mod_name")?;
    if !pathsafe::mod_name_ok(name) {
        return Err(refuse("Mod name: use 1 to 64 letters, digits, spaces and _ - ! . characters, with no spaces at the ends."));
    }
    if let Some(Side::Folder(u)) = &source.update
        && let Some(u) = u.parent()
        && inside(&md, &u.to_path_buf())
    {
        return Err(refuse("Mod folder: it may not be inside the v1.1.1 update folder."));
    }
    if inside(&md, &game) || inside(&md, &real) {
        if !host.flat() || !mods_slot(&md, &game, &real) {
            return Err(refuse("Mod folder: it may not be inside the game folder. Hookshot, Longshot and NX2navi use the game's mods folder."));
        }
    }
    let target: PB = if host.no_title() { md.join(name) } else { md.join(TITLE_ID).join(name) };
    let delete_others: bool = b.get("delete_others").and_then(|x| x.as_bool()) == Some(true);
    Ok(Place { load: md, name: name.to_string(), target, host, source, with_update, delete_others })
}

pub fn run(jobs: &Arc<Jobs>, b: &Json) -> Hr<Json> {
    if ootrr_core::ap::build::running() {
        return Err(Fail::new(409, "An Archipelago mod is being built. Wait for it to finish, then build again."));
    }
    let p: Plan = plan(b)?;
    let target: String = pathsafe::text(&p.target);
    let n: usize = p.seed.placements.len();
    // A race seed always shows its hash, drawn with the Randomizer line.
    let hash: Option<Vec<usize>> = p.race.as_ref().map(|_| ootrr_core::race::icons(&p.seed));
    let title: bool = p.cos.title || hash.is_some();
    let id: u64 = jobs.start(move |c: &Ctl| {
        c.begin(&if hash.is_some() { "Race file checked, the seed was made again and matches its hash".to_string() } else { format!("Seed file checked, {n} placements") });
        c.ok(10);
        if let Some(s) = p.source.step() {
            c.begin(s);
        }
        let game: Prepared = match game_source::prepare(&p.source, p.cos.music != Music::Off || p.cos.sfx != Sfx::Off) {
            Ok(g) => g,
            Err(e) => {
                c.mark("failed");
                c.finish("failed", &e);
                return;
            }
        };
        let romfs: PB = game.romfs.clone();
        if title {
            c.begin("Check that no other mod changes the title screen");
            if let Err(e) = ootrr_core::title_logo::preflight(&p.target, &romfs) {
                c.mark("failed");
                c.finish("failed", &e.to_string());
                return;
            }
        }
        c.begin("Read the game files and edit the chests");
        match slice::build_for(&p.seed, &romfs, &p.load, &p.name, p.host) {
            Ok(mut built) => {
                c.begin(if hash.is_some() { "Draw the Randomizer line and the seed hash on the title logo" } else if title { "Draw the Randomizer line under the title logo" } else { "Leave the title logo as the game's" });
                match if title { ootrr_core::title_logo::apply_with(&built.root, &romfs, hash.as_deref()) } else { Ok(Vec::new()) } {
                    Ok(notes) => {
                        for n in notes {
                            built.report.push_str(&format!("{n}
"));
                        }
                    }
                    Err(e) => {
                        c.mark("failed");
                        c.finish("failed", &e.to_string());
                        return;
                    }
                }
                if p.cos.any() {
                    c.begin("Write the cosmetic changes");
                    match cosmetic::apply(&built.root, &romfs, &p.cos) {
                        Ok(notes) => {
                            for n in notes {
                                built.report.push_str(&format!("{n}
"));
                            }
                        }
                        Err(e) => {
                            c.mark("failed");
                            c.finish("failed", &e.to_string());
                            return;
                        }
                    }
                }
                c.ok(90);
                c.begin(if p.race.is_some() { "Write the race file, with no spoiler log" } else if p.spoiler.is_some() { "Write the spoiler log and seed file" } else { "Remove any old spoiler log and seed file" });
                if let Err(e) = put_spoiler(&built.root, p.spoiler.as_deref()) {
                    c.mark("failed");
                    c.finish("failed", &format!("The mod was written, but {SPOILER_FILE} could not be: {e}"));
                    return;
                }
                let seed_text: Option<String> = match &p.race {
                    Some(rf) => Some(rf.to_text()),
                    None => p.spoiler.as_ref().map(|_| p.seed.to_text()),
                };
                if let Err(e) = put_seed_text(&built.root, seed_text.as_deref()) {
                    c.mark("failed");
                    c.finish("failed", &format!("The mod was written, but {SEED_FILE} could not be: {e}"));
                    return;
                }
                if p.race.is_some() {
                    built.report.push_str(&format!("Race file (share it so others can build the same seed): {}\n", pathsafe::text(&built.root.join(SEED_FILE))));
                } else if p.spoiler.is_some() {
                    built.report.push_str(&format!("Spoiler log: {}\n", pathsafe::text(&built.root.join(SPOILER_FILE))));
                    built.report.push_str(&format!("Seed file: {}\n", pathsafe::text(&built.root.join(SEED_FILE))));
                }
                if p.with_update {
                    c.begin("Add the v1.1.1 update files to the mod");
                    match game_source::add_update(&p.source, &built.root) {
                        Ok(n) => built.report.push_str(&format!("Added {n} v1.1.1 update files to the mod. Do not share this mod: it now holds game files.\n")),
                        Err(e) => {
                            c.mark("failed");
                            c.finish("failed", &format!("The mod was written, but the v1.1.1 update files could not be added: {e}"));
                            return;
                        }
                    }
                }
                if p.delete_others {
                    c.begin("Delete the other seeds in the mod folder");
                    match delete_others(&built.root) {
                        Ok(gone) if gone.is_empty() => built.report.push_str("No other seeds to delete.
"),
                        Ok(gone) => built.report.push_str(&format!("Deleted {} other seeds: {}.
", gone.len(), gone.join(", "))),
                        Err(e) => {
                            c.mark("failed");
                            c.finish("failed", &format!("The mod was written, but the other seeds could not all be deleted: {e}"));
                            return;
                        }
                    }
                }
                c.ok(95);
                c.begin("Mod files written");
                c.ok(100);
                c.finish("done", &built.report);
            }
            Err(e) => {
                c.mark("failed");
                c.finish("failed", &e.to_string());
            }
        }
    })?;
    Ok(Json::obj()
        .with("job_id", Json::Int(id as i64))
        .with("target", target.into())
        .with("writes_files", true.into())
        .with("notice", "Building the mod. Only the mod folder is written, the game folder is only read.".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ootrr_core::settings::Settings;
    use std::collections::BTreeMap;
    use std::env;
    use std::thread;
    use std::time::Duration;

    fn tmp(tag: &str) -> PB {
        let p: PB = env::temp_dir().join(format!("ootrr_web_build_{tag}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    fn game(root: &PB) -> PB {
        let g: PB = root.join("game");
        fs::create_dir_all(g.join("Pack")).unwrap();
        fs::create_dir_all(g.join("Banc")).unwrap();
        fs::write(g.join("Pack").join("ZsDic.pack.zs"), b"").unwrap();
        crate::api_validate::fake_version(&g);
        g
    }

    fn req(seed: &str, g: &PB, m: &PB, name: &str) -> Json {
        Json::obj()
            .with("seed_text", seed.into())
            .with("game_dir", g.display().to_string().into())
            .with("mod_dir", m.display().to_string().into())
            .with("mod_name", name.into())
    }

    fn seed_text() -> String {
        SeedFile::new("s", &Settings::default(), "oot-remake:test", BTreeMap::new()).unwrap().to_text()
    }

    fn files_under(p: &PB) -> usize {
        fs::read_dir(p).map(|d| d.count()).unwrap_or(0)
    }

    #[test]
    fn a_good_request_against_a_fake_game_folder_fails_cleanly_and_writes_nothing() {
        let d: PB = tmp("ok");
        let g: PB = game(&d);
        let m: PB = d.join("mods");
        fs::create_dir_all(&m).unwrap();
        let jobs: Arc<Jobs> = Jobs::new();
        let r: Json = run(&jobs, &req(&seed_text(), &g, &m, "My Seed")).unwrap();
        let id: u64 = r.get("job_id").and_then(|x| x.as_i64()).unwrap() as u64;
        let mut state: String = String::new();
        for _ in 0..300 {
            let j: Json = jobs.get(id).unwrap();
            state = j.get("state").and_then(|x| x.as_str()).unwrap().to_string();
            if state != "running" {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(state, "failed");
        assert_eq!(files_under(&m), 0);
        assert!(r.get("target").and_then(|x| x.as_str()).unwrap().ends_with("My Seed"));
        assert_eq!(r.get("writes_files").and_then(|x| x.as_bool()), Some(true));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn every_bad_input_is_refused_before_a_job_starts() {
        let d: PB = tmp("bad");
        let g: PB = game(&d);
        let m: PB = d.join("mods");
        fs::create_dir_all(&m).unwrap();
        let inside_game: PB = g.join("Banc");
        let empty: PB = d.join("empty");
        fs::create_dir_all(&empty).unwrap();
        let jobs: Arc<Jobs> = Jobs::new();
        let s: String = seed_text();
        let cases: Vec<Json> = vec![
            req("not a seed", &g, &m, "x"),
            req(&s.replace("\"s\"", "\"t\""), &g, &m, "x"),
            req(&s, &empty, &m, "x"),
            req(&s, &d.join("nope"), &m, "x"),
            req(&s, &g, &d.join("nope"), "x"),
            req(&s, &g, &m, ".."),
            req(&s, &g, &m, "a/b"),
            req(&s, &g, &m, ""),
            req(&s, &g, &inside_game, "x"),
            req(&s, &g, &g, "x"),
            req(&s, &g, &m.join("..").join("mods"), "x"),
        ];
        for c in &cases {
            assert!(run(&jobs, c).is_err(), "{c}");
        }
        assert_eq!(jobs.running(), 0);
        assert_eq!(files_under(&m), 0);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn hookshot_longshot_and_nx2navi_write_inside_the_games_mods_folder() {
        let d: PB = tmp("flat");
        let g: PB = game(&d);
        let m: PB = g.join("mods");
        let s: String = seed_text();
        assert!(!m.exists());
        assert!(plan(&req(&s, &g, &m, "randomizer-seed").with("host", "longshot".into())).is_ok());
        assert!(m.is_dir());
        assert!(plan(&req(&s, &g, &m, "x").with("host", "wii".into())).is_err());
        for host in ["hookshot", "longshot", "nx2navi"] {
            let body: Json = req(&s, &g, &m, "randomizer-seed").with("host", host.into());
            let p: Plan = plan(&body).unwrap();
            assert!(p.target.ends_with("randomizer-seed"), "{}", p.target.display());
            assert!(!p.target.components().any(|c| c.as_os_str() == TITLE_ID), "{host}");
            assert_eq!(p.host.id(), host);
        }
        assert!(plan(&req(&s, &g, &m, "x")).is_err());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn the_music_options_are_parsed_and_checked() {
        let d: PB = tmp("music");
        let g: PB = game(&d);
        let m: PB = d.join("mods");
        fs::create_dir_all(&m).unwrap();
        let s: String = seed_text();
        let base = || req(&s, &g, &m, "x");
        assert_eq!(plan(&base()).unwrap().cos.music, Music::Off);
        assert!(!plan(&base()).unwrap().cos.any());
        let p: Plan = plan(&base().with("music", "grouped".into()).with("cosmetic_seed", " tune ".into())).unwrap();
        assert_eq!(p.cos.music, Music::Grouped);
        assert_eq!(p.cos.seed, "tune");
        assert!(plan(&base().with("music", "loud".into())).is_err());
        let c: Plan = plan(&base().with("colors", "shared".into())).unwrap();
        assert_eq!(c.cos.colors, Colors::Shared);
        assert!(c.cos.any());
        assert_eq!(plan(&base()).unwrap().cos.colors, Colors::Off);
        assert!(plan(&base().with("colors", "neon".into())).is_err());
        let parts = |o: Json| plan(&base().with("color_parts", o));
        let p: Plan = parts(Json::obj().with("boots", "#804020".into()).with("kokiri", "original".into()).with("gloves", "random".into())).unwrap();
        assert_eq!(p.cos.parts.len(), 3);
        assert_eq!(p.cos.parts.get("boots"), Some(&colors::Pick::Rgb([0x80, 0x40, 0x20])));
        assert!(!p.cos.any());
        let c: Plan = plan(&base().with("colors", "custom".into()).with("color_parts", Json::obj().with("boots", "#804020".into()))).unwrap();
        assert!(c.cos.any() && c.cos.colors == Colors::Custom && !c.cos.rolls());
        let kept: Plan = plan(&base().with("colors", "custom".into()).with("color_parts", Json::obj().with("boots", "original".into()))).unwrap();
        assert!(!kept.cos.any());
        let mut all: Json = Json::obj();
        for i in &colors::ITEMS {
            all.put(i.id, "#102030".into());
        }
        let locked: Plan = plan(&base().with("colors", "random".into()).with("color_parts", all)).unwrap();
        assert!(locked.cos.any() && !locked.cos.rolls());
        assert!(parts(Json::obj().with("cape", "random".into())).is_err());
        assert!(parts(Json::obj().with("boots", "brown".into())).is_err());
        assert!(parts(Json::obj().with("boots", true.into())).is_err());
        assert!(parts("boots".into()).is_err());
        assert!(plan(&base().with("cosmetic_seed", "x".repeat(65).into())).is_err());
        assert!(plan(&base()).unwrap().cos.title);
        assert!(plan(&base().with("title_logo", true.into())).unwrap().cos.title);
        assert!(!plan(&base().with("title_logo", false.into())).unwrap().cos.title);
        let _ = fs::remove_dir_all(&d);
    }
}
