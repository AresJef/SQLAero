"""Full source ticks feed independent MySQL microsecond and JSON policies."""

# pyright: reportPrivateUsage=false


import pytest
from datetime import datetime, timedelta
from uuid import uuid4
from sqlaero import JSON, Cursor, DictCursor, SSCursor, SSDictCursor, _sqlaero, json_dumps
from test_public_connection import live, requires_mysql

np = pytest.importorskip("numpy")


@pytest.mark.parametrize(("value", "mysql", "json"), [
    (np.datetime64("2026", "Y"), "'2026-01-01 00:00:00'", '"2026-01-01T00:00:00"'),
    (np.datetime64("2026-11", "M"), "'2026-11-01 00:00:00'", '"2026-11-01T00:00:00"'),
    (np.datetime64(0, "W"), "'1970-01-01 00:00:00'", '"1970-01-01T00:00:00"'),
    (np.datetime64("2026-11-12T08:09:10.120", "ms"), "'2026-11-12 08:09:10.120000'", '"2026-11-12T08:09:10.120000"'),
    (np.datetime64("2026-11-12T08:09:10.123450", "us"), "'2026-11-12 08:09:10.123450'", '"2026-11-12T08:09:10.123450"'),
    (np.datetime64("2026-11-12T08:09:10.123456789", "ns"), "'2026-11-12 08:09:10.123456'", '"2026-11-12T08:09:10.123456789"'),
    (np.datetime64("2026-11-12T23:59:59.999999900", "ns"), "'2026-11-12 23:59:59.999999'", '"2026-11-12T23:59:59.999999900"'),
    (np.datetime64(-1, "ns"), "'1969-12-31 23:59:59.999999'", '"1969-12-31T23:59:59.999999999"'),
    (np.datetime64("0000-01-01", "D"), "'0000-01-01 00:00:00'", '"0000-01-01T00:00:00"'),
    (np.datetime64("10000-01-01", "D"), "'10000-01-01 00:00:00'", '"10000-01-01T00:00:00"'),
    (np.datetime64("-0001-01-01", "D"), "'-0001-01-01 00:00:00'", '"-0001-01-01T00:00:00"'),
    (np.timedelta64(120, "ms"), "'00:00:00.120000'", '"PT0.12S"'),
    (np.timedelta64(900, "ns"), "'00:00:00'", '"PT0.0000009S"'),
    (np.timedelta64(-1, "ns"), "'00:00:00'", '"-PT0.000000001S"'),
    (np.timedelta64(-1_123_456_900, "ns"), "'-00:00:01.123456'", '"-PT1.1234569S"'),
    (np.timedelta64(1, "D"), "'24:00:00'", '"P1D"'),
    (np.timedelta64(1, "W"), "'168:00:00'", '"P7D"'),
    (np.timedelta64(1, "M"), "'730:29:06'", '"P1M"'),
    (np.timedelta64(1, "Y"), "'8765:49:12'", '"P1Y"'),
    (np.timedelta64(-14, "M"), "'-10226:47:24'", '"-P14M"'),
    (np.timedelta64(0, "M"), "'00:00:00'", '"P0M"'),
])
def test_exact_tick_outputs(value: object, mysql: str, json: str) -> None:
    assert _sqlaero._encode_value(value).to_bytes() == mysql.encode()
    assert json_dumps(value) == json.encode()


@pytest.mark.parametrize("kind", ["M8", "m8"])
@pytest.mark.parametrize("unit", ["ps", "fs", "as", "generic", "0ns"])
def test_scalar_nat_precedes_finite_unit_or_multiplier_admission(kind: str, unit: str) -> None:
    dtype = np.dtype(kind if unit == "generic" else f"{kind}[{unit}]")
    value = np.array([-(1 << 63)], dtype="i8").view(dtype)[0]
    assert _sqlaero._encode_value(value).to_bytes() == b"NULL"
    assert json_dumps(value, allow_nan=False) == b"null"
    finite = np.array([1], dtype="i8").view(dtype)[0]
    with pytest.raises(Exception):
        _sqlaero._encode_value(finite)
    with pytest.raises((TypeError, ValueError)):
        json_dumps(finite)


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
@pytest.mark.parametrize("kind", [Cursor, DictCursor, SSCursor, SSDictCursor])
@pytest.mark.parametrize("prepared", [False, True])
@pytest.mark.parametrize("truncate_mode", [False, True])
@pytest.mark.parametrize(("dt", "span", "expected_dt", "expected_span", "json_dt", "json_span"), [
    (np.datetime64(-1, "ns"), np.timedelta64(-1, "ns"),
     datetime(1969, 12, 31, 23, 59, 59, 999999), timedelta(),
     "1969-12-31T23:59:59.999999999", "-PT0.000000001S"),
    (np.datetime64("2026-11-12T23:59:59.999999900", "ns"), np.timedelta64(-1_123_456_900, "ns"),
     datetime(2026, 11, 12, 23, 59, 59, 999999), timedelta(seconds=-1, microseconds=-123456),
     "2026-11-12T23:59:59.999999900", "-PT1.1234569S"),
    (np.datetime64("2026-11-12T08:09:10.000000009", "ns"), np.timedelta64(90, "s"),
     datetime(2026, 11, 12, 8, 9, 10), timedelta(seconds=90),
     "2026-11-12T08:09:10.000000009", "PT90S"),
], ids=["negative-epoch", "midnight", "submicrosecond"])
def test_database_receives_truncated_values_and_json_keeps_source_precision(
    label: str, kind: type[Cursor] | type[DictCursor] | type[SSCursor] | type[SSDictCursor],
    prepared: bool, truncate_mode: bool, dt: object, span: object,
    expected_dt: datetime, expected_span: timedelta, json_dt: str, json_span: str,
) -> None:
    table = "tick_values_" + uuid4().hex[:12]
    with live(label) as connection:
        with connection.cursor() as setup:
            setup.execute("SELECT @@SESSION.sql_mode")
            mode_row = setup.fetchone()
            assert mode_row is not None and isinstance(mode_row[0], str)
            modes = [value for value in mode_row[0].split(",") if value != "TIME_TRUNCATE_FRACTIONAL"]
            if truncate_mode:
                modes.append("TIME_TRUNCATE_FRACTIONAL")
            setup.execute("SET SESSION sql_mode=%s", (",".join(modes),))
            setup.execute(f"CREATE TEMPORARY TABLE `{table}` (stamp DATETIME(6), span TIME(6), doc JSON)")
        with connection.cursor(kind, prepared=prepared) as current:
            current.decode_json = True
            assert current.execute(f"INSERT INTO `{table}` VALUES (%s,%s,%s)",
                                   (dt, span, JSON({"stamp": dt, "span": span}))) == 1
            current.execute(f"SELECT stamp,span,doc FROM `{table}`")
            doc = {"stamp": json_dt, "span": json_span}
            expected = ({"stamp": expected_dt, "span": expected_span, "doc": doc}
                        if kind in (DictCursor, SSDictCursor) else (expected_dt, expected_span, doc))
            assert current.fetchone() == expected
            assert current.fetchone() is None
