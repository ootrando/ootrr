use crate::byml::{Byml, Node};
use crate::error::{Error as E, Res};
use crate::pickup_pack::actor::{MODEL_REF, SHARED_ANIM};
use crate::pickup_pack::closure::{chain, local, norm, Chain, Pool};
use crate::pickup_pack::rows::ACTOR_INFO;
use crate::pickup_pack::Layers;
use crate::romfs::Romfs;

pub const HARP: &str = "Dm_Item_Sheik_Harp";
pub const ROW_KEYS: [&str; 5] = ["FmdbName", "ModelProjectName", "ModelAabbMax", "ModelAabbMin", "PreActorRenderInfoHeapSize"];
const ANIM_LIST: &str = "AnimationResources";
const ANIM_NAME: &str = "ModelProjectName";

pub fn anim_file(name: &str) -> String {
    format!("Component/AnimationParam/{name}.engine__component__AnimationParam.bgyml")
}

pub fn shared_list() -> Node {
    list(None)
}

pub fn list(pose: Option<&str>) -> Node {
    let mut out: Vec<Node> = Vec::new();
    for n in std::iter::once(SHARED_ANIM).chain(pose) {
        let mut one: Node = Node::Map(Vec::new());
        one.set(ANIM_NAME, Node::Str(n.to_string()));
        out.push(one);
    }
    Node::Arr(out)
}

pub fn pose_of(project: &str) -> String {
    format!("{project}_Animation")
}

pub fn pose_file(name: &str) -> String {
    format!("Model/{name}.anim.bfres.zs")
}

pub fn layers(rf: &mut Romfs, l: &Layers) -> Res<Layers> {
    let Some(m) = l.model.as_deref() else {
        return Ok(l.clone());
    };
    if l.pose.is_some() {
        return Ok(l.clone());
    }
    let t: Byml = rf.read_byml(ACTOR_INFO)?;
    let project: Option<String> = crate::pickup_pack::rows::find(&t, m).and_then(|r| r.get(ANIM_NAME)).and_then(|v| v.as_str()).map(pose_of);
    let pose: Option<String> = project.filter(|n| rf.root.join(pose_file(n)).is_file());
    Ok(Layers { pose, ..l.clone() })
}

pub fn anim(name: &str, version: u16, pose: Option<&str>) -> Res<(String, String, Vec<u8>)> {
    let mut root: Node = Node::Map(Vec::new());
    root.set(ANIM_LIST, list(pose));
    let f: String = anim_file(name);
    Ok((local(&f), f, Byml { root, version, big: false }.write()?))
}

pub fn model_ref(p: &Pool, donor: &str) -> Res<String> {
    let c: Chain = chain(p, donor)?;
    let v: &str = c.comps.get(MODEL_REF).map(|s| s.as_str()).ok_or_else(|| E::Msg(format!("the model donor {donor} has no {MODEL_REF}")))?;
    if !norm(v).is_some_and(|f| p.get(&f).is_some()) {
        return Err(E::Msg(format!("the model donor {donor} keeps its model file outside its pack")));
    }
    Ok(v.to_string())
}

pub fn row(r: &mut Node, donor: &Node, pose: Option<&str>) -> Res<()> {
    for k in ROW_KEYS {
        let v: &Node = donor.get(k).ok_or_else(|| E::Msg(format!("the model donor row has no {k}")))?;
        r.set(k, v.clone());
    }
    r.set(ANIM_LIST, list(pose));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_donor_row_gives_the_model_fields_and_the_shared_animation() {
        let mut d: Node = Node::Map(Vec::new());
        for k in ROW_KEYS {
            d.set(k, Node::Str(format!("harp {k}")));
        }
        let mut r: Node = Node::Map(Vec::new());
        r.set("FmdbName", Node::Str("Obj_Rupee".to_string()));
        r.set("ActorName", Node::Str("Item_Melody_SunSong".to_string()));
        row(&mut r, &d, None).unwrap();
        assert_eq!(r.get("FmdbName").and_then(|v| v.as_str()), Some("harp FmdbName"));
        assert_eq!(r.get("ActorName").and_then(|v| v.as_str()), Some("Item_Melody_SunSong"));
        assert_eq!(r.get(ANIM_LIST), Some(&shared_list()));
        assert!(row(&mut r, &Node::Map(Vec::new()), None).is_err());
        let (refd, f, b): (String, String, Vec<u8>) = anim("Obj_Melody_SunSong", 7, None).unwrap();
        assert_eq!(refd, format!("?{f}"));
        assert_eq!(Byml::parse(&b).unwrap().root.get(ANIM_LIST), Some(&shared_list()));
        row(&mut r, &d, Some("Item_Ocarina_Time_Animation")).unwrap();
        let names: Vec<&str> = r.get(ANIM_LIST).and_then(|v| v.as_arr()).unwrap().iter().filter_map(|x| x.get(ANIM_NAME).and_then(|v| v.as_str())).collect();
        assert_eq!(names, [SHARED_ANIM, "Item_Ocarina_Time_Animation"]);
    }

    #[test]
    fn a_model_layer_takes_the_model_s_own_pose_like_the_shipped_pickup() {
        let p: std::path::PathBuf = ootrr_game::test_romfs::path();
        if !p.join("Pack").join("ZsDic.pack.zs").is_file() {
            eprintln!("skipped: no romfs at {}, set OOTRR_ROMFS", p.display());
            return;
        }
        let mut rf: Romfs = Romfs::open(&p).unwrap();
        let on = |m: &str| Layers { model: Some(m.to_string()), ..Layers::default() };
        assert_eq!(layers(&mut rf, &on("Dm_Item_Ocarina_Time")).unwrap().pose.as_deref(), Some("Item_Ocarina_Time_Animation"));
        assert_eq!(layers(&mut rf, &on("Dm_Item_Ocarina_Fairy")).unwrap().pose.as_deref(), Some("Item_Ocarina_Fairy_Animation"));
        assert_eq!(layers(&mut rf, &on(HARP)).unwrap().pose, None);
        assert_eq!(layers(&mut rf, &Layers::default()).unwrap(), Layers::default());
        let shipped: Byml = rf.read_byml(ACTOR_INFO).unwrap();
        let row: &Node = crate::pickup_pack::rows::find(&shipped, "Obj_Ocarina_Time").unwrap();
        assert_eq!(row.get(ANIM_LIST), Some(&list(Some("Item_Ocarina_Time_Animation"))));
    }
}
