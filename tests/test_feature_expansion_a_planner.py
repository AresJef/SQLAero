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


@pytest.mark.parametrize(("sql", "args", "expected"), [
    ("INSERT t VALUES (1,%s,NULL,TRUE,FALSE,DEFAULT)", [7, 8],
     b"\x03INSERT t VALUES (1,7,NULL,TRUE,FALSE,DEFAULT),(1,8,NULL,TRUE,FALSE,DEFAULT)"),
    ("INSERT t VALUES (%s),(%s) AS new ON DUPLICATE KEY UPDATE a=new.a+1", [(1, 2), (3, 4)],
     b"\x03INSERT t VALUES (1),(2),(3),(4) AS new ON DUPLICATE KEY UPDATE a=new.a+1"),
    ("REPLACE t PARTITION (p0,p1) (a,b) VALUES (%s,2)", [7, 8],
     b"\x03REPLACE t PARTITION (p0,p1) (a,b) VALUES (7,2),(8,2)"),
    ("INSERT t VALUES ROW(%s,DEFAULT),ROW(2,%s) ON DUPLICATE KEY UPDATE b=b+1", [(1, 3), (4, 5)],
     b"\x03INSERT t VALUES ROW(1,DEFAULT),ROW(2,3),ROW(4,DEFAULT),ROW(2,5) ON DUPLICATE KEY UPDATE b=b+1"),
])
def test_approved_expansions_keep_exact_complete_fragments(
    sql: str, args: object, expected: bytes,
) -> None:
    assert plan(sql, args, many=True) == [(expected, 0, 2, True)]


def test_whole_row_proof_comes_from_the_shared_value_writer() -> None:
    assert plan("INSERT t VALUES %s", [(1, "A"), [2, "B"]], many=True, itemize=False) == [
        (b"\x03INSERT t VALUES (1,'A'),(2,'B')", 0, 2, True),
    ]
    assert plan("INSERT t VALUES %s", [((1, 2),), ((3, 4),)], many=True) == [
        (b"\x03INSERT t VALUES (1,2),(3,4)", 0, 2, True),
    ]
    assert plan("INSERT t VALUES %s", [(), ()], many=True, itemize=False) == [
        (b"\x03INSERT t VALUES (),()", 0, 2, True),
    ]
    with pytest.raises(ProgrammingError):
        plan("INSERT t VALUES %s", [], many=True, itemize=False)


def test_mixed_whole_row_and_scalar_values_flush_in_input_order() -> None:
    assert plan("INSERT t VALUES %s", [(1,), (2,), "(3)", "(4)", (5,), (6,)],
                many=True, itemize=False) == [
        (b"\x03INSERT t VALUES (1),(2)", 0, 2, False),
        (b"\x03INSERT t VALUES '(3)'", 2, 3, False),
        (b"\x03INSERT t VALUES '(4)'", 3, 4, False),
        (b"\x03INSERT t VALUES (5),(6)", 4, 6, True),
    ]
    assert plan("INSERT t VALUES %s", [b"(1)", b"(2)"], many=True, itemize=False) == [
        (b"\x03INSERT t VALUES CAST(X'283129' AS BINARY)", 0, 1, False),
        (b"\x03INSERT t VALUES CAST(X'283229' AS BINARY)", 1, 2, True),
    ]


def test_native_object_and_nested_arrow_cells_publish_actual_writer_roles() -> None:
    import numpy as np
    import pandas as pd
    import pyarrow as pa

    array = np.empty(2, dtype=object)
    array[:] = [(1, 2), (3, 4)]
    for vector in (array, pd.Series([(1, 2), (3, 4)]), pa.array([[1, 2], [3, 4]])):
        assert plan("INSERT t VALUES %s", vector, many=True) == [
            (b"\x03INSERT t VALUES (1,2),(3,4)", 0, 2, True),
        ]
    for table in (np.array([[1, 2], [3, 4]]), pd.DataFrame({"a": [1, 3], "b": [2, 4]})):
        assert plan("INSERT t VALUES %s", table, itemize=False) == [
            (b"\x03INSERT t VALUES (1,2),(3,4)", 0, 2, True),
        ]


def test_original_multiple_groups_cannot_be_split_to_satisfy_request_limit() -> None:
    sql = "INSERT t VALUES (%s),(%s) AS new ON DUPLICATE KEY UPDATE a=new.a+1"
    request = b"\x03INSERT t VALUES (1),(2) AS new ON DUPLICATE KEY UPDATE a=new.a+1"
    assert plan(sql, [(1, 2), (3, 4)], many=True, limit=len(request)) == [
        (request, 0, 1, False),
        (b"\x03INSERT t VALUES (3),(4) AS new ON DUPLICATE KEY UPDATE a=new.a+1", 1, 2, True),
    ]
    from sqlaero.errors import OperationalError
    with pytest.raises(OperationalError):
        plan(sql, [(1, 2)], many=True, limit=len(request) - 1)


def test_old_measurement_probe_does_not_guess_new_whole_row_roles() -> None:
    assert getattr(_sqlaero, "_bind_sql")("INSERT t VALUES %s", ["(1)", "(2)"], many=True) == [
        b"\x03INSERT t VALUES '(1)'", b"\x03INSERT t VALUES '(2)'",
    ]


@pytest.mark.parametrize("itemize", [False, True])
def test_prepared_display_uses_native_single_value_role_validation(itemize: bool) -> None:
    from sqlaero.errors import NotSupportedError
    assert plan("SELECT %s", 42, prepared=True, itemize=itemize) == [(b"\x03SELECT 42", 0, 1, True)]
    value: object = ((1, 2),) if itemize else (1, 2)
    with pytest.raises(NotSupportedError):
        plan("SELECT %s", value, prepared=True, itemize=itemize)


def test_prepared_display_rejects_active_raw_question_without_rejecting_literals() -> None:
    with pytest.raises(ProgrammingError, match="SQL character 12"):
        plan("SELECT '中', ?", (), prepared=True)
    assert plan("SELECT '?%s%%', %s /* ? %s */", 7, prepared=True) == [
        ("\x03SELECT '?%s%%', 7 /* ? %s */".encode(), 0, 1, True),
    ]


def test_prepared_display_native_structures_and_empty_roles_match_parameter_sink() -> None:
    import numpy as np
    import pyarrow as pa
    from sqlaero.errors import NotSupportedError

    assert plan("SELECT 1", np.empty(0), itemize=False, prepared=True) == [(b"\x03SELECT 1", 0, 0, True)]
    assert plan("SELECT %s", np.array([[1], [2]]), prepared=True) == [
        (b"\x03SELECT 1", 0, 1, False), (b"\x03SELECT 2", 1, 2, True),
    ]
    for value in (pa.array([[1, 2]]), np.empty(1, dtype=[])[0]):
        with pytest.raises(NotSupportedError):
            plan("SELECT %s", value, itemize=False, prepared=True)


def test_prepared_display_checks_native_packet_size_without_capping_readable_sql() -> None:
    from sqlaero.errors import OperationalError
    value = b"x" * 100
    readable = b"\x03SELECT CAST(X'" + b"78" * 100 + b"' AS BINARY)"
    # Native EXECUTE: 10-byte header + bitmap/new-types flag/type pair (4),
    # one length byte + 100 data bytes. The displayed SQL is deliberately larger.
    assert len(readable) > 115
    assert plan("SELECT %s", value, prepared=True, limit=115) == [(readable, 0, 1, True)]
    with pytest.raises(OperationalError):
        plan("SELECT %s", value, prepared=True, limit=114)
    with pytest.raises(OperationalError):
        plan("SELECT %s", 1, prepared=True, limit=21)
    assert plan("SELECT %s", 1, prepared=True, limit=22) == [(b"\x03SELECT 1", 0, 1, True)]


def test_prepared_display_invokes_the_value_protocol_once_for_both_destinations() -> None:
    class Number(int):
        calls = 0
        def __int__(self) -> int:
            self.calls += 1
            return 7
    source = Number(1)
    assert plan("SELECT %s", source, prepared=True) == [(b"\x03SELECT 7", 0, 1, True)]
    assert source.calls == 1
    zone = CountingZone()
    source_time = datetime(2024, 1, 2, 3, 4, 5, tzinfo=zone)
    assert plan("SELECT %s", source_time, prepared=True) == [(b"\x03SELECT '2024-01-02 02:04:05'", 0, 1, True)]
    assert zone.calls == 1


def test_prepared_display_omitted_arguments_keep_unbound_text_but_guard_raw_questions() -> None:
    sql = "SELECT %(name)s, 10%%3 /*+ %s */"
    assert plan(sql, prepared=True) == [(b"\x03" + sql.encode(), 0, 0, True)]
    with pytest.raises(ProgrammingError, match="raw '\\?'"):
        plan("SELECT ?", prepared=True)
    assert plan("SELECT %s", None, prepared=True) == [(b"\x03SELECT NULL", 0, 1, True)]
