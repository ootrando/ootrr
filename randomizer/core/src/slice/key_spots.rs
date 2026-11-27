use std::collections::{BTreeMap as BM, HashMap as HM};
use std::fmt::Write as W;
use std::sync::{Mutex, OnceLock};

use crate::error::{Error as E, Res};
use crate::pickup_pack::catalog::{self, Entry, Status};
use crate::pickup_pack::kit::name_of;
use crate::pickup_pack::{default_name, Layers, Spec};
use crate::progressive::{family_of, tiers};
use crate::slice::data::{Data, Item};
use crate::world::{LocId, Placement, World};

pub const NEVER: [&str; 2] = ["Milk", "Buy Magic Bean"];
pub const TOP_ONLY: [&str; 2] = ["NutBag", "StickBag"];
pub const REFUSED: [&str; 2] = ["in_game_first", "not_for_placement"];
pub const AMMO: &str = "--ammo";
pub const SHIELDS: [(&str, &str); 2] = [("Deku Shield", "Obj_Shield_Deku"), ("Hylian Shield", "Obj_Shield_Hylian")];
pub const LOST: &str = "A shuffled pot, crate, flying pot, beehive, rock or grass tuft gives its item once and stays used in the save from the moment it breaks. An item you do not pick up is lost, also when it falls into a pit or lands out of reach, and a lost key item can make the seed unbeatable. Save before breaking holders that may hold key items. Experimental, untested in the game.";
pub const NOTE: &str = "Key items can sit at shuffled pickup spots. Every item whose pickup pack is built can lie on the ground, sit in a pot, a crate, a flying pot or a beehive, under a rock, in a grass tuft or at a Gold Skulltula, the seed build writes those packs into the mod. An upgrade picked up at these spots gives the next tier you do not have yet, whatever tier its model shows, and the get window names the tier you get (untested in the game). A spot that exists once per age only takes items that do no harm when found twice, such as rupees, hearts, ammo and gear that a second copy does not change.";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Spot {
    Free,
    Pot,
    Skull,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pick {
    pub content: String,
    pub pack: String,
    pub args: Vec<String>,
}

impl Pick {
    pub fn spec(&self) -> Res<(Spec, Layers)> {
        catalog::spec(&self.content, &self.args)
    }

    pub fn kit(&self) -> String {
        name_of(&self.pack)
    }

    pub fn kit_spec(&self) -> Res<Spec> {
        self.kit_with().map(|x| x.0)
    }

    pub fn kit_with(&self) -> Res<(Spec, Layers)> {
        let (s, l): (Spec, Layers) = self.spec()?;
        if !l.kit_ok() {
            return Err(E::Msg(format!("{} needs a pack layer, a skulltula kit cannot carry it", self.pack)));
        }
        Ok((Spec { name: self.kit(), ..s }, l))
    }

    pub fn hang(&self) -> String {
        crate::pickup_pack::kit::hang_of(&self.pack)
    }

    pub fn hang_with(&self) -> Res<(Spec, Layers)> {
        let (s, l): (Spec, Layers) = self.kit_with()?;
        Ok((Spec { name: self.hang(), ..s }, l))
    }
}

fn entries() -> &'static [Entry] {
    static C: OnceLock<Vec<Entry>> = OnceLock::new();
    C.get_or_init(|| catalog::load().unwrap_or_default())
}

pub fn chosen(e: &Entry) -> Option<Vec<String>> {
    let actor: bool = e.item.starts_with("Item_") || e.item.starts_with("TBox_") || e.item.starts_with("Tbox_");
    if e.flag.as_deref().is_some_and(|f| REFUSED.contains(&f)) || !actor {
        return None;
    }
    let ammo: Option<&catalog::Variant> = e.variants.iter().find(|v| v.status == Status::Built && v.args.iter().any(|a| a == AMMO));
    match ammo {
        Some(v) => Some(v.args.clone()),
        None if e.status == Status::Built => Some(e.args.clone()),
        None => None,
    }
}

pub fn pick_in(list: &[Entry], content: &str) -> Option<Pick> {
    let e: &Entry = list.iter().find(|e| e.item == content)?;
    let args: Vec<String> = chosen(e)?;
    let (s, _): (Spec, Layers) = catalog::spec(content, &args).ok()?;
    let named: bool = args.iter().any(|a| a == "--name");
    if Some(&s.name) != e.pack.as_ref() || (!named && s.name != default_name(content)) {
        return None;
    }
    Some(Pick { content: content.to_string(), pack: s.name, args })
}

fn picks() -> &'static BM<String, Pick> {
    static M: OnceLock<BM<String, Pick>> = OnceLock::new();
    M.get_or_init(|| entries().iter().filter_map(|e| pick_in(entries(), &e.item)).map(|p| (p.content.clone(), p)).collect())
}

pub const DUNGEON_PREFIX: &str = "Obj_OOTRR_";

pub fn dungeon_pick(d: &Data, item: &str) -> Option<Pick> {
    let i: &Item = d.item(item)?;
    let dungeon: &str = i.dungeon.as_deref().and_then(crate::dungeon_grant::struct_of)?;
    let tag: &str = match i.kind.as_str() {
        "SmallKey" | "HideoutSmallKey" => "SmallKey",
        "BossKey" | "GanonBossKey" => "BossKey",
        "Map" => "Map",
        "Compass" => "Compass",
        _ => return None,
    };
    let actor: String = i.actor.clone()?;
    let name: String = format!("{DUNGEON_PREFIX}{tag}_{dungeon}");
    // Pressed, so its get runs the Dm_SY_Get add that the key sweep follows
    // (crate::debug_names::key_sweep_after_add). A skulltula kit is always taken by touch.
    Some(Pick { content: actor, pack: name.clone(), args: vec!["--name".to_string(), name, "--heap".to_string(), "199928".to_string(), "--press".to_string()] })
}

/// The same pick taken by touch, as a skulltula kit must be.
pub fn touch(mut p: Pick) -> Pick {
    p.args.retain(|a| a != "--press");
    p
}

pub fn pick(content: &str) -> Option<Pick> {
    picks().get(content).cloned()
}

/// The pick a Gold Skulltula or holder kit can carry: the chosen build when its layers fit a kit,
/// otherwise the entry's plain build (no ammo, magic or tag layer).
pub fn kit_pick(content: &str) -> Option<Pick> {
    let p: Pick = pick(content)?;
    if p.kit_spec().is_ok() {
        return Some(p);
    }
    let e: &Entry = entries().iter().find(|e| e.item == content)?;
    if !e.status.ready() {
        return None;
    }
    let plain: Pick = Pick { content: content.to_string(), pack: p.pack.clone(), args: e.args.clone() };
    plain.kit_spec().is_ok().then_some(plain)
}

/// The pick that builds the pickup pack `pack`.
pub fn by_pack(pack: &str) -> Option<Pick> {
    picks().values().find(|p| p.pack == pack).cloned()
}

fn family(i: &Item) -> Option<&'static str> {
    family_of(&i.tiers)
}

pub fn contents(d: &Data, item: &str) -> Vec<String> {
    let Some(i) = d.item(item) else {
        return Vec::new();
    };
    let kind: bool = matches!(i.kind.as_str(), "Item" | "DungeonReward" | "Token") || crate::slice::song_spots::is_song_item(i);
    if NEVER.contains(&item) || !kind || i.dungeon.is_some() {
        return Vec::new();
    }
    if item == crate::bombchu_bag::ITEM {
        return vec![crate::bombchu_bag::BUNDLE.to_string()];
    }
    match family(i) {
        Some(f) if TOP_ONLY.contains(&f) => i.tiers.last().cloned().into_iter().collect(),
        Some(_) => i.tiers.clone(),
        None => i.actor.clone().into_iter().collect(),
    }
}

pub fn placeable(d: &Data, item: &str, at: Spot) -> bool {
    static M: OnceLock<Mutex<HM<(String, Spot), bool>>> = OnceLock::new();
    let m: &Mutex<HM<(String, Spot), bool>> = M.get_or_init(|| Mutex::new(HM::new()));
    let k: (String, Spot) = (item.to_string(), at);
    if let Some(v) = m.lock().ok().and_then(|g| g.get(&k).copied()) {
        return v;
    }
    let v: bool = judge(d, item, at);
    if let Ok(mut g) = m.lock() {
        g.insert(k, v);
    }
    v
}

fn judge(d: &Data, item: &str, at: Spot) -> bool {
    if at != Spot::Skull && crate::dungeon_packs::of(d, item).is_some() {
        return true;
    }
    if let Some(p) = dungeon_pick(d, item) {
        return at == Spot::Free || touch(p).kit_spec().is_ok();
    }
    let cs: Vec<String> = contents(d, item);
    !cs.is_empty()
        && cs.iter().all(|c| if at == Spot::Free { pick(c).is_some() } else { kit_pick(c).is_some() })
}

pub fn shield(item: &str) -> Option<&'static str> {
    SHIELDS.iter().find(|s| s.0 == item).map(|s| s.1)
}

pub fn top_only(d: &Data, item: &str) -> bool {
    d.family(item).is_some_and(|f| TOP_ONLY.contains(&f))
}

pub fn resolve(d: &Data, item: &str, tier: Option<&str>) -> Res<Pick> {
    let i: &Item = d.item(item).ok_or_else(|| E::Msg(format!("unknown item {item}")))?;
    if let Some(p) = crate::dungeon_packs::of_item(i) {
        return Ok(p.pick());
    }
    if item == crate::bombchu_bag::ITEM {
        return Ok(crate::bombchu_bag::pick());
    }
    let content: String = match (tier, family(i)) {
        (Some(t), _) => t.to_string(),
        (None, Some(f)) if TOP_ONLY.contains(&f) => i.tiers.last().cloned().unwrap_or_default(),
        (None, Some(_)) => return Err(E::Msg(format!("{item} is an upgrade, the tier order names no tier for its spot"))),
        (None, None) => i.actor.clone().unwrap_or_default(),
    };
    if !contents(d, item).contains(&content) {
        return Err(E::Msg(format!("{item} as {content} has no pickup pack this build can place yet")));
    }
    pick(&content).ok_or_else(|| E::Msg(format!("{item} has no pickup pack this build can place yet ({content})")))
}

/// resolve for a kit spot (a Gold Skulltula drop or a holder): the kit-capable build.
pub fn resolve_kit(d: &Data, item: &str, tier: Option<&str>) -> Res<Pick> {
    if let Some(p) = dungeon_pick(d, item) {
        let p: Pick = touch(p);
        p.kit_spec()?;
        return Ok(p);
    }
    let p: Pick = resolve(d, item, tier)?;
    if item == crate::bombchu_bag::ITEM {
        p.kit_spec()?;
        return Ok(p);
    }
    kit_pick(&p.content).ok_or_else(|| E::Msg(format!("{item} has no pickup pack a kit can carry ({})", p.content)))
}

pub fn top(f: &str) -> Option<&'static str> {
    tiers(f).ok().and_then(|t| t.last().copied())
}

pub fn widened(d: &Data, name: &str, item: &str) -> bool {
    if crate::slice::skulls::is_skull(d, name) {
        return !crate::slice::skulls::shared().is_some_and(|t| crate::slice::skulls::pickup(t, d, item).is_some());
    }
    if d.pots.has(name) {
        return crate::pot_slice::pickup::of(item).is_none();
    }
    d.cat.by_name(name).is_some_and(crate::slice::pickups::is_world) && crate::slice::pickups::world_actor(item).is_none()
}

pub fn class(d: &Data, name: &str) -> Option<&'static str> {
    if crate::slice::skulls::is_skull(d, name) {
        return Some("Gold Skulltula drop, a kit pickup");
    }
    if let Some(l) = d.pots.get(name) {
        return Some(match l.kind.as_str() {
            "crate" => "crate, given once, lost if not picked up",
            "flying_pot" => "flying pot, given once, lost if not picked up",
            "beehive" => "beehive, given once, lost if not picked up",
            "rock" => "rock, given once, lost if not picked up",
            "grass" => "grass, given once, lost if not picked up",
            _ => "pot, given once, lost if not picked up",
        });
    }
    if d.cat.by_name(name).is_some_and(crate::slice::pickups::is_world) {
        return Some("on the ground, given once");
    }
    None
}

pub fn spoiler(d: &Data, w: &World, free: &[LocId], got: &Placement, assign: &BM<String, String>) -> String {
    let mut lines: Vec<String> = Vec::new();
    let mut pots: bool = false;
    for l in free {
        let name: &str = &w.locs[*l].name;
        let Some(c) = class(d, name) else {
            continue;
        };
        let Some(i) = got.get(*l) else {
            continue;
        };
        let item: &str = &w.items.names[i];
        if d.vanilla_of(name) == Some(item) {
            continue;
        }
        let held: bool = d.pots.has(name);
        let actor: String = match resolve(d, item, assign.get(name).map(|s| s.as_str())) {
            Ok(p) if crate::slice::skulls::is_skull(d, name) => p.hang(),
            Ok(p) => p.pack.clone(),
            Err(_) if held && crate::pot_slice::pickup::of(item).is_some_and(|a| !crate::pot_slice::pickup::refill(a)) => crate::pickup_pack::fall::dropped(crate::pot_slice::pickup::of(item).unwrap_or("")),
            Err(_) => continue,
        };
        pots |= held;
        lines.push(format!("  {name}: {} as {actor} ({c})", crate::spoiler::shown(item)));
    }
    let mut out: String = String::new();
    if lines.is_empty() {
        return out;
    }
    let _ = writeln!(out, "Key items at pickup spots, {}", lines.len());
    for x in lines {
        let _ = writeln!(out, "{x}");
    }
    if pots {
        let _ = writeln!(out, "{LOST}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d() -> Data {
        Data::load().unwrap()
    }

    #[test]
    fn the_catalog_names_a_pack_for_each_built_key_item_and_bow_takes_the_ammo_layer() {
        let hook: Pick = pick("Item_Hookshot").unwrap();
        assert_eq!(hook.pack, "Obj_Hookshot");
        assert_eq!(pick("Item_Longshot").unwrap().pack, "Obj_Longshot");
        let bow: Pick = pick("Item_Range_Bow").unwrap();
        assert!(bow.args.iter().any(|a| a == AMMO), "{:?}", bow.args);
        assert!(bow.spec().unwrap().1.ammo);
        assert!(pick("Item_Range_SlingShot").unwrap().spec().unwrap().1.ammo);
        assert_eq!(pick("Item_Bottle_Letter").unwrap().pack, "Obj_Bottle_LetterLand");
        // Goron's Bracelet saves under its armor actor now, so its in_game_first flag is lifted.
        assert_eq!(pick("Item_Gauntlet_Goron").map(|p| p.pack), Some("Obj_Gauntlet_Goron".to_string()));
        assert!(pick("Item_NutBag_Medium").is_none());
        assert!(pick("Item_NutBag_Large").is_some());
        assert!(pick("Item_Bombchu").is_none());
        // Marked built so a Gold Skulltula can drop Bombchus (5).
        assert_eq!(pick("Item_BombchuBundle_Small").map(|p| p.pack), Some("Obj_BombchuBundle_Small".to_string()));
        assert!(pick("Item_Shield_Deku").is_none());
        // The placement sweep packs: renamed ones keep their own name, chest ammo bundles take TBox_ items.
        assert_eq!(pick("Item_Token_Gold").map(|p| p.pack), Some("Obj_Token_GoldLand".to_string()));
        assert_eq!(pick("Item_Arrow_Fire").map(|p| p.pack), Some("Obj_Arrow_FireLand".to_string()));
        assert_eq!(pick("TBox_ArrowBundle_Medium").map(|p| p.pack), Some("Obj_TBox_ArrowBundle_Medium".to_string()));
        assert_eq!(pick("Item_Medal_Light").map(|p| p.pack), Some("Obj_Medal_Light".to_string()));
        assert_eq!(pick("Item_Ocarina_Fairy").map(|p| p.pack), Some("Obj_Ocarina_Fairy".to_string()));
        assert_eq!(pick("Item_Ocarina_Time").map(|p| p.pack), Some("Obj_Ocarina_TimeLand".to_string()));
        assert!(pick("Item_Nothing").is_none());
    }

    #[test]
    fn every_pick_maps_item_to_obj_unless_the_catalog_renames_it() {
        let mut n: usize = 0;
        for e in catalog::load().unwrap() {
            let Some(p) = pick(&e.item) else {
                continue;
            };
            n += 1;
            assert_eq!(Some(&p.pack), e.pack.as_ref(), "{}", e.item);
            if !p.args.iter().any(|a| a == "--name") {
                assert_eq!(p.pack, default_name(&e.item), "{}", e.item);
            }
            assert_eq!(p.kit(), format!("{}_GS", p.pack));
        }
        assert!(n >= 50, "{n}");
    }

    #[test]
    fn a_flagged_or_unbuilt_entry_is_refused_and_a_renamed_pack_must_say_so() {
        let one = |status: &str, flag: &str, pack: &str, args: &str| {
            let t: String = format!(r#"{{"packs":[{{"group":"A","item":"Item_X","pack":"{pack}","args":[{args}],"status":"{status}","identity":"Item_X","flag":{flag},"notes":""}}]}}"#);
            pick_in(&catalog::parse(&t).unwrap(), "Item_X")
        };
        assert!(one("built", "null", "Obj_X", "").is_some());
        assert!(one("dry_ok", "null", "Obj_X", "").is_none());
        assert!(one("built", "\"in_game_first\"", "Obj_X", "").is_none());
        assert!(one("built", "\"not_for_placement\"", "Obj_X", "").is_none());
        assert!(one("built", "null", "Obj_Y", "").is_none());
        assert!(one("built", "null", "Obj_Y", r#""--name","Obj_Y""#).is_some());
    }

    #[test]
    fn logic_items_resolve_to_their_tier_packs_with_the_nut_and_stick_top_tier_rule() {
        let d: Data = d();
        for item in ["Boomerang", "Megaton Hammer", "Lens of Truth", "Mirror Shield", "Light Arrows", "Bottle", "Iron Boots", "Hover Boots", "Progressive Hookshot", "Bomb Bag", "Bow"] {
            for at in [Spot::Free, Spot::Pot] {
                assert!(placeable(&d, item, at) || (at == Spot::Pot && crate::pot_slice::pickup::holds(item)), "{item} {at:?}");
            }
        }
        // A skulltula kit cannot carry the ammo layer, so the Bow and Slingshot kits fall back to the
        // plain build (the starter bag hook adds the bag after the pickup's get).
        assert!(placeable(&d, "Bow", Spot::Skull));
        assert!(placeable(&d, "Slingshot", Spot::Skull));
        assert!(pick("Item_Range_Bow").unwrap().kit_spec().is_err());
        assert!(!kit_pick("Item_Range_Bow").unwrap().args.iter().any(|a| a == AMMO));
        assert_eq!(resolve_kit(&d, "Bow", Some("Item_Range_Bow")).unwrap().hang(), "Obj_Range_Bow_GH");
        // The letter's built entry carries the LetterBottle tag, which a kit cannot carry, so it stays off skulltulas.
        assert!(!placeable(&d, "Rutos Letter", Spot::Skull));
        assert!(placeable(&d, "Rutos Letter", Spot::Free));
        assert!(placeable(&d, "Progressive Hookshot", Spot::Skull));
        // Goron's Bracelet has a pack now, so every strength tier can lie anywhere.
        for at in [Spot::Free, Spot::Pot, Spot::Skull] {
            assert!(placeable(&d, "Progressive Strength Upgrade", at), "{at:?}");
        }
        assert!(placeable(&d, "Ocarina", Spot::Free) && placeable(&d, "Ocarina", Spot::Skull));
        assert!(!placeable(&d, "Milk", Spot::Free));
        for at in [Spot::Free, Spot::Pot, Spot::Skull] {
            assert!(placeable(&d, "Small Key (Forest Temple)", at), "{at:?}");
        }
        let key: Pick = resolve(&d, "Small Key (Forest Temple)", None).unwrap();
        assert_eq!(key.pack, "Obj_SmallKey_ForestTemple");
        assert!(key.args.iter().any(|a| a == "--own"));
        let kit: Pick = resolve_kit(&d, "Small Key (Forest Temple)", None).unwrap();
        assert_eq!(kit.pack, format!("{DUNGEON_PREFIX}SmallKey_ForestTemple"));
        assert!(!kit.args.iter().any(|a| a == "--press"));
        assert!(!placeable(&d, "Rupees (5)", Spot::Free));
        assert!(!placeable(&d, "Bunny Hood", Spot::Free));
        assert!(placeable(&d, "Deku Nut Capacity", Spot::Pot));
        assert_eq!(contents(&d, "Deku Nut Capacity"), vec!["Item_NutBag_Large".to_string()]);
        assert_eq!(resolve(&d, "Deku Stick Capacity", None).unwrap().pack, "Obj_StickBag_Large");
        assert_eq!(resolve(&d, "Progressive Hookshot", Some("Item_Longshot")).unwrap().pack, "Obj_Longshot");
        assert!(resolve(&d, "Progressive Hookshot", None).is_err());
        assert!(resolve(&d, "Deku Nut Capacity", Some("Item_NutBag_Medium")).is_err());
        assert_eq!(resolve(&d, "Bomb Bag", Some("Item_BombBag_Small")).unwrap().pack, "Obj_BombBag_Small");
        assert_eq!(resolve(&d, "Boomerang", None).unwrap().kit(), "Obj_Boomerang_GS");
        assert!(top_only(&d, "Deku Nut Capacity") && !top_only(&d, "Bomb Bag"));
        assert_eq!(top("NutBag"), Some("Item_NutBag_Large"));
        assert_eq!(shield("Deku Shield"), Some("Obj_Shield_Deku"));
    }

    #[test]
    fn a_skulltula_kit_keeps_the_heap_and_refuses_a_layer() {
        let k: Spec = pick("Item_Hookshot").unwrap().kit_spec().unwrap();
        assert_eq!(k.name, "Obj_Hookshot_GS");
        assert_eq!(k.item, "Item_Hookshot");
        assert_eq!(k.heap, Some(199928));
        assert!(pick("Item_Range_Bow").unwrap().kit_spec().is_err());
    }
}

