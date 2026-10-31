"""Public input aliases; runtime validation remains at the actual API boundary."""

from os import PathLike
from typing import TypeAlias

# Escape owns canonical provider recognition and recursive value validation.
# `object` keeps optional NumPy/Pandas/Arrow/Polars values usable without making
# those packages mandatory imports or accepting arbitrary duck types at runtime.
# The supported families, dtype whitelist and shape rules are documented by
# Cursor.execute; bytearray and memoryview remain rejected at that boundary.
EscapeInput: TypeAlias = object
PathInput: TypeAlias = str | PathLike[str] | PathLike[bytes]
ErrorTypesInput: TypeAlias = type[Exception] | tuple[type[Exception], ...] | list[type[Exception]]
ErrorCodesInput: TypeAlias = int | tuple[int, ...] | list[int]

__all__: list[str] = ['EscapeInput', 'PathInput', 'ErrorTypesInput', 'ErrorCodesInput']
