"""Public input aliases; runtime validation remains at the actual API boundary."""

from os import PathLike
from typing import TypeAlias

# The adjacent typing.pyi supplies the precise static family union. This
# annotation-only runtime placeholder avoids importing optional providers.
# It is not an isinstance predicate or permission to pass arbitrary objects;
# Escape owns real identity, dtype, shape and recursive-value validation.
EscapeInput: TypeAlias = object
PathInput: TypeAlias = str | PathLike[str] | PathLike[bytes]
ErrorTypesInput: TypeAlias = type[Exception] | tuple[type[Exception], ...] | list[type[Exception]]
ErrorCodesInput: TypeAlias = int | tuple[int, ...] | list[int]

__all__: list[str] = ['EscapeInput', 'PathInput', 'ErrorTypesInput', 'ErrorCodesInput']
