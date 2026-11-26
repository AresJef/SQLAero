//! Native SQL expression grammar. The binding supplies arguments by role.

use std::sync::Arc;

/// The structural forms an operand position can accept.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// One data value, without substitution by an SQL expression.
    Value,
    /// One value or a scalar SQL expression.
    Scalar,
    /// A row of values or scalar expressions.
    Row,
    /// A library interval expression.
    Interval,
    /// An explicitly supplied identifier reference.
    Identifier,
}

/// A canonical keyword selected from a grammar-specific list.
#[derive(Clone, Copy)]
pub struct Keyword(&'static str);

impl Keyword {
    /// Select a trusted spelling. The input text itself is never written out.
    pub fn select(input: &str, allowed: &'static [&'static str]) -> Option<Self> {
        allowed
            .iter()
            .find(|keyword| keyword.eq_ignore_ascii_case(input))
            .map(|keyword| Self(keyword))
    }

    /// Return the canonical spelling for SQL output.
    pub fn as_str(self) -> &'static str {
        self.0
    }
}

/// Fixed syntax, a checked option, or a borrowed operand of the expression.
#[derive(Clone, Copy)]
pub enum Part {
    /// Literal syntax defined by the library.
    Syntax(&'static str),
    /// A selected option from a closed set.
    Option(Keyword),
    /// A retained source operand and its structural role.
    Operand(usize, Role),
}

/// The rendering recipe contains only native grammar, never source objects.
#[derive(Clone)]
pub enum Form {
    /// A literal function name followed by its opening parenthesis.
    Call(&'static str),
    /// A fixed recipe shared by all instances with the same syntax.
    Static(&'static [Part]),
    /// A recipe with checked choices specific to this expression.
    Dynamic(Arc<[Part]>),
}

impl Form {
    /// Stream a recipe in order. Plain calls need no allocation for syntax.
    pub fn parts(&self, count: usize) -> impl Iterator<Item = Part> + '_ {
        let mut index = 0;
        std::iter::from_fn(move || {
            let part = match self {
                Self::Static(parts) => parts.get(index).copied(),
                Self::Dynamic(parts) => parts.get(index).copied(),
                Self::Call(open) => match index {
                    0 => Some(Part::Syntax(open)),
                    1 if count == 0 => Some(Part::Syntax(")")),
                    n if count == 0 || (n - 1) / 2 >= count => None,
                    n if n % 2 == 1 => Some(Part::Operand(n / 2, Role::Scalar)),
                    n if n / 2 == count => Some(Part::Syntax(")")),
                    _ => Some(Part::Syntax(",")),
                },
            };
            index += 1;
            part
        })
    }
}

/// The unit is selected by the binding from EXTRACT's closed set.
pub fn extract(unit: Keyword) -> [Part; 5] {
    [
        Part::Syntax("EXTRACT("),
        Part::Option(unit),
        Part::Syntax(" FROM "),
        Part::Operand(0, Role::Scalar),
        Part::Syntax(")"),
    ]
}
