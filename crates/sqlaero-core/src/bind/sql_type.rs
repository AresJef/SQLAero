//! Closed MySQL conversion targets shared by CAST, CONVERT, and JSON_VALUE.

use super::{sql_func::Keyword, writer};
use crate::protocol::framing::WireError;

macro_rules! type_names {
    ($( $name:ident = $keyword:literal; )+) => {
        /// A MySQL type keyword permitted in one or more conversion forms.
        #[derive(Clone, Copy, PartialEq, Eq)]
        pub enum Name {
            $(
                #[doc = concat!("The `", $keyword, "` target.")]
                $name,
            )+
        }
        impl Name {
            /// Return the canonical SQL spelling.
            pub fn keyword(self) -> &'static str {
                match self { $(Self::$name => $keyword,)+ }
            }
        }
    };
}

type_names! {
    Signed = "SIGNED";
    Unsigned = "UNSIGNED";
    Decimal = "DECIMAL";
    Float = "FLOAT";
    Double = "DOUBLE";
    Real = "REAL";
    Char = "CHAR";
    NChar = "NCHAR";
    Binary = "BINARY";
    Date = "DATE";
    Time = "TIME";
    Datetime = "DATETIME";
    Year = "YEAR";
    Json = "JSON";
    Point = "POINT";
    LineString = "LINESTRING";
    Polygon = "POLYGON";
    MultiPoint = "MULTIPOINT";
    MultiLineString = "MULTILINESTRING";
    MultiPolygon = "MULTIPOLYGON";
    GeometryCollection = "GEOMETRYCOLLECTION";
}

/// A native target recipe with parameters verified by its descriptor.
#[derive(Clone, Copy)]
pub struct Target {
    /// The selected fixed SQL type.
    pub name: Name,
    /// Optional length, precision, or fractional-second precision.
    pub first: Option<u32>,
    /// Optional decimal scale following its precision.
    pub second: Option<u32>,
    /// Optional character set of a CHAR target.
    pub charset: Option<Keyword>,
    /// Whether to append INTEGER to a signed or unsigned target.
    pub integer: bool,
}

impl Target {
    /// The default form of a known target.
    pub const fn plain(name: Name) -> Self {
        Self {
            name,
            first: None,
            second: None,
            charset: None,
            integer: false,
        }
    }

    /// Append only canonical syntax and validated unsigned parameters.
    /// # Errors
    /// Returns the request bound or native allocation failure.
    pub fn write(self, output: &mut Vec<u8>, limit: usize) -> Result<(), WireError> {
        writer::append(output, self.name.keyword().as_bytes(), limit)?;
        if let Some(first) = self.first {
            let mut number = itoa::Buffer::new();
            writer::append(output, b"(", limit)?;
            writer::append(output, number.format(first).as_bytes(), limit)?;
            if let Some(second) = self.second {
                writer::append(output, b",", limit)?;
                writer::append(output, number.format(second).as_bytes(), limit)?;
            }
            writer::append(output, b")", limit)?;
        }
        if let Some(charset) = self.charset {
            writer::append(output, b" CHARACTER SET ", limit)?;
            writer::append(output, charset.as_str().as_bytes(), limit)?;
        }
        if self.integer {
            writer::append(output, b" INTEGER", limit)?;
        }
        Ok(())
    }
}
