# Fast MySQL driver build in Cython (Sync and Async).

SQLAero 0.1.0a1 is an early alpha derived from SQLCyCli 2.3.1, commit
`aa666c6a179ab020cb8a3616734ca284623c45f2` from
https://github.com/AresJef/SQLCyCli. This release preparation changes package
and import names while retaining the existing implementation. It is not a
performance rewrite; no new performance claims are made.

The distribution name is `SQLAero`; the Python and Cython import namespace is
`sqlaero`. Existing applications must update imports and rebuild downstream
Cython extensions. Legacy pickle paths are not promised to remain compatible.
SQLAero does not install a `sqlcycli` compatibility alias.

Only locally recorded build/test results are claimed. Database behavior and
other Python/platform combinations have not been validated for this alpha.
No GitHub repository or PyPI release has been created by this preparation.
See VALIDATION.md for release readiness and verification limits.

## Installation

Install from `PyPi`

```bash
pip install sqlaero
```

The install command above applies after publication. Until then, install a locally built wheel. The future GitHub destination is not yet confirmed.

## Requirements

- CPython 3.13 only for this alpha.
- First PyPI upload: macOS arm64 wheel only; no PyPI sdist.
- macOS 26 is the intended minimum deployment target, pending successful
  `macos-26` CI installation and offline tests. Local testing was on macOS 27.0.1.
- MySQL compatibility is inherited from the SQLCyCli baseline (which stated
  MySQL 5.5+); database compatibility has not been retested for this alpha.

## Features

- Written in [Cython](https://cython.org/) for optimal performance (especially for SELECT/INSERT query).
- All classes and methods are well documented and type annotated.
- Supports both `Sync` and `Async` connection to the server.
- API Compatiable with [PyMySQL](https://github.com/PyMySQL/PyMySQL) and [aiomysql](https://github.com/aio-libs/aiomysql).
- Support conversion (escape) for most of the native python types, and objects from libaray [numpy](https://github.com/numpy/numpy) and [pandas](https://github.com/pandas-dev/pandas). Supports custom escape objects through `CustomEscapeType`; arbitrary custom conversion dictionaries are not supported.

## Usage

### Use `connect()` to create a connection (`Sync` or `Async`) with the server.

```python
import asyncio
import sqlaero

HOST = "localhost"
PORT = 3306
USER = "root"
PSWD = "password"

# Connection (Sync & Async)
async def test_connection() -> None:
    # Sync Connection - - - - - - - - - - - - - - - - - -
    with sqlaero.connect(HOST, PORT, USER, PSWD) as conn:
        with conn.cursor() as cur:
            cur.execute("SELECT 1")
            assert cur.fetchone() == (1,)

    # Connection closed
    assert conn.closed()

    # Async Connection - - - - - - - - - - - - - - - - -
    async with sqlaero.connect(HOST, PORT, USER, PSWD) as conn:
        async with conn.cursor() as cur:
            await cur.execute("SELECT 1")
            assert await cur.fetchone() == (1,)

    # Connection closed
    assert conn.closed()

if __name__ == "__main__":
    asyncio.run(test_connection())
```

### Use `create_pool()` to create a Pool for managing and maintaining connections (`Sync` or `Async`) with the server.

```python
import asyncio
import sqlaero

HOST = "localhost"
PORT = 3306
USER = "root"
PSWD = "password"

# Pool (Context: Connected)
async def test_pool_context_connected() -> None:
    async with sqlaero.create_pool(HOST, PORT, USER, PSWD, min_size=1) as pool:
        # Pool is connected: 1 free connection (min_size=1)
        assert not pool.closed() and pool.free == 1

        # Sync Connection - - - - - - - - - - - - - - - - - -
        with pool.acquire() as conn:
            with conn.cursor() as cur:
                cur.execute("SELECT 1")
                assert cur.fetchone() == (1,)

        # Async Connection - - - - - - - - - - - - - - - - -
        async with pool.acquire() as conn:
            async with conn.cursor() as cur:
                await cur.execute("SELECT 1")
                assert await cur.fetchone() == (1,)

    # Pool closed
    assert pool.closed() and pool.total == 0

# Pool (Context: Disconnected)
async def test_pool_context_disconnected() -> None:
    with sqlaero.create_pool(HOST, PORT, USER, PSWD, min_size=1) as pool:
        # Pool is not connected: 0 free connection (min_size=1)
        assert pool.closed() and pool.free == 0

        # Sync Connection - - - - - - - - - - - - - - - - - -
        with pool.acquire() as conn:
            with conn.cursor() as cur:
                cur.execute("SELECT 1")
                assert cur.fetchone() == (1,)

        # Async Connection - - - - - - - - - - - - - - - - -
        async with pool.acquire() as conn:
            async with conn.cursor() as cur:
                await cur.execute("SELECT 1")
                assert await cur.fetchone() == (1,)
        # 1 free async connection
        assert pool.free == 1

    # Pool closed
    assert pool.closed() and pool.total == 0

if __name__ == "__main__":
    asyncio.run(test_pool_context_connected())
    asyncio.run(test_pool_context_disconnected())
```

### Use the `Pool` class to create a Pool instance. `Must close manually`.

```python
import asyncio
import sqlaero

HOST = "localhost"
PORT = 3306
USER = "root"
PSWD = "password"

# Pool (Instance: Connected)
async def test_pool_instance_connected() -> None:
    pool = await sqlaero.create_pool(HOST, PORT, USER, PSWD, min_size=1)
    # Pool is connected: 1 free connection (min_size=1)
    assert not pool.closed() and pool.free == 1

    # Sync Connection - - - - - - - - - - - - - - - - - -
    with pool.acquire() as conn:
        with conn.cursor() as cur:
            cur.execute("SELECT 1")
            assert cur.fetchone() == (1,)

    # Async Connection - - - - - - - - - - - - - - - - -
    async with pool.acquire() as conn:
        async with conn.cursor() as cur:
            await cur.execute("SELECT 1")
            assert await cur.fetchone() == (1,)

    # Close pool manually
    await pool.close()
    assert pool.closed() and pool.total == 0


# Pool (Instance: Disconnected)
async def test_pool_instance_disconnected() -> None:
    pool = sqlaero.Pool(HOST, PORT, USER, PSWD, min_size=1)
    # Pool is not connected: 0 free connection (min_size=1)
    assert pool.closed() and pool.free == 0

    # Sync Connection - - - - - - - - - - - - - - - - - -
    with pool.acquire() as conn:
        with conn.cursor() as cur:
            cur.execute("SELECT 1")
            assert cur.fetchone() == (1,)

    # Async Connection - - - - - - - - - - - - - - - - -
    async with pool.acquire() as conn:
        async with conn.cursor() as cur:
            await cur.execute("SELECT 1")
            assert await cur.fetchone() == (1,)
    # 1 free async connection
    assert pool.free == 1

    # Close pool manually
    await pool.close()
    assert pool.closed() and pool.total == 0

if __name__ == "__main__":
    asyncio.run(test_pool_instance_connected())
    asyncio.run(test_pool_instance_disconnected())
```

### Use the `sqlfunc` module to escape values for MySQL functions.

```python
import asyncio
import datetime
import sqlaero
from sqlaero import sqlfunc

HOST = "localhost"
PORT = 3306
USER = "root"
PSWD = "Password_123456"

# SQLFunction
def test_sqlfunction() -> None:
    with sqlaero.connect(HOST, PORT, USER, PSWD) as conn:
        with conn.cursor() as cur:
            cur.execute("SELECT %s", sqlfunc.TO_DAYS(datetime.date(2007, 10, 7)))
            # SQLFunction 'TO_DAYS()' escaped as: TO_DAYS('2007-10-07')
            assert cur.executed_sql == "SELECT TO_DAYS('2007-10-07')"
            assert cur.fetchone() == (733321,)

    # Connection closed
    assert conn.closed()

if __name__ == "__main__":
    test_sqlfunction()
```

## Acknowledgements

SQLAero inherits SQLCyCli code and its MIT license and third-party notices.

SQLCyCli is built on top of the following open-source repositories:

- [aiomysql](https://github.com/aio-libs/aiomysql)
- [PyMySQL](https://github.com/PyMySQL/PyMySQL)

Runtime dependencies include the following open-source repositories:

- [numpy](https://github.com/numpy/numpy)
- [orjson](https://github.com/ijl/orjson)
- [pandas](https://github.com/pandas-dev/pandas)

## Build and local checks

Build with an isolated PEP 517 environment: `python -m build --sdist`.
Build a wheel from the extracted sdist, then install that wheel into a clean
virtual environment. A C compiler, Cython 3.2.1, NumPy headers and the published
cytimes package are required; do not build against a working cytimes checkout.

Run `python -I src/test_utils.py` and `python -I src/test_sqlfunc.py` explicitly against the installed wheel: `-I` prevents the uncompiled source
package from shadowing it.
The other test scripts may connect to MySQL, create databases and drop tables;
do not run them against an existing database. Database regression testing must
be performed separately in an explicitly approved disposable environment.

Optional authentication support requires `cryptography` for SHA-256/RSA
authentication and `PyNaCl` for MariaDB ed25519 authentication. These remain
optional, matching the baseline behavior.

## Attribution

The original SQLCyCli author notice and PyMySQL contributor notice are preserved
verbatim in LICENSE. The upstream aiomysql LICENSE also carries the PyMySQL
contributor MIT notice: https://github.com/aio-libs/aiomysql/blob/master/LICENSE.
No upstream release workflow or API-token configuration is included here.

## Initial release scope

The release workflow builds an sdist as its wheel input but uploads only the
validated CPython 3.13 macOS arm64 wheel to PyPI. Other systems should receive
no matching wheel instead of falling back to an unverified source build.
The source remains available in the public GitHub repository once created.

The package dependency ranges are inherited from SQLCyCli. The first release
CI uses a fixed primary dependency combination in .github/release-constraints.txt;
this does not validate every version allowed by the wider dependency metadata.
The initial CI only checks offline behavior; database tests remain separate.

See RELEASE_PREPARATION.md for the exact publisher mapping and approval steps.
Previously retained dist/ artifacts predate the Python metadata restriction
and must not be uploaded. The workflow must build and test fresh artifacts.
