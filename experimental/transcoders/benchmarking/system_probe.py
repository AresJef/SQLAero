"""Read-only macOS load probes with explicit units, provenance and unavailable values.

No commands run on import. CPU usage uses top's second one-second delta report;
runnable work counts process states R per logical CPU, not runnable threads or
load average. Memory pressure is the native memorystatus sysctl level, never a
free-memory percentage or inferred paging score. Thresholds remain caller-owned.

Sources: installed macOS top(1) CPU/event-counting and ps(1) state definitions.
The cyTimes-rewrite benchmark_environment.py observer was inspected read-only:
its vm_stat paging counters cannot substitute for this protocol's pressure level.
"""
from __future__ import annotations

from dataclasses import asdict, dataclass
import datetime as dt
import math
import os
import platform
import re
import subprocess
import time
from typing import Callable

TOP_COMMAND = ('/usr/bin/top', '-l', '2', '-s', '1', '-n', '0', '-c', 'd', '-F')
CPU_COUNT_COMMAND = ('/usr/sbin/sysctl', '-n', 'hw.logicalcpu')
PROCESS_STATE_COMMAND = ('/bin/ps', '-A', '-o', 'state=')
PRESSURE_COMMAND = ('/usr/sbin/sysctl', '-n', 'kern.memorystatus_vm_pressure_level')
SIGNAL_NAMES = ('cpu_busy_percent', 'runnable_per_cpu', 'memory_pressure')

@dataclass(frozen=True)
class CommandResult:
    """Record a read-only command including failure evidence and observed elapsed time."""
    command: tuple[str, ...]
    returncode: int | None
    stdout: str = ''
    stderr: str = ''
    elapsed_seconds: float = 0.0
    error: str | None = None

CommandRunner = Callable[[tuple[str, ...], float], CommandResult]

@dataclass(frozen=True)
class ProbeReport:
    """Capture protocol values separately from capability/source/unit and command evidence."""
    signals: dict[str, float | None]
    capabilities: dict[str, dict[str, object]]
    commands: tuple[CommandResult, ...]
    sampled_at_utc: str
    elapsed_seconds: float
    logical_cpus: int | None
    normalization: str = 'logical CPUs reported by hw.logicalcpu'

    def as_dict(self) -> dict[str, object]:
        """Return a persistence-ready snapshot without discarding unavailable provenance."""
        return asdict(self)


def run_command(command: tuple[str, ...], timeout: float) -> CommandResult:
    """Run a read-only argument vector with a bounded timeout and fixed C output locale."""
    started = time.monotonic()
    environment = os.environ.copy()
    environment.update(LC_ALL='C', LANG='C')
    try:
        result = subprocess.run(command, capture_output=True, text=True,
                                timeout=timeout, env=environment, check=False)
        return CommandResult(command, result.returncode, result.stdout, result.stderr,
                             time.monotonic() - started)
    except subprocess.TimeoutExpired as exc:
        stdout = exc.stdout.decode(errors='replace') if isinstance(exc.stdout, bytes) else exc.stdout or ''
        stderr = exc.stderr.decode(errors='replace') if isinstance(exc.stderr, bytes) else exc.stderr or ''
        return CommandResult(command, None, stdout, stderr, time.monotonic() - started,
                             'TimeoutExpired: command exceeded %.3f seconds' % timeout)
    except OSError as exc:
        return CommandResult(command, None, elapsed_seconds=time.monotonic() - started,
                             error=type(exc).__name__ + ': ' + str(exc))


def _failure(result: CommandResult) -> str | None:
    """Identify command failures before parsing so stderr can never masquerade as a signal."""
    if result.error is not None:
        return result.error
    if result.returncode != 0:
        return 'command exit %r: %s' % (result.returncode, result.stderr.strip())
    return None


def _cpu_busy(text: str) -> float:
    """Parse the final top interval CPU report; reject first-only or malformed output."""
    samples = re.findall(r'^CPU usage:\s*([0-9]+(?:\.[0-9]+)?)% user,\s*'
                         r'([0-9]+(?:\.[0-9]+)?)% sys,\s*'
                         r'([0-9]+(?:\.[0-9]+)?)% idle\s*$', text, re.MULTILINE)
    reports = re.findall(r'^CPU usage:', text, re.MULTILINE)
    if len(samples) != 2 or len(reports) != 2:
        raise ValueError('exactly two valid top reports required; no initial/stale fallback')
    user, system, idle = map(float, samples[-1])
    if any(not math.isfinite(v) or not 0 <= v <= 100 for v in (user, system, idle)):
        raise ValueError('CPU components outside percent domain')
    if abs(user + system + idle - 100) > 1:
        raise ValueError('CPU component total inconsistent with percent units')
    return 100.0 - idle


def _positive_cpu_count(text: str) -> int:
    """Read logical CPU normalization from an exact positive integer response."""
    stripped = text.strip()
    if re.fullmatch(r'[0-9]+', stripped) is None or not 0 < int(stripped) <= 65536:
        raise ValueError('invalid logical CPU count')
    return int(stripped)


def _runnable_processes(text: str) -> int:
    """Count ps states starting R; validate every row rather than treating unknown output as zero."""
    states = [line.strip() for line in text.splitlines() if line.strip()]
    if not states:
        raise ValueError('empty process-state snapshot')
    if any(re.fullmatch(r'[IRSTUZ][+<>AELNSsVWX]*', state) is None for state in states):
        raise ValueError('unrecognized ps process state')
    return sum(state.startswith('R') for state in states)


def _pressure_level(text: str) -> float:
    """Return the raw native sysctl level without inventing a normalized pressure score."""
    stripped = text.strip()
    if re.fullmatch(r'[0-9]+', stripped) is None or int(stripped) > (1 << 53):
        raise ValueError('invalid native memory-pressure level')
    return float(int(stripped))


class MacOSSystemProbe:
    """Expose protocol-compatible signals while retaining full capability/provenance snapshots.

    Runnable state is a process snapshot including the probe's own ps process;
    it does not count all runnable threads or guarantee an instantaneous kernel
    run-queue size. Calls are sequential, not an atomic three-signal snapshot.
    Each CPU call blocks for the declared one-second top sampling interval.
    Permission, timeout, missing binaries/nodes and unexpected output yield None;
    missing required metrics therefore block protocol readiness.
    """
    def __init__(self, runner: CommandRunner = run_command, *,
                 system: str | None = None, timeout_seconds: float = 4.0) -> None:
        """Inject a command reader for mocks and require a finite timeout exceeding the CPU interval."""
        if not math.isfinite(timeout_seconds) or timeout_seconds <= 1:
            raise ValueError('timeout must exceed the one-second CPU interval')
        self.runner = runner
        self.system = platform.system() if system is None else system
        self.timeout_seconds = timeout_seconds
        self.latest: ProbeReport | None = None

    def __call__(self) -> dict[str, float | None]:
        """Return only protocol numeric signals; retain the evidence in latest for the orchestrator."""
        return self.sample().signals.copy()

    def sample(self) -> ProbeReport:
        """Read bounded OS commands and retain unavailable reasons with every observation."""
        started = time.monotonic()
        sampled_at = dt.datetime.now(dt.timezone.utc).isoformat()
        signals: dict[str, float | None] = dict.fromkeys(SIGNAL_NAMES)
        capabilities: dict[str, dict[str, object]] = {}
        commands: list[CommandResult] = []
        cpus: int | None = None
        definitions = {
            'cpu_busy_percent': ('percent busy over top delta interval', TOP_COMMAND),
            'runnable_per_cpu': ('runnable processes per logical CPU, snapshot', PROCESS_STATE_COMMAND),
            'memory_pressure': ('raw kern.memorystatus_vm_pressure_level integer', PRESSURE_COMMAND),
        }
        if self.system != 'Darwin':
            for name, (unit, command) in definitions.items():
                capabilities[name] = {'status': 'unavailable', 'reason': 'unsupported platform: ' + self.system,
                                      'unit': unit, 'source_command': command}
        else:
            results: dict[tuple[str, ...], CommandResult] = {}
            for command in (TOP_COMMAND, CPU_COUNT_COMMAND, PROCESS_STATE_COMMAND, PRESSURE_COMMAND):
                try:
                    result = self.runner(command, self.timeout_seconds)
                except Exception as exc:
                    result = CommandResult(command, None, error=type(exc).__name__ + ': ' + str(exc))
                if result.command != command:
                    result = CommandResult(command, None, error='injected runner returned mismatched command provenance')
                results[command] = result
                commands.append(result)
            cpu_result = results[CPU_COUNT_COMMAND]
            cpu_error = _failure(cpu_result)
            if cpu_error is None:
                try:
                    cpus = _positive_cpu_count(cpu_result.stdout)
                except ValueError as exc:
                    cpu_error = str(exc)
            for name, (unit, command) in definitions.items():
                result = results[command]
                reason = _failure(result)
                if name == 'runnable_per_cpu' and cpu_error is not None:
                    reason = 'normalization unavailable: ' + cpu_error
                if reason is None:
                    try:
                        if name == 'cpu_busy_percent':
                            value = _cpu_busy(result.stdout)
                        elif name == 'runnable_per_cpu':
                            if cpus is None:
                                raise ValueError('logical CPU normalization unavailable')
                            value = _runnable_processes(result.stdout) / cpus
                        else:
                            value = _pressure_level(result.stdout)
                        signals[name] = value
                    except ValueError as exc:
                        reason = str(exc)
                capability: dict[str, object] = {'status': 'available' if reason is None else 'unavailable',
                                                  'unit': unit, 'source_command': command}
                if reason is not None:
                    capability['reason'] = reason
                if name == 'runnable_per_cpu':
                    capability['normalization_command'] = CPU_COUNT_COMMAND
                    capability['limitation'] = 'process states, not all runnable threads; includes observer processes'
                capabilities[name] = capability
        report = ProbeReport(signals, capabilities, tuple(commands), sampled_at,
                             time.monotonic() - started, cpus)
        self.latest = report
        return report
