"""Private native extension declarations."""

from typing import Any

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

def _rows_from_packets(packets: list[bytes], *, name_collation_id: int = 255,
                      decode_json: bool = False, decode_set: bool = False,
                      decode_bit: bool = False, use_decimal: bool = True,
                      use_timedelta: bool = True, decode_text_strict: bool = True,
                      decode_date_strict: bool = True) -> _Rows:
    """Load complete protocol packets into the same native buffered reader."""
    ...
