



use std::collections::BTreeMap as BM;

use crate::error::{Error as E, Res};
use crate::settings::seed::SeedFile;
use crate::slice::data::Data;
use crate::slice::key_spots::{self, Spot};
use crate::slice::support::{Keep, Plan};
use crate::world::{LocId, World};


pub fn active(p: &Plan) -> bool {
    p.hideout != Keep::Vanilla
}



pub fn eligible(d: &Data, item: &str) -> bool {
    key_spots::placeable(d, item, Spot::Free)
}



pub fn add_checks(w: &World, p: &Plan, free: &mut Vec<LocId>, left: &mut BM<String, usize>) {
    if !active(p) {
        return;
    }
    for name in crate::hideout_keys::NATIVE {
        match w.loc(name) {
            Some(l) => free.push(l),
            None => {
                *left.entry(format!("{name} is not in the logic world")).or_insert(0) += 1;
            }
        }
    }
}


pub fn actors(d: &Data, seed: &SeedFile, fixed: &BM<String, String>) -> Res<BM<String, String>> {
    let mut out: BM<String, String> = BM::new();
    for name in crate::hideout_keys::NATIVE {
        let item: &str = seed
            .placements
            .get(name)
            .ok_or_else(|| E::Msg(format!("{name} has no item in this seed")))?;
        let pick: key_spots::Pick = key_spots::resolve_at(d, name, item, fixed.get(name).map(|s| s.as_str()))
            .map_err(|e| E::Msg(format!("{name} holds {item}, which cannot drop from a guard: {e}")))?;
        out.insert(name.to_string(), pick.pack);
    }
    Ok(out)
}
