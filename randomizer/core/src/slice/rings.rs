use crate::error::{Error as E, Res};
use crate::logic_items::ItemId;
use crate::slice::data::Data;
use crate::start_items::pool::JUNK;
use crate::slice::support::Plan;
use crate::world::{LocId, Placement, World};



pub fn replace(
    d: &Data,
    w: &World,
    p: &Plan,
    base: &mut Placement,
    free: &[LocId],
    ok: &dyn Fn(ItemId, LocId) -> bool,
    notes: &mut Vec<String>,
) -> Res<()> {
    replace_with(d, w, p, base, free, ok, notes, &[])
}



pub fn replace_with(
    d: &Data,
    w: &World,
    p: &Plan,
    base: &mut Placement,
    free: &[LocId],
    ok: &dyn Fn(ItemId, LocId) -> bool,
    notes: &mut Vec<String>,
    extra: &[&'static crate::key_rings::Ring],
) -> Res<()> {
    if !p.rings {
        return Ok(());
    }
    if p.active_rings.is_empty() {
        return Err(E::Msg("key rings: the active dungeons were not derived from a seed".to_string()));
    }
    let junk: ItemId = w.item(JUNK).ok_or_else(|| E::Msg(format!("{JUNK} is not in the logic item list")))?;
    let mut changes: Vec<String> = Vec::new();
    for r in &p.active_rings {
        let key: ItemId = w.item(r.key).ok_or_else(|| E::Msg(format!("{} is not in the logic item list", r.key)))?;
        let ring: ItemId = w.item(r.item).ok_or_else(|| E::Msg(format!("{} is not in the logic item list", r.item)))?;
        let old: Vec<LocId> = w.real.iter().copied().filter(|l| base.get(*l) == Some(key)).collect();
        if old.is_empty() {
            return Err(E::Msg(format!("key rings: {} has no small keys to replace", r.dungeon)));
        }
        let primary: Option<LocId> = old.iter().copied().find(|l| free.contains(l) && ok(ring, *l));
        let spare: Option<LocId> = primary.or_else(|| {
            free.iter().copied().find(|l| {
                ok(ring, *l)
                    && base.get(*l).is_some_and(|i| {
                        i == junk || d.item(&w.items.names[i]).is_some_and(|it| !it.adv)
                    })
            })
        });
        let Some(at): Option<LocId> = spare else {
            return Err(E::Msg(format!("key rings: no free spot can hold {}", r.item)));
        };
        for l in &old {
            base.put(*l, junk);
        }
        base.put(at, ring);
        changes.push(format!("{} ({} keys)", r.dungeon, r.count));
    }
    for r in extra {
        if p.active_rings.contains(r) {
            continue;
        }
        let ring: ItemId = w.item(r.item).ok_or_else(|| E::Msg(format!("{} is not in the logic item list", r.item)))?;
        let at: LocId = free
            .iter()
            .copied()
            .find(|l| ok(ring, *l) && base.get(*l).is_some_and(|i| i == junk || d.item(&w.items.names[i]).is_some_and(|it| !it.adv)))
            .ok_or_else(|| E::Msg(format!("key rings: no free spot can hold the plandoed {}", r.item)))?;
        base.put(at, ring);
        changes.push(format!("{} ({} keys, plandoed)", r.dungeon, r.count));
    }
    notes.push(format!(
        "Each standard dungeon's small keys were replaced by one key ring: {}. The former key slots hold {JUNK} unless one holds its ring. A ring named by plando keeps its dungeon's small keys shuffled.",
        changes.join(", ")
    ));
    Ok(())
}



pub fn replace_hideout(
    d: &Data,
    w: &World,
    p: &Plan,
    base: &mut Placement,
    free: &[LocId],
    ok: &dyn Fn(ItemId, LocId) -> bool,
    notes: &mut Vec<String>,
) -> Res<()> {
    if !p.hideout_ring {
        return Ok(());
    }
    let r: &'static crate::key_rings::Ring = &crate::key_rings::HIDEOUT_RING;
    let junk: ItemId = w.item(JUNK).ok_or_else(|| E::Msg(format!("{JUNK} is not in the logic item list")))?;
    let key: ItemId = w.item(r.key).ok_or_else(|| E::Msg(format!("{} is not in the logic item list", r.key)))?;
    let ring: ItemId = w.item(r.item).ok_or_else(|| E::Msg(format!("{} is not in the logic item list", r.item)))?;
    let old: Vec<LocId> = w.real.iter().copied().filter(|l| base.get(*l) == Some(key)).collect();
    if old.len() != r.count as usize {
        return Err(E::Msg(format!("key rings: Hideout has {} small keys to replace, expected {}", old.len(), r.count)));
    }
    let primary: Option<LocId> = old.iter().copied().find(|l| free.contains(l) && ok(ring, *l));
    let spare: Option<LocId> = primary.or_else(|| {
        free.iter().copied().find(|l| {
            ok(ring, *l)
                && base.get(*l).is_some_and(|i| i == junk || d.item(&w.items.names[i]).is_some_and(|it| !it.adv))
        })
    });
    let Some(at): Option<LocId> = spare else {
        return Err(E::Msg(format!("key rings: no free spot can hold {}", r.item)));
    };
    for l in &old {
        base.put(*l, junk);
    }
    base.put(at, ring);
    notes.push(format!(
        "The four Thieves' Hideout small keys were replaced by one Hideout key ring. The former key slots hold {JUNK} unless one holds its ring."
    ));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::Res;
    use crate::logic_items::ItemId;
    use crate::search::Search;
    use crate::settings::{Settings, Value};
    use crate::slice::generate::allowed;
    use crate::slice::logic::world_with;
    use crate::slice::pick::pick;
    use crate::start_items::pool;
    use crate::slice::support::{plan, plan_with_seed};

    fn setup() -> Res<(Data, World, Plan, Placement, crate::slice::pick::Pick)> {
        let mut s: Settings = Settings::default();
        s.set(crate::key_rings::SETTING, Value::Bool(true))?;
        s.set("shuffle_smallkeys", Value::s("keysanity"))?;
        let d: Data = Data::load()?;
        let p: Plan = plan(&s)?;
        let w: World = world_with(&d, &p.logic())?;
        let v: Placement = crate::slice::fill::vanilla_place(&w, &d)?;
        let pk: crate::slice::pick::Pick = pick(&d, &w, &p);
        Ok((d, w, p, v, pk))
    }

    #[test]
    fn a_ring_grants_its_full_key_count_in_logic() {
        let (_, w, _, _, _): (Data, World, Plan, Placement, crate::slice::pick::Pick) = setup().unwrap();
        let mut s: Search = Search::new(&w);
        for r in crate::key_rings::RINGS {
            let key: ItemId = w.item(r.key).unwrap();
            let ring: ItemId = w.item(r.item).unwrap();
            let before: u32 = s.cnt[key];
            s.add(ring, 1);
            assert_eq!(s.cnt[key], before + r.count, "{}", r.item);
            s.remove(ring);
            assert_eq!(s.cnt[key], before, "{}", r.item);
        }
    }

    #[test]
    fn one_ring_replaces_each_dungeons_small_keys() {
        let (d, w, p, v, pk): (Data, World, Plan, Placement, crate::slice::pick::Pick) = setup().unwrap();
        let (mut base, _): (Placement, Vec<String>) = pool::base(&d, &w, &p, &v, &pk.free).unwrap();
        let ok = |it: ItemId, l: LocId| allowed(&d, &w, &p, it, l);
        let mut notes: Vec<String> = Vec::new();
        replace(&d, &w, &p, &mut base, &pk.free, &ok, &mut notes).unwrap();
        for r in crate::key_rings::RINGS {
            let key: ItemId = w.item(r.key).unwrap();
            let ring: ItemId = w.item(r.item).unwrap();
            assert_eq!(base.item.iter().filter(|i| **i == Some(key)).count(), 0, "{}", r.key);
            assert_eq!(base.item.iter().filter(|i| **i == Some(ring)).count(), 1, "{}", r.item);
        }
        assert_eq!(base.item.len(), v.item.len());
        assert_eq!(notes.len(), 1);
        for r in crate::key_rings::RINGS {
            assert!(notes[0].contains(&format!("{} ({} keys)", r.dungeon, r.count)), "{}", notes[0]);
        }
    }

    #[test]
    fn only_selected_dungeons_lose_their_individual_small_keys() {
        let mut s: Settings = Settings::default();
        s.set("shuffle_smallkeys", Value::s("keysanity")).unwrap();
        s.set(crate::key_rings::SETTING, Value::Bool(true)).unwrap();
        s.set(crate::key_rings::MODE_SETTING, Value::s("selection")).unwrap();
        s.set(crate::key_rings::SELECTION_SETTING, Value::set(&["forest_temple"])).unwrap();
        let d: Data = Data::load().unwrap();
        let p: Plan = plan_with_seed(&s, Some("seed")).unwrap();
        let w: World = world_with(&d, &p.logic()).unwrap();
        let forest: &crate::key_rings::Ring = crate::key_rings::by_choice("forest_temple").unwrap();
        let fire: &crate::key_rings::Ring = crate::key_rings::by_choice("fire_temple").unwrap();
        let forest_ring: ItemId = w.item(forest.item).unwrap();
        assert_eq!(w.ring_key(forest_ring), Some((w.item(forest.key).unwrap(), forest.count as u32)));
        let fire_ring: ItemId = w.item(fire.item).unwrap();
        assert!(w.ring_key(fire_ring).is_none());

        let v: Placement = crate::slice::fill::vanilla_place(&w, &d).unwrap();
        let pk: crate::slice::pick::Pick = pick(&d, &w, &p);
        let (mut base, _): (Placement, Vec<String>) = pool::base(&d, &w, &p, &v, &pk.free).unwrap();
        let ok = |it: ItemId, l: LocId| allowed(&d, &w, &p, it, l);
        let mut notes: Vec<String> = Vec::new();
        replace(&d, &w, &p, &mut base, &pk.free, &ok, &mut notes).unwrap();
        let forest_key: ItemId = w.item(forest.key).unwrap();
        let fire_key: ItemId = w.item(fire.key).unwrap();
        assert_eq!(base.item.iter().filter(|i| **i == Some(forest_ring)).count(), 1);
        assert_eq!(base.item.iter().filter(|i| **i == Some(forest_key)).count(), 0);
        assert!(base.item.iter().any(|i| *i == Some(fire_key)));
        assert!(notes[0].contains("Forest Temple (5 keys)"));
        assert!(!notes[0].contains("Fire Temple"));
    }

    #[test]
    fn rings_are_left_alone_when_the_setting_is_off() {
        let mut s: Settings = Settings::default();
        s.set("shuffle_smallkeys", Value::s("keysanity")).unwrap();
        let d: Data = Data::load().unwrap();
        let p: Plan = plan(&s).unwrap();
        let w: World = world_with(&d, &p.logic()).unwrap();
        let v: Placement = crate::slice::fill::vanilla_place(&w, &d).unwrap();
        let pk: crate::slice::pick::Pick = pick(&d, &w, &p);
        let (mut base, _): (Placement, Vec<String>) = pool::base(&d, &w, &p, &v, &pk.free).unwrap();
        let original: Vec<Option<ItemId>> = base.item.clone();
        let ok = |it: ItemId, l: LocId| allowed(&d, &w, &p, it, l);
        let mut notes: Vec<String> = Vec::new();
        replace(&d, &w, &p, &mut base, &pk.free, &ok, &mut notes).unwrap();
        assert_eq!(base.item, original);
        assert!(notes.is_empty());
    }

    fn setup_hideout() -> Res<(Data, World, Plan, Placement, crate::slice::pick::Pick)> {
        let mut s: Settings = Settings::default();
        s.set("gerudo_fortress", Value::s("open"))?;
        s.set("shuffle_hideoutkeys", Value::s("keysanity"))?;
        s.set(crate::key_rings::HIDEOUT_SETTING, Value::Bool(true))?;
        let d: Data = Data::load()?;
        let p: Plan = plan(&s)?;
        let w: World = world_with(&d, &p.logic())?;
        let v: Placement = crate::slice::fill::vanilla_place(&w, &d)?;
        let pk: crate::slice::pick::Pick = pick(&d, &w, &p);
        Ok((d, w, p, v, pk))
    }

    #[test]
    fn a_hideout_ring_grants_its_full_key_count_in_logic() {
        let (_, w, p, _, _): (Data, World, Plan, Placement, crate::slice::pick::Pick) = setup_hideout().unwrap();
        let r: &'static crate::key_rings::Ring = &crate::key_rings::HIDEOUT_RING;
        assert_eq!(p.all_rings().len(), 1);
        assert!(p.active_rings.is_empty());
        let mut s: Search = Search::new(&w);
        let key: ItemId = w.item(r.key).unwrap();
        let ring: ItemId = w.item(r.item).unwrap();
        let before: u32 = s.cnt[key];
        s.add(ring, 1);
        assert_eq!(s.cnt[key], before + r.count);
        s.remove(ring);
        assert_eq!(s.cnt[key], before);
    }

    #[test]
    fn the_hideout_ring_replaces_four_hideout_keys() {
        let (d, w, p, v, pk): (Data, World, Plan, Placement, crate::slice::pick::Pick) = setup_hideout().unwrap();
        let (mut base, _): (Placement, Vec<String>) = pool::base(&d, &w, &p, &v, &pk.free).unwrap();
        let ok = |it: ItemId, l: LocId| allowed(&d, &w, &p, it, l);
        let mut notes: Vec<String> = Vec::new();
        replace_hideout(&d, &w, &p, &mut base, &pk.free, &ok, &mut notes).unwrap();
        let r: &'static crate::key_rings::Ring = &crate::key_rings::HIDEOUT_RING;
        let key: ItemId = w.item(r.key).unwrap();
        let ring: ItemId = w.item(r.item).unwrap();
        assert_eq!(base.item.iter().filter(|i| **i == Some(key)).count(), 0);
        assert_eq!(base.item.iter().filter(|i| **i == Some(ring)).count(), 1);
        assert_eq!(base.item.len(), v.item.len());
        assert_eq!(notes.len(), 1);
        assert!(notes[0].contains("Thieves' Hideout"));
    }

    #[test]
    fn the_hideout_ring_is_untouched_when_its_toggle_is_off() {
        let mut s: Settings = Settings::default();
        s.set("gerudo_fortress", Value::s("open")).unwrap();
        s.set("shuffle_hideoutkeys", Value::s("keysanity")).unwrap();
        let d: Data = Data::load().unwrap();
        let p: Plan = plan(&s).unwrap();
        let w: World = world_with(&d, &p.logic()).unwrap();
        let v: Placement = crate::slice::fill::vanilla_place(&w, &d).unwrap();
        let pk: crate::slice::pick::Pick = pick(&d, &w, &p);
        let (mut base, _): (Placement, Vec<String>) = pool::base(&d, &w, &p, &v, &pk.free).unwrap();
        let original: Vec<Option<ItemId>> = base.item.clone();
        let ok = |it: ItemId, l: LocId| allowed(&d, &w, &p, it, l);
        let mut notes: Vec<String> = Vec::new();
        replace_hideout(&d, &w, &p, &mut base, &pk.free, &ok, &mut notes).unwrap();
        assert_eq!(base.item, original);
        assert!(notes.is_empty());
    }
}
