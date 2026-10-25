"""Private native extension declarations."""

from typing import Any, Iterator, overload

class ColumnInfo:
    """Immutable metadata for one column at its original result position."""
    @property
    def index(self) -> int:
        """Zero-based original column position."""
        ...
    @property
    def key(self) -> str:
        """Unique exact lookup key; preparing it preserves original labels."""
        ...
    @property
    def name(self) -> str:
        """Actual query label including AS aliases and duplicates."""
        ...
    @property
    def original_name(self) -> str | None:
        """Original source column name when reported."""
        ...
    @property
    def table(self) -> str | None:
        """Query-visible source table or alias when reported."""
        ...
    @property
    def original_table(self) -> str | None:
        """Original physical table when reported."""
        ...
    @property
    def database(self) -> str | None:
        """Reported source schema or database."""
        ...
    @property
    def type_code(self) -> int:
        """Actual MySQL protocol field type, including unknown codes."""
        ...
    @property
    def type_name(self) -> str | None:
        """Known protocol type name, without reconstructing source DDL."""
        ...
    @property
    def column_length(self) -> int:
        """Server-declared display bytes; not characters or actual row size."""
        ...
    @property
    def precision(self) -> int | None:
        """Reliable Decimal precision, otherwise None."""
        ...
    @property
    def scale(self) -> int | None:
        """Applicable numeric fractional digits, otherwise None."""
        ...
    @property
    def is_unsigned(self) -> bool:
        """Whether UNSIGNED_FLAG applies to this numeric type."""
        ...
    @property
    def is_zerofill(self) -> bool:
        """Whether ZEROFILL_FLAG applies to this numeric type."""
        ...
    @property
    def fractional_seconds_precision(self) -> int | None:
        """Declared temporal fractional digits, or None."""
        ...
    @property
    def collation_id(self) -> int:
        """Actual protocol character_set/collation ID."""
        ...
    @property
    def collation_name(self) -> str | None:
        """Catalog collation name, or None for an unknown ID."""
        ...
    @property
    def charset_name(self) -> str | None:
        """Catalog charset name for this collation, or None."""
        ...
    @property
    def is_binary_string(self) -> bool:
        """Binary-encoded string family; textual _bin collations stay text."""
        ...
    @property
    def is_enum(self) -> bool:
        """Whether the actual protocol type or flags report ENUM."""
        ...
    @property
    def is_set(self) -> bool:
        """Whether the actual protocol type or flags report SET."""
        ...
    @property
    def is_nullable(self) -> bool:
        """Inverse of the server's NOT_NULL_FLAG."""
        ...
    @property
    def is_auto_increment(self) -> bool:
        """Whether AUTO_INCREMENT_FLAG is set."""
        ...
    @property
    def is_primary_key_part(self) -> bool:
        """Reported primary-key membership."""
        ...
    @property
    def is_unique_key_part(self) -> bool:
        """Reported unique-index membership, not per-column uniqueness."""
        ...
    @property
    def is_non_unique_key_part(self) -> bool:
        """Reported ordinary-index membership."""
        ...
    @property
    def flags(self) -> int:
        """Actual raw MySQL column flag bits."""
        ...
    @property
    def decimals(self) -> int:
        """Actual raw decimals/fractional-second byte."""
        ...
    @property
    def catalog(self) -> str:
        """Validated protocol catalog, def."""
        ...

class Columns:
    """Immutable ordered column metadata with exact unique-key lookup."""
    def __len__(self) -> int: ...
    def __iter__(self) -> Iterator[ColumnInfo]: ...
    @overload
    def __getitem__(self, key: int | str) -> ColumnInfo: ...
    @overload
    def __getitem__(self, key: slice) -> tuple[ColumnInfo, ...]: ...
    @property
    def names(self) -> tuple[str, ...]:
        """Ordered actual labels, preserving duplicates; cached once."""
        ...
    @property
    def keys(self) -> tuple[str, ...]:
        """Ordered unique access labels, generated only when requested."""
        ...

Description = tuple[tuple[str, int, None, None, int | None, int | None, bool], ...]

class SQLAeroError(Exception):
    """Native root exception; the public errors module is not yet complete."""
    def __init__(self, message: str = '', *, code: int | None = None,
                 is_server_error: bool | None = None) -> None: ...
    @property
    def message(self) -> str: ...
    @property
    def code(self) -> int | None: ...
    @property
    def is_server_error(self) -> bool | None: ...

class InterfaceError(SQLAeroError):
    """Invalid client interface or session state."""
class InterfaceBusyError(InterfaceError):
    """An operation already owns the connection or cursor."""
class DatabaseError(SQLAeroError):
    """Base class for database and data-processing failures."""
class DataError(DatabaseError):
    """A supported value violates the data representation contract."""
class OperationalError(DatabaseError):
    """A transport, resource or operational failure."""
class OperationalTimeoutError(OperationalError):
    """A declared connection, operation or network deadline expired."""
class IntegrityError(DatabaseError):
    """A database integrity constraint was violated."""
class InternalError(DatabaseError):
    """An actual internal driver invariant failed."""
class ProgrammingError(DatabaseError):
    """A SQL command or binding contract was rejected."""
class NotSupportedError(DatabaseError):
    """An explicitly unsupported capability was requested."""
class SQLAeroWarning(Warning):
    """SQLAero's independent warning category."""

def _parse_size(value: str) -> int:
    """Parse a textual byte count using the shared native configuration parser."""
    ...
def _decode_json(data: bytes) -> Any:
    """Decode one complete JSON value with strict native numeric checks."""
    ...
def _encode_scalar(data: object, *, utc_confirmed: bool = True,
                   limit: int = 67108864, charset: str | None = None,
                   no_backslash: bool = False) -> bytes:
    """Exercise the real native scalar reader and SQL writer."""
    ...
def _decode_text(data: bytes, *, charset: str = "utf8mb4") -> str:
    """Exercise the native strict byte-to-Unicode conversion route."""
    ...
def _bind_sql(sql: str, args: object, *, many: bool = False,
              itemize: bool = True, charset: str | None = None,
              no_backslash: bool = False, ansi_quotes: bool = False,
              timeout_ns: int | None = None, limit: int = 67108864) -> list[bytes]:
    """Return actual COM_QUERY payloads from the incremental native binder."""
    ...

def _decode_field(type_code: int, data: bytes | None, *, collation_id: int = 255,
                  flags: int = 0, length: int = 0, decode_json: bool = False,
                  decode_set: bool = False, decode_bit: bool = False,
                  use_decimal: bool = True, use_timedelta: bool = True,
                  decode_text_strict: bool = True, decode_date_strict: bool = True) -> object:
    """Exercise the prepared native output action for real column metadata."""
    ...

class _Rows:
    """Private production buffered read component; Cursor embeds it in Rust."""
    def fetchone(self) -> tuple[object, ...] | None: ...
    def fetchmany(self, size: int | None = None) -> tuple[tuple[object, ...], ...]: ...
    def fetchall(self) -> tuple[tuple[object, ...], ...]: ...
    def close(self) -> None: ...
    def __iter__(self) -> _Rows: ...
    def __next__(self) -> tuple[object, ...]: ...
    @property
    def position(self) -> int: ...
    @property
    def closed(self) -> bool: ...
    @property
    def names(self) -> tuple[str, ...]: ...
    @property
    def columns(self) -> Columns: ...
    @property
    def description(self) -> Description | None: ...

def _rows_from_packets(packets: list[bytes], *, name_collation_id: int = 255,
                      decode_json: bool = False, decode_set: bool = False,
                      decode_bit: bool = False, use_decimal: bool = True,
                      use_timedelta: bool = True, decode_text_strict: bool = True,
                      decode_date_strict: bool = True) -> _Rows:
    """Load complete protocol packets into the same native buffered reader."""
    ...
