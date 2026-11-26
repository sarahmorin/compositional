use compositional::define_language;
use compositional::egraph::analysis::Analysis;
use compositional::egraph::core::{EGraph, Node, NodeId};
use compositional::egraph::language::Term;
use compositional::egraph::parse::parse_term;
use compositional::egraph::trace::{Event, Target};

define_language! {
    pub enum Math {
        "+" = Add(2),
        "f" = F(1),
        Num(i64),
        Sym(String),
    }
}

/// Smallest term size in each class.
#[derive(Clone, Debug)]
struct Size;

impl Analysis<Math> for Size {
    type Data = usize;
    fn make(egraph: &mut EGraph<Math, Self>, enode: &Node<Math>, _: NodeId) -> usize {
        1 + enode
            .children
            .iter()
            .map(|&c| egraph.class(c).data)
            .sum::<usize>()
    }
    fn merge(&mut self, a: &mut usize, b: usize) -> (bool, bool) {
        let changed = (b < *a, *a < b);
        *a = (*a).min(b);
        changed
    }
}

type Ev = Event<Math, usize>;

fn term(s: &str) -> Term<Math> {
    parse_term(s).unwrap()
}

fn recording() -> EGraph<Math, Size> {
    let mut eg = EGraph::new(Size);
    eg.start_recording();
    eg
}

#[test]
fn nothing_is_logged_unless_recording() {
    let mut eg = EGraph::new(Size);
    eg.add_term(&term("+(x, y)"));
    eg.note(|| panic!("note closure must not run while not recording"));
    eg.focus_class(0);
    assert!(!eg.is_recording());
    assert!(eg.take_recording().is_none());
}

#[test]
fn new_nodes_log_add_and_data_existing_nodes_log_nothing() {
    let mut eg = recording();
    let (x, _) = eg.add_term(&term("x"));
    let (fx, _) = eg.add_term(&term("f(x)"));
    eg.add_term(&term("f(x)")); // hashconsed: no events
    let events = eg.take_recording().unwrap().events;
    assert_eq!(
        events,
        vec![
            Ev::AddNode { node: x.node(), class: x.class(), op: Math::Sym("x".into()), children: vec![] },
            Ev::SetData { class: x.class(), data: 1 },
            Ev::AddNode { node: fx.node(), class: fx.class(), op: Math::F, children: vec![x.class()] },
            Ev::SetData { class: fx.class(), data: 2 },
        ]
    );
}

#[test]
fn union_logs_the_actual_leader() {
    let mut eg = recording();
    let x = eg.add_term(&term("x")).0.class();
    let fx = eg.add_term(&term("f(x)")).0.class();
    eg.take_recording();
    eg.start_recording();

    assert!(eg.union(fx, x));
    assert!(!eg.union(fx, x)); // already merged: no event
    let leader = eg.find(x);
    let loser = if leader == x { fx } else { x };
    let events = eg.take_recording().unwrap().events;
    assert_eq!(events[0], Ev::Union { leader, loser });
    // Merging size 1 into size 2 (or vice versa) changes the leader's data only if it was 2.
    let data_events = events.iter().filter(|e| matches!(e, Ev::SetData { .. })).count();
    assert_eq!(data_events, usize::from(leader == fx));
    assert_eq!(events.len(), 1 + data_events);
}

#[test]
fn congruence_during_rebuild_logs_drop_then_union() {
    let mut eg = recording();
    let a = eg.add_term(&term("a")).0.class();
    let b = eg.add_term(&term("b")).0.class();
    let fa = eg.add_term(&term("f(a)")).0;
    let fb = eg.add_term(&term("f(b)")).0;
    eg.take_recording();
    eg.start_recording();

    eg.union(a, b);
    eg.rebuild();
    eg.rebuild(); // nothing pending: no markers
    let events = eg.take_recording().unwrap().events;

    let start = events.iter().position(|e| *e == Ev::RebuildStart).unwrap();
    assert_eq!(events.last(), Some(&Ev::RebuildEnd));
    assert_eq!(events.iter().filter(|e| **e == Ev::RebuildStart).count(), 1);
    let drop = events
        .iter()
        .position(|e| matches!(e, Ev::DropNode { .. }))
        .unwrap();
    assert!(start < drop);
    let Ev::DropNode { node, kept } = events[drop] else { unreachable!() };
    let mut pair = [node, kept];
    pair.sort();
    assert_eq!(pair, [fa.node(), fb.node()]);
    assert!(matches!(events[drop + 1], Ev::Union { .. }));
    assert_eq!(eg.find(fa.class()), eg.find(fb.class()));
}

#[test]
fn annotations_are_interleaved_in_order() {
    let mut eg = recording();
    eg.note(|| "start".into());
    let x = eg.add_term(&term("x")).0;
    eg.focus_class(x.class());
    eg.focus_node(x.node());
    eg.focus(None);
    let events = eg.take_recording().unwrap().events;
    assert_eq!(events[0], Ev::Note("start".into()));
    assert!(matches!(events[1], Ev::AddNode { .. }));
    assert_eq!(
        events[3..],
        [
            Ev::Focus(Some(Target::Class(x.class()))),
            Ev::Focus(Some(Target::Node(x.node()))),
            Ev::Focus(None),
        ]
    );
}
