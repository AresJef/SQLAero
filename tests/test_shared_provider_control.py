"""Cold provider discovery and required metadata share actual owner control."""

import json
import pathlib
import subprocess
import sys

import pytest
from sqlaero import _sqlaero
from test_public_connection import requires_mysql


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
@pytest.mark.parametrize("cursor", ["Cursor", "DictCursor", "SSCursor", "SSDictCursor"])
@pytest.mark.parametrize("prepared", [False, True])
@pytest.mark.parametrize("phase", ["cold", "metadata", "array_metadata"])
@pytest.mark.parametrize("raise_original", [False, True])
def test_numpy_protocol_reentry_stops_before_later_callbacks_or_wire(
    label: str, cursor: str, prepared: bool, phase: str, raise_original: bool,
) -> None:
    # Each subprocess gets a genuinely cold interpreter-owned provider cache.
    # It inherits the exact installed native path, not the provider base wheel.
    options = json.dumps(dict(label=label, cursor=cursor, prepared=prepared,
                              phase=phase, raise_original=raise_original,
                              native=str(pathlib.Path(_sqlaero.__file__).resolve())))
    script = """
import json, pathlib, sys
sys.path.insert(0, str(pathlib.Path.cwd() / 'tests'))
import numpy as np
import sqlaero
from sqlaero.errors import InterfaceBusyError, InterfaceError
from test_public_connection import live
options = json.loads(sys.argv[1])
assert str(pathlib.Path(sqlaero._sqlaero.__file__).resolve()) == options['native']
gil = sys._is_gil_enabled() if hasattr(sys, '_is_gil_enabled') else True
original_error = MemoryError('original provider metadata error')
calls, exits = [], []
with live(options['label']) as connection:
    context = connection.transaction(getattr(sqlaero, options['cursor']), prepared=options['prepared'])
    current = context.__enter__()
    current.execute('CREATE TEMPORARY TABLE provider_control (value DATETIME(6)) ENGINE=InnoDB')
    current.execute("INSERT INTO provider_control VALUES ('2024-01-01')")
    def retire():
        calls.append('retire')
        try:
            context.__exit__(None, None, None)
        except InterfaceBusyError as error:
            exits.append(error)
        if options['raise_original']:
            raise original_error
    old_getattr, int8, int16, datetime_data = np.__getattr__, np.int8, np.int16, np.datetime_data
    if options['phase'] == 'cold':
        del np.int8, np.int16
        def get(name):
            if name == 'int8':
                retire()
                return int8
            if name == 'int16':
                calls.append('later')
                return int16
            return old_getattr(name)
        np.__getattr__ = get
    else:
        class Multiplier:
            def __index__(self):
                calls.append('later')
                return 1
        def metadata(dtype):
            retire()
            if options['phase'] == 'array_metadata':
                return ('ns', Multiplier())
            return datetime_data(dtype)
        np.datetime_data = metadata
    caught = None
    try:
        first, second = np.datetime64('2026-11-12', 'D'), np.datetime64('2026-11-13', 'D')
        if options['phase'] == 'array_metadata':
            first, second = np.array([first]), np.array([second])
        current.execute('INSERT INTO provider_control VALUES (%s),(%s)', (first, second))
    except Exception as error:
        caught = error
    finally:
        np.__getattr__, np.int8, np.int16, np.datetime_data = old_getattr, int8, int16, datetime_data
    assert current.closed and not connection.closed
    assert calls == ['retire'] and len(exits) == 1, (calls, exits)
    if options['raise_original']:
        assert caught is original_error, caught
    else:
        assert isinstance(caught, InterfaceError), caught
    with connection.cursor() as check:
        check.execute('SELECT COUNT(*) FROM provider_control')
        assert check.fetchone() == (1,)
    connection.rollback()
    with connection.cursor() as check:
        check.execute('SELECT COUNT(*) FROM provider_control')
        assert check.fetchone() == (0,)
assert (sys._is_gil_enabled() if hasattr(sys, '_is_gil_enabled') else True) == gil
"""
    result = subprocess.run([sys.executable, "-c", script, options],
                            capture_output=True, text=True, timeout=30, check=False)
    assert result.returncode == 0, result.stdout + result.stderr
