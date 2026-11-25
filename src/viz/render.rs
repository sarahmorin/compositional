/// Rendering DOT text to images with the Graphviz `dot` executable.
use std::io::{self, Write};
use std::path::Path;
use std::process::{Command, Stdio};

/// Environment variable that overrides the path of the `dot` executable.
pub const DOT_BIN_ENV: &str = "GRAPHVIZ_DOT";

/// Runs `dot -T{format}` on `dot` and returns the output bytes. `format` is any Graphviz output
/// format (`svg`, `png`, `pdf`, ...).
pub fn render(dot: &str, format: &str) -> io::Result<Vec<u8>> {
    let bin = std::env::var(DOT_BIN_ENV).unwrap_or_else(|_| "dot".into());
    let mut child = Command::new(&bin)
        .arg(format!("-T{format}"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            if e.kind() == io::ErrorKind::NotFound {
                io::Error::new(
                    e.kind(),
                    format!("could not run `{bin}`: install Graphviz or set {DOT_BIN_ENV}"),
                )
            } else {
                e
            }
        })?;
    // `dot` reads all of its input before writing anything, so this can't deadlock.
    child.stdin.take().unwrap().write_all(dot.as_bytes())?;
    let output = child.wait_with_output()?;

    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() {
        return Err(io::Error::other(format!("`{bin}` failed: {stderr}")));
    }
    if !stderr.trim().is_empty() {
        log::warn!("graphviz: {}", stderr.trim());
    }
    Ok(output.stdout)
}

/// Renders DOT text to an SVG document.
pub fn svg(dot: &str) -> io::Result<String> {
    String::from_utf8(render(dot, "svg")?).map_err(io::Error::other)
}

/// Renders DOT text to an SVG file.
pub fn write_svg(dot: &str, path: impl AsRef<Path>) -> io::Result<()> {
    std::fs::write(path, render(dot, "svg")?)
}
