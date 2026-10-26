"""Complete VALUES blocks from the same production lexical and value paths."""

# pyright: reportPrivateUsage=false

from datetime import datetime, timedelta, tzinfo
import os
import subprocess

import pytest
from sqlaero import _sqlaero
from sqlaero._sqlaero import OperationalError, ProgrammingError
from tools.mysql_fixture import mysql_command


@pytest.mark.parametrize("sql", [
    "INSERT INTO t VALUES (%s,%s)",
    "INSERT LOW_PRIORITY IGNORE INTO `db`.`t` (`x`,`y`) VALUE (%s,%s)",
    "REPLACE DELAYED t(x,y) VALUES (%s,%s)",
])
def test_approved_headers_merge_complete_rows(sql: str) -> None:
    expected = sql.replace("(%s,%s)", "(1,'A'),(2,'B')").encode()
    assert _sqlaero._bind_sql(sql, [(1, "A"), (2, "B")], many=True) == [b"\x03" + expected]


def test_fixed_suffix_is_preserved_once_per_complete_request() -> None:
    sql = "INSERT t VALUES (%s,%s) AS new(x,y) ON DUPLICATE KEY UPDATE y=CONCAT(new.y,'VALUES(%s)'),x=10%%3; # tail"
    prefix = b"\x03INSERT t VALUES "
    suffix = b" AS new(x,y) ON DUPLICATE KEY UPDATE y=CONCAT(new.y,'VALUES(%s)'),x=10%3; # tail"
    rows = [(1, "A"), (2, "B"), (3, "C")]
    limit = len(prefix + b"(1,'A'),(2,'B')" + suffix)
    assert _sqlaero._bind_sql(sql, rows, many=True, limit=limit) == [
        prefix + b"(1,'A'),(2,'B')" + suffix,
        prefix + b"(3,'C')" + suffix,
    ]


@pytest.mark.parametrize(("sql", "args", "expected"), [
    ("INSERT t VALUES %s", [(1, "A"), (2, "B")], [b"\x03INSERT t VALUES (1,'A')", b"\x03INSERT t VALUES (2,'B')"]),
    ("INSERT t VALUES (1,%s)", [7, 8], [b"\x03INSERT t VALUES (1,7)", b"\x03INSERT t VALUES (1,8)"]),
    ("INSERT t VALUES (%s) ON DUPLICATE KEY UPDATE x=%s", [(1, 7), (2, 8)], [b"\x03INSERT t VALUES (1) ON DUPLICATE KEY UPDATE x=7", b"\x03INSERT t VALUES (2) ON DUPLICATE KEY UPDATE x=8"]),
])
def test_outside_merge_scope_keeps_ordinary_per_row_binding(sql: str, args: object, expected: list[bytes]) -> None:
    itemize = not sql.endswith("VALUES %s")
    assert _sqlaero._bind_sql(sql, args, many=True, itemize=itemize) == expected


def test_hard_bound_uses_encoded_and_escaped_complete_bytes() -> None:
    sql = "INSERT t VALUES (%s)"
    expected = b"\x03INSERT t VALUES ('" + "中文".encode() + b"\\\'')"
    assert _sqlaero._bind_sql(sql, ["中文'"], many=True, limit=len(expected)) == [expected]
    with pytest.raises(OperationalError):
        _sqlaero._bind_sql(sql, ["中文'"], many=True, limit=len(expected) - 1)
    assert _sqlaero._bind_sql("INSERT t () VALUES ()", [(), ()], many=True) == [b"\x03INSERT t () VALUES (),()"]
    with pytest.raises(ProgrammingError):
        _sqlaero._bind_sql(sql, [], many=True)


class CountingZone(tzinfo):
    """Record observable conversion calls at a normal block boundary."""

    def __init__(self) -> None:
        self.calls = 0

    def utcoffset(self, dt: datetime | None) -> timedelta:
        self.calls += 1
        return timedelta(hours=1)


def test_normal_rollover_reuses_the_encoded_row_without_timezone_callbacks() -> None:
    zone = CountingZone()
    date = datetime(2024, 1, 2, 3, 4, 5, tzinfo=zone)
    expected = b"\x03INSERT t VALUES ('2024-01-02 02:04:05')"
    assert _sqlaero._bind_sql("INSERT t VALUES (%s)", [date, date, date], many=True, limit=len(expected)) == [expected] * 3
    assert zone.calls == 3


def test_later_bad_row_aborts_the_probe_instead_of_returning_a_success_prefix() -> None:
    with pytest.raises(TypeError):
        _sqlaero._bind_sql("INSERT t VALUES (%s)", [1, 2, object()], many=True)


@pytest.mark.skipif(os.environ.get("SQLAERO_NATIVE_MYSQL") != "1", reason="explicit isolated native MySQL fixture")
@pytest.mark.parametrize("label", ["84", "97"])
def test_native_mysql_executes_merged_rows_and_reports_actual_counts(label: str) -> None:
    statements = [b"CREATE TEMPORARY TABLE sqlaero_batch (id INT PRIMARY KEY, name VARCHAR(100));"]
    statements.extend(request[1:] + b";" for request in _sqlaero._bind_sql(
        "INSERT LOW_PRIORITY IGNORE INTO sqlaero_batch(id,name) VALUES (%s,%s)",
        [(1, "Ada"), (2, "Lin"), (3, "中文'\\")], many=True,
    ))
    statements.append(b"SELECT ROW_COUNT();")
    statements.extend(request[1:] + b";" for request in _sqlaero._bind_sql(
        "INSERT INTO sqlaero_batch(id,name) VALUES (%s,%s) AS new(x,y) ON DUPLICATE KEY UPDATE name=new.y",
        [(1, "NEW"), (2, "NEXT")], many=True,
    ))
    statements.append(b"SELECT ROW_COUNT();")
    statements.extend(request[1:] + b";" for request in _sqlaero._bind_sql(
        "REPLACE INTO sqlaero_batch(id,name) VALUE (%s,%s)", [(1, "A"), (2, "B")], many=True,
    ))
    statements.extend([b"SELECT ROW_COUNT();", b"SELECT id,HEX(name) FROM sqlaero_batch ORDER BY id;"])
    result = subprocess.run(mysql_command(label) + ["--default-character-set=utf8mb4", "sqlaero_test"],
                            input=b"\n".join(statements), capture_output=True, check=False)
    assert result.returncode == 0, result.stderr.decode(errors="replace")
    expected = ["3", "4", "4", "1\t41", "2\t42", "3\t" + "中文'\\".encode().hex().upper()]
    assert result.stdout.decode().splitlines() == expected
