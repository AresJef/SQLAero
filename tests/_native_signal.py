"""Deliver real OS signals while the tested extension can hold the Python GIL.

Each alarm owns a prestarted sender process. Waiting for that process to exit
closes the signal source before restoring the previous disposition; disarming
a process interval timer alone cannot join an in-flight kernel delivery.
Windows also starts the target in a private console before CPython registers
its handlers. No event can reach the Actions runner or a user's console.
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
        self.reader: Thread | None = None
        self.closed: bool = False
        self.messages: Queue[str] = Queue()
        if os.name == 'nt':
            _private_console()
            self.number: int = signal.SIGINT if sigint else signal.SIGBREAK
            event = 0 if sigint else 1
        else:
            self.number = signal.SIGINT if sigint else signal.SIGALRM
            event = self.number
        self.sender = subprocess.Popen(
            [sys.executable, str(Path(__file__).resolve()), '--sender', str(os.getpid()), str(event)],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
            creationflags=subprocess.CREATE_NO_WINDOW if os.name == 'nt' else 0,
        )
        sender = self.sender
        assert sender.stdout is not None
        stream = sender.stdout

        def read_messages() -> None:
            for line in stream:
                self.messages.put(line)
            self.messages.put('')

        self.reader = Thread(target=read_messages, daemon=True)
        self.reader.start()
        try:
            message = self.messages.get(timeout=5)
        except Empty:
            self._stop_sender()
            raise AssertionError('Native signal sender did not become ready') from None
        if message.strip() != 'ready':
            self._stop_sender()
            raise AssertionError(f'Native signal sender failed: {message!r}')
        # A periodic OS source can reenter a Python handler while it inspects
        # GC state. Coalesce nested fixture deliveries so one progress probe
        # cannot duplicate its own observation; library reentry remains tested.
        handling = False

        def dispatch(number: int, frame: FrameType | None) -> None:
            nonlocal handling
            if handling or self.closed:
                return
            handling = True
            try:
                handler(number, frame)
            finally:
                handling = False

        self.previous = signal.signal(self.number, dispatch)

    def start(self, delay: float, interval: float = 0, *, wait_ready: bool = False) -> None:
        """Arm the sender, optionally confirming it accepted the timer command.

        Progress-sensitive tests can wait before entering native work; their
        handler ignores pre-work signals. Other tests retain nonblocking arming.
        """
        assert self.sender is not None
        assert self.sender.stdin is not None
        self.sender.stdin.write(f'{delay} {interval} {int(wait_ready)}\n')
        self.sender.stdin.flush()
        if wait_ready:
            try:
                message = self.messages.get(timeout=5)
            except Empty:
                raise AssertionError('Native signal sender did not acknowledge arming') from None
            assert message.strip() == 'armed', message

    def cancel(self) -> None:
        """Disarm future signals while leaving the installed handler in place."""
        self.start(0)

    def _stop_sender(self) -> None:
        if self.sender is not None:
            self.sender.terminate()
            self.sender.wait(timeout=5)
            if self.reader is not None:
                self.reader.join(timeout=5)
                assert not self.reader.is_alive(), 'Windows signal message reader did not stop'
            for stream in (self.sender.stdin, self.sender.stdout, self.sender.stderr):
                if stream is not None:
                    stream.close()

    def close(self) -> None:
        """Stop the owned sender before restoring the previous signal handler."""
        if self.closed:
            return
        self.closed = True
        # Ignore pending deliveries while joining the only source. No sender
        # can still generate an alarm after its actual process exit. Retired
        # Python dispatches are also suppressed by the closed flag above.
        signal.signal(self.number, signal.SIG_IGN)
        self._stop_sender()
        signal.signal(self.number, self.previous)


def _send(parent: int, event: int) -> None:
    """Send real OS signals independently of the target's GIL and lifetime."""
    kernel = None
    if os.name == 'nt':
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
    commands: Queue[tuple[float, float, bool] | None] = Queue()

    def read_commands() -> None:
        for line in sys.stdin:
            delay, interval, acknowledge = line.split()
            commands.put((float(delay), float(interval), acknowledge == '1'))
        commands.put(None)

    Thread(target=read_commands, daemon=True).start()
    print('ready', flush=True)
    # Keep deadlines high resolution on Windows Python 3.12 as well.
    deadline: float | None = None
    repeat = 0.0
    while True:
        if deadline is None:
            command = commands.get()
        else:
            try:
                command = commands.get_nowait()
            except Empty:
                remaining = deadline - time.perf_counter()
                if remaining > 0:
                    # Queue's timed lock wait uses the coarse Windows wait
                    # clock. CPython's public sleep uses a high-resolution
                    # waitable timer; poll only while this finite alarm is armed.
                    time.sleep(min(remaining, 0.0001))
                    continue
                if kernel is None:
                    # Do not signal a reused PID if the owned target exits.
                    if os.getppid() != parent:
                        return
                    os.kill(parent, event)
                elif not kernel.GenerateConsoleCtrlEvent(event, 0):
                    raise _win.WinError(_win.get_last_error())
                deadline = time.perf_counter() + repeat if repeat else None
                continue
        if command is None:
            return
        delay, repeat, acknowledge = command
        if acknowledge:
            print('armed', flush=True)
        deadline = time.perf_counter() + delay if delay else None


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
        if os.name != 'nt':
            # Keep a pending default-action signal in an isolated process: the
            # old close order terminated it after unblocking, hiding pytest output.
            pending_close = '''
import os
import signal
import sys
import time
sys.path.insert(0, sys.argv[1])
from _native_signal import NativeAlarm
hits = []
def previous_handler(number, frame):
    hits.append(number)
for previous in (signal.SIG_DFL, signal.SIG_IGN, previous_handler):
    signal.signal(signal.SIGALRM, previous)
    # Reader threads inherit this mask; a process-directed signal must stay
    # pending instead of being consumed by a different unblocked thread.
    old_mask = signal.pthread_sigmask(signal.SIG_BLOCK, {signal.SIGALRM})
    alarm = NativeAlarm(lambda number, frame: None)
    alarm.start(0.0001, 0.0001, wait_ready=True)
    os.kill(os.getpid(), signal.SIGALRM)
    assert signal.SIGALRM in signal.sigpending()
    alarm.close()
    alarm.close()
    assert signal.getsignal(signal.SIGALRM) is previous
    assert signal.SIGALRM not in signal.sigpending()
    assert signal.getitimer(signal.ITIMER_REAL) == (0.0, 0.0)
    assert alarm.sender is not None and alarm.sender.poll() is not None
    assert alarm.reader is not None and not alarm.reader.is_alive()
    signal.pthread_sigmask(signal.SIG_SETMASK, old_mask)
    time.sleep(0.002)
    assert hits == []
os.kill(os.getpid(), signal.SIGALRM)
assert hits == [signal.SIGALRM]
print('pending owned alarm discarded; original handler restored')
'''
            subprocess.run([sys.executable, '-c', pending_close,
                            str(Path(__file__).resolve().parent)], check=True, timeout=10)
        for sigint in ((False, True) if os.name == 'nt' else (False,)):
            hits: list[int] = []

            def receive(number: int, frame: FrameType | None) -> None:
                hits.append(number)
                if len(hits) == 1:
                    nested = signal.getsignal(number)
                    assert callable(nested)
                    nested(number, frame)

            alarm = NativeAlarm(receive, sigint=sigint)
            installed = signal.getsignal(alarm.number)
            assert callable(installed)
            try:
                alarm.start(0.02)
                end = time.monotonic() + 3
                while not hits and time.monotonic() < end:
                    time.sleep(0.01)
                assert hits == [alarm.number], hits
                print(f'real native signal received: {hits[0]}')
            finally:
                alarm.close()
            # A Python dispatch already queued during close must stay retired.
            installed(alarm.number, None)
            assert hits == [alarm.number], hits
