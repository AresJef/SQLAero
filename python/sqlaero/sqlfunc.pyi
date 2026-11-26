"""Concrete MySQL function expression classes."""

from typing import final
from ._sqlaero import _SQLFunction, _SQLInterval  # pyright: ignore[reportPrivateUsage]

__all__: list[str]

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
