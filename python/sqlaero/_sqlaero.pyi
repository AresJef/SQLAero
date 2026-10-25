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
                   limit: int = 67108864) -> bytes:
    """Exercise the real native scalar reader and SQL writer."""
    ...
