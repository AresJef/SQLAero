"""Bounded NoGIL Reader acceptance on the existing owned MySQL fixtures."""

# Native identity is read only to verify the exact installed artifact.
# pyright: reportPrivateUsage=false

import hashlib
import json
from pathlib import Path
import subprocess
import sys
from typing import Any

import pytest
import sqlaero
from sqlaero import _sqlaero
from test_public_connection import live, requires_mysql


def gil_enabled() -> bool:
    check = getattr(sys, "_is_gil_enabled", None)
    return True if check is None else bool(check())


pytestmark = [requires_mysql, pytest.mark.skipif(gil_enabled(), reason="requires a verified GIL-disabled package")]


def test_native_identity_retains_a_disabled_gil() -> None:
    assert gil_enabled() is False
    assert sqlaero.SSCursor.__module__ == sqlaero.SSDictCursor.__module__ == "sqlaero"
    assert hashlib.sha256(Path(_sqlaero.__file__).read_bytes()).hexdigest()


CHILD = r'''
from __future__ import annotations
from concurrent.futures import ThreadPoolExecutor
import decimal
from decimal import Decimal as NativeDecimal
import gc
import hashlib
import json
from pathlib import Path
import sys
from threading import Barrier, Event
from typing import Callable

assert sys._is_gil_enabled() is False
action: Callable[[str], None] | None = None

class GateDecimal:
    def __new__(cls: type[GateDecimal], value: str) -> NativeDecimal:
        callback = action
        if callback is not None:
            callback(value)
        return NativeDecimal(value)

# Capture the existing supported final-output constructor boundary in this
# isolated interpreter; all normal module state is restored immediately.
decimal.Decimal = GateDecimal
import sqlaero
from sqlaero import _sqlaero
decimal.Decimal = NativeDecimal
assert sys._is_gil_enabled() is False, 'native import enabled the GIL'
sys.path.insert(0, sys.argv[1])
from test_public_connection import live
from sqlaero.errors import InterfaceBusyError, InterfaceError

label, case = sys.argv[2:4]

def atomic() -> None:
    global action
    entered, release = Event(), Event()
    original = MemoryError('one actual final-output failure')
    def fail_second(value: str) -> None:
        if NativeDecimal(value) == 2:
            entered.set()
            assert release.wait(5), 'GC observer did not release the decoder'
            raise original
    with live(label) as connection, connection.cursor(streaming=True) as cursor:
        with connection.cursor() as setup:
            setup.execute('SET @ss_reader_visits=0')
        cursor.execute(' UNION ALL '.join(
            f'SELECT {975312000+n} AS marker,CAST({n} AS DECIMAL(10,0)) AS d,'
            '(@ss_reader_visits:=@ss_reader_visits+1) AS visits,11 AS l,12 AS r'
            for n in (1,2,3)))
        columns = cursor.columns
        action = fail_second
        with ThreadPoolExecutor(max_workers=1) as executor:
            future = executor.submit(cursor.fetchmany, 3)
            try:
                assert entered.wait(5), 'fetch never entered the actual second field conversion'
                gc.collect()
                for candidate in gc.get_objects():
                    if type(candidate) is tuple and len(candidate) in (3,5):
                        # A private unfinished tuple must not expose NULL slots
                        # to a parallel GC/object inspection at this safepoint.
                        for index in range(len(candidate)):
                            _ = candidate[index]
                        if len(candidate) == 5 and type(candidate[0]) is int:
                            assert candidate[0] != 975312002, 'unfinished row escaped to GC'
                        if len(candidate) == 3 and type(candidate[0]) is tuple:
                            first = candidate[0]
                            if len(first) == 5 and first[0] == 975312001:
                                raise AssertionError('unfinished outer batch escaped to GC')
                assert cursor.columns is columns
            finally:
                action = None
                release.set()
            try:
                future.result(timeout=5)
            except MemoryError as error:
                assert error is original
                assert any('row 1' in note and 'position remains 0' in note
                           for note in getattr(error, '__notes__', ()))
            else:
                raise AssertionError('the actual output failure was swallowed')
        assert cursor.scroll(0, mode='absolute') is None
        expected = tuple((975312000+n,NativeDecimal(n),n,11,12) for n in (1,2,3))
        assert cursor.fetchall() == expected
        with connection.cursor() as verify:
            verify.execute('SELECT @ss_reader_visits')
            assert verify.fetchone() == (3,), 'retry replayed target SQL'

def busy() -> None:
    global action
    entered, release = Event(), Event()
    def hold(_value: str) -> None:
        entered.set()
        assert release.wait(5), 'observer did not release the decoder'
    with live(label) as connection, live(label) as independent:
        with connection.cursor(dict, streaming=True) as cursor, independent.cursor() as observer:
            cursor.execute('SELECT CAST(1 AS DECIMAL(10,0)) AS d UNION ALL SELECT 2')
            columns = cursor.columns
            action = hold
            with ThreadPoolExecutor(max_workers=1) as executor:
                future = executor.submit(cursor.fetchone)
                try:
                    assert entered.wait(5)
                    for operation in (cursor.fetchone, lambda: cursor.fetchmany(1), cursor.close):
                        try:
                            operation()
                        except InterfaceBusyError:
                            pass
                        else:
                            raise AssertionError('same-Cursor overlap did not report Busy')
                    assert cursor.columns is columns and columns.names == ('d',)
                    observer.execute('SELECT 23')
                    assert observer.fetchone() == (23,)
                finally:
                    action = None
                    release.set()
                assert future.result(timeout=5) == {'d': NativeDecimal(1)}
            assert cursor.fetchall() == ({'d': NativeDecimal(2)},)

def independent() -> None:
    global action
    rendezvous = Barrier(2)
    def meet(_value: str) -> None:
        rendezvous.wait(timeout=5)
    with live(label) as first, live(label) as second:
        with first.cursor(streaming=True) as a, second.cursor(streaming=True) as b:
            a.execute('SELECT CAST(31 AS DECIMAL(10,0))')
            b.execute('SELECT CAST(32 AS DECIMAL(10,0))')
            action = meet
            try:
                with ThreadPoolExecutor(max_workers=2) as executor:
                    x, y = executor.submit(a.fetchone), executor.submit(b.fetchone)
                    assert x.result(timeout=7) == (NativeDecimal(31),)
                    assert y.result(timeout=7) == (NativeDecimal(32),)
            finally:
                action = None
            assert a.fetchall() == b.fetchall() == ()

def closed(complete: bool) -> None:
    global action
    entered, release = Event(), Event()
    def hold(_value: str) -> None:
        entered.set()
        assert release.wait(5)
    connection = live(label)
    saved = connection.cursor()
    saved.execute('SELECT 41 AS v UNION ALL SELECT 42')
    cursor = connection.cursor(streaming=True)
    cursor.execute('SELECT CAST(7 AS DECIMAL(10,0)) AS d UNION ALL SELECT 8')
    columns = cursor.columns
    action = hold
    try:
        with ThreadPoolExecutor(max_workers=1) as executor:
            future = executor.submit(cursor.fetchall if complete else cursor.fetchone)
            try:
                assert entered.wait(5)
                assert cursor.row_count == (2 if complete else None)
                connection.close()
                assert saved.fetchall() == ((41,), (42,))
                assert columns.names == ('d',)
            finally:
                action = None
                release.set()
            if complete:
                assert future.result(timeout=5) == ((NativeDecimal(7),), (NativeDecimal(8),))
            else:
                try:
                    future.result(timeout=5)
                except InterfaceError as error:
                    assert 'closed during conversion' in str(error)
                else:
                    raise AssertionError('unfinished cache committed after Connection close')
    finally:
        connection.close()
        cursor.close()
        saved.close()

if case == 'atomic': atomic()
elif case == 'busy': busy()
elif case == 'independent': independent()
elif case == 'closed-complete': closed(True)
elif case == 'closed-incomplete': closed(False)
else: raise AssertionError(case)
assert sys._is_gil_enabled() is False
native = Path(_sqlaero.__file__)
print(json.dumps({'case':case,'label':label,'gil':False,'native':str(native),
                  'sha256':hashlib.sha256(native.read_bytes()).hexdigest()}))
'''


@pytest.mark.parametrize("label", ["84", "97"])
@pytest.mark.parametrize("case", ["atomic", "busy", "independent", "closed-complete", "closed-incomplete"])
def test_nogil_real_materialization_and_ownership_overlap(label: str, case: str) -> None:
    result = subprocess.run(
        [sys.executable, "-c", CHILD, str(Path(__file__).parent), label, case],
        text=True, capture_output=True, timeout=25, check=False,
    )
    assert result.returncode == 0, result.stdout + result.stderr
    record: dict[str, Any] = json.loads(result.stdout.strip().splitlines()[-1])
    assert record["gil"] is False and record["case"] == case and record["label"] == label
    assert record["sha256"] == hashlib.sha256(Path(_sqlaero.__file__).read_bytes()).hexdigest()


@pytest.mark.parametrize("label", ["84", "97"])
def test_nogil_cross_batch_dict_keys_release_native_cache_and_keep_delivered_owners(label: str) -> None:
    snapshots: list[sqlaero.Columns] = []
    saved: list[dict[str, object]] = []
    with live(label, result_memory_limit="256KB", total_result_memory_limit="512KB") as connection:
        for _ in range(2):
            with connection.cursor(dict, streaming=True) as cursor:
                cursor.execute("WITH RECURSIVE seq(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM seq WHERE n<128) "
                               "SELECT n AS ordinal,REPEAT('x',8192) AS payload FROM seq ORDER BY n")
                columns = cursor.columns
                keys = columns.keys
                expected = 1
                while batch := cursor.fetchmany(2):
                    for row in batch:
                        payload = row["payload"]
                        assert row["ordinal"] == expected and isinstance(payload, str) and len(payload) == 8192
                        assert next(iter(row)) is keys[0]
                        if expected == 1:
                            saved.append(row)
                        expected += 1
                assert expected == 129 and cursor.row_count == 128
                snapshots.append(columns)
            assert connection.ping()
        connection.close()
    assert all(snapshot.names == ("ordinal", "payload") for snapshot in snapshots)
    saved[0]["ordinal"] = -1
    assert saved[1]["ordinal"] == 1 and saved[0]["payload"] == saved[1]["payload"] == "x" * 8192
