use crate::settings::Settings;
use crate::slice::support::show;

pub(crate) const UNCHANGED: [&str; 19] = [
    "logic_rules",
    "starting_age",
    "bridge",
    "bridge_stones",
    "bridge_medallions",
    "bridge_rewards",
    "bridge_tokens",
    "bridge_hearts",
    "trials",
    "triforce_hunt",
    "triforce_count_per_world",
    "triforce_goal_per_world",
    "starting_hearts",
    "starting_songs",
    "ganon_bosskey_stones",
    "ganon_bosskey_medallions",
    "ganon_bosskey_rewards",
    "ganon_bosskey_tokens",
    "ganon_bosskey_hearts",
];

pub(crate) const APPLIED: [&str; 6] = ["starting_hearts", "starting_songs", "bridge", "bridge_rewards", "bridge_medallions", "trials"];

pub fn not_yet(id: &str) -> bool {
    UNCHANGED.contains(&id) && !APPLIED.contains(&id)
}

const OFF_ONLY: [&str; 1] = ["shuffle_scrubs"];

pub fn check(s: &Settings, bad: &mut Vec<String>, noted: &mut Vec<String>) {
    let def: Settings = Settings::default();
    for id in UNCHANGED {
        match (s.get(id), def.get(id)) {
            (Some(a), Some(b)) if a == b => noted.push(format!("{id}={}", show(a))),
            (Some(_), _) if APPLIED.contains(&id) => {}
            (Some(a), _) => bad.push(format!(
                "{id} is {}, which this build cannot apply yet (only the default works)",
                show(a)
            )),
            _ => bad.push(format!("no setting named {id}")),
        }
    }
    for id in OFF_ONLY {
        match s.text(id) {
            Ok("off") => {}
            Ok(v) => bad.push(format!("{id} is {v}, which this build cannot shuffle yet (only off works)")),
            Err(e) => bad.push(e.to_string()),
        }
    }
    let hideout: &str = s.text("shuffle_hideoutkeys").unwrap_or("vanilla");
    match hideout {
        "vanilla" | "any_dungeon" | "keysanity" | "anywhere" => {}
        v => bad.push(format!("shuffle_hideoutkeys is {v}, which this build cannot place yet (only vanilla, any_dungeon and keysanity work)")),
    }
    if hideout != "vanilla"
        && !matches!(s.text("gerudo_fortress").unwrap_or("normal"), "fast" | "open")
    {
        bad.push(format!("shuffle_hideoutkeys is {hideout}, which needs gerudo_fortress to be fast or open"));
    }
}
