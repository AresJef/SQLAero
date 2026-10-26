# SQLAero

SQLAero is a synchronous MySQL client for Python with a native Rust protocol,
connection, parameter binding, result buffering and decoding core. Python values
are adapted at the boundary; SQL literals are written into the current native
request rather than generated as per-value Python SQL strings.

The current functional development baseline has been exercised on macOS arm64,
CPython 3.13.3 and dedicated MySQL 8.4.13 / 9.7.4 instances. Formal performance,
the CPython 3.12–3.15 release matrix and the first reconstructed release are
still under validation. Async, free-threaded Python and additional operating
systems are future work, not current support claims.

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

The reconstruction is not yet the version available from PyPI. Do not use an
older PyPI release as a substitute for this source baseline.

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

Default TCP uses certificate/hostname verification; explicit Unix sockets use
local plain transport unless TLS is explicitly configured. Initialization
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

## Development references

Local development follows `AGENTS.md`, the final specification at
`docs/connection-reconstruction-research.md`, and the unique execution ledger at
`tasks/rust-sync-kernel/plan.md`. The frozen experimental/reference directories
are development evidence, not product runtime or distribution contents.
