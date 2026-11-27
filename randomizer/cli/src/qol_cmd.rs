use std::path::Path as P;

use ootrr_core::error::{Error, Res};
use ootrr_core::qol::bunny;
use ootrr_core::qol::navi_return::Mode;
use ootrr_core::qol::{build, Opts, Skip};

fn arg<'a>(a: &'a [String], i: usize, what: &'static str) -> Res<&'a str> {
    a.get(i).map(|s| s.as_str()).ok_or(Error::Format(what))
}

pub fn run(a: &[String]) -> Res<()> {
    let root: &str = arg(a, 0, "missing romfs_root")?;
    let load: &str = arg(a, 1, "missing load_dir")?;
    let name: &str = arg(a, 2, "missing mod_name")?;
    let o: Opts = match arg(a, 3, "missing option (skip-story, skip-all, anju, poe, navi-dungeons, navi-everywhere, bunny-speed, bunny-jump, big-chest-anim, owl-warps, gear-cycle or low-health)")? {
        "skip-story" => Opts { skip: Skip::Story, ..Opts::default() },
        "skip-all" => Opts { skip: Skip::All, ..Opts::default() },
        "anju" => Opts { anju: true, ..Opts::default() },
        "poe" => Opts { poe: true, ..Opts::default() },
        "navi-dungeons" => Opts { navi: Mode::Dungeons, ..Opts::default() },
        "navi-everywhere" => Opts { navi: Mode::Everywhere, ..Opts::default() },
        "bunny-speed" => Opts { bunny: bunny::Mode::Speed, ..Opts::default() },
        "bunny-jump" => Opts { bunny: bunny::Mode::Jump, ..Opts::default() },
        "owl-warps" => Opts { owls: true, ..Opts::default() },
        "big-chest-anim" => Opts { big_chests: true, ..Opts::default() },
        "gear-cycle" => Opts { gear_cycle: true, ..Opts::default() },
        "low-health" => Opts { low_health: true, ..Opts::default() },
        x => return Err(Error::Msg(format!("unknown option {x}"))),
    };
    print!("{}", build(P::new(root), P::new(load), name, &o)?);
    Ok(())
}

/// Writes the Hookshot storage probe mods, on top of a seed mod's romfs when one is given.
pub fn hook_probe(a: &[String]) -> Res<()> {
    let root: &str = arg(a, 0, "missing romfs_root")?;
    let out: &str = arg(a, 1, "missing out_dir")?;
    let base: Option<&P> = a.get(2).map(|s| P::new(s.as_str()));
    let mut rf: ootrr_core::romfs::Romfs = ootrr_core::romfs::Romfs::open(P::new(root))?;
    let files: Vec<std::path::PathBuf> = ootrr_core::qol::hook_probe::probe(&mut rf, P::new(out), base)?;
    for (_, name, what) in ootrr_core::qol::hook_probe::ALL {
        println!("{name}: {what}");
    }
    println!("{} files under {out}", files.len());
    Ok(())
}

pub fn gear_probe(a: &[String]) -> Res<()> {
    let root: &str = arg(a, 0, "missing romfs_root")?;
    let out: &str = arg(a, 1, "missing out_dir")?;
    let mut rf: ootrr_core::romfs::Romfs = ootrr_core::romfs::Romfs::open(P::new(root))?;
    let packs: Vec<std::path::PathBuf> = a[2..].iter().map(std::path::PathBuf::from).collect();
    for f in ootrr_core::qol::gear_cycle::probe(&mut rf, P::new(out), &packs)? {
        println!("{}", f.display());
    }
    Ok(())
}
