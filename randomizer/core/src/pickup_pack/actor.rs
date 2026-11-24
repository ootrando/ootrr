use crate::byml::{Byml, Node};
use crate::error::{Error as E, Res};
use crate::pickup_pack::closure::{actor_file, chain, local, norm, work, Chain, Pool, COMPONENTS, PARENT};
use crate::pickup_pack::magic;
use crate::pickup_pack::meter;
use crate::pickup_pack::model;
use crate::pickup_pack::{is_actor_name, Layers, Spec, Style, BASE, DONOR};

pub const NAME_REF: &str = "ActorNameRef";
pub const MODEL_REF: &str = "ModelInfoRef";
pub const ANIM_REF: &str = "AnimationRef";
pub const AS_REF: &str = "ASRef";
pub const AS_INFO_REF: &str = "ASInfoRef";
pub const POUCH_REF: &str = "PouchContentRef";
pub const EVENT_REF: &str = "EventPerformerRef";


pub const EVENT_BASE: &str = "Item_EventObject_Base";
pub const ATTENTION_REF: &str = "AttentionSenderRef";
pub const POUCH_ACTOR: &str = "PouchActor";
pub const FIRST_CHECK: &str = "IsCountCheckOnFirstGet";
pub const AUTO_GET: &str = "DefaultAutoGet";
pub const SHARED_ANIM: &str = "Item_Common_Animation";
const ANIM_LIST: &str = "AnimationResources";
const ANIM_NAME: &str = "ModelProjectName";
pub const ELINK_REF: &str = "ELinkRef";
/// What a pickup plays on its own: the shared Item_Common set's turn in place, as the game's
/// fishing pond Heart Piece (Obj_Heart_Piece_MiniGame) does. Without it the item model stands in
/// its bare pose, centred on the actor, half sunk in the floor, and never turns.
pub const SPIN: &str = "Rotate_Obj";
const AUTO_PLAY: &str = "AutoPlayASName";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Made {
    pub actor: (String, Vec<u8>),
    pub pouch: (String, Vec<u8>),
    pub identity: String,
    pub extra: Vec<(String, Vec<u8>)>,
}

/// The pickup's own animation settings: the donor's, playing [`SPIN`].
pub fn spin_file(name: &str) -> String {
    format!("Component/ASInfo/{name}.engine__component__ASInfo.bgyml")
}

pub fn own_name_ref(name: &str) -> String {
    format!("?ActorSystem/ActorName/{name}.engine__actor__ActorName.bgyml")
}

pub const DEAL_PARAM: &str = "SpecialDeal";
pub const STOCK_PARAM: &str = "EnableStock";
pub const PLAIN_DEAL: &str = "None";

pub fn deal_of(l: &Layers, name: &str) -> Option<(&'static str, bool)> {
    if l.own {
        Some((crate::pickup_pack::rows::QUIET_DEAL, false))
    } else if name.starts_with(crate::slice::key_spots::DUNGEON_PREFIX) {
        Some((PLAIN_DEAL, true))
    } else {
        None
    }
}

pub fn pouch_file(name: &str) -> String {
    format!("Component/PouchContentParam/{name}.game__component__PouchContentParam.bgyml")
}


pub fn model_file(name: &str) -> String {
    format!("Component/ModelInfo/{name}.engine__component__ModelInfo.bgyml")
}



pub fn event_file(name: &str) -> String {
    format!("Component/EventPerformerParam/{name}.engine__component__EventPerformerParam.bgyml")
}

fn need<'a>(c: &'a Chain, who: &str, k: &str) -> Res<&'a str> {
    c.comps.get(k).map(|s| s.as_str()).ok_or_else(|| E::Msg(format!("{who} has no {k}, this template cannot carry it")))
}

fn held(p: &Pool, who: &str, k: &str, v: &str) -> Res<String> {
    let f: String = norm(v).ok_or_else(|| E::Msg(format!("{who} has a {k} that is not a file reference")))?;
    if p.get(&f).is_none() {
        return Err(E::Msg(format!("{who} points its {k} at {f}, which is not in its pack")));
    }
    Ok(f)
}

pub fn shared_anim(p: &Pool, c: &Chain) -> bool {
    let Some(f) = c.comps.get(ANIM_REF).and_then(|v| norm(v)) else {
        return false;
    };
    let Ok(y) = p.byml(&f) else {
        return false;
    };
    y.root.get(ANIM_LIST).and_then(|v| v.as_arr()).and_then(|a| a.first()).and_then(|x| x.get(ANIM_NAME)).and_then(|v| v.as_str()) == Some(SHARED_ANIM)
}

pub fn fits(p: &Pool, s: &Spec) -> Res<Vec<String>> {
    fits_with(p, s, &Layers::default())
}

pub fn fits_with(p: &Pool, s: &Spec, l: &Layers) -> Res<Vec<String>> {
    let source: &str = s.source();
    let item: Chain = chain(p, source)?;
    let donor: Chain = chain(p, DONOR)?;
    let mut bad: Vec<String> = Vec::new();
    if item.files.contains(&actor_file(BASE)) {
        bad.push(format!("{source} is already a world pickup"));
    }
    if let Some(m) = &l.model {
        if let Err(e) = model::model_ref(p, m) {
            bad.push(e.to_string());
        }
        for k in [NAME_REF, POUCH_REF] {
            match item.comps.get(k) {
                None => bad.push(format!("{source} has no {k}")),
                Some(v) if k != NAME_REF && held(p, source, k, v).is_err() => bad.push(format!("{source} keeps its {k} outside its pack")),
                Some(_) => {}
            }
        }
        return Ok(bad);
    }
    for k in [NAME_REF, MODEL_REF, ANIM_REF, POUCH_REF] {
        match item.comps.get(k) {
            None => bad.push(format!("{source} has no {k}")),
            Some(v) if k != NAME_REF && held(p, source, k, v).is_err() => bad.push(format!("{source} keeps its {k} outside its pack")),
            Some(_) => {}
        }
    }
    if item.comps.get(AS_REF) != donor.comps.get(AS_REF) && !shared_anim(p, &item) {
        bad.push(format!("{source} does not use the animation set of the shipped pickups"));
    }
    Ok(bad)
}

fn actor_in(r: &str) -> Option<String> {
    let file: &str = r.rsplit('/').next().unwrap_or(r);
    let name: &str = file.split(".engine__actor__").next().unwrap_or(file);
    is_actor_name(name).then(|| name.to_string())
}

pub fn identity(p: &Pool, s: &Spec, item: &Chain, pouch: &str) -> Res<String> {
    if let Some(id) = s.identity_override() {
        return Ok(id.to_string());
    }
    let mut seen: Vec<String> = Vec::new();
    let mut cur: Option<String> = Some(pouch.to_string());
    while let Some(f) = cur {
        if seen.contains(&f) || p.get(&f).is_none() {
            break;
        }
        let y: Byml = p.byml(&f)?;
        if let Some(v) = y.root.get(POUCH_ACTOR).and_then(|v| v.as_str()) {
            return actor_in(v).ok_or_else(|| E::Msg(format!("{f} names the pouch actor {v}, which is not an actor file")));
        }
        cur = y.root.get(PARENT).and_then(|v| v.as_str()).and_then(norm);
        seen.push(f);
    }
    Ok(item.comps.get(NAME_REF).and_then(|v| actor_in(v)).unwrap_or_else(|| s.item.clone()))
}

pub fn make(p: &Pool, s: &Spec) -> Res<Made> {
    make_with(p, s, &Layers::default())
}






fn make_event_ring(p: &Pool, s: &Spec, l: &Layers) -> Res<Made> {
    let source: &str = s.source();
    let item: Chain = chain(p, source)?;
    let pouch_source: &str = l.pouch_from.as_deref().ok_or_else(|| E::Msg("a key ring needs a --pouch-from key item".to_string()))?;
    let pouch_chain: Chain = chain(p, pouch_source)?;
    let who: &str = l.ring.as_deref().ok_or_else(|| E::Msg("a key ring needs a --ring identity".to_string()))?;
    let version: u16 = p.byml(&actor_file(source))?.version;

    let key_model: String = held(p, source, MODEL_REF, need(&item, source, MODEL_REF)?)?;
    let model: String = model_file(&s.name);
    let model_data: Vec<u8> = p.get(&key_model).ok_or_else(|| E::Msg(format!("{source} model {key_model} is missing")))?.to_vec();
    let event: String = event_file(&s.name);
    let pouch: String = pouch_file(&s.name);
    let pouch_base: String = held(p, pouch_source, POUCH_REF, need(&pouch_chain, pouch_source, POUCH_REF)?)?;



    let pouch_data: Vec<u8> = p.get(&pouch_base).ok_or_else(|| E::Msg(format!("{pouch_source} pouch {pouch_base} is missing")))?.to_vec();
    let elink: String = held(p, pouch_source, ELINK_REF, need(&pouch_chain, pouch_source, ELINK_REF)?)?;

    let mut performer: Node = Node::Map(Vec::new());
    performer.set("EventUniqueAin", Node::Str(work(&format!("AI/Root/Event/{who}.event.root.ain"))));
    performer.set("UseEventUniqueAin", Node::Bool(true));

    let mut comps: Node = Node::Map(Vec::new());
    comps.set(AS_REF, Node::Str(need(&pouch_chain, pouch_source, AS_REF)?.to_string()));
    comps.set(NAME_REF, Node::Str(format!("?ActorSystem/ActorName/{who}.engine__actor__ActorName.bgyml")));
    comps.set(ANIM_REF, Node::Str(need(&pouch_chain, pouch_source, ANIM_REF)?.to_string()));
    comps.set(ELINK_REF, Node::Str(local(&elink)));
    comps.set(EVENT_REF, Node::Str(local(&event)));
    comps.set(MODEL_REF, Node::Str(local(&model)));
    comps.set(POUCH_REF, Node::Str(local(&pouch)));

    let mut actor: Node = Node::Map(Vec::new());
    actor.set(PARENT, Node::Str(work(&actor_file(EVENT_BASE))));
    actor.set(COMPONENTS, comps);
    Ok(Made {
        actor: (actor_file(&s.name), Byml { root: actor, version, big: false }.write()?),
        pouch: (pouch.clone(), pouch_data),
        identity: who.to_string(),
        extra: vec![
            (event, Byml { root: performer, version, big: false }.write()?),
            (model, model_data),
        ],
    })
}

pub fn make_with(p: &Pool, s: &Spec, l: &Layers) -> Res<Made> {
    if l.ring.is_some() && !l.drop_ring {
        return make_event_ring(p, s, l);
    }



    let source: &str = s.source();
    let item: Chain = chain(p, source)?;
    let donor: Chain = chain(p, DONOR)?;
    if let Some(m) = fits_with(p, s, l)?.first() {
        return Err(E::Msg(format!("{m}, so the thin pickup template does not fit")));
    }
    let pouch_source: &str = l.pouch_from.as_deref().unwrap_or(source);
    let pouch_chain: Chain = if pouch_source == source { item.clone() } else { chain(p, pouch_source)? };
    let item_pouch: String = held(p, pouch_source, POUCH_REF, need(&pouch_chain, pouch_source, POUCH_REF)?)?;
    let version: u16 = p.byml(&actor_file(DONOR))?.version;
    let pouch_path: String = pouch_file(&s.name);
    let mut comps: Node = Node::Map(Vec::new());
    let mut extra: Vec<(String, Vec<u8>)> = Vec::new();
    if l.spin {
        let as_info: String = held(p, DONOR, AS_INFO_REF, need(&donor, DONOR, AS_INFO_REF)?)?;
        let mut spin: Node = Node::Map(Vec::new());
        spin.set(PARENT, Node::Str(work(&as_info)));
        spin.set(AUTO_PLAY, Node::Str(SPIN.to_string()));
        let spin_path: String = spin_file(&s.name);
        comps.set(AS_INFO_REF, Node::Str(local(&spin_path)));
        extra.push((spin_path, Byml { root: spin, version, big: false }.write()?));
    } else {
        comps.set(AS_INFO_REF, Node::Str(need(&donor, DONOR, AS_INFO_REF)?.to_string()));
    }
    comps.set(AS_REF, Node::Str(need(&donor, DONOR, AS_REF)?.to_string()));
    let name_ref: String = match l.ring.as_deref() {
        Some(who) => own_name_ref(who),
        None if l.own || s.name.starts_with(crate::slice::key_spots::DUNGEON_PREFIX) => {


            own_name_ref(&s.name)
        }
        None => need(&item, source, NAME_REF)?.to_string(),
    };
    comps.set(NAME_REF, Node::Str(name_ref));




    match &l.model {
        Some(m) => {
            let (r, f, b): (String, String, Vec<u8>) = model::anim(&s.name, version, l.pose.as_deref())?;
            comps.set(ANIM_REF, Node::Str(r));
            comps.set(MODEL_REF, Node::Str(model::model_ref(p, m)?));
            extra.push((f, b));
        }
        None => {
            comps.set(ANIM_REF, Node::Str(need(&item, source, ANIM_REF)?.to_string()));
            comps.set(MODEL_REF, Node::Str(need(&item, source, MODEL_REF)?.to_string()));
        }
    }
    comps.set(POUCH_REF, Node::Str(local(&pouch_path)));
    if s.style == Style::Press {
        comps.set(ATTENTION_REF, Node::Str(need(&donor, DONOR, ATTENTION_REF)?.to_string()));
    }
    if l.magic {
        if meter::applies(s.source()) {
            return Err(E::Msg(format!("{} gives no magic meter, the magic layer would add the raise back", s.source())));
        }
        let (f, d, r): (String, Vec<u8>, String) = magic::make(p, &item, &chain(p, BASE)?, s.source(), &s.name)?;
        comps.set(magic::TABLE_REF, Node::Str(r));
        extra.push((f, d));
    }
    let mut actor: Node = Node::Map(Vec::new());
    actor.set(PARENT, Node::Str(work(&actor_file(BASE))));
    actor.set(COMPONENTS, comps);
    let mut pouch: Node = Node::Map(Vec::new());
    pouch.set(PARENT, Node::Str(work(&item_pouch)));
    // Goron's Bracelet saves under its armor actor, as the shop shelf does (crate::shelf::actor).
    let who: String = if l.own || s.name.starts_with(crate::slice::key_spots::DUNGEON_PREFIX) {
        s.name.clone()
    } else {
        crate::shelf::actor::shelf_identity(&identity(p, s, &item, &item_pouch)?).to_string()
    };
    pouch.set(POUCH_ACTOR, Node::Str(work(&actor_file(&who))));
    pouch.set(FIRST_CHECK, Node::Bool(false));
    if s.style == Style::Touch {
        pouch.set(AUTO_GET, Node::Bool(true));
    }
    if let Some((deal, stock)) = deal_of(l, &s.name) {
        pouch.set(DEAL_PARAM, Node::U32(crate::flags::hash_name(deal)));
        pouch.set(STOCK_PARAM, Node::Bool(stock));
        for k in [crate::pickup_pack::kit::MANY, crate::pickup_pack::kit::AT_MAX] {
            pouch.set(k, Node::Bool(true));
        }
    }
    Ok(Made {
        actor: (actor_file(&s.name), Byml { root: actor, version, big: false }.write()?),
        pouch: (pouch_path, Byml { root: pouch, version, big: false }.write()?),
        identity: who,
        extra,
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn file(pairs: &[(&str, Node)]) -> Vec<u8> {
        let mut root: Node = Node::Map(Vec::new());
        for (k, v) in pairs {
            root.set(k, v.clone());
        }
        Byml { root, version: 7, big: false }.write().unwrap()
    }

    fn s(x: &str) -> Node {
        Node::Str(x.to_string())
    }

    fn comps(pairs: &[(&str, &str)]) -> Node {
        let mut m: Node = Node::Map(Vec::new());
        for (k, v) in pairs {
            m.set(k, s(v));
        }
        m
    }

    pub(crate) fn pool() -> Pool {
        let mut p: Pool = Pool::default();
        let one: Vec<u8> = file(&[("A", Node::Bool(true))]);
        p.add(
            &actor_file(BASE),
            &file(&[(
                COMPONENTS,
                comps(&[
                    (ATTENTION_REF, "?Component/Attention/Auto.game__component__AttentionSenderParam.bgyml"),
                    (MODEL_REF, "?Component/ModelInfo/Dummy.engine__component__ModelInfo.bgyml"),
                    (POUCH_REF, "?Component/PouchContentParam/Obj_Item_Base.game__component__PouchContentParam.bgyml"),
                    ("PhysicsRef", "?Component/Physics/Obj_Item_Base.engine__component__PhysicsParam.bgyml"),
                ]),
            )]),
        )
        .unwrap();
        p.add("Component/Attention/Auto.game__component__AttentionSenderParam.bgyml", &one).unwrap();
        p.add("Component/ModelInfo/Dummy.engine__component__ModelInfo.bgyml", &one).unwrap();
        p.add("Component/PouchContentParam/Obj_Item_Base.game__component__PouchContentParam.bgyml", &one).unwrap();
        p.add("Component/Physics/Obj_Item_Base.engine__component__PhysicsParam.bgyml", &one).unwrap();
        p.add(
            &actor_file(DONOR),
            &file(&[
                (PARENT, s(&work(&actor_file(BASE)))),
                (
                    COMPONENTS,
                    comps(&[
                        (AS_INFO_REF, "?Component/ASInfo/Item_Obj.engine__component__ASInfo.bgyml"),
                        (AS_REF, "?AS/Item_Common.root.asb"),
                        (ATTENTION_REF, "?Component/Attention/Press.game__component__AttentionSenderParam.bgyml"),
                        (NAME_REF, "?ActorSystem/ActorName/Item_Arrow_Fire.engine__actor__ActorName.bgyml"),
                        ("XLinkRef", "?Component/XLink/ActorBasic.engine__component__XLinkParam.bgyml"),
                    ]),
                ),
            ]),
        )
        .unwrap();
        p.add("Component/ASInfo/Item_Obj.engine__component__ASInfo.bgyml", &one).unwrap();
        p.add("Component/XLink/ActorBasic.engine__component__XLinkParam.bgyml", &one).unwrap();
        p.add("Component/Attention/Press.game__component__AttentionSenderParam.bgyml", &one).unwrap();
        p.add(
            &actor_file("Item_Test"),
            &file(&[(
                COMPONENTS,
                comps(&[
                    (AS_REF, "?AS/Item_Common.root.asb"),
                    (NAME_REF, "?ActorSystem/ActorName/Item_Test.engine__actor__ActorName.bgyml"),
                    (ANIM_REF, "?Component/AnimationParam/Item_Test.engine__component__AnimationParam.bgyml"),
                    ("CaptureRef", "?Component/CaptureParam/Item_Test.game__component__CaptureParam.bgyml"),
                    (MODEL_REF, "?Component/ModelInfo/Item_Test.engine__component__ModelInfo.bgyml"),
                    (POUCH_REF, "?Component/PouchContentParam/Item_Test.game__component__PouchContentParam.bgyml"),
                ]),
            )]),
        )
        .unwrap();
        p.add("Component/AnimationParam/Item_Test.engine__component__AnimationParam.bgyml", &one).unwrap();
        p.add("Component/CaptureParam/Item_Test.game__component__CaptureParam.bgyml", &one).unwrap();
        p.add("Component/ModelInfo/Item_Test.engine__component__ModelInfo.bgyml", &one).unwrap();
        p.add("Component/PouchContentParam/Item_Test.game__component__PouchContentParam.bgyml", &one).unwrap();
        p
    }

    pub(crate) fn spec(style: Style) -> Spec {
        Spec { item: "Item_Test".to_string(), name: "Obj_Test".to_string(), style, heap: None, source: None, identity: None }
    }

    #[test]
    fn the_new_actor_is_a_child_of_the_pickup_base_that_keeps_the_item_identity() {
        let m: Made = make(&pool(), &spec(Style::Touch)).unwrap();
        assert_eq!(m.actor.0, "Actor/Obj_Test.engine__actor__ActorParam.bgyml");
        let y: Byml = Byml::parse(&m.actor.1).unwrap();
        assert_eq!(y.version, 7);
        assert_eq!(y.root.get(PARENT).and_then(|v| v.as_str()), Some("Work/Actor/Obj_Item_Base.engine__actor__ActorParam.gyml"));
        let c: &Node = y.root.get(COMPONENTS).unwrap();
        let text = |k: &str| c.get(k).and_then(|v| v.as_str()).map(|x| x.to_string());
        assert_eq!(text(NAME_REF).as_deref(), Some("?ActorSystem/ActorName/Item_Test.engine__actor__ActorName.bgyml"));
        assert_eq!(text(MODEL_REF).as_deref(), Some("?Component/ModelInfo/Item_Test.engine__component__ModelInfo.bgyml"));
        assert_eq!(text(POUCH_REF).as_deref(), Some("?Component/PouchContentParam/Obj_Test.game__component__PouchContentParam.bgyml"));
        assert_eq!(text(AS_INFO_REF).as_deref(), Some("?Component/ASInfo/Item_Obj.engine__component__ASInfo.bgyml"));
        assert_eq!(text(ELINK_REF), None);
        assert_eq!(text(ATTENTION_REF), None);
        assert_eq!(text("PhysicsRef"), None);
        assert_eq!(text("CaptureRef"), None);
    }

    #[test]
    fn the_pouch_file_inherits_the_item_and_names_it_as_the_pouch_actor() {
        let m: Made = make(&pool(), &spec(Style::Touch)).unwrap();
        let y: Byml = Byml::parse(&m.pouch.1).unwrap();
        assert_eq!(y.root.get(PARENT).and_then(|v| v.as_str()), Some("Work/Component/PouchContentParam/Item_Test.game__component__PouchContentParam.gyml"));
        assert_eq!(y.root.get(POUCH_ACTOR).and_then(|v| v.as_str()), Some("Work/Actor/Item_Test.engine__actor__ActorParam.gyml"));
        assert_eq!(y.root.get(FIRST_CHECK), Some(&Node::Bool(false)));
        assert_eq!(y.root.get(AUTO_GET), Some(&Node::Bool(true)));
        assert_eq!(y.root.get("SpecialDeal"), None);
        assert_eq!(y.root.get(crate::pickup_pack::kit::AT_MAX), None);
    }

    fn pouch_of(name: &str, l: &Layers) -> Byml {
        let s: Spec = Spec { name: name.to_string(), ..spec(Style::Touch) };
        Byml::parse(&make_with(&pool(), &s, l).unwrap().pouch.1).unwrap()
    }

    #[test]
    fn a_dungeon_item_pack_deals_as_its_pouch_row_says_and_always_plays_its_get() {
        let own: Layers = Layers { own: true, ..Layers::default() };
        let cases: [(&str, &Layers, &str, bool); 2] = [("Obj_SmallKey_ForestTemple", &own, "NotPushToPouch", false), ("Obj_OOTRR_SmallKey_ForestTemple", &Layers::default(), "None", true)];
        for (name, l, deal, stock) in cases {
            let y: Byml = pouch_of(name, l);
            assert_eq!(y.root.get(DEAL_PARAM), Some(&Node::U32(crate::flags::hash_name(deal))), "{name}");
            assert_eq!(y.root.get(STOCK_PARAM), Some(&Node::Bool(stock)), "{name}");
            for k in [crate::pickup_pack::kit::MANY, crate::pickup_pack::kit::AT_MAX] {
                assert_eq!(y.root.get(k), Some(&Node::Bool(true)), "{name} {k}");
            }
        }
        assert_eq!(crate::flags::hash_name("SmallKey"), 2499381459);
        assert_eq!(crate::flags::hash_name("NotPushToPouch"), 2292023464);
    }

    #[test]
    fn the_press_style_takes_the_button_attention_and_drops_auto_get() {
        let m: Made = make(&pool(), &spec(Style::Press)).unwrap();
        let a: Byml = Byml::parse(&m.actor.1).unwrap();
        let got: Option<&str> = a.root.get(COMPONENTS).and_then(|c| c.get(ATTENTION_REF)).and_then(|v| v.as_str());
        assert_eq!(got, Some("?Component/Attention/Press.game__component__AttentionSenderParam.bgyml"));
        let y: Byml = Byml::parse(&m.pouch.1).unwrap();
        assert_eq!(y.root.get(AUTO_GET), None);
    }

    #[test]
    fn an_item_with_another_animation_set_or_a_missing_part_is_refused() {
        let mut p: Pool = pool();
        let odd: Vec<u8> = file(&[(
            COMPONENTS,
            comps(&[
                (AS_REF, "?AS/Weapon_Sword.root.asb"),
                (NAME_REF, "?ActorSystem/ActorName/Item_Odd.engine__actor__ActorName.bgyml"),
                (MODEL_REF, "?Component/ModelInfo/Item_Test.engine__component__ModelInfo.bgyml"),
            ]),
        )]);
        p.add(&actor_file("Item_Odd"), &odd).unwrap();
        let sp: Spec = Spec { item: "Item_Odd".to_string(), name: "Obj_Odd".to_string(), style: Style::Touch, heap: None, source: None, identity: None };
        let bad: Vec<String> = fits(&p, &sp).unwrap();
        assert_eq!(bad.len(), 3);
        assert!(make(&p, &sp).is_err());
        let own: Spec = Spec { item: DONOR.to_string(), name: "Obj_Twice".to_string(), style: Style::Touch, heap: None, source: None, identity: None };
        assert!(fits(&p, &own).unwrap().iter().any(|m| m.contains("already a world pickup")));
    }

    fn anim(first: &str) -> Vec<u8> {
        let mut a: Node = Node::Map(Vec::new());
        a.set(ANIM_NAME, s(first));
        let mut b: Node = Node::Map(Vec::new());
        b.set(ANIM_NAME, s("Item_Long_Animation"));
        file(&[(ANIM_LIST, Node::Arr(vec![a, b]))])
    }

    fn long(p: &mut Pool, name: &str, first: &str) -> Spec {
        let f: String = format!("Component/AnimationParam/{name}.engine__component__AnimationParam.bgyml");
        p.add(&f, &anim(first)).unwrap();
        let y: Vec<u8> = file(&[(
            COMPONENTS,
            comps(&[
                (AS_REF, "?AS/Pouch_Long.root.asb"),
                (NAME_REF, "?ActorSystem/ActorName/Item_Long.engine__actor__ActorName.bgyml"),
                (ANIM_REF, &format!("?{f}")),
                (MODEL_REF, "?Component/ModelInfo/Item_Test.engine__component__ModelInfo.bgyml"),
                (POUCH_REF, "?Component/PouchContentParam/Item_Test.game__component__PouchContentParam.bgyml"),
            ]),
        )]);
        p.add(&actor_file(name), &y).unwrap();
        Spec { item: name.to_string(), name: format!("Obj_{name}"), style: Style::Touch, heap: None, source: None, identity: None }
    }

    #[test]
    fn another_animation_set_is_accepted_only_when_the_shared_animations_come_first() {
        let mut p: Pool = pool();
        let good: Spec = long(&mut p, "Item_Long", SHARED_ANIM);
        assert!(fits(&p, &good).unwrap().is_empty());
        let m: Made = make(&p, &good).unwrap();
        let y: Byml = Byml::parse(&m.actor.1).unwrap();
        assert_eq!(y.root.get(COMPONENTS).and_then(|c| c.get(AS_REF)).and_then(|v| v.as_str()), Some("?AS/Item_Common.root.asb"));
        let second: Spec = long(&mut p, "Item_Late", "Item_Long_Animation");
        let bad: Vec<String> = fits(&p, &second).unwrap();
        assert_eq!(bad.len(), 1);
        assert!(bad[0].contains("animation set"));
        assert!(make(&p, &second).is_err());
        assert!(!shared_anim(&p, &chain(&p, "Item_Test").unwrap()));
        assert!(fits(&p, &spec(Style::Touch)).unwrap().is_empty());
    }

    #[test]
    fn the_spin_layer_plays_the_turn_and_adds_no_effect() {
        let on: Layers = Layers { spin: true, ..Layers::default() };
        let m: Made = make_with(&pool(), &spec(Style::Touch), &on).unwrap();
        assert_eq!(m.extra.iter().map(|e| e.0.clone()).collect::<Vec<String>>(), vec![spin_file("Obj_Test")]);
        let y: Byml = Byml::parse(&m.actor.1).unwrap();
        let c: &Node = y.root.get(COMPONENTS).unwrap();
        let text = |k: &str| c.get(k).and_then(|v| v.as_str()).map(|x| x.to_string());
        assert_eq!(text(AS_INFO_REF).as_deref(), Some("?Component/ASInfo/Obj_Test.engine__component__ASInfo.bgyml"));
        assert_eq!(text(ELINK_REF), None);
        assert_eq!(text("XLinkRef"), None);
        let own = |f: &str| -> Byml { Byml::parse(&m.extra.iter().find(|e| e.0 == f).unwrap().1).unwrap() };
        let spin: Byml = own(&spin_file("Obj_Test"));
        assert_eq!(spin.root.get(PARENT).and_then(|v| v.as_str()), Some("Work/Component/ASInfo/Item_Obj.engine__component__ASInfo.gyml"));
        assert_eq!(spin.root.get(AUTO_PLAY).and_then(|v| v.as_str()), Some(SPIN));
    }

    #[test]
    fn the_magic_layer_is_off_unless_asked_for() {
        let p: Pool = pool();
        let m: Made = make(&p, &spec(Style::Touch)).unwrap();
        assert!(m.extra.is_empty());
        assert_eq!(make_with(&p, &spec(Style::Touch), &Layers::default()).unwrap(), m);
        let y: Byml = Byml::parse(&m.actor.1).unwrap();
        assert!(y.root.get(COMPONENTS).and_then(|c| c.get(magic::TABLE_REF)).is_none());
        let on: Layers = Layers { magic: true, ..Layers::default() };
        assert!(make_with(&p, &spec(Style::Touch), &on).is_err());
    }
}


