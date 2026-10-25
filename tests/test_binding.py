"""Native SQL positions and the one shared scalar/container shape contract."""

# pyright: reportPrivateUsage=false

from datetime import datetime, timedelta, tzinfo
import signal
from types import FrameType
import pytest
from sqlaero import _sqlaero
from sqlaero._sqlaero import DataError, ProgrammingError, OperationalTimeoutError


@pytest.mark.parametrize(("args", "many", "itemize", "sql", "expected"), [
    (None, False, True, "SELECT %s", [b"\x03SELECT NULL"]),
    ("ab", True, True, "SELECT %s", [b"\x03SELECT 'ab'"]),
    ([1, 2], False, True, "SELECT %s,%s", [b"\x03SELECT 1,2"]),
    ([1, 2], False, False, "SELECT 1 IN %s", [b"\x03SELECT 1 IN (1,2)"]),
    ([1, 2], True, True, "SELECT %s", [b"\x03SELECT 1", b"\x03SELECT 2"]),
    ([1, 2], True, False, "SELECT %s", [b"\x03SELECT 1", b"\x03SELECT 2"]),
    ([(1, "A"), (2, "B")], True, True, "VALUES (%s,%s)", [b"\x03VALUES (1,'A')", b"\x03VALUES (2,'B')"]),
    ([(1, "A"), (2, "B")], True, False, "VALUES %s", [b"\x03VALUES (1,'A')", b"\x03VALUES (2,'B')"]),
    ([], False, True, "SELECT 1", [b"\x03SELECT 1"]),
    ([], False, False, "VALUES %s", [b"\x03VALUES ()"]),
    ([], True, False, "SELECT 1", [b"\x03SELECT 1"]),
    ([(), ()], True, True, "SELECT 1", [b"\x03SELECT 1", b"\x03SELECT 1"]),
    ([()], True, False, "VALUES %s", [b"\x03VALUES ()"]),
    ([1, (2,), [3]], True, True, "SELECT %s", [b"\x03SELECT 1", b"\x03SELECT 2", b"\x03SELECT 3"]),
])
def test_shape_matrix(args: object, many: bool, itemize: bool, sql: str, expected: list[bytes]) -> None:
    assert _sqlaero._bind_sql(sql, args, many=many, itemize=itemize) == expected


def test_literals_comments_and_percent_sequences_have_one_pass_semantics() -> None:
    sql = "SELECT '%s',\"%s\",`%s`,%%s,%%%s /* %s */ -- %s\n,%s"
    assert _sqlaero._bind_sql(sql, [7, 8]) == [b"\x03SELECT '%s',\"%s\",`%s`,%s,%7 /* %s */ -- %s\n,8"]
    assert _sqlaero._bind_sql("SELECT %s", ["value%s%%"]) == [b"\x03SELECT 'value%s%%'"]
    assert _sqlaero._bind_sql("SELECT %s,'unfinished", [7]) == [b"\x03SELECT 7,'unfinished"]
    assert _sqlaero._bind_sql("SELECT 10%(2)", []) == [b"\x03SELECT 10%(2)"]
    assert _sqlaero._bind_sql("SELECT 7--\n%s", [8]) == [b"\x03SELECT 7--\n8"]
    assert _sqlaero._bind_sql("SELECT 7--\r%s", []) == [b"\x03SELECT 7--\r%s"]
    assert _sqlaero._bind_sql("SELECT 7--\u00a0%s", [], charset="armscii8") == [b"\x03SELECT 7--\xa0%s"]


def test_quote_modes_change_positions_using_the_actual_mysql_context() -> None:
    sql = r'SELECT "x\", %s'
    with pytest.raises(ProgrammingError):
        _sqlaero._bind_sql(sql, [1])
    assert _sqlaero._bind_sql(sql, [1], ansi_quotes=True) == [b'\x03SELECT "x\\", 1']
    sql = r"SELECT 'x\', %s"
    with pytest.raises(ProgrammingError):
        _sqlaero._bind_sql(sql, [1])
    assert _sqlaero._bind_sql(sql, [1], no_backslash=True) == [b"\x03SELECT 'x\\', 1"]


@pytest.mark.parametrize("sql", ["SELECT /*! %s */ 1", "SELECT /*+ %s */ 1", "SELECT %(name)s"])
def test_prohibited_active_templates_are_located(sql: str) -> None:
    with pytest.raises(ProgrammingError) as caught:
        _sqlaero._bind_sql(sql, [7])
    assert caught.value.code == 65541
    assert caught.value.is_server_error is False
    with pytest.raises(ProgrammingError, match="character 2"):
        _sqlaero._bind_sql("中 %(name)s", [7])


def test_special_comment_quoted_text_is_not_a_binding_directive() -> None:
    assert _sqlaero._bind_sql("SELECT /*! '%s' */ %s", [7]) == [b"\x03SELECT /*! '%s' */ 7"]


def test_multibyte_trail_backslash_does_not_escape_the_quote_boundary() -> None:
    assert _sqlaero._bind_sql("SELECT 'ソ',%s", [7], charset="sjis") == [b"\x03SELECT '\x83\x5c',7"]


def test_shared_container_references_are_not_cycles() -> None:
    shared = [1, 2]
    assert _sqlaero._bind_sql("SELECT %s,%s", [shared, shared]) == [b"\x03SELECT (1,2),(1,2)"]


def test_true_cycles_and_source_paths_are_preserved() -> None:
    cycle: list[object] = []
    cycle.append(cycle)
    with pytest.raises(DataError, match="args\\[0\\]") as caught:
        _sqlaero._bind_sql("SELECT %s", cycle)
    assert caught.value.code == 65543
    with pytest.raises(DataError):
        _sqlaero._bind_sql("SELECT %s", cycle, many=True)


def test_nesting_counts_the_root_batch_and_row_roles() -> None:
    value: object = 1
    for _ in range(64):
        value = [value]
    assert _sqlaero._bind_sql("SELECT %s", value, itemize=False)[0] == b"\x03SELECT " + b"(" * 64 + b"1" + b")" * 64
    with pytest.raises(DataError):
        _sqlaero._bind_sql("SELECT %s", [value], itemize=False)
    with pytest.raises(DataError):
        _sqlaero._bind_sql("SELECT %s", [value], many=True, itemize=False)


class ObservableZone(tzinfo):
    """Count callbacks to prove width errors precede scalar conversion."""

    def __init__(self) -> None:
        self.calls = 0

    def utcoffset(self, dt: datetime | None) -> timedelta | None:
        self.calls += 1
        return timedelta(0)


def test_width_mismatch_does_not_run_approved_conversion_callbacks() -> None:
    zone = ObservableZone()
    with pytest.raises(ProgrammingError) as caught:
        _sqlaero._bind_sql("SELECT %s,%s", [datetime(2024, 1, 1, tzinfo=zone)])
    assert caught.value.code == 65542
    assert zone.calls == 0


def test_whole_operation_deadline_is_not_an_extra_sampling_timeout() -> None:
    with pytest.raises(OperationalTimeoutError) as caught:
        _sqlaero._bind_sql("SELECT %s", [1], timeout_ns=0)
    assert caught.value.code == 65544


def test_sql_encoding_error_keeps_the_original_sql_character_position() -> None:
    source = "SELECT 'Café😀',%s"
    with pytest.raises(UnicodeEncodeError) as caught:
        _sqlaero._bind_sql(source, [1], charset="latin1")
    assert caught.value.object is source
    assert (caught.value.encoding, caught.value.start, caught.value.end) == ("latin1", 12, 13)


@pytest.mark.parametrize("source", ["中" * 200000, bytes(range(256)) * 2000, bytearray(bytes(range(256)) * 2000)])
def test_large_scalar_fragments_keep_exact_contents(source: str | bytes | bytearray) -> None:
    literal = _sqlaero._encode_scalar(source)
    assert _sqlaero._bind_sql("SELECT %s", [source]) == [b"\x03SELECT " + literal]


def test_large_binding_checks_the_original_operation_deadline() -> None:
    source = "'" * (16 * 1024 * 1024)
    with pytest.raises(OperationalTimeoutError):
        _sqlaero._bind_sql("SELECT %s", [source], timeout_ns=5_000_000)


class InterruptedBinding(Exception):
    """An actual user signal exception, preserved without library wrapping."""


@pytest.mark.skipif(not hasattr(signal, "setitimer"), reason="POSIX signal fixture")
def test_large_binding_returns_to_python_signal_checks() -> None:
    source = "'" * (16 * 1024 * 1024)

    def interrupt(signum: int, frame: FrameType | None) -> None:
        raise InterruptedBinding("original signal handler")

    previous = signal.signal(signal.SIGALRM, interrupt)
    try:
        signal.setitimer(signal.ITIMER_REAL, 0.005)
        with pytest.raises(InterruptedBinding, match="original signal handler"):
            _sqlaero._bind_sql("SELECT %s", [source])
    finally:
        signal.setitimer(signal.ITIMER_REAL, 0)
        signal.signal(signal.SIGALRM, previous)
