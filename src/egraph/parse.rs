/// Parsing and printing of terms and patterns.
///
/// Grammar:
///
/// ```text
/// expr := <op> | <op>(<expr>, <expr>, ...)
/// ```
///
/// An `<op>` is any run of characters other than whitespace, `(`, `)` and `,`, so
/// `+(x, -3)` and `neg(?a)` are both valid. In patterns, an op starting with `?` is a
/// variable (`?a` is the variable named `a`) and cannot have children.
///
/// Parsing happens in two steps: the input is first read into an untyped syntax tree, which
/// is then converted into a `Term` or `Pattern` by looking up each op with `FromOp` and
/// checking its arity.
use crate::egraph::language::{Language, OpOrVar, Pattern, Term};
use std::fmt::{self, Display};
use std::str::FromStr;

/// Languages whose operators can be read from their textual name.
/// Implemented automatically by `define_language!`.
pub trait FromOp: Sized {
    /// The operator named `op`, if there is one.
    fn from_op(op: &str) -> Option<Self>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    /// Byte offset into the input where the error was found.
    pub pos: usize,
    pub msg: String,
}

impl ParseError {
    fn new(pos: usize, msg: impl Into<String>) -> Self {
        Self {
            pos,
            msg: msg.into(),
        }
    }
}

impl Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "parse error at byte {}: {}", self.pos, self.msg)
    }
}

impl std::error::Error for ParseError {}

// ------------------------------------------------------------ public API

/// Parses a fully grounded term, e.g. `+(x, *(2, y))`.
pub fn parse_term<L: Language + FromOp>(input: &str) -> Result<Term<L>, ParseError> {
    to_term(&parse_tree(input)?)
}

/// Parses a pattern whose leaves may be variables, e.g. `+(?a, *(2, ?b))`.
pub fn parse_pattern<L: Language + FromOp>(input: &str) -> Result<Pattern<L>, ParseError> {
    to_pattern(&parse_tree(input)?)
}

impl<L: Language + FromOp> FromStr for Term<L> {
    type Err = ParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        parse_term(s)
    }
}

impl<L: Language + FromOp> FromStr for Pattern<L> {
    type Err = ParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        parse_pattern(s)
    }
}

// --------------------------------------------------------------- printing
// Printing uses the same syntax, so `parse_term(&t.to_string())` round-trips.

fn write_app<C: Display>(
    f: &mut fmt::Formatter<'_>,
    op: &dyn Display,
    children: &[C],
) -> fmt::Result {
    write!(f, "{op}")?;
    if let Some((first, rest)) = children.split_first() {
        write!(f, "({first}")?;
        for child in rest {
            write!(f, ", {child}")?;
        }
        write!(f, ")")?;
    }
    Ok(())
}

impl<L: Language + Display> Display for Term<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_app(f, &self.op, &self.children)
    }
}

impl<L: Language + Display> Display for Pattern<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.root {
            OpOrVar::Var(name) => write!(f, "?{name}"),
            OpOrVar::Op(op) => write_app(f, op, &self.children),
        }
    }
}

// ------------------------------------------------------------ syntax tree

/// Untyped parse tree: an op name applied to zero or more children.
struct Tree<'a> {
    op: &'a str,
    pos: usize,
    children: Vec<Tree<'a>>,
}

fn parse_tree(input: &str) -> Result<Tree<'_>, ParseError> {
    let mut parser = Parser { input, pos: 0 };
    let tree = parser.expr()?;
    if parser.peek().is_some() {
        return Err(ParseError::new(
            parser.pos,
            "unexpected input after expression",
        ));
    }
    Ok(tree)
}

struct Parser<'a> {
    input: &'a str,
    pos: usize,
}

impl<'a> Parser<'a> {
    /// The next non-whitespace character, without consuming it.
    fn peek(&mut self) -> Option<char> {
        let rest = &self.input[self.pos..];
        self.pos += rest.len() - rest.trim_start().len();
        self.input[self.pos..].chars().next()
    }

    /// Consumes `c` if it is the next non-whitespace character.
    fn eat(&mut self, c: char) -> bool {
        if self.peek() == Some(c) {
            self.pos += c.len_utf8();
            true
        } else {
            false
        }
    }

    fn op(&mut self) -> Result<(&'a str, usize), ParseError> {
        self.peek();
        let start = self.pos;
        let rest = &self.input[start..];
        let len = rest
            .find(|c: char| c.is_whitespace() || "(),".contains(c))
            .unwrap_or(rest.len());
        if len == 0 {
            return Err(ParseError::new(start, "expected an operator"));
        }
        self.pos += len;
        Ok((&rest[..len], start))
    }

    fn expr(&mut self) -> Result<Tree<'a>, ParseError> {
        let (op, pos) = self.op()?;
        let mut children = Vec::new();
        if self.eat('(') && !self.eat(')') {
            loop {
                children.push(self.expr()?);
                if self.eat(')') {
                    break;
                }
                if !self.eat(',') {
                    return Err(ParseError::new(self.pos, "expected ',' or ')'"));
                }
            }
        }
        Ok(Tree { op, pos, children })
    }
}

// ------------------------------------------------- tree -> term / pattern

fn to_op<L: Language + FromOp>(tree: &Tree) -> Result<L, ParseError> {
    let op = L::from_op(tree.op)
        .ok_or_else(|| ParseError::new(tree.pos, format!("unknown operator '{}'", tree.op)))?;
    if op.arity() != tree.children.len() {
        return Err(ParseError::new(
            tree.pos,
            format!(
                "operator '{}' expects {} children but got {}",
                tree.op,
                op.arity(),
                tree.children.len()
            ),
        ));
    }
    Ok(op)
}

fn to_term<L: Language + FromOp>(tree: &Tree) -> Result<Term<L>, ParseError> {
    let op = to_op(tree)?;
    let children = tree
        .children
        .iter()
        .map(to_term)
        .collect::<Result<_, _>>()?;
    Ok(Term::new(op, children))
}

fn to_pattern<L: Language + FromOp>(tree: &Tree) -> Result<Pattern<L>, ParseError> {
    if let Some(name) = tree.op.strip_prefix('?') {
        if name.is_empty() {
            return Err(ParseError::new(tree.pos, "variable needs a name after '?'"));
        }
        if !tree.children.is_empty() {
            return Err(ParseError::new(
                tree.pos,
                format!("variable '?{name}' cannot have children"),
            ));
        }
        return Ok(Pattern::var(name));
    }
    let op = to_op(tree)?;
    let children = tree
        .children
        .iter()
        .map(to_pattern)
        .collect::<Result<_, _>>()?;
    Ok(Pattern::new(op, children))
}
