"""Concrete MySQL function expression classes."""

from typing import final, overload
from ._sqlaero import _SQLFunction, _SQLInterval  # pyright: ignore[reportPrivateUsage]
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
