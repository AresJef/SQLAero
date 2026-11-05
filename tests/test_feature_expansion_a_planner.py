"""Request planning shared by execution and complete local SQL previews."""

from datetime import datetime, timedelta, tzinfo
from typing import Any

import pytest
from sqlaero import _sqlaero
from sqlaero.errors import ProgrammingError


def plan(sql: str, *args: object, **options: Any) -> list[tuple[bytes, int, int, bool]]:
    return getattr(_sqlaero, "_plan_sql")(sql, *args, **options)


def test_omitted_arguments_keep_original_template_and_empty_source_range() -> None:
    assert plan("SELECT '%s', 10%%3") == [(b"\x03SELECT '%s', 10%%3", 0, 0, True)]
    assert plan("SELECT %s", None) == [(b"\x03SELECT NULL", 0, 1, True)]
    with pytest.raises(TypeError):
        plan("SELECT %s", ...)


def test_empty_source_ranges_are_not_invented_rows() -> None:
    assert plan("SELECT 1", [], many=True) == [(b"\x03SELECT 1", 0, 0, True)]
    assert plan("INSERT t () VALUES ()", [(), ()], many=True) == [
        (b"\x03INSERT t () VALUES (),()", 0, 2, True),
    ]
    with pytest.raises(ProgrammingError):
        plan("SELECT %s", [], many=True)


def test_request_count_and_final_flag_follow_actual_requests() -> None:
    assert plan("SELECT %s", [1, 2, 3], many=True) == [
        (b"\x03SELECT 1", 0, 1, False), (b"\x03SELECT 2", 1, 2, False),
        (b"\x03SELECT 3", 2, 3, True),
    ]
    assert plan("SELECT %s; SELECT %s", (1, 2)) == [
        (b"\x03SELECT 1; SELECT 2", 0, 1, True),
    ]
    assert plan("INSERT t VALUES (%s)", [1, 2, 3], many=True, limit=len(b"\x03INSERT t VALUES (1),(2),(3)")) == [
        (b"\x03INSERT t VALUES (1),(2),(3)", 0, 3, True),
    ]


class CountingZone(tzinfo):
    def __init__(self) -> None:
        self.calls = 0

    def utcoffset(self, dt: datetime | None) -> timedelta:
        self.calls += 1
        return timedelta(hours=1)


def test_rollover_keeps_encoded_row_when_context_is_unchanged() -> None:
    zone = CountingZone()
    date = datetime(2024, 1, 2, 3, 4, 5, tzinfo=zone)
    request = b"\x03INSERT t VALUES ('2024-01-02 02:04:05')"
    assert plan("INSERT t VALUES (%s)", [date, date, date], many=True, limit=len(request)) == [
        (request, 0, 1, False), (request, 1, 2, False), (request, 2, 3, True),
    ]
    assert zone.calls == 3


def test_new_confirmed_context_reencodes_only_retained_rollover_row() -> None:
    first = b"\x03INSERT t VALUES ('\xc3\xa9')"
    assert plan("INSERT t VALUES (%s)", ["é", "é"], many=True, limit=len(first),
                change_charset="latin1") == [
        (first, 0, 1, False), (b"\x03INSERT t VALUES ('\xe9')", 1, 2, True),
    ]


def test_prepared_display_plans_rows_without_text_values_merging() -> None:
    assert plan("INSERT t VALUES (%s)", [1, 2], many=True, prepared=True) == [
        (b"\x03INSERT t VALUES (1)", 0, 1, False),
        (b"\x03INSERT t VALUES (2)", 1, 2, True),
    ]


def test_natural_table_and_zero_field_record_keep_the_shared_row_roles() -> None:
    import numpy as np
    import pandas as pd
    import pyarrow as pa

    for table in (np.array([[1, 2], [3, 4]]), pd.DataFrame({"a": [1, 3], "b": [2, 4]}),
                  pa.table({"a": [1, 3], "b": [2, 4]})):
        assert plan("INSERT t VALUES (%s,%s)", table) == [
            (b"\x03INSERT t VALUES (1,2),(3,4)", 0, 2, True),
        ]
    assert plan("SELECT 1", np.empty((0, 2))) == [(b"\x03SELECT 1", 0, 0, True)]
    assert plan("SELECT 1", np.empty((1,), dtype=[])[0]) == [(b"\x03SELECT 1", 0, 1, True)]
