use std::path::Path as P;

use crate::bombchu_bag::ACTOR;
use crate::error::{Error as E, Res};
use crate::msbt::{self, Msbt, Tok};
use crate::romfs::Romfs;
use crate::sarc::{self, Entry};
use crate::staging::Out;

pub const MESSAGES: &str = "ActorMsg/PouchContent.msbt";
pub const TEXTS: [(&str, &str); 5] = [
    ("Name", "Bombchu Bag"),
    ("Caption", "Carry up to 40 Bombchus\nin this squeaky sack."),
    ("ShopCaption", "Carry up to 40 Bombchus\nin this squeaky sack."),
    ("GetMessage", "You found a Bombchu Bag!"),
    ("GetCaption", "It holds 40 Bombchus and came fully stocked!\nBombchus no longer need a bomb bag."),
];

pub fn add(raw: &[u8]) -> Res<Vec<u8>> {
    let mut es: Vec<Entry> = sarc::parse(raw)?;
    let b: Vec<u8> = sarc::get(&es, MESSAGES).ok_or_else(|| E::Msg(format!("bombchu bag: a message archive has no {MESSAGES}")))?.data.clone();
    let mut m: Msbt = msbt::parse(&b)?;
    for (sfx, t) in TEXTS {
        m.put_tokens(&format!("{ACTOR}_{sfx}"), &[Tok::Text(t.to_string())])?;
    }
    let w: Vec<u8> = m.write()?;
    msbt::parse(&w)?;
    sarc::set(&mut es, MESSAGES, w);
    sarc::write(&es)
}

pub fn stage(rf: &mut Romfs, late: &mut Vec<Out>, work: &P) -> Res<()> {
    for l in crate::hints::languages(rf)? {
        crate::story_skip::stage_late_file(rf, late, work, &l, add)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_text_names_forty_or_the_bag() {
        assert!(TEXTS.iter().all(|(_, t)| t.contains("40") || t.contains("Bombchu Bag")));
        assert!(TEXTS.iter().all(|(_, t)| !t.contains(';')));
    }

    #[test]
    fn the_real_archives_take_the_texts() {
        let Some(root) = std::env::var_os("OOTRR_ROMFS").map(std::path::PathBuf::from).filter(|p| p.join("Mals").is_dir()) else {
            return;
        };
        let mut rf: Romfs = Romfs::open(&root).unwrap();
        let l: String = crate::hints::languages(&rf).unwrap().into_iter().find(|x| x.contains("USen")).unwrap();
        let out: Vec<u8> = add(&rf.read_zs(&l).unwrap()).unwrap();
        let m: Msbt = msbt::parse(&sarc::get(&sarc::parse(&out).unwrap(), MESSAGES).unwrap().data).unwrap();
        assert_eq!(m.text(&format!("{ACTOR}_Name")).unwrap(), vec![Tok::Text("Bombchu Bag".to_string())]);
        assert!(m.find(&format!("{ACTOR}_GetCaption")).is_some());
    }
}
