//! Numeric, angle, base conversion, and bit-count expressions.

use super::calls::{fixed_calls, optional_calls};
use super::{Kind, SqlFunction};
use pyo3::prelude::*;
use pyo3::types::PyTuple;
use sqlaero_core::bind::sql_func::Form;

fixed_calls! {
    Abs, "ABS", (value), "Return the absolute value of a scalar expression.";
    Acos, "ACOS", (value), "Return the inverse cosine in radians.";
    Asin, "ASIN", (value), "Return the inverse sine in radians.";
    Atan2, "ATAN2", (y, x), "Return the angle of the point (x, y) in radians.";
    BitCount, "BIT_COUNT", (value), "Count the bits set in the supplied value.";
    Ceil, "CEIL", (value), "Return the smallest integer greater than or equal to value.";
    Conv, "CONV", (value, from_base, to_base), "Convert a number between two bases and return text.";
    Cos, "COS", (value), "Return the cosine of an angle in radians.";
    Cot, "COT", (value), "Return the cotangent of an angle in radians.";
    Crc32, "CRC32", (value), "Return the unsigned CRC-32 checksum of the argument.";
    Degrees, "DEGREES", (radians), "Convert an angle from radians to degrees.";
    Exp, "EXP", (value), "Return e raised to the given power.";
    Floor, "FLOOR", (value), "Return the largest integer less than or equal to value.";
    Ln, "LN", (value), "Return the natural logarithm.";
    Log10, "LOG10", (value), "Return the base-ten logarithm.";
    Log2, "LOG2", (value), "Return the base-two logarithm.";
    Mod, "MOD", (dividend, divisor), "Return the remainder after dividing dividend by divisor.";
    Pi, "PI", (), "Return the value of pi.";
    Pow, "POW", (base, exponent), "Raise base to the given exponent.";
    Radians, "RADIANS", (degrees), "Convert an angle from degrees to radians.";
    Sign, "SIGN", (value), "Return -1, 0, or 1 according to the sign of value.";
    Sin, "SIN", (value), "Return the sine of an angle in radians.";
    Sqrt, "SQRT", (value), "Return the square root.";
    Tan, "TAN", (value), "Return the tangent of an angle in radians.";
    Truncate, "TRUNCATE", (value, digits), "Truncate value to the given number of decimal places.";
}

optional_calls! {
    Atan, "ATAN", (y), x, "(y, x=...)", "Return the inverse tangent, optionally using point coordinates (x, y).";
    Log, "LOG", (base_or_value), value, "(base_or_value, value=...)", "Return the natural logarithm with one argument or the logarithm of value in the supplied base.";
    Rand, "RAND", (), seed, "(seed=...)", "Return a random value in [0, 1), optionally initializing the generator with a seed.";
    Round, "ROUND", (value), digits, "(value, digits=...)", "Round value to digits decimal places, defaulting to zero places.";
}

pub(super) fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    register_fixed(module)?;
    register_optional(module)
}
