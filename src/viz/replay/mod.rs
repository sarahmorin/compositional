/// Replaying a recorded e-graph execution as a sequence of frames.
///
/// ```ignore
/// egraph.start_recording();
/// // ... build and explore, calling egraph.focus_class(..) / egraph.note(..) along the way ...
/// let replay = Replay::new(egraph.take_recording().unwrap());
/// let view = ReplayView::new().display_labels();
///
/// // Interactive, self-contained player (opens offline in any browser):
/// replay.write_html("replay.html", &Step::Event, &view, &PlayerOptions::default())?;
///
/// // Or the frames themselves:
/// for frame in replay.frames(&Step::Event, &view) {
///     println!("{}: {}", frame.events.start, frame.caption);
///     let dot = frame.snapshot.to_dot(&DotOptions::default());
/// }
/// ```
///
/// The player supports play/pause, stepping by event or by visit, restart, a scrubber,
/// 0.25x-4x speed, pan/zoom, toggles for the trail and heat layers, keyboard shortcuts, and
/// a URL fragment (`replay.html#f=120&trail&heat`) that opens a specific frame.
///
/// Each frame's snapshot carries three style layers, later ones winning:
/// 1. the view's own styles;
/// 2. change highlights for the frame's events (`added` nodes, `merged` classes);
/// 3. the driver's current focus (`focus`).
///
/// It also carries CSS classes for optional layers that a viewer can toggle: `trail-1` (most
/// recent) to `trail-5` on the last few targets visited before the current focus, and `heat-1`
/// to `heat-5` on every visited target, by visit count relative to the most visited.
mod model;
mod player;

pub use model::{ModelClass, ModelNode, ReplayModel};
pub use player::{PlayerOptions, player_html};

use crate::egraph::core::{ClassId, NodeId};
use crate::egraph::trace::{Event, Target, Trace};
use crate::viz::snapshot::Snapshot;
use crate::viz::style::Style;
use std::collections::HashMap;
use std::fmt::{Debug, Display};
use std::ops::Range;

/// Number of previously visited targets marked with `trail-*` classes.
pub const TRAIL_LEN: usize = 5;
/// Number of `heat-*` buckets.
pub const HEAT_LEVELS: usize = 5;

/// How events are grouped into frames.
pub enum Step<'a, L, D> {
    /// One frame per event. Analysis-data updates and the end of a rebuild join the event
    /// before them, and the start of a rebuild joins the event after it.
    Event,
    /// One frame per focus: each frame starts at a `Focus` event and shows everything that
    /// happened until the next one.
    Focus,
    /// A frame ends after every event the predicate accepts.
    Custom(Box<dyn Fn(&Event<L, D>) -> bool + 'a>),
}

/// Labels and base styles for replay frames; the replay counterpart of `Capture`.
///
/// Callbacks see the operator and the analysis data of the node's class (`None` until the
/// trace has logged some).
pub struct ReplayView<'a, L, D> {
    node_label: Box<dyn Fn(NodeId, &L) -> String + 'a>,
    node_style: Box<dyn Fn(NodeId, &L, Option<&D>) -> Style + 'a>,
    class_label: Box<dyn Fn(ClassId, Option<&D>) -> Option<String> + 'a>,
    class_style: Box<dyn Fn(ClassId, Option<&D>) -> Style + 'a>,
}

impl<'a, L: Debug, D> ReplayView<'a, L, D> {
    pub fn new() -> Self {
        Self {
            node_label: Box::new(|_, op| format!("{op:?}")),
            node_style: Box::new(|_, _, _| Style::default()),
            class_label: Box::new(|_, _| None),
            class_style: Box::new(|_, _| Style::default()),
        }
    }
}

impl<'a, L: Debug, D> Default for ReplayView<'a, L, D> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a, L, D> ReplayView<'a, L, D> {
    /// Labels nodes with the operator's `Display` (e.g. `+` instead of `Add`).
    pub fn display_labels(self) -> Self
    where
        L: Display,
    {
        self.node_label(|_, op| op.to_string())
    }

    pub fn node_label(mut self, f: impl Fn(NodeId, &L) -> String + 'a) -> Self {
        self.node_label = Box::new(f);
        self
    }

    pub fn node_style(mut self, f: impl Fn(NodeId, &L, Option<&D>) -> Style + 'a) -> Self {
        self.node_style = Box::new(f);
        self
    }

    pub fn class_label(mut self, f: impl Fn(ClassId, Option<&D>) -> Option<String> + 'a) -> Self {
        self.class_label = Box::new(f);
        self
    }

    pub fn class_style(mut self, f: impl Fn(ClassId, Option<&D>) -> Style + 'a) -> Self {
        self.class_style = Box::new(f);
        self
    }
}

/// One step of a replay.
#[derive(Clone, Debug)]
pub struct Frame {
    /// The events this frame applies; the snapshot shows the state after them.
    pub events: Range<usize>,
    /// Short description of the frame's events.
    pub caption: String,
    /// The driver's focus after this frame, canonicalized.
    pub focus: Option<Target>,
    pub stats: FrameStats,
    pub snapshot: Snapshot,
}

/// Counts of the events in a frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FrameStats {
    pub added: usize,
    pub merged: usize,
    pub dropped: usize,
    /// `Focus` events that point at something (clearing the focus is not a visit).
    pub visits: usize,
}

/// A recorded trace, ready to be stepped through.
pub struct Replay<L, D> {
    events: Vec<Event<L, D>>,
}

impl<L: Clone, D: Clone> Replay<L, D> {
    pub fn new(trace: Trace<L, D>) -> Self {
        Self {
            events: trace.events,
        }
    }

    pub fn events(&self) -> &[Event<L, D>] {
        &self.events
    }

    /// The model after applying the first `n` events.
    pub fn model_at(&self, n: usize) -> ReplayModel<L, D> {
        let mut model = ReplayModel::new();
        for event in &self.events[..n] {
            model.apply(event);
        }
        model
    }

    /// Event ranges of the frames for `step`. The first frame is always the empty range (the
    /// state before any event); the ranges are contiguous and cover every event.
    pub fn frame_ranges(&self, step: &Step<L, D>) -> Vec<Range<usize>> {
        let events = &self.events;
        let n = events.len();
        let mut ranges = vec![0..0];
        let mut start = 0;
        let mut i = 0;
        while i < n {
            let boundary = match step {
                Step::Event => !matches!(
                    events[i],
                    Event::SetData { .. } | Event::RebuildStart | Event::RebuildEnd
                ),
                Step::Focus => matches!(events.get(i + 1), Some(Event::Focus(_))),
                Step::Custom(accept) => accept(&events[i]),
            };
            i += 1;
            if !boundary {
                continue;
            }
            // Trailing bookkeeping belongs with the event that caused it.
            while i < n && matches!(events[i], Event::SetData { .. } | Event::RebuildEnd) {
                i += 1;
            }
            ranges.push(start..i);
            start = i;
        }
        if start < n {
            ranges.push(start..n);
        }
        ranges
    }

    /// All frames for `step`, rendered with `view`.
    pub fn frames(&self, step: &Step<L, D>, view: &ReplayView<L, D>) -> Vec<Frame> {
        let mut model = ReplayModel::new();
        let mut visits = Visits::default();
        self.frame_ranges(step)
            .into_iter()
            .map(|range| {
                let events = &self.events[range.clone()];
                let mut changes = Changes::default();
                for (offset, event) in events.iter().enumerate() {
                    model.apply(event);
                    changes.record(event);
                    if let Event::Focus(Some(target)) = event {
                        visits.record(*target, range.start + offset);
                    }
                }
                let mut snapshot = model.snapshot(view);
                decorate(&mut snapshot, &model, &changes, &visits);
                Frame {
                    caption: caption(events, view),
                    focus: model.focus(),
                    stats: changes.stats,
                    events: range,
                    snapshot,
                }
            })
            .collect()
    }
}

// ------------------------------------------------------------------ highlights

fn added_style() -> Style {
    Style::new().border("#16a34a").pen_width(2.5).class("added")
}

fn merged_style() -> Style {
    Style::new().border("#ea580c").pen_width(2.4).class("merged")
}

fn focus_node_style() -> Style {
    Style::new()
        .fill("#fecaca")
        .border("#dc2626")
        .pen_width(2.5)
        .class("focus")
}

fn focus_class_style() -> Style {
    Style::new()
        .fill("#fee2e2")
        .border("#dc2626")
        .pen_width(2.4)
        .class("focus")
}

/// What a frame's events changed.
#[derive(Default)]
struct Changes {
    added: Vec<NodeId>,
    merged: Vec<ClassId>,
    stats: FrameStats,
}

impl Changes {
    fn record<L, D>(&mut self, event: &Event<L, D>) {
        match event {
            Event::AddNode { node, .. } => {
                self.added.push(*node);
                self.stats.added += 1;
            }
            Event::Union { leader, .. } => {
                self.merged.push(*leader);
                self.stats.merged += 1;
            }
            Event::DropNode { .. } => self.stats.dropped += 1,
            Event::Focus(Some(_)) => self.stats.visits += 1,
            _ => {}
        }
    }
}

/// Every focus so far: last event index and count, by target as logged.
#[derive(Default)]
struct Visits(HashMap<Target, (usize, usize)>);

impl Visits {
    fn record(&mut self, target: Target, index: usize) {
        let entry = self.0.entry(target).or_insert((index, 0));
        entry.0 = index;
        entry.1 += 1;
    }

    /// Visits merged under canonical targets.
    fn canonical<L: Clone, D: Clone>(
        &self,
        model: &ReplayModel<L, D>,
    ) -> HashMap<Target, (usize, usize)> {
        let mut merged: HashMap<Target, (usize, usize)> = HashMap::new();
        for (&target, &(last, count)) in &self.0 {
            let entry = merged.entry(model.canonical(target)).or_insert((last, 0));
            entry.0 = entry.0.max(last);
            entry.1 += count;
        }
        merged
    }
}

/// Layers change, focus, trail and heat styles over the view's styles.
fn decorate<L: Clone, D: Clone>(
    snapshot: &mut Snapshot,
    model: &ReplayModel<L, D>,
    changes: &Changes,
    visits: &Visits,
) {
    let mut layers: HashMap<Target, Style> = HashMap::new();
    let mut add = |target: Target, style: Style| {
        let slot = layers.entry(target).or_default();
        *slot = std::mem::take(slot).over(&style);
    };

    for &node in &changes.added {
        // Skip nodes a later event in the same frame dropped.
        if model.find_node(node) == node && model.node(node).is_some() {
            add(Target::Node(node), added_style());
        }
    }
    for &class in &changes.merged {
        add(Target::Class(model.find(class)), merged_style());
    }
    let focus = model.focus();
    match focus {
        Some(target @ Target::Node(_)) => add(target, focus_node_style()),
        Some(target @ Target::Class(_)) => add(target, focus_class_style()),
        None => {}
    }

    let visited = visits.canonical(model);
    let max_count = visited.values().map(|&(_, count)| count).max().unwrap_or(1);
    let mut recent: Vec<_> = visited
        .iter()
        .filter(|(target, _)| Some(**target) != focus)
        .map(|(&target, &(last, _))| (last, target))
        .collect();
    recent.sort_unstable_by(|a, b| b.0.cmp(&a.0));
    for (rank, (_, target)) in recent.into_iter().take(TRAIL_LEN).enumerate() {
        add(target, Style::new().class(format!("trail-{}", rank + 1)));
    }
    for (&target, &(_, count)) in &visited {
        let level = (count * HEAT_LEVELS).div_ceil(max_count).clamp(1, HEAT_LEVELS);
        add(target, Style::new().class(format!("heat-{level}")));
    }

    for class in &mut snapshot.classes {
        if let Some(top) = layers.get(&Target::Class(class.id)) {
            class.style = std::mem::take(&mut class.style).over(top);
        }
        for node in &mut class.nodes {
            if let Some(top) = layers.get(&Target::Node(node.id)) {
                node.style = std::mem::take(&mut node.style).over(top);
            }
        }
    }
}

// -------------------------------------------------------------------- captions

/// One line describing an event, or `None` for bookkeeping that isn't worth a mention.
pub fn describe<L, D>(event: &Event<L, D>, view: &ReplayView<L, D>) -> Option<String> {
    Some(match event {
        Event::AddNode {
            node,
            class,
            op,
            children,
        } => {
            let label = (view.node_label)(*node, op);
            if children.is_empty() {
                format!("add n{node} {label} in new class c{class}")
            } else {
                let args: Vec<_> = children.iter().map(|c| format!("c{c}")).collect();
                format!("add n{node} {label}({}) in new class c{class}", args.join(", "))
            }
        }
        Event::Union { leader, loser } => format!("merge c{loser} into c{leader}"),
        Event::DropNode { node, kept } => format!("drop n{node}, congruent to n{kept}"),
        Event::SetData { .. } => return None,
        Event::RebuildStart => "rebuild starts".into(),
        Event::RebuildEnd => "rebuild ends".into(),
        Event::Focus(Some(Target::Class(class))) => format!("visit c{class}"),
        Event::Focus(Some(Target::Node(node))) => format!("visit n{node}"),
        Event::Focus(None) => "clear focus".into(),
        Event::Note(text) => text.clone(),
    })
}

/// Lists up to three events; summarizes longer frames as counts plus their focus and notes.
fn caption<L, D>(events: &[Event<L, D>], view: &ReplayView<L, D>) -> String {
    if events.is_empty() {
        return "initial state".into();
    }
    let lines: Vec<String> = events.iter().filter_map(|e| describe(e, view)).collect();
    if lines.is_empty() {
        return "analysis data updated".into();
    }
    if lines.len() <= 3 {
        return lines.join("; ");
    }

    let last = |f: fn(&Event<L, D>) -> bool| events.iter().rev().find(|e| f(e));
    let mut parts: Vec<String> = [
        last(|e| matches!(e, Event::Note(_))),
        last(|e| matches!(e, Event::Focus(_))),
    ]
    .into_iter()
    .flatten()
    .filter_map(|e| describe(e, view))
    .collect();
    let count = |f: fn(&Event<L, D>) -> bool| events.iter().filter(|e| f(e)).count();
    for (n, what) in [
        (count(|e| matches!(e, Event::AddNode { .. })), "added"),
        (count(|e| matches!(e, Event::Union { .. })), "merged"),
        (count(|e| matches!(e, Event::DropNode { .. })), "dropped"),
    ] {
        if n > 0 {
            parts.push(format!("{n} {what}"));
        }
    }
    parts.join(" · ")
}

#[cfg(test)]
mod tests;
