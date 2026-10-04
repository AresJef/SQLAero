"""Combine stricter SQLAero signals with unchanged cyTimes host flag evaluation."""
from __future__ import annotations

import threading
from typing import Callable
from benchmark_environment import BenchmarkPolicy, EnvironmentObserver, environment_flags
from protocol import Event, PROTOCOL_VERSION, Signal
from system_probe import MacOSSystemProbe


class CommonHostProbe:
    """Retain physical signals, common flags and every unavailable capability."""
    def __init__(self, emit: Callable[[Event],None]) -> None:
        """Use the copied v2 policy without changing its thresholds or optional readers."""
        self.emit=emit
        self.native=MacOSSystemProbe()
        self.observer=EnvironmentObserver(BenchmarkPolicy())
        self.lock=threading.Lock()

    def __call__(self) -> Signal:
        """Observe both gates read-only and retain unmodified source evidence."""
        with self.lock:
            native=self.native.sample()
            common=self.observer.snapshot()
            evaluated=environment_flags(common,self.observer.policy)
            signals=dict(native.signals)
            signals['common_host_flag_count']=float(len(evaluated['flags']))
            self.emit({'protocol':PROTOCOL_VERSION,'kind':'probe-provenance',
                       'native':native.as_dict(),'common':common,'common_evaluation':evaluated,
                       'signal_contract':'physical strict signals plus count of unchanged common policy flags'})
            return signals
