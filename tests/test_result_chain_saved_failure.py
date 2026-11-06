"""R05 retained boundaries preserve frame-free original error facts."""

from contextlib import closing
from types import FrameType
from typing import Self, cast
import gc
import weakref

import pytest
from _native_signal import NativeAlarm
from sqlaero import Connection
from sqlaero.errors import InterfaceError, ProgrammingError, SQLAeroError, SQLAeroErrorCode

from test_public_connection import connection as connection, live, requires_mysql


@requires_mysql
def test_retained_unicode_encode_error_preserves_structure_and_notes() -> None:
    with live('84', charset='latin1') as client, client.cursor() as cur:
        with pytest.raises(UnicodeEncodeError) as immediate:
            cur.executemany('SELECT %s AS trusted', [('ok',), ('\U0001f600',)])
        original = immediate.value
        expected = (original.encoding, original.object, original.start,
                    original.end, original.reason)
        notes = tuple(original.__notes__)
        assert cur.fetchall() == (('ok',),) and cur.batch_row_count is None
        for _ in range(2):
            with pytest.raises(UnicodeEncodeError) as boundary:
                cur.nextset()
            error = boundary.value
            assert type(error) is type(original)
            assert (error.encoding, error.object, error.start, error.end, error.reason) == expected
            assert tuple(error.__notes__) == notes


@requires_mysql
def test_retained_native_error_keeps_execution_and_position_notes(connection: Connection) -> None:
    with connection.cursor() as cur:
        with pytest.raises(Exception) as immediate:
            cur.execute('SELECT 7 AS trusted; SELECT * FROM sqlaero_absent_saved_failure')
        original = cast(SQLAeroError, immediate.value)
        assert original.code == 1146
        notes = tuple(original.__notes__)
        assert any('request 0, result set 1' in note for note in notes)
        assert cur.fetchall() == ((7,),) and cur.batch_row_count == 1
        for _ in range(2):
            with pytest.raises(type(original)) as boundary:
                cur.nextset()
            error = boundary.value
            assert error.code == original.code and error.message == original.message
            assert error.is_server_error is True
            assert tuple(error.__notes__) == notes


@requires_mysql
def test_retained_signal_type_does_not_call_user_constructors(connection: Connection) -> None:
    forbidden = False

    class Sentinel(LookupError):
        def __new__(cls, *args: object) -> Self:
            assert not forbidden, 'retained failure called user __new__'
            return super().__new__(cls, *args)

        def __init__(self, *args: object) -> None:
            assert not forbidden, 'retained failure called user __init__'
            super().__init__(*args)

    original = Sentinel('retained signal origin')

    def interrupt(_number: int, _frame: FrameType | None) -> None:
        raise original

    with closing(NativeAlarm(interrupt)) as alarm:
        with connection.cursor() as cur:
            alarm.start(0.1)
            with pytest.raises(Sentinel) as immediate:
                cur.execute('SELECT 11 AS trusted; DO SLEEP(1)')
            alarm.cancel()
            assert immediate.value is original
            notes = tuple(original.__notes__)
            forbidden = True
            assert cur.fetchall() == ((11,),)
            for _ in range(2):
                with pytest.raises(Sentinel) as boundary:
                    cur.nextset()
                assert str(boundary.value) == str(original)
                assert tuple(boundary.value.__notes__) == notes


@requires_mysql
def test_oversized_unicode_object_reports_retained_diagnostic_limit() -> None:
    text = 'a' * 2000 + '\U0001f600'
    with live('84', charset='latin1') as client, client.cursor() as cur:
        with pytest.raises(UnicodeEncodeError) as immediate:
            cur.executemany('SELECT %s AS trusted', [('ok',), (text,)])
        assert immediate.value.object == text
        assert (immediate.value.start, immediate.value.end) == (2000, 2001)
        assert cur.fetchall() == (('ok',),)
        for _ in range(2):
            with pytest.raises(InterfaceError) as boundary:
                cur.nextset()
            error = boundary.value
            assert error.code == SQLAeroErrorCode.SESSION_STATE
            assert 'UnicodeEncodeError' in error.message
            assert 'local diagnostic limitation, not a new database failure' in error.message
            assert 'Unicode diagnostic text exceeds' in error.message
            assert "encoding='latin1', start=2000, end=2001" in error.message
            assert 'not representable in MySQL latin1' in error.message
            assert any('confirmed 1 commands' in note for note in error.__notes__)


@requires_mysql
def test_retained_custom_state_is_not_kept_alive(connection: Connection) -> None:
    cur = connection.cursor()

    class Payload:
        pass

    class Sentinel(LookupError):
        pass

    def fail() -> weakref.ReferenceType[Payload]:
        payload = Payload()
        error = Sentinel('state requires the original object')
        # This custom instance state is intentionally outside the exception API.
        setattr(error, 'payload', payload)

        def interrupt(_number: int, _frame: FrameType | None) -> None:
            raise error

        with closing(NativeAlarm(interrupt)) as alarm:
            alarm.start(0.1)
            with pytest.raises(Sentinel) as immediate:
                cur.execute('SELECT 13 AS trusted; DO SLEEP(1)')
            assert getattr(immediate.value, 'payload') is payload
            return weakref.ref(payload)

    reference = fail()
    gc.collect()
    assert reference() is None
    assert cur.fetchall() == ((13,),)
    with pytest.raises(InterfaceError, match='custom exception attributes') as boundary:
        cur.nextset()
    assert 'state requires the original object' in boundary.value.message
    cur.close()


@requires_mysql
def test_retained_type_cycle_is_visible_to_python_gc(connection: Connection) -> None:
    def cycle() -> weakref.ReferenceType[type[LookupError]]:
        cur = connection.cursor()

        class Sentinel(LookupError):
            owner = cur

        error = Sentinel('type to cursor cycle')

        def interrupt(_number: int, _frame: FrameType | None) -> None:
            raise error

        with closing(NativeAlarm(interrupt)) as alarm:
            alarm.start(0.1)
            with pytest.raises(Sentinel):
                cur.execute('SELECT 17 AS trusted; DO SLEEP(1)')
            return weakref.ref(Sentinel)

    reference = cycle()
    gc.collect()
    assert reference() is None


@requires_mysql
@pytest.mark.parametrize('kind', ['slots', 'display', 'args', 'os-fields', 'exit-code', 'getattribute', 'library'])
def test_unreconstructible_state_is_explicit_at_retained_boundary(connection: Connection, kind: str) -> None:
    class WithSlots(LookupError):
        __slots__ = ('payload',)

    class WithDisplay(LookupError):
        def __str__(self) -> str:
            return 'custom display'

    class WithAccess(LookupError):
        def __getattribute__(self, name: str) -> object:
            if name in ('args', '__cause__', '__context__', '__notes__'):
                raise AssertionError('user exception attribute access must be bypassed')
            return super().__getattribute__(name)

    if kind == 'slots':
        original = WithSlots('slot state')
        original.payload = object()
    elif kind == 'display':
        original = WithDisplay('display state')
    elif kind == 'args':
        original = LookupError(object())
    elif kind == 'os-fields':
        original = OSError(5, 'original IO cause', '/original/path')
    elif kind == 'exit-code':
        original = SystemExit(42)
    elif kind == 'getattribute':
        original = WithAccess('safe intrinsic original reason')
    else:
        original = ProgrammingError('known original database reason', code=1146, is_server_error=True)
        # Deliberate custom state on the otherwise standard library exception.
        setattr(original, 'payload', object())

    def interrupt(_number: int, _frame: FrameType | None) -> None:
        raise original

    with closing(NativeAlarm(interrupt)) as alarm:
        with connection.cursor() as cur:
            alarm.start(0.1)
            with pytest.raises(type(original)) as immediate:
                cur.execute('SELECT 19 AS trusted; DO SLEEP(1)')
            alarm.cancel()
            assert immediate.value is original
            assert cur.fetchall() == ((19,),)
            with pytest.raises(InterfaceError) as boundary:
                cur.nextset()
            assert type(original).__name__ in boundary.value.message
            assert 'local diagnostic limitation, not a new database failure' in boundary.value.message
            if kind == 'getattribute':
                assert 'safe intrinsic original reason' in boundary.value.message
            if kind == 'os-fields':
                assert 'original IO cause' in boundary.value.message
            if kind == 'library':
                assert 'known original database reason' in boundary.value.message
                assert 'original code=Some(1146), is_server_error=Some(true)' in boundary.value.message
            if kind != 'getattribute':
                assert any('result set 1' in note for note in boundary.value.__notes__)
