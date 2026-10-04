"""Deterministic mock tests; no actual system probes or performance workloads."""
import sys
import unittest
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[1]/'benchmarking'))
from protocol import LoadPolicy,balanced_orders,wait_for_stability,record_measurement,assert_same_protocol,PROTOCOL_VERSION

class ProtocolTests(unittest.TestCase):
    """Validate balance, sustained-window control and complete raw evidence retention."""
    def setUp(self) -> None:
        """Initialize a fake clock, collector and explicit test-only thresholds."""
        self.time=0.;self.events:list[dict[str,object]]=[]
        self.policy=LoadPolicy(20.,.25,0.,2.,3,6.,1.)
        self.quiet={'cpu_busy_percent':0.,'runnable_per_cpu':0.,'memory_pressure':0.}
    def clock(self) -> float:
        """Return deterministic monotonic test time."""
        return self.time
    def sleep(self,seconds:float) -> None:
        """Advance fake time without waiting in the execution environment."""
        self.time+=seconds
    def test_nonfinite_policy(self) -> None:
        """Reject nonfinite thresholds and wait durations before load admission."""
        for value in (float('nan'), float('inf'), -1.):
            with self.assertRaises(ValueError):
                LoadPolicy(value, 1., 1., 2., 3, 10., 1.)

    def test_slow_probe_deadline(self) -> None:
        """Never admit a window from a probe completing after the waiting deadline."""
        def slow() -> dict[str,float]:
            """Advance simulated time beyond the entire waiting budget."""
            self.time += 7.
            return self.quiet
        self.assertFalse(wait_for_stability(self.policy,slow,self.clock,self.sleep,self.events.append))
        self.assertEqual(self.events[-1]['kind'],'wait-timeout')
        self.assertEqual(self.events[0]['observation_ended'],7.)

    def test_balance(self) -> None:
        """Confirm AB/BA and all six three-engine orders are balanced."""
        self.assertEqual(balanced_orders(('A','B'),2),[('A','B'),('B','A')])
        orders=balanced_orders(('old','cython','rust'),6)
        self.assertEqual(len(set(orders)),6)
        for index in range(3):
            self.assertEqual(sorted(order[index] for order in orders),['cython']*2+['old']*2+['rust']*2)
    def test_one_low_sample_not_ready(self) -> None:
        """Require a continuous window after a noisy sample resets prior progress."""
        samples=iter([self.quiet,dict(self.quiet,cpu_busy_percent=90.),self.quiet,self.quiet,self.quiet])
        self.assertTrue(wait_for_stability(self.policy,lambda:next(samples),self.clock,self.sleep,self.events.append))
        self.assertEqual(self.time,4.)
        self.assertEqual(len([event for event in self.events if event['kind']=='wait-sample']),5)
    def test_missing_signal_timeout_retained(self) -> None:
        """Never downgrade requirements when a system signal is unavailable."""
        self.assertFalse(wait_for_stability(self.policy,lambda:{},self.clock,self.sleep,self.events.append))
        self.assertEqual(self.events[-1]['kind'],'wait-timeout')
        self.assertTrue(all('unavailable:cpu_busy_percent' in event['reasons'] for event in self.events[:-1]))
    def test_interference_and_error_retained(self) -> None:
        """Keep failed and contaminated raw measurement records instead of deleting them."""
        samples=iter([self.quiet,dict(self.quiet,cpu_busy_percent=90.),self.quiet])
        def measurement(name:str,sample:object) -> float:
            """Inject interference and a deterministic measurement exception."""
            sample()
            raise RuntimeError('test failure')
        record=record_measurement('A',0,self.policy,lambda:next(samples),self.clock,measurement,self.events.append)
        self.assertFalse(record['eligible']);self.assertIn('RuntimeError',record['error'])
        self.assertIn('during:threshold:cpu_busy_percent',record['interference'])
        self.assertEqual(len(self.events),4)
    def test_missing_during_and_protocol_mixing(self) -> None:
        """Flag unobserved measurement intervals and reject cross-version pooling."""
        record=record_measurement('A',0,self.policy,lambda:self.quiet,self.clock,lambda name,sample:1.,self.events.append)
        self.assertFalse(record['eligible'])
        assert_same_protocol(self.events)
        with self.assertRaises(ValueError):assert_same_protocol([{'protocol':PROTOCOL_VERSION},{'protocol':'other'}])

if __name__=='__main__':unittest.main()
