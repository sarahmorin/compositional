use super::*;
use crate::egraph::analysis::Analysis;
use crate::egraph::core::{EGraph, Node};
use crate::egraph::language::Term;
use crate::viz::snapshot::Capture;
use crate::viz::test_lang::{Math, term};

/// Smallest term size in each class, so traces carry analysis data.
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

type EG = EGraph<Math, Size>;

fn recording() -> EG {
    let mut eg = EGraph::new(Size);
    eg.start_recording();
    eg
}

fn trace_len(eg: &EG) -> usize {
    eg.recording().unwrap().len()
}

/// Deterministic pseudo-random numbers (no dependency needed).
struct Lcg(u64);

impl Lcg {
    fn next(&mut self, bound: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.0 >> 33) as usize % bound
    }
}

fn random_term(rng: &mut Lcg, depth: usize) -> Term<Math> {
    if depth == 0 || rng.next(3) == 0 {
        let leaves = ["a", "b", "c", "0", "1"];
        return term(leaves[rng.next(leaves.len())]);
    }
    match rng.next(3) {
        0 => Term::new(
            Math::Add,
            vec![random_term(rng, depth - 1), random_term(rng, depth - 1)],
        ),
        1 => Term::new(
            Math::Mul,
            vec![random_term(rng, depth - 1), random_term(rng, depth - 1)],
        ),
        _ => Term::new(Math::Neg, vec![random_term(rng, depth - 1)]),
    }
}

/// Replaying the trace must reproduce exactly what `Capture` sees on the live e-graph, at
/// every point, including between a union and the rebuild that restores congruence.
#[test]
fn replay_matches_live_egraph_at_every_step() {
    let (mut drops, mut unions) = (0, 0);
    for seed in 0..20 {
        let mut rng = Lcg(seed);
        let mut eg = recording();
        let mut classes = Vec::new();
        let mut checkpoints = Vec::new();
        for _ in 0..60 {
            match rng.next(10) {
                0..=5 => classes.push(eg.add_term(&random_term(&mut rng, 3)).0.class()),
                6..=8 if classes.len() >= 2 => {
                    let a = classes[rng.next(classes.len())];
                    let b = classes[rng.next(classes.len())];
                    eg.union(a, b);
                }
                _ => eg.rebuild(),
            }
            let live = Capture::new()
                .class_label(|c: &crate::egraph::core::EClass<usize>| Some(c.data.to_string()))
                .capture(&eg);
            checkpoints.push((trace_len(&eg), live));
        }

        let replay = Replay::new(eg.take_recording().unwrap());
        for event in replay.events() {
            drops += usize::from(matches!(event, Event::DropNode { .. }));
            unions += usize::from(matches!(event, Event::Union { .. }));
        }
        let view = ReplayView::new().class_label(|_, data: Option<&usize>| data.map(|d| d.to_string()));
        let mut model = ReplayModel::new();
        let mut applied = 0;
        for (step, (n, live)) in checkpoints.iter().enumerate() {
            for event in &replay.events()[applied..*n] {
                model.apply(event);
            }
            applied = *n;
            assert_eq!(
                &model.snapshot(&view),
                live,
                "seed {seed}, step {step}: replay diverged from the live e-graph"
            );
        }
    }
    // Make sure the random runs exercised congruence, not just additions.
    assert!(drops > 0 && unions > 0, "drops {drops}, unions {unions}");
    eprintln!("cross-checked {drops} drops and {unions} unions");
}

#[test]
fn event_frames_attach_bookkeeping_to_their_cause() {
    let mut eg = recording();
    let a = eg.add_term(&term("a")).0.class();
    let b = eg.add_term(&term("b")).0.class();
    eg.add_term(&term("neg(a)"));
    eg.add_term(&term("neg(b)"));
    eg.union(a, b);
    eg.rebuild();
    let replay = Replay::new(eg.take_recording().unwrap());
    let ranges = replay.frame_ranges(&Step::Event);

    assert_eq!(ranges[0], 0..0);
    assert_eq!(ranges.last().unwrap().end, replay.events().len());
    assert!(ranges.windows(2).all(|w| w[0].end == w[1].start));
    for range in &ranges[1..] {
        let events = &replay.events()[range.clone()];
        let primary = events
            .iter()
            .filter(|e| !matches!(e, Event::SetData { .. } | Event::RebuildStart | Event::RebuildEnd))
            .count();
        assert_eq!(primary, 1, "{events:?}");
        assert!(!matches!(events[0], Event::SetData { .. } | Event::RebuildEnd));
    }
}

#[test]
fn focus_frames_start_at_each_focus() {
    let mut eg = recording();
    let x = eg.add_term(&term("+(a, b)")).0.class();
    eg.focus_class(x);
    eg.add_term(&term("+(b, a)"));
    eg.focus_class(x);
    eg.note(|| "done".into());
    let replay = Replay::new(eg.take_recording().unwrap());
    let ranges = replay.frame_ranges(&Step::Focus);
    // initial, build before first focus, each focus up to the next
    assert_eq!(ranges.len(), 4);
    for range in &ranges[2..] {
        assert!(matches!(replay.events()[range.start], Event::Focus(_)));
    }
}

#[test]
fn frames_highlight_additions_merges_and_focus() {
    let mut eg = recording();
    let a = eg.add_term(&term("a")).0;
    let b = eg.add_term(&term("b")).0;
    eg.union(a.class(), b.class());
    eg.focus_node(b.node());
    let replay = Replay::new(eg.take_recording().unwrap());
    let view = ReplayView::new().display_labels();
    let frames = replay.frames(&Step::Event, &view);
    assert_eq!(frames.len(), 5); // initial, add a, add b, union, focus

    let node = |f: &Frame, id| f.snapshot.nodes().find(|(_, n)| n.id == id).unwrap().1.clone();
    assert!(node(&frames[1], a.node()).style.classes.contains(&"added".into()));
    // The highlight only lasts for the frame that made the change.
    assert!(!node(&frames[2], a.node()).style.classes.contains(&"added".into()));

    let leader = eg.find(a.class());
    let merged = frames[3].snapshot.class(leader).unwrap();
    assert!(merged.style.classes.contains(&"merged".into()));
    assert_eq!(frames[3].caption, format!("merge c{} into c{leader}", if leader == a.class() { b.class() } else { a.class() }));

    let focused = node(&frames[4], b.node());
    assert!(focused.style.classes.contains(&"focus".into()));
    assert_eq!(focused.style.fill.as_deref(), Some("#fecaca"));
    assert_eq!(frames[4].focus, Some(Target::Node(b.node())));
}

#[test]
fn trail_ranks_recent_visits_and_heat_counts_them() {
    let mut eg = recording();
    let a = eg.add_term(&term("a")).0.class();
    let b = eg.add_term(&term("b")).0.class();
    let c = eg.add_term(&term("c")).0.class();
    for class in [a, a, a, b, c] {
        eg.focus_class(class);
    }
    let replay = Replay::new(eg.take_recording().unwrap());
    let frames = replay.frames(&Step::Event, &ReplayView::new());
    let last = frames.last().unwrap();
    let classes = |id| last.snapshot.class(id).unwrap().style.classes.clone();

    assert!(classes(c).contains(&"focus".into())); // current focus: no trail
    assert!(!classes(c).iter().any(|k| k.starts_with("trail")));
    assert!(classes(b).contains(&"trail-1".into()));
    assert!(classes(a).contains(&"trail-2".into()));
    assert!(classes(a).contains(&"heat-5".into())); // 3 of max 3 visits
    assert!(classes(b).contains(&"heat-2".into())); // 1 of 3, rounded up
}

#[test]
fn visits_follow_merged_classes() {
    let mut eg = recording();
    let a = eg.add_term(&term("a")).0.class();
    let b = eg.add_term(&term("b")).0.class();
    eg.focus_class(a);
    eg.focus_class(b);
    eg.focus(None);
    eg.union(a, b);
    let replay = Replay::new(eg.take_recording().unwrap());
    let frames = replay.frames(&Step::Event, &ReplayView::new());
    let merged = frames.last().unwrap().snapshot.class(eg.find(a)).unwrap();
    // Two visits under one canonical class: it is the most visited and most recent.
    assert!(merged.style.classes.contains(&"heat-5".into()));
    assert!(merged.style.classes.contains(&"trail-1".into()));
}

#[test]
fn model_at_matches_frames() {
    let mut eg = recording();
    eg.add_term(&term("*(+(a, b), c)"));
    let replay = Replay::new(eg.take_recording().unwrap());
    let view = ReplayView::new();
    for frame in replay.frames(&Step::Event, &view) {
        let plain = replay.model_at(frame.events.end).snapshot(&view);
        assert_eq!(plain.classes.len(), frame.snapshot.classes.len());
    }
}
