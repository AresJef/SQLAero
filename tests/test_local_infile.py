"""Installed-wheel LOCAL authorization against the owned MySQL fixtures.

Deterministic after-open mutation and abnormal peer cases also have existing
native coverage: local_infile.rs tests same-handle rename, symlinks, fixed opening
length, APPEND/early EOF and FIFO rejection; local_infile_protocol.rs tests
unrequested/duplicate 0xfb, NULL rows and partial upload facts. Those tests use
the same Source/Upload/Query path and are not replaced by timing races here.
No public LOCAL PASS follows from those native tests alone.
"""

from __future__ import annotations

from collections.abc import Iterator
from decimal import Decimal
from enum import IntEnum
import io
import os
from pathlib import Path
from typing import TYPE_CHECKING
from uuid import uuid4

import pytest
from sqlaero import Connection
from sqlaero.errors import InterfaceError, SQLAeroError
from test_public_connection import live, requires_mysql
from tools.mysql_fixture import mysql_command, run as fixture_command

if TYPE_CHECKING:
    from sqlaero import Cursor, DictCursor, SSCursor, SSDictCursor
    from sqlaero.typing import PathInput


def admin(label: str, sql: str) -> str:
    """Reuse the private fixture's credential-redacting administration bridge."""
    return fixture_command(mysql_command(label), data=sql).strip()


@pytest.fixture(scope='module', params=['84', '97'])
def local_server(request: pytest.FixtureRequest) -> Iterator[str]:
    """Temporarily enable LOCAL only on the explicitly selected owned instance."""
    label = str(request.param)
    original = admin(label, 'SELECT @@GLOBAL.local_infile')
    assert original in {'0', '1'}
    try:
        admin(label, 'SET GLOBAL local_infile=ON')
        yield label
    finally:
        admin(label, f'SET GLOBAL local_infile={original}')
        assert admin(label, 'SELECT @@GLOBAL.local_infile') == original


def cursor_for(connection: Connection, kind: str) -> Cursor | DictCursor | SSCursor | SSDictCursor:
    """Use the four public factory selections, without a LOCAL-specific Cursor."""
    if kind == 'tuple':
        return connection.cursor()
    if kind == 'dict':
        return connection.cursor(dict)
    if kind == 'ss-tuple':
        return connection.cursor(streaming=True)
    assert kind == 'ss-dict'
    return connection.cursor(dict, streaming=True)


def load_sql(table: str = 'local_rows') -> str:
    """The filename marker binds a SQL label and never the actual source path."""
    return f'LOAD DATA LOCAL INFILE %s INTO TABLE {table} CHARACTER SET utf8mb4'


def make_table(connection: Connection, *, binary: bool = False) -> None:
    with connection.cursor() as setup:
        setup.execute('CREATE TEMPORARY TABLE local_rows '
                      f'(id INT PRIMARY KEY, txt {"VARBINARY(80)" if binary else "VARCHAR(80)"})')


def saved_result(current: Cursor | DictCursor) -> object:
    """Leave a buffered row unread so a preflight failure must preserve it."""
    assert current.execute('SELECT 41 AS saved') == 1
    return current.columns


def assert_saved(current: Cursor | DictCursor, columns: object, dictionary: bool) -> None:
    assert current.columns is columns
    assert current.row_count == 1
    assert current.fetchone() == ({'saved': 41} if dictionary else (41,))


class OncePath(os.PathLike[str]):
    """A user path protocol is resolved once and is not a file reader callback."""

    def __init__(self, path: Path) -> None:
        self.path = path
        self.calls = 0

    def __fspath__(self) -> str:
        self.calls += 1
        assert self.calls == 1, 'local_file must not reopen or reconvert the Python path'
        return str(self.path)


class OnceBytesPath(os.PathLike[bytes]):
    """PathLike returning filesystem bytes follows the existing PathInput API."""

    def __init__(self, path: Path) -> None:
        self.path = path
        self.calls = 0

    def __fspath__(self) -> bytes:
        self.calls += 1
        assert self.calls == 1
        return os.fsencode(self.path)


class BrokenPath(os.PathLike[str]):
    def __init__(self, error: RuntimeError) -> None:
        self.error = error
        self.calls = 0

    def __fspath__(self) -> str:
        self.calls += 1
        raise self.error


class IntegerFlag(IntEnum):
    ONE = 1


class IntegerSubclass(int):
    """Not an exact bool even when its value is one."""


class BooleanTrap:
    def __bool__(self) -> bool:
        raise AssertionError('local_infile must not coerce truth values')


@requires_mysql
def test_capability_defaults_strict_bool_and_read_only_snapshot(local_server: str) -> None:
    choices: tuple[tuple[dict[str, bool], bool], ...] = (
        ({}, False), ({'local_infile': False}, False), ({'local_infile': True}, True),
    )
    for options, expected in choices:
        before = admin(local_server, 'SELECT @@GLOBAL.local_infile')
        with live(local_server, **options) as connection:
            assert connection.local_infile is expected
            with pytest.raises(AttributeError):
                setattr(connection, 'local_infile', not expected)
        assert connection.local_infile is expected
        with pytest.raises(AttributeError):
            setattr(connection, 'local_infile', not expected)
        assert admin(local_server, 'SELECT @@GLOBAL.local_infile') == before
    invalid_flags: tuple[object, ...] = (
        None, 0, 1, -1, 1.0, 'true', [], {}, IntegerFlag.ONE, IntegerSubclass(1), BooleanTrap(),
    )
    for invalid in invalid_flags:
        with pytest.raises(TypeError):
            live(local_server, local_infile=invalid)


@requires_mysql
@pytest.mark.parametrize('kind', ['tuple', 'dict', 'ss-tuple', 'ss-dict'])
@pytest.mark.parametrize('source_form', ['str', 'path', 'pathlike', 'bytes-pathlike'])
def test_only_the_explicit_source_is_imported_and_all_four_cursors_publish_nonrow_state(
    local_server: str, tmp_path: Path, kind: str, source_form: str,
) -> None:
    source = tmp_path / '授权-🙂.tsv'
    source.write_bytes('1\tAda\n2\t中文🙂\n'.encode())
    decoy = tmp_path / '服务器请求名.tsv'
    decoy.write_bytes(b'99\twrong-source\n')
    authorized: PathInput
    if source_form == 'str':
        authorized = str(source)
    elif source_form == 'path':
        authorized = source
    elif source_form == 'pathlike':
        authorized = OncePath(source)
    else:
        authorized = OnceBytesPath(source)
    with live(local_server, local_infile=True) as connection:
        make_table(connection)
        with cursor_for(connection, kind) as current:
            assert current.execute(load_sql() + '; /* ; ? %s protected */',
                                   (str(decoy),), local_file=authorized) == 2
            assert current.row_count == current.rowcount == current.affected_rows == 2
            assert current.batch_row_count == 2
            assert current.warning_count == 0
            assert current.lastrowid is current.last_row_id is None
            assert current.description is None
            assert len(current.columns) == 0
            with pytest.raises(InterfaceError):
                current.fetchone()
            with pytest.raises(InterfaceError):
                current.fetchall()
            # MySQL returns an additional empty success for the preserved
            # post-semicolon comment. Real nonrow statuses remain public.
            assert current.nextset() is True
            assert current.description is None and current.row_count == 0
            assert current.batch_row_count == 2
            assert current.nextset() is None
        if isinstance(authorized, (OncePath, OnceBytesPath)):
            assert authorized.calls == 1
        with connection.cursor() as check:
            assert check.execute('SELECT id,txt FROM local_rows ORDER BY id') == 2
            assert check.fetchall() == ((1, 'Ada'), (2, '中文🙂'))
            assert check.execute('SELECT NULL') == 1
            assert check.fetchone() == (None,)


@requires_mysql
def test_empty_file_and_unrelated_legal_sql_label_use_the_actual_final_ok(
    local_server: str, tmp_path: Path,
) -> None:
    source = tmp_path / 'empty.tsv'
    source.write_bytes(b'')
    with live(local_server, local_infile=True) as connection:
        make_table(connection)
        with connection.cursor() as current:
            assert current.execute(load_sql(), ('orders; ? %s 中文.csv',), local_file=source) == 0
            assert current.row_count == current.rowcount == current.batch_row_count == 0
            assert current.warning_count == 0
            assert current.description is None
            with pytest.raises(InterfaceError):
                current.fetchall()
            assert current.execute('SELECT COUNT(*) FROM local_rows') == 1
            assert current.fetchone() == (0,)


@requires_mysql
def test_actual_server_counts_and_warnings_are_not_inferred_from_file_lines(
    local_server: str, tmp_path: Path,
) -> None:
    source = tmp_path / 'duplicate.tsv'
    source.write_bytes(b'1\tfirst\n1\tduplicate\n2\tsecond\n')
    with live(local_server, local_infile=True) as connection:
        make_table(connection)
        with connection.cursor() as current:
            sql = load_sql().replace(' INTO TABLE ', ' IGNORE INTO TABLE ')
            assert current.execute(sql, ('duplicate-rows',), local_file=source) == 2
            assert current.row_count == current.batch_row_count == 2
            assert current.warning_count == 1
            current.execute('SELECT id,txt FROM local_rows ORDER BY id')
            assert current.fetchall() == ((1, 'first'), (2, 'second'))


@requires_mysql
def test_ordinary_symlink_resolution_is_allowed(local_server: str, tmp_path: Path) -> None:
    source = tmp_path / 'target.tsv'
    source.write_bytes(b'1\tlinked-source\n')
    link = tmp_path / 'link.tsv'
    try:
        link.symlink_to(source)
    except OSError as error:
        pytest.skip(f'platform does not permit creating this test symlink: {error}')
    with live(local_server, local_infile=True) as connection:
        make_table(connection)
        with connection.cursor() as current:
            assert current.execute(load_sql(), ('not-the-link',), local_file=link) == 1
            current.execute('SELECT id,txt FROM local_rows')
            assert current.fetchall() == ((1, 'linked-source'),)


@requires_mysql
def test_large_file_is_chunked_beyond_message_and_result_limits(
    local_server: str, tmp_path: Path,
) -> None:
    source = tmp_path / 'large.tsv'
    source.write_text(''.join(f'{index}\trow-{index}-中文🙂\n' for index in range(1, 20_001)), encoding='utf-8')
    assert source.stat().st_size > 256 * 1024
    with live(local_server, local_infile=True, max_allowed_packet=1024,
              result_memory_limit=64 * 1024, total_result_memory_limit=256 * 1024) as connection:
        make_table(connection)
        with connection.cursor() as current:
            assert current.execute(load_sql(), ('chunked-label',), local_file=source) == 20_000
            assert current.row_count == current.batch_row_count == 20_000
            assert current.warning_count == 0
            current.execute('SELECT COUNT(*),MIN(id),MAX(id),SUM(txt=CONCAT(\'row-\',id,\'-中文🙂\')) FROM local_rows')
            assert current.fetchone() == (20_000, 1, 20_000, Decimal(20_000))


@requires_mysql
@pytest.mark.parametrize('dictionary', [False, True])
@pytest.mark.parametrize(('capability', 'provide_source'), [(False, False), (False, True), (True, False)])
def test_missing_authority_is_a_local_preflight_failure_preserving_old_results(
    local_server: str, tmp_path: Path, dictionary: bool, capability: bool, provide_source: bool,
) -> None:
    source = tmp_path / 'data.tsv'
    source.write_bytes(b'1\tapproved\n')
    with live(local_server, local_infile=capability) as connection:
        make_table(connection)
        with connection.cursor(dict if dictionary else tuple) as current:
            columns = saved_result(current)
            with pytest.raises(SQLAeroError) as caught:
                current.execute(load_sql(), ('no-implicit-source',),
                                local_file=source if provide_source else None)
            assert caught.value.is_server_error is False
            assert 'local' in str(caught.value).lower()
            assert_saved(current, columns, dictionary)
            assert not connection.closed


@requires_mysql
@pytest.mark.parametrize('dictionary', [False, True])
@pytest.mark.parametrize('bad_source', ['missing', 'directory', 'protocol-error'])
def test_source_preparation_errors_preserve_old_result_and_original_python_exception(
    local_server: str, tmp_path: Path, dictionary: bool, bad_source: str,
) -> None:
    original = RuntimeError('the authorized test path protocol failed')
    source: PathInput = tmp_path / 'missing.tsv'
    if bad_source == 'directory':
        source = tmp_path
    elif bad_source == 'protocol-error':
        source = BrokenPath(original)
    with live(local_server, local_infile=True) as connection:
        make_table(connection)
        with connection.cursor(dict if dictionary else tuple) as current:
            columns = saved_result(current)
            if bad_source == 'protocol-error':
                with pytest.raises(RuntimeError) as caught:
                    current.execute(load_sql(), ('source-error',), local_file=source)
                assert caught.value is original
                assert isinstance(source, BrokenPath) and source.calls == 1
            else:
                # Local source preparation shares the existing standard-I/O
                # boundary, including errno-specific OSError subclasses.
                with pytest.raises(OSError) as caught_native:
                    current.execute(load_sql(), ('source-error',), local_file=source)
                assert any('local_file' in note and 'not been sent' in note
                           for note in caught_native.value.__notes__)
            assert_saved(current, columns, dictionary)
            assert not connection.closed


@requires_mysql
def test_local_file_rejects_nonpaths_and_never_accepts_python_file_readers(
    local_server: str, tmp_path: Path,
) -> None:
    source = tmp_path / 'valid.tsv'
    source.write_bytes(b'1\tvalid\n')
    with source.open('rb') as file_object, io.BytesIO(b'1\tin-memory\n') as memory_source:
        invalid_sources: tuple[object, ...] = (True, False, os.fsencode(source), 0, 1.5,
                                             memory_source, file_object, iter([b'contents']))
        with live(local_server, local_infile=True) as connection:
            make_table(connection)
            with connection.cursor() as current:
                for invalid in invalid_sources:
                    columns = saved_result(current)
                    with pytest.raises(TypeError):
                        getattr(current, 'execute')(load_sql(), ('invalid-source',), local_file=invalid)
                    assert_saved(current, columns, False)
                    assert not connection.closed


@requires_mysql
@pytest.mark.parametrize('combination', ['many', 'multi-sql', 'select', 'server-file', 'prepared', 'special-comment'])
def test_upload_is_only_one_plain_text_local_load(
    local_server: str, tmp_path: Path, combination: str,
) -> None:
    source = tmp_path / 'valid.tsv'
    source.write_bytes(b'1\tvalid\n')
    with live(local_server, local_infile=True) as connection:
        make_table(connection)
        with connection.cursor(prepared=combination == 'prepared') as current:
            columns = saved_result(current)
            sql = load_sql()
            args: tuple[object, ...] | list[tuple[object, ...]] = ('disallowed-combination',)
            if combination == 'multi-sql':
                sql += '; SELECT 1'
            elif combination == 'select':
                sql, args = 'SELECT 1', ()
            elif combination == 'server-file':
                sql = sql.replace('DATA LOCAL INFILE', 'DATA INFILE')
            elif combination == 'special-comment':
                sql, args = "/*!80000 LOAD DATA LOCAL INFILE 'hidden-label' INTO TABLE local_rows */", ()
            elif combination == 'many':
                args = [('disallowed-combination',), ('disallowed-combination',)]
            with pytest.raises(SQLAeroError) as caught:
                current.execute(sql, args, many=combination == 'many', local_file=source)
            assert caught.value.is_server_error is False
            assert any(word in str(caught.value).lower() for word in ('local', 'upload', 'file'))
            assert_saved(current, columns, False)
            assert not connection.closed
            with connection.cursor() as check:
                check.execute('SELECT COUNT(*) FROM local_rows')
                assert check.fetchone() == (0,)


@requires_mysql
def test_source_keyword_is_execute_only_and_not_inherited_by_the_next_call(
    local_server: str, tmp_path: Path,
) -> None:
    source = tmp_path / 'valid.tsv'
    source.write_bytes(b'1\tvalid\n')
    with live(local_server, local_infile=True) as connection:
        make_table(connection)
        with connection.cursor() as current:
            unopened = OncePath(source)
            for method in ('executemany', 'mogrify'):
                with pytest.raises(TypeError):
                    getattr(current, method)(load_sql(), [('label',)], local_file=unopened)
                assert unopened.calls == 0
            assert current.execute(load_sql(), ('first-label',), local_file=source) == 1
            columns = saved_result(current)
            with pytest.raises(SQLAeroError) as caught:
                current.execute(load_sql(), ('second-label',))
            assert caught.value.is_server_error is False
            assert_saved(current, columns, False)
            assert not connection.closed
            current.execute('SELECT COUNT(*) FROM local_rows')
            assert current.fetchone() == (1,)


@requires_mysql
def test_server_global_refusal_is_a_real_server_error_and_keeps_a_synchronized_connection(
    local_server: str, tmp_path: Path,
) -> None:
    source = tmp_path / 'data.tsv'
    source.write_bytes(b'1\tnot-imported\n')
    try:
        with live(local_server, local_infile=True) as connection:
            assert connection.local_infile is True
            make_table(connection)
            admin(local_server, 'SET GLOBAL local_infile=OFF')
            assert admin(local_server, 'SELECT @@GLOBAL.local_infile') == '0'
            with connection.cursor() as current:
                with pytest.raises(SQLAeroError) as caught:
                    current.execute(load_sql(), ('server-refusal',), local_file=source)
                assert caught.value.is_server_error is True
                code = caught.value.code
                assert code is not None and 0 < code < 65536
                assert 'local' in str(caught.value).lower()
                assert not connection.closed
                assert current.execute('SELECT COUNT(*) FROM local_rows') == 1
                assert current.fetchone() == (0,)
    finally:
        admin(local_server, 'SET GLOBAL local_infile=ON')


@requires_mysql
def test_unsolicited_real_request_cannot_read_even_an_existing_test_canary(
    local_server: str, tmp_path: Path,
) -> None:
    canary = tmp_path / 'never-authorized.tsv'
    canary.write_bytes(b'73\tprivate-test-canary\n')
    table = 'local_canary_' + uuid4().hex
    with live(local_server, local_infile=True, sql_mode='') as connection:
        with connection.cursor() as setup:
            setup.execute(f'CREATE TABLE {table}(id INT, txt VARCHAR(80))')
        try:
            # No local_file is supplied. The existing no-args text path leaves
            # executable comments to MySQL; a real 0xfb must never authorize IO.
            label = canary.as_posix().replace("'", "''")
            sql = f"/*!80000 LOAD DATA LOCAL INFILE '{label}' INTO TABLE {table} */"
            with connection.cursor() as current:
                with pytest.raises(SQLAeroError) as caught:
                    current.execute(sql)
                assert caught.value.is_server_error is False
                assert connection.closed
            with live(local_server) as observer, observer.cursor() as check:
                check.execute(f'SELECT COUNT(*) FROM {table}')
                assert check.fetchone() == (0,)
            assert canary.read_bytes() == b'73\tprivate-test-canary\n'
        finally:
            with live(local_server) as cleanup, cleanup.cursor() as drop:
                drop.execute(f'DROP TABLE IF EXISTS {table}')


@requires_mysql
def test_post_upload_trigger_error_is_not_relabelled_as_a_local_read_failure(
    local_server: str, tmp_path: Path,
) -> None:
    source = tmp_path / 'trigger.tsv'
    source.write_bytes(b'1\tone\n2\ttwo\n')
    table = 'local_trigger_' + uuid4().hex
    with live(local_server, local_infile=True) as connection:
        with connection.cursor() as setup:
            setup.execute(f'CREATE TABLE {table}(id INT, txt VARCHAR(80))')
        try:
            admin(local_server, f'CREATE TRIGGER sqlaero_test.{table}_error BEFORE INSERT '
                  f"ON sqlaero_test.{table} FOR EACH ROW SIGNAL SQLSTATE '45000' "
                  "SET MESSAGE_TEXT='actual public LOCAL trigger failure'")
            with connection.cursor() as current:
                with pytest.raises(SQLAeroError) as caught:
                    current.execute(load_sql(table), ('trigger-label',), local_file=source)
                assert caught.value.is_server_error is True and caught.value.code == 1644
                assert 'actual public LOCAL trigger failure' in caught.value.message
                assert not connection.closed
                assert current.execute('SELECT 9') == 1
                assert current.fetchone() == (9,)
        finally:
            with live(local_server) as cleanup_connection, cleanup_connection.cursor() as cleanup:
                cleanup.execute(f'DROP TABLE IF EXISTS {table}')
