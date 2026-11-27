use std::fs;
use std::path::Path as P;
use std::sync::Arc;

use ootrr_core::colors::{self, Colors};
use ootrr_core::cosmetic::{self, Cosmetics, Music};
use ootrr_core::sfx::Sfx;
use ootrr_core::error::{Error, Res};
use ootrr_core::modpack::Host;
use ootrr_core::json::{self, Json};
use ootrr_core::settings::presets::{self, Preset};
use ootrr_core::settings::seed::SeedFile;
use ootrr_core::settings::{Loaded, Settings};
use ootrr_core::slice::generate::LIMIT;
use ootrr_core::slice::watch::Live;
use ootrr_core::slice::{self, plando, scan, BuildCounts, Built, GenOpts, Generated};

use ootrr_web::game_input::{self, Found, Prepared, Source};

use crate::gen_progress;

fn arg<'a>(a: &'a [String], i: usize, what: &'static str) -> Res<&'a str> {
    a.get(i).map(|s| s.as_str()).ok_or(Error::Format(what))
}

fn resolve(t: &str) -> Res<Loaded> {
    if t.starts_with("OOTRR") {
        return Settings::decode(t);
    }
    if let Some(p) = presets::builtin()?.into_iter().find(|p| p.id == t) {
        return Ok(Loaded { settings: p.apply(&Settings::default())?, warnings: Vec::new() });
    }
    let p: &P = P::new(t);
    if !p.is_file() {
        return Err(Error::Msg(format!("{t} is not a settings string, a preset id or a file")));
    }
    let text: String = fs::read_to_string(p)?;
    let j: Json = json::parse(&text)?;
    if j.get("id").is_some() {
        let pr: Preset = presets::parse(&text)?;
        return Ok(Loaded { settings: pr.apply(&Settings::default())?, warnings: Vec::new() });
    }
    Settings::from_json(&j)
}

pub fn generate(a: &[String]) -> Res<()> {
    let s: Loaded = resolve(arg(a, 0, "missing settings string, preset id or file")?)?;
    let seed: &str = arg(a, 1, "missing seed")?;
    let out: &str = arg(a, 2, "missing out file")?;
    let mut o: GenOpts = GenOpts { extras: true, ..GenOpts::default() };
    let mut spoiler: Option<&str> = None;
    let mut race: bool = false;
    let mut i: usize = 3;
    while i < a.len() {
        match a[i].as_str() {
            "--race" => race = true,
            "--spoiler" => {
                i += 1;
                spoiler = Some(arg(a, i, "--spoiler needs a file")?);
                o.spoiler = true;
            }
            "--plando" => {
                i += 1;
                o.plando.push(plando::parse(arg(a, i, "--plando needs Location=Item")?)?);
            }
            "--plando-file" => {
                i += 1;
                let t: String = fs::read_to_string(arg(a, i, "--plando-file needs a file")?)?;
                for line in t.lines().map(|l| l.trim()).filter(|l| !l.is_empty() && !l.starts_with('#')) {
                    o.plando.push(plando::parse(line)?);
                }
            }
            other => return Err(Error::Msg(format!("unknown option {other}"))),
        }
        i += 1;
    }
    for w in &s.warnings {
        eprintln!("warning: {w}");
    }
    let live: Arc<Live> = Arc::new(Live::default());
    o.limit = Some(LIMIT);
    o.live = Some(Arc::clone(&live));
    if race && spoiler.is_some() {
        return Err(Error::Msg(ootrr_core::race::NO_SPOILER.to_string()));
    }
    let line: gen_progress::Line = gen_progress::start(live);
    if race {
        let r = ootrr_core::race::generate(&s.settings, seed, &o);
        line.stop();
        let (g, rf) = r?;
        fs::write(out, rf.to_text())?;
        print!("{}", g.report.replace(&g.seed_file.seed, &rf.seed));
        println!("race file: {out} (no spoiler log; build it with ootrr build)");
        println!("seed hash: {}", rf.icon_names().join(", "));
        return Ok(());
    }
    let r: Res<Generated> = slice::generate_with(&s.settings, seed, &o);
    line.stop();
    let g: Generated = r?;
    g.seed_file.write(P::new(out))?;
    if let (Some(p), Some(t)) = (spoiler, &g.spoiler) {
        fs::write(p, t)?;
    }
    print!("{}", g.report);
    println!("seed file: {out}");
    Ok(())
}

pub fn build(a: &[String]) -> Res<()> {
    let mut rest: Vec<String> = Vec::new();
    let mut host: Host = Host::Eden;
    let mut cos: Cosmetics = Cosmetics::default();
    let mut update: Option<String> = None;
    let mut with_update: bool = false;
    let mut i: usize = 0;
    while i < a.len() {
        if a[i] == "--update" {
            update = Some(a.get(i + 1).cloned().ok_or(Error::Format("missing folder or package after --update"))?);
            i += 2;
            continue;
        }
        if a[i] == "--include-update" {
            with_update = true;
            i += 1;
            continue;
        }
        if a[i] == "--music" {
            let v: &str = a.get(i + 1).map(|s| s.as_str()).ok_or(Error::Format("missing mode after --music"))?;
            cos.music = Music::parse(v).ok_or(Error::Format("--music is off, grouped or anywhere"))?;
            i += 2;
            continue;
        }
        if a[i] == "--sfx" {
            let v: &str = a.get(i + 1).map(|s| s.as_str()).ok_or(Error::Format("missing mode after --sfx"))?;
            cos.sfx = Sfx::parse(v).ok_or(Error::Format("--sfx is off, grouped or anywhere"))?;
            i += 2;
            continue;
        }
        if a[i] == "--colors" {
            let v: &str = a.get(i + 1).map(|s| s.as_str()).ok_or(Error::Format("missing mode after --colors"))?;
            cos.colors = Colors::parse(v).ok_or(Error::Format("--colors is off, custom, random or shared"))?;
            i += 2;
            continue;
        }
        if a[i] == "--no-title-logo" {
            cos.title = false;
            i += 1;
            continue;
        }
        if a[i] == "--color-part" {
            let v: &str = a.get(i + 1).map(|s| s.as_str()).ok_or(Error::Format("missing part=colour after --color-part"))?;
            let (k, c): (&str, &str) = v.split_once('=').ok_or(Error::Format("--color-part takes part=colour, as boots=#804020, used with --colors custom"))?;
            cos.parts.extend(colors::parse_picks(&[(k.to_string(), c.to_string())])?);
            i += 2;
            continue;
        }
        if a[i] == "--cosmetic-seed" {
            cos.seed = a.get(i + 1).cloned().ok_or(Error::Format("missing text after --cosmetic-seed"))?;
            i += 2;
            continue;
        }
        if a[i] == "--host" {
            let v: &str = a.get(i + 1).map(|s| s.as_str()).ok_or(Error::Format("missing host after --host"))?;
            host = Host::parse(v).ok_or(Error::Format("--host is eden, hookshot, longshot or nx2navi"))?;
            i += 2;
            continue;
        }
        rest.push(a[i].clone());
        i += 1;
    }
    let (seed, race): (SeedFile, Option<ootrr_core::race::RaceFile>) = ootrr_core::race::open(&fs::read_to_string(arg(&rest, 0, "missing seed file")?)?)?;
    // A race seed always shows its hash, which is drawn with the Randomizer line.
    let hash: Option<Vec<usize>> = race.as_ref().map(|_| ootrr_core::race::icons(&seed));
    if hash.is_some() {
        cos.title = true;
    }
    let game: &str = arg(&rest, 1, "missing romfs_root")?;
    let load: &str = arg(&rest, 2, "missing load_dir")?;
    let name: &str = arg(&rest, 3, "missing mod_name")?;
    let source: Option<Source> = match game_input::check(P::new(game), update.as_deref().map(P::new)) {
        Found::Ok(s) => Some(s),
        Found::Refused(why) => return Err(Error::Msg(format!("game: {why}"))),
        Found::Unknown(_) if with_update => return Err(Error::Msg("--include-update needs a v1.0.0 game with a separate v1.1.1 update, or a package holding both".into())),
        Found::Unknown(_) => None,
    };
    if with_update && !source.as_ref().is_some_and(|s| s.has_update()) {
        return Err(Error::Msg("--include-update needs a separate v1.1.1 update (--update, or a package holding it)".into()));
    }
    if update.is_some() && source.as_ref().is_some_and(|s| s.update.is_none()) {
        println!("--update is not needed: the game is already v1.1.1, so it is ignored");
    }
    let sound: bool = cos.music != Music::Off || cos.sfx != Sfx::Off;
    let prepared: Option<Prepared> = match &source {
        Some(s) => {
            if let Some(step) = s.step() {
                println!("{step}");
            }
            Some(game_input::prepare(s, sound).map_err(Error::Msg)?)
        }
        None => None,
    };
    let romfs: &P = prepared.as_ref().map(|p| p.romfs.as_path()).unwrap_or(P::new(game));
    if cos.title {
        let target: std::path::PathBuf = if host.no_title() { P::new(load).join(name) } else { P::new(load).join(ootrr_core::TITLE_ID).join(name) };
        ootrr_core::title_logo::preflight(&target, romfs)?;
    }
    let b: Built = slice::build_for(&seed, romfs, P::new(load), name, host)?;
    let c: &BuildCounts = &b.counts;
    print!("{}", b.report);
    if cos.title {
        for n in ootrr_core::title_logo::apply_with(&b.root, P::new(romfs), hash.as_deref())? {
            println!("{n}");
        }
    }
    for n in cosmetic::apply(&b.root, P::new(romfs), &cos)? {
        println!("{n}");
    }
    if let (true, Some(s)) = (with_update, &source) {
        let n: usize = game_input::add_update(s, &b.root).map_err(Error::Msg)?;
        println!("added {n} v1.1.1 update files to the mod. Do not share this mod: it now holds game files.");
    }
    println!("changed {} of {} placed chests", c.changed, c.placed);
    Ok(())
}

pub fn preview(a: &[String]) -> Res<()> {
    let romfs: &str = arg(a, 0, "missing romfs_root")?;
    let out: &str = arg(a, 1, "missing out_dir")?;
    let mode: Colors = Colors::parse(arg(a, 2, "missing mode")?).ok_or(Error::Format("the mode is off, custom, random or shared"))?;
    let pairs: Vec<(String, String)> = a.iter().skip(4).filter_map(|x| x.split_once('=')).map(|(k, v)| (k.to_string(), v.to_string())).collect();
    let picks: colors::Picks = colors::parse_picks(&pairs)?;
    for l in colors::preview(P::new(romfs), P::new(out), mode, &picks, arg(a, 3, "missing seed text")?)? {
        println!("{l}");
    }
    Ok(())
}

pub fn scan_cmd(a: &[String]) -> Res<()> {
    let root: &str = arg(a, 0, "missing romfs_root")?;
    let out: &str = arg(a, 1, "missing out file")?;
    let s = scan(P::new(root))?;
    let mut t: String = scan::to_json(&s).pretty();
    t.push('\n');
    fs::write(out, t)?;
    print!("{}", scan::summary(&s));
    Ok(())
}
