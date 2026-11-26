"""Concrete MySQL function expression classes."""

from typing import Literal, final, overload
from ._sqlaero import Column, _SQLFunction, _SQLInterval  # pyright: ignore[reportPrivateUsage]
from .sqltype import _Target  # pyright: ignore[reportPrivateUsage]

__all__: list[str]

@final
class CAST(_SQLFunction):
    """Convert a value to the selected sqltype target."""
    def __init__(self, value: object, target: _Target, *, at_utc: bool = False) -> None: ...

@final
class CONVERT(_SQLFunction):
    """Convert with a sqltype target or transcode with a MySQL charset."""
    @overload
    def __init__(self, value: object, target: _Target, *, using: None = None) -> None: ...
    @overload
    def __init__(self, value: object, *, using: str) -> None: ...

@final
class ABS(_SQLFunction):
    """Return the absolute value of a scalar expression."""
    def __init__(self, value: object) -> None: ...

@final
class ACOS(_SQLFunction):
    """Return the inverse cosine in radians."""
    def __init__(self, value: object) -> None: ...

@final
class ASIN(_SQLFunction):
    """Return the inverse sine in radians."""
    def __init__(self, value: object) -> None: ...

@final
class ATAN(_SQLFunction):
    """Return the inverse tangent, optionally using point coordinates (x, y)."""
    def __init__(self, y: object, x: object = ...) -> None: ...

@final
class ATAN2(_SQLFunction):
    """Return the angle of the point (x, y) in radians."""
    def __init__(self, y: object, x: object) -> None: ...

@final
class BIT_COUNT(_SQLFunction):
    """Count the bits set in the supplied value."""
    def __init__(self, value: object) -> None: ...

@final
class CEIL(_SQLFunction):
    """Return the smallest integer greater than or equal to value."""
    def __init__(self, value: object) -> None: ...

@final
class CONV(_SQLFunction):
    """Convert a number between two bases and return text."""
    def __init__(self, value: object, from_base: object, to_base: object) -> None: ...

@final
class COS(_SQLFunction):
    """Return the cosine of an angle in radians."""
    def __init__(self, value: object) -> None: ...

@final
class COT(_SQLFunction):
    """Return the cotangent of an angle in radians."""
    def __init__(self, value: object) -> None: ...

@final
class CRC32(_SQLFunction):
    """Return the unsigned CRC-32 checksum of the argument."""
    def __init__(self, value: object) -> None: ...

@final
class DEGREES(_SQLFunction):
    """Convert an angle from radians to degrees."""
    def __init__(self, radians: object) -> None: ...

@final
class EXP(_SQLFunction):
    """Return e raised to the given power."""
    def __init__(self, value: object) -> None: ...

@final
class FLOOR(_SQLFunction):
    """Return the largest integer less than or equal to value."""
    def __init__(self, value: object) -> None: ...

@final
class LN(_SQLFunction):
    """Return the natural logarithm."""
    def __init__(self, value: object) -> None: ...

@final
class LOG(_SQLFunction):
    """Return the natural logarithm with one argument or the logarithm of value in the supplied base."""
    def __init__(self, base_or_value: object, value: object = ...) -> None: ...

@final
class LOG10(_SQLFunction):
    """Return the base-ten logarithm."""
    def __init__(self, value: object) -> None: ...

@final
class LOG2(_SQLFunction):
    """Return the base-two logarithm."""
    def __init__(self, value: object) -> None: ...

@final
class MOD(_SQLFunction):
    """Return the remainder after dividing dividend by divisor."""
    def __init__(self, dividend: object, divisor: object) -> None: ...

@final
class PI(_SQLFunction):
    """Return the value of pi."""
    def __init__(self) -> None: ...

@final
class POW(_SQLFunction):
    """Raise base to the given exponent."""
    def __init__(self, base: object, exponent: object) -> None: ...

@final
class RADIANS(_SQLFunction):
    """Convert an angle from degrees to radians."""
    def __init__(self, degrees: object) -> None: ...

@final
class RAND(_SQLFunction):
    """Return a random value in [0, 1), optionally initializing the generator with a seed."""
    def __init__(self, seed: object = ...) -> None: ...

@final
class ROUND(_SQLFunction):
    """Round value to digits decimal places, defaulting to zero places."""
    def __init__(self, value: object, digits: object = ...) -> None: ...

@final
class SIGN(_SQLFunction):
    """Return -1, 0, or 1 according to the sign of value."""
    def __init__(self, value: object) -> None: ...

@final
class SIN(_SQLFunction):
    """Return the sine of an angle in radians."""
    def __init__(self, value: object) -> None: ...

@final
class SQRT(_SQLFunction):
    """Return the square root."""
    def __init__(self, value: object) -> None: ...

@final
class TAN(_SQLFunction):
    """Return the tangent of an angle in radians."""
    def __init__(self, value: object) -> None: ...

@final
class TRUNCATE(_SQLFunction):
    """Truncate value to the given number of decimal places."""
    def __init__(self, value: object, digits: object) -> None: ...

@final
class DATE(_SQLFunction):
    """Extract the date part of a date or datetime expression."""
    def __init__(self, value: object) -> None: ...

@final
class EXTRACT(_SQLFunction):
    """Extract the requested MySQL temporal unit from a scalar expression."""
    def __init__(self, unit: str, value: object) -> None: ...

@final
class DATE_ADD(_SQLFunction):
    """Add one MySQL interval to a date or datetime expression."""
    def __init__(self, date: object, interval: _SQLInterval) -> None: ...

@final
class SUM(_SQLFunction):
    """Sum a scalar expression over a group, with optional DISTINCT."""
    def __init__(self, value: object, *, distinct: bool = False) -> None: ...

@final
class ASCII(_SQLFunction):
    """Return the numeric code of the first byte of value."""
    def __init__(self, value: object) -> None: ...

@final
class BIN(_SQLFunction):
    """Return the binary digits of an integer as text."""
    def __init__(self, value: object) -> None: ...

@final
class BIT_LENGTH(_SQLFunction):
    """Return the length of value in bits."""
    def __init__(self, value: object) -> None: ...

@final
class CHAR(_SQLFunction):
    """Return characters represented by one or more numeric codes with an optional MySQL charset."""
    def __init__(self, code: object, *codes: object, using: str | None = None) -> None: ...

@final
class CHAR_LENGTH(_SQLFunction):
    """Return the length of value in characters."""
    def __init__(self, value: object) -> None: ...

@final
class CONCAT(_SQLFunction):
    """Concatenate all supplied values as text or binary data."""
    def __init__(self, value: object, *values: object) -> None: ...

@final
class CONCAT_WS(_SQLFunction):
    """Join non-NULL values with separator."""
    def __init__(self, separator: object, value: object, *values: object) -> None: ...

@final
class ELT(_SQLFunction):
    """Select the value at a one-based index."""
    def __init__(self, index: object, value: object, *values: object) -> None: ...

@final
class EXPORT_SET(_SQLFunction):
    """Select on or off text for each bit and join the results."""
    def __init__(
        self,
        bits: object,
        on: object,
        off: object,
        separator: object = ...,
        number_of_bits: object = ...,
    ) -> None: ...

@final
class FIELD(_SQLFunction):
    """Return the one-based index of value among the remaining arguments."""
    def __init__(self, value: object, first: object, *values: object) -> None: ...

@final
class FIND_IN_SET(_SQLFunction):
    """Find the one-based position of value in a comma-separated string."""
    def __init__(self, value: object, string_list: object) -> None: ...

@final
class FORMAT(_SQLFunction):
    """Format a number with grouping separators and a specified scale."""
    def __init__(self, value: object, digits: object, locale: object = ...) -> None: ...

@final
class FROM_BASE64(_SQLFunction):
    """Decode a base64 string to binary data."""
    def __init__(self, value: object) -> None: ...

@final
class HEX(_SQLFunction):
    """Return the hexadecimal representation of a number or byte string."""
    def __init__(self, value: object) -> None: ...

@final
class INSERT(_SQLFunction):
    """Replace part of a string beginning at a one-based position."""
    def __init__(self, value: object, position: object, length: object, replacement: object) -> None: ...

@final
class INSTR(_SQLFunction):
    """Return the one-based position of substring in value or zero."""
    def __init__(self, value: object, substring: object) -> None: ...

@final
class LEFT(_SQLFunction):
    """Return the leftmost length characters."""
    def __init__(self, value: object, length: object) -> None: ...

@final
class LENGTH(_SQLFunction):
    """Return the length of value in bytes."""
    def __init__(self, value: object) -> None: ...

@final
class LOCATE(_SQLFunction):
    """Find substring starting at an optional one-based position."""
    def __init__(self, substring: object, value: object, position: object = ...) -> None: ...

@final
class LOWER(_SQLFunction):
    """Convert text to lowercase using its character set."""
    def __init__(self, value: object) -> None: ...

@final
class LPAD(_SQLFunction):
    """Pad or truncate value on the left to the requested character length."""
    def __init__(self, value: object, length: object, padding: object) -> None: ...

@final
class LTRIM(_SQLFunction):
    """Remove leading spaces."""
    def __init__(self, value: object) -> None: ...

@final
class MAKE_SET(_SQLFunction):
    """Join values selected by the corresponding bits."""
    def __init__(self, bits: object, value: object, *values: object) -> None: ...

@final
class OCT(_SQLFunction):
    """Return the octal digits of an integer as text."""
    def __init__(self, value: object) -> None: ...

@final
class ORD(_SQLFunction):
    """Return the combined byte code of the first character."""
    def __init__(self, value: object) -> None: ...

@final
class POSITION(_SQLFunction):
    """Return the one-based position of substring within value, or zero."""
    def __init__(self, substring: object, value: object) -> None: ...

@final
class QUOTE(_SQLFunction):
    """Return the server's quoted SQL representation of a value."""
    def __init__(self, value: object) -> None: ...

@final
class REGEXP_INSTR(_SQLFunction):
    """Return a position of the requested regular expression match."""
    def __init__(
        self,
        value: object,
        pattern: object,
        position: object = ...,
        occurrence: object = ...,
        return_option: object = ...,
        match_type: object = ...,
    ) -> None: ...

@final
class REGEXP_LIKE(_SQLFunction):
    """Test whether value matches the regular expression."""
    def __init__(self, value: object, pattern: object, match_type: object = ...) -> None: ...

@final
class REGEXP_REPLACE(_SQLFunction):
    """Replace regular expression matches with the supplied text."""
    def __init__(
        self,
        value: object,
        pattern: object,
        replacement: object,
        position: object = ...,
        occurrence: object = ...,
        match_type: object = ...,
    ) -> None: ...

@final
class REGEXP_SUBSTR(_SQLFunction):
    """Return the requested regular expression match."""
    def __init__(
        self,
        value: object,
        pattern: object,
        position: object = ...,
        occurrence: object = ...,
        match_type: object = ...,
    ) -> None: ...

@final
class REPEAT(_SQLFunction):
    """Return count copies of value."""
    def __init__(self, value: object, count: object) -> None: ...

@final
class REPLACE(_SQLFunction):
    """Replace all occurrences of substring in value."""
    def __init__(self, value: object, substring: object, replacement: object) -> None: ...

@final
class REVERSE(_SQLFunction):
    """Return the characters in reverse order."""
    def __init__(self, value: object) -> None: ...

@final
class RIGHT(_SQLFunction):
    """Return the rightmost length characters."""
    def __init__(self, value: object, length: object) -> None: ...

@final
class RPAD(_SQLFunction):
    """Pad or truncate value on the right to the requested character length."""
    def __init__(self, value: object, length: object, padding: object) -> None: ...

@final
class RTRIM(_SQLFunction):
    """Remove trailing spaces."""
    def __init__(self, value: object) -> None: ...

@final
class SOUNDEX(_SQLFunction):
    """Return the MySQL Soundex code for value."""
    def __init__(self, value: object) -> None: ...

@final
class SPACE(_SQLFunction):
    """Return a string of count space characters."""
    def __init__(self, count: object) -> None: ...

@final
class STRCMP(_SQLFunction):
    """Compare strings using the effective collation."""
    def __init__(self, left: object, right: object) -> None: ...

@final
class SUBSTRING(_SQLFunction):
    """Return characters beginning at the specified position."""
    def __init__(self, value: object, position: object, length: object = ...) -> None: ...

@final
class SUBSTRING_INDEX(_SQLFunction):
    """Return the portion before or after the requested delimiter occurrence."""
    def __init__(self, value: object, delimiter: object, count: object) -> None: ...

@final
class TO_BASE64(_SQLFunction):
    """Encode a value as base64 text."""
    def __init__(self, value: object) -> None: ...

@final
class TRIM(_SQLFunction):
    """Remove a repeated prefix or suffix, defaulting to spaces on both sides."""
    def __init__(self, value: object, *, remove: object = ..., direction: str | None = None) -> None: ...

@final
class UNHEX(_SQLFunction):
    """Decode hexadecimal digits to binary data."""
    def __init__(self, value: object) -> None: ...

@final
class UPPER(_SQLFunction):
    """Convert text to uppercase using its character set."""
    def __init__(self, value: object) -> None: ...

@final
class AES_DECRYPT(_SQLFunction):
    """Decrypt AES ciphertext using the session's block mode and optional key derivation."""
    def __init__(
        self,
        value: object,
        key: object,
        iv: object = ...,
        kdf: object = ...,
        salt: object = ...,
        info_or_iterations: object = ...,
    ) -> None: ...

@final
class AES_ENCRYPT(_SQLFunction):
    """Encrypt data using the session's block mode and optional key derivation."""
    def __init__(
        self,
        value: object,
        key: object,
        iv: object = ...,
        kdf: object = ...,
        salt: object = ...,
        info_or_iterations: object = ...,
    ) -> None: ...

@final
class BIN_TO_UUID(_SQLFunction):
    """Convert a binary UUID to text, optionally reversing the time-field swap."""
    def __init__(self, value: object, swap_flag: object = ...) -> None: ...

@final
class CHARSET(_SQLFunction):
    """Return the character set of value."""
    def __init__(self, value: object) -> None: ...

@final
class COALESCE(_SQLFunction):
    """Return the first value that is not SQL NULL."""
    def __init__(self, value: object, *values: object) -> None: ...

@final
class COERCIBILITY(_SQLFunction):
    """Return the collation coercibility of value."""
    def __init__(self, value: object) -> None: ...

@final
class COLLATION(_SQLFunction):
    """Return the collation of value."""
    def __init__(self, value: object) -> None: ...

@final
class COMPRESS(_SQLFunction):
    """Compress value into MySQL's binary format."""
    def __init__(self, value: object) -> None: ...

@final
class CONNECTION_ID(_SQLFunction):
    """Return the current server connection identifier."""
    def __init__(self) -> None: ...

@final
class CURRENT_ROLE(_SQLFunction):
    """Return the active roles of the current session."""
    def __init__(self) -> None: ...

@final
class CURRENT_USER(_SQLFunction):
    """Return the MySQL account used to check privileges."""
    def __init__(self) -> None: ...

@final
class DATABASE(_SQLFunction):
    """Return the current default database."""
    def __init__(self) -> None: ...

@final
class GREATEST(_SQLFunction):
    """Return the greatest value according to MySQL comparison rules."""
    def __init__(self, first: object, second: object, *values: object) -> None: ...

@final
class ICU_VERSION(_SQLFunction):
    """Return the ICU library version used by the server."""
    def __init__(self) -> None: ...

@final
class IF(_SQLFunction):
    """Select a value according to the SQL condition."""
    def __init__(self, condition: object, if_true: object, if_false: object) -> None: ...

@final
class IFNULL(_SQLFunction):
    """Return fallback when value is SQL NULL."""
    def __init__(self, value: object, fallback: object) -> None: ...

@final
class INET6_ATON(_SQLFunction):
    """Convert a text IPv4 or IPv6 address to binary."""
    def __init__(self, address: object) -> None: ...

@final
class INET6_NTOA(_SQLFunction):
    """Convert a binary IPv4 or IPv6 address to text."""
    def __init__(self, address: object) -> None: ...

@final
class INET_ATON(_SQLFunction):
    """Convert a text IPv4 address to an integer."""
    def __init__(self, address: object) -> None: ...

@final
class INET_NTOA(_SQLFunction):
    """Convert an integer IPv4 address to text."""
    def __init__(self, address: object) -> None: ...

@final
class INTERVAL(_SQLFunction):
    """Return the position of value among increasing numeric boundaries."""
    def __init__(self, value: object, first: object, *boundaries: object) -> None: ...

@final
class ISNULL(_SQLFunction):
    """Test whether value is SQL NULL."""
    def __init__(self, value: object) -> None: ...

@final
class IS_IPV4(_SQLFunction):
    """Test whether address is valid IPv4 text."""
    def __init__(self, address: object) -> None: ...

@final
class IS_IPV4_COMPAT(_SQLFunction):
    """Test whether a binary address is IPv4-compatible IPv6."""
    def __init__(self, address: object) -> None: ...

@final
class IS_IPV4_MAPPED(_SQLFunction):
    """Test whether a binary address is IPv4-mapped IPv6."""
    def __init__(self, address: object) -> None: ...

@final
class IS_IPV6(_SQLFunction):
    """Test whether address is valid IPv6 text."""
    def __init__(self, address: object) -> None: ...

@final
class IS_UUID(_SQLFunction):
    """Test whether value is a valid MySQL UUID string."""
    def __init__(self, value: object) -> None: ...

@final
class LAST_INSERT_ID(_SQLFunction):
    """Read the last insert ID or set it for the session when value is supplied."""
    def __init__(self, value: object = ...) -> None: ...

@final
class LEAST(_SQLFunction):
    """Return the least value according to MySQL comparison rules."""
    def __init__(self, first: object, second: object, *values: object) -> None: ...

@final
class NULLIF(_SQLFunction):
    """Return SQL NULL when left equals right, otherwise left."""
    def __init__(self, left: object, right: object) -> None: ...

@final
class RANDOM_BYTES(_SQLFunction):
    """Return length random bytes from the server."""
    def __init__(self, length: object) -> None: ...

@final
class ROW_COUNT(_SQLFunction):
    """Return the affected row count from the preceding statement."""
    def __init__(self) -> None: ...

@final
class SHA2(_SQLFunction):
    """Return a hexadecimal SHA-2 digest using the specified digest width."""
    def __init__(self, value: object, bits: object) -> None: ...

@final
class UNCOMPRESS(_SQLFunction):
    """Decode MySQL compressed data to its original bytes."""
    def __init__(self, value: object) -> None: ...

@final
class UNCOMPRESSED_LENGTH(_SQLFunction):
    """Return the original byte length of MySQL compressed data."""
    def __init__(self, value: object) -> None: ...

@final
class USER(_SQLFunction):
    """Return the supplied client username and client host."""
    def __init__(self) -> None: ...

@final
class UUID(_SQLFunction):
    """Generate a version-1 UUID string when the server evaluates the expression."""
    def __init__(self) -> None: ...

@final
class UUID_SHORT(_SQLFunction):
    """Generate a server-based unsigned integer identifier."""
    def __init__(self) -> None: ...

@final
class UUID_TO_BIN(_SQLFunction):
    """Convert a UUID to binary, optionally swapping the time fields."""
    def __init__(self, value: object, swap_flag: object = ...) -> None: ...

@final
class VERSION(_SQLFunction):
    """Return the server's version string."""
    def __init__(self) -> None: ...

@final
class JSON_ARRAY(_SQLFunction):
    """Build a JSON array from zero or more SQL values."""
    def __init__(self, *values: object) -> None: ...

@final
class JSON_ARRAY_APPEND(_SQLFunction):
    """Append values to arrays at the specified JSON paths."""
    def __init__(self, document: object, path: object, value: object, *path_values: object) -> None: ...

@final
class JSON_ARRAY_INSERT(_SQLFunction):
    """Insert values at JSON array positions."""
    def __init__(self, document: object, path: object, value: object, *path_values: object) -> None: ...

@final
class JSON_INSERT(_SQLFunction):
    """Insert values at missing JSON paths."""
    def __init__(self, document: object, path: object, value: object, *path_values: object) -> None: ...

@final
class JSON_OBJECT(_SQLFunction):
    """Build a JSON object from zero or more alternating SQL keys and values."""
    def __init__(self, *key_values: object) -> None: ...

@final
class JSON_REPLACE(_SQLFunction):
    """Replace values at existing JSON paths."""
    def __init__(self, document: object, path: object, value: object, *path_values: object) -> None: ...

@final
class JSON_SEARCH(_SQLFunction):
    """Find one or all matching strings in a JSON document."""
    def __init__(
        self, document: object, mode: object, search: object, escape_char: str | None = ..., *paths: object,
    ) -> None: ...

@final
class JSON_SET(_SQLFunction):
    """Insert or replace values at the specified JSON paths."""
    def __init__(self, document: object, path: object, value: object, *path_values: object) -> None: ...

_JsonPolicy = Literal["null", "error"] | tuple[Literal["default"], str | int | float | None]

@final
class JSON_VALUE(_SQLFunction):
    """Extract a JSON scalar at a literal path, optionally with a conversion target."""
    def __init__(
        self, document: object, path: str, *, returning: _Target | None = None,
        on_empty: _JsonPolicy | None = None, on_error: _JsonPolicy | None = None,
    ) -> None: ...

@final
class JSON_CONTAINS(_SQLFunction):
    """Check whether a JSON document contains a candidate at an optional path."""
    def __init__(self, document: object, candidate: object, path: object = ...) -> None: ...

@final
class JSON_CONTAINS_PATH(_SQLFunction):
    """Check for data at one or all given JSON paths."""
    def __init__(self, document: object, mode: object, path: object, *paths: object) -> None: ...

@final
class JSON_DEPTH(_SQLFunction):
    """Return the maximum depth of a JSON document."""
    def __init__(self, document: object) -> None: ...

@final
class JSON_EXTRACT(_SQLFunction):
    """Extract JSON data at the specified paths."""
    def __init__(self, document: object, path: object, *paths: object) -> None: ...

@final
class JSON_KEYS(_SQLFunction):
    """Return the keys of a JSON object at an optional path."""
    def __init__(self, document: object, path: object = ...) -> None: ...

@final
class JSON_LENGTH(_SQLFunction):
    """Return the length of a JSON value at an optional path."""
    def __init__(self, document: object, path: object = ...) -> None: ...

@final
class JSON_MERGE_PATCH(_SQLFunction):
    """Merge JSON documents using JSON Merge Patch."""
    def __init__(self, document: object, other: object, *documents: object) -> None: ...

@final
class JSON_MERGE_PRESERVE(_SQLFunction):
    """Merge JSON documents while preserving conflicting values."""
    def __init__(self, document: object, other: object, *documents: object) -> None: ...

@final
class JSON_OVERLAPS(_SQLFunction):
    """Check whether JSON documents share an element or key-value pair."""
    def __init__(self, document: object, other: object) -> None: ...

@final
class JSON_PRETTY(_SQLFunction):
    """Return an indented representation of a JSON document."""
    def __init__(self, document: object) -> None: ...

@final
class JSON_QUOTE(_SQLFunction):
    """Quote a string as a JSON string literal."""
    def __init__(self, value: object) -> None: ...

@final
class JSON_REMOVE(_SQLFunction):
    """Remove JSON values at the specified paths."""
    def __init__(self, document: object, path: object, *paths: object) -> None: ...

@final
class JSON_SCHEMA_VALID(_SQLFunction):
    """Check a JSON document against a JSON Schema."""
    def __init__(self, schema: object, document: object) -> None: ...

@final
class JSON_SCHEMA_VALIDATION_REPORT(_SQLFunction):
    """Return the JSON Schema validation report for a document."""
    def __init__(self, schema: object, document: object) -> None: ...

@final
class JSON_STORAGE_FREE(_SQLFunction):
    """Return bytes freed in a JSON column by in-place updates."""
    def __init__(self, document: object) -> None: ...

@final
class JSON_STORAGE_SIZE(_SQLFunction):
    """Return the binary storage size of a JSON document."""
    def __init__(self, document: object) -> None: ...

@final
class JSON_TYPE(_SQLFunction):
    """Return the type name of the JSON value."""
    def __init__(self, document: object) -> None: ...

@final
class JSON_UNQUOTE(_SQLFunction):
    """Unquote a JSON string to yield a SQL string."""
    def __init__(self, value: object) -> None: ...

@final
class JSON_VALID(_SQLFunction):
    """Check whether a value is a valid JSON document."""
    def __init__(self, document: object) -> None: ...

@final
class ADDDATE(_SQLFunction):
    """Add a number of days or an explicit MySQL interval to a date."""
    def __init__(self, date: object, days: object) -> None: ...

@final
class SUBDATE(_SQLFunction):
    """Subtract a number of days or an explicit MySQL interval from a date."""
    def __init__(self, date: object, days: object) -> None: ...

@final
class DATE_SUB(_SQLFunction):
    """Subtract an explicit MySQL interval from a date or datetime."""
    def __init__(self, date: object, interval: _SQLInterval) -> None: ...

@final
class GET_FORMAT(_SQLFunction):
    """Return a MySQL format string for the given temporal type and format name."""
    def __init__(self, type: str, format: object) -> None: ...

@final
class TIMESTAMPADD(_SQLFunction):
    """Add an integer quantity in a selected unit to a temporal value."""
    def __init__(self, unit: str, quantity: object, value: object) -> None: ...

@final
class TIMESTAMPDIFF(_SQLFunction):
    """Return end minus start as a count of completed units."""
    def __init__(self, unit: str, start: object, end: object) -> None: ...

@final
class CURRENT_TIME(_SQLFunction):
    """Return the current session time with optional fractional precision."""
    def __init__(self, fsp: int = ...) -> None: ...

@final
class CURRENT_TIMESTAMP(_SQLFunction):
    """Return the current session datetime with optional fractional precision."""
    def __init__(self, fsp: int = ...) -> None: ...

@final
class NOW(_SQLFunction):
    """Return the session datetime at statement start with optional fractional precision."""
    def __init__(self, fsp: int = ...) -> None: ...

@final
class SYSDATE(_SQLFunction):
    """Return the system datetime at invocation with optional fractional precision."""
    def __init__(self, fsp: int = ...) -> None: ...

@final
class UTC_TIME(_SQLFunction):
    """Return the current UTC time with optional fractional precision."""
    def __init__(self, fsp: int = ...) -> None: ...

@final
class UTC_TIMESTAMP(_SQLFunction):
    """Return the current UTC datetime with optional fractional precision."""
    def __init__(self, fsp: int = ...) -> None: ...

@final
class ADDTIME(_SQLFunction):
    """Add a time value to a time or datetime."""
    def __init__(self, value: object, time: object) -> None: ...

@final
class CONVERT_TZ(_SQLFunction):
    """Convert a datetime from one time zone to another."""
    def __init__(self, value: object, from_zone: object, to_zone: object) -> None: ...

@final
class CURRENT_DATE(_SQLFunction):
    """Return the current date in the session time zone."""
    def __init__(self) -> None: ...

@final
class DATEDIFF(_SQLFunction):
    """Return left minus right as a difference in calendar days."""
    def __init__(self, left: object, right: object) -> None: ...

@final
class DATE_FORMAT(_SQLFunction):
    """Format a date or datetime with a MySQL format string."""
    def __init__(self, value: object, format: object) -> None: ...

@final
class DAY(_SQLFunction):
    """Return the day of the month."""
    def __init__(self, value: object) -> None: ...

@final
class DAYNAME(_SQLFunction):
    """Return the localized name of the weekday."""
    def __init__(self, value: object) -> None: ...

@final
class DAYOFWEEK(_SQLFunction):
    """Return the weekday index, with Sunday as 1."""
    def __init__(self, value: object) -> None: ...

@final
class DAYOFYEAR(_SQLFunction):
    """Return the day of the year starting at 1."""
    def __init__(self, value: object) -> None: ...

@final
class FROM_DAYS(_SQLFunction):
    """Convert a MySQL day number to a date."""
    def __init__(self, days: object) -> None: ...

@final
class FROM_UNIXTIME(_SQLFunction):
    """Convert Unix epoch seconds to the session time zone, optionally formatting the result."""
    def __init__(self, seconds: object, format: object = ...) -> None: ...

@final
class HOUR(_SQLFunction):
    """Extract the hour component of a time or datetime."""
    def __init__(self, value: object) -> None: ...

@final
class LAST_DAY(_SQLFunction):
    """Return the date of the last day of the input month."""
    def __init__(self, value: object) -> None: ...

@final
class MAKEDATE(_SQLFunction):
    """Make a date from a year and day of the year."""
    def __init__(self, year: object, day_of_year: object) -> None: ...

@final
class MAKETIME(_SQLFunction):
    """Make a time from hours, minutes, and seconds."""
    def __init__(self, hour: object, minute: object, second: object) -> None: ...

@final
class MICROSECOND(_SQLFunction):
    """Extract the microsecond component of a temporal value."""
    def __init__(self, value: object) -> None: ...

@final
class MINUTE(_SQLFunction):
    """Extract the minute component of a temporal value."""
    def __init__(self, value: object) -> None: ...

@final
class MONTH(_SQLFunction):
    """Return the number of the month."""
    def __init__(self, value: object) -> None: ...

@final
class MONTHNAME(_SQLFunction):
    """Return the localized name of the month."""
    def __init__(self, value: object) -> None: ...

@final
class PERIOD_ADD(_SQLFunction):
    """Add months to a MySQL year-month period."""
    def __init__(self, period: object, months: object) -> None: ...

@final
class PERIOD_DIFF(_SQLFunction):
    """Return the difference in months between two year-month periods."""
    def __init__(self, left: object, right: object) -> None: ...

@final
class QUARTER(_SQLFunction):
    """Return the quarter of the year."""
    def __init__(self, value: object) -> None: ...

@final
class SECOND(_SQLFunction):
    """Extract the seconds component of a temporal value."""
    def __init__(self, value: object) -> None: ...

@final
class SEC_TO_TIME(_SQLFunction):
    """Convert a number of seconds to a MySQL time."""
    def __init__(self, seconds: object) -> None: ...

@final
class STR_TO_DATE(_SQLFunction):
    """Parse a string using a MySQL temporal format."""
    def __init__(self, value: object, format: object) -> None: ...

@final
class SUBTIME(_SQLFunction):
    """Subtract a time value from a time or datetime."""
    def __init__(self, value: object, time: object) -> None: ...

@final
class TIME(_SQLFunction):
    """Extract the time component of a temporal expression."""
    def __init__(self, value: object) -> None: ...

@final
class TIMEDIFF(_SQLFunction):
    """Return the difference between two times or datetimes."""
    def __init__(self, left: object, right: object) -> None: ...

@final
class TIMESTAMP(_SQLFunction):
    """Return a datetime, optionally adding a time value."""
    def __init__(self, value: object, time: object = ...) -> None: ...

@final
class TIME_FORMAT(_SQLFunction):
    """Format a time with a MySQL format string."""
    def __init__(self, value: object, format: object) -> None: ...

@final
class TIME_TO_SEC(_SQLFunction):
    """Convert a time to a number of seconds."""
    def __init__(self, value: object) -> None: ...

@final
class TO_DAYS(_SQLFunction):
    """Return the MySQL day number for a date."""
    def __init__(self, value: object) -> None: ...

@final
class TO_SECONDS(_SQLFunction):
    """Return the number of seconds since year zero."""
    def __init__(self, value: object) -> None: ...

@final
class UNIX_TIMESTAMP(_SQLFunction):
    """Return Unix epoch seconds for a session datetime or for the current time."""
    def __init__(self, value: object = ...) -> None: ...

@final
class UTC_DATE(_SQLFunction):
    """Return the current date in UTC."""
    def __init__(self) -> None: ...

@final
class WEEK(_SQLFunction):
    """Return the week number using MySQL's optional week mode."""
    def __init__(self, value: object, mode: object = ...) -> None: ...

@final
class WEEKDAY(_SQLFunction):
    """Return the weekday index, with Monday as 0."""
    def __init__(self, value: object) -> None: ...

@final
class WEEKOFYEAR(_SQLFunction):
    """Return the ISO week number of a date."""
    def __init__(self, value: object) -> None: ...

@final
class YEAR(_SQLFunction):
    """Return the year of a date or datetime."""
    def __init__(self, value: object) -> None: ...

@final
class YEARWEEK(_SQLFunction):
    """Return the corresponding year and week number using MySQL's optional week mode."""
    def __init__(self, value: object, mode: object = ...) -> None: ...

@final
class AVG(_SQLFunction):
    """Return the average of the group, optionally over distinct values."""
    def __init__(self, value: object, *, distinct: bool = False) -> None: ...

@final
class MAX(_SQLFunction):
    """Return the largest group value. DISTINCT does not affect the result."""
    def __init__(self, value: object, *, distinct: bool = False) -> None: ...

@final
class MIN(_SQLFunction):
    """Return the smallest group value. DISTINCT does not affect the result."""
    def __init__(self, value: object, *, distinct: bool = False) -> None: ...

@final
class COUNT(_SQLFunction):
    """Count all rows, non-NULL values, or distinct tuples of non-NULL values."""
    def __init__(self, *values: object, distinct: bool = False) -> None: ...

_SortKey = tuple[Column, str]

@final
class GROUP_CONCAT(_SQLFunction):
    """Concatenate non-NULL values with an optional explicit order and separator."""
    def __init__(
        self, value: object, *values: object, distinct: bool = False,
        order_by: list[_SortKey] | tuple[_SortKey, ...] | None = None,
        separator: str | None = None,
    ) -> None: ...

@final
class FIRST_VALUE(_SQLFunction):
    """Return the value for the first row of the window frame."""
    def __init__(self, value: object, *, null_treatment: str | None = None) -> None: ...

@final
class LAST_VALUE(_SQLFunction):
    """Return the value for the last row of the window frame."""
    def __init__(self, value: object, *, null_treatment: str | None = None) -> None: ...

@final
class LAG(_SQLFunction):
    """Return the value a given number of rows before the current row."""
    def __init__(
        self, value: object, offset: int = ..., default: object = ..., *, null_treatment: str | None = None,
    ) -> None: ...

@final
class LEAD(_SQLFunction):
    """Return the value a given number of rows after the current row."""
    def __init__(
        self, value: object, offset: int = ..., default: object = ..., *, null_treatment: str | None = None,
    ) -> None: ...

@final
class NTH_VALUE(_SQLFunction):
    """Return the value at a positive position within the window frame."""
    def __init__(
        self, value: object, position: int, *, from_position: str | None = None,
        null_treatment: str | None = None,
    ) -> None: ...

@final
class NTILE(_SQLFunction):
    """Divide the partition into a positive number of ordered buckets."""
    def __init__(self, buckets: int) -> None: ...

@final
class ANY_VALUE(_SQLFunction):
    """Return a value without an ONLY_FULL_GROUP_BY check."""
    def __init__(self, value: object) -> None: ...

@final
class BIT_AND(_SQLFunction):
    """Return the bitwise AND over the values in a group."""
    def __init__(self, value: object) -> None: ...

@final
class BIT_OR(_SQLFunction):
    """Return the bitwise OR over the values in a group."""
    def __init__(self, value: object) -> None: ...

@final
class BIT_XOR(_SQLFunction):
    """Return the bitwise XOR over the values in a group."""
    def __init__(self, value: object) -> None: ...

@final
class GROUPING(_SQLFunction):
    """Return a bit mask identifying rollup values for the given grouping expressions."""
    def __init__(self, value: object, *values: object) -> None: ...

@final
class JSON_ARRAYAGG(_SQLFunction):
    """Aggregate the group values into one JSON array."""
    def __init__(self, value: object) -> None: ...

@final
class JSON_OBJECTAGG(_SQLFunction):
    """Aggregate keys and values into one JSON object."""
    def __init__(self, key: object, value: object) -> None: ...

@final
class STDDEV_POP(_SQLFunction):
    """Return the population standard deviation of the group."""
    def __init__(self, value: object) -> None: ...

@final
class STDDEV_SAMP(_SQLFunction):
    """Return the sample standard deviation of the group."""
    def __init__(self, value: object) -> None: ...

@final
class VAR_POP(_SQLFunction):
    """Return the population variance of the group."""
    def __init__(self, value: object) -> None: ...

@final
class VAR_SAMP(_SQLFunction):
    """Return the sample variance of the group."""
    def __init__(self, value: object) -> None: ...

@final
class CUME_DIST(_SQLFunction):
    """Return the current row's cumulative distribution within the partition."""
    def __init__(self) -> None: ...

@final
class DENSE_RANK(_SQLFunction):
    """Return the peer-group rank without gaps."""
    def __init__(self) -> None: ...

@final
class PERCENT_RANK(_SQLFunction):
    """Return the current row's relative rank within its partition."""
    def __init__(self) -> None: ...

@final
class RANK(_SQLFunction):
    """Return the rank of the current row with gaps for peers."""
    def __init__(self) -> None: ...

@final
class ROW_NUMBER(_SQLFunction):
    """Return the sequential number of the current row in its partition."""
    def __init__(self) -> None: ...
