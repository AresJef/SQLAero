"""Deliver real OS signals while the tested extension can hold the Python GIL.

POSIX keeps its existing interval timer. Windows uses a prestarted sender in a
private console, so no event can reach the Actions runner or a user's console.
This is a test fixture only; production signal handling remains in CPython.
"""

from collections.abc import Callable
import ctypes
import os
from pathlib import Path
from queue import Empty, Queue
import signal
import subprocess
import sys
from threading import Thread
import time
from types import FrameType
from typing import Any

_console_ready: bool = False
_win: Any = ctypes  # Windows-only ctypes exports are absent from non-Windows stubs.


def _kernel() -> Any:
    """Bind only the documented Windows console API, with exact scalar types."""
    kernel = _win.WinDLL('kernel32', use_last_error=True)
    for name in ('FreeConsole', 'AllocConsole'):
        function = getattr(kernel, name)
        function.argtypes = []
        function.restype = ctypes.c_int
    kernel.AttachConsole.argtypes = [ctypes.c_ulong]
    kernel.AttachConsole.restype = ctypes.c_int
    kernel.SetConsoleCtrlHandler.argtypes = [ctypes.c_void_p, ctypes.c_int]
    kernel.SetConsoleCtrlHandler.restype = ctypes.c_int
    kernel.GenerateConsoleCtrlEvent.argtypes = [ctypes.c_ulong, ctypes.c_ulong]
    kernel.GenerateConsoleCtrlEvent.restype = ctypes.c_int
    return kernel


def _private_console() -> None:
    """Detach only this test process before creating its isolated signal domain."""
    global _console_ready
    if _console_ready:
        return
    kernel = _kernel()
    kernel.FreeConsole()
    if not kernel.AllocConsole():
        raise _win.WinError(_win.get_last_error())
    if not kernel.SetConsoleCtrlHandler(None, False):
        raise _win.WinError(_win.get_last_error())
    _console_ready = True


class NativeAlarm:
    """Install a real handler and provide finite, cancellable test alarms."""

    def __init__(self, handler: Callable[[int, FrameType | None], None], *, sigint: bool = False) -> None:
        self.sender: subprocess.Popen[str] | None = None
        self.closed: bool = False
        if os.name == 'nt':
            _private_console()
            self.number: int = signal.SIGINT if sigint else signal.SIGBREAK
            self.sender = subprocess.Popen(
                [sys.executable, str(Path(__file__).resolve()), '--sender', str(os.getpid()),
                 '0' if sigint else '1'], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                stderr=subprocess.PIPE, text=True, creationflags=subprocess.CREATE_NO_WINDOW,
            )
            ready: Queue[str] = Queue()
            sender = self.sender
            assert sender.stdout is not None
            stream = sender.stdout
            Thread(target=lambda: ready.put(stream.readline()), daemon=True).start()
            try:
                message = ready.get(timeout=5)
            except Empty:
                self._stop_sender()
                raise AssertionError('Windows signal sender did not become ready') from None
            if message.strip() != 'ready':
                self._stop_sender()
                raise AssertionError(f'Windows signal sender failed: {message!r}')
        else:
            self.number = signal.SIGINT if sigint else signal.SIGALRM
        self.previous = signal.signal(self.number, handler)

    def start(self, delay: float, interval: float = 0) -> None:
        """Arm an already ready sender; no new Python thread needs the target GIL."""
        if os.name != 'nt':
            signal.setitimer(signal.ITIMER_REAL, delay, interval)
        else:
            assert self.sender is not None
            assert self.sender.stdin is not None
            self.sender.stdin.write(f'{delay} {interval}\n')
            self.sender.stdin.flush()

    def cancel(self) -> None:
        """Disarm future signals while leaving the installed handler in place."""
        self.start(0)

    def _stop_sender(self) -> None:
        if self.sender is not None:
            self.sender.terminate()
            self.sender.wait(timeout=5)
            for stream in (self.sender.stdin, self.sender.stdout, self.sender.stderr):
                if stream is not None:
                    stream.close()

    def close(self) -> None:
        """Stop the owned sender before restoring the previous signal handler."""
        if self.closed:
            return
        self.closed = True
        if self.sender is not None:
            signal.signal(self.number, signal.SIG_IGN)
            self._stop_sender()
        else:
            self.cancel()
        signal.signal(self.number, self.previous)


def _send(parent: int, event: int) -> None:
    """Send console events from a separate process, independent of target GIL."""
    kernel = _kernel()
    kernel.FreeConsole()
    if not kernel.AttachConsole(parent):
        raise _win.WinError(_win.get_last_error())
    callback_type = _win.WINFUNCTYPE(ctypes.c_int, ctypes.c_ulong)
    def ignore_event(_event: int) -> bool:
        return True

    ignore = callback_type(ignore_event)
    if not kernel.SetConsoleCtrlHandler(ctypes.cast(ignore, ctypes.c_void_p), True):
        raise _win.WinError(_win.get_last_error())
    commands: Queue[tuple[float, float] | None] = Queue()

    def read_commands() -> None:
        for line in sys.stdin:
            delay, interval = map(float, line.split())
            commands.put((delay, interval))
        commands.put(None)

    Thread(target=read_commands, daemon=True).start()
    print('ready', flush=True)
    deadline: float | None = None
    repeat = 0.0
    while True:
        try:
            command = commands.get(timeout=None if deadline is None else max(0, deadline - time.monotonic()))
        except Empty:
            if not kernel.GenerateConsoleCtrlEvent(event, 0):
                raise _win.WinError(_win.get_last_error())
            deadline = time.monotonic() + repeat if repeat else None
            continue
        if command is None:
            return
        delay, repeat = command
        deadline = time.monotonic() + delay if delay else None


if __name__ == '__main__':
    if len(sys.argv) == 4 and sys.argv[1] == '--sender':
        _send(int(sys.argv[2]), int(sys.argv[3]))
    else:
        hits: list[int] = []
        alarm = NativeAlarm(lambda number, _frame: hits.append(number))
        try:
            alarm.start(0.02)
            end = time.monotonic() + 3
            while not hits and time.monotonic() < end:
                time.sleep(0.01)
            assert hits == [alarm.number], hits
            print(f'real native signal received: {hits[0]}')
        finally:
            alarm.close()
