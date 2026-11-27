pub mod analyzed;
mod apply;
mod clone;
pub mod cows;
mod data;
pub mod frogs;
mod gate;
pub mod goron_pot;
mod op;
pub mod potion;
pub mod sim;
pub mod sun;
pub mod trade;

pub use apply::{always_ops, edit_bytes, edit_flow, ops_for, ops_of};
pub use data::{Extra, Spec, JSON};
pub use gate::{bound, fits, open, pouch, POUCH_ONLY};
pub use op::{apply as apply_op, How, Once, Op, ITEM};
pub(crate) use op::progressive_once;

pub const SETTING: &str = "shuffle_rewards_extra";

pub const GERUDO_CARD: &str = "shuffle_gerudo_card";
pub const GERUDO_CARD_NUM: u16 = 170;
pub const GERUDO_CARD_LOC: &str = "Hideout Gerudo Membership Card";

pub fn scripted(name: &str) -> bool {
    sun::is(name) || goron_pot::is(name)
}

pub const MERCHANTS: u16 = 97;
pub const MERCHANTS_SETTING: &str = "shuffle_merchants";

pub fn merchants(s: &crate::settings::Settings, bad: &mut Vec<String>) -> (bool, bool) {
    match s.text(MERCHANTS_SETTING) {
        Ok("off") => (false, false),
        Ok("beans") => (true, false),
        Ok("no_beans") => (false, true),
        Ok("all") => (true, true),
        Ok(v) => {
            bad.push(format!("{MERCHANTS_SETTING} is {v}, which is not a known choice"));
            (false, false)
        }
        Err(e) => {
            bad.push(e.to_string());
            (false, false)
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Rewards {
    pub on: bool,
    /// The Stones and Medallions leave their vanilla spots (End of dungeons or Anywhere).
    pub dungeon_rewards: bool,
    /// End of dungeons: they only trade places among the nine boss and Rauru reward events.
    pub dungeon_own: bool,
    pub extra: bool,
    pub trade: bool,
    /// Every adult trade item is its own shuffled item (crate::open_world::trade_shuffle).
    pub trade_items: bool,
    pub cows: bool,
    pub cows_other: bool,
    pub frog_songs: bool,
    pub frog_bug: bool,
    pub potion: bool,
    pub gerudo_card: bool,
    pub scrubs: bool,
    pub scrubs_b: bool,
    pub medigoron: bool,
    pub bean_sale: bool,
    pub scrub_prices: crate::scrub_prices::Mode,
    pub scrub_hint: bool,
    pub merchant_prices: crate::scrub_prices::Mode,
    pub fairy_prices: crate::scrub_prices::Mode,
}

impl Rewards {
    pub fn read(s: &crate::settings::Settings, bad: &mut Vec<String>) -> Rewards {
        use crate::slice::support::toggle;
        let on: bool = match s.text("shuffle_rewards") {
            Ok("off") => false,
            Ok("supported") => true,
            Ok(v) => {
                bad.push(format!("shuffle_rewards is {v}, which this build cannot apply (only off or supported work)"));
                false
            }
            Err(e) => {
                bad.push(e.to_string());
                false
            }
        };
        let (dungeon_rewards, dungeon_own): (bool, bool) = match s.text(crate::slice::support::DUNGEON_REWARDS) {
            Ok("vanilla") => (false, false),
            Ok("anywhere") => (on, false),
            Ok("dungeons") => (on, on),
            Ok(v) => {
                bad.push(format!("{} is {v}, which is not a known choice", crate::slice::support::DUNGEON_REWARDS));
                (false, false)
            }
            Err(e) => {
                bad.push(e.to_string());
                (false, false)
            }
        };
        let extra: bool = on && toggle(s, SETTING, bad);
        let trade: bool = on && toggle(s, trade::SETTING, bad);
        let trade_items: bool = trade && toggle(s, crate::open_world::trade_shuffle::SETTING, bad);
        let cows: bool = on && toggle(s, cows::SETTING, bad);
        let frog_songs: bool = on && toggle(s, frogs::SONGS_SETTING, bad);
        let frog_bug: bool = on && toggle(s, frogs::BUG_SETTING, bad);
        let potion: bool = on && toggle(s, potion::SETTING, bad);
        let gerudo_card: bool = on && toggle(s, GERUDO_CARD, bad);
        let scrubs: bool = crate::scrubs_a::read(s, on, bad);
        let scrubs_b: bool = crate::scrubs_b::read(s, on, bad);
        Rewards {
            on,
            dungeon_rewards,
            dungeon_own,
            extra,
            trade,
            trade_items,
            cows,
            cows_other: cows,
            frog_songs,
            frog_bug,
            potion,
            gerudo_card,
            scrubs,
            scrubs_b,
            medigoron: false,
            bean_sale: false,
            scrub_prices: crate::scrub_prices::Mode::Vanilla,
            scrub_hint: false,
            merchant_prices: crate::scrub_prices::Mode::Vanilla,
            fairy_prices: crate::scrub_prices::Mode::Vanilla,
        }
    }

    pub fn read_merchants(&mut self, s: &crate::settings::Settings, beans: bool, bad: &mut Vec<String>) {
        let (bean_merchant, medigoron_merchant): (bool, bool) = merchants(s, bad);
        self.medigoron = self.on && medigoron_merchant;
        self.bean_sale = beans && bean_merchant;
        self.scrub_prices = crate::scrub_prices::read(s, bad);
        self.scrub_hint = crate::scrub_prices::read_hint(s, bad);
        self.merchant_prices = crate::merchant_prices::read(s, bad);
        self.fairy_prices = crate::fairy_prices::read(s, bad);
    }
}
