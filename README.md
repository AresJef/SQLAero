# SQLAero

SQLAero is a synchronous MySQL client for Python with a native Rust protocol,
connection, parameter binding, result buffering and decoding core. Python values
are adapted at the boundary; SQL literals are written into the current native
request rather than generated as per-value Python SQL strings.

The published 0.2.0b2 Beta targets regular-GIL CPython 3.12–3.15 on macOS arm64 / x86_64,
Windows x86_64 / arm64, and glibc / musl Linux x86_64 / aarch64, with MySQL
8.4 / 9.7. The release workflow builds and installs all 32 original wheels,
independently rebuilds and tests the same source distribution in all 32
combinations, then publishes the same 33 files. A further 32 jobs install
the exact version from PyPI and repeat the
applicable integration suite. Async remains future work.

The development branch adds CPython 3.14t and 3.15t with the same API and Rust
core. Historical R0 source `296f6e4` completed build and installation validation
in all 48 platform/runtime
combinations, including 96 actual wheel and independently rebuilt
source-distribution installations. The [CI run](https://github.com/AresJef/SQLAero/actions/runs/41115846043)
records that frozen source and its install checks; it does not validate later
Escape input extensions or artifacts. Later checks use the existing
[native build workflow](https://github.com/AresJef/SQLAero/actions/workflows/ci.yml),
with each result bound to its recorded source. This work does not publish a new
PyPI release. Local GIL performance costs and statistical uncertainty are
recorded separately in the [execution plan](tasks/gil-nogil-support/plan.md)
and its final reports; successful functional validation does not mean every
performance guard passed.
The [Python boundary contracts](docs/python-boundary-extension-contracts.md)
define ownership, mutable input, GC and future Escape/Decode extension rules.

The current feature candidate adds streaming and dictionary Cursors, ordered
multiple results, native prepared execution, protocol compression, explicit
LOCAL uploads, local SQL previews and expanded TLS configuration/diagnostics.
Its own source, installed-package, platform and performance evidence is recorded
separately; the historical run above does not validate these additions.

Use a matching free-threaded interpreter and wheel without forcing the GIL off
to bypass an unsupported extension. `sysconfig.get_config_var('Py_GIL_DISABLED')`
identifies the build; `sys._is_gil_enabled()` must remain false after import and
work. Ordinary Python builds use the same public interfaces. Independent
connections can execute concurrently through `ThreadPoolExecutor`; concurrent
commands on one connection fail immediately with `InterfaceBusyError`. Separate
already-buffered Cursors can decode their own results independently. Keep input
containers and mutable nested values stable until the call returns. Threading
does not imply linear scaling, and network waiting already releases the GIL in
ordinary builds.

NumPy-backed inputs must also obey NumPy's own concurrent-access contract.
Keep every shared array, record and Pandas backing store stable while Escape
reads it, or synchronize **all** readers and writers with the same application
lock. Concurrent element writes, dtype/shape changes or storage replacement
can race inside NumPy and crash the interpreter, especially with Object arrays
under free threading. A read-only flag, an owning reference or a SQLAero-only
lock does not synchronize other aliases. SQLAero still protects its own borrow,
callback, bounds and error-handling boundaries; it cannot enforce a lock that
an external writer does not acquire.

The macOS 11.0, manylinux_2_28 and musllinux_1_2 tags are build compatibility
targets, not claims of runtime tests on every older OS release. Native CI runs
on macOS 26 arm64, macOS 15 Intel, Windows Server 2022 x86_64 and Windows 11
ARM64. Linux installation tests run inside pinned native PyPA containers on
x86_64 / ARM64 Ubuntu hosts; MySQL servers remain owned loopback processes.

## Install

```sh
python -m pip install --pre 'sqlaero==0.2.0b2'
```

This Beta is a new Rust client. It is not an API-compatible update to the old
Cython 0.1 alpha. See the [release notes](docs/releases/v0.2.0b2.md) for scope,
validation boundaries and migration cautions.

## Build from source

Use a virtual environment, the locked Rust toolchain and the development
requirements. From the repository root:

```sh
python -m venv .venv
source .venv/bin/activate
python -m pip install -r requirements-dev.txt
maturin develop --release --locked -m crates/sqlaero-python/Cargo.toml
python -m pytest -q
pyright
```

Running the full integration suite also requires two isolated MySQL fixtures.
On a macOS development machine, install `mysql@8.4`, `mysql@9.7` and `openssl@4`
through Homebrew. Explicitly set `SQLAERO_TEST_MYSQL_ROOT` to an owned temporary
directory and `SQLAERO_NATIVE_MYSQL=1`, then run `python tools/mysql_fixture.py
prepare` before tests and `python tools/mysql_fixture.py stop` afterward. Tests
that require a real database refuse to select an implicit user database.
Linux and Windows use the official MySQL installations selected with
`SQLAERO_MYSQL_84_ROOT` and `SQLAERO_MYSQL_97_ROOT`; `SQLAERO_OPENSSL` can select
the OpenSSL executable. CI uses the shared native-environment action for this
preparation. The utility uses loopback ports and private temporary data, never
Homebrew services or their default database directories.
For Windows signal coverage, run `python tests/_native_signal.py --pytest -q
tests`; the launcher creates a private console before Python starts so real
control events cannot reach the shell or Actions runner. On POSIX this same
launcher executes the ordinary pytest command.

## Query and fetch

```python
import os
from pathlib import Path
from sqlaero import SSL, connect

with connect(
    user='app', password=os.environ['MYSQL_PASSWORD'], database='app',
    host='db.example.com', ssl=SSL(ca_file=Path('mysql-ca.pem')),
) as connection:
    with connection.cursor() as cursor:
        cursor.execute('SELECT %s AS n, %s AS label', (7, "a safe ' value"))
        assert cursor.fetchone() == (7, "a safe ' value")
        assert cursor.columns.names == ('n', 'label')
```

Default TCP uses certificate/hostname verification; explicit Unix sockets on
macOS/Linux use local plain transport unless TLS is explicitly configured.
Windows uses TCP and clearly rejects `unix_socket` without a transport fallback.
Initialization
confirms UTC, character encoding, autocommit and the server session timeouts
before returning a usable connection. Paths accept str and os.PathLike values.
Memory-size settings accept byte integers or forms such as `64MB` / `64MiB`.

`execute(sql)` sends original SQL without interpolation. Explicit args, including
`None`, bind `%s` positions outside SQL quotes/comments; `None` is SQL NULL.
Named placeholders are not supported. Scalar inputs include None, bool, int,
float, Decimal, str, bytes, date, datetime, time, timedelta and struct_time.
Lists, tuples, dictionary values and views, sets, frozensets and ranges provide
positional, nested and batch structure. Approved real built-in subclasses use
their documented conversion protocols; arbitrary duck types are rejected.
Optional NumPy, Pandas, PyArrow and Polars inputs are described below.
For mutable binary sources, callers may synchronize their own access and pass
an already created immutable `bytes` value. Escape performs no implicit conversion.
Nonfinite float/Decimal values become NULL. Aware datetime inputs normalize to
UTC and require a reliably confirmed UTC session.
Temporal subclasses use their base date/time fields. For `datetime`, the base
offset method calls the real `tzinfo` protocol; overriding `datetime.utcoffset`
on a subclass does not replace that protocol or change the UTC fast path.
`datetime.time` inputs preserve their clock fields and microseconds, ignore
`tzinfo`, and never call timezone methods. For example, `00:30+01:00` is written
as `'00:30:00'`; unlike aware `datetime`, it is not converted to UTC.

`executemany(sql, rows)` is the same native `many=True, itemize=True` path.
Approved INSERT/REPLACE VALUES forms merge complete rows within the actual
encoded request limit; other legal SQL executes per row. A confirmed prefix is
not a promise that its transaction committed. An entered failed call exposes
unknown aggregate counters while preserving complete trusted prefix results and
their execution facts. Current `rowcount`/`row_count`/`affected_rows`, insert ID
and warning count describe the current result set. `batch_row_count` separately
retains the reliable sum of each actual request's first result count. A failed
call does not turn a partial sum into overall success.

### Optional typed Escape inputs

These packages remain optional: ordinary SQLAero import and execution do not
import them. Use the provider's canonical classes and keep the entire source
stable until execute returns, including external buffer aliases.

| Provider | Supported input families | Native representation |
| --- | --- | --- |
| NumPy | Fixed-width signed/unsigned integers, float16/32/64, bool, str_/bytes_, datetime64/timedelta64; exact 1D/2D ndarray; np.record | Real strides and byte order, schema field order, original time unit and multiplier |
| Pandas | NA/NaT, Timestamp/Timedelta, Series, Index and approved index subclasses, exact DataFrame; Categorical and nullable integer/float/bool/string/Arrow extension arrays | Public per-column storage, masks, actual category labels and Arrow chunks |
| PyArrow | Canonical CPU Array, ChunkedArray, RecordBatch, Table and DictionaryArray | Public C Data/Stream ownership, validity, offsets, chunks and supported recursive List/Array/Struct/Dictionary values |
| Polars | Exact Series and DataFrame, including supported category/Enum and nested types; top-level Object | Public native Arrow export; Object values use the shared Python value visitor |

One-dimensional sources follow `many`/`itemize`; two-dimensional tables are row
sources even with `many=False`. Record fields form one logical row. Nested
containers and typed values remain a single SQL parameter fragment; they are
not converted to JSON or expanded using the SQL placeholder count. Empty typed
vectors and zero-row tables mean zero positions. A nonempty table with no
columns retains every logical row. A zero-field record is one row: it supplies
zero positions with `itemize=True`, or one `()` fragment with `itemize=False`.
Ordinary empty containers follow their documented `many`/`itemize` rules.

Floating inputs preserve their binary source value when widened to float64,
including negative zero. Missing/nonfinite inputs become NULL. Temporal values
keep necessary source fractional digits up to nanoseconds; target column
precision, rounding and SQL mode are enforced by MySQL. Decimal literals retain
up to 65 necessary digits without imposing a target column's scale limit.
Custom Arrow extensions/device storage, structured ndarrays other than record
scalars, ndarray subclasses, Polars LazyFrame/Expr/nested Object, bytearray and
memoryview remain outside this input contract. Provider wheel availability is
separate from the kernel's 48 GIL/NoGIL platform/runtime combinations: fixed
PyArrow 26.0.0 has no official Windows ARM64 wheel, and the fixed Polars 2.1.0
runtime has no free-threaded wheel.

Rows default to tuples. `connection.cursor(dict)` selects `DictCursor`;
`streaming=True` selects `SSCursor` or `SSDictCursor` for the chosen row format.
The same selection is available on `connection.transaction()`. Buffered results
are received into native storage, then converted lazily by fetch. Streaming
results arrive as they are consumed; a new command on that Connection reports
busy until the response is safely finished. A failed multi-row conversion
leaves its original position unchanged.
`columns` is an immutable sequence with integer/slice/unique-key access;
`names` preserves actual labels, while `keys` resolves duplicate labels once.
Old column snapshots remain valid after another execution or Cursor close.

### Execution and result options

```python
with connection.cursor(dict, streaming=True, prepared=True) as cursor:
    cursor.execute('SELECT %s AS n', (7,))
    assert cursor.fetchone() == {'n': 7}
```

Prepared mode uses the native MySQL protocol and retains public `%s` positions;
each position binds one scalar value. Ordinary text mode keeps structural
fragments and approved automatic VALUES merging. A prepared Cursor manages its
current server handle; changing SQL or its required context rebuilds it on the
next explicit execution. Execution does not automatically fall back or replay.

`multi_statements` defaults to True. `nextset()` advances through every result
in order, including independent no-row states, returning True on a switch and
None at the normal end. Batch results are retained in the same order. After a
later failure, complete trusted prefix results remain readable; reaching the
saved failure boundary raises its recorded cause instead of reporting a normal
end. Buffered `scroll()` supports relative/absolute navigation; streaming
Cursors move only forward and raise IndexError on an invalid position.

`connection.escape()` and `cursor.mogrify()` are local helpers. Escape returns
the documented str/tuple/list shape; mogrify returns str for one planned request
or list[str] for multiple requests. They capture a complete native reference
without executing SQL or taking over a Cursor's fetch state. A preview cannot
confirm what a later execution will do after session facts change. SQL that
changes binding dependencies before later parameters is rejected when safe
binding cannot be guaranteed. Aware inputs also require the statement's
effective time zone to remain reliably UTC.

Optional connection features include `init_sql`, explicit `show_warnings()` and
`create_savepoint()`/`rollback_to_savepoint()`/`release_savepoint()`. Protocol
compression is selected only from the supplied `compression` permissions;
advanced zstd levels use the immutable `Compression` class. LOCAL upload needs
both `local_infile=True` and the current execute's `local_file` path; the server's
INFILE label never authorizes opening a different client file.

SSL supports version bounds, CA directories or memory PEM, and explicit append
or replace trust policy. `connection.tls_info` is an immutable snapshot of actual
TLS facts, available after close; certificate exports retain the original peer
material. Displays summarize multi-entry name collections, while read-only
attributes return their complete tuples. The sole `server_public_key` argument
accepts trusted SPKI PEM bytes or a str/PathLike file path.

## Transactions, local settings and errors

```python
with connection.transaction() as cursor:
    cursor.execute('UPDATE users SET active=%s WHERE id=%s', (True, 7))
```

A successful block commits; a propagated business error attempts rollback.
Cleanup preserves one primary exception and records actual later failures.
Manual transaction commands inside the block follow the documented MySQL
semantics; earlier commits and implicit DDL commits cannot be undone by exit.

Local runtime settings use property assignment, such as
`connection.read_timeout = 5`. Remote session changes use explicit methods, such
as `connection.set_net_read_timeout(30)`. Client timeouts and server-side timeout
variables have different meanings; every property/method documents its scope,
units, defaults, None behavior, effects and closed-state rules.

Decode options on Connection default new Cursors; Cursor settings apply to new
executions. Existing results keep their execution-time settings. Options include
`decode_json`, `decode_set`, `decode_bit`, `use_decimal`, `use_timedelta`,
`decode_text_strict` and `decode_date_strict`.

See [errors and diagnostic snapshots](docs/errors.md), the maintained
[error-code catalog](docs/error-codes.md), and [explicit outer retry](docs/retry.md).
The public typing aliases are in `sqlaero.typing`; retry preserves callable
parameters and return inference. Execution never reconnects or replays SQL
implicitly.
`EscapeInput` has an installed family union in `typing.pyi`, including small
public-surface protocols for optional providers without importing them. Static
checking rejects unsupported roots such as `bytearray`, `memoryview` and an
ordinary unrelated object. It does not prove exact provider identity, supported
dtype, dimensions or every nested value; Escape validates those at runtime.
The runtime alias is annotation-only, not an `isinstance` support predicate.
For Pandas static checking, install its official `pandas-stubs` package; it is
a development dependency here, not a runtime requirement of SQLAero.

## Performance and development

The following figures belong to the original alpha evidence and do not describe
the later repair artifacts or this Beta. The [Beta release notes](docs/releases/v0.2.0b2.md)
explain those boundaries. Native build, installed-package and source-rebuild
checks are available in the [release workflow runs](https://github.com/AresJef/SQLAero/actions/workflows/release.yml);
local formal performance measurements are kept separate from CI observations.

On the measured macOS arm64 / CPython 3.13.3 machine, the original frozen W06 combination
passed 480 main and 204 independent protection scenarios against its initial
functional Rust baseline. Representative improvements included 1.61× for a
1,000-element string list, 1.58× for binding 1,000 INSERT rows, and 1.16–1.19×
for materializing 10,000 numeric/text rows. These are particular native phases,
not promises about total application or network-query latency. Some smaller
costs remained within the agreed 5% protection band.

The historical shared-type, same-text microbenchmark comparison found the
experimental escape implementation faster in most measured cases. Its
Python-string return contract differs from this client's Rust request writer;
that earlier comparison does not establish performance for the current expanded
input types or later artifacts, and no universal speed advantage is claimed.
Frozen experiments, private test credentials and local timing records are not
runtime dependencies or distribution contents. Public error and retry guides,
source tests and dependency notices accompany the new client.
