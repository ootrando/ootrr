pub mod actor;
pub mod ammo;
pub mod build;
pub mod catalog;
pub mod closure;
pub mod fall;
pub mod guard;
pub mod kit;
pub mod lift;
pub mod magic;
pub mod meter;
pub mod model;
pub mod place;
pub mod rows;
pub mod sizes;
pub mod tag;
pub mod write;

pub const BASE: &str = "Obj_Item_Base";
pub const DONOR: &str = "Obj_Arrow_Fire";
pub const PACK_DICT: u32 = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Style {
    Touch,
    Press,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Spec {
    pub item: String,
    pub name: String,
    pub style: Style,
    pub heap: Option<i32>,

    pub source: Option<String>,

    pub identity: Option<String>,
}

impl Spec {
    pub fn source(&self) -> &str {
        self.source.as_deref().unwrap_or(&self.item)
    }

    pub fn identity_override(&self) -> Option<&str> {
        self.identity.as_deref()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Layers {
    pub tags: Vec<String>,
    pub ammo: bool,
    pub magic: bool,
    pub model: Option<String>,


    pub pouch_from: Option<String>,

    pub ring: Option<String>,


    pub drop_ring: bool,


    pub spin: bool,
    /// With spin: how far, in millimetres, the pickup's body reaches below its origin so that the
    /// turning item model clears the ground (crate::pickup_pack::lift). The pack builds fill it in.
    pub lift: u32,
    pub own: bool,
    pub pose: Option<String>,
}

impl Layers {
    pub fn plain(&self) -> bool {
        self.tags.is_empty() && !self.ammo && !self.magic && self.model.is_none() && self.pouch_from.is_none() && self.ring.is_none() && !self.drop_ring && !self.spin && !self.own
    }

    pub fn kit_ok(&self) -> bool {
        self.tags.is_empty() && !self.ammo && !self.magic && !self.own
    }
}

pub fn pack_of(name: &str) -> String {
    format!("Pack/Actor/{name}.pack.zs")
}

pub fn default_name(item: &str) -> String {
    match item.strip_prefix("Item_") {
        Some(t) => format!("Obj_{t}"),
        None => format!("Obj_{item}"),
    }
}

pub fn is_actor_name(s: &str) -> bool {
    !s.is_empty() && s.len() <= 64 && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pickup_name_follows_the_shipped_pattern() {
        assert_eq!(default_name("Item_Hookshot"), "Obj_Hookshot");
        assert_eq!(default_name("Weapon_Shield_Mirror"), "Obj_Weapon_Shield_Mirror");
        assert_eq!(pack_of("Obj_Hookshot"), "Pack/Actor/Obj_Hookshot.pack.zs");
    }

    #[test]
    fn actor_names_cannot_carry_path_parts() {
        assert!(is_actor_name("Obj_Hookshot"));
        assert!(!is_actor_name("../Obj"));
        assert!(!is_actor_name("Obj Hookshot"));
        assert!(!is_actor_name(""));
    }

    #[test]
    fn no_layer_is_on_by_default() {
        let l: Layers = Layers::default();
        assert!(l.plain());
        assert!(!Layers { ammo: true, ..Layers::default() }.plain());
        assert!(!Layers { magic: true, ..Layers::default() }.plain());
        assert!(!Layers { spin: true, ..Layers::default() }.plain());
        assert!(!Layers { own: true, ..Layers::default() }.plain());
        assert!(!Layers { own: true, ..Layers::default() }.kit_ok());
        assert!(!Layers { tags: vec!["LetterBottle".to_string()], ..Layers::default() }.plain());
    }
}
