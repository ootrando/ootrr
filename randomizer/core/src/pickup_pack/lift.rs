//! Lifts a pickup's item model off the ground.
//!
//! Item models are centred on their origin (Link holds them there), and the turn (actor::SPIN)
//! also tips them, so a pickup whose origin sits on the ground shows half its model below it.
//! Pickups do not rest on their physics body (moving the body changed nothing in the game):
//!
//! - A dropped pickup (a Gold Skulltula's, a pot's) follows the drop bounce curve until its ground
//!   check, a ray from 0.1 m above the origin and GroundCheckLength (0.11 m) long, meets the
//!   ground (Obj_Item_Base's PerimeterAnalyserParam, detector HitGround). The pickup gets its own
//!   copy with the ray longer by the lift, so it lands that far up ([`ground`]).
//! - A pickup placed in a map stays where it stands (soft fixed placement), so the map edit
//!   raises its position by the lift (slice::banc_edit, [`for_pack`]).
//!
//! The lift is the farthest the model reaches from its origin along any axis ([`of_row`], from
//! the model's ModelAabbMin and ModelAabbMax), up to [`CAP`].

use crate::byml::{Byml, Node};
use crate::error::{Error as E, Res};
use crate::pickup_pack::actor::Made;
use crate::pickup_pack::closure::{chain, local, norm, Pool, COMPONENTS};
use crate::pickup_pack::{Layers, BASE};
use crate::romfs::Romfs;

pub const PERIMETER_REF: &str = "PerimeterAnalyserRef";
/// The highest lift, in millimetres: the Megaton Hammer reaches 1.45 m along its handle.
pub const CAP: u32 = 600;
const DETECTORS: &str = "DetectorList";
const GROUND: &str = "HitGround";
const CHECKS: &str = "GroundCheckParams";
const LENGTH: &str = "GroundCheckLength";

/// The lift for a model, in millimetres, from its actor row's ModelAabbMin and ModelAabbMax: the
/// farthest the model reaches from its origin along any axis, up to [`CAP`].
pub fn of_row(row: &Node) -> Option<u32> {
    let mut far: f32 = 0.0;
    for k in ["ModelAabbMin", "ModelAabbMax"] {
        let v: &Node = row.get(k)?;
        for a in ["X", "Y", "Z"] {
            far = far.max(num(v.get(a)?)?.abs());
        }
    }
    Some(((far * 1000.0).round() as u32).min(CAP))
}

/// How far a model hangs below its origin, in metres, from its actor row's ModelAabbMin (0 for a
/// model that stands on its origin), up to [`CAP`]. A shelf deal stands without a turn, so
/// raising it by this much seats the model on the shelf board, as the shop's own goods sit (their
/// models start at the origin).
pub fn below_of(row: &Node) -> Option<f32> {
    let y: f32 = num(row.get("ModelAabbMin")?.get("Y")?)?;
    Some((-y).clamp(0.0, CAP as f32 / 1000.0))
}

fn num(n: &Node) -> Option<f32> {
    match n {
        Node::F32(x) => Some(*x),
        Node::F64(x) => Some(*x as f32),
        Node::I32(x) => Some(*x as f32),
        _ => None,
    }
}

fn lift_of(t: &Byml, shown: &str) -> u32 {
    crate::pickup_pack::rows::find(t, shown).and_then(of_row).unwrap_or(0)
}

/// `l` with the lift filled in for a spin layer: from the actor row of the model the pickup shows
/// (the model layer's donor, else the item).
pub fn layers(rf: &mut Romfs, item: &str, l: &Layers) -> Res<Layers> {
    if !l.spin || l.lift != 0 {
        return Ok(l.clone());
    }
    let shown: &str = l.model.as_deref().unwrap_or(item);
    let t: Byml = rf.read_byml(crate::pickup_pack::rows::ACTOR_INFO)?;
    Ok(Layers { lift: lift_of(&t, shown), ..l.clone() })
}

/// The lift of the seed pickup pack `pack` (a pickup the catalog builds), in metres.
pub fn for_pack(t: &Byml, pack: &str) -> Option<f32> {
    let p: crate::slice::key_spots::Pick = pick_of(pack)?;
    let (s, l) = p.spec().ok()?;
    let mm: u32 = lift_of(t, l.model.as_deref().unwrap_or(s.source()));
    (mm > 0).then_some(mm as f32 / 1000.0)
}

pub fn pick_of(pack: &str) -> Option<crate::slice::key_spots::Pick> {
    crate::slice::key_spots::by_pack(pack).or_else(|| crate::dungeon_packs::by_name(pack).map(|p| p.pick()))
}


pub fn perimeter_file(name: &str) -> String {
    format!("Component/PerimeterAnalyserParam/{name}.game__component__PerimeterAnalyserParam.bgyml")
}

/// Lengthens every ground check ray of a perimeter analyser by `dy` metres.
fn reach(root: &mut Node, dy: f32) -> usize {
    let mut n: usize = 0;
    if let Some(cs) = root.get_mut(DETECTORS).and_then(|d| d.get_mut(GROUND)).and_then(|g| g.get_mut(CHECKS)).and_then(|c| c.as_arr_mut()) {
        for c in cs.iter_mut() {
            if let Some(Node::F32(l)) = c.get_mut(LENGTH) {
                *l += dy;
                n += 1;
            }
        }
    }
    n
}

/// Gives the pickup `m` (actor `name`) a ground check `mm` millimetres longer, built from the one
/// its actor uses (its own PerimeterAnalyserRef, else the pickup base's).
pub fn ground(p: &Pool, mut m: Made, name: &str, mm: u32) -> Res<Made> {
    if mm == 0 {
        return Ok(m);
    }
    let mut actor: Byml = Byml::parse(&m.actor.1)?;
    let own: Option<String> = actor.root.get(COMPONENTS).and_then(|c| c.get(PERIMETER_REF)).and_then(|v| v.as_str()).map(|s| s.to_string());
    let base_ref: String = match own {
        Some(r) => r,
        None => chain(p, BASE)?.comps.get(PERIMETER_REF).cloned().ok_or(E::Format("lift: the pickup base has no perimeter analyser"))?,
    };
    let f: String = norm(&base_ref).ok_or_else(|| E::Msg(format!("lift: {base_ref} is not a file")))?;
    if p.get(&f).is_none() {
        return Err(E::Msg(format!("lift: the perimeter analyser {f} is not in the pickup's packs")));
    }
    let mut y: Byml = p.byml(&f)?;
    if reach(&mut y.root, mm as f32 / 1000.0) == 0 {
        return Err(E::Msg(format!("lift: {f} has no ground check to lengthen")));
    }
    let pf: String = perimeter_file(name);
    m.extra.push((pf.clone(), y.write()?));
    actor.root.get_mut(COMPONENTS).ok_or(E::Format("lift: the actor has no component list"))?.set(PERIMETER_REF, Node::Str(local(&pf)));
    m.actor.1 = actor.write()?;
    Ok(m)
}

/// Raises the map actor `hash` of `y` by `dy` metres. Returns how many actors moved.
pub fn raise(y: &mut Byml, hash: u64, dy: f32) -> usize {
    let Some(list) = y.root.get_mut("Actors").and_then(|a| a.as_arr_mut()) else {
        return 0;
    };
    let mut n: usize = 0;
    for a in list.iter_mut() {
        if crate::placement::hash_of(a) != Some(hash) {
            continue;
        }
        if let Some(Node::F32(v)) = a.get_mut("Translate").and_then(|t| t.as_arr_mut()).and_then(|t| t.get_mut(1)) {
            *v += dy;
            n += 1;
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(x: f32, y: f32, z: f32) -> Node {
        Node::Map(vec![("X".into(), Node::F32(x)), ("Y".into(), Node::F32(y)), ("Z".into(), Node::F32(z))])
    }

    #[test]
    fn the_lift_is_the_farthest_reach_of_the_model_up_to_the_cap() {
        let row = |a: Node, b: Node| Node::Map(vec![("ModelAabbMin".into(), a), ("ModelAabbMax".into(), b)]);
        assert_eq!(of_row(&row(v(-0.10858154, -0.039733887, -0.120462894), v(0.10882568, 0.03945923, 0.5272422))), Some(527));
        assert_eq!(of_row(&row(v(-0.44, -0.27, -0.32), v(0.21, 0.27, 1.45))), Some(CAP));
        assert_eq!(of_row(&Node::Map(Vec::new())), None);
    }

    #[test]
    fn a_shelf_model_rises_by_how_far_it_hangs_below_its_origin() {
        let row = |a: Node| Node::Map(vec![("ModelAabbMin".into(), a)]);
        assert_eq!(below_of(&row(v(-0.52, -0.1521505, -0.15))), Some(0.1521505));
        assert_eq!(below_of(&row(v(-0.1, 0.0072, -0.04))), Some(0.0));
        assert_eq!(below_of(&row(v(0.0, -2.0, 0.0))), Some(CAP as f32 / 1000.0));
        assert_eq!(below_of(&Node::Map(Vec::new())), None);
    }

    #[test]
    fn dungeon_item_packs_find_their_pick_for_the_lift() {
        for n in ["Obj_BossKey_FireTemple", "Obj_SmallKey_WaterTemple", "Obj_Map_ForestTemple", "Obj_Compass_SpiritTemple"] {
            assert_eq!(pick_of(n).map(|p| p.pack), Some(n.to_string()), "{n}");
        }
        assert!(pick_of("Obj_BossKey_Nowhere").is_none());
    }

    #[test]
    fn a_lifted_placement_keeps_the_game_s_own_size() {
        let a = |h: u64| Node::Map(vec![("Hash".into(), Node::U64(h)), ("Translate".into(), Node::Arr(vec![Node::F32(0.0); 3]))]);
        let mut y: Byml = Byml { root: Node::Map(vec![("Actors".into(), Node::Arr(vec![a(1), a(2)]))]), version: 7, big: false };
        assert_eq!(raise(&mut y, 2, 0.13), 1);
        let list = y.root.get("Actors").and_then(|v| v.as_arr()).unwrap();
        assert_eq!(list[1].get("Translate").and_then(|t| t.as_arr()).map(|t| t[1].clone()), Some(Node::F32(0.13)));
        assert!(list.iter().all(|x| x.get("Scale").is_none()));
    }

    #[test]
    fn the_ground_check_grows_by_the_lift() {
        let check = Node::Map(vec![(LENGTH.into(), Node::F32(0.11)), ("RayCastStartOffset".into(), v(0.0, 0.1, 0.0))]);
        let mut root = Node::Map(vec![(DETECTORS.into(), Node::Map(vec![(GROUND.into(), Node::Map(vec![(CHECKS.into(), Node::Arr(vec![check]))]))]))]);
        assert_eq!(reach(&mut root, 0.5), 1);
        let l = root.get(DETECTORS).and_then(|d| d.get(GROUND)).and_then(|g| g.get(CHECKS)).and_then(|c| c.as_arr()).and_then(|c| c.first()).and_then(|c| c.get(LENGTH)).cloned();
        assert_eq!(l, Some(Node::F32(0.61)));
    }
}
