"""Public input aliases; runtime validation remains at the actual API boundary."""

from datetime import date, datetime, time, timedelta
from decimal import Decimal
from os import PathLike
from typing import Any, TypeAlias

_EscapeScalar: TypeAlias = (
    None | bool | int | float | Decimal | str | bytes
    | date | datetime | time | timedelta
)
# Mutable list invariance makes a fully recursive public union reject ordinary
# list[int] arguments. Elements are validated against exact types at runtime.
EscapeInput: TypeAlias = _EscapeScalar | list[Any] | tuple[Any, ...]
PathInput: TypeAlias = str | PathLike[str] | PathLike[bytes]
ErrorTypesInput: TypeAlias = type[Exception] | tuple[type[Exception], ...] | list[type[Exception]]
ErrorCodesInput: TypeAlias = int | tuple[int, ...] | list[int]

__all__: list[str] = ['EscapeInput', 'PathInput', 'ErrorTypesInput', 'ErrorCodesInput']
