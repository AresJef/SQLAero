"""Independent standard-family output examples from the frozen type table."""

from datetime import date, datetime, time, timedelta, timezone, tzinfo

import pytest

from sqlaero import json_dumps


@pytest.mark.parametrize(("value", "expected"), [
    (date(1, 1, 2), b'"0001-01-02"'),
    (date(9999, 12, 31), b'"9999-12-31"'),
    (datetime(2026, 11, 12, 8), b'"2026-11-12T08:00:00"'),
    (datetime(2026, 11, 12, 8, microsecond=120000), b'"2026-11-12T08:00:00.120000"'),
    (datetime(2026, 11, 12, 8, tzinfo=timezone.utc), b'"2026-11-12T08:00:00+00:00"'),
    (datetime(2026, 11, 12, 8, tzinfo=timezone(timedelta(hours=8))), b'"2026-11-12T08:00:00+08:00"'),
    (time(8, 1, 2), b'"08:01:02"'),
    (time(8, 1, 2, 120000), b'"08:01:02.120000"'),
    (time(8, tzinfo=timezone.utc), b'"08:00:00+00:00"'),
    (time(8, tzinfo=timezone(timedelta(seconds=30, microseconds=500000))), b'"08:00:00+00:00:30.500000"'),
    (timedelta(), b'"P0D"'),
    (timedelta(seconds=90), b'"PT90S"'),
    (timedelta(seconds=1, microseconds=120000), b'"PT1.12S"'),
    (timedelta(days=1, seconds=90), b'"P1DT90S"'),
    (timedelta(microseconds=-1), b'"-PT0.000001S"'),
    (timedelta(days=-1, microseconds=1), b'"-PT86399.999999S"'),
    (timedelta.max, b'"P999999999DT86399.999999S"'),
    (timedelta.min, b'"-P999999999D"'),
])
def test_standard_temporal_json(value: object, expected: bytes) -> None:
    assert json_dumps(value) == expected
    assert json_dumps([value]) == b"[" + expected + b"]"


@pytest.mark.parametrize(("base", "args", "expected"), [
    (date, (2024, 1, 2), b'"2024-01-02"'),
    (datetime, (2024, 1, 2, 3, 4, 5), b'"2024-01-02T03:04:05"'),
    (time, (3, 4, 5), b'"03:04:05"'),
    (timedelta, (1, 2), b'"P1DT2S"'),
])
def test_temporal_real_subclasses_ignore_replaced_fields_and_display(base: type, args: tuple[int, ...], expected: bytes) -> None:
    def forbidden(*_args: object) -> object:
        raise AssertionError("base storage must not call subclass replacements")
    attrs = {name: property(forbidden) for name in ("year", "month", "day", "hour", "minute", "second", "microsecond", "days", "seconds", "microseconds")}
    attrs.update(__str__=forbidden, isoformat=forbidden, utcoffset=forbidden)
    Child = type("Child", (base,), attrs)
    assert json_dumps(Child(*args)) == expected


def test_offset_protocol_keeps_local_fields_and_the_original_error() -> None:
    calls: list[datetime | None] = []
    original = MemoryError("original zone error")
    class Zone(tzinfo):
        def utcoffset(self, dt: datetime | None) -> timedelta:
            calls.append(dt)
            if len(calls) > 2:
                raise original
            return timedelta(seconds=-30, microseconds=-500000)
    zone = Zone()
    stamp = datetime(2026, 1, 2, 3, tzinfo=zone, fold=1)
    clock = time(3, tzinfo=zone)
    assert json_dumps(stamp) == b'"2026-01-02T03:00:00-00:00:30.500000"'
    assert json_dumps(clock) == b'"03:00:00-00:00:30.500000"'
    assert calls == [stamp, None]
    with pytest.raises(MemoryError) as caught:
        json_dumps(stamp)
    assert caught.value is original


def test_offset_returning_none_is_naive_without_a_fallback() -> None:
    class Zone(tzinfo):
        def utcoffset(self, _dt: datetime | None) -> None:
            return None
    assert json_dumps(datetime(2026, 1, 2, tzinfo=Zone())) == b'"2026-01-02T00:00:00"'
    assert json_dumps(time(3, tzinfo=Zone())) == b'"03:00:00"'
