use std::path::Path as P;

use crate::error::Res;
use crate::romfs::Romfs;
use crate::slice::key_spots::Pick;
use crate::staging::Out;

pub mod actor;
pub mod cap;
pub mod look;
pub mod pickup;
pub mod pool;
pub mod text;

pub const ITEM: &str = "Bombchu Bag";
pub const ACTOR: &str = "Item_BombchuBag";
pub const PACK: &str = "Obj_BombchuBag";
pub const CHU: &str = "Item_Bombchu";
pub const BUNDLE: &str = "Item_BombchuBundle_Large";
pub const BAG: &str = "Item_BombBag_Small";
pub const PROJECT: &str = "Item_BombchuBag";
pub const CAPACITY: i32 = 40;
pub const HEAP: &str = "199928";
pub const NOTE: &str = "Bombchus have their own bag: the Bombchu Bag holds 40 Bombchus and comes full. Bombchus no longer use the bomb bag's maximum, and a bomb bag no longer refills them. Bombchus found before the Bombchu Bag turn into rupees, like ammo without its bag. Not tested in the game yet.";

pub fn actor_path(a: &str) -> String {
    format!("Work/Actor/{a}.engine__actor__ActorParam.gyml")
}

pub fn is_pack(name: &str) -> bool {
    name == PACK || name.strip_prefix(PACK).is_some_and(|r| r.starts_with('_'))
}

pub fn pick() -> Pick {
    Pick { content: BUNDLE.to_string(), pack: PACK.to_string(), args: vec!["--name".to_string(), PACK.to_string(), "--heap".to_string(), HEAP.to_string()] }
}

pub fn size_peer(path: &str) -> Option<String> {
    (path == look::model_file()).then(look::bag_file)
}

pub fn stage(rf: &mut Romfs, late: &mut Vec<Out>, work: &P) -> Res<()> {
    cap::stage(rf, late, work)?;
    let model: look::Look = look::make(rf)?;
    actor::stage(rf, late, work, &model)?;
    text::stage(rf, late, work)?;
    late.push(model.out);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pickup_pick_builds_the_bag_from_the_twenty_pack() {
        let p: Pick = pick();
        assert_eq!(p.content, BUNDLE);
        assert_eq!(p.pack, PACK);
        assert!(p.spec().is_ok());
        assert!(p.kit_spec().is_ok());
        assert!(is_pack(PACK) && is_pack(&p.hang()) && !is_pack("Obj_BombchuBagX") && !is_pack("Obj_Bombchu"));
    }

    #[test]
    fn only_the_new_model_borrows_the_bag_size() {
        assert_eq!(size_peer(&look::model_file()), Some(look::bag_file()));
        assert_eq!(size_peer(&look::bag_file()), None);
    }
}
