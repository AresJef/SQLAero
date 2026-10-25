"""Value and callback contracts for the actual native scalar writer."""

# pyright: reportPrivateUsage=false

from datetime import date, datetime, time, timedelta, timezone, tzinfo
from decimal import Decimal, localcontext
from zoneinfo import ZoneInfo

import pytest

from sqlaero._sqlaero import DataError, InterfaceError, OperationalError, SQLAeroError
from sqlaero import _sqlaero


@pytest.mark.parametrize(("value", "expected"), [
    (None, b"NULL"), (True, b"1"), (False, b"0"),
    (-(2**63), b"-9223372036854775808"),
    (2**63, b"9223372036854775808"),
    (2**64 - 1, b"18446744073709551615"),
    (10**65 - 1, b"9" * 65), (-(10**65 - 1), b"-" + b"9" * 65),
    (float("nan"), b"NULL"), (float("inf"), b"NULL"),
    (-float("inf"), b"NULL"), (Decimal("NaN123"), b"NULL"),
    (Decimal("-sNaN42"), b"NULL"), (Decimal("Infinity"), b"NULL"),
    (Decimal("-Infinity"), b"NULL"), (Decimal("1234.5600"), b"1234.56"),
    (Decimal("1e-30"), b"0." + b"0" * 29 + b"1"),
    (Decimal("1e64"), b"1" + b"0" * 64),
    (Decimal("-0e999999999999999999"), b"-0"),
    (date(1, 1, 1), b"'0001-01-01'"),
    (datetime(2024, 1, 2, 3, 4, 5, 123789), b"'2024-01-02 03:04:05.123789'"),
    (time(0, 30, tzinfo=timezone(timedelta(hours=1))), b"'00:30:00'"),
    (timedelta(days=1, seconds=1), b"'24:00:01'"),
    (timedelta(microseconds=-1), b"'-00:00:00.000001'"),
    (timedelta(days=-999999999, microseconds=1), b"'-23999999975:59:59.999999'"),
])
def test_exact_scalar_values(value: object, expected: bytes) -> None:
    assert _sqlaero._encode_scalar(value) == expected


@pytest.mark.parametrize("value", [10**65, -(10**65), pytest.param(10**10000, id="oversized-integer"), Decimal("1e-31"), Decimal("1e65")])
def test_numeric_range_has_a_real_client_data_error(value: object) -> None:
    with pytest.raises(DataError) as caught:
        _sqlaero._encode_scalar(value)
    assert isinstance(caught.value, SQLAeroError)
    assert caught.value.code == 65536
    assert caught.value.is_server_error is False


@pytest.mark.parametrize("value", [0.1, -0.0, 5e-324, 1.7976931348623157e308, 1.0])
def test_float_text_round_trips_and_remains_approximate(value: float) -> None:
    text = _sqlaero._encode_scalar(value).decode("ascii")
    assert "e" in text.lower()
    assert float(text).hex() == value.hex()


def test_decimal_is_independent_of_the_current_rounding_context() -> None:
    with localcontext() as context:
        context.prec = 2
        assert _sqlaero._encode_scalar(Decimal("1234.5600")) == b"1234.56"


def test_utc_normalization_preserves_calendar_endpoints_and_fold() -> None:
    assert _sqlaero._encode_scalar(datetime(1, 1, 1, 0, 30, tzinfo=timezone(timedelta(hours=1)))) == b"'0000-12-31 23:30:00'"
    assert _sqlaero._encode_scalar(datetime(9999, 12, 31, 23, 30, tzinfo=timezone(-timedelta(hours=1)))) == b"'10000-01-01 00:30:00'"
    zone = ZoneInfo("America/New_York")
    assert _sqlaero._encode_scalar(datetime(2024, 11, 3, 1, 30, tzinfo=zone, fold=0)) == b"'2024-11-03 05:30:00'"
    assert _sqlaero._encode_scalar(datetime(2024, 11, 3, 1, 30, tzinfo=zone, fold=1)) == b"'2024-11-03 06:30:00'"


class BrokenZone(tzinfo):
    """An observable approved callback; time must never call it."""

    def utcoffset(self, dt: datetime | None) -> timedelta | None:
        raise ValueError("original zone callback")


def test_time_ignores_timezone_but_datetime_preserves_callback_failure() -> None:
    zone = BrokenZone()
    assert _sqlaero._encode_scalar(time(3, 4, 5, tzinfo=zone)) == b"'03:04:05'"
    with pytest.raises(ValueError, match="original zone callback"):
        _sqlaero._encode_scalar(datetime(2024, 1, 1, tzinfo=zone))


def test_aware_values_require_real_utc_session_confirmation() -> None:
    with pytest.raises(InterfaceError, match="confirmed UTC") as caught:
        _sqlaero._encode_scalar(datetime(2024, 1, 1, tzinfo=timezone.utc), utc_confirmed=False)
    assert caught.value.is_server_error is False
    assert _sqlaero._encode_scalar(datetime(2024, 1, 1), utc_confirmed=False) == b"'2024-01-01 00:00:00'"


def test_private_request_limit_uses_operational_error_not_data_error() -> None:
    with pytest.raises(OperationalError) as caught:
        _sqlaero._encode_scalar(1234, limit=3)
    assert caught.value.code == 65538
    assert caught.value.is_server_error is False


class IntegerSubclass(int):
    """A subclass is intentionally outside this first-version value contract."""


@pytest.mark.parametrize("value", [IntegerSubclass(1), memoryview(b"a"), object()])
def test_unsupported_types_are_not_implicitly_converted(value: object) -> None:
    with pytest.raises(TypeError, match="unsupported exact escape type"):
        _sqlaero._encode_scalar(value)
