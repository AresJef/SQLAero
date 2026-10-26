"""Private native extension declarations."""

from typing import Any, Iterator, overload
from .error_codes import SQLAeroErrorCode as SQLAeroErrorCode, MySQLErrorCode as MySQLErrorCode

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
    """Base exception with immutable message/code/source and lazy diagnostics."""
    def __init__(self, message: str = '', *, code: int | None = None,
                 is_server_error: bool | None = None) -> None: ...
    @property
    def message(self) -> str: ...
    @property
    def code(self) -> int | None: ...
    @property
    def is_server_error(self) -> bool | None: ...
    @property
    def error_info(self) -> ErrorInfo:
        """Return a cached immutable snapshot; library-added facts refresh it.

        Previously returned snapshots remain unchanged. No server request is sent.
        """
        ...

class ErrorInfo:
    """Read-only captured error facts; not an exception and not constructible."""
    @property
    def stage(self) -> str | None:
        """Actual handling stage, or None when unrecorded."""
        ...
    @property
    def exception_type(self) -> type[BaseException] | None:
        """Actual Python class; None for a pure Rust source node."""
        ...
    @property
    def native_type(self) -> str | None:
        """Reliable native type name, not a stable matching key."""
        ...
    @property
    def message(self) -> str:
        """Frozen original node message."""
        ...
    @property
    def code(self) -> int | None:
        """Actual library code belonging to this node alone."""
        ...
    @property
    def os_errno(self) -> int | None:
        """Actual OS number of this node, never inferred from text."""
        ...
    @property
    def is_server_error(self) -> bool | None:
        """Declared library origin; external/native nodes may be None."""
        ...
    @property
    def cause(self) -> ErrorInfo | None:
        """Actual explicit/native cause, preserving shared and cyclic edges."""
        ...
    @property
    def context(self) -> ErrorInfo | None:
        """Actual Python background, even when suppressed from display."""
        ...
    @property
    def suppress_context(self) -> bool:
        """Original Python implicit-context display suppression."""
        ...
    @property
    def traceback(self) -> str | None:
        """Lazily formatted captured stack/chain; no live frames are retained."""
        ...
    @property
    def notes(self) -> tuple[str, ...]:
        """Frozen ordered notes, unaffected by later exception changes."""
        ...
    @property
    def additional_errors(self) -> tuple[ErrorInfo, ...]:
        """Independent later failures in actual order, cached as a tuple."""
        ...

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
class _EncodedValue:
    """Own native diagnostic output without constructing Python SQL text."""
    def __len__(self) -> int: ...
    def to_bytes(self) -> bytes:
        """Copy bytes for an untimed output check."""
        ...
def _encode_value(data: object, *, no_backslash: bool = False) -> _EncodedValue:
    """Measure the production value visitor; SQL binding is a separate task."""
    ...
class _ValueEncoder:
    """Retain prepared type and charset state for the production value visitor."""
    def encode(self, data: object) -> _EncodedValue: ...
def _prepare_encoder(*, no_backslash: bool = False) -> _ValueEncoder:
    """Prepare the same immutable context used outside production traversal."""
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

from .typing import EscapeInput as EscapeInput, PathInput as PathInput, ErrorTypesInput, ErrorCodesInput
from types import TracebackType

class SSL:
    """Immutable native TLS policy and validated reusable PEM materials."""
    def __init__(self, *, mode: str | None = None, min_tls_version: str | None = None,
                 ca_file: PathInput | None = None, cert_file: PathInput | None = None,
                 key_file: PathInput | None = None, key_password: str | None = None,
                 server_name: str | None = None) -> None: ...

class Connection:
    """Fully initialized native sync connection, created by connect()."""
    def cursor(self, cursor: type[Cursor] | type[tuple[Any, ...]] | None = None) -> Cursor: ...
    def transaction(self, cursor: type[Cursor] | type[tuple[Any, ...]] | None = None) -> _Transaction: ...
    def close(self) -> None: ...
    def begin(self) -> None: ...
    def commit(self) -> None: ...
    def rollback(self) -> None: ...
    def set_autocommit(self, enabled: bool) -> None: ...
    def ping(self) -> bool: ...
    def __enter__(self) -> Connection: ...
    def __exit__(self, typ: object, value: object, traceback: object) -> bool: ...
    @property
    def closed(self) -> bool:
        """Irreversible local retirement, not server transaction confirmation."""
        ...
    @property
    def server_version(self) -> str:
        """Actual handshake version, still readable after close."""
        ...
    @property
    def connection_id(self) -> int:
        """Actual server connection identity, still readable after close."""
        ...
    @property
    def ssl_mode(self) -> str:
        """Resolved TLS policy, still readable after close."""
        ...
    @property
    def tls_version(self) -> str | None:
        """Actual negotiated TLS version, or None for plain transport."""
        ...
    @property
    def net_read_timeout(self) -> int:
        """MySQL server-side SESSION whole seconds for waiting for more client upload data.

        connect(None) preserves and confirms the server default, not disables it.
        Cached reads send no SQL and remain readable after close. An unknown fact
        raises InterfaceError instead of returning stale data."""
        ...
    def set_net_read_timeout(self, value: int) -> None:
        """Set MySQL server-side SESSION net_read_timeout in positive whole seconds.

        Only int excluding bool; None/bad types raise TypeError, zero/negative raise
        ValueError. MySQL decides upper limits/normalization; actual confirmed values
        are cached. A change sends SET under the operation deadline. Reliable same
        values still check lifecycle/ownership, then send no SQL. Closed/busy calls
        raise InterfaceError/InterfaceBusyError; success returns None."""
        ...
    @property
    def net_write_timeout(self) -> int:
        """MySQL server-side SESSION whole seconds for writing result data to this client.

        connect(None) preserves and confirms the server default, not disables it.
        Cached reads send no SQL and remain readable after close. An unknown fact
        raises InterfaceError instead of returning stale data."""
        ...
    def set_net_write_timeout(self, value: int) -> None:
        """Set MySQL server-side SESSION net_write_timeout in positive whole seconds.

        Only int excluding bool; None/bad types raise TypeError, zero/negative raise
        ValueError. MySQL decides upper limits/normalization; actual confirmed values
        are cached. A change sends SET under the operation deadline. Reliable same
        values still check lifecycle/ownership, then send no SQL. Closed/busy calls
        raise InterfaceError/InterfaceBusyError; success returns None."""
        ...
    @property
    def wait_timeout(self) -> int:
        """MySQL server-side SESSION whole seconds for session idle time.

        connect(None) preserves and confirms the server default, not disables it.
        Cached reads send no SQL and remain readable after close. An unknown fact
        raises InterfaceError instead of returning stale data."""
        ...
    def set_wait_timeout(self, value: int) -> None:
        """Set MySQL server-side SESSION wait_timeout in positive whole seconds.

        Only int excluding bool; None/bad types raise TypeError, zero/negative raise
        ValueError. MySQL decides upper limits/normalization; actual confirmed values
        are cached. A change sends SET under the operation deadline. Reliable same
        values still check lifecycle/ownership, then send no SQL. Closed/busy calls
        raise InterfaceError/InterfaceBusyError; success returns None."""
        ...
    @property
    def innodb_lock_wait_timeout(self) -> int:
        """MySQL server-side SESSION whole seconds for an InnoDB row lock, not a metadata lock.

        connect(None) preserves and confirms the server default, not disables it.
        Cached reads send no SQL and remain readable after close. An unknown fact
        raises InterfaceError instead of returning stale data."""
        ...
    def set_innodb_lock_wait_timeout(self, value: int) -> None:
        """Set MySQL server-side SESSION innodb_lock_wait_timeout in positive whole seconds.

        Only int excluding bool; None/bad types raise TypeError, zero/negative raise
        ValueError. MySQL decides upper limits/normalization; actual confirmed values
        are cached. A change sends SET under the operation deadline. Reliable same
        values still check lifecycle/ownership, then send no SQL. Closed/busy calls
        raise InterfaceError/InterfaceBusyError; success returns None."""
        ...
    @property
    def autocommit(self) -> bool:
        """Last reliable MySQL autocommit fact; unknown raises InterfaceError."""
        ...
    @property
    def connect_timeout(self) -> int | None:
        """Client-side whole-call establishment seconds; default 10, None disables.

        Positive int excluding bool; bad types/values raise TypeError/ValueError.
        Covers preparation, dialing, authentication and initialization; no extra
        network cleanup grace. Read-only after creation, readable after close."""
        ...
    @property
    def read_timeout(self) -> int | None:
        """Client-side maximum network read wait without progress, in whole seconds.

        Default None disables; positive int or None, excluding bool. Bad type/value
        raises TypeError/ValueError. Actual progress restarts this interval; readiness
        does not. Local assignment sends no SQL and affects subsequent operations.
        Not a total-response timer. Readable after close; modification then fails.
        Expiry does not establish SQL cancellation or confirmed rollback."""
        ...
    @read_timeout.setter
    def read_timeout(self, value: int | None) -> None: ...
    @property
    def write_timeout(self) -> int | None:
        """Client-side maximum network write wait without progress, in whole seconds.

        Default None disables; positive int or None, excluding bool. Bad type/value
        raises TypeError/ValueError. Actual progress restarts this interval, not the
        total upload/operation deadline. Local assignment affects subsequent commands
        and sends no SQL. Saved value is readable after close; modification fails.
        Expiry does not establish SQL cancellation or confirmed rollback."""
        ...
    @write_timeout.setter
    def write_timeout(self, value: int | None) -> None: ...
    @property
    def operation_timeout(self) -> int | None:
        """Client-side total whole-second limit shared by an operation's batches.

        Default None disables; positive int or None excluding bool, with
        TypeError/ValueError on invalid type/value. Covers binding, necessary internal
        confirmation, sending, complete reception and state/result publication;
        progress does not reset it. Does not time buffered fetch or transaction-body
        user code. Assignment affects new operations without SQL; readable after
        close, modification fails. No new network cleanup grace or implied rollback."""
        ...
    @operation_timeout.setter
    def operation_timeout(self, value: int | None) -> None: ...
    @property
    def max_allowed_packet(self) -> int:
        """Configured client logical-send byte limit, readable after close."""
        ...
    @property
    def max_received_packet(self) -> int:
        """Configured client logical-receive byte limit, readable after close."""
        ...
    @property
    def result_memory_limit(self) -> int:
        """Per-result native capacity limit in bytes, readable after close."""
        ...
    @property
    def total_result_memory_limit(self) -> int:
        """Connection aggregate native result limit in bytes, readable after close."""
        ...
    @property
    def decode_json(self) -> bool:
        """Native JSON decoding; default False, copied only to new Cursors."""
        ...
    @decode_json.setter
    def decode_json(self, value: bool) -> None: ...
    @property
    def decode_set(self) -> bool:
        """SET tuple decoding; default False, copied only to new Cursors."""
        ...
    @decode_set.setter
    def decode_set(self, value: bool) -> None: ...
    @property
    def decode_bit(self) -> bool:
        """BIT integer decoding; default False, copied only to new Cursors."""
        ...
    @decode_bit.setter
    def decode_bit(self, value: bool) -> None: ...
    @property
    def use_decimal(self) -> bool:
        """Exact Decimal output; default True, copied only to new Cursors."""
        ...
    @use_decimal.setter
    def use_decimal(self, value: bool) -> None: ...
    @property
    def use_timedelta(self) -> bool:
        """TIME duration output; default True, copied only to new Cursors."""
        ...
    @use_timedelta.setter
    def use_timedelta(self, value: bool) -> None: ...
    @property
    def decode_text_strict(self) -> bool:
        """Strict text decoding; default True, copied only to new Cursors."""
        ...
    @decode_text_strict.setter
    def decode_text_strict(self, value: bool) -> None: ...
    @property
    def decode_date_strict(self) -> bool:
        """Strict date decoding; default True, copied only to new Cursors."""
        ...
    @decode_date_strict.setter
    def decode_date_strict(self, value: bool) -> None: ...

class Cursor:
    """Independent buffered results, created by Connection.cursor()."""
    def execute(self, sql: str, args: EscapeInput = ..., *, many: bool = False,
                itemize: bool = True) -> int: ...
    def executemany(self, sql: str, args: EscapeInput) -> int: ...
    def fetchone(self) -> tuple[object, ...] | None: ...
    def fetchmany(self, size: int | None = None) -> tuple[tuple[object, ...], ...]: ...
    def fetchall(self) -> tuple[tuple[object, ...], ...]: ...
    def close(self) -> None: ...
    def __iter__(self) -> Cursor: ...
    def __next__(self) -> tuple[object, ...]: ...
    def __enter__(self) -> Cursor: ...
    def __exit__(self, typ: object, value: object, traceback: object) -> bool: ...
    @property
    def closed(self) -> bool:
        """Whether this Cursor itself has been permanently retired."""
        ...
    @property
    def arraysize(self) -> int:
        """Positive local default fetchmany count; initial value 1."""
        ...
    @arraysize.setter
    def arraysize(self, value: int) -> None: ...
    @property
    def rowcount(self) -> int:
        """Actual row count, or PEP 249's -1 when unknown; readable after close."""
        ...
    @property
    def row_count(self) -> int | None:
        """Actual row count, or None when unknown; readable after close."""
        ...
    @property
    def affected_rows(self) -> int | None:
        """Alias of row_count; readable after close."""
        ...
    @property
    def lastrowid(self) -> int | None:
        """Actual final insert ID, or None; readable after close."""
        ...
    @property
    def last_row_id(self) -> int | None:
        """Alias of lastrowid; readable after close."""
        ...
    @property
    def warning_count(self) -> int | None:
        """Sum of actual completed-command warnings, or None when unknown."""
        ...
    @property
    def columns(self) -> Columns:
        """Immutable execution-time metadata, available lazily after close."""
        ...
    @property
    def description(self) -> Description | None:
        """PEP 249 metadata, or None without a current row result."""
        ...
    @property
    def decode_json(self) -> bool:
        """Native JSON decoding; changes apply only to new executions."""
        ...
    @decode_json.setter
    def decode_json(self, value: bool) -> None: ...
    @property
    def decode_set(self) -> bool:
        """SET tuple decoding; changes apply only to new executions."""
        ...
    @decode_set.setter
    def decode_set(self, value: bool) -> None: ...
    @property
    def decode_bit(self) -> bool:
        """BIT integer decoding; changes apply only to new executions."""
        ...
    @decode_bit.setter
    def decode_bit(self, value: bool) -> None: ...
    @property
    def use_decimal(self) -> bool:
        """Exact Decimal output; changes apply only to new executions."""
        ...
    @use_decimal.setter
    def use_decimal(self, value: bool) -> None: ...
    @property
    def use_timedelta(self) -> bool:
        """TIME duration output; changes apply only to new executions."""
        ...
    @use_timedelta.setter
    def use_timedelta(self, value: bool) -> None: ...
    @property
    def decode_text_strict(self) -> bool:
        """Strict text decoding; changes apply only to new executions."""
        ...
    @decode_text_strict.setter
    def decode_text_strict(self, value: bool) -> None: ...
    @property
    def decode_date_strict(self) -> bool:
        """Strict date decoding; changes apply only to new executions."""
        ...
    @decode_date_strict.setter
    def decode_date_strict(self, value: bool) -> None: ...

def connect(*, user: str, host: str = '127.0.0.1', port: int = 3306,
            unix_socket: PathInput | None = None, password: str | None = None,
            database: str | None = None, charset: str | None = None,
            collation: str | None = None, ssl: SSL | None = None,
            server_public_key_path: PathInput | None = None, autocommit: bool = True,
            sql_mode: str | None = None, connect_timeout: int | None = 10,
            read_timeout: int | None = None, write_timeout: int | None = None,
            operation_timeout: int | None = None, net_read_timeout: int | None = None,
            net_write_timeout: int | None = None, wait_timeout: int | None = None,
            innodb_lock_wait_timeout: int | None = None, max_allowed_packet: int | str = '64MB',
            max_received_packet: int | str = '64MB', result_memory_limit: int | str = '256MB',
            total_result_memory_limit: int | str = '512MB', decode_json: bool = False,
            decode_set: bool = False, decode_bit: bool = False, use_decimal: bool = True,
            use_timedelta: bool = True, decode_text_strict: bool = True,
            decode_date_strict: bool = True) -> Connection:
    """Create and fully initialize a native sync MySQL connection.

    Client connect_timeout defaults to 10; read/write/operation_timeout to None.
    Values are positive whole-second int excluding bool, or None to disable the
    applicable client limit. Type/value errors raise TypeError/ValueError.
    Establishment includes preparation/authentication/initialization; operation
    timeout covers later complete commands, not buffered fetch or transaction-body
    code. Progress restarts only read/write waits. No extra network cleanup grace
    or implied SQL cancellation/rollback follows expiry.

    MySQL server SESSION net_read_timeout (upload), net_write_timeout (download),
    wait_timeout (idle) and innodb_lock_wait_timeout (row lock) default to None:
    preserve then confirm their actual server values. Supplied positive int values
    are initialized together where possible; MySQL decides upper normalization.
    Later remote changes use explicit set_* methods; local runtime timeouts use
    property assignment. See each property's precise lifecycle/unknown semantics.
    """
    ...

class _Transaction:
    """Private managed scope returned by Connection.transaction()."""
    def __enter__(self) -> Cursor: ...
    def __exit__(self, typ: object, value: object, traceback: object) -> bool: ...


class RetryRule:
    """Read-only AND condition; separate rules are ORed by Retry."""
    def __init__(self, *, error_types: ErrorTypesInput | None = None,
                 error_codes: ErrorCodesInput | None = None) -> None: ...
    @property
    def error_types(self) -> tuple[type[Exception], ...] | None:
        """Normalized classes, or None for no type constraint."""
        ...
    @property
    def error_codes(self) -> tuple[int, ...] | None:
        """Normalized plain integer codes, or None for no code constraint."""
        ...

class Retry:
    """Reusable read-only retry configuration; traversals have independent state."""
    def __init__(self, *rules: RetryRule, max_attempts: int | None = 3,
                 wait_interval: int | float = 1.0, wait_multiplier: int | float = 1.0,
                 max_wait_interval: int | float = 10.0, wait_jitter: bool = False) -> None: ...
    def __iter__(self) -> Iterator[RetryAttempt]: ...
    @property
    def rules(self) -> tuple[RetryRule, ...]:
        """Immutable ORed rule tuple."""
        ...
    @property
    def max_attempts(self) -> int | None:
        """Attempts including the first; None is unlimited."""
        ...
    @property
    def wait_interval(self) -> int | float:
        """Nonnegative initial seconds; first attempt does not wait."""
        ...
    @property
    def wait_multiplier(self) -> int | float:
        """Finite multiplier >= 1; jitter does not change the base sequence."""
        ...
    @property
    def max_wait_interval(self) -> int | float:
        """Mandatory finite cap, applied to every retry delay."""
        ...
    @property
    def wait_jitter(self) -> bool:
        """Whether actual delays are uniform from zero through capped base."""
        ...

class RetryAttempt:
    """Context yielded by Retry; no public constructor is provided."""
    def __enter__(self) -> RetryAttempt: ...
    def __exit__(self, exc_type: type[BaseException] | None,
                 exc: BaseException | None, traceback: TracebackType | None) -> bool: ...
