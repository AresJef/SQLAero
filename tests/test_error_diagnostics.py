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
