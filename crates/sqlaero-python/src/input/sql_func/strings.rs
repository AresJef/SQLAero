//! String, character, base encoding, and regular expression calls.

use super::calls::{fixed_calls, optional_calls, prefix_arguments, variadic_calls};
use super::{Kind, SqlFunction, keyword};
use crate::config::Argument;
use crate::errors;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyString, PyTuple};
use sqlaero_core::bind::sql_func::{Form, Keyword, Part, Role};

fixed_calls! {
    Ascii, "ASCII", (value), "Return the numeric code of the first byte of value.";
    Bin, "BIN", (value), "Return the binary digits of an integer as text.";
    BitLength, "BIT_LENGTH", (value), "Return the length of value in bits.";
    CharLength, "CHAR_LENGTH", (value), "Return the length of value in characters." => [CharacterLength, "CHARACTER_LENGTH"];
    FindInSet, "FIND_IN_SET", (value, string_list), "Find the one-based position of value in a comma-separated string.";
    FromBase64, "FROM_BASE64", (value), "Decode a base64 string to binary data.";
    Hex, "HEX", (value), "Return the hexadecimal representation of a number or byte string.";
    Insert, "INSERT", (value, position, length, replacement), "Replace part of a string beginning at a one-based position.";
    Instr, "INSTR", (value, substring), "Return the one-based position of substring in value or zero.";
    Left, "LEFT", (value, length), "Return the leftmost length characters.";
    Length, "LENGTH", (value), "Return the length of value in bytes." => [OctetLength, "OCTET_LENGTH"];
    Lower, "LOWER", (value), "Convert text to lowercase using its character set." => [Lcase, "LCASE"];
    Lpad, "LPAD", (value, length, padding), "Pad or truncate value on the left to the requested character length.";
    Ltrim, "LTRIM", (value), "Remove leading spaces.";
    Oct, "OCT", (value), "Return the octal digits of an integer as text.";
    Ord, "ORD", (value), "Return the combined byte code of the first character.";
    Quote, "QUOTE", (value), "Return the server's quoted SQL representation of a value.";
    Repeat, "REPEAT", (value, count), "Return count copies of value.";
    Replace, "REPLACE", (value, substring, replacement), "Replace all occurrences of substring in value.";
    Reverse, "REVERSE", (value), "Return the characters in reverse order.";
    Right, "RIGHT", (value, length), "Return the rightmost length characters.";
    Rpad, "RPAD", (value, length, padding), "Pad or truncate value on the right to the requested character length.";
    Rtrim, "RTRIM", (value), "Remove trailing spaces.";
    Soundex, "SOUNDEX", (value), "Return the MySQL Soundex code for value.";
    Space, "SPACE", (count), "Return a string of count space characters.";
    Strcmp, "STRCMP", (left, right), "Compare strings using the effective collation.";
    SubstringIndex, "SUBSTRING_INDEX", (value, delimiter, count), "Return the portion before or after the requested delimiter occurrence.";
    ToBase64, "TO_BASE64", (value), "Encode a value as base64 text.";
    Unhex, "UNHEX", (value), "Decode hexadecimal digits to binary data.";
    Upper, "UPPER", (value), "Convert text to uppercase using its character set." => [Ucase, "UCASE"];
}

optional_calls! {
    ExportSet, "EXPORT_SET", (bits, on, off), (separator, number_of_bits),
        "(bits, on, off, separator=..., number_of_bits=...)", "Select on or off text for each bit and join the results.";
    Format, "FORMAT", (value, digits), (locale), "(value, digits, locale=...)", "Format a number with grouping separators and a specified scale.";
    Locate, "LOCATE", (substring, value), (position), "(substring, value, position=...)", "Find substring starting at an optional one-based position.";
    RegexpInstr, "REGEXP_INSTR", (value, pattern), (position, occurrence, return_option, match_type),
        "(value, pattern, position=..., occurrence=..., return_option=..., match_type=...)", "Return a position of the requested regular expression match.";
    RegexpLike, "REGEXP_LIKE", (value, pattern), (match_type), "(value, pattern, match_type=...)", "Test whether value matches the regular expression.";
    RegexpReplace, "REGEXP_REPLACE", (value, pattern, replacement), (position, occurrence, match_type),
        "(value, pattern, replacement, position=..., occurrence=..., match_type=...)", "Replace regular expression matches with the supplied text.";
    RegexpSubstr, "REGEXP_SUBSTR", (value, pattern), (position, occurrence, match_type),
        "(value, pattern, position=..., occurrence=..., match_type=...)", "Return the requested regular expression match.";
    Substring, "SUBSTRING", (value, position), (length), "(value, position, length=...)", "Return characters beginning at the specified position." => [Mid, "MID", Substr, "SUBSTR"];
}

variadic_calls! {
    Concat, "CONCAT", (value), values, "Concatenate all supplied values as text or binary data.";
    ConcatWs, "CONCAT_WS", (separator, value), values, "Join non-NULL values with separator.";
    Elt, "ELT", (index, value), values, "Select the value at a one-based index.";
    Field, "FIELD", (value, first), values, "Return the one-based index of value among the remaining arguments.";
    MakeSet, "MAKE_SET", (bits, value), values, "Join values selected by the corresponding bits.";
}

/// Return characters represented by one or more numeric codes.
///
/// using selects a named MySQL charset. Omit it for binary output.
#[pyclass(extends = SqlFunction, name = "CHAR", module = "sqlaero.sqlfunc")]
struct Char;

#[pymethods]
impl Char {
    #[new]
    #[pyo3(signature = (code, *codes, using=None))]
    fn new<'py>(
        code: Bound<'py, PyAny>,
        codes: &Bound<'py, PyTuple>,
        using: Option<&Bound<'py, PyString>>,
    ) -> PyResult<PyClassInitializer<Self>> {
        let py = code.py();
        let charset = using
            .map(|name| {
                Keyword::charset(name.to_str()?).ok_or_else(|| {
                    errors::builtin_message::<PyValueError>(
                        py,
                        "CHAR using requires a server charset name",
                    )
                })
            })
            .transpose()?;
        let operands = prefix_arguments(py, [code], codes)?;
        let form = if let Some(charset) = charset {
            let mut parts = Vec::new();
            parts
                .try_reserve_exact(operands.len().saturating_mul(2).saturating_add(4))
                .map_err(|_| errors::memory_error(py))?;
            parts.push(Part::Syntax("CHAR("));
            for index in 0..operands.len() {
                if index.is_multiple_of(16_384) && index != 0 {
                    py.check_signals()?;
                }
                if index != 0 {
                    parts.push(Part::Syntax(","));
                }
                parts.push(Part::Operand(index, Role::Scalar));
            }
            parts.extend([
                Part::Syntax(" USING "),
                Part::Option(charset),
                Part::Syntax(")"),
            ]);
            Form::Dynamic(parts.into())
        } else {
            Form::Call("CHAR(")
        };
        let expression = SqlFunction::new("CHAR", Kind::Scalar, form, operands)?;
        Ok(PyClassInitializer::from(expression).add_subclass(Self))
    }
}

/// Return the one-based position of substring within value, or zero.
#[pyclass(extends = SqlFunction, name = "POSITION", module = "sqlaero.sqlfunc")]
struct Position;

#[pymethods]
impl Position {
    #[new]
    fn new<'py>(
        substring: Bound<'py, PyAny>,
        value: Bound<'py, PyAny>,
    ) -> PyResult<PyClassInitializer<Self>> {
        const FORM: Form = Form::Static(&[
            Part::Syntax("POSITION("),
            Part::Operand(0, Role::Scalar),
            Part::Syntax(" IN "),
            Part::Operand(1, Role::Scalar),
            Part::Syntax(")"),
        ]);
        let operands = PyTuple::new(substring.py(), [substring, value])?;
        let expression = SqlFunction::new("POSITION", Kind::Scalar, FORM, operands)?;
        Ok(PyClassInitializer::from(expression).add_subclass(Self))
    }
}

/// Remove a repeated prefix or suffix, defaulting to spaces on both sides.
///
/// direction accepts BOTH, LEADING, or TRAILING, ignoring case.
#[pyclass(extends = SqlFunction, name = "TRIM", module = "sqlaero.sqlfunc")]
struct Trim;

#[pymethods]
impl Trim {
    #[new]
    #[pyo3(signature = (value, *, remove=Argument::default(), direction=None), text_signature = "(value, *, remove=..., direction=None)")]
    fn new<'py>(
        value: Bound<'py, PyAny>,
        remove: Argument<'py>,
        direction: Option<&Bound<'py, PyAny>>,
    ) -> PyResult<PyClassInitializer<Self>> {
        let py = value.py();
        let direction = direction
            .map(|direction| {
                keyword(
                    "TRIM",
                    "direction",
                    direction,
                    &["BOTH", "LEADING", "TRAILING"],
                )
            })
            .transpose()?;
        let remove = remove.0;
        let form = if direction.is_none() && remove.is_none() {
            Form::Call("TRIM(")
        } else {
            let mut parts = Vec::with_capacity(7);
            parts.push(Part::Syntax("TRIM("));
            if let Some(direction) = direction {
                parts.push(Part::Option(direction));
            }
            if remove.is_some() {
                if direction.is_some() {
                    parts.push(Part::Syntax(" "));
                }
                parts.push(Part::Operand(1, Role::Scalar));
            }
            parts.extend([
                Part::Syntax(" FROM "),
                Part::Operand(0, Role::Scalar),
                Part::Syntax(")"),
            ]);
            Form::Dynamic(parts.into())
        };
        let operands = PyTuple::new(
            py,
            [Some(value), remove]
                .into_iter()
                .flatten()
                .collect::<smallvec::SmallVec<[_; 2]>>(),
        )?;
        let expression = SqlFunction::new("TRIM", Kind::Scalar, form, operands)?;
        Ok(PyClassInitializer::from(expression).add_subclass(Self))
    }
}

pub(super) fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    register_fixed(module)?;
    register_optional(module)?;
    register_variadic(module)?;
    module.add_class::<Char>()?;
    module.add_class::<Position>()?;
    module.add_class::<Trim>()
}
