"""All JSON families use coordinates rather than per-type tracing adapters."""

from datetime import datetime, tzinfo
import math

import pytest

from sqlaero import JSON, json_dumps, _sqlaero


def notes(error: BaseException) -> str:
    return "\n".join(getattr(error, "__notes__", ()))


@pytest.mark.parametrize("sql", [False, True])
def test_nested_fields_and_rows_locate_the_actual_nonfinite_value(sql: bool) -> None:
    source = {"orders": [{"amount": math.nan}]}
    with pytest.raises(ValueError) as caught:
        if sql:
            _sqlaero._bind_sql("SELECT %s", (JSON(source, allow_nan=False),))
        else:
            json_dumps(source, allow_nan=False)
    assert "while serializing JSON at data.orders[0].amount" in notes(caught.value)
    if sql:
        assert "args[0]" in notes(caught.value)


@pytest.mark.parametrize(("source", "location", "error_type"), [
    ({"a": [b"binary"]}, "data.a[0]", TypeError),
    ({None: 7}, "data[key#0]", TypeError),
    ({1: 7, "1": 8}, "data[key#1]", ValueError),
    ({"a\0x": object()}, 'data["a\\0x"]', TypeError),
])
def test_value_and_key_failures_keep_distinct_source_coordinates(source: object, location: str, error_type: type[Exception]) -> None:
    with pytest.raises(error_type) as caught:
        json_dumps(source)
    assert "while serializing JSON at " + location in notes(caught.value)


def test_the_original_protocol_exception_is_kept_and_located_once() -> None:
    original = MemoryError("actual offset protocol error")
    class Zone(tzinfo):
        def utcoffset(self, dt):
            raise original
    with pytest.raises(MemoryError) as caught:
        json_dumps({"orders": [{"created": datetime(2026, 1, 2, tzinfo=Zone())}]})
    assert caught.value is original
    assert notes(original).count("while serializing JSON at") == 1
    assert "data.orders[0].created" in notes(original)


def test_long_diagnostic_names_are_explicitly_bounded_not_copied_in_full() -> None:
    name = "😀" * 10000
    with pytest.raises(TypeError) as caught:
        json_dumps({name: object()})
    diagnostic = notes(caught.value)
    assert name not in diagnostic
    assert "entry#0" in diagnostic and "…" in diagnostic
    assert len(diagnostic) < 1000
