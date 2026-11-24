





use crate::rng::Rng;

pub const SETTING: &str = "key_rings";
pub const NUM: u16 = 204;
pub const MODE_SETTING: &str = "key_rings_mode";
pub const MODE_NUM: u16 = 205;
pub const COUNT_SETTING: &str = "key_rings_count";
pub const COUNT_NUM: u16 = 206;
pub const SELECTION_SETTING: &str = "key_rings_selection";
pub const SELECTION_NUM: u16 = 207;
pub const HIDEOUT_SETTING: &str = "hideout_key_rings";
pub const HIDEOUT_NUM: u16 = 208;

pub const NOTE: &str = "Each standard dungeon's small keys become one key ring. A ring gives every key of its dungeon when it is picked up.";


#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Ring {
    pub item: &'static str,
    pub key: &'static str,
    pub dungeon: &'static str,

    pub choice: &'static str,

    pub select: &'static str,
    pub folder: &'static str,

    pub actor: &'static str,

    pub drop_item: &'static str,

    pub drop_actor: &'static str,
    pub count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    All,
    Count,
    Selection,
    Random,
}

impl Mode {
    pub fn id(self) -> &'static str {
        match self {
            Mode::All => "all",
            Mode::Count => "count",
            Mode::Selection => "selection",
            Mode::Random => "random",
        }
    }
}

pub const CHOICES: [(&str, &str); 8] = [
    ("forest_temple", "Forest Temple"),
    ("fire_temple", "Fire Temple"),
    ("water_temple", "Water Temple"),
    ("shadow_temple", "Shadow Temple"),
    ("spirit_temple", "Spirit Temple"),
    ("bottom_of_the_well", "Bottom of the Well"),
    ("gerudo_training_ground", "Gerudo Training Ground"),
    ("ganons_castle", "Ganon's Castle"),
];

pub const CHOICE_IDS: [&str; 8] = [
    "forest_temple",
    "fire_temple",
    "water_temple",
    "shadow_temple",
    "spirit_temple",
    "bottom_of_the_well",
    "gerudo_training_ground",
    "ganons_castle",
];

pub const FOLDERS: [&str; 8] = [
    "ForestTemple",
    "FireTemple",
    "WaterTemple",
    "ShadowTemple",
    "SpiritTemple",
    "BottomOfTheWell",
    "GerudoTrainingGround",
    "InsideGanonCastle",
];

pub const RINGS: [Ring; 8] = [
    Ring { item: "Small Key Ring (Forest Temple)", key: "Small Key (Forest Temple)", dungeon: "Forest Temple", choice: "forest_temple", select: "key_ring_forest_temple", folder: "ForestTemple", actor: "Item_Ring_Forest", drop_item: "Item_RingDrop_Forest", drop_actor: "Obj_RingDrop_Forest", count: 5 },
    Ring { item: "Small Key Ring (Fire Temple)", key: "Small Key (Fire Temple)", dungeon: "Fire Temple", choice: "fire_temple", select: "key_ring_fire_temple", folder: "FireTemple", actor: "Item_Ring_Fire", drop_item: "Item_RingDrop_Fire", drop_actor: "Obj_RingDrop_Fire", count: 8 },
    Ring { item: "Small Key Ring (Water Temple)", key: "Small Key (Water Temple)", dungeon: "Water Temple", choice: "water_temple", select: "key_ring_water_temple", folder: "WaterTemple", actor: "Item_Ring_Water", drop_item: "Item_RingDrop_Water", drop_actor: "Obj_RingDrop_Water", count: 6 },
    Ring { item: "Small Key Ring (Shadow Temple)", key: "Small Key (Shadow Temple)", dungeon: "Shadow Temple", choice: "shadow_temple", select: "key_ring_shadow_temple", folder: "ShadowTemple", actor: "Item_Ring_Shadow", drop_item: "Item_RingDrop_Shadow", drop_actor: "Obj_RingDrop_Shadow", count: 5 },
    Ring { item: "Small Key Ring (Spirit Temple)", key: "Small Key (Spirit Temple)", dungeon: "Spirit Temple", choice: "spirit_temple", select: "key_ring_spirit_temple", folder: "SpiritTemple", actor: "Item_Ring_Spirit", drop_item: "Item_RingDrop_Spirit", drop_actor: "Obj_RingDrop_Spirit", count: 5 },
    Ring { item: "Small Key Ring (Bottom of the Well)", key: "Small Key (Bottom of the Well)", dungeon: "Bottom of the Well", choice: "bottom_of_the_well", select: "key_ring_bottom_of_the_well", folder: "BottomOfTheWell", actor: "Item_Ring_Well", drop_item: "Item_RingDrop_Well", drop_actor: "Obj_RingDrop_Well", count: 3 },
    Ring { item: "Small Key Ring (Gerudo Training Ground)", key: "Small Key (Gerudo Training Ground)", dungeon: "Gerudo Training Ground", choice: "gerudo_training_ground", select: "key_ring_gerudo_training_ground", folder: "GerudoTrainingGround", actor: "Item_Ring_GTG", drop_item: "Item_RingDrop_GTG", drop_actor: "Obj_RingDrop_GTG", count: 9 },
    Ring { item: "Small Key Ring (Ganons Castle)", key: "Small Key (Ganons Castle)", dungeon: "Ganons Castle", choice: "ganons_castle", select: "key_ring_ganons_castle", folder: "InsideGanonCastle", actor: "Item_Ring_Ganon", drop_item: "Item_RingDrop_Ganon", drop_actor: "Obj_RingDrop_Ganon", count: 2 },
];




pub const HIDEOUT_RING: Ring = Ring {
    item: "Small Key Ring (Thieves Hideout)",
    key: "Small Key (Thieves Hideout)",
    dungeon: "Thieves Hideout",
    choice: "thieves_hideout",
    select: HIDEOUT_SETTING,
    folder: "GerudoFortress",
    actor: "Item_Ring_Hideout",
    drop_item: "Item_RingDrop_Hideout",
    drop_actor: "Obj_RingDrop_Hideout",
    count: 4,
};

pub fn all() -> Vec<&'static Ring> {
    RINGS.iter().collect()
}



pub fn all_with_hideout() -> Vec<&'static Ring> {
    let mut out: Vec<&'static Ring> = all();
    out.push(&HIDEOUT_RING);
    out
}

pub fn by_item(item: &str) -> Option<&'static Ring> {
    RINGS.iter().find(|r| r.item == item)
}

pub fn by_key(item: &str) -> Option<&'static Ring> {
    RINGS.iter().find(|r| r.key == item)
}

pub fn by_dungeon(dungeon: &str) -> Option<&'static Ring> {
    RINGS.iter().find(|r| r.dungeon == dungeon)
}

pub fn by_choice(choice: &str) -> Option<&'static Ring> {
    RINGS.iter().find(|r| r.choice == choice)
}

pub fn by_folder(folder: &str) -> Option<&'static Ring> {
    RINGS.iter().find(|r| r.folder == folder)
}

pub fn by_actor(actor: &str) -> Option<&'static Ring> {
    RINGS.iter().find(|r| r.actor == actor || r.drop_actor == actor)
}

pub fn by_drop_item(item: &str) -> Option<&'static Ring> {
    RINGS.iter().find(|r| r.drop_item == item)
}

pub fn by_drop_actor(actor: &str) -> Option<&'static Ring> {
    RINGS.iter().find(|r| r.drop_actor == actor)
}



pub fn by_item_any(item: &str) -> Option<&'static Ring> {
    by_item(item).or_else(|| (item == HIDEOUT_RING.item).then_some(&HIDEOUT_RING))
}

pub fn by_key_any(key: &str) -> Option<&'static Ring> {
    by_key(key).or_else(|| (key == HIDEOUT_RING.key).then_some(&HIDEOUT_RING))
}

pub fn by_folder_any(folder: &str) -> Option<&'static Ring> {
    by_folder(folder).or_else(|| (folder == HIDEOUT_RING.folder).then_some(&HIDEOUT_RING))
}

pub fn by_actor_any(actor: &str) -> Option<&'static Ring> {
    by_actor(actor).or_else(|| (actor == HIDEOUT_RING.actor || actor == HIDEOUT_RING.drop_actor).then_some(&HIDEOUT_RING))
}

pub fn drop_pack_any(actor: &str) -> String {
    by_actor_any(actor).map(|r| r.drop_actor.to_string()).unwrap_or_else(|| actor.to_string())
}

pub fn is_any_ring(item: &str) -> bool {
    by_item_any(item).is_some()
}


pub fn selected(ids: &[String]) -> Vec<&'static Ring> {
    RINGS.iter().filter(|r| ids.iter().any(|id| id == r.choice)).collect()
}

fn order(seed: &str, tag: &str) -> Vec<usize> {
    let mut out: Vec<usize> = (0..RINGS.len()).collect();
    Rng::from_text(&format!("{seed} key rings {tag}")).shuffle(&mut out);
    out
}


pub fn count(seed: &str, n: usize) -> Vec<&'static Ring> {
    order(seed, &format!("count {n}"))
        .into_iter()
        .take(n.min(RINGS.len()))
        .map(|i| &RINGS[i])
        .collect()
}


pub fn random(seed: &str) -> (usize, Vec<&'static Ring>) {
    let mut rng: Rng = Rng::from_text(&format!("{seed} key rings random count"));
    let n: usize = 1 + rng.below(RINGS.len() as u64) as usize;
    (n, order(seed, &format!("random {n}")).into_iter().take(n).map(|i| &RINGS[i]).collect())
}


pub fn mode_note(mode: Mode, count: usize, active: &[&'static Ring]) -> String {
    let names: Vec<&str> = active.iter().map(|r| r.dungeon).collect();
    match mode {
        Mode::All => String::new(),
        Mode::Count => format!("Key rings were given to {count} seed-randomized standard dungeons: {}.", names.join(", ")),
        Mode::Selection => format!("Key rings were given to the selected standard dungeons: {}.", names.join(", ")),
        Mode::Random => format!("Key rings were given to {count} seed-randomized standard dungeons: {}.", names.join(", ")),
    }
}

pub fn is_ring(item: &str) -> bool {
    by_item(item).is_some()
}


pub fn pack(actor: &str) -> String {
    actor.to_string()
}



pub fn drop_pack(actor: &str) -> String {
    by_actor(actor).map(|r| r.drop_actor.to_string()).unwrap_or_else(|| actor.to_string())
}

pub fn got_flag(actor: &str) -> String {
    format!("IsGet.{actor}")
}

pub fn applied_flag(folder: &str) -> String {
    format!("OOTRR_KeyRing.{folder}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_ring_has_a_unique_short_actor_and_flag() {
        let mut actors = std::collections::BTreeSet::new();
        let mut drops = std::collections::BTreeSet::new();
        let mut flags = std::collections::BTreeSet::new();
        let mut choices = std::collections::BTreeSet::new();
        let mut selects = std::collections::BTreeSet::new();
        for r in &RINGS {
            assert!(r.actor.len() <= 25, "{} is too long", r.actor);
            assert!(r.drop_actor.len() <= 25, "{} is too long", r.drop_actor);
            assert!(actors.insert(r.actor), "{} repeats an actor", r.actor);
            assert!(drops.insert(r.drop_actor), "{} repeats a drop actor", r.drop_actor);
            assert!(flags.insert(applied_flag(r.folder)), "{} repeats a flag", r.folder);
            assert!(choices.insert(r.choice), "{} repeats a choice", r.choice);
            assert!(selects.insert(r.select), "{} repeats a logic setting", r.select);
            assert!(by_item(r.item) == Some(r));
            assert!(by_key(r.key) == Some(r));
            assert!(by_dungeon(r.dungeon) == Some(r));
            assert!(by_choice(r.choice) == Some(r));
            assert!(by_folder(r.folder) == Some(r));
            assert!(by_actor(r.actor) == Some(r));
            assert!(by_actor(r.drop_actor) == Some(r));
            assert!(by_drop_item(r.drop_item) == Some(r));
            assert!(by_drop_actor(r.drop_actor) == Some(r));
            assert_eq!(pack(r.actor), r.actor);
            assert_eq!(drop_pack(r.actor), r.drop_actor);
        }
        assert_eq!(actors.len(), RINGS.len());
        assert_eq!(drops.len(), RINGS.len());
        assert_eq!(flags.len(), RINGS.len());
        assert_eq!(choices.len(), RINGS.len());
        assert_eq!(selects.len(), RINGS.len());
    }

    #[test]
    fn hideout_and_tcg_rings_are_not_standard_rings() {
        assert!(!is_ring("Small Key Ring (Thieves Hideout)"));
        assert!(!is_ring("Small Key Ring (Treasure Chest Game)"));
        assert!(is_any_ring("Small Key Ring (Thieves Hideout)"));
        assert!(!is_any_ring("Small Key Ring (Treasure Chest Game)"));
    }

    #[test]
    fn hideout_ring_is_only_reachable_through_the_manual_helpers() {
        let r: &'static Ring = &HIDEOUT_RING;
        assert!(r.actor.len() <= 25);
        assert!(r.drop_actor.len() <= 25);
        assert_eq!(r.select, HIDEOUT_SETTING);
        assert_eq!(by_item_any(r.item), Some(r));
        assert_eq!(by_key_any(r.key), Some(r));
        assert_eq!(by_folder_any(r.folder), Some(r));
        assert_eq!(by_actor_any(r.actor), Some(r));
        assert_eq!(by_actor_any(r.drop_actor), Some(r));
        assert_eq!(drop_pack_any(r.actor), r.drop_actor);
        assert_eq!(all_with_hideout().len(), RINGS.len() + 1);

        assert_eq!(by_item(r.item), None);
        assert_eq!(by_key(r.key), None);
        assert_eq!(by_choice(r.choice), None);
        assert!(!FOLDERS.contains(&r.folder));
        assert!(!all().iter().any(|x| x.item == r.item));
        assert!(!selected(&[r.choice.to_string()]).iter().any(|x| x.item == r.item));
        assert!(!count("seed", 8).iter().any(|x| x.item == r.item));
        assert!(!random("seed").1.iter().any(|x| x.item == r.item));
        assert!(!mode_note(Mode::All, 8, &all()).contains(r.dungeon));
    }

    #[test]
    fn seed_derived_subsets_are_stable_and_bounded() {
        let a: Vec<&'static Ring> = count("seed", 3);
        assert_eq!(a.len(), 3);
        assert_eq!(a, count("seed", 3));
        assert_ne!(a, count("other seed", 3));
        let (n, b): (usize, Vec<&'static Ring>) = random("seed");
        assert!((1..=RINGS.len()).contains(&n));
        assert_eq!(b.len(), n);
        assert_eq!(b, random("seed").1);
        for subset in [&a, &b] {
            let mut seen: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
            for r in subset {
                assert!(seen.insert(r.choice));
            }
        }
    }

    #[test]
    fn manual_selection_keeps_catalog_order() {
        let ids: Vec<String> = vec!["ganons_castle".into(), "forest_temple".into()];
        let got: Vec<&'static Ring> = selected(&ids);
        assert_eq!(got.iter().map(|r| r.choice).collect::<Vec<_>>(), ["forest_temple", "ganons_castle"]);
    }
}
