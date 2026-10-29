# SQLAero

SQLAero is a synchronous MySQL client for Python with a native Rust protocol,
connection, parameter binding, result buffering and decoding core. Python values
are adapted at the boundary; SQL literals are written into the current native
request rather than generated as per-value Python SQL strings.

The 0.2 Beta targets regular-GIL CPython 3.12–3.15 on macOS arm64 / x86_64,
Windows x86_64 / arm64, and glibc / musl Linux x86_64 / aarch64, with MySQL
8.4 / 9.7. The release workflow builds and installs all 32 original wheels,
independently rebuilds and tests the same source distribution in all 32
combinations, then publishes the same 33 files. A further 32 jobs install
the exact version from PyPI and repeat the
applicable integration suite. Async and free-threaded Python remain future work.

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
Named placeholders are not supported. Exact built-in scalar inputs are None,
bool, int, float, Decimal, str, bytes, bytearray, date, datetime, time and timedelta;
exact list/tuple provide positional, nested and batch structure. Third-party
arrays/tables, arbitrary subclasses and memoryview are outside this first version.
Nonfinite float/Decimal values become NULL. Aware datetime inputs normalize to
UTC and require a reliably confirmed UTC session.
`datetime.time` inputs preserve their clock fields and microseconds, ignore
`tzinfo`, and never call timezone methods. For example, `00:30+01:00` is written
as `'00:30:00'`; unlike aware `datetime`, it is not converted to UTC.

`executemany(sql, rows)` is the same native `many=True, itemize=True` path.
Approved INSERT/REPLACE VALUES forms merge complete rows within the actual
encoded request limit; other legal SQL executes per row. A confirmed prefix is
not a promise that its transaction committed. An entered failed call exposes
unknown aggregate counters and preserves confirmed prefix facts in diagnostics.

Rows are tuples. Results are received into native storage, then converted lazily
by fetch. A failed multi-row fetch leaves its original position unchanged.
`columns` is an immutable sequence with integer/slice/unique-key access;
`names` preserves actual labels, while `keys` resolves duplicate labels once.
Old column snapshots remain valid after another execution or Cursor close.

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

The older experimental escape implementation remains faster on most same-text
escape microbenchmarks. Its Python-string return contract differs from this
client's Rust request writer; no universal speed advantage is claimed.
Frozen experiments, private test credentials and local timing records are not
runtime dependencies or distribution contents. Public error and retry guides,
source tests and dependency notices accompany the new client.
