"""Static input families without importing optional providers at runtime.

The small protocols describe public surfaces shared by supported provider
families. They do not authorize arbitrary duck types: Escape still checks real
class identity, dtype, dimensions and recursive values at runtime. Those rules
cannot all be expressed as a Python type alias.
"""

from collections.abc import ItemsView, KeysView, ValuesView
from datetime import date, time, timedelta
from decimal import Decimal
from os import PathLike
from time import struct_time
from typing import Any, Literal, Protocol, TypeAlias

class _ArrayInput(Protocol):
    """NumPy values and Pandas one-dimensional public storage surfaces."""
    @property
    def dtype(self) -> object: ...
    @property
    def ndim(self) -> int: ...
    @property
    def shape(self) -> tuple[int, ...]: ...

class _FrameInput(Protocol):
    """Pandas and Polars DataFrame public column/shape surfaces."""
    @property
    def columns(self) -> object: ...
    @property
    def dtypes(self) -> object: ...
    @property
    def shape(self) -> tuple[int, ...]: ...

class _PolarsSeriesInput(Protocol):
    @property
    def dtype(self) -> object: ...
    @property
    def shape(self) -> tuple[int, ...]: ...
    def n_chunks(self) -> int: ...

class _ArrowArrayInput(Protocol):
    """Arrow Array and ChunkedArray; individual Arrow scalars lack null_count."""
    @property
    def type(self) -> object: ...
    @property
    def null_count(self) -> int: ...
    def __len__(self) -> int: ...

class _ArrowTableInput(Protocol):
    @property
    def schema(self) -> object: ...
    @property
    def num_rows(self) -> int: ...
    @property
    def num_columns(self) -> int: ...

class _PandasMissingInput(Protocol):
    """Pandas' public stubs describe NA's stable singleton pickle name."""
    def __reduce__(self) -> Literal["NA"]: ...

class _PandasMissingTimeInput(Protocol):
    """NaT and Timestamp expose the same native datetime conversion surface."""
    @property
    def value(self) -> object: ...
    def to_datetime64(self) -> object: ...

# Any is confined to elements of already accepted container families. It does
# not become a top-level member of the union or erase unsupported root errors.
EscapeInput: TypeAlias = (
    None | bool | int | float | str | bytes | Decimal | date | time | timedelta | struct_time
    | list[Any] | tuple[object, ...] | dict[Any, Any] | set[Any] | frozenset[object] | range
    | KeysView[object] | ValuesView[object] | ItemsView[object, object]
    | _ArrayInput | _FrameInput | _PolarsSeriesInput | _ArrowArrayInput | _ArrowTableInput
    | _PandasMissingInput | _PandasMissingTimeInput
)
PathInput: TypeAlias = str | PathLike[str] | PathLike[bytes]
ErrorTypesInput: TypeAlias = type[Exception] | tuple[type[Exception], ...] | list[type[Exception]]
ErrorCodesInput: TypeAlias = int | tuple[int, ...] | list[int]

__all__: list[str]
