"""Legacy public Decimal source callbacks obey the shared owner boundary."""

import os
import pathlib
import subprocess
import sys

import pytest
from test_public_connection import requires_mysql


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
@pytest.mark.parametrize("kind", ["Cursor", "DictCursor", "SSCursor", "SSDictCursor"])
@pytest.mark.parametrize("prepared", [False, True])
@pytest.mark.parametrize("raise_original", [False, True])
def test_public_as_tuple_success_or_error_cannot_hide_retired_cursor(
    label: str, kind: str, prepared: bool, raise_original: bool,
) -> None:
    script = f"label={label!r}\nkind={kind!r}\nprepared={prepared!r}\nraise_original={raise_original!r}\n" + """
import decimal
import sys
import pytest
original = MemoryError('original public Decimal source error after retirement')
callback = None
calls = 0
class DecimalSource:
    def as_tuple(self):
        global calls
        calls += 1
        callback()
        if raise_original:
            raise original
        return (0, (1, 2, 5, 0), -3)
decimal.Decimal = DecimalSource
import sqlaero
from sqlaero.errors import InterfaceError, InterfaceBusyError
from test_public_connection import live
before = getattr(sys, '_is_gil_enabled', lambda: True)()
with live(label) as connection:
    with connection.cursor() as setup:
        setup.execute('CREATE TEMPORARY TABLE decimal_owner (value DECIMAL(10,3))')
    context = connection.transaction(getattr(sqlaero, kind), prepared=prepared)
    cursor = context.__enter__()
    cursor.execute('INSERT INTO decimal_owner VALUES (1)')
    exits = []
    def retire():
        try:
            context.__exit__(None, None, None)
        except InterfaceBusyError as caught:
            exits.append(caught)
    callback = retire
    caught = None
    try:
        cursor.execute('INSERT INTO decimal_owner VALUES (%s)', (DecimalSource(),))
    except Exception as error:
        caught = error
    assert calls == 1 and len(exits) == 1 and cursor.closed and not connection.closed
    if raise_original:
        assert caught is original
    else:
        assert isinstance(caught, InterfaceError)
    with connection.cursor() as check:
        check.execute('SELECT COUNT(*) FROM decimal_owner')
        assert check.fetchone() == (1,)
    connection.rollback()
    with connection.cursor() as check:
        check.execute('SELECT COUNT(*) FROM decimal_owner')
        assert check.fetchone() == (0,)
assert getattr(sys, '_is_gil_enabled', lambda: True)() == before
"""
    environment = dict(os.environ)
    environment["PYTHONPATH"] = str(pathlib.Path(__file__).parent) + os.pathsep + environment.get("PYTHONPATH", "")
    result = subprocess.run([sys.executable, "-c", script], env=environment,
                            capture_output=True, text=True, timeout=20, check=False)
    assert result.returncode == 0, result.stdout + result.stderr
