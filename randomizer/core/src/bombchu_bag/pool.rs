use crate::bombchu_bag::{ACTOR, ITEM};

pub const HOME: &str = "Bottom of the Well Front Center Bombable Chest";

pub fn fit_actors(item: &str, want: usize, acts: &mut Vec<String>) {
    if item == ITEM {
        acts.clear();
        acts.extend(std::iter::repeat_n(ACTOR.to_string(), want));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::explore;
    use crate::settings::{Settings, Value};
    use crate::slice::data::Data;
    use crate::slice::fill::vanilla_place;
    use crate::slice::generate::{generate_data, GenOpts, Generated};
    use crate::slice::logic::world;
    use crate::world::{Placement, World};

    #[test]
    fn the_bag_always_gets_its_own_actor() {
        let mut a: Vec<String> = vec!["TBox_Rupee_Blue".to_string()];
        fit_actors(ITEM, 2, &mut a);
        assert_eq!(a, vec![ACTOR.to_string(), ACTOR.to_string()]);
        let mut k: Vec<String> = vec!["x".to_string(), "y".to_string()];
        fit_actors("Kokiri Sword", 1, &mut k);
        assert_eq!(k.len(), 2);
    }

    #[test]
    fn the_bag_has_one_vanilla_home_and_the_vanilla_world_wins() {
        let d: Data = Data::load().unwrap();
        assert_eq!(d.vanilla_of(HOME), Some(ITEM));
        assert_eq!(d.pool_item(HOME), Some(ITEM));
        let w: World = world(&d).unwrap();
        let p: Placement = vanilla_place(&w, &d).unwrap();
        let bag = w.item(ITEM).unwrap();
        assert_eq!(w.real.iter().filter(|l| p.get(**l) == Some(bag)).count(), 1);
        assert!(explore(&w, &p, &[]).won());
    }

    #[test]
    fn an_unshuffled_home_chest_is_still_written_with_the_bag() {
        let d: Data = Data::load().unwrap();
        let mut s: Settings = Settings::default();
        s.set("shuffle_chests", Value::s("off")).unwrap();
        let g: Generated = generate_data(&d, &s, "chu home", &GenOpts::default()).unwrap();
        assert!(!g.seed_file.placements.contains_key(HOME));
        let (ts, _, _) = crate::slice::chest_targets::targets(&d, &g.seed_file).unwrap();
        assert!(ts.iter().any(|t| t.name == HOME && t.new_actor == ACTOR));
        let on: Generated = generate_data(&d, &Settings::default(), "chu home", &GenOpts::default()).unwrap();
        let placed: &String = on.seed_file.placements.get(HOME).unwrap();
        let (ts, _, _) = crate::slice::chest_targets::targets(&d, &on.seed_file).unwrap();
        assert!(ts.iter().filter(|t| t.name == HOME).all(|t| &t.new_item == placed));
    }
}
