use std::collections::{BTreeMap as BM, BTreeSet as BS};
use std::fmt::Write as W;
use std::path::{Path as P, PathBuf as PB};

use crate::bfevfl::flow::NONE;
use crate::bfevfl::{scale_waits, Actor, Bf, Case, Cont, Entry, Event, Flow, Kind, Val};
use crate::byml::{Byml, Node};
use crate::flow_locations::path_of;
use crate::error::{Error, Res};
use crate::placement::{banc_files, hash_of};
use crate::romfs::Romfs;
use crate::slice::data::Data;
use crate::staging::{banc_out, banc_raw, commit, rewrite, Committed, Out};
use crate::tier_chain::Step;

pub const FAMILIES: &[(&str, &[&str])] = &[
    ("Hookshot", &["Item_Hookshot", "Item_Longshot"]),
    ("Strength", &["Item_Gauntlet_Goron", "Item_Gauntlet_Silver", "Item_Gauntlet_Gold"]),
    ("BombBag", &["Item_BombBag_Small", "Item_BombBag_Medium", "Item_BombBag_Large"]),
    ("Bow", &["Item_Range_Bow", "Item_Quiver_Medium", "Item_Quiver_Large"]),
    ("Slingshot", &["Item_Range_SlingShot", "Item_BulletBag_Medium", "Item_BulletBag_Large"]),
    ("Wallet", &["Item_Wallet_Medium", "Item_Wallet_Large"]),
    ("Scale", &["Item_Scale_Silver", "Item_Scale_Gold"]),
    ("NutBag", &["Item_NutBag_Medium", "Item_NutBag_Large"]),
    ("StickBag", &["Item_StickBag_Medium", "Item_StickBag_Large"]),
    ("Magic", &["Item_FairyBless_MP", "Item_FairyBless_MPDouble"]),
    ("Ocarina", &["Item_Ocarina_Fairy", "Item_Ocarina_Time"]),
];

pub const SMALL_FLOW: &str = "Event/EventFlow/Dm_SY_TreasureboxSmall.bfevfl.zs";
pub const BIG_FLOW: &str = "Event/EventFlow/Dm_SY_Treasurebox.bfevfl.zs";
pub const SETTING_TABLE: &str = "RSDB/EventSetting.Product.111.rstbl.byml.zs";
pub const ANALYZED_TABLE: &str = "RSDB/GameAnalyzedEventInfo.Product.111.rstbl.byml.zs";
const QUERY: &str = "EventQueryCheckIsGet";
const POUCH: &str = "PouchSystemActor";
const EXISTS: &str = "EventQueryIsExistInPouch";
const HOLDER: &str = "GameDataActor";
const ROOT_ENTRY: &str = "Success_";
const GUARD: &str = "AmmoGuard_";
/// What ammo gives to a player without its bag: a Blue Rupee get, then 5 rupees more
/// (NO_BAG_EXTRA), 10 rupees in all.
pub const NO_BAG: &str = "TBox_Rupee_Blue";
pub const NO_BAG_EXTRA: i32 = 5;

/// The event after the extra rupees of a no-bag give, when `at` is that event; `at` otherwise.
pub fn skip_extra_rupees(f: &Flow, at: u16) -> u16 {
    if at == NONE {
        return at;
    }
    match f.events.get(usize::from(at)).map(|e| &e.kind) {
        Some(Kind::Action { next, actor, act, params }) => {
            let a = f.actors.get(usize::from(*actor));
            let rupee: bool = a.is_some_and(|a| a.name == "PouchSystemActor" && a.actions.get(usize::from(*act)).is_some_and(|x| x == "EventTriggerIncreaseRupee"));
            let ours: bool = params.as_ref().is_some_and(|c| c.items.iter().any(|(k, v)| k == "Value" && *v == Val::Int(NO_BAG_EXTRA)));
            if rupee && ours { *next } else { at }
        }
        _ => at,
    }
}

/// Runs `then` after the extra rupees of a no-bag give: PouchSystemActor.EventTriggerIncreaseRupee.
fn extra_rupees(f: &mut Flow, then: u16) -> Res<u16> {
    let (a, x): (u16, u16) = crate::dungeon_credit::action_slot(f, "PouchSystemActor", "EventTriggerIncreaseRupee")?;
    let c: Cont = Cont { items: vec![("Value".to_string(), Val::Int(NO_BAG_EXTRA))] };
    push(f, Kind::Action { next: then, actor: a, act: x, params: Some(c) })
}
const BOMB_BAGS: &[&str] = &["Item_BombBag_Small", "Item_BombBag_Medium", "Item_BombBag_Large"];
const QUIVERS: &[&str] = &["Item_Quiver_Small", "Item_Quiver_Medium", "Item_Quiver_Large"];
const BULLET_BAGS: &[&str] = &["Item_BulletBag_Small", "Item_BulletBag_Medium", "Item_BulletBag_Large"];
const CHU_BAG: &[&str] = &[crate::bombchu_bag::CHU];
/// Ammo the pouch can only hold with a bag, and those bags. Handed out without one, the give adds
/// the ammo anyway and the game sets up its slot (a usable bomb bag for bombs), so every give of
/// these checks for a bag first and hands out a green rupee without one.
const AMMO: &[(&str, &[&str])] = &[
    ("Item_Bomb", BOMB_BAGS),
    ("Item_BombBundle_Small", BOMB_BAGS),
    ("TBox_ArrowBundle_Medium", QUIVERS),
    ("TBox_ArrowBundle_Large", QUIVERS),
    ("Item_Arrow_Normal", QUIVERS),
    ("TBox_DekuSeedBundle", BULLET_BAGS),
    ("Item_DekuSeed", BULLET_BAGS),
    ("Item_BombchuBundle_Small", CHU_BAG),
    ("Item_BombchuBundle_Medium", CHU_BAG),
    ("Item_BombchuBundle_Large", CHU_BAG),
];

/// The bags that let the pouch hold `actor`, if it is ammo that needs one.
pub fn bags_of(actor: &str) -> Option<&'static [&'static str]> {
    AMMO.iter().find(|a| a.0 == actor).map(|a| a.1)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Size {
    Small,
    Big,
}

impl Size {
    pub fn of_gyaml(g: &str) -> Size {
        if g.starts_with("Obj_TBox_A_02") || g.starts_with("Obj_TBox_A_03") {
            Size::Big
        } else {
            Size::Small
        }
    }

    pub fn flow(self) -> &'static str {
        match self {
            Size::Small => SMALL_FLOW,
            Size::Big => BIG_FLOW,
        }
    }

    pub fn template(self) -> &'static str {
        match self {
            Size::Small => "Dm_SY_TreasureboxSmall",
            Size::Big => "Dm_SY_Treasurebox",
        }
    }

    fn tag(self) -> &'static str {
        match self {
            Size::Small => "Small",
            Size::Big => "Big",
        }
    }
}

pub fn owners(family: &str) -> Res<&'static [&'static str]> {
    match family {
        "Strength" => Ok(&["Armor_Child_Gauntlet_Goron", "Armor_Adult_Gauntlet_Silver", "Armor_Adult_Gauntlet_Gold"]),
        "Bow" => Ok(&["Weapon_Range_Bow", "Item_Quiver_Medium", "Item_Quiver_Large"]),
        "Slingshot" => Ok(&["Weapon_Range_SlingShot", "Item_BulletBag_Medium", "Item_BulletBag_Large"]),
        _ => tiers(family),
    }
}

pub fn tiers(family: &str) -> Res<&'static [&'static str]> {
    FAMILIES
        .iter()
        .find(|f| f.0 == family)
        .map(|f| f.1)
        .ok_or_else(|| Error::Msg(format!("{family} is not a progressive family")))
}

pub fn family_of(t: &[String]) -> Option<&'static str> {
    FAMILIES
        .iter()
        .find(|f| f.1.len() == t.len() && f.1.iter().zip(t).all(|(a, b)| a == b))
        .map(|f| f.0)
}

pub fn needs_every_holder(family: &str) -> bool {
    matches!(family, "Hookshot" | "Strength" | "Scale" | "Wallet" | "Ocarina")
}

pub fn event_name(family: &str, size: Size) -> String {
    format!("OOTRR_Prog_{family}_{}", size.tag())
}

pub fn entry_name(family: &str) -> String {
    format!("Success_Prog_{family}")
}

fn idx(n: usize) -> Res<u16> {
    u16::try_from(n).map_err(|_| Error::Format("progressive: index does not fit in 16 bits"))
}

fn push(f: &mut Flow, kind: Kind) -> Res<u16> {
    let i: u16 = idx(f.events.len())?;
    f.events.push(Event { name: format!("Prog{i}"), kind });
    Ok(i)
}

fn give(f: &mut Flow, actor: &str) -> Res<u16> {
    let c: Cont = Cont {
        items: vec![
            ("ContentName".to_string(), Val::Str(actor.to_string())),
            ("IsMiniGame".to_string(), Val::Bool(0)),
        ],
    };
    push(f, Kind::Sub { next: NONE, params: Some(c), flow: String::new(), entry: ROOT_ENTRY.to_string() })
}

fn ensure_query(f: &mut Flow) -> Res<(u16, u16)> {
    ensure(f, HOLDER, QUERY)
}

pub(crate) fn chain_to(f: &mut Flow, family: &str, gives: &[u16]) -> Res<Kind> {
    let (a, q) = ensure_query(f)?;
    let o: &[&str] = owners(family)?;
    chain(f, a, q, family, o, gives)
}

pub(crate) fn ensure(f: &mut Flow, holder: &str, query: &str) -> Res<(u16, u16)> {
    let i: usize = match f.actors.iter().position(|a| a.name == holder && a.sub.is_empty()) {
        Some(i) => i,
        None => {
            let t: &Actor = f.actors.first().ok_or(Error::Format("progressive: the flow has no actors"))?;
            let a: Actor = Actor {
                name: holder.to_string(),
                sub: String::new(),
                arg: String::new(),
                actions: Vec::new(),
                queries: Vec::new(),
                params: t.params.clone(),
                ep: t.ep,
                cut: t.cut,
            };
            f.actors.push(a);
            f.actors.len() - 1
        }
    };
    let a = &mut f.actors[i];
    let j: usize = match a.queries.iter().position(|q| q == query) {
        Some(j) => j,
        None => {
            a.queries.push(query.to_string());
            a.queries.len() - 1
        }
    };
    Ok((idx(i)?, idx(j)?))
}

pub fn wrap(f: &mut Flow, family: &str) -> Res<()> {
    let t: &[&str] = tiers(family)?;
    let o: &[&str] = owners(family)?;
    let name: String = entry_name(family);
    if f.entries.iter().any(|e| e.name == name) {
        return Ok(());
    }
    let pad: u16 = f
        .entries
        .iter()
        .find(|e| e.name == ROOT_ENTRY)
        .map(|e| e.pad)
        .ok_or(Error::Format("progressive: the chest flow has no Success_ entry"))?;
    let (a, q) = ensure_query(f)?;
    let mut subs: Vec<u16> = Vec::new();
    for x in t {
        let form: String = crate::ocarina_hold::chest_form(x);
        let at: u16 = give(f, &form)?;
        if form != *x {
            let added: u16 = crate::ocarina_hold::after(f, x, NONE)?;
            set_next(&mut f.events[usize::from(at)].kind, added)?;
        }
        subs.push(at);
    }
    let root: Kind = chain(f, a, q, family, o, &subs)?;
    let main: u16 = push(f, root)?;
    f.entries.push(Entry { name, subs, vars: Vec::new(), main, pad });
    Ok(())
}

pub fn guarded(actor: &str) -> bool {
    bags_of(actor).is_some()
}

pub fn guard_family(actor: &str) -> String {
    format!("{GUARD}{actor}")
}

fn guard_of(family: &str) -> Option<&str> {
    family.strip_prefix(GUARD)
}

pub fn is_guard(family: &str) -> bool {
    guard_of(family).is_some()
}

pub fn guard(f: &mut Flow, actor: &str) -> Res<()> {
    let name: String = entry_name(&guard_family(actor));
    if f.entries.iter().any(|e| e.name == name) {
        return Ok(());
    }
    let pad: u16 = f
        .entries
        .iter()
        .find(|e| e.name == ROOT_ENTRY)
        .map(|e| e.pad)
        .ok_or(Error::Format("progressive: the chest flow has no Success_ entry"))?;
    let bags: &[&str] = bags_of(actor).ok_or_else(|| Error::Msg(format!("progressive: {actor} is not ammo that needs a bag")))?;
    let keep: u16 = give(f, actor)?;
    let none: u16 = give(f, NO_BAG)?;
    let at: u16 = bag_check(f, bags, keep, none)?;
    f.entries.push(Entry { name, subs: vec![keep, none], vars: Vec::new(), main: at, pad });
    Ok(())
}

/// Questions that lead to `keep` for an owner of one of `bags`, else to `none` followed by the
/// extra rupees of a no-bag give. `none` must be a sub flow call that ends the flow.
fn bag_check(f: &mut Flow, bags: &[&str], keep: u16, none: u16) -> Res<u16> {
    let (a, q) = ensure_query(f)?;
    let more: u16 = extra_rupees(f, NONE)?;
    if let Kind::Sub { next, .. } = &mut f.events[usize::from(none)].kind {
        *next = more;
    }
    let mut at: u16 = none;
    for owner in bags {
        let c: Cont = Cont { items: vec![("ActorName".to_string(), Val::Str(owner.to_string()))] };
        let cases: Vec<Case> = vec![Case { val: 0, ev: keep }, Case { val: 1, ev: at }];
        at = push(f, Kind::Switch { actor: a, query: q, cases, params: Some(c) })?;
    }
    Ok(at)
}

/// A new entry `name` that runs the entry `inner` of the same flow: with the chest's own
/// ContentName for a player who owns a bag for `actor`, with the no-bag rupee for anyone else.
pub fn guard_entry(f: &mut Flow, name: &str, inner: &str, actor: &str) -> Res<()> {
    if f.entries.iter().any(|e| e.name == name) {
        return Ok(());
    }
    let pad: u16 = f.entries.iter().find(|e| e.name == inner).map(|e| e.pad).ok_or_else(|| Error::Msg(format!("progressive: the flow has no {inner} entry")))?;
    let bags: &[&str] = bags_of(actor).ok_or_else(|| Error::Msg(format!("progressive: {actor} is not ammo that needs a bag")))?;
    let call = |content: Val| Kind::Sub { next: NONE, params: Some(Cont { items: vec![("ContentName".to_string(), content)] }), flow: String::new(), entry: inner.to_string() };
    let keep: u16 = push(f, call(Val::Arg("ContentName".to_string())))?;
    let none: u16 = push(f, call(Val::Str(NO_BAG.to_string())))?;
    let at: u16 = bag_check(f, bags, keep, none)?;
    f.entries.push(Entry { name: name.to_string(), subs: vec![keep, none], vars: Vec::new(), main: at, pad });
    Ok(())
}

pub(crate) fn chain(f: &mut Flow, a: u16, q: u16, family: &str, o: &[&str], gives: &[u16]) -> Res<Kind> {
    let s: Vec<Step> = crate::tier_chain::of(family, gives.len());
    let mut at: Vec<u16> = gives.to_vec();
    let mut root: Option<Kind> = None;
    for (i, x) in s.iter().enumerate().skip(gives.len()) {
        let Step::Ask { owner, yes, no } = *x else {
            return Err(Error::Format("progressive: a chain step past the gives is not a question"));
        };
        let c: Cont = Cont { items: vec![("ActorName".to_string(), Val::Str(o[owner].to_string()))] };
        let cases: Vec<Case> = vec![Case { val: 0, ev: at[yes] }, Case { val: 1, ev: at[no] }];
        let k: Kind = Kind::Switch { actor: a, query: q, cases, params: Some(c) };
        if i + 1 == s.len() {
            root = Some(k);
        } else {
            at.push(push(f, k)?);
        }
    }
    root.ok_or(Error::Format("progressive: a family needs at least two tiers"))
}

fn next_of(k: &Kind) -> Option<u16> {
    match k {
        Kind::Action { next, .. } | Kind::Sub { next, .. } => Some(*next),
        _ => None,
    }
}

fn set_next(k: &mut Kind, to: u16) -> Res<()> {
    match k {
        Kind::Action { next, .. } | Kind::Sub { next, .. } => {
            *next = to;
            Ok(())
        }
        _ => Err(Error::Format("progressive: the grant event has no single next")),
    }
}

fn with_param(k: &Kind, param: &str, value: &str) -> Res<Kind> {
    let mut out: Kind = k.clone();
    let c: &mut Cont = out.params_mut().ok_or(Error::Format("progressive: the grant event has no parameters"))?;
    let slot: &mut Val = c
        .items
        .iter_mut()
        .find(|(n, _)| n == param)
        .map(|(_, v)| v)
        .ok_or_else(|| Error::Msg(format!("the grant event has no {param} parameter")))?;
    *slot = Val::Str(value.to_string());
    Ok(out)
}

pub fn wrap_grant(f: &mut Flow, index: usize, event: &str, param: &str, family: &str) -> Res<()> {
    wrap_grant_as(f, index, event, param, family, &[])
}

/// wrap_grant, with each tier's branch handing out `forms[i]` instead of the tier actor (the
/// ownership questions still ask about the tier actors). Empty `forms` means the tiers.
pub fn wrap_grant_as(f: &mut Flow, index: usize, event: &str, param: &str, family: &str, forms: &[String]) -> Res<()> {
    let tiers_of: &[&str] = tiers(family)?;
    if !forms.is_empty() && forms.len() != tiers_of.len() {
        return Err(Error::Msg(format!("progressive: {family} has {} tiers but {} forms", tiers_of.len(), forms.len())));
    }
    let t: Vec<&str> = if forms.is_empty() { tiers_of.to_vec() } else { forms.iter().map(|s| s.as_str()).collect() };
    let o: &[&str] = owners(family)?;
    let base: Kind = {
        let e: &Event = f.events.get(index).ok_or_else(|| Error::Msg(format!("event index {index} is out of range")))?;
        if e.name != event {
            return Err(Error::Msg(format!("event {index} is named {} and not {event}, the game files differ", e.name)));
        }
        if matches!(e.kind, Kind::Switch { .. }) {
            return Ok(());
        }
        e.kind.clone()
    };
    let (a, q) = ensure_query(f)?;
    let mut gives: Vec<u16> = Vec::new();
    for (x, tier) in t.iter().zip(tiers_of) {
        let mut give: Kind = with_param(&base, param, x)?;
        // A pickup actor handed out in place of a tier with no animation (crate::ocarina_hold) is
        // followed by the tier item added like Saria's gift.
        if x != tier && crate::ocarina_hold::no_anim(tier) {
            let then: u16 = next_of(&give).ok_or(Error::Format("progressive: the grant event has no single next"))?;
            let added: u16 = crate::ocarina_hold::after(f, tier, then)?;
            set_next(&mut give, added)?;
        }
        // The Bow or Slingshot tier then adds its starting bag (crate::starter_bags).
        if let Some(then) = next_of(&give)
            && let Some(bag) = crate::starter_bags::line(f, tier, then)?
        {
            set_next(&mut give, bag)?;
        }
        gives.push(push(f, give)?);
    }
    f.events[index].kind = chain(f, a, q, family, o, &gives)?;
    let at: u16 = idx(index)?;
    for e in f.entries.iter_mut() {
        if let Some(p) = e.subs.iter().position(|x| *x == at) {
            e.subs.remove(p);
            e.subs.extend(gives.iter().copied());
        }
    }
    Ok(())
}

pub const PLAIN_GIFT: &str = "Dm_SY_GetInEvent:GetInEvent_Demo_NotLookatObjectEntry";

fn with_sub(k: &Kind, to: &str) -> Res<Kind> {
    let (tf, te): (&str, &str) = to.split_once(':').ok_or_else(|| Error::Msg(format!("progressive: {to} has no colon")))?;
    let mut out: Kind = k.clone();
    let Kind::Sub { flow, entry, .. } = &mut out else {
        return Err(Error::Format("progressive: only a sub flow call can be pointed at another sub flow"));
    };
    *flow = tf.to_string();
    *entry = te.to_string();
    Ok(out)
}

pub fn guard_grant(f: &mut Flow, index: usize, event: &str, param: &str, family: &str, plain: bool) -> Res<()> {
    let t: &[&str] = tiers(family)?;
    let o: &[&str] = owners(family)?;
    let kept: Kind = {
        let e: &Event = f.events.get(index).ok_or_else(|| Error::Msg(format!("event index {index} is out of range")))?;
        if e.name != event {
            return Err(Error::Msg(format!("event {index} is named {} and not {event}, the game files differ", e.name)));
        }
        if matches!(e.kind, Kind::Switch { .. }) {
            return Ok(());
        }
        e.kind.clone()
    };
    let mut first: Kind = with_param(&kept, param, t[0])?;
    if plain {
        first = with_sub(&first, PLAIN_GIFT)?;
    }
    let (a, q) = ensure_query(f)?;
    let (pa, pq) = ensure(f, POUCH, EXISTS)?;
    let keep: u16 = push(f, kept)?;
    let base: u16 = push(f, first)?;
    // The base tier given to a player without the weapon brings its starting bag (crate::starter_bags).
    if let Some(then) = next_of(&f.events[usize::from(base)].kind)
        && let Some(bag) = crate::starter_bags::line(f, t[0], then)?
    {
        set_next(&mut f.events[usize::from(base)].kind, bag)?;
    }
    let stock: Cont = Cont { items: vec![("Num".to_string(), Val::Int(1)), ("Actor".to_string(), Val::Str(o[0].to_string()))] };
    let in_pouch: u16 = push(f, Kind::Switch { actor: pa, query: pq, cases: vec![Case { val: 0, ev: keep }, Case { val: 1, ev: base }], params: Some(stock) })?;
    let c: Cont = Cont { items: vec![("ActorName".to_string(), Val::Str(o[0].to_string()))] };
    let cases: Vec<Case> = vec![Case { val: 0, ev: keep }, Case { val: 1, ev: in_pouch }];
    f.events[index].kind = Kind::Switch { actor: a, query: q, cases, params: Some(c) };
    let at: u16 = idx(index)?;
    for e in f.entries.iter_mut() {
        if let Some(p) = e.subs.iter().position(|x| *x == at) {
            e.subs.remove(p);
            e.subs.extend([keep, base]);
        }
    }
    Ok(())
}

/// The give at `index` hands out the actor in its `param`. When that actor is ammo that needs a
/// bag, the give becomes a question: a player with a bag gets it, anyone else a green rupee given
/// the same way. Other actors, and a give that is already a question, are left alone.
pub fn ammo_guard(f: &mut Flow, index: usize, event: &str, param: &str) -> Res<bool> {
    let kept: Kind = {
        let e: &Event = f.events.get(index).ok_or_else(|| Error::Msg(format!("event index {index} is out of range")))?;
        if e.name != event {
            return Err(Error::Msg(format!("event {index} is named {} and not {event}, the game files differ", e.name)));
        }
        if matches!(e.kind, Kind::Switch { .. }) {
            return Ok(false);
        }
        e.kind.clone()
    };
    let actor: Option<String> = match kept.params().and_then(|c| c.get(param)) {
        Some(Val::Str(s)) => Some(s.clone()),
        _ => None,
    };
    let Some(bags) = actor.as_deref().and_then(bags_of) else {
        return Ok(false);
    };
    let mut rupee: Kind = with_param(&kept, param, NO_BAG)?;
    let then: Option<u16> = match &rupee {
        Kind::Action { next, .. } | Kind::Sub { next, .. } => Some(*next),
        _ => None,
    };
    if let Some(t) = then {
        let more: u16 = extra_rupees(f, t)?;
        if let Kind::Action { next, .. } | Kind::Sub { next, .. } = &mut rupee {
            *next = more;
        }
    }
    let (a, q) = ensure_query(f)?;
    let keep: u16 = push(f, kept)?;
    let none: u16 = push(f, rupee)?;
    let mut at: u16 = none;
    for (i, owner) in bags.iter().enumerate() {
        let c: Cont = Cont { items: vec![("ActorName".to_string(), Val::Str(owner.to_string()))] };
        let k: Kind = Kind::Switch { actor: a, query: q, cases: vec![Case { val: 0, ev: keep }, Case { val: 1, ev: at }], params: Some(c) };
        if i + 1 == bags.len() {
            f.events[index].kind = k;
        } else {
            at = push(f, k)?;
        }
    }
    let was: u16 = idx(index)?;
    for e in f.entries.iter_mut() {
        if let Some(p) = e.subs.iter().position(|x| *x == was) {
            e.subs.remove(p);
            e.subs.extend([keep, none]);
        }
    }
    Ok(true)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Wrap {
    Chain,
    Base { plain: bool },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Grant {
    pub file: String,
    pub index: usize,
    pub event: String,
    pub param: String,
    pub family: &'static str,
    pub wrap: Wrap,
    /// The actor each tier's branch hands out, when it is not the tier actor itself
    /// (crate::ocarina_hold). Empty means the tiers.
    pub forms: Vec<String>,
}

impl Grant {
    pub fn apply(&self, f: &mut Flow) -> Res<()> {
        match self.wrap {
            Wrap::Chain => wrap_grant_as(f, self.index, &self.event, &self.param, self.family, &self.forms),
            Wrap::Base { plain } => guard_grant(f, self.index, &self.event, &self.param, self.family, plain),
        }
    }
}

pub fn stage_grants(rf: &mut Romfs, grants: &[Grant]) -> Res<Vec<Out>> {
    let mut by: BM<String, Vec<&Grant>> = BM::new();
    for g in grants {
        by.entry(path_of(&g.file)).or_default().push(g);
    }
    let mut outs: Vec<Out> = Vec::new();
    for (path, gs) in &by {
        outs.push(rewrite(rf, path, |raw: &[u8]| {
            let mut bf: Bf = Bf::parse(raw)?;
            let flow: &mut Flow = bf.flows.first_mut().ok_or(Error::Format("progressive: the flow file has no flow"))?;
            for g in gs {
                g.apply(flow)?;
            }
            bf.write()
        })?);
    }
    Ok(outs)
}

fn text<'a>(n: &'a Node, k: &str) -> &'a str {
    n.get(k).and_then(|v| v.as_str()).unwrap_or("")
}

pub fn add_rows(y: &mut Byml, template: &str, rows: &[(String, Option<String>)]) -> Res<usize> {
    let list: &mut Vec<Node> = y.root.as_arr_mut().ok_or(Error::Format("progressive: table root is not a list"))?;
    let base: Node = list
        .iter()
        .find(|r| text(r, "__RowId") == template)
        .cloned()
        .ok_or_else(|| Error::Msg(format!("the table has no {template} row")))?;
    let mut n: usize = 0;
    for (id, entry) in rows {
        let mut r: Node = base.clone();
        r.set("__RowId", Node::Str(id.clone()));
        if let Some(e) = entry {
            r.set("EntryPointName", Node::Str(e.clone()));
        }
        match list.iter().position(|x| text(x, "__RowId") == id) {
            Some(i) => list[i] = r,
            None => {
                let at: usize = list.partition_point(|x| text(x, "__RowId") < id.as_str());
                list.insert(at, r);
            }
        }
        n += 1;
    }
    Ok(n)
}

pub fn add_children(y: &mut Byml, kids: &[(String, String)]) -> usize {
    let Some(list) = y.root.as_arr_mut() else {
        return 0;
    };
    let mut n: usize = 0;
    for (parent, child) in kids {
        let Some(r) = list.iter_mut().find(|r| text(r, "__RowId") == parent) else {
            continue;
        };
        let Some(c) = r.get_mut("ChildEventList").and_then(|c| c.as_arr_mut()) else {
            continue;
        };
        if !c.iter().any(|x| x.as_str() == Some(child.as_str())) {
            c.push(Node::Str(child.clone()));
            n += 1;
        }
    }
    n
}

pub fn edit_chest(y: &mut Byml, hash: u64, drop: &str, event: &str) -> Option<Size> {
    let list: &mut Vec<Node> = y.root.get_mut("Actors")?.as_arr_mut()?;
    let mut out: Option<Size> = None;
    for a in list.iter_mut() {
        if hash_of(a) != Some(hash) {
            continue;
        }
        let g: String = text(a, "Gyaml").to_string();
        let Some(d) = a.get_mut("Dynamic") else {
            continue;
        };
        // A chest without the drop field gives its actor's default (banc::DEFAULT_DROPS).
        if d.get("Drop__DropActor").is_none() && crate::slice::banc::default_drop(hash).is_none() {
            continue;
        }
        d.set("Drop__DropActor", Node::Str(drop.to_string()));
        if d.get("Drop__DropActor_Substitute").is_some() {
            d.set("Drop__DropActor_Substitute", Node::Str(drop.to_string()));
        }
        d.set("GetEventName", Node::Str(event.to_string()));
        out = Some(Size::of_gyaml(&g));
    }
    out
}

pub fn route(y: &mut Byml, hash: u64, family: &str) -> Option<Size> {
    let list: &mut Vec<Node> = y.root.get_mut("Actors")?.as_arr_mut()?;
    let mut out: Option<Size> = None;
    for a in list.iter_mut() {
        if hash_of(a) != Some(hash) {
            continue;
        }
        let size: Size = Size::of_gyaml(text(a, "Gyaml"));
        let Some(d) = a.get_mut("Dynamic") else {
            continue;
        };
        d.set("GetEventName", Node::Str(event_name(family, size)));
        out = Some(size);
    }
    out
}

#[derive(Clone, Debug)]
pub struct Opts {
    pub scale: Option<(f32, i32)>,
    pub rst_from: Option<PB>,
    pub banc_from: Option<PB>,
    pub grants: bool,
}

#[derive(Clone, Debug)]
pub struct Built {
    pub report: String,
    pub files: usize,
    pub grown: usize,
}

pub fn stage(rf: &mut Romfs, used: &BS<(String, Size)>, scale: Option<(f32, i32)>, kids: &[(String, String)], keys: &[&str]) -> Res<Vec<Out>> {
    let mut outs: Vec<Out> = Vec::new();
    let mut sizes: BS<Size> = used.iter().map(|u| u.1).collect();
    if !keys.is_empty() {
        sizes.extend([Size::Small, Size::Big]);
    }
    for s in &sizes {
        let fams: Vec<&String> = used.iter().filter(|u| u.1 == *s).map(|u| &u.0).collect();
        outs.push(rewrite(rf, s.flow(), |raw: &[u8]| {
            let mut bf: Bf = Bf::parse(raw)?;
            let flow: &mut Flow = bf.flows.first_mut().ok_or(Error::Format("progressive: the flow file has no flow"))?;
            for fam in &fams {
                match guard_of(fam) {
                    Some(actor) => guard(flow, actor)?,
                    None => wrap(flow, fam)?,
                }
            }
            if !keys.is_empty() {
                crate::dungeon_credit::credit_keys(flow, keys, *s == Size::Big)?;
            }
            if let Some((factor, min)) = scale {
                scale_waits(&mut bf, factor, min);
            }
            bf.write()
        })?);
    }
    for table in [SETTING_TABLE, ANALYZED_TABLE] {
        let analyzed: bool = table == ANALYZED_TABLE;
        let by: BM<Size, Vec<(String, Option<String>)>> = {
            let mut m: BM<Size, Vec<(String, Option<String>)>> = BM::new();
            for (fam, s) in used {
                let e: Option<String> = if analyzed { None } else { Some(entry_name(fam)) };
                m.entry(*s).or_default().push((event_name(fam, *s), e));
            }
            for s in [Size::Small, Size::Big].into_iter().filter(|_| !keys.is_empty()) {
                for d in keys {
                    let e: Option<String> = if analyzed { None } else { Some(crate::dungeon_credit::entry_name(d)) };
                    m.entry(s).or_default().push((crate::dungeon_credit::event_name(d, s), e));
                }
            }
            m
        };
        outs.push(rewrite(rf, table, |raw: &[u8]| {
            let mut y: Byml = Byml::parse(raw)?;
            for (s, rows) in &by {
                add_rows(&mut y, s.template(), rows)?;
            }
            if analyzed {
                add_children(&mut y, kids);
            }
            y.write()
        })?);
    }
    Ok(outs)
}

pub fn build(romfs: &P, load: &P, name: &str, plans: &[(String, Vec<u64>)], o: &Opts) -> Res<Built> {
    let mut rf: Romfs = Romfs::open(romfs)?;
    let mut want: BM<u64, String> = BM::new();
    for (family, hashes) in plans {
        tiers(family)?;
        for h in hashes {
            want.insert(*h, family.clone());
        }
    }
    let mut used: BS<(String, Size)> = BS::new();
    let mut outs: Vec<Out> = Vec::new();
    let mut found: BS<u64> = BS::new();
    for f in banc_files(&rf)? {
        let raw: Vec<u8> = banc_raw(&mut rf, &f, o.banc_from.as_ref())?;
        let mut y: Byml = Byml::parse(&raw)?;
        let mut hit: bool = false;
        for (h, family) in &want {
            let probe: Option<Size> = {
                let g: Option<String> = y
                    .root
                    .get("Actors")
                    .and_then(|a| a.as_arr())
                    .and_then(|a| a.iter().find(|n| hash_of(n) == Some(*h)))
                    .map(|n| text(n, "Gyaml").to_string());
                g.map(|g| Size::of_gyaml(&g))
            };
            let Some(size) = probe else {
                continue;
            };
            let first: &str = crate::pregrant::chest_drop(family)?;
            if edit_chest(&mut y, *h, first, &event_name(family, size)).is_some() {
                used.insert((family.clone(), size));
                found.insert(*h);
                hit = true;
            }
        }
        if hit {
            let bytes: Vec<u8> = y.write()?;
            outs.push(banc_out(&mut rf, &f, raw.len(), &bytes)?);
        }
    }
    for h in want.keys() {
        if !found.contains(h) {
            return Err(Error::Msg(format!("no chest with hash {h} and a Drop__DropActor field")));
        }
    }
    outs.extend(stage(&mut rf, &used, o.scale, &[], &[])?);
    if o.grants {
        let d: Data = Data::load()?;
        let mut gs: Vec<Grant> = Vec::new();
        for (family, _) in plans {
            if let Some(f) = FAMILIES.iter().find(|x| x.0 == family).map(|x| x.0) {
                gs.extend(d.grants(f));
            }
        }
        outs.extend(stage_grants(&mut rf, &gs)?);
    }
    let c: Committed = commit(&mut rf, load, name, &outs, o.rst_from.as_ref())?;
    let mut report: String = c.report;
    for (fam, s) in &used {
        let _ = writeln!(report, "event {} runs {} for {} ({})", event_name(fam, *s), entry_name(fam), fam, tiers(fam)?.join(" then "));
    }
    Ok(Built { report, files: c.files, grown: c.grown })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bfevfl::Actor;

    fn actor(name: &str, queries: &[&str]) -> Actor {
        Actor {
            name: name.to_string(),
            sub: String::new(),
            arg: String::new(),
            actions: Vec::new(),
            queries: queries.iter().map(|s| s.to_string()).collect(),
            params: None,
            ep: 0,
            cut: 0,
        }
    }

    fn flow() -> Flow {
        Flow {
            name: "Dm_SY_TreasureboxSmall".to_string(),
            actors: vec![actor("GameSystemActor", &[]), actor(HOLDER, &["EventQueryIsBool"])],
            events: Vec::new(),
            entries: vec![Entry { name: ROOT_ENTRY.to_string(), subs: Vec::new(), vars: Vec::new(), main: 0, pad: 0 }],
        }
    }

    #[test]
    fn wraps_two_tiers_into_one_switch() {
        let mut f: Flow = flow();
        wrap(&mut f, "Hookshot").unwrap();
        assert_eq!(f.entries.len(), 2);
        assert_eq!(f.events.len(), 3);
        let e: &Entry = &f.entries[1];
        assert_eq!(e.name, "Success_Prog_Hookshot");
        assert_eq!(e.subs, vec![0, 1]);
        assert_eq!(e.main, 2);
        assert_eq!(f.actors[1].queries, vec!["EventQueryIsBool".to_string(), QUERY.to_string()]);
        let Kind::Switch { actor, query, cases, params } = &f.events[2].kind else {
            panic!("expected a switch");
        };
        assert_eq!((*actor, *query), (1, 1));
        assert_eq!(cases, &vec![Case { val: 0, ev: 1 }, Case { val: 1, ev: 0 }]);
        assert_eq!(params.as_ref().and_then(|c| c.get("ActorName")), Some(&Val::Str("Item_Hookshot".to_string())));
    }

    #[test]
    fn an_ocarina_chest_hands_out_the_pickup_then_adds_and_equips_the_tier() {
        let mut f: Flow = flow();
        wrap(&mut f, "Ocarina").unwrap();
        let e: &Entry = f.entries.iter().find(|e| e.name == "Success_Prog_Ocarina").unwrap();
        for (sub, (form, tier)) in e.subs.iter().zip([("Obj_Ocarina_Fairy", "Item_Ocarina_Fairy"), ("Obj_Ocarina_TimeLand", "Item_Ocarina_Time")]) {
            let Kind::Sub { next, params, .. } = &f.events[usize::from(*sub)].kind else {
                panic!("expected the Success_ call");
            };
            assert_eq!(params.as_ref().and_then(|c| c.get("ContentName")), Some(&Val::Str(form.to_string())));
            let add = &f.events[usize::from(*next)].kind;
            assert_eq!(add.params().and_then(|c| c.get("Actor")), Some(&Val::Str(tier.to_string())));
            let Kind::Action { next: equip, .. } = add else {
                panic!("expected the pouch add");
            };
            let eq = &f.events[usize::from(*equip)].kind;
            assert_eq!(eq.params().and_then(|c| c.get("GameDataName")), Some(&Val::Str(crate::ocarina_hold::EQUIP.to_string())));
            assert_eq!(eq.params().and_then(|c| c.get("Value")), Some(&Val::Str(tier.to_string())));
            assert!(matches!(eq, Kind::Action { next: NONE, .. }));
        }
    }

    #[test]
    fn wraps_three_tiers_into_a_chain() {
        let mut f: Flow = flow();
        wrap(&mut f, "Strength").unwrap();
        assert_eq!(f.events.len(), 6);
        let e: &Entry = &f.entries[1];
        assert_eq!(e.subs, vec![0, 1, 2]);
        assert_eq!(e.main, 5);
        let want: [(usize, u16, u16); 3] = [(5, 2, 4), (4, 2, 3), (3, 1, 0)];
        for (i, yes, no) in want {
            let Kind::Switch { cases, .. } = &f.events[i].kind else {
                panic!("expected a switch");
            };
            assert_eq!(cases, &vec![Case { val: 0, ev: yes }, Case { val: 1, ev: no }], "event {i}");
        }
    }

    #[test]
    fn ownership_is_checked_on_the_pouch_actor_not_the_chest_actor() {
        let mut f: Flow = flow();
        wrap(&mut f, "Strength").unwrap();
        for (i, who) in [(5usize, "Armor_Adult_Gauntlet_Gold"), (4, "Armor_Adult_Gauntlet_Silver"), (3, "Armor_Child_Gauntlet_Goron")] {
            let Kind::Switch { params, .. } = &f.events[i].kind else {
                panic!("expected a switch");
            };
            assert_eq!(params.as_ref().and_then(|c| c.get("ActorName")), Some(&Val::Str(who.to_string())));
        }
        for (fam, _) in FAMILIES.iter().map(|x| (x.0, x.1)) {
            assert_eq!(owners(fam).unwrap().len(), tiers(fam).unwrap().len(), "{fam}");
        }
    }

    #[test]
    fn a_grant_event_becomes_a_chain_that_keeps_its_place() {
        let mut f: Flow = Flow {
            name: "Dm_Goron_0030".to_string(),
            actors: vec![actor("GameSystemActor", &[])],
            events: vec![Event {
                name: "Event127".to_string(),
                kind: Kind::Sub {
                    next: NONE,
                    params: Some(Cont { items: vec![("ActorName".to_string(), Val::Str("Item_Gauntlet_Goron".to_string()))] }),
                    flow: "Dm_SY_GetInEvent".to_string(),
                    entry: "GetInEvent_Goron".to_string(),
                },
            }],
            entries: vec![Entry { name: "C10".to_string(), subs: vec![0], vars: Vec::new(), main: 0, pad: 0 }],
        };
        wrap_grant(&mut f, 0, "Event127", "ActorName", "Strength").unwrap();
        assert_eq!(f.actors.len(), 2);
        assert_eq!(f.actors[1].name, HOLDER);
        assert_eq!(f.events.len(), 6);
        assert_eq!(f.entries[0].subs, vec![1, 2, 3]);
        let Kind::Switch { cases, params, .. } = &f.events[0].kind else {
            panic!("expected a switch at the old index");
        };
        assert_eq!(cases, &vec![Case { val: 0, ev: 3 }, Case { val: 1, ev: 5 }]);
        assert_eq!(params.as_ref().and_then(|c| c.get("ActorName")), Some(&Val::Str("Armor_Adult_Gauntlet_Gold".to_string())));
        for (i, want) in [(1usize, "Item_Gauntlet_Goron"), (2, "Item_Gauntlet_Silver"), (3, "Item_Gauntlet_Gold")] {
            let Kind::Sub { next, params, flow, entry } = &f.events[i].kind else {
                panic!("expected a sub");
            };
            assert_eq!((*next, flow.as_str(), entry.as_str()), (NONE, "Dm_SY_GetInEvent", "GetInEvent_Goron"));
            assert_eq!(params.as_ref().and_then(|c| c.get("ActorName")), Some(&Val::Str(want.to_string())));
        }
        let before: Flow = f.clone();
        wrap_grant(&mut f, 0, "Event127", "ActorName", "Strength").unwrap();
        assert_eq!(f, before);
        assert!(wrap_grant(&mut f, 0, "Other", "ActorName", "Strength").is_err());
    }

    fn prize_flow(item: &str, sub: (&str, &str)) -> Flow {
        Flow {
            name: "MiniGame_HitTarget".to_string(),
            actors: vec![actor("GameSystemActor", &[])],
            events: vec![Event {
                name: "Event378".to_string(),
                kind: Kind::Sub {
                    next: 7,
                    params: Some(Cont { items: vec![("ReveiveItem".to_string(), Val::Str(item.to_string())), ("ASName".to_string(), Val::Str("Wait".to_string()))] }),
                    flow: sub.0.to_string(),
                    entry: sub.1.to_string(),
                },
            }],
            entries: vec![Entry { name: "Child_Reward".to_string(), subs: vec![0], vars: Vec::new(), main: 0, pad: 0 }],
        }
    }

    #[test]
    fn a_guarded_prize_keeps_the_vanilla_event_for_an_owner_and_gives_the_base_to_anyone_else() {
        let mut f: Flow = prize_flow("Item_BulletBag_Large", ("NpcCommon", "NpcCommon_HandOver_NpcToPlayer_NoDisplay"));
        guard_grant(&mut f, 0, "Event378", "ReveiveItem", "Slingshot", false).unwrap();
        assert_eq!(f.events.len(), 6);
        assert_eq!(f.entries[0].subs, vec![1, 2]);
        let Kind::Switch { cases, params, .. } = &f.events[0].kind else {
            panic!("expected a switch at the old index");
        };
        assert_eq!(cases, &vec![Case { val: 0, ev: 1 }, Case { val: 1, ev: 5 }]);
        assert_eq!(params.as_ref().and_then(|c| c.get("ActorName")), Some(&Val::Str("Weapon_Range_SlingShot".to_string())));
        // The base Slingshot then adds the small bullet bag and 30 seeds before going on.
        for (i, actor, next) in [(4usize, "Item_BulletBag_Small", 3u16), (3, "Item_DekuSeed", 7)] {
            let Kind::Action { next: n, params, .. } = &f.events[i].kind else {
                panic!("expected an add");
            };
            assert_eq!((params.as_ref().and_then(|c| c.get("Actor")), *n), (Some(&Val::Str(actor.to_string())), next));
        }
        let Kind::Switch { actor, query, cases, params } = &f.events[5].kind else {
            panic!("expected the pouch switch");
        };
        assert_eq!(cases, &vec![Case { val: 0, ev: 1 }, Case { val: 1, ev: 2 }]);
        assert_eq!(params.as_ref().and_then(|c| c.get("Actor")), Some(&Val::Str("Weapon_Range_SlingShot".to_string())));
        assert_eq!(params.as_ref().and_then(|c| c.get("Num")), Some(&Val::Int(1)));
        assert_eq!((f.actors[usize::from(*actor)].name.as_str(), f.actors[usize::from(*actor)].queries[usize::from(*query)].as_str()), ("PouchSystemActor", "EventQueryIsExistInPouch"));
        for (i, want) in [(1usize, "Item_BulletBag_Large"), (2, "Item_Range_SlingShot")] {
            let Kind::Sub { next, params, flow, entry } = &f.events[i].kind else {
                panic!("expected a sub");
            };
            assert_eq!((*next, flow.as_str(), entry.as_str()), (if i == 2 { 4 } else { 7 }, "NpcCommon", "NpcCommon_HandOver_NpcToPlayer_NoDisplay"));
            assert_eq!(params.as_ref().and_then(|c| c.get("ReveiveItem")), Some(&Val::Str(want.to_string())));
            assert_eq!(params.as_ref().and_then(|c| c.get("ASName")), Some(&Val::Str("Wait".to_string())));
        }
        let before: Flow = f.clone();
        guard_grant(&mut f, 0, "Event378", "ReveiveItem", "Slingshot", false).unwrap();
        assert_eq!(f, before);
        assert!(guard_grant(&mut f, 0, "Other", "ReveiveItem", "Slingshot", false).is_err());
    }

    #[test]
    fn a_plain_guard_hands_the_base_over_with_the_plain_gift_sub() {
        let mut f: Flow = prize_flow("Item_FairyBless_MPDouble", ("Dm_SY_GetInEvent", "GetInEvent_GreatFairyWisdom"));
        guard_grant(&mut f, 0, "Event378", "ReveiveItem", "Magic", true).unwrap();
        let Kind::Sub { flow, entry, params, .. } = &f.events[1].kind else {
            panic!("expected a sub");
        };
        assert_eq!((flow.as_str(), entry.as_str()), ("Dm_SY_GetInEvent", "GetInEvent_GreatFairyWisdom"));
        assert_eq!(params.as_ref().and_then(|c| c.get("ReveiveItem")), Some(&Val::Str("Item_FairyBless_MPDouble".to_string())));
        let Kind::Sub { flow, entry, params, .. } = &f.events[2].kind else {
            panic!("expected a sub");
        };
        assert_eq!((flow.as_str(), entry.as_str()), ("Dm_SY_GetInEvent", "GetInEvent_Demo_NotLookatObjectEntry"));
        assert_eq!(params.as_ref().and_then(|c| c.get("ReveiveItem")), Some(&Val::Str("Item_FairyBless_MP".to_string())));
        let Kind::Switch { params, .. } = &f.events[0].kind else {
            panic!("expected a switch");
        };
        assert_eq!(params.as_ref().and_then(|c| c.get("ActorName")), Some(&Val::Str("Item_FairyBless_MP".to_string())));
    }

    #[test]
    fn a_chain_wrapped_prize_is_left_to_its_chain() {
        let mut f: Flow = prize_flow("Item_BombBag_Medium", ("NpcCommon", "NpcCommon_HandOver_NpcToPlayer_NoDisplay"));
        wrap_grant(&mut f, 0, "Event378", "ReveiveItem", "BombBag").unwrap();
        let before: Flow = f.clone();
        guard_grant(&mut f, 0, "Event378", "ReveiveItem", "BombBag", false).unwrap();
        assert_eq!(f, before);
    }

    fn walk(f: &Flow, main: u16, owned: &[&str]) -> String {
        let mut at: u16 = main;
        for _ in 0..f.events.len() {
            match &f.events[at as usize].kind {
                Kind::Switch { cases, params, .. } => {
                    let who: Option<&Val> = params.as_ref().and_then(|c| c.get("ActorName"));
                    let has: bool = matches!(who, Some(Val::Str(s)) if owned.contains(&s.as_str()));
                    let v: u32 = if has { 0 } else { 1 };
                    at = cases.iter().find(|c| c.val == v).map(|c| c.ev).unwrap();
                }
                Kind::Sub { params, .. } => {
                    let c: &Cont = params.as_ref().unwrap();
                    let v: Option<&Val> = c.get("ContentName").or_else(|| c.get("ActorName"));
                    let Some(Val::Str(s)) = v else {
                        panic!("a give without an actor");
                    };
                    return s.clone();
                }
                _ => panic!("unexpected event"),
            }
        }
        panic!("the chain loops")
    }

    #[test]
    fn the_built_chain_follows_ownership_and_never_lowers_a_tier() {
        let cases: [(&str, &[&str], &str); 9] = [
            ("BombBag", &[], "Item_BombBag_Small"),
            ("BombBag", &["Item_BombBag_Small"], "Item_BombBag_Medium"),
            ("BombBag", &["Item_BombBag_Small", "Item_BombBag_Medium"], "Item_BombBag_Large"),
            ("BombBag", &["Item_BombBag_Medium"], "Item_BombBag_Large"),
            ("BombBag", &["Item_BombBag_Large"], "Item_BombBag_Large"),
            ("Bow", &["Item_Quiver_Medium"], "Item_Range_Bow"),
            ("Bow", &["Weapon_Range_Bow", "Item_Quiver_Large"], "Item_Quiver_Large"),
            ("Scale", &["Item_Scale_Gold"], "Item_Scale_Gold"),
            ("Hookshot", &["Item_Hookshot"], "Item_Longshot"),
        ];
        for (fam, owned, want) in cases {
            let mut f: Flow = flow();
            wrap(&mut f, fam).unwrap();
            let main: u16 = f.entries[1].main;
            assert_eq!(walk(&f, main, owned), want, "{fam} {owned:?}");
        }
    }

    #[test]
    fn the_bomb_guard_gives_bombs_only_to_a_bag_owner() {
        let cases: [(&[&str], &str); 5] = [
            (&[], NO_BAG),
            (&["Item_BombBag_Small"], "Item_BombBundle_Small"),
            (&["Item_BombBag_Medium"], "Item_BombBundle_Small"),
            (&["Item_BombBag_Large"], "Item_BombBundle_Small"),
            (&["Item_Hookshot", "Item_Bombchu"], NO_BAG),
        ];
        for (owned, want) in cases {
            let mut f: Flow = flow();
            guard(&mut f, "Item_BombBundle_Small").unwrap();
            assert_eq!(f.entries[1].name, "Success_Prog_AmmoGuard_Item_BombBundle_Small");
            let main: u16 = f.entries[1].main;
            assert_eq!(walk(&f, main, owned), want, "{owned:?}");
        }
    }

    #[test]
    fn arrow_and_seed_chests_need_their_bag() {
        let cases: [(&str, &[&str], &str); 5] = [
            ("TBox_ArrowBundle_Medium", &[], NO_BAG),
            ("TBox_ArrowBundle_Medium", &["Weapon_Range_Bow"], NO_BAG),
            ("TBox_ArrowBundle_Medium", &["Item_Quiver_Small"], "TBox_ArrowBundle_Medium"),
            ("TBox_DekuSeedBundle", &["Item_BulletBag_Large"], "TBox_DekuSeedBundle"),
            ("TBox_DekuSeedBundle", &["Item_Quiver_Small"], NO_BAG),
        ];
        for (actor, owned, want) in cases {
            let mut f: Flow = flow();
            guard(&mut f, actor).unwrap();
            let main: u16 = f.entries[1].main;
            assert_eq!(walk(&f, main, owned), want, "{actor} {owned:?}");
        }
        assert!(guard(&mut flow(), "Item_Hookshot").is_err());
    }

    #[test]
    fn an_ammo_gift_gives_a_rupee_without_a_bag_and_other_gifts_stay() {
        for (item, bag) in [("Item_BombBundle_Small", "Item_BombBag_Medium"), ("Item_Bomb", "Item_BombBag_Small"), ("TBox_ArrowBundle_Large", "Item_Quiver_Large"), ("TBox_DekuSeedBundle", "Item_BulletBag_Small")] {
            let mut f: Flow = prize_flow(item, ("Dm_SY_GetInEvent", "GetInEvent_NotTurn"));
            assert!(ammo_guard(&mut f, 0, "Event378", "ReveiveItem").unwrap());
            // Event 1 is the extra rupees of the no-bag give, so the kept give and the rupee are 2 and 3.
            assert_eq!(f.entries[0].subs, vec![2, 3]);
            let pick = |owned: &[&str]| -> (String, u16) {
                let mut at: u16 = 0;
                loop {
                    match &f.events[at as usize].kind {
                        Kind::Switch { cases, params, .. } => {
                            let has: bool = matches!(params.as_ref().and_then(|c| c.get("ActorName")), Some(Val::Str(s)) if owned.contains(&s.as_str()));
                            at = cases.iter().find(|c| c.val == if has { 0 } else { 1 }).unwrap().ev;
                        }
                        Kind::Sub { next, params, .. } => {
                            let Some(Val::Str(s)) = params.as_ref().and_then(|c| c.get("ReveiveItem")) else { panic!("no item") };
                            return (s.clone(), *next);
                        }
                        _ => panic!("unexpected event"),
                    }
                }
            };
            // No bag: the Blue Rupee, then NO_BAG_EXTRA rupees, then on to the vanilla next event.
            let (rupee, after): (String, u16) = pick(&[]);
            assert_eq!(rupee, NO_BAG, "{item}");
            assert_ne!(after, 7, "{item}");
            assert_eq!(skip_extra_rupees(&f, after), 7, "{item}");
            assert_eq!(pick(&[bag]), (item.to_string(), 7), "{item}");
            let before: Flow = f.clone();
            assert!(!ammo_guard(&mut f, 0, "Event378", "ReveiveItem").unwrap());
            assert_eq!(f, before);
        }
        let mut f: Flow = prize_flow("Item_Hookshot", ("Dm_SY_GetInEvent", "GetInEvent_NotTurn"));
        let before: Flow = f.clone();
        assert!(!ammo_guard(&mut f, 0, "Event378", "ReveiveItem").unwrap());
        assert_eq!(f, before);
        assert!(ammo_guard(&mut f, 0, "Other", "ReveiveItem").is_err());
    }

    #[test]
    fn guarding_twice_changes_nothing_and_each_actor_gets_its_own_entry() {
        let mut f: Flow = flow();
        guard(&mut f, "Item_Bomb").unwrap();
        let before: Flow = f.clone();
        guard(&mut f, "Item_Bomb").unwrap();
        assert_eq!(f, before);
        guard(&mut f, "Item_BombBundle_Small").unwrap();
        assert_eq!(f.entries.len(), 3);
        assert!(guarded("Item_Bomb") && guarded("Item_BombBundle_Small"));
        assert!(guarded("Item_BombchuBundle_Small") && !guarded(crate::bombchu_bag::ACTOR) && !guarded("Item_BombBag_Small"));
        assert_eq!(guard_of(&guard_family("Item_Bomb")), Some("Item_Bomb"));
        assert_eq!(guard_of("Hookshot"), None);
    }

    #[test]
    fn wrapping_twice_changes_nothing() {
        let mut f: Flow = flow();
        wrap(&mut f, "Hookshot").unwrap();
        let before: Flow = f.clone();
        wrap(&mut f, "Hookshot").unwrap();
        assert_eq!(f, before);
    }

    #[test]
    fn rows_land_in_sorted_order() {
        let row = |id: &str| Node::Map(vec![("EntryPointName".to_string(), Node::Str("Success".to_string())), ("__RowId".to_string(), Node::Str(id.to_string()))]);
        let mut y: Byml = Byml { root: Node::Arr(vec![row("A"), row("Dm_SY_TreasureboxSmall"), row("Z")]), version: 2, big: false };
        add_rows(&mut y, "Dm_SY_TreasureboxSmall", &[("OOTRR_Prog_Hookshot_Small".to_string(), Some("Success_Prog_Hookshot".to_string()))]).unwrap();
        let ids: Vec<String> = y.root.as_arr().unwrap().iter().map(|r| text(r, "__RowId").to_string()).collect();
        assert_eq!(ids, vec!["A", "Dm_SY_TreasureboxSmall", "OOTRR_Prog_Hookshot_Small", "Z"]);
        let r: &Node = &y.root.as_arr().unwrap()[2];
        assert_eq!(text(r, "EntryPointName"), "Success_Prog_Hookshot");
    }

    #[test]
    fn unknown_family_is_refused() {
        assert!(tiers("Sword").is_err());
        assert_eq!(Size::of_gyaml("Obj_TBox_A_02_NoBake"), Size::Big);
        assert_eq!(Size::of_gyaml("Obj_TBox_A_01"), Size::Small);
    }
}

