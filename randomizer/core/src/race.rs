//! Race seeds: seeds made to be played blind by everyone, the person who generated them too.
//!
//! - No spoiler log is made, and the report leaves out where items went.
//! - The seed file that is shared (and written beside the mod) is a race file: only what the
//!   seed was made from (the seed text, the settings string, the game and the generator) and the
//!   finished seed's checksum. Building it makes the seed again and checks that checksum, so
//!   everyone who builds it gets the same seed and nobody gets a file that lists the items.
//! - A race seed rolls with a seed text of its own ([`rolled`]): "race:" and a hash of the seed
//!   text. A seed made without race mode can not use a seed text that starts with "race:", so the
//!   same settings and seed text without race mode (where a spoiler log can be asked for) give a
//!   different seed, and no seed text gives the race seed.
//! - The title screen shows a row of the game's item icons picked from the checksum
//!   (crate::hash_art), so players can check that they all have the same seed.
//!
//! What this does not stop: the built mod's files hold the items (that is how the game gets
//! them), and anyone can change the program to print what it generates. It stops the spoiler
//! from being made, shown or shared by any build of the app as it ships.

use std::path::Path as P;
use std::sync::Mutex;

use crate::error::{Error as E, Res};
use crate::json::{self, Json};
use crate::settings::digest::sha256_hex;
use crate::settings::seed::{MAX_SEED_BYTES, SeedFile};
use crate::settings::{Loaded, Settings};
use crate::slice::{GenOpts, Generated};

pub const FORMAT: &str = "ootrr-race";
pub const FORMAT_VERSION: u32 = 1;
/// The start of every seed text a race seed rolls with.
pub const MARK: &str = "race:";
pub const RESERVED: &str = "seed texts that start with \"race:\" are kept for race seeds, pick another seed text";
pub const NO_PLANDO: &str = "a race seed cannot use plandomizer entries: everyone must be able to make the same seed from the race file, and hand-placed items would be known to whoever placed them";
pub const NO_SPOILER: &str = "Race seed: no spoiler log is made, for anyone.";

/// The seed text a race seed made from `seed` rolls with.
pub fn rolled(seed: &str) -> String {
    format!("{MARK}{}", &sha256_hex(format!("ootrr race seed\n{seed}").as_bytes())[..32])
}

/// Whether `seed` is a seed text only race seeds may roll with.
pub fn reserved(seed: &str) -> bool {
    seed.get(..MARK.len()).is_some_and(|h| h.eq_ignore_ascii_case(MARK))
}

/// Whether a full seed was made as a race seed.
pub fn is_race(sf: &SeedFile) -> bool {
    reserved(&sf.seed)
}

/// The generation options of every race seed. Building a race file makes the seed again with
/// these, so they must not change between the generation and the build.
pub fn opts() -> GenOpts {
    GenOpts { race: true, extras: true, ..GenOpts::default() }
}

/// The hash icons (crate::hash_art) a full seed shows on the title screen.
pub fn icons(full: &SeedFile) -> Vec<usize> {
    crate::hash_art::pick(&full.checksum())
}

pub fn icon_names(full: &SeedFile) -> Vec<&'static str> {
    crate::hash_art::names(&full.checksum())
}

fn bad(m: &str) -> E {
    E::Msg(format!("race file: {m}"))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RaceFile {
    /// The seed text as typed (or picked at random), before [`rolled`].
    pub seed: String,
    pub settings_string: String,
    pub game: String,
    pub generator: String,
    /// The checksum of the full seed this file makes.
    pub hash: String,
}

fn field<'a>(j: &'a Json, k: &str) -> Res<&'a str> {
    match j.get(k) {
        Some(Json::Str(s)) => Ok(s),
        _ => Err(bad(&format!("field {k} is missing or not text"))),
    }
}

impl RaceFile {
    /// The race file of `full`, a seed made with [`opts`] from the seed text `seed`.
    pub fn of(full: &SeedFile, seed: &str) -> Res<RaceFile> {
        if full.seed != rolled(seed) {
            return Err(bad("the seed was not made as a race seed from this seed text"));
        }
        Ok(RaceFile { seed: seed.to_string(), settings_string: full.settings_string.clone(), game: full.game.clone(), generator: full.generator.clone(), hash: full.checksum() })
    }

    fn sum(&self) -> String {
        sha256_hex(format!("{FORMAT}\nversion\t{FORMAT_VERSION}\nseed\t{}\nsettings\t{}\ngame\t{}\ngenerator\t{}\nhash\t{}\n", self.seed, self.settings_string, self.game, self.generator, self.hash).as_bytes())
    }

    pub fn to_json(&self) -> Json {
        Json::obj()
            .with("format", FORMAT.into())
            .with("version", Json::Int(FORMAT_VERSION as i64))
            .with("note", "A race seed: build it with the OOTRR app to play it. It holds only what the seed is made from, so it shows no items; no spoiler log exists for it.".into())
            .with("generator", self.generator.as_str().into())
            .with("seed", self.seed.as_str().into())
            .with("settings", self.settings_string.as_str().into())
            .with("game", self.game.as_str().into())
            .with("hash", self.hash.as_str().into())
            .with("checksum", self.sum().into())
    }

    pub fn to_text(&self) -> String {
        let mut t: String = self.to_json().pretty();
        t.push('\n');
        t
    }

    pub fn from_json(j: &Json) -> Res<RaceFile> {
        if field(j, "format")? != FORMAT {
            return Err(bad("this is not an ootrr race file"));
        }
        match j.get("version").and_then(|x| x.as_i64()) {
            Some(1) => {}
            Some(v) if v > FORMAT_VERSION as i64 => return Err(bad(&format!("format version {v} is newer than this build understands, update the app"))),
            _ => return Err(bad("the format version is not valid")),
        }
        let f: RaceFile = RaceFile {
            seed: field(j, "seed")?.to_string(),
            settings_string: field(j, "settings")?.to_string(),
            game: field(j, "game")?.to_string(),
            generator: field(j, "generator")?.to_string(),
            hash: field(j, "hash")?.to_string(),
        };
        if f.seed.is_empty() || f.seed.len() > MAX_SEED_BYTES || f.seed.chars().any(|c| c.is_control()) {
            return Err(bad("the seed text is empty, too long or has control characters"));
        }
        if f.hash.len() != 64 || !f.hash.chars().all(|c| c.is_ascii_hexdigit()) || f.generator.len() > 64 || f.game.len() > 128 {
            return Err(bad("a field is malformed"));
        }
        if field(j, "checksum")? != f.sum() {
            return Err(bad("the checksum does not match, the file was edited or damaged"));
        }
        Settings::decode(&f.settings_string).map_err(|e| bad(&format!("the settings string is not usable: {e}")))?;
        Ok(f)
    }

    pub fn parse(text: &str) -> Res<RaceFile> {
        RaceFile::from_json(&json::parse(text)?)
    }

    pub fn icon_names(&self) -> Vec<&'static str> {
        crate::hash_art::names(&self.hash)
    }

    pub fn icon_ids(&self) -> Vec<&'static str> {
        crate::hash_art::ids(&self.hash)
    }
}

/// Whether `text` is a race file (and not an ordinary seed file).
pub fn is_race_text(text: &str) -> bool {
    json::parse(text).ok().is_some_and(|j| j.get("format").and_then(|f| f.as_str()) == Some(FORMAT))
}

/// Full race seeds made or built lately, so a build right after generating does not make the
/// seed a second time. Only kept in memory.
static MADE: Mutex<Vec<SeedFile>> = Mutex::new(Vec::new());
const KEEP: usize = 4;

pub fn remember(full: &SeedFile) {
    let mut m = MADE.lock().unwrap_or_else(|e| e.into_inner());
    let sum: String = full.checksum();
    m.retain(|s| s.checksum() != sum);
    m.push(full.clone());
    if m.len() > KEEP {
        m.remove(0);
    }
}

fn known(rf: &RaceFile) -> Option<SeedFile> {
    let want: String = rolled(&rf.seed);
    MADE.lock().unwrap_or_else(|e| e.into_inner()).iter().find(|s| s.seed == want && s.settings_string == rf.settings_string && s.checksum() == rf.hash).cloned()
}

/// How many seed texts a race generation tries before it gives up on a seed the build would refuse.
const ROLLS: usize = 8;

/// Whether the build will take this full seed: the checks a build makes from the seed alone,
/// without the game files. A race file must build for everyone, and nobody can look at the
/// seed to see why it would not, so a seed that fails is rolled again instead of handed out.
pub fn buildable(d: &crate::slice::data::Data, full: &SeedFile) -> Res<()> {
    let (prepared, _) = crate::frost::prepare(full)?;
    // The same verdict the build reads (slice::seed_packs::build_hooked): any pickup spot out of
    // upgrade tier order stops the build.
    match crate::slice::tier_order::fixed_actors(d, &prepared)?.bad.first() {
        Some(b) => Err(E::Msg(format!("a pickup spot is out of upgrade tier order: {b}"))),
        None => Ok(()),
    }
}

/// Generates a race seed from the seed text `seed`, with `o` (its spoiler and plando are
/// ignored and refused). Returns the full seed (never to be shown or saved) and its race file.
/// When the build would refuse the seed, it rolls again with " 2", " 3", ... after the seed text,
/// and the race file names the seed text that was used.
pub fn generate(s: &Settings, seed: &str, o: &GenOpts) -> Res<(Generated, RaceFile)> {
    if !o.plando.is_empty() {
        return Err(E::Msg(NO_PLANDO.to_string()));
    }
    let base: GenOpts = opts();
    let o: GenOpts = GenOpts { spoiler: false, race: true, extras: base.extras, tries: base.tries, switch: base.switch, ..o.clone() };
    let d: crate::slice::data::Data = crate::slice::data::Data::load()?;
    let mut why: String = String::new();
    for n in 1..=ROLLS {
        let text: String = if n == 1 { seed.to_string() } else { format!("{seed} {n}") };
        if text.len() > MAX_SEED_BYTES {
            break;
        }
        let g: Generated = crate::slice::generate::generate_data(&d, s, &text, &o)?;
        if let Err(e) = buildable(&d, &g.seed_file) {
            why = e.to_string();
            continue;
        }
        let rf: RaceFile = RaceFile::of(&g.seed_file, &text)?;
        remember(&g.seed_file);
        return Ok((g, rf));
    }
    Err(E::Msg(format!("no race seed the build would take came out of this seed text, try another one ({why})")))
}

/// The full seed a race file makes: made again (or taken from memory) and checked against the
/// file's hash, so every player builds the same seed.
pub fn make(rf: &RaceFile) -> Res<SeedFile> {
    if let Some(s) = known(rf) {
        return Ok(s);
    }
    let l: Loaded = Settings::decode(&rf.settings_string)?;
    let g: Generated = crate::slice::generate_with(&l.settings, &rf.seed, &opts())?;
    if g.seed_file.checksum() != rf.hash {
        let ours: &str = &g.seed_file.generator;
        return Err(E::Msg(if rf.generator != *ours {
            format!("This race seed was made by {} and this app is {ours}, which makes a different seed from it. Everyone in the race needs the same version of the app.", rf.generator)
        } else {
            "This race seed does not come out the same here as where it was made, so it cannot be built. Everyone in the race needs the same version of the app.".to_string()
        }));
    }
    remember(&g.seed_file);
    Ok(g.seed_file)
}

/// A seed file or a race file, read for a build: the full seed and, for a race file, the file.
pub fn open(text: &str) -> Res<(SeedFile, Option<RaceFile>)> {
    if is_race_text(text) {
        let rf: RaceFile = RaceFile::parse(text)?;
        return Ok((make(&rf)?, Some(rf)));
    }
    Ok((SeedFile::parse(text)?, None))
}

/// The build report of a race seed: where the mod went and the hash, nothing about items.
pub fn build_report(root: &P, full: &SeedFile) -> String {
    format!(
        "Mod folder: {}\nRace seed: the build report and the spoiler log are hidden, so nobody sees where items are.\nSeed hash on the title screen: {}\n",
        root.display(),
        icon_names(full).join(", ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Value;

    fn small() -> Settings {
        let mut s: Settings = Settings::default();
        s.set("shuffle_mapcompass", Value::s("vanilla")).unwrap();
        s.finish().unwrap()
    }

    #[test]
    fn the_rolled_seed_is_reserved_and_no_typed_seed_is() {
        let r: String = rolled("abc");
        assert!(reserved(&r) && r.len() == MARK.len() + 32);
        assert_ne!(r, rolled("abd"));
        assert!(reserved("RACE:x") && !reserved("racer") && !reserved("") && !reserved(" race:x"));
    }

    #[test]
    fn a_race_seed_rolls_apart_from_the_same_seed_without_race_mode() {
        let s: Settings = small();
        let (g, rf) = generate(&s, "same text", &GenOpts::default()).unwrap();
        assert!(g.spoiler.is_none() && g.spheres.is_empty());
        assert!(is_race(&g.seed_file));
        let plain: Generated = crate::slice::generate_with(&s, "same text", &GenOpts { extras: true, spoiler: true, ..GenOpts::default() }).unwrap();
        assert_ne!(plain.seed_file.checksum(), rf.hash);
        assert_ne!(plain.seed_file.placements, g.seed_file.placements);
        let e: String = crate::slice::generate_with(&s, &g.seed_file.seed, &GenOpts { spoiler: true, ..GenOpts::default() }).unwrap_err().to_string();
        assert!(e.contains("kept for race seeds"), "{e}");
        assert!(!g.report.contains("Changed chests"), "{}", g.report);
        for loc in g.seed_file.placements.keys() {
            assert!(!g.report.contains(&format!("{loc}: ")), "{loc} is in the report");
        }
    }

    #[test]
    fn the_race_file_holds_no_items_and_builds_the_same_seed() {
        let s: Settings = small();
        let (g, rf) = generate(&s, "file text", &GenOpts { spoiler: true, ..GenOpts::default() }).unwrap();
        let t: String = rf.to_text();
        for (loc, item) in g.seed_file.placements.iter().take(50) {
            assert!(!t.contains(loc.as_str()) && !t.contains(&format!("\"{item}\"")), "{loc} {item}");
        }
        assert!(!t.contains(&g.seed_file.seed), "the rolled seed text is not in the file");
        let back: RaceFile = RaceFile::parse(&t).unwrap();
        assert_eq!(back, rf);
        MADE.lock().unwrap().clear();
        let (full, again) = open(&t).unwrap();
        assert_eq!(again, Some(rf.clone()));
        assert_eq!(full, g.seed_file, "made again from the race file alone");
        assert_eq!(icon_names(&full), rf.icon_names());
        assert!(SeedFile::parse(&t).is_err(), "a race file is not a seed file");
        let edited: String = t.replace("file text", "file texu");
        assert!(RaceFile::parse(&edited).unwrap_err().to_string().contains("checksum"));
        let mut wrong: RaceFile = rf.clone();
        wrong.hash = "0".repeat(64);
        wrong.generator = "ootrr-core 0.0.1".into();
        let e: String = make(&wrong).unwrap_err().to_string();
        assert!(e.contains("same version"), "{e}");
    }

    #[test]
    fn plando_is_refused_in_race_mode() {
        let o: GenOpts = GenOpts { plando: vec![crate::slice::plando::parse("KF Midos Top Left Chest=Boomerang").unwrap()], ..GenOpts::default() };
        assert!(generate(&small(), "p", &o).unwrap_err().to_string().contains("plandomizer"));
    }
}
