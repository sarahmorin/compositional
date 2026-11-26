/// Visual style overrides for a node or class.
///
/// Every field is optional; `None` means "use the renderer's default". Colors are passed straight
/// through to the backend, so for Graphviz any of its color names (`"lightblue"`) or hex strings
/// (`"#fde68a"`, `"#fde68a80"` with alpha) work.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Style {
    /// Background color.
    pub fill: Option<String>,
    /// Border color.
    pub border: Option<String>,
    /// Label text color.
    pub font: Option<String>,
    /// Border width in points.
    pub pen_width: Option<f32>,
    /// Hover text (shown by SVG viewers).
    pub tooltip: Option<String>,
    /// Names added to the element's `class` attribute in SVG output, so a stylesheet (like the
    /// replay player's) can restyle or toggle it.
    pub classes: Vec<String>,
}

impl Style {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn fill(mut self, color: impl Into<String>) -> Self {
        self.fill = Some(color.into());
        self
    }

    pub fn border(mut self, color: impl Into<String>) -> Self {
        self.border = Some(color.into());
        self
    }

    pub fn font(mut self, color: impl Into<String>) -> Self {
        self.font = Some(color.into());
        self
    }

    pub fn pen_width(mut self, width: f32) -> Self {
        self.pen_width = Some(width);
        self
    }

    pub fn tooltip(mut self, text: impl Into<String>) -> Self {
        self.tooltip = Some(text.into());
        self
    }

    pub fn class(mut self, name: impl Into<String>) -> Self {
        self.classes.push(name.into());
        self
    }

    /// This style with `top` layered over it: fields set in `top` win, classes accumulate.
    pub fn over(mut self, top: &Style) -> Self {
        fn pick<T: Clone>(base: &mut Option<T>, top: &Option<T>) {
            if top.is_some() {
                *base = top.clone();
            }
        }
        pick(&mut self.fill, &top.fill);
        pick(&mut self.border, &top.border);
        pick(&mut self.font, &top.font);
        pick(&mut self.pen_width, &top.pen_width);
        pick(&mut self.tooltip, &top.tooltip);
        self.classes.extend(top.classes.iter().cloned());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn over_prefers_top_and_merges_classes() {
        let base = Style::new().fill("white").border("black").class("a");
        let top = Style::new().border("red").pen_width(2.0).class("b");
        let merged = base.over(&top);
        assert_eq!(merged.fill.as_deref(), Some("white"));
        assert_eq!(merged.border.as_deref(), Some("red"));
        assert_eq!(merged.pen_width, Some(2.0));
        assert_eq!(merged.classes, vec!["a", "b"]);
    }
}
