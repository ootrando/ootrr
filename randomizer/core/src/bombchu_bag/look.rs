use crate::bombchu_bag::{BAG, PROJECT};
use crate::error::{Error as E, Res};
use crate::fres::merge::{check, merge, Merged, Place};
use crate::romfs::Romfs;
use crate::staging::Out;
use crate::zs;

pub const CHU_MODEL: &str = "Item_Bombchu";
pub const BONE: &str = "Skl_Root";
pub const PLACE: Place = Place { scale: [0.3, 0.27, 0.22], at: [0.0, 0.055, 0.057] };

pub struct Look {
    pub out: Out,
    pub min: [f64; 3],
    pub max: [f64; 3],
}

pub fn model_file() -> String {
    format!("Model/{PROJECT}.{BAG}.bfres.zs")
}

pub fn bag_file() -> String {
    format!("Model/{BAG}.{BAG}.bfres.zs")
}

pub fn chu_file() -> String {
    format!("Model/{CHU_MODEL}.{CHU_MODEL}.bfres.zs")
}

pub fn make(rf: &mut Romfs) -> Res<Look> {
    let z: Vec<u8> = rf.read(&bag_file())?;
    let bag: Vec<u8> = rf.zs.decompress(&z)?;
    let chu: Vec<u8> = rf.read_zs(&chu_file())?;
    let m: Merged = merge(&bag, &chu, PLACE, BONE).map_err(|e| E::Msg(format!("bombchu bag: the bag and chu models do not merge: {e}")))?;
    check(&m.bytes).map_err(|e| E::Msg(format!("bombchu bag: the merged model fails its check: {e}")))?;
    let out: Out = Out { path: model_file(), old: 0, new: m.bytes.len(), z: zs::compress_raw(&m.bytes, zs::dict_of(&z))? };
    Ok(Look { out, min: m.min, max: m.max })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_model_is_a_new_project_of_the_bag_model() {
        assert_eq!(model_file(), "Model/Item_BombchuBag.Item_BombBag_Small.bfres.zs");
        assert_eq!(bag_file(), "Model/Item_BombBag_Small.Item_BombBag_Small.bfres.zs");
        assert!(PLACE.scale.iter().all(|s| *s > 0.0 && *s < 0.5));
        assert!(PLACE.scale[0] > PLACE.scale[1] && PLACE.scale[2] < PLACE.scale[1]);
    }

    #[test]
    fn the_real_models_merge_with_the_chu_face_out_front() {
        let Some(root) = std::env::var_os("OOTRR_ROMFS").map(std::path::PathBuf::from).filter(|p| p.join("Model").is_dir()) else {
            return;
        };
        let mut rf: Romfs = Romfs::open(&root).unwrap();
        let l: Look = make(&mut rf).unwrap();
        assert_eq!(l.out.path, model_file());
        assert!(l.max[2] > 0.09, "{:?}", l.max);
        assert!(l.min[2] > -0.089, "{:?}", l.min);
        let raw: Vec<u8> = rf.zs.decompress(&l.out.z).unwrap();
        assert_eq!(raw.len(), l.out.new);
        check(&raw).unwrap();
    }
}
