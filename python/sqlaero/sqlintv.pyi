"""Named MySQL temporal interval constructors."""

from typing import final
from ._sqlaero import _SQLInterval  # pyright: ignore[reportPrivateUsage]

__all__: list[str]

@final
class YEAR(_SQLInterval):
    """Create a MySQL interval measured in calendar years."""
    def __init__(self, years: object) -> None: ...

@final
class QUARTER(_SQLInterval):
    """Create a MySQL interval measured in calendar quarters."""
    def __init__(self, quarters: object) -> None: ...

@final
class MONTH(_SQLInterval):
    """Create a MySQL interval measured in calendar months."""
    def __init__(self, months: object) -> None: ...

@final
class WEEK(_SQLInterval):
    """Create a MySQL interval measured in weeks."""
    def __init__(self, weeks: object) -> None: ...

@final
class DAY(_SQLInterval):
    """Create a MySQL interval measured in days."""
    def __init__(self, days: object) -> None: ...

@final
class HOUR(_SQLInterval):
    """Create a MySQL interval measured in hours."""
    def __init__(self, hours: object) -> None: ...

@final
class MINUTE(_SQLInterval):
    """Create a MySQL interval measured in minutes."""
    def __init__(self, minutes: object) -> None: ...

@final
class SECOND(_SQLInterval):
    """Create a MySQL interval measured in seconds."""
    def __init__(self, seconds: object) -> None: ...

@final
class MICROSECOND(_SQLInterval):
    """Create a MySQL interval measured in microseconds."""
    def __init__(self, microseconds: object) -> None: ...

@final
class YEAR_MONTH(_SQLInterval):
    """Create a calendar interval from signed years and months."""
    def __init__(self, years: int, months: int) -> None: ...

@final
class DAY_HOUR(_SQLInterval):
    """Create an interval from signed days and hours."""
    def __init__(self, days: int, hours: int) -> None: ...

@final
class DAY_MINUTE(_SQLInterval):
    """Create an interval from signed days, hours, and minutes."""
    def __init__(self, days: int, hours: int, minutes: int) -> None: ...

@final
class DAY_SECOND(_SQLInterval):
    """Create an interval from signed days, hours, minutes, and seconds."""
    def __init__(self, days: int, hours: int, minutes: int, seconds: int) -> None: ...

@final
class DAY_MICROSECOND(_SQLInterval):
    """Create an interval from signed days through microseconds."""
    def __init__(self, days: int, hours: int, minutes: int, seconds: int,
                 microseconds: int) -> None: ...

@final
class HOUR_MINUTE(_SQLInterval):
    """Create an interval from signed hours and minutes."""
    def __init__(self, hours: int, minutes: int) -> None: ...

@final
class HOUR_SECOND(_SQLInterval):
    """Create an interval from signed hours, minutes, and seconds."""
    def __init__(self, hours: int, minutes: int, seconds: int) -> None: ...

@final
class HOUR_MICROSECOND(_SQLInterval):
    """Create an interval from signed hours through microseconds."""
    def __init__(self, hours: int, minutes: int, seconds: int, microseconds: int) -> None: ...

@final
class MINUTE_SECOND(_SQLInterval):
    """Create an interval from signed minutes and seconds."""
    def __init__(self, minutes: int, seconds: int) -> None: ...

@final
class MINUTE_MICROSECOND(_SQLInterval):
    """Create an interval from signed minutes, seconds, and microseconds."""
    def __init__(self, minutes: int, seconds: int, microseconds: int) -> None: ...

@final
class SECOND_MICROSECOND(_SQLInterval):
    """Create an interval from signed seconds and microseconds."""
    def __init__(self, seconds: int, microseconds: int) -> None: ...
