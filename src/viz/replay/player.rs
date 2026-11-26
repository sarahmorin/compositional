/// Self-contained HTML player for replay frames.
///
/// The page inlines the vendored d3 and d3-graphviz builds (see `vendor/README.md`), so it opens
/// offline from `file://`. Rust does all the interpretation: each frame is embedded as finished
/// DOT text, and the page only lays it out, animates between frames and drives playback.
use crate::viz::dot::DotOptions;
use crate::viz::replay::{Frame, Replay, ReplayView, Step};
use std::fmt::Write;
use std::io;
use std::path::Path;

const TEMPLATE: &str = include_str!("player.html");
const D3: &str = include_str!("vendor/d3.min.js");
const D3_GRAPHVIZ: &str = include_str!("vendor/d3-graphviz.min.js");

/// Markers in `player.html`, each replaced wherever it appears.
const PLACEHOLDERS: [&str; 4] = ["__TITLE__", "__DATA__", "/*__D3__*/", "/*__D3_GRAPHVIZ__*/"];

#[derive(Clone, Debug)]
pub struct PlayerOptions {
    /// Shown in the page header and the browser tab.
    pub title: String,
    pub dot: DotOptions,
}

impl Default for PlayerOptions {
    fn default() -> Self {
        Self {
            title: "E-graph replay".into(),
            dot: DotOptions::default(),
        }
    }
}

impl<L: Clone, D: Clone> Replay<L, D> {
    /// The player page for this replay's frames under `step`.
    pub fn player_html(&self, step: &Step<L, D>, view: &ReplayView<L, D>, opts: &PlayerOptions) -> String {
        player_html(&self.frames(step, view), self.events().len(), opts)
    }

    /// Writes the player page to `path`.
    pub fn write_html(
        &self,
        path: impl AsRef<Path>,
        step: &Step<L, D>,
        view: &ReplayView<L, D>,
        opts: &PlayerOptions,
    ) -> io::Result<()> {
        std::fs::write(path, self.player_html(step, view, opts))
    }
}

/// The player page for any frames. `total_events` is shown in the frame counter.
pub fn player_html(frames: &[Frame], total_events: usize, opts: &PlayerOptions) -> String {
    let data = frames_json(frames, total_events, &opts.dot);
    let title = escape_html(&opts.title);
    fill(TEMPLATE, |marker| match marker {
        "__TITLE__" => &title,
        "__DATA__" => &data,
        "/*__D3__*/" => D3,
        "/*__D3_GRAPHVIZ__*/" => D3_GRAPHVIZ,
        _ => unreachable!(),
    })
}

/// Replaces every placeholder in `template` in one pass, so inserted text is never rescanned.
fn fill<'a>(template: &str, value: impl Fn(&str) -> &'a str) -> String {
    let mut out = String::with_capacity(template.len() + D3.len() + D3_GRAPHVIZ.len());
    let mut rest = template;
    while let Some((at, marker)) = PLACEHOLDERS
        .iter()
        .filter_map(|m| rest.find(m).map(|at| (at, *m)))
        .min()
    {
        out.push_str(&rest[..at]);
        out.push_str(value(marker));
        rest = &rest[at + marker.len()..];
    }
    out.push_str(rest);
    out
}

fn frames_json(frames: &[Frame], total_events: usize, dot: &DotOptions) -> String {
    let mut out = String::new();
    write!(out, "{{\"total_events\":{total_events},\"frames\":[").unwrap();
    for (i, frame) in frames.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let s = &frame.stats;
        write!(
            out,
            "{{\"events\":[{},{}],\"caption\":{},\"added\":{},\"merged\":{},\"dropped\":{},\"visits\":{},\"dot\":{}}}",
            frame.events.start,
            frame.events.end,
            json_string(&frame.caption),
            s.added,
            s.merged,
            s.dropped,
            s.visits,
            json_string(&frame.snapshot.to_dot(dot)),
        )
        .unwrap();
    }
    out.push_str("]}");
    out
}

/// A JSON string literal that is also safe inside an HTML `<script>` element.
fn json_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            // `<` could start `</script>`; U+2028/9 are line terminators in older JS.
            '<' | '\u{2028}' | '\u{2029}' => write!(out, "\\u{:04x}", c as u32).unwrap(),
            c if (c as u32) < 0x20 => write!(out, "\\u{:04x}", c as u32).unwrap(),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_strings_cannot_close_the_script_tag() {
        let s = json_string("a\"b\\c\n</script><!--\u{1}\u{2028}");
        // Built with `u` so no escape sequence has to survive in this source file.
        let u = |hex: &str| format!("{}u{hex}", '\\');
        let expected = format!(
            r#""a\"b\\c\n{lt}/script>{lt}!--{one}{ls}""#,
            lt = u("003c"),
            one = u("0001"),
            ls = u("2028")
        );
        assert_eq!(s, expected);
        assert!(!s.contains("</"));
    }

    #[test]
    fn fill_replaces_markers_without_rescanning() {
        let page = fill("<t>__TITLE__</t>__DATA__|__TITLE__", |m| match m {
            "__TITLE__" => "x",
            "__DATA__" => "__TITLE__", // inserted text must stay as is
            _ => "",
        });
        assert_eq!(page, "<t>x</t>__TITLE__|x");
    }

    #[test]
    fn template_has_every_placeholder() {
        for marker in PLACEHOLDERS {
            assert!(TEMPLATE.contains(marker), "{marker} missing from player.html");
        }
        // The data marker must appear exactly once: the frames can be large.
        assert_eq!(TEMPLATE.matches("__DATA__").count(), 1);
    }

    #[test]
    fn vendored_libraries_cannot_close_their_script_tags() {
        assert!(!D3.contains("</script"));
        assert!(!D3_GRAPHVIZ.contains("</script"));
    }
}
