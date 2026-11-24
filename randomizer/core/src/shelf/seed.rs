use std::fmt::Write as W;
use std::path::{Path as P, PathBuf as PB};

use crate::error::{Error as E, Res};
use crate::modpack::Host;
use crate::settings::seed::SeedFile;
use crate::shelf::stage::{run_with, Done};
use crate::shelf::{audit, Job};
use crate::shops::layout::Layout;
use crate::slice::data::Data;
use crate::slice::edit::Built;

pub fn after(seed: &SeedFile, romfs: &P, load: &P, mod_name: &str, host: Host, mut b: Built) -> Res<Built> {
    let p = crate::slice::support::plan_with_seed(&seed.settings()?.settings, Some(&seed.seed))?;
    crate::shops::seed::check(&p.shops, seed)?;
    let Some(l) = crate::shops::seed::layout(seed)? else {
        return Ok(b);
    };
    let d: Data = Data::load()?;
    let jobs: Vec<Job> = crate::shops::jobs::of(&d, &l)?;
    let base: PB = b.root.join("romfs");
    let done: Done = run_with(romfs, Some(&base), load, mod_name, host, &jobs, &[])?;
    if done.root != b.root {
        return Err(E::Msg(format!("the shelf files went to {} and not to the seed mod {}", done.root.display(), b.root.display())));
    }
    let lines: Vec<String> = audit::run(romfs, &base, &jobs)?;
    b.counts.files += done.files;
    let _ = writeln!(b.report, "{}", lines_for(&l, &jobs, &done));
    for x in lines {
        let _ = writeln!(b.report, "  {x}");
    }
    Ok(b)
}

fn lines_for(l: &Layout, jobs: &[Job], done: &Done) -> String {
    let mut out: String = String::new();
    let deals: usize = jobs.iter().filter(|j| j.once()).count();
    let _ = writeln!(out, "{} shelf slots in 8 shops were written, {} of them special deals through {} new shelf actors, all read back. None of the shelf edits is tested in the game except one Kokiri Shop deal:", jobs.len(), deals, done.shelves.len());
    for p in l.places.iter().filter(|p| p.deal) {
        let _ = writeln!(out, "  {} | {} | {} rupees | sold once", p.loc, p.item, p.price);
    }
    out.push_str(&done.report);
    out
}
