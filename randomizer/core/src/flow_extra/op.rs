use crate::bfevfl::{Actor, Bf, Case, Cont, Event, Flow, Kind, Val};
use crate::error::{Error as E, Res};
use crate::flow_locations::{set_param, Req};

pub const ITEM: &str = "$item";
const HOLDER: &str = "GameDataActor";
const SET: &str = "EventTriggerSetGameDataBool";
const ASK: &str = "EventQueryIsGameDataBool";
const PLAYER: &str = "Player_Event";
const CHILD: &str = "EventQueryPlayerIsChild";
const OWNED: &str = "EventQueryCheckIsGet";
const POUCH: &str = "PouchSystemActor";
const ADD: &str = "EventTriggerAddOrRemovePouchContent";

#[derive(Clone, Debug, PartialEq)]
pub enum How {
    Retarget { case: u32, old: u16, new: u16 },
    SwapSub { from: String, to: String, params: Vec<(String, Val)> },
    Query { from: String, to: String, params: Vec<(String, Val)> },
    Set { param: String, old: String, new: String },
    Insert { flag: String },
    Rename { old: String, new: String },
    Clone { param: String, old: String, new: String },
    Once(Once),
    Next { old: u16, new: u16 },
    Backfill(Backfill),
}

/// Placed after an action whose next event is `old`: a player who owns `have` but not `lack`
/// is handed every actor of `give`, in order, the way a chest hands an item over, and then goes
/// on to `old`. Everyone else goes straight on.
#[derive(Clone, Debug, PartialEq)]
pub struct Backfill {
    pub old: u16,
    pub have: String,
    pub lack: String,
    pub give: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Once {
    pub entry: String,
    pub flag: String,
    pub child: bool,
    pub adult: bool,
    pub sub: String,
    pub params: Vec<(String, Val)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Op {
    pub file: String,
    pub index: usize,
    pub event: String,
    pub how: How,
}

fn sub_item(s: &str, item: &str) -> String {
    if s == ITEM { item.to_string() } else { s.to_string() }
}

fn bind_val(v: &Val, item: &str) -> Val {
    match v {
        Val::Str(s) => Val::Str(sub_item(s, item)),
        Val::Actor(s, t) => Val::Actor(sub_item(s, item), t.clone()),
        x => x.clone(),
    }
}

fn bind_params(p: &[(String, Val)], item: &str) -> Vec<(String, Val)> {
    p.iter().map(|(k, v)| (k.clone(), bind_val(v, item))).collect()
}

impl Op {
    pub fn bind(&self, item: &str) -> Op {
        let how: How = match &self.how {
            How::SwapSub { from, to, params } => How::SwapSub { from: from.clone(), to: to.clone(), params: bind_params(params, item) },
            How::Query { from, to, params } => How::Query { from: from.clone(), to: to.clone(), params: bind_params(params, item) },
            How::Set { param, old, new } => How::Set { param: param.clone(), old: old.clone(), new: sub_item(new, item) },
            How::Rename { old, new } => How::Rename { old: old.clone(), new: sub_item(new, item) },
            How::Clone { param, old, new } => How::Clone { param: param.clone(), old: old.clone(), new: sub_item(new, item) },
            How::Once(w) => How::Once(Once { params: bind_params(&w.params, item), ..w.clone() }),
            x => x.clone(),
        };
        Op { file: self.file.clone(), index: self.index, event: self.event.clone(), how }
    }

    pub fn kind(&self) -> &'static str {
        match self.how {
            How::Retarget { .. } => "retarget",
            How::SwapSub { .. } => "swap_sub",
            How::Query { .. } => "query",
            How::Set { .. } => "set",
            How::Insert { .. } => "insert_set",
            How::Rename { .. } => "rename_actor",
            How::Clone { .. } => "clone_sub",
            How::Once(_) => "once_grant",
            How::Next { .. } => "next",
            How::Backfill(_) => "backfill",
        }
    }
}

fn idx(n: usize) -> Res<u16> {
    u16::try_from(n).map_err(|_| E::Format("flow extra: index does not fit in 16 bits"))
}

fn event<'a>(f: &'a mut Flow, i: usize, name: &str) -> Res<&'a mut Event> {
    let e: &mut Event = f.events.get_mut(i).ok_or_else(|| E::Msg(format!("event index {i} is out of range")))?;
    if e.name != name {
        return Err(E::Msg(format!("event {i} is named {} and not {name}, the game files differ", e.name)));
    }
    Ok(e)
}

fn holder(f: &mut Flow, who: &str) -> Res<usize> {
    if let Some(i) = f.actors.iter().position(|a| a.name == who && a.sub.is_empty()) {
        return Ok(i);
    }
    let t: &Actor = f.actors.first().ok_or(E::Format("flow extra: the flow has no actors"))?;
    let a: Actor = Actor {
        name: who.to_string(),
        sub: String::new(),
        arg: String::new(),
        actions: Vec::new(),
        queries: Vec::new(),
        params: t.params.clone(),
        ep: t.ep,
        cut: t.cut,
    };
    f.actors.push(a);
    Ok(f.actors.len() - 1)
}

fn slot(f: &mut Flow, who: &str, name: &str, query: bool) -> Res<(u16, u16)> {
    let i: usize = holder(f, who)?;
    let a: &mut Actor = &mut f.actors[i];
    let list: &mut Vec<String> = if query { &mut a.queries } else { &mut a.actions };
    let j: usize = match list.iter().position(|x| x == name) {
        Some(j) => j,
        None => {
            list.push(name.to_string());
            list.len() - 1
        }
    };
    Ok((idx(i)?, idx(j)?))
}

fn split<'a>(s: &'a str, sep: char) -> Res<(&'a str, &'a str)> {
    s.split_once(sep).ok_or_else(|| E::Msg(format!("flow extra: {s} has no {sep}")))
}

fn set_bool(flag: &str) -> Cont {
    Cont {
        items: vec![
            ("Index".to_string(), Val::Int(-1)),
            ("Value".to_string(), Val::Bool(1)),
            ("GameDataName".to_string(), Val::Str(flag.to_string())),
        ],
    }
}

fn retarget(f: &mut Flow, o: &Op, case: u32, old: u16, new: u16) -> Res<()> {
    let n: usize = f.events.len();
    if usize::from(new) >= n {
        return Err(E::Msg(format!("retarget of event {} points at {new}, past the {n} events", o.index)));
    }
    let e: &mut Event = event(f, o.index, &o.event)?;
    let cases: &mut Vec<Case> = match &mut e.kind {
        Kind::Switch { cases, .. } | Kind::SubSwitch { cases, .. } => cases,
        _ => return Err(E::Msg(format!("event {} is not a switch", o.event))),
    };
    let c: &mut Case = cases
        .iter_mut()
        .find(|c| c.val == case)
        .ok_or_else(|| E::Msg(format!("switch {} has no case {case}", o.event)))?;
    if c.ev != old {
        return Err(E::Msg(format!("case {case} of {} goes to {} and not {old}, the game files differ", o.event, c.ev)));
    }
    c.ev = new;
    Ok(())
}

fn swap_sub(f: &mut Flow, o: &Op, from: &str, to: &str, params: &[(String, Val)]) -> Res<()> {
    let (tf, te): (&str, &str) = split(to, ':')?;
    let e: &mut Event = event(f, o.index, &o.event)?;
    let Kind::Sub { flow, entry, params: p, .. } = &mut e.kind else {
        return Err(E::Msg(format!("event {} is not a sub flow call", o.event)));
    };
    let have: String = format!("{flow}:{entry}");
    if have != from {
        return Err(E::Msg(format!("event {} calls {have} and not {from}, the game files differ", o.event)));
    }
    *flow = tf.to_string();
    *entry = te.to_string();
    *p = Some(Cont { items: params.to_vec() });
    Ok(())
}

fn query(f: &mut Flow, o: &Op, from: &str, to: &str, params: &[(String, Val)]) -> Res<()> {
    let (ta, tq): (&str, &str) = split(to, '.')?;
    let (a, q): (usize, usize) = {
        let e: &mut Event = event(f, o.index, &o.event)?;
        match &e.kind {
            Kind::Switch { actor, query, .. } => (usize::from(*actor), usize::from(*query)),
            _ => return Err(E::Msg(format!("event {} is not a query switch", o.event))),
        }
    };
    let x: &Actor = f.actors.get(a).ok_or(E::Format("flow extra: switch actor is out of range"))?;
    let n: &String = x.queries.get(q).ok_or(E::Format("flow extra: switch query is out of range"))?;
    let have: String = format!("{}.{n}", x.name);
    if have != from {
        return Err(E::Msg(format!("event {} asks {have} and not {from}, the game files differ", o.event)));
    }
    let (na, nq): (u16, u16) = slot(f, ta, tq, true)?;
    let e: &mut Event = event(f, o.index, &o.event)?;
    if let Kind::Switch { actor, query, params: p, .. } = &mut e.kind {
        *actor = na;
        *query = nq;
        *p = if params.is_empty() { None } else { Some(Cont { items: params.to_vec() }) };
    }
    Ok(())
}

fn insert(f: &mut Flow, o: &Op, flag: &str) -> Res<()> {
    let at: u16 = idx(f.events.len())?;
    let after: u16 = {
        let e: &mut Event = event(f, o.index, &o.event)?;
        match &mut e.kind {
            Kind::Action { next, .. } | Kind::Sub { next, .. } => {
                let old: u16 = *next;
                *next = at;
                old
            }
            _ => return Err(E::Msg(format!("event {} has no single next event", o.event))),
        }
    };
    let (a, x): (u16, u16) = slot(f, HOLDER, SET, false)?;
    f.events.push(Event { name: format!("Extra{at}"), kind: Kind::Action { next: after, actor: a, act: x, params: Some(set_bool(flag)) } });
    Ok(())
}

fn next(f: &mut Flow, o: &Op, old: u16, new: u16) -> Res<()> {
    let n: usize = f.events.len();
    if usize::from(new) >= n {
        return Err(E::Msg(format!("next of event {} points at {new}, past the {n} events", o.index)));
    }
    let e: &mut Event = event(f, o.index, &o.event)?;
    let at: &mut u16 = match &mut e.kind {
        Kind::Action { next, .. } | Kind::Sub { next, .. } => next,
        _ => return Err(E::Msg(format!("event {} has no single next event", o.event))),
    };
    if *at != old {
        return Err(E::Msg(format!("event {} goes to {} and not {old}, the game files differ", o.event, *at)));
    }
    *at = new;
    Ok(())
}

fn push(f: &mut Flow, kind: Kind) -> Res<u16> {
    let at: u16 = idx(f.events.len())?;
    f.events.push(Event { name: format!("Extra{at}"), kind });
    Ok(at)
}

fn flag_ask(flag: &str) -> Cont {
    Cont { items: vec![("Index".to_string(), Val::Int(-1)), ("GameDataName".to_string(), Val::Str(flag.to_string()))] }
}

fn stacked(f: &Flow, cur: u16, main: u16, val: u32) -> bool {
    let Some(Kind::Switch { actor, query, cases, .. }) = f.events.get(usize::from(cur)).map(|e| &e.kind) else {
        return false;
    };
    let Some(a) = f.actors.get(usize::from(*actor)) else {
        return false;
    };
    a.name == PLAYER && a.queries.get(usize::from(*query)).map(|q| q.as_str()) == Some(CHILD) && cases.iter().any(|c| c.val == val && c.ev == main)
}

fn once(f: &mut Flow, o: &Op, w: &Once) -> Res<u16> {
    let at: usize = f
        .entries
        .iter()
        .position(|e| e.name == w.entry)
        .ok_or_else(|| E::Msg(format!("the flow has no entry {}, the game files differ", w.entry)))?;
    if w.child && w.adult {
        return Err(E::Msg(format!("the once gate of {} asks for a child and an adult at once", w.entry)));
    }
    let cur: u16 = f.entries[at].main;
    let main: u16 = idx(o.index)?;
    let val: u32 = if w.adult { 1 } else { 0 };
    let aged: bool = w.child || w.adult;
    let stack: bool = cur != main && aged && stacked(f, cur, main, val);
    if cur != main && !stack {
        return Err(E::Msg(format!("entry {} starts at event {cur} and not {}, the game files differ", w.entry, o.index)));
    }
    event(f, o.index, &o.event)?;
    let (tf, te): (&str, &str) = split(&w.sub, ':')?;
    let (qa, qi): (u16, u16) = slot(f, HOLDER, ASK, true)?;
    let (sa, si): (u16, u16) = slot(f, HOLDER, SET, false)?;
    let grant: u16 = push(f, Kind::Sub { next: main, params: Some(Cont { items: w.params.clone() }), flow: tf.to_string(), entry: te.to_string() })?;
    let set: u16 = push(f, Kind::Action { next: grant, actor: sa, act: si, params: Some(set_bool(&w.flag)) })?;
    let mut head: u16 = push(f, Kind::Switch { actor: qa, query: qi, cases: vec![Case { val: 0, ev: main }, Case { val: 1, ev: set }], params: Some(flag_ask(&w.flag)) })?;
    if stack {
        if let Kind::Switch { cases, .. } = &mut f.events[usize::from(cur)].kind {
            for c in cases.iter_mut().filter(|c| c.val == val) {
                c.ev = head;
            }
        }
        head = cur;
    } else if aged {
        let (pa, pi): (u16, u16) = slot(f, PLAYER, CHILD, true)?;
        let (kid, grown): (u16, u16) = if w.child { (head, main) } else { (main, head) };
        head = push(f, Kind::Switch { actor: pa, query: pi, cases: vec![Case { val: 0, ev: kid }, Case { val: 1, ev: grown }], params: None })?;
    }
    f.entries[at].main = head;
    f.entries[at].subs.push(grant);
    Ok(grant)
}

pub fn progressive_once(b: &mut Bf, o: &Op, family: &str, forms: &[String]) -> Res<()> {
    let How::Once(w) = &o.how else {
        return Err(E::Format("flow extra: a progressive once grant needs a once operation"));
    };
    let f: &mut Flow = b.flows.first_mut().ok_or(E::Format("flow extra: the file holds no flow"))?;
    let grant: u16 = once(f, o, w)?;
    let name: String = f.events[usize::from(grant)].name.clone();
    crate::progressive::wrap_grant_as(f, usize::from(grant), &name, "ActorName", family, forms)
}

fn owns(actor: &str) -> Cont {
    Cont { items: vec![("ActorName".to_string(), Val::Str(actor.to_string()))] }
}

fn got(actor: &str) -> Cont {
    Cont {
        items: vec![
            ("Num".to_string(), Val::Int(1)),
            ("IsEquipDIrect".to_string(), Val::Bool(0)),
            ("IsSetMessageTargetActorName".to_string(), Val::Bool(0)),
            ("Actor".to_string(), Val::Str(actor.to_string())),
            ("IsGetEvent".to_string(), Val::Bool(1)),
        ],
    }
}

fn backfill(f: &mut Flow, o: &Op, w: &Backfill) -> Res<()> {
    if w.give.is_empty() {
        return Err(E::Msg(format!("the backfill after {} gives nothing", o.event)));
    }
    match &event(f, o.index, &o.event)?.kind {
        Kind::Action { next, .. } if *next == w.old => {}
        Kind::Action { next, .. } => return Err(E::Msg(format!("event {} goes to {next} and not {}, the game files differ", o.event, w.old))),
        _ => return Err(E::Msg(format!("event {} is not an action", o.event))),
    }
    let (qa, qi): (u16, u16) = slot(f, HOLDER, OWNED, true)?;
    let (pa, pi): (u16, u16) = slot(f, POUCH, ADD, false)?;
    let mut at: u16 = w.old;
    for g in w.give.iter().rev() {
        at = push(f, Kind::Action { next: at, actor: pa, act: pi, params: Some(got(g)) })?;
    }
    let lack: u16 = push(f, Kind::Switch { actor: qa, query: qi, cases: vec![Case { val: 0, ev: w.old }, Case { val: 1, ev: at }], params: Some(owns(&w.lack)) })?;
    let have: u16 = push(f, Kind::Switch { actor: qa, query: qi, cases: vec![Case { val: 0, ev: lack }, Case { val: 1, ev: w.old }], params: Some(owns(&w.have)) })?;
    if let Kind::Action { next, .. } = &mut event(f, o.index, &o.event)?.kind {
        *next = have;
    }
    Ok(())
}

fn rename(f: &mut Flow, old: &str, new: &str) -> Res<()> {
    if f.actors.iter().any(|a| a.name == new && a.sub.is_empty()) {
        return Ok(());
    }
    let a: &mut Actor = f
        .actors
        .iter_mut()
        .find(|a| a.name == old && a.sub.is_empty())
        .ok_or_else(|| E::Msg(format!("the flow has no actor {old}, the game files differ")))?;
    a.name = new.to_string();
    Ok(())
}

pub fn apply(b: &mut Bf, o: &Op) -> Res<()> {
    if let How::Set { param, old, new } = &o.how {
        return set_param(b, &Req { index: o.index, event: o.event.clone(), param: param.clone(), old: old.clone(), new: new.clone() });
    }
    let f: &mut Flow = b.flows.first_mut().ok_or(E::Format("flow extra: the file holds no flow"))?;
    match &o.how {
        How::Retarget { case, old, new } => retarget(f, o, *case, *old, *new),
        How::SwapSub { from, to, params } => swap_sub(f, o, from, to, params),
        How::Query { from, to, params } => query(f, o, from, to, params),
        How::Insert { flag } => insert(f, o, flag),
        How::Rename { old, new } => rename(f, old, new),
        How::Clone { param, old, new } => crate::flow_extra::clone::clone_sub(f, o.index, &o.event, param, old, new),
        How::Once(w) => once(f, o, w).map(|_| ()),
        How::Next { old, new } => next(f, o, *old, *new),
        How::Backfill(w) => backfill(f, o, w),
        How::Set { .. } => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bfevfl::flow::NONE;
    use crate::bfevfl::Entry;

    fn actor(n: &str, acts: &[&str], qs: &[&str]) -> Actor {
        Actor {
            name: n.to_string(),
            sub: String::new(),
            arg: String::new(),
            actions: acts.iter().map(|s| s.to_string()).collect(),
            queries: qs.iter().map(|s| s.to_string()).collect(),
            params: None,
            ep: 0xFFFF,
            cut: 1,
        }
    }

    fn sample() -> Bf {
        let p = |k: &str, v: &str| Some(Cont { items: vec![(k.to_string(), Val::Str(v.to_string()))] });
        Bf {
            ver: [0, 4, 0, 0],
            bom: 0xFEFF,
            shift: 3,
            pad: 0,
            name: "T".to_string(),
            flows: vec![Flow {
                name: "T".to_string(),
                actors: vec![actor("PouchSystemActor", &[], &["EventQueryIsExistInPouch"]), actor("Item_A", &[], &[])],
                events: vec![
                    Event {
                        name: "Event0".to_string(),
                        kind: Kind::Switch { actor: 0, query: 0, cases: vec![Case { val: 0, ev: 1 }, Case { val: 1, ev: 2 }], params: p("Actor", "Item_A") },
                    },
                    Event { name: "Event1".to_string(), kind: Kind::Sub { next: NONE, params: p("ActorName", "Item_A"), flow: "F".to_string(), entry: "Old".to_string() } },
                    Event { name: "Event2".to_string(), kind: Kind::Sub { next: 1, params: p("ActorName", "Item_A"), flow: String::new(), entry: "Local".to_string() } },
                ],
                entries: vec![Entry { name: "Start".to_string(), subs: vec![1, 2], vars: Vec::new(), main: 0, pad: 0 }],
            }],
            tls: Vec::new(),
        }
    }

    fn op(i: usize, how: How) -> Op {
        Op { file: "T.bfevfl".to_string(), index: i, event: format!("Event{i}"), how }
    }

    fn round(b: &Bf) -> Bf {
        Bf::parse(&b.write().unwrap()).unwrap()
    }

    #[test]
    fn retarget_moves_one_case_and_checks_the_old_target() {
        let mut b: Bf = sample();
        apply(&mut b, &op(0, How::Retarget { case: 1, old: 2, new: 1 })).unwrap();
        let Kind::Switch { cases, .. } = &round(&b).flows[0].events[0].kind else { panic!("switch") };
        assert_eq!(cases, &vec![Case { val: 0, ev: 1 }, Case { val: 1, ev: 1 }]);
        assert!(apply(&mut sample(), &op(0, How::Retarget { case: 1, old: 7, new: 1 })).is_err());
        assert!(apply(&mut sample(), &op(0, How::Retarget { case: 5, old: 2, new: 1 })).is_err());
        assert!(apply(&mut sample(), &op(0, How::Retarget { case: 1, old: 2, new: 9 })).is_err());
        assert!(apply(&mut sample(), &op(1, How::Retarget { case: 1, old: 2, new: 1 })).is_err());
    }

    #[test]
    fn swap_sub_changes_the_callee_and_replaces_the_parameters() {
        let mut b: Bf = sample();
        let ps: Vec<(String, Val)> = vec![("ActorName".to_string(), Val::Str(ITEM.to_string())), ("Target".to_string(), Val::Int(3))];
        let o: Op = op(1, How::SwapSub { from: "F:Old".to_string(), to: "F:New".to_string(), params: ps }).bind("Item_B");
        apply(&mut b, &o).unwrap();
        let Kind::Sub { flow, entry, params, .. } = &round(&b).flows[0].events[1].kind else { panic!("sub") };
        assert_eq!((flow.as_str(), entry.as_str()), ("F", "New"));
        let c: &Cont = params.as_ref().unwrap();
        assert_eq!(c.get("ActorName"), Some(&Val::Str("Item_B".to_string())));
        assert_eq!(c.get("Target"), Some(&Val::Int(3)));
        assert!(apply(&mut sample(), &op(1, How::SwapSub { from: "F:Other".to_string(), to: "F:New".to_string(), params: Vec::new() })).is_err());
        assert!(apply(&mut sample(), &op(0, How::SwapSub { from: "F:Old".to_string(), to: "F:New".to_string(), params: Vec::new() })).is_err());
    }

    #[test]
    fn query_swaps_actor_and_query_and_adds_them_when_missing() {
        let mut b: Bf = sample();
        let ps: Vec<(String, Val)> = vec![("GameDataName".to_string(), Val::Str("Flag".to_string())), ("Index".to_string(), Val::Int(-1))];
        let o: Op = op(0, How::Query { from: "PouchSystemActor.EventQueryIsExistInPouch".to_string(), to: "GameDataActor.EventQueryIsGameDataBool".to_string(), params: ps });
        apply(&mut b, &o).unwrap();
        let f: &Flow = &round(&b).flows[0];
        let Kind::Switch { actor, query, cases, params } = &f.events[0].kind else { panic!("switch") };
        assert_eq!(f.actors[*actor as usize].name, "GameDataActor");
        assert_eq!(f.actors[*actor as usize].queries[*query as usize], "EventQueryIsGameDataBool");
        assert_eq!(cases.len(), 2);
        assert_eq!(params.as_ref().and_then(|c| c.get("GameDataName")), Some(&Val::Str("Flag".to_string())));
        assert!(apply(&mut b, &o).is_err());
    }

    #[test]
    fn insert_set_splices_a_flag_write_after_an_event() {
        let mut b: Bf = sample();
        apply(&mut b, &op(2, How::Insert { flag: "Sold".to_string() })).unwrap();
        let f: &Flow = &round(&b).flows[0];
        assert_eq!(f.events.len(), 4);
        let Kind::Sub { next, .. } = &f.events[2].kind else { panic!("sub") };
        assert_eq!(*next, 3);
        let Kind::Action { next, actor, act, params } = &f.events[3].kind else { panic!("action") };
        assert_eq!(*next, 1);
        assert_eq!(f.actors[*actor as usize].name, HOLDER);
        assert_eq!(f.actors[*actor as usize].actions[*act as usize], SET);
        assert_eq!(params.as_ref().and_then(|c| c.get("Value")), Some(&Val::Bool(1)));
        assert!(apply(&mut sample(), &op(0, How::Insert { flag: "Sold".to_string() })).is_err());
    }

    #[test]
    fn set_and_rename_follow_the_chosen_item() {
        let mut b: Bf = sample();
        apply(&mut b, &op(2, How::Set { param: "ActorName".to_string(), old: "Item_A".to_string(), new: ITEM.to_string() }).bind("Item_C")).unwrap();
        apply(&mut b, &op(0, How::Rename { old: "Item_A".to_string(), new: ITEM.to_string() }).bind("Item_C")).unwrap();
        let f: &Flow = &round(&b).flows[0];
        assert_eq!(f.events[2].kind.params().and_then(|c| c.get("ActorName")), Some(&Val::Str("Item_C".to_string())));
        assert_eq!(f.actors[1].name, "Item_C");
        assert!(apply(&mut sample(), &op(0, How::Rename { old: "Item_Z".to_string(), new: "Item_C".to_string() })).is_err());
        let mut twice: Bf = sample();
        apply(&mut twice, &op(0, How::Rename { old: "Item_A".to_string(), new: "PouchSystemActor".to_string() })).unwrap();
        assert_eq!(twice, sample());
    }

    #[test]
    fn next_moves_the_follow_up_of_one_event_and_checks_the_old_one() {
        let mut b: Bf = sample();
        apply(&mut b, &op(2, How::Next { old: 1, new: 0 })).unwrap();
        let Kind::Sub { next, .. } = &round(&b).flows[0].events[2].kind else { panic!("sub") };
        assert_eq!(*next, 0);
        assert!(apply(&mut sample(), &op(2, How::Next { old: 0, new: 1 })).is_err());
        assert!(apply(&mut sample(), &op(2, How::Next { old: 1, new: 9 })).is_err());
        assert!(apply(&mut sample(), &op(0, How::Next { old: 1, new: 2 })).is_err());
    }

    fn once_op(child: bool) -> Op {
        let w: Once = Once {
            entry: "Start".to_string(),
            flag: "Flag_Once".to_string(),
            child,
            adult: false,
            sub: "G:Grant".to_string(),
            params: vec![("ActorName".to_string(), Val::Str(ITEM.to_string())), ("IsNpcEventStarter".to_string(), Val::Bool(0))],
        };
        op(0, How::Once(w)).bind("Item_C")
    }

    #[test]
    fn once_puts_a_flag_gate_in_front_of_the_entry() {
        let mut b: Bf = sample();
        apply(&mut b, &once_op(false)).unwrap();
        let f: &Flow = &round(&b).flows[0];
        assert_eq!(f.events.len(), 6);
        assert_eq!(f.entries[0].main, 5);
        assert_eq!(f.entries[0].subs, vec![1, 2, 3]);
        let Kind::Switch { actor, query, cases, params } = &f.events[5].kind else { panic!("switch") };
        assert_eq!(f.actors[*actor as usize].name, HOLDER);
        assert_eq!(f.actors[*actor as usize].queries[*query as usize], ASK);
        assert_eq!(cases, &vec![Case { val: 0, ev: 0 }, Case { val: 1, ev: 4 }]);
        assert_eq!(params.as_ref().and_then(|c| c.get("GameDataName")), Some(&Val::Str("Flag_Once".to_string())));
        let Kind::Action { next, actor, act, params } = &f.events[4].kind else { panic!("action") };
        assert_eq!(*next, 3);
        assert_eq!(f.actors[*actor as usize].actions[*act as usize], SET);
        assert_eq!(params.as_ref().and_then(|c| c.get("Value")), Some(&Val::Bool(1)));
        let Kind::Sub { next, params, flow, entry } = &f.events[3].kind else { panic!("sub") };
        assert_eq!((*next, flow.as_str(), entry.as_str()), (0, "G", "Grant"));
        assert_eq!(params.as_ref().and_then(|c| c.get("ActorName")), Some(&Val::Str("Item_C".to_string())));
        assert_eq!(f.events[0].kind, sample().flows[0].events[0].kind);
    }

    #[test]
    fn progressive_once_selects_the_tier_before_the_unchanged_milk_path() {
        let mut b: Bf = sample();
        let original: Vec<Event> = b.flows[0].events.clone();
        progressive_once(&mut b, &once_op(false), "Hookshot", &[]).unwrap();
        let f: &Flow = &round(&b).flows[0];
        assert_eq!(&f.events[..original.len()], original.as_slice());
        let Kind::Switch { cases: gate, .. } = &f.events[usize::from(f.entries[0].main)].kind else { panic!("flag gate") };
        assert_eq!(gate.iter().find(|c| c.val == 0).unwrap().ev, 0);
        let set: u16 = gate.iter().find(|c| c.val == 1).unwrap().ev;
        let Kind::Action { next: grant, params, .. } = &f.events[usize::from(set)].kind else { panic!("set flag") };
        assert_eq!(params.as_ref().and_then(|c| c.get("GameDataName")), Some(&Val::Str("Flag_Once".to_string())));
        let Kind::Switch { cases, params, .. } = &f.events[usize::from(*grant)].kind else { panic!("tier check") };
        assert_eq!(params.as_ref().and_then(|c| c.get("ActorName")), Some(&Val::Str("Item_Hookshot".to_string())));
        for (owned, item) in [(0, "Item_Longshot"), (1, "Item_Hookshot")] {
            let at: u16 = cases.iter().find(|c| c.val == owned).unwrap().ev;
            let Kind::Sub { next, flow, entry, params } = &f.events[usize::from(at)].kind else { panic!("tier grant") };
            assert_eq!((*next, flow.as_str(), entry.as_str()), (0, "G", "Grant"));
            assert_eq!(params.as_ref().and_then(|c| c.get("ActorName")), Some(&Val::Str(item.to_string())));
            assert!(f.entries[0].subs.contains(&at));
        }
        assert!(!f.entries[0].subs.contains(grant));
    }

    #[test]
    fn once_with_the_child_check_leaves_adults_on_the_old_path() {
        let mut b: Bf = sample();
        apply(&mut b, &once_op(true)).unwrap();
        let f: &Flow = &round(&b).flows[0];
        assert_eq!(f.events.len(), 7);
        assert_eq!(f.entries[0].main, 6);
        let Kind::Switch { actor, query, cases, params } = &f.events[6].kind else { panic!("switch") };
        assert_eq!(f.actors[*actor as usize].name, PLAYER);
        assert_eq!(f.actors[*actor as usize].queries[*query as usize], CHILD);
        assert_eq!(cases, &vec![Case { val: 0, ev: 5 }, Case { val: 1, ev: 0 }]);
        assert!(params.is_none());
    }

    fn aged(child: bool, adult: bool, flag: &str) -> Op {
        let mut o: Op = once_op(child);
        if let How::Once(w) = &mut o.how {
            w.adult = adult;
            w.flag = flag.to_string();
        }
        o
    }

    fn flag_of(f: &Flow, i: u16) -> (String, Vec<Case>) {
        let Kind::Switch { params, cases, .. } = &f.events[usize::from(i)].kind else { panic!("switch") };
        let Some(Val::Str(s)) = params.as_ref().and_then(|c| c.get("GameDataName")) else { panic!("flag") };
        (s.clone(), cases.clone())
    }

    #[test]
    fn an_adult_gate_and_a_child_gate_share_one_age_switch_in_either_order() {
        let mut lone: Bf = sample();
        apply(&mut lone, &aged(false, true, "Flag_Old")).unwrap();
        let f: &Flow = &round(&lone).flows[0];
        let Kind::Switch { cases, .. } = &f.events[usize::from(f.entries[0].main)].kind else { panic!("switch") };
        assert_eq!(cases[0], Case { val: 0, ev: 0 });
        assert_eq!(flag_of(f, cases[1].ev).0, "Flag_Old");
        for kid_first in [true, false] {
            let (c, a): (Op, Op) = (aged(true, false, "Flag_Kid"), aged(false, true, "Flag_Old"));
            let ops: [Op; 2] = if kid_first { [c.clone(), a.clone()] } else { [a.clone(), c.clone()] };
            let mut b: Bf = sample();
            apply(&mut b, &ops[0]).unwrap();
            apply(&mut b, &ops[1]).unwrap();
            let f: &Flow = &round(&b).flows[0];
            assert_eq!(f.events.len(), 3 + 3 + 3 + 1, "{kid_first}");
            let Kind::Switch { actor, query, cases, .. } = &f.events[usize::from(f.entries[0].main)].kind else { panic!("switch") };
            assert_eq!(f.actors[*actor as usize].name, PLAYER);
            assert_eq!(f.actors[*actor as usize].queries[*query as usize], CHILD);
            for (v, flag) in [(0, "Flag_Kid"), (1, "Flag_Old")] {
                let ev: u16 = cases.iter().find(|x| x.val == v).unwrap().ev;
                let (name, fc): (String, Vec<Case>) = flag_of(f, ev);
                assert_eq!(name, flag, "{kid_first}");
                assert_eq!(fc[0], Case { val: 0, ev: 0 });
            }
            assert_eq!(f.entries[0].subs.len(), 4);
            let after: Bf = b.clone();
            assert!(apply(&mut b, &ops[0]).is_err());
            assert_eq!(b, after);
        }
        assert!(apply(&mut sample(), &aged(true, true, "Flag_Both")).is_err());
    }

    #[test]
    fn progressive_and_plain_gerudo_cow_gates_remain_independent() {
        for both_progressive in [false, true] {
            for kid_first in [false, true] {
                let kid: Op = aged(true, false, "Flag_Kid");
                let adult: Op = aged(false, true, "Flag_Adult");
                let mut b: Bf = sample();
                let add_kid = |b: &mut Bf| progressive_once(b, &kid, "Hookshot", &[]).unwrap();
                let add_adult = |b: &mut Bf| {
                    if both_progressive {
                        progressive_once(b, &adult, "Wallet", &[]).unwrap();
                    } else {
                        apply(b, &adult).unwrap();
                    }
                };
                if kid_first {
                    add_kid(&mut b);
                    add_adult(&mut b);
                } else {
                    add_adult(&mut b);
                    add_kid(&mut b);
                }
                let f: &Flow = &round(&b).flows[0];
                assert_eq!(&f.events[..3], sample().flows[0].events.as_slice());
                let Kind::Switch { cases, .. } = &f.events[usize::from(f.entries[0].main)].kind else { panic!("age gate") };
                for (age, flag, progressive) in [(0, "Flag_Kid", true), (1, "Flag_Adult", both_progressive)] {
                    let at: u16 = cases.iter().find(|c| c.val == age).unwrap().ev;
                    let (name, flags): (String, Vec<Case>) = flag_of(f, at);
                    assert_eq!(name, flag);
                    assert_eq!(flags.iter().find(|c| c.val == 0).unwrap().ev, 0);
                    let set: u16 = flags.iter().find(|c| c.val == 1).unwrap().ev;
                    let Kind::Action { next, .. } = &f.events[usize::from(set)].kind else { panic!("flag write") };
                    assert_eq!(matches!(f.events[usize::from(*next)].kind, Kind::Switch { .. }), progressive);
                }
            }
        }
    }

    #[test]
    fn once_refuses_a_moved_entry_a_wrong_name_and_a_second_run() {
        let mut b: Bf = sample();
        apply(&mut b, &once_op(false)).unwrap();
        let after: Bf = b.clone();
        assert!(apply(&mut b, &once_op(false)).is_err());
        assert_eq!(b, after);
        let mut o: Op = once_op(false);
        o.event = "Event9".to_string();
        assert!(apply(&mut sample(), &o).is_err());
        let mut o: Op = once_op(false);
        o.index = 1;
        o.event = "Event1".to_string();
        assert!(apply(&mut sample(), &o).is_err());
        let mut o: Op = once_op(false);
        if let How::Once(w) = &mut o.how {
            w.entry = "Missing".to_string();
        }
        assert!(apply(&mut sample(), &o).is_err());
    }
}
