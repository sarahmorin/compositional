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
}
