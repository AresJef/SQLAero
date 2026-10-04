"""Injected clocks validate batching and raw retention without a real workload timer."""
import sys
import unittest
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[1]/'benchmarking'))
from child_worker import collect_timings
from protocol import LoadPolicy


class WorkerBatchingTests(unittest.TestCase):
    """Prove normalization, finite caps and public-entry failures retain evidence."""
    def setUp(self) -> None:
        """Initialize synthetic timer and callback counters."""
        self.now=0.;self.calls=0;self.events=[]

    def timer(self) -> float:
        """Return predetermined positive durations rather than measuring execution."""
        self.now+=.1
        return self.now

    def callback(self) -> None:
        """Count an inert callback with no external side effect."""
        self.calls+=1

    def test_normalization(self) -> None:
        """Four public calls per batch yield per-call costs and exact raw counts."""
        raw,interval,met=collect_timings(self.callback,2,4,0.,None,self.events.append,self.timer,lambda:self.now)
        self.assertEqual(self.calls,8);self.assertEqual(len(raw),2);self.assertTrue(met)
        self.assertAlmostEqual(raw[0]['seconds'],.025)
        self.assertEqual(raw[0]['calls'],4)

    def test_finite_duration_cap(self) -> None:
        """A stalled monotonic clock cannot run beyond the explicit repetition cap."""
        raw,interval,met=collect_timings(self.callback,1,2,4.,3,self.events.append,self.timer,lambda:0.)
        self.assertEqual(len(raw),3);self.assertFalse(met)
        with self.assertRaises(ValueError):collect_timings(self.callback,1,1,4.,None,self.events.append,self.timer,lambda:0.)

    def test_error_retained_without_retry(self) -> None:
        """A failure flushes the partial batch and stops rather than replacing the sample."""
        def failure() -> None:
            """Raise a deterministic synthetic public-entry error."""
            raise ValueError('retained')
        raw,interval,met=collect_timings(failure,3,4,0.,None,self.events.append,self.timer,lambda:self.now)
        self.assertEqual(len(raw),1);self.assertEqual(raw[0]['calls'],1)
        self.assertIn('retained',self.events[0]['error'])

    def test_common_gate_required_when_declared(self) -> None:
        """Missing or breached unchanged host flags cannot pass the stricter composite gate."""
        policy=LoadPolicy(45.,.45,1.,30.,10,90.,2.,0.)
        signals={'cpu_busy_percent':1.,'runnable_per_cpu':.1,'memory_pressure':1.}
        self.assertFalse(policy.classify(signals)[0])
        self.assertTrue(policy.classify(dict(signals,common_host_flag_count=0.))[0])
        self.assertFalse(policy.classify(dict(signals,common_host_flag_count=1.))[0])

if __name__=='__main__':unittest.main()
