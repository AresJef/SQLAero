"""Versioned load-controlled measurement primitives; never run benchmarks on import.

System probes and the actual measurer are injected. No process termination,
system setting changes, sample deletion or automatic weakened signal policy.
"""
from dataclasses import dataclass
import math
from itertools import permutations
from typing import Callable, Mapping

PROTOCOL_VERSION = 'sqlaero-load-ab-v1-draft'
Signal = Mapping[str, float | None]
Event = dict[str, object]

@dataclass(frozen=True)
class LoadPolicy:
    """Explicit thresholds and continuous-window requirements for a protocol version."""
    cpu_busy_max: float
    runnable_per_cpu_max: float
    memory_pressure_max: float
    stable_seconds: float
    minimum_samples: int
    max_wait_seconds: float
    poll_seconds: float
    def __post_init__(self) -> None:
        """Reject policies that cannot substantiate a continuous stable window."""
        values = (self.cpu_busy_max, self.runnable_per_cpu_max, self.memory_pressure_max,
                  self.stable_seconds, self.max_wait_seconds, self.poll_seconds)
        if any(not math.isfinite(value) for value in values):
            raise ValueError('Policy values must be finite')
        if min(self.cpu_busy_max, self.runnable_per_cpu_max, self.memory_pressure_max) < 0:
            raise ValueError('Load thresholds must be nonnegative')
        if self.stable_seconds <= 0 or self.minimum_samples < 2 or self.poll_seconds <= 0:
            raise ValueError('A multi-sample positive stable window is required')
        if self.max_wait_seconds < self.stable_seconds:
            raise ValueError('Wait budget must permit the stable window')

    def classify(self, sample: Signal) -> tuple[bool, list[str]]:
        """Reject unavailable required metrics and report every breached threshold."""
        limits={'cpu_busy_percent':self.cpu_busy_max,'runnable_per_cpu':self.runnable_per_cpu_max,
                'memory_pressure':self.memory_pressure_max}
        reasons:list[str]=[]
        for key,limit in limits.items():
            value=sample.get(key)
            if value is None:
                reasons.append('unavailable:'+key)
            elif value != value or value < 0 or value > limit:
                reasons.append('threshold:'+key)
        return not reasons,reasons


def balanced_orders(names: tuple[str,...], blocks:int) -> list[tuple[str,...]]:
    """Use AB/BA or all permutations, preserving complete-cycle balance metadata."""
    if len(names)<2 or len(set(names))!=len(names) or blocks<1:
        raise ValueError('Distinct candidates and positive paired blocks required')
    if len(names)>4:
        raise ValueError('More than four candidates requires a separate order-design version')
    orders=list(permutations(names))
    return [orders[index%len(orders)] for index in range(blocks)]


def wait_for_stability(policy:LoadPolicy, probe:Callable[[],Signal],
                       clock:Callable[[],float], sleep:Callable[[float],None],
                       emit:Callable[[Event],None]) -> bool:
    """Wait within a finite budget, retain all samples and reset on any interference."""
    started=clock(); quiet_started:float|None=None; count=0
    while True:
        now=clock(); sample=dict(probe()); ok,reasons=policy.classify(sample)
        emit({'protocol':PROTOCOL_VERSION,'kind':'wait-sample','time':now,
              'signals':sample,'stable':ok,'reasons':reasons})
        if ok:
            if quiet_started is None: quiet_started=now
            count+=1
            if now-quiet_started>=policy.stable_seconds and count>=policy.minimum_samples:
                emit({'protocol':PROTOCOL_VERSION,'kind':'window-ready','time':now,'samples':count})
                return True
        else:
            quiet_started=None;count=0
        if now-started>=policy.max_wait_seconds:
            emit({'protocol':PROTOCOL_VERSION,'kind':'wait-timeout','time':now})
            return False
        sleep(min(policy.poll_seconds,policy.max_wait_seconds-(now-started)))


def record_measurement(name:str, block:int, policy:LoadPolicy,
                       probe:Callable[[],Signal], clock:Callable[[],float],
                       measure:Callable[[str,Callable[[],None]],float],
                       emit:Callable[[Event],None]) -> Event:
    """Retain pre/during/post interference and failures without retrying or deleting them.

    The future measurer must call the supplied sampling hook between repetitions.
    A separate supported sampler is required for uninterrupted long native calls.
    """
    reasons:list[str]=[]; phase_samples:list[Event]=[]
    def sample(phase:str) -> None:
        """Capture raw load signals and label interference rather than hide slow work."""
        signals=dict(probe()); ok,why=policy.classify(signals)
        reasons.extend(phase+':'+reason for reason in why)
        event={'protocol':PROTOCOL_VERSION,'kind':'measurement-sample','candidate':name,
               'block':block,'phase':phase,'time':clock(),'signals':signals,'stable':ok,'reasons':why}
        phase_samples.append(event);emit(event)
    sample('before');started=clock()
    value:float|None=None;error:str|None=None
    try:
        value=measure(name,lambda:sample('during'))
    except Exception as exc:
        error=type(exc).__name__+': '+str(exc)
    finally:
        sample('after')
    if not any(event['phase']=='during' for event in phase_samples):
        reasons.append('unavailable:during-measurement-sampling')
    event={'protocol':PROTOCOL_VERSION,'kind':'measurement','candidate':name,'block':block,
           'started':started,'ended':clock(),'value':value,'error':error,
           'interference':reasons,'eligible':not reasons and error is None}
    emit(event)
    return event


def assert_same_protocol(events:list[Event]) -> None:
    """Prevent silently pooling samples collected under different protocol revisions."""
    versions={event.get('protocol') for event in events}
    if versions!={PROTOCOL_VERSION}:
        raise ValueError('Different protocol versions cannot be pooled')
