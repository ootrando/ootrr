use std::collections::BTreeSet as BS;
use std::path::PathBuf as PB;

use crate::error::Res;
use crate::romfs::Romfs;
use crate::settings::seed::SeedFile;
use crate::slice::data::Data;
use crate::slice::support::{Keep, Plan, Songs};
use crate::staging::Out;

pub struct Ctx<'a> {
    pub d: &'a Data,
    pub seed: &'a SeedFile,
    pub p: &'a Plan,
    pub roam: bool,
    pub keys: &'a [&'static str],
    pub hearts: &'a [(u64, Vec<String>)],
    pub marked: &'a BS<(crate::dungeon_grant::Kind, &'static str)>,
    pub silver: &'a BS<&'static str>,
    pub quiet: &'a [&'a str],
    pub work: &'a PB,
    pub pots: &'a [u64],
    pub scrub_jobs: &'a [crate::scrubs_b::Job],
    pub scrub_rows: usize,
}

pub type Stage = fn(&mut Romfs, &Ctx, &mut Vec<Out>) -> Res<()>;

pub const STAGES: [(&str, Stage); 30] = [
    ("boss heart chests", boss_hearts),
    ("songs", songs),
    ("song models", song_models),
    ("hints", hints),
    ("dungeon item text", dungeon_text),
    ("key ring text", key_ring_text),
    ("scrub hint text", scrub_hints),
    ("fairy price text", fairy_text),
    ("silver rupee text", silver_text),
    ("pouch", pouch),
    ("bomb flowers", bomb_lift),
    ("quiet bomb and arrow pickups", quiet_pickups),
    ("door fix", door_fix),
    ("no tutorials", no_tutorials),
    ("open door of time", door_open),
    ("master sword pedestal", pedestal),
    ("bombchu alley", bombchu),
    ("water temple gate", water_gate),
    ("medallion gate", medal_gate),
    ("fire temple gorons", fire_gorons),
    ("start reward and flags", start_reward),
    ("starting items", start_items),
    ("dpad items", dpad),
    ("pot rows", pot_rows),
    ("hideout drop rows", hideout_drop_rows),
    ("key ring flags", key_ring_flags),
    ("start defaults", start_defaults),
    ("scrub rows", scrub_rows),
    ("debug location names", debug_names),
    ("threads of time keys", key_log),
];

fn key_log(rf: &mut Romfs, c: &Ctx, more: &mut Vec<Out>) -> Res<()> {
    crate::key_log::stage(rf, more, &crate::key_log::plan(c.d, c.seed)?)
}

pub fn run(rf: &mut Romfs, c: &Ctx, more: &mut Vec<Out>) -> Res<()> {
    for (_, s) in STAGES {
        s(rf, c, more)?;
    }
    Ok(())
}

fn boss_hearts(rf: &mut Romfs, c: &Ctx, more: &mut Vec<Out>) -> Res<()> {
    if !c.hearts.is_empty() {
        more.extend(crate::boss_heart::stage(rf, c.d, c.hearts, true)?);
    }
    Ok(())
}

fn songs(rf: &mut Romfs, c: &Ctx, more: &mut Vec<Out>) -> Res<()> {
    if c.p.songs != Songs::Off {
        more.push(crate::songs::stage(rf)?);
    }
    Ok(())
}

fn song_models(rf: &mut Romfs, c: &Ctx, more: &mut Vec<Out>) -> Res<()> {
    if c.p.songs == Songs::Anywhere {
        more.extend(crate::song_models::stage(rf, c.p.harp_colors)?);
    }
    Ok(())
}

fn hints(rf: &mut Romfs, c: &Ctx, more: &mut Vec<Out>) -> Res<()> {
    if c.p.hints || c.roam || c.p.npc.any() || !c.keys.is_empty() {
        more.extend(crate::hints::stage(rf, c.d, c.seed, c.p.hints, c.keys, &c.p.npc)?);
    }
    Ok(())
}

fn dungeon_text(rf: &mut Romfs, c: &Ctx, more: &mut Vec<Out>) -> Res<()> {
    crate::dungeon_grant::stage_text(rf, more, c.marked)?;
    crate::dungeon_packs::stage_text(rf, more, &crate::dungeon_packs::placed(c.d, c.seed))
}

fn key_ring_text(rf: &mut Romfs, c: &Ctx, more: &mut Vec<Out>) -> Res<()> {
    if c.p.rings || c.p.hideout_ring {
        crate::key_ring_text::stage(rf, c.work, more, &c.p.all_rings())?;
    }
    Ok(())
}

fn scrub_hints(rf: &mut Romfs, c: &Ctx, more: &mut Vec<Out>) -> Res<()> {
    crate::scrub_prices::stage_text(rf, c.d, c.seed, c.p.rewards.scrub_hint, more)
}

fn fairy_text(rf: &mut Romfs, c: &Ctx, more: &mut Vec<Out>) -> Res<()> {
    crate::fairy_prices::stage_text(rf, c.p.rewards.fairy_prices, c.seed, more)
}

fn silver_text(rf: &mut Romfs, c: &Ctx, more: &mut Vec<Out>) -> Res<()> {
    crate::slice::silver::stage_text(rf, more, c.silver)
}

fn pouch(rf: &mut Romfs, c: &Ctx, more: &mut Vec<Out>) -> Res<()> {
    more.push(crate::dungeon_credit::stage_pouch(rf, c.roam)?);
    Ok(())
}

fn bomb_lift(rf: &mut Romfs, _: &Ctx, more: &mut Vec<Out>) -> Res<()> {
    more.extend(crate::bomb_lift::stage(rf)?);
    Ok(())
}

fn quiet_pickups(rf: &mut Romfs, c: &Ctx, more: &mut Vec<Out>) -> Res<()> {
    more.extend(crate::bomb_pickup::stage_all(rf, c.quiet)?);
    Ok(())
}

fn door_fix(rf: &mut Romfs, c: &Ctx, more: &mut Vec<Out>) -> Res<()> {
    more.push(crate::door_gate::stage_flow(rf, &c.p.door)?);
    more.push(crate::door_gate::stage_river(rf)?);
    Ok(())
}

fn no_tutorials(rf: &mut Romfs, c: &Ctx, more: &mut Vec<Out>) -> Res<()> {
    more.push(crate::no_tutorials::stage(rf, c.p.rings || c.p.hideout_ring)?);
    Ok(())
}

fn door_open(rf: &mut Romfs, c: &Ctx, more: &mut Vec<Out>) -> Res<()> {
    if c.p.gates.door {
        more.extend(crate::door_open::stage_door(rf)?);
    } else {
        more.extend(crate::door_gate::stage_door(rf, &c.p.door)?);
    }
    if c.p.door.moat {
        crate::door_gate::stage_moat(rf, more, Some(c.work))?;
    } else {
        crate::door_gate::stage_escape_ocarina(rf, more, Some(c.work))?;
    }
    crate::door_gate::stage_escape(rf, more, Some(c.work), &c.p.door)?;
    Ok(())
}

fn bombchu(rf: &mut Romfs, c: &Ctx, more: &mut Vec<Out>) -> Res<()> {
    crate::bombchu_open::stage(rf, more, c.work)
}

fn water_gate(rf: &mut Romfs, c: &Ctx, more: &mut Vec<Out>) -> Res<()> {
    crate::water_gate::stage(rf, more, c.work)
}

fn pedestal(rf: &mut Romfs, c: &Ctx, more: &mut Vec<Out>) -> Res<()> {
    more.extend(crate::door_open::stage_pedestal(rf, Some(c.work))?);
    Ok(())
}

fn fire_gorons(rf: &mut Romfs, _: &Ctx, more: &mut Vec<Out>) -> Res<()> {
    more.extend(crate::fire_gorons::stage(rf)?);
    Ok(())
}

fn medal_gate(rf: &mut Romfs, c: &Ctx, more: &mut Vec<Out>) -> Res<()> {
    if c.p.rewards.dungeon_rewards {
        crate::medal_gate::stage(rf, more, Some(c.work))?;
    } else {
        if c.p.prelude {
            crate::prelude_gate::stage(rf, more)?;
        }
        if c.p.lacs.is_some() {
            crate::medal_gate::stage_lacs(rf, more, Some(c.work))?;
        }
    }
    if c.p.rainbow.is_some() {
        crate::rainbow_bridge::stage(rf, more)?;
    }
    Ok(())
}

fn start_reward(rf: &mut Romfs, c: &Ctx, more: &mut Vec<Out>) -> Res<()> {
    let reward: Option<&str> = crate::start_reward::pick(&c.seed.seed, c.p.start.reward).map(|(_, a)| a);
    let mut flags: Vec<(String, bool)> = c.p.start_flags();
    flags.extend(crate::no_tutorials::start_flags());
    let mut give: Vec<&str> = c.p.start.pouch.clone();
    if let Some(a) = reward {
        give.push(a);
    }
    if !give.is_empty() || !flags.is_empty() {
        more.push(crate::start_reward::stage_all(rf, &give, &flags)?);
    }
    Ok(())
}

fn start_items(rf: &mut Romfs, c: &Ctx, more: &mut Vec<Out>) -> Res<()> {
    if c.p.first_pickups {
        crate::start_items::stage::stage_list(rf, more, &crate::first_pickups::bag_sets(&c.p.kit))?;
    }
    crate::start_items::stage::stage(rf, more, &c.p.kit)
}

fn dpad(rf: &mut Romfs, c: &Ctx, more: &mut Vec<Out>) -> Res<()> {
    crate::start_items::dpad::stage(rf, more, &crate::start_items::dpad::actors(&[&c.p.kit], &c.p.start.pouch))?;
    Ok(())
}

fn pot_rows(rf: &mut Romfs, c: &Ctx, more: &mut Vec<Out>) -> Res<()> {
    if c.p.drops.rocks != crate::slice::support::Mode::Off {
        more.push(crate::pot_slice::kf_ring::stage(rf)?);
    }
    crate::pot_slice::rows::stage(rf, more, c.pots)
}

fn hideout_drop_rows(rf: &mut Romfs, c: &Ctx, more: &mut Vec<Out>) -> Res<()> {
    crate::hideout_keys::stage_rows(rf, more, c.p.hideout != Keep::Vanilla)
}

fn key_ring_flags(rf: &mut Romfs, c: &Ctx, more: &mut Vec<Out>) -> Res<()> {
    crate::key_ring_credit::stage_rows(rf, more, &c.p.all_rings())
}

fn start_defaults(rf: &mut Romfs, c: &Ctx, more: &mut Vec<Out>) -> Res<()> {
    let mut edits: Vec<crate::start_defaults::Edit> = crate::start_defaults::presets::chosen(&c.p.defaults())?;
    if let Some(n) = c.p.lacs {
        crate::medal_gate::override_lacs(&mut edits, n);
    }
    edits.extend(crate::gerudo_card::exps());
    edits.extend(crate::open_world::fortress::exps());
    edits.extend(crate::open_world::trade::edits());
    edits.extend(c.p.trials.edits());
    c.p.trials.stage(rf, more, c.work)?;
    if let Some(n) = c.p.rainbow {
        edits.push(crate::rainbow_bridge::exp(n));
    }
    edits.extend(c.p.door.exps());
    if c.p.nocturne && !c.p.rewards.dungeon_rewards {
        edits.extend(crate::nocturne_gate::exp());
    }
    if c.p.prelude && !c.p.rewards.dungeon_rewards {
        edits.push(crate::prelude_gate::exp());
    }
    if c.p.qol.fast_session {
        edits.extend(crate::qol::skull_kid_session::defaults());
    }
    edits.push(crate::always::exp());
    crate::start_defaults::stage(rf, more, &edits)
}

fn debug_names(rf: &mut Romfs, c: &Ctx, more: &mut Vec<Out>) -> Res<()> {
    if c.p.debug_names {
        crate::debug_names::stage(rf, c.d, c.work, more)?;
    }
    Ok(())
}

fn scrub_rows(rf: &mut Romfs, c: &Ctx, more: &mut Vec<Out>) -> Res<()> {
    if c.scrub_rows == 0 {
        crate::scrubs_b::stage_rows(rf, c.scrub_jobs, more, true)?;
    }
    Ok(())
}
