"""Independent DEC01/GDA expectations at both public JSON and SQL exits."""

# pyright: reportPrivateUsage=false


from collections.abc import Callable
from decimal import Decimal, DecimalTuple, localcontext
from concurrent.futures import ThreadPoolExecutor
import json
import os
import pathlib
import random
import subprocess
import sys

import pytest
from _native_signal import private_console_options

from sqlaero import JSON, Cursor, DictCursor, SSCursor, SSDictCursor, _sqlaero, json_dumps
from sqlaero.errors import DataError
from test_public_connection import live, requires_mysql


@pytest.mark.parametrize(("text", "sql", "document"), [
    ("12.500", b"12.500", b'"12.500"'),
    ("1.20", b"1.20", b'"1.20"'),
    ("1E+3", b"1000", b'"1E+3"'),
    ("1.00E+3", b"1000", b'"1.00E+3"'),
    ("0.000001", b"0.000001", b'"0.000001"'),
    ("0.0000001", b"0.0000001", b'"1E-7"'),
    ("-0.00", b"-0.00", b'"-0.00"'),
    ("0.00", b"0.00", b'"0.00"'),
    ("0E+3", b"0", b'"0E+3"'),
    ("+001.20", b"1.20", b'"1.20"'),
    ("-1.2300E-7", b"-0.00000012300", b'"-1.2300E-7"'),
    ("1.00E-6", b"0.00000100", b'"0.00000100"'),
    ("-0E-7", b"-0.0000000", b'"-0E-7"'),
])
@pytest.mark.parametrize("capitals", [0, 1])
def test_exact_representation_is_independent_of_context(text: str, sql: bytes, document: bytes, capitals: int) -> None:
    value = Decimal(text)
    with localcontext() as context:
        context.prec = 1
        context.capitals = capitals
        for signal in context.traps:
            context.traps[signal] = True
        context.clear_flags()
        assert _sqlaero._encode_value(value).to_bytes() == sql
        assert json_dumps(value) == document
        assert json_dumps({"value": value}) == b'{"value":' + document + b'}'
        assert _sqlaero._encode_value(JSON(value)).to_bytes() == b"'" + document.replace(b'"', b'\\"') + b"'"
        assert not any(context.flags.values())


@pytest.mark.parametrize("text", ["NaN", "NaN123", "-sNaN42", "Infinity", "-Infinity"])
def test_nonfinite_categories_share_policy_without_float_conversion(text: str) -> None:
    value = Decimal(text)
    assert _sqlaero._encode_value(value).to_bytes() == b"NULL"
    assert json_dumps(value) == b"null"
    with pytest.raises(ValueError, match="nonfinite"):
        json_dumps({"x": value}, allow_nan=False)


def test_decimal_real_subclass_ignores_every_replacement_value_method() -> None:
    class Child(Decimal):
        def __getattribute__(self, _name: str) -> object:
            raise AssertionError("must use canonical base storage")
        def __str__(self) -> str:
            raise AssertionError("subclass str")
        def as_tuple(self) -> DecimalTuple:
            raise AssertionError("subclass as_tuple")
        def __float__(self) -> float:
            raise AssertionError("subclass float")
    value = Child("-12.500")
    assert json_dumps(value) == b'"-12.500"'
    assert _sqlaero._encode_value(value).to_bytes() == b"-12.500"


@pytest.mark.parametrize("text", ["1E+100", "9" * 66, "1E-66"])
def test_sql_necessary_precision_does_not_limit_json_decimal_strings(text: str) -> None:
    value = Decimal(text)
    assert json.loads(json_dumps(value)) == text
    with pytest.raises(DataError) as caught:
        _sqlaero._encode_value(value)
    assert caught.value.code == 65536 and caught.value.is_server_error is False


def test_shared_immutable_decimal_has_no_per_instance_serialization_state() -> None:
    before = getattr(sys, "_is_gil_enabled", lambda: True)()
    value = Decimal("-12345.678900")
    def convert(_index: int) -> tuple[bytes, bytes]:
        return json_dumps(value), _sqlaero._encode_value(value).to_bytes()
    with ThreadPoolExecutor(max_workers=4) as executor:
        assert list(executor.map(convert, range(128))) == [(b'"-12345.678900"', b"-12345.678900")] * 128
    assert getattr(sys, "_is_gil_enabled", lambda: True)() == before


def test_deterministic_finite_sweep_matches_independent_standard_decimal_formatters() -> None:
    source = random.Random(110314)
    for _ in range(128):
        coefficient = str(source.randrange(1, 10**25)) + "0" * source.randrange(20)
        exponent = source.randrange(-30, 20)
        value = Decimal((source.randrange(2), tuple(int(c) for c in coefficient), exponent))
        # These inputs have at most64 necessary decimal places. Standard base
        # formatters provide an independent oracle, not SQLAero-generated data.
        assert _sqlaero._encode_value(value).to_bytes() == format(value, "f").encode("ascii")
        assert json.loads(json_dumps(value)) == Decimal.__str__(value).upper()


@pytest.mark.parametrize("digits", [2000000, 32000000])
@pytest.mark.parametrize("target", ["json", "mysql"])
def test_long_coefficient_parse_and_output_preserve_repeated_native_signal_error(target: str, digits: int, record_property: Callable[[str, object], None]) -> None:
    script = "target = " + repr(target) + "\ndigits = " + repr(digits) + "\n" + """
from decimal import Decimal
import json
import time
from _native_signal import NativeAlarm, native_call_offsets
from sqlaero import json_dumps, _sqlaero
value = Decimal('1.' + '0' * digits)
original = MemoryError('original bounded Decimal work failure')
count = 0
deliveries = 0
preflight_deliveries = 0
started = None
def convert():
    return json_dumps(value) if target == 'json' else _sqlaero._encode_value(value)
offsets = native_call_offsets(convert.__code__)
def interrupt(number, frame):
    global count, deliveries
    deliveries += 1
    if frame is None or frame.f_code is not convert.__code__ or frame.f_lasti not in offsets:
        return
    count += 1
    if count == 3:
        alarm.cancel()
        raise original
alarm = NativeAlarm(interrupt)
try:
    alarm.start(0.0001, 0.0001, wait_ready=True)
    # Timer-command acknowledgement does not prove a real OS event reached
    # this interpreter. Exclude that startup interval from native progress.
    deadline = time.monotonic() + 5
    while deliveries == 0 and time.monotonic() < deadline:
        time.sleep(0.001)
    preflight_deliveries = deliveries
    assert preflight_deliveries > 0, 'no real OS signal reached the prepared handler'
    started = time.perf_counter()
    try:
        convert()
    except MemoryError as caught:
        assert caught is original and count == 3
    else:
        raise AssertionError('long Decimal work did not return repeated control')
finally:
    elapsed = None if started is None else time.perf_counter() - started
    alarm.close()
    print(json.dumps({'target': target, 'digits': digits, 'native_callbacks': count,
                      'all_deliveries': deliveries, 'preflight_deliveries': preflight_deliveries,
                      'elapsed_seconds': elapsed}), flush=True)
assert json_dumps(Decimal('1.20')) == b'"1.20"'
"""
    environment = dict(os.environ)
    environment["PYTHONPATH"] = str(pathlib.Path(__file__).parent) + os.pathsep + environment.get("PYTHONPATH", "")
    result = subprocess.run([sys.executable, "-c", script], **private_console_options(environment),
                            capture_output=True, text=True, timeout=20, check=False)
    record_property('decimal_signal_diagnostic', result.stdout)
    assert result.returncode == 0, result.stdout + result.stderr


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
@pytest.mark.parametrize("kind", [Cursor, DictCursor, SSCursor, SSDictCursor])
@pytest.mark.parametrize("prepared", [False, True])
def test_database_exact_values_scale_sensitive_expression_and_json(label: str, kind: type[Cursor] | type[DictCursor] | type[SSCursor] | type[SSDictCursor], prepared: bool) -> None:
    with live(label) as connection:
        with connection.cursor() as setup:
            setup.execute("CREATE TEMPORARY TABLE decimal_facts (amount DECIMAL(20,5), doc JSON)")
        with connection.cursor(kind, prepared=prepared) as cursor:
            cursor.decode_json = True
            assert connection.escape(Decimal("1.2300")) == "1.2300"
            assert cursor.mogrify("SELECT %s", (Decimal("1.2300"),)) == "SELECT 1.2300"
            assert cursor.execute("INSERT INTO decimal_facts VALUES (%s,%s)",
                                  (Decimal("12.500"), JSON({"n": Decimal("-0.00")}))) == 1
            cursor.execute("SELECT amount,doc FROM decimal_facts")
            expected = {"amount": Decimal("12.50000"), "doc": {"n": "-0.00"}} if kind in (DictCursor, SSDictCursor) else (Decimal("12.50000"), {"n": "-0.00"})
            assert cursor.fetchone() == expected and cursor.fetchone() is None
            cursor.execute("SELECT CAST(%s AS CHAR) AS value", (Decimal("1.2300"),))
            row = cursor.fetchone()
            assert row is not None
            if kind in (DictCursor, SSDictCursor):
                assert isinstance(row, dict) and row["value"] == "1.2300"
            else:
                assert isinstance(row, tuple) and row[0] == "1.2300"
            assert cursor.fetchone() is None
