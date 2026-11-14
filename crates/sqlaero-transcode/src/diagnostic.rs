//! Borrowed source coordinates and bounded diagnostic rendering, not errors.
//!
//! Targets choose their root label. A coordinate borrows native facts only;
//! constructing or formatting a path cannot call Python or retain an input tree.

use std::fmt::{self, Write};

/// One actual position, independent of the Python/provider source family.
#[derive(Clone, Copy)]
pub enum Coordinate<'a> {
    /// A positional element or row.
    Index(usize),
    /// A key which is being acquired or normalized.
    Key(usize),
    /// A complete normalized name and its actual input ordinal.
    Field {
        /// Complete normalized source name, never shortened as a value fact.
        name: &'a str,
        /// Actual source ordinal, retained when the display must be shortened.
        ordinal: usize,
    },
}

impl fmt::Display for Coordinate<'_> {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Index(index) => write!(output, "[{index}]"),
            Self::Key(index) => write!(output, "[key#{index}]"),
            Self::Field { name, ordinal } => {
                let width = name.floor_char_boundary(name.len().min(64));
                let complete = width == name.len();
                let identifier = complete
                    && !name.is_empty()
                    && name.as_bytes().iter().enumerate().all(|(index, byte)| {
                        byte.is_ascii_alphabetic()
                            || *byte == b'_'
                            || (index != 0 && byte.is_ascii_digit())
                    });
                if identifier {
                    output.write_char('.')?;
                    output.write_str(name)
                } else if complete {
                    write!(output, "[{name:?}]")
                } else {
                    write!(output, "[{:?}…; entry#{ordinal}]", &name[..width])
                }
            }
        }
    }
}

enum Node<'a> {
    Root(&'a str),
    Child {
        parent: &'a Path<'a>,
        coordinate: Coordinate<'a>,
    },
}

/// A stack-linked view. Name/position work is rendered only on an actual error.
pub struct Path<'a>(Node<'a>);

impl<'a> Path<'a> {
    /// Begin an input path with a caller-owned root label.
    pub fn root(label: &'a str) -> Self {
        Self(Node::Root(label))
    }

    /// Borrow one actual child coordinate without copying a field name.
    pub fn child<'b>(&'b self, coordinate: Coordinate<'b>) -> Path<'b> {
        Path(Node::Child {
            parent: self,
            coordinate,
        })
    }
}

impl fmt::Display for Path<'_> {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            Node::Root(label) => output.write_str(label),
            Node::Child { parent, coordinate } => write!(output, "{parent}{coordinate}"),
        }
    }
}

/// Preserve the established positional path without rebuilding a linked view.
pub struct Indices<'a> {
    root: &'a str,
    indices: &'a [usize],
}

impl<'a> Indices<'a> {
    /// Borrow the actual deferred positional coordinates.
    pub fn new(root: &'a str, indices: &'a [usize]) -> Self {
        Self { root, indices }
    }
}

impl fmt::Display for Indices<'_> {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        output.write_str(self.root)?;
        for index in self.indices {
            write!(output, "{}", Coordinate::Index(*index))?;
        }
        Ok(())
    }
}
