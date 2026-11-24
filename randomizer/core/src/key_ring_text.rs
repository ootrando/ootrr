





use std::path::Path as P;

use crate::error::{Error as E, Res};
use crate::key_rings::Ring;
use crate::msbt::{self, Msbt};
use crate::romfs::Romfs;
use crate::sarc;
use crate::staging::Out;

pub const POUCH_MSG: &str = "ActorMsg/PouchContent.msbt";

fn lines(r: &Ring) -> [(&str, String); 4] {
    let name: String = format!("Small Key Ring ({})", r.dungeon);
    [
        ("_Name", name.clone()),
        ("_Caption", format!("A ring holding all {} small keys for the {}.", r.count, r.dungeon)),
        ("_GetMessage", format!("You got the {name}!")),
        ("_GetCaption", format!("All {} small keys for the {} are now yours.", r.count, r.dungeon)),
    ]
}

fn label(r: &Ring, suffix: &str) -> String {
    format!("{}{suffix}", r.actor)
}


pub fn apply(raw: &[u8]) -> Res<Vec<u8>> {
    apply_with(raw, &crate::key_rings::all_with_hideout())
}

pub fn apply_with(raw: &[u8], rings: &[&Ring]) -> Res<Vec<u8>> {
    let mut es: Vec<sarc::Entry> = sarc::parse(raw)?;
    let at: usize = es
        .iter()
        .position(|e| e.name == POUCH_MSG)
        .ok_or_else(|| E::Msg(format!("key ring text: the language archive has no {POUCH_MSG}")))?;
    let mut m: Msbt = msbt::parse(&es[at].data)?;
    if m.write()? != es[at].data {
        return Err(E::Format("key ring text: PouchContent.msbt does not round trip"));
    }
    for r in rings {
        for (suffix, text) in lines(r) {
            m.put_markup(&label(r, suffix), &text)?;
        }
    }
    let w: Vec<u8> = m.write()?;
    msbt::parse(&w)?;
    es[at].data = w;
    sarc::write(&es)
}



pub fn stage(rf: &mut Romfs, work: &P, more: &mut Vec<Out>, rings: &[&Ring]) -> Res<()> {
    if rings.is_empty() {
        return Ok(());
    }
    for f in crate::hints::languages(rf)? {
        crate::staging::edit_staged(rf, more, work, &f, true, |raw| apply_with(raw, rings))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::msbt::{Label, Sec};

    fn book() -> Vec<u8> {
        let groups: usize = 2;
        let mut g: Vec<Vec<Label>> = vec![Vec::new(); groups];
        let mut t: Vec<Vec<u8>> = Vec::new();
        let src: Vec<u8> = msbt::encode(&msbt::from_markup("Small Key").unwrap(), false).unwrap();
        g[msbt::label_group("Item_Dungeon_SmallKey_Name", groups)].push(Label { name: "Item_Dungeon_SmallKey_Name".to_string(), idx: 0 });
        t.push(src);
        Msbt { be: false, unk_a: [0, 0], enc: 0, ver: 3, unk_b: [0, 0], pad: [0; 10], secs: vec![Sec::Lbl(g), Sec::Txt(t)] }.write().unwrap()
    }

    fn archive() -> Vec<u8> {
        sarc::write(&[sarc::Entry { name: POUCH_MSG.to_string(), data: book() }]).unwrap()
    }

    #[test]
    fn every_ring_gets_four_nonempty_texts_that_name_the_dungeon() {
        let es: Vec<sarc::Entry> = sarc::parse(&apply(&archive()).unwrap()).unwrap();
        let m: Msbt = msbt::parse(&sarc::get(&es, POUCH_MSG).unwrap().data).unwrap();
        for r in crate::key_rings::all_with_hideout() {
            for (suffix, _) in lines(r) {
                let s: String = msbt::to_markup(&m.text(&label(r, suffix)).unwrap_or_else(|e| panic!("{}: {e}", r.actor)));
                assert!(!s.is_empty(), "{}", r.actor);
            }
            let got: String = msbt::to_markup(&m.text(&label(r, "_GetMessage")).unwrap());
            assert!(got.contains(r.dungeon), "{got}");
        }
    }

    #[test]
    fn the_pouch_name_matches_the_item_name_in_the_spoiler_log() {
        for r in crate::key_rings::all_with_hideout() {
            let name: String = lines(r)[0].1.clone();
            assert_eq!(name, r.item);
        }
    }
}
