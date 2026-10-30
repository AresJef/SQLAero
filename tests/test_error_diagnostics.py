"""Immutable diagnostic ownership, real Python chains, and safe display."""

import errno
import gc
import weakref

import pytest
from sqlaero.errors import ErrorInfo, SQLAeroError


def test_manual_snapshot_fields_are_immutable_and_message_stays_lossless() -> None:
    error = SQLAeroError('é\udcff\n\x00', code=123, is_server_error=False)
    info = error.error_info
    assert isinstance(info, ErrorInfo) and error.error_info is info
    assert info.message == error.message and info.code == error.code
    assert info.is_server_error is False and info.exception_type is SQLAeroError
    assert info.stage is None and info.native_type is None and info.os_errno is None
    assert info.cause is None and info.context is None and not info.suppress_context
    assert info.traceback is None and info.notes == () and info.additional_errors == ()
    assert '\n' not in str(info) and '\\n' in str(info) and '\\x00' in str(info)
    assert 'is_server_error=False' in str(info)
    assert repr(info).startswith('ErrorInfo(stage=None, exception_type=sqlaero.errors.SQLAeroError,')
    with pytest.raises(TypeError):
        ErrorInfo()  # type: ignore[call-arg]
    for field in ('stage', 'exception_type', 'native_type', 'message', 'code', 'os_errno',
                  'is_server_error', 'cause', 'context', 'suppress_context', 'traceback',
                  'notes', 'additional_errors'):
        with pytest.raises(AttributeError):
            setattr(info, field, None)


def chained_snapshot(suppressed: bool = False) -> ErrorInfo:
    """Capture a genuine raised chain while the handling frames are active."""
    try:
        raise FileNotFoundError(errno.ENOENT, 'a missing path')
    except OSError as cause:
        try:
            if suppressed:
                raise SQLAeroError('outer') from None
            raise SQLAeroError('outer') from cause
        except SQLAeroError as error:
            error.add_note('captured first')
            info = error.error_info
            error.add_note('later user edit is not watched')
            assert error.error_info is info
            return info


def test_real_explicit_context_shared_node_and_lazy_frozen_traceback() -> None:
    info = chained_snapshot()
    assert info.cause is not None and info.cause is info.context
    assert info.suppress_context
    assert info.cause.exception_type is FileNotFoundError
    assert info.cause.os_errno == errno.ENOENT and info.cause.code is None
    assert info.cause.is_server_error is None
    assert info.notes == ('captured first',)
    rendered = info.traceback
    assert rendered is not None and info.traceback is rendered
    assert 'chained_snapshot' in rendered and 'direct cause' in rendered
    assert rendered.index('FileNotFoundError') < rendered.index('sqlaero.errors.SQLAeroError')
    assert 'captured first' in rendered and 'later user edit' not in rendered
    assert info.additional_errors is info.additional_errors


def test_from_none_keeps_actual_context_but_suppresses_it_in_display() -> None:
    info = chained_snapshot(True)
    assert info.cause is None and info.context is not None and info.suppress_context
    assert info.traceback is not None and 'FileNotFoundError' not in info.traceback


def cycle_snapshot() -> tuple[weakref.ReferenceType[ErrorInfo], weakref.ReferenceType[ErrorInfo]]:
    """Create actual cyclic exception relationships, then release originals."""
    root = SQLAeroError('cycle root')
    cause = ValueError('cycle cause')
    root.__cause__ = cause
    root.__context__ = cause
    cause.__context__ = root
    try:
        raise root
    except SQLAeroError as error:
        info = error.error_info
        assert info.cause is not None and info.cause is info.context
        assert info.cause.context is info
        rendered = info.traceback
        assert rendered is not None and '[cycle in captured exception chain]' in rendered
        assert info.traceback is rendered
        return weakref.ref(info), weakref.ref(info.cause)


def test_real_cycle_is_preserved_display_terminates_and_graph_is_collectible() -> None:
    first, second = cycle_snapshot()
    gc.collect()
    assert first() is None and second() is None


class Payload:
    """A weak-referenceable local that must not be retained by stack capture."""


def snapshot_without_local_ownership() -> tuple[ErrorInfo, weakref.ReferenceType[Payload]]:
    payload = Payload()
    reference = weakref.ref(payload)
    try:
        raise SQLAeroError('no frame locals')
    except SQLAeroError as error:
        return error.error_info, reference


def test_snapshot_retains_no_live_frames_exception_or_local_payload() -> None:
    info, reference = snapshot_without_local_ownership()
    gc.collect()
    assert reference() is None
    assert info.traceback is not None and 'snapshot_without_local_ownership' in info.traceback


def test_distinct_cause_context_and_notes_are_independent() -> None:
    root = SQLAeroError('root')
    root.__cause__ = ValueError('explicit')
    root.__context__ = RuntimeError('background')
    info = root.error_info
    assert info.cause is not None and info.context is not None and info.cause is not info.context
    assert info.cause.message == 'explicit' and info.context.message == 'background'
    assert info.additional_errors == ()
    # Mutating an original later cannot rewrite a published immutable node.
    root.__cause__.args = ('changed',)
    assert info.cause.message == 'explicit'


def test_representative_full_traceback_matches_python_and_summary_is_stable() -> None:
    import traceback
    try:
        raise ValueError('inner value')
    except ValueError as cause:
        try:
            raise SQLAeroError('outer value', code=1213, is_server_error=True) from cause
        except SQLAeroError as error:
            error.add_note('one diagnostic note')
            expected = ''.join(traceback.format_exception(error))
            assert error.error_info.traceback == expected
    plain = SQLAeroError('plain').error_info
    assert str(plain) == 'SQLAeroError [stage=None, code=None, is_server_error=None]: plain'
    assert repr(plain) == ("ErrorInfo(stage=None, exception_type=sqlaero.errors.SQLAeroError, "
                           "native_type=None, code=None, os_errno=None, is_server_error=None, message='plain')")


def test_concurrent_cold_snapshot_reads_return_the_same_published_object() -> None:
    from concurrent.futures import ThreadPoolExecutor
    from threading import Barrier

    capturing = Barrier(2)

    class SharedFailure(SQLAeroError):
        """Make both readers enter capture before either can publish."""

        def __str__(self) -> str:
            capturing.wait(timeout=5)
            return super().__str__()

    error = SharedFailure('shared capture')
    with ThreadPoolExecutor(max_workers=2) as executor:
        first = executor.submit(lambda: error.error_info)
        second = executor.submit(lambda: error.error_info)
        first_info = first.result(timeout=10)
        second_info = second.result(timeout=10)
    assert first_info is second_info and error.error_info is first_info


def test_concurrent_traceback_reads_return_the_same_published_text(monkeypatch: pytest.MonkeyPatch) -> None:
    from concurrent.futures import ThreadPoolExecutor
    from threading import Barrier
    import traceback

    info = chained_snapshot()
    formatting = Barrier(2)
    original = traceback.StackSummary.format

    def synchronized_format(stack: traceback.StackSummary) -> list[str]:
        formatting.wait(timeout=5)
        return original(stack)

    monkeypatch.setattr(traceback.StackSummary, 'format', synchronized_format)
    with ThreadPoolExecutor(max_workers=2) as executor:
        first = executor.submit(lambda: info.traceback)
        second = executor.submit(lambda: info.traceback)
        first_text = first.result(timeout=10)
        second_text = second.result(timeout=10)
    assert first_text is not None and first_text is second_text
    assert info.traceback is first_text


@pytest.mark.parametrize('kind', ['notes', 'metadata', 'retry-rule', 'error-code', 'escape', 'config'])
def test_shared_boundary_tuple_allocation_failure_preserves_memory_error(kind: str) -> None:
    """Exercise final construction and all four integer-input boundaries."""
    from pathlib import Path
    import subprocess
    import sys

    source = r'''
import _testcapi
from collections.abc import Callable
from functools import partial
import sys
from typing import Any
sys.path.insert(0, TEST_HELPERS)
from test_buffered_reader import column, packets
from sqlaero import connect
from sqlaero._sqlaero import _prepare_encoder, _rows_from_packets
from sqlaero.errors import SQLAeroError
from sqlaero.retry import RetryRule

kind: str = KIND
types: tuple[type[Exception], ...] = (Exception,) * 32
codes: tuple[int, ...] = (*range(1000, 1030), 1 << 200, -(1 << 200))
whole: slice = slice(None)
enable = _testcapi.set_nomemory
disable = _testcapi.remove_mem_hooks

class ReadyFailure(SQLAeroError):
    def __str__(self) -> str:
        return 'ready'

def prepare() -> Callable[[], Any]:
    if kind == 'notes':
        error = ReadyFailure('notes')
        error.__notes__ = ['a'] * 32
        return lambda: error.error_info
    if kind == 'metadata':
        columns = _rows_from_packets(packets([column(f'c{index}') for index in range(32)], [])).columns
        get = columns.__getitem__
        return lambda: get(whole)
    if kind == 'error-code':
        return lambda: SQLAeroError('code', code=codes[-1])
    if kind == 'escape':
        encode = _prepare_encoder().encode
        return lambda: encode(codes[-2])
    if kind == 'config':
        # Invoke the C entry directly: an extra Python lambda frame can itself
        # lose an error during CPython's fault-injected frame cleanup.
        return partial(connect, user='unopened', connect_timeout=codes[-2])
    return lambda: RetryRule(error_types=types, error_codes=codes)

def injected(operation: Callable[[], Any], offset: int) -> tuple[bool, Any]:
    try:
        enable(offset, offset + 1)
        try:
            result = operation()
        finally:
            disable()
    except MemoryError:
        return True, None
    except ValueError as error:
        if kind != 'config':
            raise
        return False, error
    return False, result

def verify(result: Any) -> None:
    if kind == 'notes':
        assert result.notes == ('a',) * 32
    elif kind == 'metadata':
        assert len(result) == 32
    elif kind == 'error-code':
        assert result.code == codes[-1] and type(result.code) is int
    elif kind == 'escape':
        assert result.to_bytes() == str(codes[-2]).encode()
    elif kind == 'config':
        assert isinstance(result, ValueError)
        assert 'connect_timeout' in str(result) and 'represented safely' in str(result)
    else:
        assert result.error_types == types and result.error_codes == codes

failures: int = 0
for offset in range(96):
    operation = prepare()
    failed, result = injected(operation, offset)
    if not failed:
        verify(result)
    failures += int(failed)
assert failures > 0
if kind == 'config':
    try:
        prepare()()
    except ValueError as error:
        verify(error)
    else:
        raise AssertionError('oversized timeout did not fail before connecting')
else:
    verify(prepare()())
print('shared boundary allocation failures propagated as MemoryError:', failures)
'''
    source = source.replace('KIND', repr(kind)).replace('TEST_HELPERS', repr(str(Path(__file__).resolve().parent)))
    result = subprocess.run([sys.executable, '-'], input=source, text=True,
                            capture_output=True, timeout=20, check=False)
    assert result.returncode == 0, result.stdout + result.stderr
    assert 'propagated as MemoryError' in result.stdout


def test_unsigned_integer_fallback_preserves_the_original_memory_error() -> None:
    """An expected unsigned overflow must not hide its message-allocation OOM."""
    import subprocess
    import sys

    source = r'''
import _testcapi
from functools import partial
from sqlaero._sqlaero import _prepare_encoder

value = 1 << 200
expected = str(value).encode('ascii')
operation = partial(_prepare_encoder().encode, value)
enable = _testcapi.set_nomemory
disable = _testcapi.remove_mem_hooks
assert operation().to_bytes() == expected
try:
    try:
        # All arguments, native entry and caches are ready. The first Python
        # allocation is the unsigned-conversion overflow message, not a lambda
        # frame or conversion of the final native result to Python bytes.
        enable(0, 1)
        operation()
    finally:
        disable()
except MemoryError:
    pass
else:
    raise AssertionError('unsigned fallback swallowed the allocation failure')
assert operation().to_bytes() == expected
print('unsigned fallback preserved MemoryError and recovered')
'''
    result = subprocess.run([sys.executable, '-'], input=source, text=True,
                            capture_output=True, timeout=20, check=False)
    assert result.returncode == 0, result.stdout + result.stderr
    assert 'preserved MemoryError and recovered' in result.stdout


@pytest.mark.parametrize('number', [-(1 << 300), -(1 << 200), -(1 << 64), -(1 << 31),
                                  -129, -128, -1, 0, 1, 127, 128, 1 << 31,
                                  1 << 64, 1 << 200, 1 << 300])
def test_integer_inputs_keep_signed_precision_and_intenum_normalization(number: int) -> None:
    from enum import IntEnum
    from sqlaero.retry import RetryRule

    class Code(IntEnum):
        ACTUAL = number

    error = SQLAeroError(code=Code.ACTUAL)
    rule = RetryRule(error_codes=Code.ACTUAL)
    codes = rule.error_codes
    assert error.code == number and type(error.code) is int
    assert codes is not None and codes == (number,) and type(codes[0]) is int


@pytest.mark.parametrize('boundary', ['error-code', 'retry-rule'])
def test_integer_subclass_method_errors_keep_the_existing_version_semantics(boundary: str) -> None:
    import sys
    from sqlaero.retry import RetryRule

    original = MemoryError('original integer method failure')

    class Code(int):
        def bit_length(self) -> int:
            raise original

    value = Code(-(1 << 200))

    def construct() -> object:
        return SQLAeroError(code=value) if boundary == 'error-code' else RetryRule(error_codes=value)

    if sys.version_info < (3, 13):
        with pytest.raises(MemoryError) as caught:
            construct()
        assert caught.value is original
    else:
        # The existing native-byte extraction does not call this method.
        result = construct()
        if isinstance(result, SQLAeroError):
            assert result.code == int(value)
        else:
            assert isinstance(result, RetryRule) and result.error_codes == (int(value),)


def test_snapshot_graph_stays_private_until_all_relationships_are_complete() -> None:
    """GC inspection must not expose nodes which capture will still mutate."""
    import subprocess
    import sys

    source = r'''
import gc
from threading import Event, Thread
from typing import Any
from sqlaero.errors import ErrorInfo, SQLAeroError

cause_started, allow_cause = Event(), Event()
reader_borrowed, allow_reader = Event(), Event()
capture_done, watch = Event(), Event()
failures: list[BaseException] = []
finished: list[ErrorInfo] = []

class RootType(type):
    def __getattribute__(cls, name: str) -> Any:
        if name == '__qualname__' and watch.is_set():
            reader_borrowed.set()
            assert allow_reader.wait(5), 'diagnostic reader was not released'
        return super().__getattribute__(name)

class Root(SQLAeroError, metaclass=RootType):
    pass

class Cause(Exception):
    def __str__(self) -> str:
        cause_started.set()
        assert allow_cause.wait(5), 'cause capture was not released'
        return 'private cause'

root = Root('construction-private-root')
root.__cause__ = Cause()

def capture() -> None:
    try:
        finished.append(root.error_info)
    except BaseException as error:
        failures.append(error)
    finally:
        capture_done.set()

thread = Thread(target=capture, daemon=True)
reader: Thread | None = None
thread.start()
try:
    assert cause_started.wait(5), 'capture did not reach its Python cause boundary'
    partial = [value for value in gc.get_objects()
               if type(value) is ErrorInfo and value.message == 'construction-private-root']
    if partial:
        watch.set()
        reader = Thread(target=lambda: repr(partial[0]), daemon=True)
        reader.start()
        assert reader_borrowed.wait(5), 'partial diagnostic could not be observed'
    allow_cause.set()
    assert capture_done.wait(5), 'capture did not complete'
finally:
    allow_cause.set()
    allow_reader.set()
    thread.join(5)
    if reader is not None:
        reader.join(5)
assert not partial, 'GC exposed an ErrorInfo whose relationships were still being written'
assert not failures, [(type(error).__name__, str(error)) for error in failures]
assert len(finished) == 1 and finished[0].cause is not None
assert finished[0].cause.message == 'private cause' and finished[0].context is None
assert gc.is_tracked(finished[0]) and gc.is_tracked(finished[0].cause)
print('snapshot graph published only after all relationship writes')
'''
    result = subprocess.run([sys.executable, '-'], input=source, text=True,
                            capture_output=True, timeout=20, check=False)
    assert result.returncode == 0, result.stdout + result.stderr
    assert 'graph published only after all relationship writes' in result.stdout


def test_failed_snapshot_capture_releases_a_partly_constructed_cycle() -> None:
    class CaptureStopped(Exception):
        """Abort after capture has already formed a root/cause cycle."""

    class BrokenContext(Exception):
        def __getattribute__(self, name: str) -> object:
            if name == '__context__':
                raise CaptureStopped('stop after the first cycle is linked')
            return super().__getattribute__(name)

    def failed_capture_type() -> weakref.ReferenceType[type[SQLAeroError]]:
        class Root(SQLAeroError):
            pass

        root = Root('abandoned cyclic capture')
        cause = ValueError('cycle cause')
        root.__cause__ = cause
        cause.__context__ = root
        root.__context__ = BrokenContext('last pending branch')
        with pytest.raises(CaptureStopped):
            _ = root.error_info
        return weakref.ref(Root)

    reference = failed_capture_type()
    # Clearing the diagnostic graph releases its type reference first; the
    # temporary class's own MRO cycle is eligible in the following collection.
    gc.collect()
    gc.collect()
    assert reference() is None, 'abandoned untracked diagnostic cycle retained its exception type'


def test_transient_relationship_sources_cannot_alias_a_later_exception() -> None:
    """Distinct getter results stay distinct even when an address is recycled."""
    class State:
        def __init__(self) -> None:
            self.address = 0
            self.source: weakref.ReferenceType[Exception] | None = None
            self.reads: dict[tuple[str, str], int] = {}

        def read(self, label: str, name: str) -> None:
            key = label, name
            self.reads[key] = self.reads.get(key, 0) + 1

    class Transient(Exception):
        def __init__(self, label: str, state: State) -> None:
            super().__init__(label)
            self.label = label
            self.state = state

        def __getattribute__(self, name: str) -> object:
            if name in ('__cause__', '__context__'):
                state: State = super().__getattribute__('state')
                label: str = super().__getattribute__('label')
                state.read(label, name)
                return None
            return super().__getattribute__(name)

    class Bridge(Exception):
        def __init__(self, state: State) -> None:
            super().__init__('bridge')
            self.state = state

        def __getattribute__(self, name: str) -> object:
            if name in ('__cause__', '__context__'):
                state: State = super().__getattribute__('state')
                state.read('bridge', name)
                if name == '__context__':
                    return None
                # A bounded pool exercises normal allocator reuse without
                # forging pointers or requiring reuse on every interpreter.
                candidates: list[Transient] = []
                for _ in range(128):
                    candidate = Transient('later', state)
                    candidates.append(candidate)
                    if id(candidate) == state.address:
                        return candidate
                return candidates[0]
            return super().__getattribute__(name)

    class Root(SQLAeroError):
        state: State
        bridge: Bridge

        def __getattribute__(self, name: str) -> object:
            if name in ('__cause__', '__context__'):
                state: State = super().__getattribute__('state')
                state.read('root', name)
                if name == '__context__':
                    return super().__getattribute__('bridge')
                source = Transient('first', state)
                state.address = id(source)
                state.source = weakref.ref(source)
                return source
            return super().__getattribute__(name)

    for _ in range(4):
        state = State()
        root = Root('root')
        root.state = state
        root.bridge = Bridge(state)
        info = root.error_info
        assert info.cause is not None and info.context is not None
        later = info.context.cause
        assert later is not None and info.cause.message == 'first'
        assert later.message == 'later', 'a new exception reused an old diagnostic identity'
        assert later is not info.cause
        assert state.reads == {
            (label, name): 1 for label in ('root', 'first', 'bridge', 'later')
            for name in ('__cause__', '__context__')
        }
        gc.collect()
        assert state.source is not None and state.source() is None, 'source escaped capture'
