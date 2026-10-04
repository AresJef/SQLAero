"""Independent parent-process monitoring for child native calls; no work on import."""
from __future__ import annotations

import json
import math
import subprocess
import threading
import time
from dataclasses import dataclass
from typing import Callable
from protocol import Event, LoadPolicy, PROTOCOL_VERSION, Signal


@dataclass(frozen=True)
class ChildResult:
    """Retain raw child output, errors and load observations separately from timings."""
    returncode: int
    stdout: str
    stderr: str
    observations: tuple[Event,...]
    monitor_errors: tuple[str,...]

    maximum_gap_seconds: float = 0.0

    def qualified(self) -> bool:
        """Require actual during-child observations and every observed signal stable."""
        if self.returncode != 0 or self.monitor_errors or not all(row['stable'] for row in self.observations):
            return False
        try:
            payload=json.loads(self.stdout)
            if payload.get('minimum_duration_met') is False:
                return False
            interval = payload['timed_interval']
            started, ended = float(interval['started']), float(interval['ended'])
            if not all(math.isfinite(value) for value in (started, ended)) or ended <= started:
                return False
            overlapping = sorted((float(row['started']), float(row['ended']))
                                 for row in self.observations if row['phase'] == 'during-child'
                                 and float(row['started']) < ended and float(row['ended']) > started)
            if not overlapping or not math.isfinite(self.maximum_gap_seconds) or self.maximum_gap_seconds <= 0:
                return False
            covered_until = started
            for left, right in overlapping:
                if right < left or left - covered_until > self.maximum_gap_seconds:
                    return False
                covered_until = max(covered_until, right)
            return ended - covered_until <= self.maximum_gap_seconds
        except (KeyError, TypeError, ValueError, json.JSONDecodeError):
            return False



def monitor_child(command: list[str], policy: LoadPolicy, probe: Callable[[],Signal],
                  emit: Callable[[Event],None], timeout_seconds: float) -> ChildResult:
    """Sample in the parent while its child runs, retaining complete stdout/stderr.

    Parent sampling avoids the child's GIL; command duration includes setup while
    the child's reported timings exclude startup. Timeout stops only this owned
    child through subprocess.run, never another project's process.
    """
    if not math.isfinite(timeout_seconds) or timeout_seconds <= 0:
        raise ValueError("Child timeout must be finite and positive")
    observations: list[Event]=[]
    errors: list[str]=[]
    stop=threading.Event()
    def observe(phase: str) -> None:
        """Store failures as evidence rather than replacing them with idle values."""
        try:
            started=time.monotonic()
            signals=dict(probe());ok,reasons=policy.classify(signals)
            ended=time.monotonic()
            event={'protocol':PROTOCOL_VERSION,'kind':'child-load','phase':phase,
                   'time':ended,'started':started,'ended':ended,'signals':signals,'stable':ok,'reasons':reasons}
            observations.append(event);emit(event)
        except Exception as exc:
            error=type(exc).__name__+': '+str(exc)
            errors.append(error)
            emit({'protocol':PROTOCOL_VERSION,'kind':'child-monitor-error','phase':phase,'error':error})
    def during() -> None:
        """Probe at a bounded cadence independently of child Python/native execution."""
        while not stop.wait(policy.poll_seconds): observe('during-child')
    observe('before-child')
    thread=threading.Thread(target=during,daemon=True)
    thread.start()
    try:
        try:
            run=subprocess.run(command,capture_output=True,text=True,timeout=timeout_seconds,check=False)
            code,stdout,stderr=run.returncode,run.stdout,run.stderr
        except subprocess.TimeoutExpired as exc:
            code=-1
            stdout=exc.stdout.decode(errors='replace') if isinstance(exc.stdout,bytes) else exc.stdout or ''
            stderr=exc.stderr.decode(errors='replace') if isinstance(exc.stderr,bytes) else exc.stderr or ''
            errors.append('owned-child-timeout')
        except OSError as exc:
            code=-1;stdout='';stderr=str(exc);errors.append(type(exc).__name__+': '+str(exc))
    finally:
        stop.set();thread.join();observe('after-child')
    result=ChildResult(code,stdout,stderr,tuple(observations),tuple(errors),2*policy.poll_seconds)
    emit({'protocol':PROTOCOL_VERSION,'kind':'child-result','command':command,
          'returncode':code,'stdout':stdout,'stderr':stderr,'monitor_errors':errors,
          'qualified':result.qualified()})
    return result


@dataclass(frozen=True)
class NativeTiming:
    """Own-worker durations reported separately from startup and parent probe overhead."""
    seconds: float
    raw: tuple[dict[str,object],...]
    child_qualified: bool


def parse_timing(result: ChildResult) -> NativeTiming:
    """Read complete child evidence, retaining errors and never inventing a duration."""
    if result.returncode: raise RuntimeError('Child failed; retained output has returncode '+str(result.returncode))
    payload=json.loads(result.stdout)
    raw=tuple(payload['raw'])
    if not raw or any(row['error'] is not None for row in raw):
        raise ValueError('Child returned no timings or public-entry errors')
    durations = [float(row['seconds']) for row in raw]
    seconds = float(payload['mean_seconds'])
    if any(not math.isfinite(value) or value <= 0 for value in durations) or not math.isfinite(seconds) or seconds <= 0:
        raise ValueError('Child timing values must be finite and positive')
    if not math.isclose(seconds, sum(durations)/len(durations), rel_tol=1e-12, abs_tol=0):
        raise ValueError('Child mean does not match retained repetitions')
    return NativeTiming(seconds,raw,result.qualified())
