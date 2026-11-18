/// Convenience macros for defining languages and rewrites.

/// Defines an enum of operators and implements `Language`, `FromOp` and `Display` for it.
///
/// ```ignore
/// define_language! {
///     pub enum Math {
///         "+" = Add(2),     // named operator with arity 2
///         "neg" = Neg(1),   // named operator with arity 1
///         "pi" = Pi,        // named operator with arity 0
///         Num(i64),         // leaf holding a value
///         Sym(String),      // leaf holding a value
///     }
/// }
/// ```
///
/// Children live in the e-graph's nodes, not in the enum, so a variant's parentheses hold
/// either its arity (named operators) or a data type (leaves). Leaf types must implement
/// `FromStr + Display + Clone + Debug + Hash + Eq`.
///
/// When parsing, named operators are tried first, then leaves in declaration order. A leaf
/// type that parses anything (like `String`) should therefore come last.
///
/// The enum derives `Clone, Debug, PartialEq, Eq, Hash`; don't derive these yourself.
#[macro_export]
macro_rules! define_language {
    // Munch one variant at a time, sorting it into named operators or data leaves.
    (@munch $head:tt [$($named:tt)*] [$($data:tt)*]
        $op:literal = $variant:ident ($arity:literal) $(, $($rest:tt)*)?
    ) => {
        $crate::define_language!(@munch $head
            [$($named)* ($op, $variant, $arity)] [$($data)*] $($($rest)*)?);
    };
    (@munch $head:tt [$($named:tt)*] [$($data:tt)*]
        $op:literal = $variant:ident $(, $($rest:tt)*)?
    ) => {
        $crate::define_language!(@munch $head
            [$($named)* ($op, $variant, 0)] [$($data)*] $($($rest)*)?);
    };
    (@munch $head:tt [$($named:tt)*] [$($data:tt)*]
        $variant:ident ($ty:ty) $(, $($rest:tt)*)?
    ) => {
        $crate::define_language!(@munch $head
            [$($named)*] [$($data)* ($variant, $ty)] $($($rest)*)?);
    };

    // All variants sorted: emit the enum and its impls.
    (@munch ([$(#[$meta:meta])*] $vis:vis $name:ident)
        [$(($op:literal, $nvariant:ident, $arity:literal))*]
        [$(($dvariant:ident, $ty:ty))*]
    ) => {
        $(#[$meta])*
        #[derive(Clone, Debug, PartialEq, Eq, Hash)]
        $vis enum $name {
            $($nvariant,)*
            $($dvariant($ty),)*
        }

        impl $crate::egraph::language::Language for $name {
            fn matches(&self, other: &Self) -> bool {
                self == other
            }

            fn arity(&self) -> usize {
                match self {
                    $($name::$nvariant => $arity,)*
                    $($name::$dvariant(_) => 0,)*
                }
            }
        }

        impl $crate::egraph::parse::FromOp for $name {
            fn from_op(op: &str) -> ::std::option::Option<Self> {
                match op {
                    $($op => return ::std::option::Option::Some($name::$nvariant),)*
                    _ => {}
                }
                $(
                    if let ::std::result::Result::Ok(data) = op.parse::<$ty>() {
                        return ::std::option::Option::Some($name::$dvariant(data));
                    }
                )*
                ::std::option::Option::None
            }
        }

        impl ::std::fmt::Display for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                match self {
                    $($name::$nvariant => f.write_str($op),)*
                    $($name::$dvariant(data) => ::std::fmt::Display::fmt(data, f),)*
                }
            }
        }
    };

    // Entry point.
    ($(#[$meta:meta])* $vis:vis enum $name:ident { $($variants:tt)* }) => {
        $crate::define_language!(@munch ([$(#[$meta])*] $vis $name) [] [] $($variants)*);
    };
}

/// Creates a `Rewrite` from a name, a search pattern and an apply pattern, using the syntax of
/// `parse::parse_pattern`. The language is inferred from context.
///
/// ```ignore
/// let rules: Vec<Rewrite<Math>> = vec![
///     rewrite!("commute-add"; "+(?a, ?b)" => "+(?b, ?a)"),
///     rewrite!("add-zero"; "+(?a, 0)" => "?a"),
/// ];
/// ```
///
/// Panics if either pattern fails to parse, or if the apply pattern uses a variable that the
/// search pattern doesn't bind.
#[macro_export]
macro_rules! rewrite {
    ($name:expr; $search:expr => $apply:expr $(,)?) => {{
        let name: &str = &$name;
        let search = $crate::egraph::parse::parse_pattern($search)
            .unwrap_or_else(|e| panic!("rewrite {name}: bad search pattern: {e}"));
        let apply = $crate::egraph::parse::parse_pattern($apply)
            .unwrap_or_else(|e| panic!("rewrite {name}: bad apply pattern: {e}"));
        $crate::egraph::rewrite::Rewrite::new(name, search, apply)
    }};
}
