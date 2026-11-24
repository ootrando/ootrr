use crate::error::{Error as E, Res};
use crate::settings::{Settings, Value};
use crate::slice::dungeon_items as di;
use crate::slice::pickups as pu;
use crate::start_reward::Reward;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Off,
    Overworld,
    Dungeons,
    All,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Keep {
    Vanilla,
    Dungeon,
    AnyDungeon,
    Anywhere,
    Overworld,
    Regional,
    Start,
    /// Ganon's Boss Key only: given by the Light Arrows cutscene (crate::lacs_key).
    Lacs,
}

impl Keep {
    pub fn roams(self) -> bool {
        matches!(self, Keep::AnyDungeon | Keep::Anywhere | Keep::Overworld | Keep::Regional)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Forest {
    Closed,
    Open,
}

pub const MIDO_WALL: &str = "KokiriForest_Npc_Kokiri_Mido_Child_Blocker";
pub const EXIT_WALL: &str = "KokiriForest_Npc_Kokiri_007_Child_Blocker_00";
pub const DOOR_OPENED: &str = "Market_Child_TimeTempleDoor_Opened";
pub const KAKARIKO: (&str, &str) = ("open_kakariko", "open");
pub const DUNGEON_REWARDS: &str = "shuffle_dungeon_rewards";
pub const DUNGEON_REWARD_CAT: &str = "dungeon_reward";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Songs {
    Off,
    Own,
    Anywhere,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub own_strict: Vec<(&'static str, &'static str)>,
    pub chests: Mode,
    pub keys: di::Keys,
    pub rings: bool,
    pub ring_mode: crate::key_rings::Mode,
    pub ring_count: usize,
    pub active_rings: Vec<&'static crate::key_rings::Ring>,
    pub hideout_ring: bool,
    pub hideout: Keep,
    pub rewards: crate::flow_extra::Rewards,
    pub hearts: bool,
    pub world: pu::Free,
    pub songs: Songs,
    pub harp_colors: bool,
    pub hints: bool,
    pub fast: bool,
    pub chest_sizes: bool,
    pub intro: bool,
    pub start: crate::start_items::Start,
    pub gates: crate::open_world::Gates,
    pub excluded: Vec<String>,
    pub notes: Vec<String>,
    pub drops: crate::pot_slice::Drops,
    pub skulls: crate::slice::skulls::Mode,
    pub remake: bool,
    pub chus: bool,
    pub qol: crate::qol::Opts,
    pub treasure: bool,
    pub day_skulls: bool,
    pub kit: crate::start_items::Kit,
    pub open: crate::open_world::Opts,
    pub difficulty: crate::difficulty::Opts,
    pub npc: crate::npc_hints::Opts,
    /// crate::story_skip: Saria's gift, the child Temple of Time and the first sword draw.
    /// Always on; the field stays so the parts can be told apart in the build.
    pub story: bool,
    /// crate::story_skip dungeon endings.
    pub endings: bool,
    pub shops: crate::shops::Opts,
    pub bridge: bool,
    pub debug_names: bool,
    pub silver: bool,
    /// crate::medal_gate: Some(n) when the Light Arrows scene waits for n dungeon rewards.
    pub lacs: Option<crate::medal_gate::Goal>,
    /// crate::rainbow_bridge: Some(n) when the bridge waits for n dungeon rewards.
    pub rainbow: Option<crate::medal_gate::Goal>,
    pub trials: crate::ganon_trials::Trials,
    pub door: crate::door_gate::Opts,
    pub nocturne: bool,
    pub prelude: bool,
    /// Saria's Fairy Ocarina joins the pool (crate::slice::pick::OCARINAS).
    pub ocarinas: bool,
    /// The Kokiri Sword chest joins the pool (crate::slice::pick::SWORD).
    pub kokiri_sword: bool,
    pub short: bool,
    pub first_pickups: bool,
    pub area_intros: bool,
}

pub use crate::slice::unsupported::not_yet;
#[cfg(test)]
use crate::slice::unsupported::{APPLIED, UNCHANGED};

pub fn show(v: &Value) -> String {
    match v {
        Value::Bool(b) => b.to_string(),
        Value::Str(s) => s.clone(),
        Value::Int(n) => n.to_string(),
        Value::Set(x) => format!("[{}]", x.join(", ")),
    }
}

fn chest_mode(s: &Settings, bad: &mut Vec<String>) -> Mode {
    match s.text("shuffle_chests") {
        Ok("off") => Mode::Off,
        Ok("overworld") => Mode::Overworld,
        Ok("dungeons") => Mode::Dungeons,
        Ok("all") => Mode::All,
        Ok(v) => {
            bad.push(format!("shuffle_chests is {v}, which is not a known choice"));
            Mode::Off
        }
        Err(e) => {
            bad.push(e.to_string());
            Mode::Off
        }
    }
}

fn hideout_mode(s: &Settings, bad: &mut Vec<String>) -> Keep {
    match s.text("shuffle_hideoutkeys") {
        Ok("vanilla") => Keep::Vanilla,
        Ok("any_dungeon") => Keep::AnyDungeon,
        Ok("keysanity") | Ok("anywhere") => Keep::Anywhere,
        Ok(v) => {
            bad.push(format!("shuffle_hideoutkeys is {v}, which this build cannot place yet (only vanilla, any_dungeon and keysanity work)"));
            Keep::Vanilla
        }
        Err(e) => {
            bad.push(e.to_string());
            Keep::Vanilla
        }
    }
}

fn ring_mode(s: &Settings, bad: &mut Vec<String>) -> crate::key_rings::Mode {
    use crate::key_rings::Mode;
    match s.text(crate::key_rings::MODE_SETTING) {
        Ok("all") => Mode::All,
        Ok("count") => Mode::Count,
        Ok("selection") => Mode::Selection,
        Ok("random") => Mode::Random,
        Ok(v) => {
            bad.push(format!("{} is {v}, which is not a known choice", crate::key_rings::MODE_SETTING));
            Mode::All
        }
        Err(e) => {
            bad.push(e.to_string());
            Mode::All
        }
    }
}

fn ring_count(s: &Settings, bad: &mut Vec<String>) -> usize {
    match s.int(crate::key_rings::COUNT_SETTING) {
        Ok(n) if (1..=crate::key_rings::RINGS.len() as i64).contains(&n) => n as usize,
        Ok(n) => {
            bad.push(format!("{} is {n}, outside 1..=8", crate::key_rings::COUNT_SETTING));
            8
        }
        Err(e) => {
            bad.push(e.to_string());
            8
        }
    }
}

fn ring_selection(s: &Settings, bad: &mut Vec<String>) -> Vec<&'static crate::key_rings::Ring> {
    match s.items(crate::key_rings::SELECTION_SETTING) {
        Ok(ids) => crate::key_rings::selected(ids),
        Err(e) => {
            bad.push(e.to_string());
            Vec::new()
        }
    }
}

fn active_rings(
    s: &Settings,
    seed: Option<&str>,
    mode: crate::key_rings::Mode,
    count: &mut usize,
    bad: &mut Vec<String>,
) -> Vec<&'static crate::key_rings::Ring> {
    use crate::key_rings::Mode;
    match mode {
        Mode::All => crate::key_rings::all(),
        Mode::Selection => {
            let got: Vec<&'static crate::key_rings::Ring> = ring_selection(s, bad);
            if got.is_empty() {
                bad.push(format!("{} is empty; select at least one dungeon when key rings are on", crate::key_rings::SELECTION_SETTING));
            }
            got
        }
        Mode::Count => seed.map(|x| crate::key_rings::count(x, *count)).unwrap_or_default(),
        Mode::Random => match seed {
            Some(x) => {
                let (n, got): (usize, Vec<&'static crate::key_rings::Ring>) = crate::key_rings::random(x);
                *count = n;
                got
            }
            None => Vec::new(),
        },
    }
}

pub const STRICT_KEYS: [(&str, &str); 3] = [("fire_key_layout", "shuffled"), ("water_key_layout", "shuffled"), ("spirit_key_layout", "shuffled")];
pub const OWN_STRICT: [(&str, &str); 2] = [("water_key_layout", "shuffled"), ("spirit_key_layout", "shuffled")];
pub const FIRE_LOOP: (&str, &str) = ("fire_loop_door", "open");
pub const FIRE_LOOP_FLAG: &str = "IsChainLockDoorOpen.4189300454150217955";
pub const FIRE_SPARE_KEY: &str = "Fire Temple Highest Goron Chest";
pub const OWN_KEYS: [(&str, &str); 3] = [OWN_STRICT[0], OWN_STRICT[1], FIRE_LOOP];

impl Plan {


    pub fn all_rings(&self) -> Vec<&'static crate::key_rings::Ring> {
        let mut out: Vec<&'static crate::key_rings::Ring> = if self.rings {
            if self.active_rings.is_empty() {
                crate::key_rings::all()
            } else {
                self.active_rings.clone()
            }
        } else {
            Vec::new()
        };
        if self.hideout_ring {
            out.push(&crate::key_rings::HIDEOUT_RING);
        }
        out
    }

    pub fn keys_roam(&self) -> bool {
        self.keys.small.roams()
    }

    pub fn fire_loop_open(&self) -> bool {
        self.keys.small == Keep::Dungeon
    }

    pub fn strict_layouts(&self) -> &[(&'static str, &'static str)] {
        match self.keys.small {
            Keep::Vanilla => &[],
            Keep::Dungeon => &self.own_strict,
            _ => &STRICT_KEYS,
        }
    }

    pub fn start_flags(&self) -> Vec<(String, bool)> {
        let mut out: Vec<(String, bool)> = match self.gates.forest {
            Forest::Closed => Vec::new(),
            Forest::Open => vec![(MIDO_WALL.to_string(), false), (EXIT_WALL.to_string(), false)],
        };
        if self.gates.door {
            out.push((DOOR_OPENED.to_string(), true));
        }
        out.extend(di::start_flags(self));
        if self.fire_loop_open() {
            out.push((FIRE_LOOP_FLAG.to_string(), true));
        }
        if self.first_pickups {
            out.extend(crate::first_pickups::start_flags());
        }
        if self.area_intros {
            out.extend(crate::area_intros::start_flags());
        }
        out
    }

    pub fn logic(&self) -> Vec<(&'static str, &'static str)> {
        let mut out: Vec<(&'static str, &'static str)> = match self.gates.forest {
            Forest::Closed => Vec::new(),
            Forest::Open => vec![("open_forest", "open")],
        };
        if self.gates.door {
            out.push(("open_door_of_time", "open"));
        }
        out.extend(self.strict_layouts().iter().copied());
        if self.fire_loop_open() {
            out.push(FIRE_LOOP);
        }
        if self.rings {
            out.push((crate::key_rings::SETTING, "true"));
            for r in &self.active_rings {
                out.push((r.select, "true"));
            }
        }
        if self.hideout_ring {
            out.push((crate::key_rings::HIDEOUT_SETTING, "true"));
        }
        if self.hideout != Keep::Vanilla {
            let mode: &str = match self.hideout {
                Keep::AnyDungeon => "any_dungeon",
                Keep::Anywhere => "keysanity",
                _ => "any_dungeon",
            };
            out.push(("shuffle_hideoutkeys", mode));
        }
        if self.chus {
            out.push((crate::slice::bombchus::LOGIC_KEY, "true"));
        }
        if self.gates.kakariko {
            out.push(KAKARIKO);
            out.push(("skip_child_zelda", "true"));
        }
        out.extend(self.qol.logic());
        out.extend(self.open.logic());
        out.extend(crate::open_world::trade::LOGIC);
        if self.rewards.trade_items {
            out.push(crate::open_world::trade_shuffle::LOGIC);
        }
        out.extend(self.trials.logic());
        if self.skulls != crate::slice::skulls::Mode::Off {
            out.push((crate::slice::skull_rules::KEY, crate::slice::skull_rules::ON));
        }
        if self.drops.wild() {
            out.push((crate::pot_slice::rocks::KEY, crate::pot_slice::rocks::ON));
        }
        if self.drops.remake != Mode::Off {
            out.push((crate::pot_slice::remake::KEY, crate::pot_slice::remake::ON));
        }
        if self.world.remake() {
            out.push((crate::spots::free::KEY, crate::spots::free::ON));
        }
        if self.shops.on() {
            out.push((crate::shops::door::LOGIC_KEY, "true"));
        }
        if self.rewards.dungeon_rewards {
            out.push((crate::medal_gate::LOGIC_KEY, "true"));
        }
        if self.prelude {
            out.push((crate::prelude_gate::LOGIC_KEY, "true"));
        }
        if let Some(n) = self.lacs {
            out.extend(crate::medal_gate::lacs_goal_logic(n));
        }
        if let Some(n) = self.rainbow {
            out.extend(crate::rainbow_bridge::logic(n));
        }
        out.extend(self.door.logic());
        if self.rewards.potion {
            out.push((crate::flow_extra::potion::KEY, crate::flow_extra::potion::ON));
        }
        if self.day_skulls {
            out.push((crate::slice::skull_day::KEY, crate::slice::skull_day::ON));
        }
        out
    }

    pub fn defaults(&self) -> Vec<(&'static str, &'static str)> {
        let mut out: Vec<(&'static str, &'static str)> = if self.gates.kakariko { vec![KAKARIKO] } else { Vec::new() };
        out.extend(self.open.defaults());
        if self.rewards.dungeon_rewards {
            out.push((crate::medal_gate::SETTING, "true"));
        }
        out
    }

    pub fn opening_step(&self) -> Option<&'static str> {
        match self.gates.forest {
            Forest::Open => Some(crate::start_defaults::opening::OPEN_STEP),
            _ => None,
        }
    }
}

pub(crate) fn toggle(s: &Settings, key: &str, bad: &mut Vec<String>) -> bool {
    s.flag(key).unwrap_or_else(|e| {
        bad.push(e.to_string());
        false
    })
}

pub fn plan(s: &Settings) -> Res<Plan> {
    plan_with_seed(s, None)
}



pub fn plan_with_seed(s: &Settings, seed: Option<&str>) -> Res<Plan> {
    let mut bad: Vec<String> = Vec::new();
    let mut noted: Vec<String> = Vec::new();
    crate::slice::unsupported::check(s, &mut bad, &mut noted);
    let chests: Mode = chest_mode(s, &mut bad);
    let hideout: Keep = hideout_mode(s, &mut bad);
    if hideout != Keep::Vanilla && matches!(chests, Mode::Off | Mode::Overworld) {
        bad.push("shuffle_hideoutkeys needs chest shuffle to be Dungeons only or Everywhere".to_string());
    }
    let world: pu::Free = pu::mode(s, &mut bad);
    let drops: crate::pot_slice::Drops = crate::pot_slice::plan::read(s, &mut bad);
    let keys: di::Keys = di::Keys::read(s, &mut bad);
    let rings: bool = toggle(s, crate::key_rings::SETTING, &mut bad);
    if rings && keys.small == Keep::Vanilla {
        bad.push("key_rings is on while shuffle_smallkeys is vanilla; key rings only work when small keys are shuffled".to_string());
    }
    let ring_mode: crate::key_rings::Mode = ring_mode(s, &mut bad);
    let mut ring_count: usize = ring_count(s, &mut bad);
    let active_rings: Vec<&'static crate::key_rings::Ring> = if rings {
        active_rings(s, seed, ring_mode, &mut ring_count, &mut bad)
    } else {
        Vec::new()
    };
    let hideout_ring: bool = toggle(s, crate::key_rings::HIDEOUT_SETTING, &mut bad);
    di::chest_guard(s, chests, [keys.small, keys.boss, keys.ganon, keys.map], &mut bad);
    let mut rewards: crate::flow_extra::Rewards = crate::flow_extra::Rewards::read(s, &mut bad);
    let hearts: bool = toggle(s, "shuffle_boss_hearts", &mut bad);
    let hints: bool = toggle(s, "gossip_hints", &mut bad);
    let fast: bool = toggle(s, "faster_chests", &mut bad);
    let intro: bool = toggle(s, "skip_intro", &mut bad);
    let start: crate::start_items::Start = crate::start_items::Start::read(s, &mut bad);
    let gates: crate::open_world::Gates = crate::open_world::Gates::read(s, &mut bad);
    let songs: Songs = crate::songs::mode(s, &mut bad);
    let harp_colors: bool = toggle(s, crate::song_models::SETTING, &mut bad);
    let chosen: Vec<String> = s.items("starting_items").map(|v| v.to_vec()).unwrap_or_default();
    let mut kit: crate::start_items::Kit = crate::start_items::read(s, &mut bad);
    // Settings that would hand out the same check twice.
    // The open gate needs the forest open: its story state skips the Deku Tree quest past Mido.
    // A closed forest closes the gate too: the schema locks the gate to closed (the page greys it
    // out and a settings string decodes it as closed); a value set directly is refused here, so no
    // such seed is made.
    if gates.kakariko && gates.forest != Forest::Open {
        bad.push(format!("open_kakariko is open while open_forest is {}, which cannot go together; set Kokiri Forest to Open or the Kakariko Gate to Closed", s.text("open_forest").unwrap_or("closed")));
    }
    if gates.kakariko && chosen.iter().any(|i| i == "zeldas_letter") {
        bad.push("starting_items includes zeldas_letter while open_kakariko is open, which already hands out that check at new game; drop one of them".to_string());
    }
    if gates.kakariko && s.items("starting_songs").is_ok_and(|v| v.iter().any(|x| x == "zeldas_lullaby")) {
        bad.push("starting_songs includes zeldas_lullaby while open_kakariko is open, which already hands out that check at new game; drop one of them".to_string());
    }
    let skulls: crate::slice::skulls::Mode = crate::slice::skulls::mode(s, &mut bad);
    let remake: bool = chests != Mode::Off;
    let chus: bool = crate::slice::bombchus::mode(s, &mut bad);
    let chest_sizes: bool = crate::chest_size::mode(s, &mut bad);
    let qol: crate::qol::Opts = crate::qol::read(s, &mut bad);
    let npc: crate::npc_hints::Opts = crate::npc_hints::read(s, gates.kakariko, &mut bad);
    let treasure: bool = crate::slice::treasure::mode(s, &mut bad);
    let day_skulls: bool = toggle(s, crate::slice::skull_day::SETTING, &mut bad);
    let open: crate::open_world::Opts = crate::open_world::read(s, &mut bad);
    let trials: crate::ganon_trials::Trials = crate::ganon_trials::read(s, &mut bad);
    let difficulty: crate::difficulty::Opts = crate::difficulty::read(s, &mut bad);
    if hideout != Keep::Vanilla && open.fortress != crate::open_world::Fortress::Open {
        bad.push("shuffle_hideoutkeys needs gerudo_fortress=open; the four native drops are only rewired for the already-cleared fortress".to_string());
    }
    if hideout_ring && !s.text("shuffle_hideoutkeys").is_ok_and(|v| v == "keysanity") {
        bad.push("hideout_key_rings needs shuffle_hideoutkeys=keysanity; the Hideout ring replaces the four shuffled Hideout keys".to_string());
    }
    if hideout_ring && open.fortress != crate::open_world::Fortress::Open {
        bad.push("hideout_key_rings needs gerudo_fortress=open, like shuffled Hideout keys".to_string());
    }
    if open.fortress == crate::open_world::Fortress::Open {
        crate::start_items::give::add(&mut kit, "gerudo_card");
    }
    rewards.read_merchants(s, open.beans, &mut bad);
    // Always on in this randomizer, like the other cutscene skips (no setting).
    let story: bool = true;
    let endings: bool = true;
    let shops: crate::shops::Opts = crate::shops::plan::read(s, &mut bad);
    let bridge: bool = crate::deku_bridge::mode(s, &mut bad);
    let debug_names: bool = toggle(s, crate::debug_names::SETTING, &mut bad);
    let silver: bool = crate::slice::silver::mode(s, chests, &mut bad);
    let lacs: Option<crate::medal_gate::Goal> = crate::medal_gate::read_lacs(s, keys.ganon, &mut bad);
    let rainbow: Option<crate::medal_gate::Goal> = crate::rainbow_bridge::read(s, &mut bad);
    let door: crate::door_gate::Opts = crate::door_gate::read(s, gates.door, &mut bad);
    let nocturne: bool = crate::nocturne_gate::read(s, &mut bad);
    let prelude: bool = crate::prelude_gate::read(s, &mut bad);
    let short: bool = true;
    let first_pickups: bool = s.flag(crate::first_pickups::SETTING).unwrap_or_else(|e| {
        bad.push(e.to_string());
        false
    });
    let area_intros: bool = s.flag(crate::area_intros::SETTING).unwrap_or_else(|e| {
        bad.push(e.to_string());
        false
    });
    if lacs.is_some() && (!rewards.on || matches!(chests, Mode::Off | Mode::Overworld)) {
        bad.push(crate::lacs_key::NEEDS.to_string());
    }
    let ocarinas: bool = toggle(s, crate::slice::pick::OCARINAS, &mut bad);
    let kokiri_sword: bool = toggle(s, crate::slice::pick::SWORD, &mut bad);
    let excluded: Vec<String> = s.items("disabled_locations").map(|v| v.to_vec()).unwrap_or_default();
    if !bad.is_empty() {
        let lines: Vec<String> = bad.iter().map(|b| format!("  {b}")).collect();
        return Err(E::Msg(format!(
            "This build cannot apply these settings yet:\n{}",
            lines.join("\n")
        )));
    }
    let mut notes: Vec<String> = Vec::new();
    if !noted.is_empty() {
        notes.push(format!(
            "Not applied by this build, the game plays as unmodified: {}",
            noted.join(", ")
        ));
    }
    for (on, text) in [
        (hearts, "Boss Heart Containers are shuffled. Each becomes a chest."),
        (songs != Songs::Off, "Songs are shuffled. The song lessons are edited."),
        (world.on(), world.note().unwrap_or_default()),
        (fast, "Chest and item pickup scenes are sped up."),
        (true, crate::fire_gorons::NOTE),
        (true, crate::open_world::trade::NOTE),
        (intro, "The new game intro is skipped. The opening movie and the wake up scene are bypassed, and the harmless first-visit scenes of the areas are marked as played."),
        (start.reward != Reward::Off, "You start the game with one random Stone or Medallion."),
        (!start.pouch.is_empty(), "The ticked Kokiri Sword, Deku Shield or Slingshot is added to the pouch when a new game starts. A ticked Big or Biggest Bullet Bag brings the Slingshot with it."),
        (keys.small.roams(), "Small keys can be found in other dungeons or outside them. A chest key is counted for the dungeon it belongs to and the chest says which."),
        (rings, crate::key_rings::NOTE),
        (hideout != Keep::Vanilla, "Thieves' Hideout Small Keys are shuffled. The four native key drops hold shuffled non-key items."),
        (hideout_ring, "The four Thieves' Hideout keys become one manually enabled Hideout key ring."),
        (gates.forest != Forest::Closed, "Kokiri Forest starts open. The blocker walls are switched off when a new game starts."),
        (gates.kakariko, "The Kakariko gate starts open and the castle visit is skipped. A new game sets the castle quest complete, the Door of Time quest past Impa's send off, the Fire Stone quest past the gate and the gate guard's flag, and gives the items of Zelda's Letter and the Song from Impa."),
        (gates.door, "The Door of Time starts open. The Temple of Time logic is edited so the door is always open and the Master Sword can be drawn without the Door of Time quest, and the door's opened flag is set when a new game starts, so the Ocarina and the Song of Time are not needed to reach the Master Sword. The quest itself is left alone, so Zelda's escape and the Song of Time stay checks, and the adult pedestal works before the Forest Sage so you can return to childhood at any time."),
        (gates.forest == Forest::Open, "The Deku Tree quest starts at the step Mido normally sets when he steps aside (Meet_DekuTree Step03), so Mido stands aside and the Kokiri Shop shows its later stock."),
        (gates.forest == Forest::Open, crate::story_skip::NOTE_DEKU),
        (story, crate::story_skip::NOTE),
        (endings, crate::story_skip::NOTE_ENDINGS),
        (hints, "Gossip stones give hints. Their text is replaced in every language."),
        (rewards.extra, "Kak 20 Gold Skulltula Reward, DMT Biggoron and GC Medigoron are shuffled with structural event edits."),
        (rewards.trade && !rewards.trade_items, "The ten adult trade quest hand ins each give one extra shuffled item the first time. The trade items themselves stay in their chain."),
        (rewards.trade_items, crate::open_world::trade_shuffle::NOTE),
        (rewards.cows, crate::flow_extra::cows::NOTE),
        (rewards.cows_other, crate::flow_extra::cows::NOTE_OTHER),
        (rewards.frog_songs, crate::flow_extra::frogs::SONGS_NOTE),
        (rewards.frog_bug, crate::flow_extra::frogs::BUG_NOTE),
        (rewards.potion, crate::flow_extra::potion::NOTE),
        (day_skulls, crate::slice::skull_day::NOTE),
    ] {
        if on {
            notes.push(text.to_string());
        }
    }
    if rings {
        let extra: String = crate::key_rings::mode_note(ring_mode, ring_count, &active_rings);
        if !extra.is_empty() {
            notes.push(extra);
        }
    }
    if gates.kakariko {
        notes.push(crate::start_defaults::presets::note(KAKARIKO.0, KAKARIKO.1)?);
    }
    notes.extend(di::notes(keys.boss, keys.ganon, keys.map));
    notes.extend(drops.notes());
    if skulls != crate::slice::skulls::Mode::Off {
        notes.push(crate::slice::skulls::NOTE.to_string());
    }
    if world.on() || drops.any() || skulls != crate::slice::skulls::Mode::Off {
        notes.push(crate::slice::key_spots::NOTE.to_string());
    }
    if drops.pots != Mode::Off {
        notes.push(crate::slice::key_spots::LOST.to_string());
    }
    if rewards.scrubs {
        notes.push(crate::scrubs_a::NOTE.to_string());
    }
    if rewards.scrubs_b {
        notes.push(crate::scrubs_b::NOTE.to_string());
    }
    if remake {
        notes.push(crate::slice::remake_chests::NOTE.to_string());
    }

    if chus {
        notes.push(crate::slice::bombchus::NOTE.to_string());
    }
    if chest_sizes {
        notes.push(crate::chest_size::NOTE.to_string());
    }
    notes.extend(qol.notes());
    notes.extend(crate::npc_hints::note(&npc));
    notes.extend(kit.notes.iter().cloned());
    if treasure {
        notes.push(crate::slice::treasure::NOTE.to_string());
    }
    notes.extend(open.notes()?);
    notes.extend(difficulty.notes());
    if bridge {
        notes.push(crate::deku_bridge::NOTE.to_string());
    }
    if debug_names {
        notes.push(crate::debug_names::NOTE.to_string());
    }
    notes.extend(shops.notes());
    if silver {
        notes.push(crate::slice::silver::NOTE.to_string());
    }
    if rainbow.is_some() {
        notes.push(crate::rainbow_bridge::NOTE.to_string());
    }
    notes.extend(door.notes());
    notes.push(crate::bombchu_open::NOTE.to_string());
    notes.push(crate::water_gate::NOTE.to_string());
    if short {
        notes.push(crate::short_get::NOTE.to_string());
    }
    if first_pickups {
        notes.push(crate::first_pickups::NOTE.to_string());
    }
    if area_intros {
        notes.push(crate::area_intros::NOTE.to_string());
    }
    if lacs.is_some() {
        notes.push(crate::lacs_key::NOTE.to_string());
        notes.push(crate::medal_gate::LACS_NOTE.to_string());
    }
    let own_strict: Vec<(&'static str, &'static str)> = if matches!(chests, Mode::Dungeons | Mode::All) { OWN_STRICT.to_vec() } else { Vec::new() };
    Ok(Plan { own_strict, chests, keys, rings, ring_mode, ring_count, active_rings, hideout_ring, hideout, rewards, hearts, world, songs, harp_colors, hints, fast, chest_sizes, intro, start, gates, excluded, notes, drops, skulls, remake, chus, qol, treasure, day_skulls, kit, open, npc, shops, bridge, debug_names, silver, lacs, rainbow, trials, door, nocturne, prelude, short, first_pickups, area_intros, story, endings, ocarinas, kokiri_sword, difficulty })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_accepted_and_noted() {
        let p: Plan = plan(&Settings::default()).unwrap();
        assert_eq!(p.chests, Mode::All);
        assert_eq!(p.keys.small, Keep::Dungeon);
        assert_eq!(p.keys.map, Keep::Dungeon);
        assert!(p.intro);
        assert!(p.story && p.endings);
        assert_eq!(p.notes.len(), 24);
        assert!(p.notes.iter().any(|n| n == crate::qol::gear_cycle::NOTE), "the gear cycle is always on");
        assert!(p.short && p.notes.iter().any(|n| n == crate::short_get::NOTE));
        assert!(!p.notes.iter().any(|n| n == crate::deku_bridge::NOTE));
        assert!(p.remake && p.hearts && p.rewards.extra);
        assert_eq!(p.world, crate::slice::pickups::Free::Pieces);
        assert!(p.notes.iter().any(|n| n.contains("world clock runs from the start")));
        assert_eq!(p.open, crate::open_world::Opts { time: true, epona: true, ..crate::open_world::Opts::default() });
        assert!(!p.gates.door);
        // The default bridge (vanilla) plays as the unmodified game, so it is listed as not applied.
        assert!(p.notes.iter().any(|n| n.contains("bridge=vanilla")));
        assert_eq!(p.rainbow, None);
        assert!(p.notes.iter().any(|n| n.contains("intro is skipped")));
        assert!(!p.notes.iter().any(|n| n.contains("Door of Time starts open")));
        assert!(!p.notes.iter().any(|n| n.contains("Kakariko gate starts open")));
    }

    #[test]
    fn the_first_pickup_setting_is_off_by_default_and_writes_the_start_flags_when_on() {
        let off: Plan = plan(&Settings::default()).unwrap();
        assert!(!off.first_pickups);
        assert!(!off.notes.iter().any(|n| n == crate::first_pickups::NOTE));
        assert!(!off.start_flags().iter().any(|(n, _)| n.starts_with(crate::first_pickups::PREFIX)));
        let mut s: Settings = Settings::default();
        s.set(crate::first_pickups::SETTING, Value::Bool(true)).unwrap();
        let on: Plan = plan(&s).unwrap();
        assert!(on.first_pickups && on.notes.iter().any(|n| n == crate::first_pickups::NOTE));
        let f: Vec<(String, bool)> = on.start_flags();
        for x in crate::first_pickups::start_flags() {
            assert!(f.contains(&x), "{x:?}");
        }
        assert_eq!(f.len(), off.start_flags().len() + crate::first_pickups::PICKUPS.len());
    }

    #[test]
    fn the_area_intro_setting_is_off_by_default_and_writes_the_start_flags_when_on() {
        let off: Plan = plan(&Settings::default()).unwrap();
        assert!(!off.area_intros);
        assert!(!off.notes.iter().any(|n| n == crate::area_intros::NOTE));
        assert!(!off.start_flags().iter().any(|(n, _)| n.starts_with(crate::area_intros::VISIT) || n.contains("Goto_")));
        let mut s: Settings = Settings::default();
        s.set(crate::area_intros::SETTING, Value::Bool(true)).unwrap();
        let on: Plan = plan(&s).unwrap();
        assert!(on.area_intros && on.notes.iter().any(|n| n == crate::area_intros::NOTE));
        let f: Vec<(String, bool)> = on.start_flags();
        let want: Vec<(String, bool)> = crate::area_intros::start_flags();
        for x in &want {
            assert!(f.contains(x), "{x:?}");
        }
        assert_eq!(f.len(), off.start_flags().len() + want.len());
    }

    #[test]
    fn the_kakariko_setting_becomes_logic_values_and_a_note() {
        let mut s: Settings = Settings::default();
        s.set("open_door_of_time", Value::Bool(false)).unwrap();
        s.set("open_forest", Value::s("open")).unwrap();
        s.set("open_kakariko", Value::s("open")).unwrap();
        let p: Plan = plan(&s).unwrap();
        assert!(p.gates.kakariko);
        assert_eq!(p.logic(), [&[("open_forest", "open")][..], &OWN_KEYS, &[("open_kakariko", "open"), ("skip_child_zelda", "true"), (crate::open_world::EPONA, "1"), crate::open_world::trade::LOGIC[0]]].concat());
        assert_eq!(p.start_flags(), vec![(MIDO_WALL.to_string(), false), (EXIT_WALL.to_string(), false), (FIRE_LOOP_FLAG.to_string(), true)]);
        s.set("open_kakariko", Value::s("closed")).unwrap();
        let p: Plan = plan(&s).unwrap();
        assert!(!p.gates.kakariko);
        assert_eq!(p.logic(), [&[("open_forest", "open")][..], &OWN_KEYS, &[(crate::open_world::EPONA, "1"), crate::open_world::trade::LOGIC[0]]].concat());
        assert!(!p.notes.iter().any(|n| n.contains("Kakariko")));
    }

    #[test]
    fn vanilla_keys_and_chest_modes_are_honored() {
        let mut s: Settings = Settings::default();
        s.set("shuffle_smallkeys", Value::s("vanilla")).unwrap();
        s.set("shuffle_chests", Value::s("dungeons")).unwrap();
        let p: Plan = plan(&s).unwrap();
        assert_eq!(p.keys.small, Keep::Vanilla);
        assert_eq!(p.chests, Mode::Dungeons);
    }

    #[test]
    fn key_rings_need_shuffled_small_keys_and_reach_the_logic() {
        let mut s: Settings = Settings::default();
        s.set("shuffle_smallkeys", Value::s("vanilla")).unwrap();
        s.set(crate::key_rings::SETTING, Value::Bool(true)).unwrap();
        let e: String = plan(&s).err().unwrap().to_string();
        assert!(e.contains("key_rings is on while shuffle_smallkeys is vanilla"), "{e}");

        s.set("shuffle_smallkeys", Value::s("keysanity")).unwrap();
        let p: Plan = plan(&s).unwrap();
        assert!(p.rings);
        assert!(p.notes.iter().any(|n| n == crate::key_rings::NOTE));
        assert!(p.logic().contains(&(crate::key_rings::SETTING, "true")));
    }

    #[test]
    fn ring_modes_derive_the_active_standard_dungeons_from_settings_and_seed() {
        use crate::key_rings::{self, Mode};

        let mut s: Settings = Settings::default();
        s.set("shuffle_smallkeys", Value::s("keysanity")).unwrap();
        s.set(key_rings::SETTING, Value::Bool(true)).unwrap();

        let all: Plan = plan_with_seed(&s, Some("seed")).unwrap();
        assert_eq!(all.ring_mode, Mode::All);
        assert_eq!(all.active_rings, key_rings::all());
        assert_eq!(all.logic().iter().filter(|(k, _)| k.starts_with("key_ring_")).count(), 8);

        s.set(key_rings::MODE_SETTING, Value::s("selection")).unwrap();
        s.set(key_rings::SELECTION_SETTING, Value::set(&["ganons_castle", "forest_temple"])).unwrap();
        let picked: Plan = plan_with_seed(&s, Some("seed")).unwrap();
        assert_eq!(picked.active_rings.iter().map(|r| r.choice).collect::<Vec<_>>(), ["forest_temple", "ganons_castle"]);
        assert!(picked.logic().contains(&("key_ring_forest_temple", "true")));
        assert!(!picked.logic().contains(&("key_ring_fire_temple", "true")));
        s.set(key_rings::SELECTION_SETTING, Value::set(&[])).unwrap();
        let e: String = plan_with_seed(&s, Some("seed")).err().unwrap().to_string();
        assert!(e.contains("key_rings_selection is empty"), "{e}");

        s.set(key_rings::MODE_SETTING, Value::s("count")).unwrap();
        s.set(key_rings::SELECTION_SETTING, Value::set(&key_rings::CHOICE_IDS)).unwrap();
        s.set(key_rings::COUNT_SETTING, Value::Int(3)).unwrap();
        let a: Plan = plan_with_seed(&s, Some("seed")).unwrap();
        let b: Plan = plan_with_seed(&s, Some("seed")).unwrap();
        assert_eq!(a.active_rings.len(), 3);
        assert_eq!(a.ring_count, 3);
        assert_eq!(a.active_rings, b.active_rings);
        assert!(a.notes.iter().any(|n| n.contains("3 seed-randomized standard dungeons")));

        s.set(key_rings::MODE_SETTING, Value::s("random")).unwrap();
        let r: Plan = plan_with_seed(&s, Some("seed")).unwrap();
        assert!((1..=8).contains(&r.ring_count));
        assert_eq!(r.active_rings.len(), r.ring_count);
        assert_eq!(r.active_rings, plan_with_seed(&s, Some("seed")).unwrap().active_rings);
        assert!(r.active_rings.iter().all(|x| key_rings::by_choice(x.choice).is_some()));
    }

    #[test]
    fn hideout_ring_is_a_separate_gated_toggle() {
        use crate::key_rings;

        let mut s: Settings = Settings::default();
        s.set("shuffle_smallkeys", Value::s("keysanity")).unwrap();
        s.set("gerudo_fortress", Value::s("open")).unwrap();
        s.set("shuffle_hideoutkeys", Value::s("keysanity")).unwrap();
        s.set(key_rings::HIDEOUT_SETTING, Value::Bool(true)).unwrap();
        let p: Plan = plan_with_seed(&s, Some("seed")).unwrap();
        assert!(p.hideout_ring);
        assert!(!p.rings);
        assert!(p.active_rings.is_empty());
        assert_eq!(p.all_rings().iter().map(|r| r.item).collect::<Vec<_>>(), ["Small Key Ring (Thieves Hideout)"]);
        assert!(p.logic().contains(&("hideout_key_rings", "true")));
        assert!(p.logic().contains(&("shuffle_hideoutkeys", "keysanity")));
        assert!(p.notes.iter().any(|n| n.contains("one manually enabled Hideout key ring")));

        for bad in ["vanilla", "regional", "overworld", "any_dungeon"] {
            s.set("shuffle_hideoutkeys", Value::s(bad)).unwrap();
            let e: String = plan_with_seed(&s, Some("seed")).err().unwrap().to_string();
            assert!(e.contains("hideout_key_rings needs shuffle_hideoutkeys=keysanity"), "{e}");
        }

        s.set("shuffle_hideoutkeys", Value::s("keysanity")).unwrap();
        s.set("gerudo_fortress", Value::s("normal")).unwrap();
        let e: String = plan_with_seed(&s, Some("seed")).err().unwrap().to_string();
        assert!(e.contains("hideout_key_rings needs gerudo_fortress=open"), "{e}");
    }

    #[test]
    fn every_setting_flagged_as_not_yet_is_refused_at_a_non_default_value() {
        let sch: &crate::settings::Schema = crate::settings::schema();
        let ids: Vec<&str> = UNCHANGED.iter().copied().filter(|i| not_yet(i)).collect();
        assert_eq!(ids.len(), UNCHANGED.len() - APPLIED.len());
        assert!(!not_yet("bridge_medallions") && not_yet("ganon_bosskey_medallions"));
        assert!(APPLIED.iter().all(|i| !not_yet(i)));
        assert!(!not_yet("shuffle_chests"));
        for id in ids {
            let st: &crate::settings::Setting = sch.get(id).unwrap_or_else(|| panic!("no setting {id}"));
            let alt: Value = match &st.default {
                Value::Bool(b) => Value::Bool(!b),
                Value::Str(d) => Value::s(&st.choices.iter().find(|c| &c.id != d).unwrap().id),
                Value::Int(n) => Value::Int(if *n < st.max { n + 1 } else { n - 1 }),
                Value::Set(_) => panic!("{id} is a set"),
            };
            let mut s: Settings = Settings::default();
            s.set(id, alt).unwrap();
            let e: String = plan(&s).err().unwrap_or_else(|| panic!("{id} was accepted")).to_string();
            assert!(e.contains(&format!("{id} is")), "{id}: {e}");
        }
    }

    #[test]
    fn unsupported_values_are_named_not_ignored() {
        let mut s: Settings = Settings::default();
        s.set("shuffle_scrubs", Value::s("low")).unwrap();
        s.set("shuffle_smallkeys", Value::s("remove")).unwrap();
        s.set("bridge", Value::s("open")).unwrap();
        s.set("starting_hearts", Value::Int(5)).unwrap();
        let e: String = plan(&s).err().unwrap().to_string();
        assert!(e.contains("shuffle_scrubs is low"));
        assert!(e.contains("shuffle_smallkeys is remove"));
        assert!(e.contains("bridge is open"));
        assert!(!e.contains("starting_hearts"));
    }

    #[test]
    fn small_keys_can_roam_and_the_logic_is_told_to_count_them_strictly() {
        let mut s: Settings = Settings::default();
        s.set("open_door_of_time", Value::Bool(false)).unwrap();
        s.set("open_kakariko", Value::s("closed")).unwrap();
        s.set("open_forest", Value::s("closed")).unwrap();
        assert!(!plan(&s).unwrap().keys_roam());
        assert_eq!(plan(&s).unwrap().logic(), [&OWN_KEYS[..], &[(crate::open_world::EPONA, "1"), crate::open_world::trade::LOGIC[0]]].concat());
        for v in ["any_dungeon", "keysanity"] {
            s.set("shuffle_smallkeys", Value::s(v)).unwrap();
            let p: Plan = plan(&s).unwrap();
            assert!(p.keys_roam());
            assert_eq!(p.logic().len(), 5);
            assert!(p.logic().iter().filter(|(k, _)| *k != crate::open_world::EPONA && *k != crate::open_world::trade::LOGIC[0].0).all(|(k, v)| k.ends_with("_key_layout") && *v == "shuffled"));
            assert!(p.notes.iter().any(|n| n.contains("Small keys can be found")));
        }
        s.set("shuffle_smallkeys", Value::s("vanilla")).unwrap();
        assert!(plan(&s).unwrap().strict_layouts().is_empty());
        assert!(!plan(&s).unwrap().logic().iter().any(|(k, _)| k.ends_with("_key_layout")));
        s.set("shuffle_smallkeys", Value::s("overworld")).unwrap();
        assert!(plan(&s).is_err());
        s.set("shuffle_smallkeys", Value::s("dungeon")).unwrap();
        s.set("shuffle_ganon_bosskey", Value::s("medallions")).unwrap();
        assert!(plan(&s).err().unwrap().to_string().contains("shuffle_ganon_bosskey is medallions"));
    }

    #[test]
    fn the_door_of_time_setting_becomes_a_start_flag_and_a_logic_value() {
        let mut s: Settings = Settings::default();
        s.set("open_kakariko", Value::s("closed")).unwrap();
        s.set("open_forest", Value::s("closed")).unwrap();
        assert!(!plan(&s).unwrap().gates.door);
        s.set("open_door_of_time", Value::Bool(true)).unwrap();
        let p: Plan = plan(&s).unwrap();
        assert_eq!(p.start_flags(), vec![(DOOR_OPENED.to_string(), true), (FIRE_LOOP_FLAG.to_string(), true)]);
        assert_eq!(p.logic(), [&[("open_door_of_time", "open")][..], &OWN_KEYS, &[(crate::open_world::EPONA, "1"), crate::open_world::trade::LOGIC[0], ("door_of_time_escape", "moat")]].concat());
        assert!(p.notes.iter().any(|n| n.contains("Door of Time starts open")));
        s.set("open_door_of_time", Value::Bool(false)).unwrap();
        let p: Plan = plan(&s).unwrap();
        assert_eq!(p.start_flags(), vec![(FIRE_LOOP_FLAG.to_string(), true)]);
        assert_eq!(p.logic(), [&OWN_KEYS[..], &[(crate::open_world::EPONA, "1"), crate::open_world::trade::LOGIC[0]]].concat());
        assert!(!p.notes.iter().any(|n| n.contains("Door of Time starts open")));
    }

    #[test]
    fn an_open_kakariko_gate_needs_an_open_forest() {
        let mut s: Settings = Settings::default();
        s.set("open_forest", Value::s("open")).unwrap();
        s.set("open_kakariko", Value::s("open")).unwrap();
        assert!(plan(&s).unwrap().gates.kakariko);
        s.set("open_forest", Value::s("closed")).unwrap();
        // A seed is never made from both.
        let e: String = plan(&s).err().unwrap().to_string();
        assert!(e.contains("open_kakariko is open while open_forest is closed"), "{e}");
        // A closed forest locks the gate closed: the page greys it out and a string reads closed.
        assert!(!s.is_enabled("open_kakariko"));
        let back: Settings = Settings::decode(&s.encode().unwrap()).unwrap().settings;
        assert_eq!(back.get("open_kakariko"), Some(&Value::s("closed")));
        assert!(!plan(&back).unwrap().gates.kakariko);
    }

    #[test]
    fn an_open_forest_with_a_closed_kakariko_gate_is_accepted() {
        let mut s: Settings = Settings::default();
        s.set("open_forest", Value::s("open")).unwrap();
        assert_eq!(s.text("open_kakariko").unwrap(), "closed");
        let p: Plan = plan(&s).unwrap();
        assert!(!p.gates.kakariko && p.gates.forest == Forest::Open);
    }

    #[test]
    fn starting_with_zeldas_letter_next_to_an_open_kakariko_is_refused() {
        let mut s: Settings = Settings::default();
        s.set("open_forest", Value::s("open")).unwrap();
        s.set("open_kakariko", Value::s("open")).unwrap();
        s.set("starting_items", Value::set(&["zeldas_letter"])).unwrap();
        let e: String = plan(&s).unwrap_err().to_string();
        assert!(e.contains("zeldas_letter while open_kakariko"), "{e}");
    }

    #[test]
    fn the_forest_setting_becomes_start_flags_and_a_logic_value() {
        let mut s: Settings = Settings::default();
        s.set("open_door_of_time", Value::Bool(false)).unwrap();
        s.set("open_kakariko", Value::s("closed")).unwrap();
        s.set("open_forest", Value::s("closed")).unwrap();
        assert_eq!(plan(&s).unwrap().start_flags(), vec![(FIRE_LOOP_FLAG.to_string(), true)]);
        s.set("open_forest", Value::s("open")).unwrap();
        let p: Plan = plan(&s).unwrap();
        assert_eq!(p.start_flags().len(), 3);
        assert_eq!(p.logic(), [&[("open_forest", "open")][..], &OWN_KEYS, &[(crate::open_world::EPONA, "1"), crate::open_world::trade::LOGIC[0]]].concat());
        assert!(p.notes.iter().any(|n| n.contains("Kokiri Forest starts open")));
    }

    #[test]
    fn the_kakariko_gate_is_vanilla_by_default_and_open_becomes_a_default_edit_and_a_logic_value() {
        let mut s: Settings = Settings::default();
        assert_eq!(s.text("open_kakariko").unwrap(), "closed");
        let p: Plan = plan(&s).unwrap();
        assert!(!p.gates.kakariko && p.defaults() == vec![(crate::open_world::TIME, "true")] && p.logic() == [&OWN_KEYS[..], &[(crate::open_world::EPONA, "1"), crate::open_world::trade::LOGIC[0]]].concat());
        assert!(!p.notes.iter().any(|n| n.contains("open_kakariko")));
        s.set("open_forest", Value::s("open")).unwrap();
        s.set("open_kakariko", Value::s("open")).unwrap();
        let p: Plan = plan(&s).unwrap();
        assert!(p.gates.kakariko);
        assert_eq!(p.defaults(), vec![KAKARIKO, (crate::open_world::TIME, "true"), ("open_forest", "open")]);
        assert_eq!(p.logic(), [&[("open_forest", "open")][..], &OWN_KEYS, &[KAKARIKO, ("skip_child_zelda", "true"), (crate::open_world::EPONA, "1"), crate::open_world::trade::LOGIC[0]]].concat());
        assert!(p.notes.iter().any(|n| n.contains("Death Mountain starts open") && n.contains("Mask Shop")));
        assert_eq!(p.start_flags(), vec![(MIDO_WALL.to_string(), false), (EXIT_WALL.to_string(), false), (FIRE_LOOP_FLAG.to_string(), true)]);
        s.set("open_kakariko", Value::s("closed")).unwrap();
        assert!(!plan(&s).unwrap().gates.kakariko);
    }

    #[test]
    fn only_an_open_forest_moves_the_opening_quest_step() {
        let mut s: Settings = Settings::default();
        assert_eq!(plan(&s).unwrap().opening_step(), None);
        s.set("open_forest", Value::s("open")).unwrap();
        let p: Plan = plan(&s).unwrap();
        assert_eq!(p.opening_step(), Some("Step03"));
        assert!(p.notes.iter().any(|n| n.contains("Meet_DekuTree Step03")));
    }

    #[test]
    fn starting_sword_shield_and_slingshot_are_accepted() {
        let mut s: Settings = Settings::default();
        s.set("starting_items", Value::set(&["slingshot", "kokiri_sword", "deku_shield"])).unwrap();
        let p: Plan = plan(&s).unwrap();
        assert_eq!(p.start.gear, vec!["Slingshot", "Kokiri Sword", "Deku Shield"]);
        assert_eq!(p.start.pouch, vec!["Weapon_Range_SlingShot", "Weapon_Sword_Kokiri", "Weapon_Shield_Deku"]);
        s.set("starting_items", Value::set(&["ocarina"])).unwrap();
        let p: Plan = plan(&s).unwrap();
        assert!(p.start.gear.is_empty() && p.kit.logic() == vec![("Ocarina", 1)]);
        s.set("starting_items", Value::set(&["zeldas_letter"])).unwrap();
        assert!(plan(&s).err().unwrap().to_string().contains("zeldas_letter"));
    }

    #[test]
    fn a_bullet_bag_start_tier_brings_the_slingshot_once() {
        let mut s: Settings = Settings::default();
        for ids in [vec!["big_bullet_bag"], vec!["biggest_bullet_bag"], vec!["big_bullet_bag", "slingshot"], vec!["slingshot", "biggest_bullet_bag", "big_bullet_bag"]] {
            s.set("starting_items", Value::set(&ids)).unwrap();
            let p: Plan = plan(&s).unwrap();
            assert_eq!(p.start.gear, vec!["Slingshot"], "{ids:?}");
            assert_eq!(p.start.pouch, vec!["Weapon_Range_SlingShot"], "{ids:?}");
        }
        s.set("starting_items", Value::set(&["big_quiver", "big_bomb_bag", "bomb_bag"])).unwrap();
        let p: Plan = plan(&s).unwrap();
        assert!(p.start.gear.is_empty() && p.start.pouch.is_empty());
    }

    #[test]
    fn starting_hearts_and_songs_are_applied_and_only_noted_at_their_defaults() {
        let d: Plan = plan(&Settings::default()).unwrap();
        assert!(d.notes[0].contains("starting_hearts=3") && d.notes[0].contains("starting_songs=[]"));
        assert_eq!(d.kit, crate::start_items::Kit::default());
        let mut s: Settings = Settings::default();
        s.set("starting_hearts", Value::Int(6)).unwrap();
        s.set("starting_songs", Value::set(&["sarias_song"])).unwrap();
        let p: Plan = plan(&s).unwrap();
        assert!(!p.notes[0].contains("starting_hearts") && !p.notes[0].contains("starting_songs"));
        assert_eq!(p.kit.hearts, Some(6));
        assert_eq!(p.kit.logic(), vec![("Sarias Song", 1)]);
    }

    #[test]
    fn excluded_locations_pass_through() {
        let mut s: Settings = Settings::default();
        s.set("disabled_locations", Value::set(&["ZD Chest"])).unwrap();
        assert_eq!(plan(&s).unwrap().excluded, vec!["ZD Chest".to_string()]);
    }

    const ROAMING: [(&str, &str, &[&str]); 4] = [
        ("shuffle_smallkeys", "Small Keys", &["any_dungeon", "keysanity"]),
        ("shuffle_bosskeys", "Boss Keys", &["regional", "overworld", "any_dungeon", "keysanity"]),
        ("shuffle_ganon_bosskey", "Ganon's Boss Key", &["regional", "overworld", "any_dungeon", "keysanity"]),
        ("shuffle_mapcompass", "Maps and Compasses", &["regional", "any_dungeon", "keysanity"]),
    ];

    #[test]
    fn roaming_dungeon_items_need_the_dungeon_chests_shuffled() {
        for (id, label, values) in ROAMING {
            for v in values {
                for chests in ["off", "overworld", "dungeons", "all"] {
                    let mut s: Settings = Settings::default();
                    s.set(id, Value::s(v)).unwrap();
                    s.set("shuffle_chests", Value::s(chests)).unwrap();
                    let conflict: bool = match chests {
                        "off" | "overworld" => true,
                        "dungeons" => *v == "overworld",
                        _ => false,
                    };
                    let r: Res<Plan> = plan(&s);
                    assert_eq!(r.is_err(), conflict, "{id}={v} chests={chests}");
                    if conflict {
                        let e: String = r.err().unwrap().to_string();
                        assert!(e.contains(&format!("{id} is {v} while shuffle_chests is {chests}")), "{e}");
                        assert!(e.contains(&format!("set {label} to ")) && e.contains("Set Chests to "), "{e}");
                        assert!(!e.contains(';') && !e.contains('\u{2014}'), "{e}");
                    }
                }
            }
        }
    }

    #[test]
    fn small_keys_that_roam_over_unshuffled_chests_name_the_unsatisfiable_counts() {
        let mut s: Settings = Settings::default();
        s.set("shuffle_smallkeys", Value::s("keysanity")).unwrap();
        s.set("shuffle_chests", Value::s("overworld")).unwrap();
        let e: String = plan(&s).err().unwrap().to_string();
        assert!(e.contains("the small key chests stay as they are while the logic counts every key as roaming"), "{e}");
        s.set("shuffle_smallkeys", Value::s("dungeon")).unwrap();
        s.set("shuffle_bosskeys", Value::s("keysanity")).unwrap();
        let e: String = plan(&s).err().unwrap().to_string();
        assert!(e.contains("nothing of this kind can roam") && !e.contains("small key chests"), "{e}");
    }

    #[test]
    fn every_conflicting_class_is_listed_on_its_own_line() {
        let mut s: Settings = Settings::default();
        s.set("shuffle_chests", Value::s("off")).unwrap();
        for (id, _, values) in ROAMING {
            s.set(id, Value::s(values[values.len() - 1])).unwrap();
        }
        let e: String = plan(&s).err().unwrap().to_string();
        assert_eq!(e.lines().filter(|l| l.contains("while shuffle_chests is off")).count(), 4, "{e}");
    }

    #[test]
    fn items_that_do_not_roam_never_conflict_with_any_chest_mode() {
        for chests in ["off", "overworld", "dungeons", "all"] {
            for (id, _, _) in ROAMING {
                for v in ["vanilla", "dungeon"] {
                    let mut s: Settings = Settings::default();
                    s.set(id, Value::s(v)).unwrap();
                    s.set("shuffle_chests", Value::s(chests)).unwrap();
                    assert!(plan(&s).is_ok(), "{id}={v} chests={chests}");
                }
            }
            let mut s: Settings = Settings::default();
            for (id, v) in [("shuffle_bosskeys", "remove"), ("shuffle_ganon_bosskey", "remove"), ("shuffle_mapcompass", "startwith")] {
                s.set(id, Value::s(v)).unwrap();
            }
            s.set("shuffle_chests", Value::s(chests)).unwrap();
            assert!(plan(&s).is_ok(), "removed items chests={chests}");
        }
    }

    #[test]
    fn no_builtin_preset_trips_the_chest_guard_and_the_default_and_keysanity_still_plan() {
        assert!(plan(&Settings::default()).is_ok());
        for pr in crate::settings::presets::builtin().unwrap() {
            let s: Settings = pr.apply(&Settings::default()).unwrap();
            match plan(&s) {
                Ok(_) => {}
                Err(e) => assert!(!e.to_string().contains("while shuffle_chests"), "{}: {e}", pr.id),
            }
            if pr.id == "keysanity" {
                assert!(plan(&s).is_ok());
            }
        }
    }

    #[test]
    fn every_builtin_preset_generates_a_seed() {
        let mut bad: Vec<String> = Vec::new();
        for pr in crate::settings::presets::builtin().unwrap() {
            let s: Settings = pr.apply(&Settings::default()).unwrap();
            if let Err(e) = crate::slice::generate::generate(&s, "preset check") {
                bad.push(format!("{}: {e}", pr.id));
            }
        }
        assert!(bad.is_empty(), "{}", bad.join("\n"));
    }
}
