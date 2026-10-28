"""Deliver real OS signals while the tested extension can hold the Python GIL.

POSIX keeps its existing interval timer. Windows starts the test interpreter in
a private console before CPython registers its handlers, then uses a prestarted
sender. No event can reach the Actions runner or a user's console.
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

_win: Any = ctypes  # Windows-only ctypes exports are absent from non-Windows stubs.


def _kernel() -> Any:
    """Bind only the documented Windows console API, with exact scalar types."""
    kernel = _win.WinDLL('kernel32', use_last_error=True)
    for name in ('FreeConsole',):
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
    """Require a console allocated before Python initialized its CRT handlers.

    AllocConsole/AttachConsole reset Windows control handlers, so creating a
    console inside the already running target interpreter would invalidate the
    test. The CLI below owns process creation and keeps the runner separate.
    """
    if os.environ.get('SQLAERO_TEST_PRIVATE_CONSOLE') != '1':
        raise RuntimeError('Windows signal tests require python tests/_native_signal.py --pytest ...')
    # A runner can pass down the inheritable ignore-Ctrl+C attribute. Restoring
    # normal processing changes only this private target and keeps CPython's
    # registered handler table intact (no console detach/reallocation).
    if not _kernel().SetConsoleCtrlHandler(None, False):
        raise _win.WinError(_win.get_last_error())


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
        if deadline is None:
            command = commands.get()
        else:
            try:
                command = commands.get_nowait()
            except Empty:
                remaining = deadline - time.monotonic()
                if remaining > 0:
                    # Queue's timed lock wait uses the coarse Windows wait
                    # clock. CPython's public sleep uses a high-resolution
                    # waitable timer; poll only while this finite alarm is armed.
                    time.sleep(min(remaining, 0.0001))
                    continue
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
    elif sys.argv[1:2] == ['--pytest']:
        command = [sys.executable, '-m', 'pytest', *sys.argv[2:]]
        if os.name == 'nt':
            environment = {**os.environ, 'SQLAERO_TEST_PRIVATE_CONSOLE': '1'}
            completed = subprocess.run(command, env=environment, stdin=subprocess.DEVNULL,
                                       stdout=sys.stdout, stderr=sys.stderr,
                                       creationflags=subprocess.CREATE_NEW_CONSOLE, check=False)
            raise SystemExit(completed.returncode)
        os.execv(sys.executable, command)
    elif os.name == 'nt' and os.environ.get('SQLAERO_TEST_PRIVATE_CONSOLE') != '1':
        environment = {**os.environ, 'SQLAERO_TEST_PRIVATE_CONSOLE': '1'}
        completed = subprocess.run([sys.executable, str(Path(__file__).resolve())], env=environment,
                                   stdin=subprocess.DEVNULL, stdout=sys.stdout, stderr=sys.stderr,
                                   creationflags=subprocess.CREATE_NEW_CONSOLE, timeout=10, check=False)
        raise SystemExit(completed.returncode)
    else:
        for sigint in ((False, True) if os.name == 'nt' else (False,)):
            hits: list[int] = []
            alarm = NativeAlarm(lambda number, _frame: hits.append(number), sigint=sigint)
            try:
                alarm.start(0.02)
                end = time.monotonic() + 3
                while not hits and time.monotonic() < end:
                    time.sleep(0.01)
                assert hits == [alarm.number], hits
                print(f'real native signal received: {hits[0]}')
            finally:
                alarm.close()
