"""Exact client timeout input, nanosecond storage and saved getter behavior."""

from decimal import Decimal
from enum import IntEnum
import math
from pathlib import Path
from typing import Any

import pytest
from sqlaero import connect
from sqlaero.errors import InterfaceError, OperationalTimeoutError
from test_public_connection import live, requires_mysql

CLIENT_TIMEOUTS = ("connect_timeout", "read_timeout", "write_timeout", "operation_timeout")
SERVER_TIMEOUTS = ("net_read_timeout", "net_write_timeout", "wait_timeout", "innodb_lock_wait_timeout")


class PreparationReached(Exception):
    """Accepted configuration reached the existing path preparation boundary."""


class StopPath:
    def __fspath__(self) -> str:
        raise PreparationReached("accepted timeout configuration")


class IntSubclass(int):
    def __int__(self) -> int:
        raise AssertionError("client configuration must not invoke __int__")


class FloatSubclass(float):
    def __float__(self) -> float:
        raise AssertionError("client configuration must not invoke __float__")


class NumberProtocol:
    def __int__(self) -> int:
        raise AssertionError("client configuration must not invoke __int__")

    def __index__(self) -> int:
        raise AssertionError("client configuration must not invoke __index__")

    def __float__(self) -> float:
        raise AssertionError("client configuration must not invoke __float__")


class Code(IntEnum):
    VALUE = 1


@pytest.mark.parametrize("name", CLIENT_TIMEOUTS)
@pytest.mark.parametrize("value", [None, 2, 2.0, 0.25, 1e-9, 2**-10, 3 * 2**-10])
def test_client_timeout_acceptance_precedes_native_transport(name: str, value: object) -> None:
    settings: dict[str, Any] = {name: value}
    with pytest.raises(PreparationReached):
        connect(user="unopened", unix_socket=StopPath(), **settings)


@pytest.mark.parametrize("name", CLIENT_TIMEOUTS)
@pytest.mark.parametrize("value", [True, False, IntSubclass(1), FloatSubclass(1), Code.VALUE,
                                  Decimal("1"), "1", NumberProtocol()])
def test_client_timeout_rejects_nonexact_types_without_numeric_protocols(name: str, value: object) -> None:
    settings: dict[str, Any] = {name: value}
    with pytest.raises(TypeError, match=name):
        connect(user="unopened", unix_socket=StopPath(), **settings)


@pytest.mark.parametrize("name", CLIENT_TIMEOUTS)
@pytest.mark.parametrize("value", [0, -1, 0.0, -0.0, -0.25, math.nan, math.inf, -math.inf,
                                  1e-10, math.nextafter(1e-9, 0.0), 1e100, 2**100])
def test_client_timeout_rejects_invalid_values_before_transport(name: str, value: object) -> None:
    settings: dict[str, Any] = {name: value}
    with pytest.raises(ValueError, match=name):
        connect(user="unopened", unix_socket=StopPath(), **settings)


@pytest.mark.parametrize("name", SERVER_TIMEOUTS)
@pytest.mark.parametrize("value", [1.0, 0.25, True, IntSubclass(1), Code.VALUE, NumberProtocol()])
def test_server_timeout_remains_an_exact_integer_configuration(
    name: str, value: object, tmp_path: Path,
) -> None:
    settings: dict[str, Any] = {name: value}
    with pytest.raises(TypeError, match=name):
        connect(user="unopened", unix_socket=tmp_path / "unopened.sock", **settings)


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
def test_client_timeout_rounding_getters_and_failed_setter_preserve_saved_values(label: str) -> None:
    cases: tuple[tuple[int | float | None, int | float | None], ...] = (
        (2, 2), (2.0, 2), (0.25, 0.25), (1e-9, 1e-9),
        (2**-10, 0.000976562), (3 * 2**-10, 0.002929688),
        (1.0000000014, 1.000000001), (1.9999999996, 2), (None, None),
    )
    connection = live(label, connect_timeout=2.0, read_timeout=2.25, write_timeout=2.5, operation_timeout=5.75)
    assert type(connection.connect_timeout) is int and connection.connect_timeout == 2
    for name in CLIENT_TIMEOUTS[1:]:
        for value, expected in cases:
            setattr(connection, name, value)
            actual = getattr(connection, name)
            assert actual == expected and type(actual) is type(expected)
        setattr(connection, name, 2.25)
        for invalid, error in [(0.0, ValueError), (1e-10, ValueError), (True, TypeError), (FloatSubclass(1), TypeError)]:
            with pytest.raises(error):
                setattr(connection, name, invalid)
            assert getattr(connection, name) == 2.25
    connection.close()
    for name in CLIENT_TIMEOUTS[1:]:
        assert getattr(connection, name) == 2.25
        with pytest.raises(InterfaceError):
            setattr(connection, name, 0.5)


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
def test_server_timeout_setters_keep_their_exact_integer_contract(label: str) -> None:
    with live(label) as connection:
        for name in SERVER_TIMEOUTS:
            previous = getattr(connection, name)
            setter = getattr(connection, "set_" + name)
            for invalid in (1.0, 0.25, None, True, IntSubclass(1), Code.VALUE, NumberProtocol()):
                with pytest.raises(TypeError, match=name):
                    setter(invalid)
                assert getattr(connection, name) == previous
            setter(3)
            assert getattr(connection, name) == 3


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
def test_fractional_operation_timeout_is_used_by_the_real_command_budget(label: str) -> None:
    with live(label) as connection, connection.cursor() as cursor:
        connection.operation_timeout = 0.05
        assert connection.operation_timeout == 0.05
        with pytest.raises(OperationalTimeoutError):
            cursor.execute("SELECT SLEEP(0.2)")
