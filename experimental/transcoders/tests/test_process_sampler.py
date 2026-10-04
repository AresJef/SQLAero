"""Independent parent sampling tests use mock signals and synthetic child data."""
import json
import sys
import unittest
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[1]/'benchmarking'))
from process_sampler import ChildResult, monitor_child, parse_timing
from protocol import LoadPolicy


class ProcessSamplerTests(unittest.TestCase):
    """Validate parent monitoring without executing a performance workload."""
    def setUp(self) -> None:
        """Provide deterministic quiet signals and a short test sampling cadence."""
        self.policy=LoadPolicy(20.,1.,1.,.01,2,1.,.01)
        self.quiet={'cpu_busy_percent':0.,'runnable_per_cpu':0.,'memory_pressure':1.}
        self.events=[]

    def test_independent_parent(self) -> None:
        """A sleeping synthetic child permits parent observations and raw stdout retention."""
        payload={'raw':[{'seconds':.123,'error':None}],'mean_seconds':.123}
        code='import time,json; started=time.monotonic();time.sleep(.05);payload='+repr(payload)+';payload["timed_interval"]={"started":started,"ended":time.monotonic()};print(json.dumps(payload))'
        result=monitor_child([sys.executable,'-c',code],self.policy,lambda:self.quiet,self.events.append,2.)
        self.assertTrue(result.qualified())
        self.assertEqual(parse_timing(result).seconds,.123)
        self.assertTrue(any(event['phase']=='during-child' for event in result.observations))

    def test_missing_and_errors_retained(self) -> None:
        """Unavailable signals and failed child output never qualify as normal runs."""
        result=monitor_child([sys.executable,'-c','print("retained");raise SystemExit(2)'],
                             self.policy,lambda:{},self.events.append,2.)
        self.assertFalse(result.qualified());self.assertIn('retained',result.stdout)
        with self.assertRaises(RuntimeError): parse_timing(result)

    def test_no_during_no_claim(self) -> None:
        """A short child without a during observation cannot prove a quiet native interval."""
        result=ChildResult(0,'{}','',({'phase':'before-child','stable':True},),())
        self.assertFalse(result.qualified())

    def test_startup_only_does_not_qualify(self) -> None:
        """Observations of setup cannot establish coverage of the timed batch."""
        payload=json.dumps({'timed_interval':{'started':10.,'ended':11.}})
        observations=({'phase':'during-child','stable':True,'started':1.,'ended':2.},)
        self.assertFalse(ChildResult(0,payload,'',observations,(),.1).qualified())

    def test_measurement_gap_does_not_qualify(self) -> None:
        """A native interval with an unobserved long gap remains unqualified."""
        payload=json.dumps({'timed_interval':{'started':10.,'ended':12.}})
        observations=tuple({'phase':'during-child','stable':True,'started':a,'ended':b}
                           for a,b in ((9.99,10.02),(11.98,12.01)))
        self.assertFalse(ChildResult(0,payload,'',observations,(),.1).qualified())
        complete=({'phase':'during-child','stable':True,'started':9.99,'ended':12.01},)
        self.assertTrue(ChildResult(0,payload,'',complete,(),.1).qualified())

    def test_invalid_timeout_before_launch(self) -> None:
        """Invalid budgets reject before starting any owned subprocess."""
        for timeout in (float('nan'),float('inf'),0.,-1.):
            with self.assertRaises(ValueError):
                monitor_child([],self.policy,lambda:self.quiet,self.events.append,timeout)

    def test_invalid_raw_timing(self) -> None:
        """Reject inconsistent or invalid child data rather than invent durations."""
        for seconds, mean in ((-1.,1.),(1.,2.),(float('nan'),1.)):
            payload=json.dumps({'raw':[{'seconds':seconds,'error':None}],'mean_seconds':mean})
            with self.assertRaises(ValueError):parse_timing(ChildResult(0,payload,'',(),()))

if __name__=='__main__': unittest.main()
