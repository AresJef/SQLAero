"""Private native extension declarations."""

from collections.abc import Mapping
from datetime import datetime
from typing import Any, Generic, Iterator, Literal, Never, Self, SupportsIndex, TypeAlias, TypeVar, final, overload
from .error_codes import SQLAeroErrorCode as SQLAeroErrorCode, MySQLErrorCode as MySQLErrorCode

@final
class ColumnInfo:
    """Immutable metadata for one column at its original result position."""
    def __new__(cls, _factory_only: Never, /) -> Self: ...
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

@final
class Columns:
    """Immutable ordered column metadata with exact unique-key lookup."""
    def __new__(cls, _factory_only: Never, /) -> Self: ...
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

@final
class ErrorInfo:
    """Read-only captured error facts; not an exception and not constructible."""
    def __new__(cls, _factory_only: Never, /) -> Self: ...
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
    """A reliably identified internal driver or server condition failed."""
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

def _decode_binary_field(type_code: int, data: bytes | None, *, collation_id: int = 255,
                         flags: int = 0, length: int = 0, decimals: int = 0,
                         decode_json: bool = False, decode_set: bool = False,
                         decode_bit: bool = False, use_decimal: bool = True,
                         use_timedelta: bool = True, decode_text_strict: bool = True,
                         decode_date_strict: bool = True) -> object:
    """Exercise the production binary field interpreter and final output policies."""
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

@final
class SSL:
    """Immutable native TLS configuration and construction-time PEM snapshot.

    ca_file, ca_dir and ca_data are mutually exclusive. CA bundles contain only
    CERTIFICATE sections; ordinary explanation text and comments are allowed.
    ca_mode None/replace uses custom roots alone when present; append combines
    custom roots with actual system trust. Unavailable system trust does not
    silently become custom-only validation. ca_mode does not itself enable TLS.

    cert_file/cert_data and key_file/key_data are independently exclusive, and
    certificate and key must be supplied together. Their sources may be mixed.
    Data arguments are PEM str/bytes, without path or DER guessing. Encrypted
    PKCS#8 keys use key_password and the existing native identity preparation.

    Version bounds are validated even with DISABLED mode, without enabling TLS.
    Explicit custom file/directory contents are saved during construction and
    are not reloaded. System trust follows native platform semantics.
    """
    def __init__(self, *, mode: str | None = None,
                 min_tls_version: str | None = None,
                 max_tls_version: str | None = None,
                 ca_file: PathInput | None = None,
                 ca_dir: PathInput | None = None,
                 ca_data: str | bytes | None = None,
                 ca_mode: str | None = None,
                 cert_file: PathInput | None = None,
                 cert_data: str | bytes | None = None,
                 key_file: PathInput | None = None,
                 key_data: str | bytes | None = None,
                 key_password: str | None = None,
                 server_name: str | None = None) -> None: ...
    @property
    def min_tls_version(self) -> Literal["TLSv1.2", "TLSv1.3"]:
        """Normalized configured minimum; None input resolves to TLSv1.2.

        Read-only configuration, independent of any negotiated connection.
        """
        ...
    @property
    def max_tls_version(self) -> Literal["TLSv1.2", "TLSv1.3"] | None:
        """Read-only extra configured maximum, or None when no extra cap applies.

        None is not replaced with a negotiated version or a claimed TLSv1.3 cap.
        """
        ...
    def __repr__(self) -> str: ...


@final
class TLSInfo:
    """Read-only owned facts for an actual TLS connection, created internally.

    This object is not SSL configuration. It remains usable after Connection
    close or disposal; reading or displaying it performs no SQL or network I/O.
    """
    def __new__(cls, _factory_only: Never, /) -> Self: ...
    @property
    def mode(self) -> Literal["REQUIRED", "VERIFY_CA", "VERIFY_IDENTITY"]:
        """Resolved policy actually used by this TLS connection."""
        ...
    @property
    def min_version(self) -> Literal["TLSv1.2", "TLSv1.3"]:
        """Normalized configured minimum, independent of the negotiated result."""
        ...
    @property
    def max_version(self) -> Literal["TLSv1.2", "TLSv1.3"] | None:
        """Extra configured maximum, or None when no extra maximum was selected."""
        ...
    @property
    def server_name(self) -> str | None:
        """Effective native DNS/IP target, or None for an unnamed Unix target.

        The verification guarantees follow mode; REQUIRED does not authenticate
        the peer's CA or name. Unix does not borrow the TCP host as its identity.
        """
        ...
    @property
    def version(self) -> Literal["TLSv1.2", "TLSv1.3"]:
        """Actual negotiated TLS protocol version."""
        ...
    @property
    def cipher(self) -> str:
        """Actual cipher suite's canonical IANA name."""
        ...
    @property
    def cipher_id(self) -> int:
        """Exact negotiated 16-bit cipher suite number, in 0..65535."""
        ...
    @property
    def session_reused(self) -> bool:
        """True only when the native TLS engine reports a resumed handshake."""
        ...
    @property
    def peer_certificates(self) -> tuple[TLSCertificate, ...]:
        """Immutable saved peer-sent certificate chain, in original order.

        This is the peer identity, not the verifier's constructed trust path.
        Each certificate can outlive this TLSInfo and its original Connection.
        """
        ...
    def __str__(self) -> str:
        """Multiline basic facts with a certificate-count summary, without parsing details."""
        ...
    def __repr__(self) -> str:
        """Compact basic facts and certificate count, without parsing certificate details."""
        ...


@final
class TLSCertificate:
    """One independently owned, read-only peer certificate, created internally.

    Structured metadata is prepared only when a detail is requested. A content
    failure affects dependent properties; raw exports and SHA-256 remain
    independent of detail parsing. Native content failures raise DataError with
    TLS_CERTIFICATE_CONTENT; MemoryError and control exceptions keep their types.
    The original peer bytes remain usable after its TLSInfo/Connection is gone.
    """
    def __new__(cls, _factory_only: Never, /) -> Self: ...
    @property
    def subject(self) -> str:
        """Complete Subject DN with quoted, escaped decoded attribute values."""
        ...
    @property
    def issuer(self) -> str:
        """Complete Issuer DN, using the same display convention as subject."""
        ...
    @property
    def serial_number(self) -> int:
        """Exact ASN.1 serial INTEGER as an arbitrary-precision Python int."""
        ...
    @property
    def version(self) -> int:
        """Human X.509 version number; v3 is 3, independent of TLS version."""
        ...
    @property
    def not_before(self) -> datetime:
        """Absolute validity start as a timezone-aware datetime with timezone.utc."""
        ...
    @property
    def not_after(self) -> datetime:
        """Absolute validity end as a timezone-aware datetime with timezone.utc."""
        ...
    @property
    def dns_names(self) -> tuple[str, ...]:
        """SAN DNS names in original order, including duplicates and wildcard text.

        Missing DNS names give (); malformed dependent content raises DataError.
        """
        ...
    @property
    def ip_addresses(self) -> tuple[str, ...]:
        """Canonical SAN IPv4/IPv6 strings in original order, including duplicates.

        Missing IP addresses give (); malformed dependent content raises DataError.
        """
        ...
    @property
    def is_ca(self) -> bool:
        """The basicConstraints CA bit; absent is False, malformed raises DataError."""
        ...
    @property
    def signature_algorithm_oid(self) -> str:
        """Outer certificate signature algorithm's exact dotted-decimal OID."""
        ...
    @property
    def public_key_algorithm_oid(self) -> str:
        """SubjectPublicKeyInfo algorithm's exact dotted-decimal OID."""
        ...
    @property
    def fingerprint_sha256(self) -> str:
        """SHA-256 of original DER as 64 lowercase hex digits, computed lazily."""
        ...
    def to_der(self) -> bytes:
        """Return exact original peer DER, without requiring successful detail parsing."""
        ...
    def to_pem(self) -> str:
        """Encode original DER as CERTIFICATE PEM: LF, 64-column Base64, final LF."""
        ...
    def __str__(self) -> str:
        """Multiline twelve-field display; deterministic content failures use markers.

        Explicit display may parse details and compute the fingerprint, without
        expanding PEM. Resource and control exceptions propagate unchanged.
        """
        ...
    def __repr__(self) -> str:
        """Compact twelve-field display in the same order, without PEM expansion."""
        ...

@final
class Compression:
    """Immutable reusable permissions, independent of actual negotiation.

    algorithms is mandatory: None permits only uncompressed transport; str
    accepts CSV, while each list/tuple element is one algorithm name. ASCII
    trimming/case folding and deduplication produce a permission set, not a
    priority order. Only zlib, zstd and uncompressed are accepted. Empty or
    unknown names raise ValueError; invalid carriers/elements raise TypeError.
    zstd_compression_level must always be an exact built-in int in 1..22,
    including when zstd is absent. No arbitrary conversion hooks are used.
    """
    def __init__(self, *, algorithms: str | list[str] | tuple[str, ...] | None,
                 zstd_compression_level: int = 3) -> None: ...
    @property
    def algorithms(self) -> tuple[str, ...]:
        """Cached canonical permissions, or the single actual snapshot algorithm."""
        ...
    @property
    def zstd_compression_level(self) -> int | None:
        """Saved/applicable zstd level; None when algorithms does not contain zstd."""
        ...
    def __str__(self) -> str: ...
    def __repr__(self) -> str: ...

_RowT = TypeVar("_RowT", covariant=True)
_ExecutionCountT = TypeVar("_ExecutionCountT", bound=int | None, covariant=True)
_CursorT = TypeVar("_CursorT", bound="_CursorCore[Any, Any]", covariant=True)
_TupleFactory: TypeAlias = type[Cursor] | type[SSCursor] | type[tuple[Any, ...]] | None
_TupleBufferedFactory: TypeAlias = type[Cursor] | type[tuple[Any, ...]] | None
_DictFactory: TypeAlias = type[DictCursor] | type[SSDictCursor] | type[dict[Any, Any]]
_DictBufferedFactory: TypeAlias = type[DictCursor] | type[dict[Any, Any]]

@final
class Connection:
    """Fully initialized native sync connection, created by connect()."""
    def __new__(cls, _factory_only: Never, /) -> Self: ...
    @overload
    def cursor(self, cursor: _TupleFactory = None, *, streaming: Literal[True],
                   prepared: bool = False) -> SSCursor:
        """Choose row family and reading mode; exact bool/None streaming only.

        None preserves a class choice; explicit True/False selects SS/buffered
        within that tuple/dict family. Only the four native classes and exact
        tuple/dict markers are accepted at runtime. prepared is exact bool and
        fixes this Cursor protocol at creation. Creating a Cursor sends no SQL.
        """
        ...
    @overload
    def cursor(self, cursor: _TupleFactory = None, *, streaming: Literal[False],
                   prepared: bool = False) -> Cursor:
        ...
    @overload
    def cursor(self, cursor: _TupleBufferedFactory = None, *, streaming: None = None,
                   prepared: bool = False) -> Cursor:
        ...
    @overload
    def cursor(self, cursor: type[SSCursor], *, streaming: None = None,
                   prepared: bool = False) -> SSCursor:
        ...
    @overload
    def cursor(self, cursor: _TupleFactory = None, *, streaming: bool | None = None,
                   prepared: bool = False) -> Cursor | SSCursor:
        ...
    @overload
    def cursor(self, cursor: _DictFactory, *, streaming: Literal[True],
                   prepared: bool = False) -> SSDictCursor:
        ...
    @overload
    def cursor(self, cursor: _DictFactory, *, streaming: Literal[False],
                   prepared: bool = False) -> DictCursor:
        ...
    @overload
    def cursor(self, cursor: _DictBufferedFactory, *, streaming: None = None,
                   prepared: bool = False) -> DictCursor:
        ...
    @overload
    def cursor(self, cursor: type[SSDictCursor], *, streaming: None = None,
                   prepared: bool = False) -> SSDictCursor:
        ...
    @overload
    def cursor(self, cursor: _DictFactory, *, streaming: bool | None = None,
                   prepared: bool = False) -> DictCursor | SSDictCursor:
        ...
    @overload
    def transaction(self, cursor: _TupleFactory = None, *, streaming: Literal[True],
                   prepared: bool = False) -> _Transaction[SSCursor]:
        """Enter one managed transaction with the same Cursor selection rules.

        Entry starts the transaction. Exit finishes this Cursor response before
        committing on success or rolling back on failure, then closes it under
        the existing shared cleanup deadline. There is no nested-savepoint scope.
        """
        ...
    @overload
    def transaction(self, cursor: _TupleFactory = None, *, streaming: Literal[False],
                   prepared: bool = False) -> _Transaction[Cursor]:
        ...
    @overload
    def transaction(self, cursor: _TupleBufferedFactory = None, *, streaming: None = None,
                   prepared: bool = False) -> _Transaction[Cursor]:
        ...
    @overload
    def transaction(self, cursor: type[SSCursor], *, streaming: None = None,
                   prepared: bool = False) -> _Transaction[SSCursor]:
        ...
    @overload
    def transaction(self, cursor: _TupleFactory = None, *, streaming: bool | None = None,
                   prepared: bool = False) -> _Transaction[Cursor | SSCursor]:
        ...
    @overload
    def transaction(self, cursor: _DictFactory, *, streaming: Literal[True],
                   prepared: bool = False) -> _Transaction[SSDictCursor]:
        ...
    @overload
    def transaction(self, cursor: _DictFactory, *, streaming: Literal[False],
                   prepared: bool = False) -> _Transaction[DictCursor]:
        ...
    @overload
    def transaction(self, cursor: _DictBufferedFactory, *, streaming: None = None,
                   prepared: bool = False) -> _Transaction[DictCursor]:
        ...
    @overload
    def transaction(self, cursor: type[SSDictCursor], *, streaming: None = None,
                   prepared: bool = False) -> _Transaction[SSDictCursor]:
        ...
    @overload
    def transaction(self, cursor: _DictFactory, *, streaming: bool | None = None,
                   prepared: bool = False) -> _Transaction[DictCursor | SSDictCursor]:
        ...
    def close(self) -> None:
        """Permanently retire this connection. An unread SS response is abandoned
        by local transport retirement, without draining or sending a new command.
        When already synchronized, COM_QUIT is sent without a response.
        Only write_timeout and operation_timeout apply; read_timeout is not
        used. If both applicable limits are None, there is no library-imposed
        deadline. Sending failure still performs local transport cleanup and
        keeps the connection retired. Timeout or interruption grants no fresh
        network budget or cleanup grace. Repeated completed close returns None;
        overlapping operations raise InterfaceBusyError.
        """
        ...
    def begin(self) -> None: ...
    def commit(self) -> None: ...
    def rollback(self) -> None: ...
    def set_autocommit(self, enabled: bool) -> None: ...
    def escape(self, data: EscapeInput, *, many: bool = False,
               itemize: bool = True) -> str | tuple[str, ...] | list[str | tuple[str, ...]]:
        """Project escaped values/rows/batches with strict bool role switches.

        Scalars stay str, parameter rows are tuple, and batches are list; these
        shapes depend on native input roles as well as many/itemize. None is
        SQL NULL. Empty roots, actual zero-position rows and Record roles retain
        their documented distinctions. No SQL template or network is involved.
        The latest complete confirmed native reference is captured once, even
        while busy, streaming, closed or current session facts are unknown.
        This local conversion does not inherit SQL timeouts or result-pool
        quotas. A charset that cannot represent the resulting display str
        losslessly raises ValueError; no target SQL has been executed.
        """
        ...
    def create_savepoint(self, name: str) -> None:
        """Safely quote a nonempty raw name and send SAVEPOINT in the current transaction.

        No implicit BEGIN/COMMIT occurs. Success does not promise the marker was
        retained, especially with autocommit and no explicit transaction. Empty
        or NUL names raise ValueError, non-str raises TypeError. Str subclasses
        are read by base content. Other naming limits are decided by MySQL.
        """
        ...
    def rollback_to_savepoint(self, name: str) -> None:
        """Undo changes after a marker without ending the transaction; retain that marker.

        Later markers follow MySQL lifetime rules. Missing/expired markers keep
        the original server error. This neither begins nor commits a transaction.
        """
        ...
    def release_savepoint(self, name: str) -> None:
        """Release a marker without committing or rolling back its data changes.

        COMMIT, full ROLLBACK and applicable implicit-commit DDL remove markers.
        All three savepoint methods preserve Cursor results, obey normal command
        ownership/deadlines, and reject busy, unread-stream or closed sessions.
        """
        ...
    def show_warnings(self) -> tuple[tuple[str, int, str], ...]:
        """Query current diagnostics as ordered (Level, Code, Message) tuples; empty is ().

        This explicit request preserves every business Cursor result and position.
        It cannot restore diagnostics already replaced or not saved by MySQL.
        An Error-level diagnostic is a row, not a new exception. No preparatory
        SQL is sent to repair unknown session facts; decoding uses reliable
        metadata from this response or fails clearly. Normal busy/closed,
        deadlines and resource rules apply. Returned values remain independent.
        """
        ...
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
    def tls_info(self) -> TLSInfo | None:
        """Owned actual TLS facts, or None for plain transport; readable after close.

        Access performs no SQL or network I/O. The snapshot records negotiated
        facts and configured bounds; certificate details remain lazy.
        """
        ...
    @property
    def compression(self) -> Compression | None:
        """Cached actual zlib/zstd snapshot; None when transport is uncompressed.

        The input configuration is unchanged. Reading after close remains valid
        and does not query the server or retain the network transport.
        """
        ...
    @property
    def multi_statements(self) -> bool:
        """Confirmed multi-statement permission; unknown facts raise InterfaceError.

        Read-only and readable after close. This does not disable CALL results.
        """
        ...
    def set_multi_statements(self, enabled: bool) -> None:
        """Switch remote permission explicitly; exact bool only, success is None.

        Busy/closed rules apply even for a reliable same-value short circuit.
        Existing Cursor results and already running operations are unchanged.
        """
        ...
    @property
    def local_infile(self) -> bool:
        """Confirmed client upload capability; read-only, still readable after close.

        This alone never authorizes a file or changes the server global option.
        """
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

        Only exact built-in int; None/bad types raise TypeError, zero/negative raise
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

        Only exact built-in int; None/bad types raise TypeError, zero/negative raise
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

        Only exact built-in int; None/bad types raise TypeError, zero/negative raise
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

        Only exact built-in int; None/bad types raise TypeError, zero/negative raise
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
    def connect_timeout(self) -> int | float | None:
        """Client-side whole-call establishment seconds; default 10, None disables.

        Exact built-in int/float seconds only; bool/subclasses and other types
        raise TypeError. Positive finite values round to nearest nanosecond with
        ties to even; values below 1 ns, zero and negative values raise ValueError.
        Covers preparation, dialing, authentication and initialization; no extra
        network cleanup grace. Read-only after creation, readable after close."""
        ...
    @property
    def read_timeout(self) -> int | float | None:
        """Client-side maximum network read wait without progress, in seconds.

        Default None disables; exact positive int/float seconds or None. Bad type/value
        raises TypeError/ValueError. Actual progress restarts this interval; readiness
        does not. Local assignment sends no SQL and affects subsequent operations.
        An existing SS response retains its execute-time timeout snapshot.
        Not a total-response timer. Readable after close; modification then fails.
        Expiry does not establish SQL cancellation or confirmed rollback."""
        ...
    @read_timeout.setter
    def read_timeout(self, value: int | float | None) -> None: ...
    @property
    def write_timeout(self) -> int | float | None:
        """Client-side maximum network write wait without progress, in seconds.

        Default None disables; exact positive int/float seconds or None. Bad type/value
        raises TypeError/ValueError. Actual progress restarts this interval, not the
        total upload/operation deadline. Local assignment affects subsequent commands
        and sends no SQL. Saved value is readable after close; modification fails.
        An existing SS response retains its execute-time timeout snapshot.
        Expiry does not establish SQL cancellation or confirmed rollback."""
        ...
    @write_timeout.setter
    def write_timeout(self, value: int | float | None) -> None: ...
    @property
    def operation_timeout(self) -> int | float | None:
        """Client-side total limit shared by an operation's batches, in seconds.

        Default None disables; exact positive int/float or None, with
        TypeError/ValueError on invalid type/value. Covers binding, necessary internal
        confirmation, sending, complete reception and state/result publication;
        progress does not reset it. Does not time buffered fetch or transaction-body
        user code. Assignment affects new operations without SQL; readable after
        close, modification fails. No new network cleanup grace or implied rollback."""
        ...
    @operation_timeout.setter
    def operation_timeout(self, value: int | float | None) -> None: ...
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
        """Default False. False returns the original JSON text as str. True parses
        the complete JSON into Python values; malformed, incomplete or out-of-
        range content raises DataError without text fallback. This applies to
        JSON fields, not JSON-looking TEXT. SQL NULL remains None.

        This local default is copied only to new Cursors; existing Cursors and
        results do not change. Exact bool assignment only; other types raise
        TypeError. Reads and assignments send no SQL. Saved values remain
        readable after close, but assignment after close raises InterfaceError.
        """
        ...
    @decode_json.setter
    def decode_json(self, value: bool) -> None: ...
    @property
    def decode_set(self) -> bool:
        """Default False. False returns SET text as str. True returns an ordered
        tuple[str, ...]; the empty SET is (), and an empty member is preserved.
        SQL NULL remains None.

        This local default is copied only to new Cursors; existing Cursors and
        results do not change. Exact bool assignment only; other types raise
        TypeError. Reads and assignments send no SQL. Saved values remain
        readable after close, but assignment after close raises InterfaceError.
        """
        ...
    @decode_set.setter
    def decode_set(self, value: bool) -> None: ...
    @property
    def decode_bit(self) -> bool:
        """Default False. False returns the exact BIT bytes, including leading zero
        bytes. True returns an unsigned big-endian int, never a bool. Invalid
        BIT contents raise DataError. SQL NULL remains None.

        This local default is copied only to new Cursors; existing Cursors and
        results do not change. Exact bool assignment only; other types raise
        TypeError. Reads and assignments send no SQL. Saved values remain
        readable after close, but assignment after close raises InterfaceError.
        """
        ...
    @decode_bit.setter
    def decode_bit(self, value: bool) -> None: ...
    @property
    def use_decimal(self) -> bool:
        """Default True. True returns exact decimal.Decimal values for DECIMAL.
        False uses the native correctly rounded binary64 parser and returns
        float, potentially losing decimal precision; invalid or non-finite
        results raise DataError. SQL NULL remains None.

        This local default is copied only to new Cursors; existing Cursors and
        results do not change. Exact bool assignment only; other types raise
        TypeError. Reads and assignments send no SQL. Saved values remain
        readable after close, but assignment after close raises InterfaceError.
        """
        ...
    @use_decimal.setter
    def use_decimal(self, value: bool) -> None: ...
    @property
    def use_timedelta(self) -> bool:
        """Default True. True returns datetime.timedelta for MySQL TIME, including
        negative and multi-day durations. False returns datetime.time and raises
        DataError for negative values or durations at least 24 hours. SQL NULL
        remains None.

        This local default is copied only to new Cursors; existing Cursors and
        results do not change. Exact bool assignment only; other types raise
        TypeError. Reads and assignments send no SQL. Saved values remain
        readable after close, but assignment after close raises InterfaceError.
        """
        ...
    @use_timedelta.setter
    def use_timedelta(self, value: bool) -> None: ...
    @property
    def decode_text_strict(self) -> bool:
        """Default True. True raises UnicodeDecodeError for invalid text bytes.
        False preserves undecodable bytes through surrogateescape, without
        replacement characters. Binary fields remain bytes and input encoding is
        unaffected. SQL NULL remains None.

        This local default is copied only to new Cursors; existing Cursors and
        results do not change. Exact bool assignment only; other types raise
        TypeError. Reads and assignments send no SQL. Saved values remain
        readable after close, but assignment after close raises InterfaceError.
        """
        ...
    @decode_text_strict.setter
    def decode_text_strict(self, value: bool) -> None: ...
    @property
    def decode_date_strict(self) -> bool:
        """Default True. True raises DataError for zero or otherwise
        unrepresentable DATE/DATETIME values. False returns their original
        calendar text as str. Valid values retain date/datetime output in both
        modes. SQL NULL remains None.

        This local default is copied only to new Cursors; existing Cursors and
        results do not change. Exact bool assignment only; other types raise
        TypeError. Reads and assignments send no SQL. Saved values remain
        readable after close, but assignment after close raises InterfaceError.
        """
        ...
    @decode_date_strict.setter
    def decode_date_strict(self, value: bool) -> None: ...

class _CursorCore(Generic[_RowT, _ExecutionCountT]):
    """Private static common surface for four independent native Cursor classes."""
    def __new__(cls, _factory_only: Never, /) -> Self: ...
    def execute(self, sql: str, args: EscapeInput = ..., *, many: bool = False,
                itemize: bool = True, local_file: PathInput | None = None) -> _ExecutionCountT:
        """Execute SQL using the native value encoder and positional binding.

        Escape validates supported built-in values, real approved subclasses,
        containers and optional NumPy/Pandas/PyArrow/Polars inputs at runtime.
        One-dimensional sources follow many/itemize; two-dimensional tables
        supply logical rows even when many=False. Optional providers are not
        imported by this API. bytearray and memoryview are not supported.

        Exact datetime.time values preserve their clock and microseconds;
        tzinfo is ignored without calling timezone methods. For example,
        00:30+01:00 is encoded as '00:30:00'. Aware datetime values instead
        follow the documented UTC conversion rule.

        Keep args and all nested mutable values unchanged until this call
        returns. Each constructed Rust request owns stable bytes; batch
        construction does not provide an atomic snapshot of all inputs.

        many/itemize are exact bool. Omitted args leaves raw template text
        unchanged; explicit None is one SQL NULL. Binding uses only positional
        %s. Ordinary text SQL may contain multiple statements when enabled;
        approved single VALUES shapes merge automatically within this call.
        This includes fixed literal/DEFAULT fields, multiple original groups,
        proven complete-row fragments, PARTITION/approved ROW forms, and fixed
        AS/ON DUPLICATE tails without active tail parameters. Other expressions
        or active-tail parameters retain ordinary per-source-row execution.
        Complete requests and all their result sets remain in server order.
        On batch success all planned requests have been sent. SS buffers earlier
        responses natively and only streams the last request, without delaying
        later SQL until nextset(). A whole template is repeated for each source;
        unsafe in-request binding-context changes fail before that request sends.
        No merge switch or execute-time adaptive measurement is provided.

        Buffered classes return the reliable sum of each actual request's first
        result count. SS returns None while a required contribution is unknown;
        None is neither zero rows nor execution failure. Current-set counts and
        batch_row_count are separate. SS fetches do not reset its operation
        budget; user time between calls is excluded from accumulated library time.

        A prepared Cursor sends native scalar parameters at active %s positions,
        does not expand structural fragments or auto-merge VALUES, and does not
        enable public raw ? markers or split multiple SQL into prepared calls.
        Local-file upload requires ordinary text, one LOAD DATA LOCAL statement,
        local_infile capability and this call's local_file path. The server's
        INFILE name never selects an arbitrary client file. No automatic retry
        or replay occurs after execution may have begun.
        """
        ...
    def executemany(self, sql: str, args: EscapeInput) -> _ExecutionCountT:
        """Execute a batch using execute's input and datetime.time rules.

        Keep args and all nested mutable values unchanged until this call
        returns. Each constructed Rust request owns stable bytes; batch
        construction does not provide an atomic snapshot of all inputs.
        """
        ...
    def mogrify(self, sql: str, args: EscapeInput = ..., *, many: bool = False,
                itemize: bool = True) -> str | list[str]:
        """Preview every complete planned request without executing target SQL.

        One actual request returns str; multiple requests return an ordered
        list[str], including automatic VALUES chunks or repeated full templates.
        many alone does not determine the shape. Prepared previews share native
        parameter validation and wire limits; displayed SQL is not the native
        payload length. Output is not truncated or concatenated into extra SQL.
        The latest complete confirmed Connection reference is captured once.
        Busy, unread-stream, close and unknown current facts do not alone block
        this independent local helper. It does not occupy execution/fetch state,
        inherit SQL deadlines or use database result quotas. Actual later sends
        may differ after facts change; the preview is not execution confirmation.
        """
        ...
    def callproc(self, proc_name: str, args: object = (), *, database: str | None = None,
                 parameter_modes: list[str] | tuple[str, ...] | None = None) -> _ExecutionCountT:
        """Call one stored procedure and return its first public result count.

        proc_name/database are separately quoted raw identifiers; a dot in
        proc_name is not a database separator. Empty/NUL names fail locally.
        Omitted args=() means zero positions; explicit None is one NULL. Values
        use the shared Escape single-row contract; a multirow source is rejected.
        The input is unchanged and the procedure signature is not queried.

        parameter_modes is None (all IN), or a list/tuple of case-insensitive
        IN/OUT/INOUT strings, exactly one per argument. OUT requires a None
        placeholder. INOUT accepts an initial value including None. A NULL value
        never infers a direction. Local direction-count mismatches and invalid
        OUT placeholders fail before CALL. MySQL defines the actual signature;
        a detectable disagreement may be discovered after sending and does not
        cause automatic replay.

        Pure IN example: cur.callproc('report', (2026, 11)). For a procedure
        adjust(IN n, OUT result, INOUT counter), use
        cur.callproc('adjust', (5, None, 10), parameter_modes=('IN','OUT','INOUT')).
        parameter_modes=('IN',) mismatches this three-position row;
        (5, 7, 10) with OUT at position 1 is invalid because OUT must be None.

        Buffered results complete before return, so confirmed output is ready.
        SS keeps its early-return behavior and does not buffer rows merely to
        obtain OUT values. Only prepared SS callproc with explicitly declared
        OUT/INOUT may look ahead at most two native rows or to the set terminator
        to identify the real output flag. Ordinary queries, direct execute CALL
        and calls declaring no outputs still return at metadata headers.
        Finish every CALL set, including its final status,
        before reading output: while processing sets, iterate rows only when
        description is not None, then call nextset() until it returns None.
        Explicit close/context/transaction exit can also perform bounded finish.
        call_output_params then maps original OUT/INOUT positions to decoded
        values; None means unready and an empty mapping means confirmed no OUT.
        A None mapping value is a real SQL NULL. The property itself sends no SQL.

        Buffered usage (after a successful call, outputs are already confirmed):
            cur.callproc('adjust', (5, None, 10),
                         parameter_modes=('IN', 'OUT', 'INOUT'))
            outputs = cur.call_output_params  # keys 1 and 2, not the IN position

        SS usage, without forcing all rows into a Python collection:
            ss = conn.cursor(SSCursor)
            ss.callproc('adjust', (5, None, 10),
                        parameter_modes=('IN', 'OUT', 'INOUT'))
            while True:
                if ss.description is not None:
                    for row in ss:
                        consume(row)
                if ss.nextset() is None:
                    break
            outputs = ss.call_output_params

        Output uses actual server-field decode policy. The text user-variable
        bridge may return dates or JSON as strings. Prepared output can retain
        native date/JSON fields and then uses the corresponding Decode options.
        Text output management may add internal communication; prepared CALL
        uses native output metadata and creates no output user variables.
        Internal cleanup shares the remaining deadline, preserves the primary
        failure/real transaction facts and never silently replays the call.
        """
        ...
    def nextset(self) -> Literal[True] | None:
        """Discard unread current-set rows and switch in order; True or terminal None.

        An independent no-row OK is still a set. A trustworthy failure boundary
        raises its original error, never terminal None. Earlier complete sets
        remain readable according to the result/lifecycle rules.
        """
        ...
    def scroll(self, value: SupportsIndex, mode: Literal["relative", "absolute"] = "relative") -> None:
        """Move within the current set; success is None, invalid range is IndexError.

        value uses the integer index protocol, excluding bool at runtime. Valid
        positions include exact EOF and position zero for a zero-row SELECT.
        Buffered classes can move backward/forward, including after EOF. SS only
        moves forward; if its target exceeds EOF, actual skipped progress remains.
        Failed buffered range checks leave the position unchanged. No SQL replay,
        implicit nextset, extra cursor mode flag or silent clipping is performed.
        """
        ...
    def fetchone(self) -> _RowT | None:
        """Return one independently owned row, or None at current-set exhaustion."""
        ...
    def fetchmany(self, size: int | None = None) -> tuple[_RowT, ...]:
        """Return up to size rows; None uses arraysize, zero returns ().

        A supplied size is an exact built-in nonnegative int; bool, subclasses
        and index-only objects raise TypeError. Failed output does not publish
        a partial batch or advance the successful-delivery position.
        """
        ...
    def fetchall(self) -> tuple[_RowT, ...]:
        """Materialize every remaining row of this set; exhaustion returns ()."""
        ...
    def close(self) -> None:
        """Retire this Cursor; SS safely drains its unread response when permitted.

        Drain retains the original flow budget and does not grant fresh grace.
        Unrecoverable synchronization failure retires the connection. Known
        metadata/counts and confirmed call outputs remain locally readable.
        """
        ...
    def __iter__(self) -> Self: ...
    def __next__(self) -> _RowT: ...
    def __enter__(self) -> Self: ...
    def __exit__(self, typ: object, value: object, traceback: object) -> bool: ...
    @property
    def prepared(self) -> bool:
        """Creation-time protocol choice; read-only and readable after close."""
        ...
    @property
    def batch_row_count(self) -> int | None:
        """Reliable sum of the first result count of each actual user request.

        Applies to ordinary and batch calls. Unknown required contributions give
        None; confirmed prefix contributions survive a later failure. Additional
        sets/internal commands do not contribute. Fetch/nextset never count twice.
        This is separate from the current-set row_count and remains readable
        after close; starting a new execution replaces it at the real handoff.
        """
        ...
    @property
    def call_output_params(self) -> Mapping[int, Any] | None:
        """Confirmed OUT/INOUT values by original callproc argument position; no I/O.

        None means not ready; a confirmed pure-IN call gives an empty read-only
        mapping. A contained None is SQL NULL. Direct execute CALL does not infer
        managed outputs. Decode errors retain actual policy failures; valid native
        outputs can first materialize after close. Old returned mappings remain
        independent after later calls, and reading this property never drains SS.
        """
        ...
    @property
    def closed(self) -> bool:
        """Whether this Cursor itself has been permanently retired."""
        ...
    @property
    def arraysize(self) -> int:
        """Exact built-in positive int local fetchmany default, initially 1."""
        ...
    @arraysize.setter
    def arraysize(self, value: int) -> None: ...
    @property
    def rowcount(self) -> int:
        """Current-set row count, or PEP 249's -1 when unknown; readable after close."""
        ...
    @property
    def row_count(self) -> int | None:
        """Current-set row count, or None when unknown; readable after close."""
        ...
    @property
    def affected_rows(self) -> int | None:
        """Alias of row_count; readable after close."""
        ...
    @property
    def lastrowid(self) -> int | None:
        """Current-set actual insert ID, or None; readable after close."""
        ...
    @property
    def last_row_id(self) -> int | None:
        """Alias of lastrowid; readable after close."""
        ...
    @property
    def warning_count(self) -> int | None:
        """Current-set actual warning count, or None before reliable completion."""
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
        """Default False. False returns the original JSON text as str. True parses
        the complete JSON into Python values; malformed, incomplete or out-of-
        range content raises DataError without text fallback. This applies to
        JSON fields, not JSON-looking TEXT. SQL NULL remains None.

        The initial value is copied from Connection. Changes apply only to
        subsequent executions; existing results keep their execution-time
        snapshot. Exact bool assignment only; other types raise TypeError. Reads
        and assignments send no SQL. Saved values remain readable after close,
        but assignment after close raises InterfaceError.
        """
        ...
    @decode_json.setter
    def decode_json(self, value: bool) -> None: ...
    @property
    def decode_set(self) -> bool:
        """Default False. False returns SET text as str. True returns an ordered
        tuple[str, ...]; the empty SET is (), and an empty member is preserved.
        SQL NULL remains None.

        The initial value is copied from Connection. Changes apply only to
        subsequent executions; existing results keep their execution-time
        snapshot. Exact bool assignment only; other types raise TypeError. Reads
        and assignments send no SQL. Saved values remain readable after close,
        but assignment after close raises InterfaceError.
        """
        ...
    @decode_set.setter
    def decode_set(self, value: bool) -> None: ...
    @property
    def decode_bit(self) -> bool:
        """Default False. False returns the exact BIT bytes, including leading zero
        bytes. True returns an unsigned big-endian int, never a bool. Invalid
        BIT contents raise DataError. SQL NULL remains None.

        The initial value is copied from Connection. Changes apply only to
        subsequent executions; existing results keep their execution-time
        snapshot. Exact bool assignment only; other types raise TypeError. Reads
        and assignments send no SQL. Saved values remain readable after close,
        but assignment after close raises InterfaceError.
        """
        ...
    @decode_bit.setter
    def decode_bit(self, value: bool) -> None: ...
    @property
    def use_decimal(self) -> bool:
        """Default True. True returns exact decimal.Decimal values for DECIMAL.
        False uses the native correctly rounded binary64 parser and returns
        float, potentially losing decimal precision; invalid or non-finite
        results raise DataError. SQL NULL remains None.

        The initial value is copied from Connection. Changes apply only to
        subsequent executions; existing results keep their execution-time
        snapshot. Exact bool assignment only; other types raise TypeError. Reads
        and assignments send no SQL. Saved values remain readable after close,
        but assignment after close raises InterfaceError.
        """
        ...
    @use_decimal.setter
    def use_decimal(self, value: bool) -> None: ...
    @property
    def use_timedelta(self) -> bool:
        """Default True. True returns datetime.timedelta for MySQL TIME, including
        negative and multi-day durations. False returns datetime.time and raises
        DataError for negative values or durations at least 24 hours. SQL NULL
        remains None.

        The initial value is copied from Connection. Changes apply only to
        subsequent executions; existing results keep their execution-time
        snapshot. Exact bool assignment only; other types raise TypeError. Reads
        and assignments send no SQL. Saved values remain readable after close,
        but assignment after close raises InterfaceError.
        """
        ...
    @use_timedelta.setter
    def use_timedelta(self, value: bool) -> None: ...
    @property
    def decode_text_strict(self) -> bool:
        """Default True. True raises UnicodeDecodeError for invalid text bytes.
        False preserves undecodable bytes through surrogateescape, without
        replacement characters. Binary fields remain bytes and input encoding is
        unaffected. SQL NULL remains None.

        The initial value is copied from Connection. Changes apply only to
        subsequent executions; existing results keep their execution-time
        snapshot. Exact bool assignment only; other types raise TypeError. Reads
        and assignments send no SQL. Saved values remain readable after close,
        but assignment after close raises InterfaceError.
        """
        ...
    @decode_text_strict.setter
    def decode_text_strict(self, value: bool) -> None: ...
    @property
    def decode_date_strict(self) -> bool:
        """Default True. True raises DataError for zero or otherwise
        unrepresentable DATE/DATETIME values. False returns their original
        calendar text as str. Valid values retain date/datetime output in both
        modes. SQL NULL remains None.

        The initial value is copied from Connection. Changes apply only to
        subsequent executions; existing results keep their execution-time
        snapshot. Exact bool assignment only; other types raise TypeError. Reads
        and assignments send no SQL. Saved values remain readable after close,
        but assignment after close raises InterfaceError.
        """
        ...
    @decode_date_strict.setter
    def decode_date_strict(self, value: bool) -> None: ...

@final
class Cursor(_CursorCore[tuple[object, ...], int]):
    """Independent buffered tuple rows; default Connection.cursor() result.

    Complete owned results remain locally fetchable after Connection close;
    this Cursor's own close forbids further fetch. Current-set rows remain
    available for backward scroll after EOF until abandoned or replaced.
    """

@final
class DictCursor(_CursorCore[dict[str, object], int]):
    """Buffered rows are independent mutable dicts using unique Columns.keys.

    Batch fetch outer containers remain tuple. Immutable key strings can be
    shared by result, and editing one row never changes another. The buffered
    lifetime/scroll rules are identical to the tuple row family.
    """

@final
class SSCursor(_CursorCore[tuple[object, ...], int | None]):
    """Forward-only tuple rows; a streaming request returns at complete metadata.

    No row is prefetched to learn its count. Prepared callproc with explicit
    OUT/INOUT alone may inspect up to two native rows for the real output flag.
    Unread response ownership prevents
    another command without blocking. Explicit close can drain safely; abandoning
    its last reader retires the connection. Connection close does not drain it.
    """

@final
class SSDictCursor(_CursorCore[dict[str, object], int | None]):
    """Forward-only independent dict rows; shares the SS lifetime/count contract.

    Per-row keys follow Columns.keys, batch fetch outer containers are tuple,
    and fetchall still materializes every remaining Python row.
    """


def connect(*, user: str, host: str = '127.0.0.1', port: int = 3306,
            unix_socket: PathInput | None = None, password: str | None = None,
            database: str | None = None, charset: str | None = None,
            collation: str | None = None, ssl: SSL | None = None,
            server_public_key: PathInput | bytes | None = None, autocommit: bool = True,
            sql_mode: str | None = None, connect_timeout: int | float | None = 10,
            read_timeout: int | float | None = None, write_timeout: int | float | None = None,
            operation_timeout: int | float | None = None, net_read_timeout: int | None = None,
            net_write_timeout: int | None = None, wait_timeout: int | None = None,
            innodb_lock_wait_timeout: int | None = None, max_allowed_packet: int | str = '64MB',
            max_received_packet: int | str = '64MB', result_memory_limit: int | str = '256MB',
            total_result_memory_limit: int | str = '512MB', decode_json: bool = False,
            decode_set: bool = False, decode_bit: bool = False, use_decimal: bool = True,
            use_timedelta: bool = True, decode_text_strict: bool = True,
            decode_date_strict: bool = True,
            compression: str | list[str] | tuple[str, ...] | Compression | None = None,
            multi_statements: bool = True, local_infile: bool = False,
            init_sql: str | list[str] | tuple[str, ...] | None = None) -> Connection:
    """Create and fully initialize a native sync MySQL connection.

    Client connect_timeout defaults to 10; read/write/operation_timeout to None.
    Values are exact built-in int/float seconds or None to disable the applicable
    client limit; bool and numeric subclasses are rejected. Positive finite values
    must be at least 1 ns and round to nearest ns with ties to even. Integer
    seconds never pass through float. Getters return exact int for whole seconds,
    float for fractional seconds, and None when disabled. Type/value errors raise TypeError/ValueError.
    Establishment includes preparation/authentication/initialization; operation
    timeout covers later complete commands, not buffered fetch or transaction-body
    code. Progress restarts only read/write waits. No extra network cleanup grace
    or implied SQL cancellation/rollback follows expiry.

    MySQL server SESSION net_read_timeout (upload), net_write_timeout (download),
    wait_timeout (idle) and innodb_lock_wait_timeout (row lock) default to None:
    preserve then confirm their actual server values. Supplied exact positive int values
    are initialized together where possible; MySQL decides upper normalization.
    Later remote changes use explicit set_* methods; local runtime timeouts use
    property assignment. See each property's precise lifecycle/unknown semantics.

    multi_statements defaults to True and accepts exact bool; switching later uses
    set_multi_statements(). local_infile defaults to False and accepts exact bool;
    an upload additionally needs this execution's explicit local_file path.
    compression accepts strict algorithm CSV/list/tuple or reusable Compression.
    None disables compression; only explicit uncompressed permission allows raw
    fallback. A shorthand zstd connection uses level 3. Actual negotiated facts
    are reported by Connection.compression, independently of input configuration.

    Explicit utf8mb3 is BMP-only UTF-8, including strict input/output boundaries.
    The historical utf8 and utf-8 aliases continue to select utf8mb4. Binary
    parameter and result roles do not become text because of charset selection.

    init_sql is None, one nonempty SQL str or a finite list/tuple of nonempty str.
    After normal initialization, every step and all its results complete under
    the original connect deadline and are discarded; strings are not split at
    semicolons. The final real settings and transaction are retained without an
    automatic commit/rollback. Required final UTC/session conditions still apply.
    A failed step stops later steps and does not undo already committed effects.

    server_public_key is an explicit trusted RSA SubjectPublicKeyInfo PEM source.
    str and PathLike represent paths; top-level bytes represent PEM contents,
    even when they look like a filename. PathLike returning bytes is still a
    path. None means no source. No guessing, arbitrary conversion, DER/PKCS#1/
    certificate extraction, aliases or network key fetching occurs. Explicit bad
    material fails preparation even if TLS/fast authentication would not use it.
    This key does not encrypt later SQL/results or authenticate the peer itself.
    """
    ...

class _Transaction(Generic[_CursorT]):
    """Private managed scope preserving its selected native Cursor type."""
    def __enter__(self) -> _CursorT: ...
    def __exit__(self, typ: object, value: object, traceback: object) -> bool: ...


@final
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

@final
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

@final
class RetryAttempt:
    """Context yielded by Retry; no public constructor is provided."""
    def __new__(cls, _factory_only: Never, /) -> Self: ...
    def __enter__(self) -> RetryAttempt: ...
    def __exit__(self, exc_type: type[BaseException] | None,
                 exc: BaseException | None, traceback: TracebackType | None) -> bool: ...
