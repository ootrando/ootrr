use std::sync::OnceLock;

use crate::error::{Error, Res};
use crate::json::Json;

pub mod check;
mod codec;
pub mod digest;
pub mod export;
pub mod list;
pub mod menu;
pub mod model;
pub mod presets;
pub mod seed;
pub mod state;
pub mod string;

pub use model::{Cond, Kind, Schema, Setting, Tag, Value};
pub use state::Vals;

static SCHEMA: OnceLock<Schema> = OnceLock::new();

pub fn schema() -> &'static Schema {
    SCHEMA.get_or_init(list::build)
}

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    vals: Vals,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Loaded {
    pub settings: Settings,
    pub warnings: Vec<String>,
}

impl Default for Settings {
    fn default() -> Settings {
        let mut vals: Vals = schema().defaults();
        schema().normalize(&mut vals);
        Settings { vals }
    }
}

fn missing(id: &str) -> Error {
    Error::Msg(format!("no setting named {id}"))
}

fn mismatch(id: &str, want: &str) -> Error {
    Error::Msg(format!("setting {id} is not a {want}"))
}

impl Settings {
    pub fn values(&self) -> &Vals {
        &self.vals
    }

    pub fn get(&self, id: &str) -> Option<&Value> {
        self.vals.get(id)
    }

    pub fn set(&mut self, id: &str, v: Value) -> Res<()> {
        let s: &Setting = schema().get(id).ok_or_else(|| missing(id))?;
        s.check(&v)?;
        self.vals.insert(id.to_string(), v);
        Ok(())
    }

    pub fn flag(&self, id: &str) -> Res<bool> {
        match self.get(id) {
            Some(Value::Bool(b)) => Ok(*b),
            Some(_) => Err(mismatch(id, "toggle")),
            None => Err(missing(id)),
        }
    }

    pub fn text(&self, id: &str) -> Res<&str> {
        match self.get(id) {
            Some(Value::Str(s)) => Ok(s),
            Some(_) => Err(mismatch(id, "choice")),
            None => Err(missing(id)),
        }
    }

    pub fn int(&self, id: &str) -> Res<i64> {
        match self.get(id) {
            Some(Value::Int(n)) => Ok(*n),
            Some(_) => Err(mismatch(id, "number")),
            None => Err(missing(id)),
        }
    }

    pub fn items(&self, id: &str) -> Res<&[String]> {
        match self.get(id) {
            Some(Value::Set(v)) => Ok(v),
            Some(_) => Err(mismatch(id, "multi-select or list")),
            None => Err(missing(id)),
        }
    }

    pub fn is_enabled(&self, id: &str) -> bool {
        schema().get(id).is_some_and(|s| schema().enabled(s, &self.vals))
    }

    pub fn normalize(&mut self) {
        schema().normalize(&mut self.vals);
    }

    pub fn validate(&self) -> Res<()> {
        schema().validate(&self.vals)
    }

    pub fn finish(mut self) -> Res<Settings> {
        self.normalize();
        self.validate()?;
        Ok(self)
    }

    pub fn to_json(&self) -> Json {
        schema().to_json(&self.vals)
    }

    pub fn from_json(j: &Json) -> Res<Loaded> {
        let (vals, warnings) = schema().from_json(j)?;
        Ok(Loaded { settings: Settings { vals }, warnings })
    }

    pub fn encode(&self) -> Res<String> {
        schema().encode(&self.vals)
    }

    pub fn decode(text: &str) -> Res<Loaded> {
        let (vals, warnings) = schema().decode(text)?;
        Ok(Loaded { settings: Settings { vals }, warnings })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json;

    #[test]
    fn a_stale_legacy_value_never_changes_what_the_page_sends() {
        let sch: &Schema = schema();
        let legacy: Vec<&Setting> = sch.settings.iter().filter(|s| s.legacy).collect();
        assert!(legacy.iter().any(|s| s.id == crate::open_world::DOOR));
        let stale = |s: &Setting| -> Json {
            match (&s.kind, &s.default) {
                (Kind::Toggle, Value::Bool(b)) => Json::Bool(!b),
                _ => s.choices.iter().find(|c| Value::s(&c.id) != s.default).map(|c| Json::from(c.id.as_str())).expect("a legacy setting needs a non-default value"),
            }
        };
        let mut checked: usize = 0;
        for s in sch.settings.iter().filter(|s| !s.legacy && s.kind == Kind::Choice) {
            for c in &s.choices {
                let mut v: Vals = sch.defaults();
                v.insert(s.id.clone(), Value::s(&c.id));
                let page: Json = sch.to_json(&v);
                let want: Vals = sch.from_json(&page).unwrap().0;
                for l in &legacy {
                    let mut j: Json = page.clone();
                    j.put(&l.id, stale(l));
                    assert_eq!(sch.from_json(&j).unwrap().0, want, "{} = {} with a stale {}", s.id, c.id, l.id);
                    checked += 1;
                }
            }
        }
        assert!(checked > 0);
    }

    #[test]
    fn the_shipped_schema_is_consistent() {
        let p: Vec<String> = schema().problems();
        assert!(p.is_empty(), "{p:#?}");
        assert!(schema().settings.len() >= 30);
    }

    #[test]
    fn every_dependency_names_a_real_setting() {
        for s in &schema().settings {
            for d in &s.deps {
                assert!(schema().get(&d.on).is_some(), "{} depends on missing {}", s.id, d.on);
            }
            if let Some(b) = &s.max_by {
                assert!(schema().get(b).is_some(), "{} max_by missing {b}", s.id);
            }
            assert!(schema().groups.iter().any(|g| g.id == s.group));
        }
    }

    #[test]
    fn every_default_is_valid_and_stable_under_normalize() {
        for s in &schema().settings {
            assert!(s.check(&s.default).is_ok(), "{}", s.id);
            if let Some(d) = &s.disabled {
                assert!(s.check(d).is_ok(), "{}", s.id);
            }
        }
        let d: Settings = Settings::default();
        assert!(d.validate().is_ok());
        let mut n: Settings = d.clone();
        n.normalize();
        assert_eq!(n, d);
    }

    #[test]
    fn heart_pool_choice_is_enabled_and_round_trips_only_for_nondefault_hearts() {
        let mut s = Settings::default();
        assert!(!s.is_enabled("heart_pool"));
        s.set("starting_hearts", Value::Int(1)).unwrap();
        s.set("heart_pool", Value::s("pieces")).unwrap();
        assert!(s.is_enabled("heart_pool"));
        let decoded = Settings::decode(&s.encode().unwrap()).unwrap().settings;
        assert_eq!(decoded.text("heart_pool").unwrap(), "pieces");
        s.set("starting_hearts", Value::Int(3)).unwrap();
        let decoded = Settings::decode(&s.encode().unwrap()).unwrap().settings;
        assert_eq!(decoded.text("heart_pool").unwrap(), "random");
    }

    #[test]
    fn nums_and_choice_order_are_pinned() {
        let want: [(&str, u16, &[&str]); 4] = [
            ("shuffle_chests", 8, &["off", "overworld", "dungeons", "all"]),
            ("shuffle_smallkeys", 14, &["remove", "vanilla", "dungeon", "regional", "overworld", "any_dungeon", "keysanity"]),
            ("bridge", 24, &["open", "vanilla", "stones", "medallions", "dungeons", "tokens", "hearts", "random"]),
            ("triforce_goal_per_world", 34, &[]),
        ];
        for (id, num, ch) in want {
            let s: &Setting = schema().get(id).unwrap();
            assert_eq!(s.num, num, "{id}");
            let got: Vec<&str> = s.choices.iter().map(|c| c.id.as_str()).collect();
            assert_eq!(got, ch, "{id}");
        }
    }

    #[test]
    fn every_kind_is_used_and_every_tag_is_used() {
        for k in [Kind::Toggle, Kind::Choice, Kind::Number, Kind::Multi, Kind::List] {
            assert!(schema().settings.iter().any(|s| s.kind == k), "{k:?}");
        }
        for t in [Tag::Core, Tag::Common, Tag::Niche] {
            assert!(schema().settings.iter().any(|s| s.tag == t), "{t:?}");
        }
    }

    #[test]
    fn dependencies_switch_settings_off() {
        let mut s: Settings = Settings::default();
        // The bridge defaults to vanilla, which counts nothing.
        assert!(!s.is_enabled("bridge_stones"));
        assert!(!s.is_enabled("bridge_medallions"));
        s.set("bridge", Value::s("stones")).unwrap();
        s.set("bridge_stones", Value::Int(2)).unwrap();
        s.set("bridge_medallions", Value::Int(4)).unwrap();
        s.normalize();
        assert!(s.is_enabled("bridge_stones"));
        assert_eq!(s.int("bridge_stones").unwrap(), 2);
        assert_eq!(s.int("bridge_medallions").unwrap(), 6);
    }

    #[test]
    fn typed_getters_and_errors() {
        let mut s: Settings = Settings::default();
        assert_eq!(s.text("shuffle_chests").unwrap(), "all");
        assert!(!s.flag("triforce_hunt").unwrap());
        assert_eq!(s.int("starting_hearts").unwrap(), 3);
        assert!(s.items("starting_items").unwrap().is_empty());
        assert!(s.flag("shuffle_chests").is_err());
        assert!(s.text("nope").is_err());
        assert!(s.set("shuffle_chests", Value::s("sometimes")).is_err());
        assert!(s.set("starting_hearts", Value::Int(21)).is_err());
        assert!(s.set("nope", Value::Bool(true)).is_err());
    }

    #[test]
    fn cross_field_limit_is_enforced() {
        let mut s: Settings = Settings::default();
        s.set("triforce_hunt", Value::Bool(true)).unwrap();
        s.set("triforce_count_per_world", Value::Int(10)).unwrap();
        assert!(s.clone().finish().is_err());
        s.set("triforce_goal_per_world", Value::Int(10)).unwrap();
        assert!(s.finish().is_ok());
    }

    #[test]
    fn json_round_trip_and_unknown_keys() {
        let mut s: Settings = Settings::default();
        s.set("bridge", Value::s("tokens")).unwrap();
        s.set("starting_songs", Value::set(&["song_of_time", "sarias_song"])).unwrap();
        let s: Settings = s.finish().unwrap();
        let l: Loaded = Settings::from_json(&s.to_json()).unwrap();
        assert_eq!(l.settings, s);
        assert!(l.warnings.is_empty());
        let j: Json = json::parse(r#"{"bridge":"open","removed_in_a_future_version":1}"#).unwrap();
        let l: Loaded = Settings::from_json(&j).unwrap();
        assert_eq!(l.settings.text("bridge").unwrap(), "open");
        assert_eq!(l.warnings.len(), 1);
    }

    #[test]
    fn string_round_trip_for_a_busy_configuration() {
        let mut s: Settings = Settings::default();
        s.set("shuffle_chests", Value::s("dungeons")).unwrap();
        s.set("shuffle_pots", Value::s("all")).unwrap();
        s.set("shuffle_smallkeys", Value::s("keysanity")).unwrap();
        s.set("shuffle_ganon_bosskey", Value::s("hearts")).unwrap();
        s.set("ganon_bosskey_hearts", Value::Int(12)).unwrap();
        s.set("bridge", Value::s("stones")).unwrap();
        s.set("bridge_stones", Value::Int(1)).unwrap();
        s.set("triforce_hunt", Value::Bool(true)).unwrap();
        s.set("triforce_count_per_world", Value::Int(40)).unwrap();
        s.set("triforce_goal_per_world", Value::Int(25)).unwrap();
        s.set("starting_items", Value::set(&["hookshot", "ocarina", "bottle"])).unwrap();
        s.set("starting_hearts", Value::Int(5)).unwrap();
        s.set("disabled_locations", Value::set(&["Skulltula House 10 Tokens", "ZR Magic Bean Salesman"])).unwrap();
        let s: Settings = s.finish().unwrap();
        let t: String = s.encode().unwrap();
        let l: Loaded = Settings::decode(&t).unwrap();
        assert_eq!(l.settings, s);
        assert!(l.warnings.is_empty());
        assert_eq!(l.settings.encode().unwrap(), t);
    }

    #[test]
    fn frozen_settings_string_keeps_decoding() {
        let l: Loaded = Settings::decode("OOTRR1-BEAQGGABAUOACZBEAMBAABWEOQ").unwrap();
        assert!(l.warnings.is_empty());
        let s: Settings = l.settings;
        assert_eq!(s.text("bridge").unwrap(), "tokens");
        assert_eq!(s.int("bridge_tokens").unwrap(), 50);
        assert_eq!(s.text("shuffle_pots").unwrap(), "all");
        assert_eq!(s.items("starting_items").unwrap(), ["ocarina", "hookshot"]);
        assert_eq!(s.text("shuffle_chests").unwrap(), "all");
        assert_eq!(s.encode().unwrap(), "OOTRR1-BEAQGGABAUOACZBEAMBAABWEOQ");
    }

    #[test]
    fn default_string_is_tiny_and_stable() {
        let t: String = Settings::default().encode().unwrap();
        assert_eq!(t, Settings::default().encode().unwrap());
        // The defaults that changed after strings were first shared (boss Heart Containers,
        // More NPC rewards, Return with Navi, Ocarina and Kokiri Sword shuffle) are written out, so
        // old strings keep their meaning.
        assert!(t.len() < 48, "{t}");
        assert_eq!(Settings::decode(&t).unwrap().settings, Settings::default());
    }

    #[test]
    fn rewards_are_on_by_default_and_stones_and_medallions_are_not() {
        let s: Settings = Settings::default();
        assert_eq!(s.text("shuffle_rewards").unwrap(), "supported");
        assert_eq!(s.text("shuffle_dungeon_rewards").unwrap(), "vanilla");
        assert!(s.is_enabled("shuffle_dungeon_rewards"));
        let d: &Setting = schema().get("shuffle_dungeon_rewards").unwrap();
        assert_eq!(d.num, 74);
        assert_eq!(d.kind, Kind::Choice);
        // The old toggle wrote 0 and 1: those indexes still read as vanilla and anywhere.
        assert_eq!(d.choice_index("vanilla"), Some(0));
        assert_eq!(d.choice_index("anywhere"), Some(1));
        assert_eq!(d.choice_index("dungeons"), Some(2));
        assert_eq!(d.group, "shuffle");
        assert_eq!(d.deps, vec![model::Dep { on: "shuffle_rewards".into(), cond: Cond::Is(Value::s("supported")) }]);
        assert!(!schema().get("shuffle_rewards").unwrap().tip.contains("never shuffled yet"));
    }

    #[test]
    fn stones_and_medallions_only_show_under_supported_rewards() {
        let mut s: Settings = Settings::default();
        s.set("shuffle_dungeon_rewards", Value::s("anywhere")).unwrap();
        s.set("shuffle_rewards", Value::s("off")).unwrap();
        s.normalize();
        assert!(!s.is_enabled("shuffle_dungeon_rewards"));
        assert_eq!(s.text("shuffle_dungeon_rewards").unwrap(), "vanilla");
        s.set("shuffle_rewards", Value::s("supported")).unwrap();
        assert!(s.is_enabled("shuffle_dungeon_rewards"));
    }

    #[test]
    fn old_strings_keep_the_rewards_they_were_written_with() {
        let cases: [(&str, &str, bool, bool, &str); 4] = [
            ("OOTRR1-XZLA", "off", false, false, "all"),
            ("OOTRR1-EYAQD32I", "supported", false, false, "all"),
            ("OOTRR1-EYAQCMYBAHY7A", "supported", false, true, "all"),
            ("OOTRR1-BAAQFSMI", "off", false, false, "dungeons"),
        ];
        for (t, rewards, dungeon, extra, chests) in cases {
            let l: Loaded = Settings::decode(t).unwrap();
            assert!(l.warnings.is_empty(), "{t}");
            let s: Settings = l.settings;
            assert_eq!(s.text("shuffle_rewards").unwrap(), rewards, "{t}");
            assert_eq!(s.text("shuffle_dungeon_rewards").unwrap() == "anywhere", dungeon, "{t}");
            assert_eq!(s.flag("shuffle_rewards_extra").unwrap(), extra, "{t}");
            assert_eq!(s.text("shuffle_chests").unwrap(), chests, "{t}");
            assert_eq!(s.encode().unwrap(), t);
        }
    }

    #[test]
    fn rewards_off_writes_the_old_default_string_and_the_new_toggle_round_trips() {
        let old = |s: &mut Settings| {
            for (k, v) in [("time_always_passes", Value::Bool(false)), ("deku_bridge_safe", Value::Bool(false)), ("shuffle_boss_hearts", Value::Bool(false)), ("shuffle_rewards_extra", Value::Bool(false)), ("navi_return", Value::s("off")), ("shuffle_ocarinas", Value::Bool(true)), ("shuffle_kokiri_sword", Value::Bool(true))] {
                s.set(k, v).unwrap();
            }
        };
        let mut s: Settings = Settings::default();
        old(&mut s);
        assert_eq!(s.encode().unwrap(), "OOTRR1-EYAQD32I");
        s.set("shuffle_rewards", Value::s("off")).unwrap();
        assert_eq!(s.encode().unwrap(), "OOTRR1-XZLA");
        let mut s: Settings = Settings::default();
        old(&mut s);
        for v in ["anywhere", "dungeons"] {
            s.set("shuffle_dungeon_rewards", Value::s(v)).unwrap();
            let t: String = s.encode().unwrap();
            assert_ne!(t, "OOTRR1-EYAQD32I");
            let l: Loaded = Settings::decode(&t).unwrap();
            assert!(l.warnings.is_empty());
            assert_eq!(l.settings, s);
            assert_eq!(l.settings.text("shuffle_dungeon_rewards").unwrap(), v);
        }
    }

    #[test]
    fn old_strings_keep_the_clock_off_and_the_door_closed_while_new_defaults_turn_the_clock_on() {
        for t in ["OOTRR1-XZLA", "OOTRR1-EYAQD32I", "OOTRR1-BAAQFSMI", "OOTRR1-BEAQGGABAUOACZBEAMBAABWEOQ"] {
            let s: Settings = Settings::decode(t).unwrap().settings;
            assert!(!s.flag("time_always_passes").unwrap(), "{t}");
            assert!(!s.flag("deku_bridge_safe").unwrap(), "{t}");
            assert!(!s.flag("open_door_of_time").unwrap(), "{t}");
            assert!(!s.flag("jabu_open").unwrap(), "{t}");
            assert_eq!(s.text("zora_fountain").unwrap(), "closed", "{t}");
            // Old strings shuffled Saria's Ocarina and the Kokiri Sword chest, so they read as on.
            assert!(s.flag("shuffle_ocarinas").unwrap() && s.flag("shuffle_kokiri_sword").unwrap(), "{t}");
            assert_eq!(s.encode().unwrap(), t);
        }
        let d: Settings = Settings::default();
        assert!(d.flag("time_always_passes").unwrap());
        assert!(!d.flag("open_door_of_time").unwrap());
        let t: String = d.encode().unwrap();
        assert_ne!(t, "OOTRR1-EYAQD32I");
        assert_eq!(Settings::decode(&t).unwrap().settings, d);
        let mut all: Settings = Settings::default();
        for (k, v) in [("zora_fountain", Value::s("open")), ("jabu_open", Value::Bool(true)), (crate::door_gate::SETTING, Value::s(crate::door_gate::OPEN)), ("early_beds", Value::Bool(true))] {
            all.set(k, v).unwrap();
        }
        let back: Settings = Settings::decode(&all.encode().unwrap()).unwrap().settings;
        assert_eq!(back, all);
        let mut late: Settings = all.clone();
        late.set("time_always_passes", Value::Bool(false)).unwrap();
        late.normalize();
        assert!(!late.flag("early_beds").unwrap());
        for (n, id) in [(75, "jabu_open"), (76, "time_always_passes"), (77, "early_beds")] {
            assert_eq!(schema().get(id).unwrap().num, n);
        }
    }

    #[test]
    fn the_potion_shop_tip_reads_as_checks_and_the_echo_gem_keeps_its_id() {
        let s: &Setting = schema().get("shuffle_potion_shop").unwrap();
        assert_eq!(s.num, 46);
        for w in ["two shuffled checks", "has met Zelda", "by day", "empty bottle"] {
            assert!(s.tip.contains(w), "{w}");
        }
        assert!(!s.tip.contains("gives a child"));
        let c: &model::Choice = schema().get("starting_items").unwrap().choices.iter().find(|c| c.id == "stone_of_agony").unwrap();
        assert_eq!(c.label, "Echo Gem (Stone of Agony)");
        assert!(schema().get("starting_items").unwrap().choices.iter().all(|c| c.label != "Stone of Agony"));
    }
}
