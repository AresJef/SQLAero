"""Native provider diagnostics retain full original text with bounded work."""

# pyright: reportPrivateUsage=false
from types import FrameType


import signal
import pytest
from sqlaero import _sqlaero

pa = pytest.importorskip("pyarrow")


@pytest.mark.parametrize("kind", ["utf8", "large", "view", "dictionary"])
@pytest.mark.parametrize("prefix", ["a", "é", "中"])
def test_long_encoding_error_keeps_full_source_and_character_coordinates(kind: str, prefix: str) -> None:
    count = 256 * 1024 + 1
    source = prefix * count + "🙂\0tail"
    dtype = pa.string() if kind in ("utf8", "dictionary") else pa.large_string() if kind == "large" else pa.string_view()
    values = pa.array([source], type=dtype)
    if kind == "dictionary":
        values = pa.DictionaryArray.from_arrays(pa.array([0], type=pa.int8()), values)
    with pytest.raises(UnicodeEncodeError) as caught:
        _sqlaero._bind_sql("SELECT %s", values, charset="gbk")
    error = caught.value
    assert error.object == source
    assert (error.start, error.end) == (count, count + 1)
    assert "U+1F642" in error.reason
    assert "args[0]" in "\n".join(getattr(error, "__notes__", ()))


def test_error_payload_reconstruction_uses_the_original_operation_deadline() -> None:
    _sqlaero._bind_sql("SELECT %s", pa.array(["a"]), charset="ascii")
    # The incompatible first character reaches encoding failure immediately;
    # the full error payload still has substantial required source work.
    value = pa.array(["🙂" + "a" * (16 * 1024 * 1024)])
    with pytest.raises(_sqlaero.OperationalTimeoutError):
        _sqlaero._bind_sql("SELECT %s", value, charset="ascii", timeout_ns=5_000_000)


@pytest.mark.skipif(not hasattr(signal, "setitimer"), reason="requires the platform's interval timer")
def test_original_signal_is_preserved_while_reconstructing_the_error_payload() -> None:
    class InterruptedDiagnostic(Exception):
        pass
    original = InterruptedDiagnostic("original diagnostic signal")
    _sqlaero._bind_sql("SELECT %s", pa.array(["a"]), charset="ascii")
    value = pa.array(["🙂" + "a" * (16 * 1024 * 1024)])
    def interrupt(signum: int, frame: FrameType | None):
        raise original
    previous = signal.signal(signal.SIGALRM, interrupt)
    signal.setitimer(signal.ITIMER_REAL, 0.005)
    try:
        with pytest.raises(InterruptedDiagnostic) as caught:
            _sqlaero._bind_sql("SELECT %s", value, charset="ascii")
        assert caught.value is original
    finally:
        signal.setitimer(signal.ITIMER_REAL, 0)
        signal.signal(signal.SIGALRM, previous)
