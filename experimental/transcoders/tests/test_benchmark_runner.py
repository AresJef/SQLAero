"""Mock controller tests plus optional actual three-entry correctness integration."""
import json
import os
import sys
import tempfile
import unittest
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[1]/'benchmarking'))
from protocol import LoadPolicy, PROTOCOL_VERSION
from runner import JsonlEvidence, RunConfig, run_comparison, summarize, exclusive_lock, three_way_report
from backends import cases, load_backends, validate_cases, receipt, rust_dispatch_candidates


class RunnerTests(unittest.TestCase):
    """Check balanced orders, retained failures, qualification and exclusive storage."""
    def setUp(self) -> None:
        """Initialize deterministic clocks and an isolated cooperative lease."""
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.now = 0.
        self.tick = 0.
        self.events = []
        self.policy = LoadPolicy(20.,1.,1.,1.,2,4.,1.)
        self.quiet = {'cpu_busy_percent':1.,'runnable_per_cpu':0.,'memory_pressure':1.}
        self.config = RunConfig(6,2,1,1.,str(Path(self.directory.name)/'lease'),'fixed',{})

    def clock(self) -> float:
        """Return fake host time independently of the interval timer."""
        return self.now

    def sleep(self, seconds: float) -> None:
        """Advance fake load-window time without sleeping."""
        self.now += seconds

    def timer(self) -> float:
        """Return strictly increasing deterministic interval timings."""
        self.tick += .01
        return self.tick

    def run_mock(self, callbacks: dict[str,object]) -> list[dict[str,object]]:
        """Execute the real controller with injected idle observations and clocks."""
        return run_comparison(callbacks,self.config,self.policy,lambda:self.quiet,
                              lambda event:self.events.append(json.loads(json.dumps(event))),
                              self.clock,self.sleep,self.timer)

    def test_three_way_complete_orders(self) -> None:
        """All six old/Cython/Rust permutations retain 36 raw repetitions."""
        result=self.run_mock({name:lambda:1 for name in ('old','cython','rust')})
        self.assertEqual(len(result),18)
        orders=[tuple(event['order']) for event in self.events if event['kind']=='block-start']
        self.assertEqual(len(set(orders)),6)
        self.assertEqual(sum(event['kind']=='raw-repetition' for event in self.events),36)
        self.assertTrue(all(row['block_eligible'] for row in result))
        report=summarize(result,'old',bootstrap_repeats=20)
        self.assertEqual(set(report['paired_ratios']),{'cython','rust'})
        self.assertEqual(report['paired_ratios']['rust']['blocks'],1)
        self.assertIn('cython',summarize(result,'rust',bootstrap_repeats=20)['paired_ratios'])
        self.assertEqual(set(three_way_report({'int':result})['by_type']['int']['ratios']),
                         {'cython_over_old','rust_over_old','rust_over_cython'})

    def test_error_and_interference_retained(self) -> None:
        """Failed timed attempts remain visible and reject the entire paired block."""
        self.config=RunConfig(6,2,0,1.,self.config.shared_lock_path,'fixed',{})
        def failure() -> None:
            """Raise a test-only failure at the actual callback boundary."""
            raise ValueError('retained')
        result=self.run_mock({'old':lambda:1,'cython':failure,'rust':lambda:1})
        self.assertEqual(len(result),18)
        self.assertFalse(any(row['block_eligible'] for row in result))
        self.assertEqual(sum(bool(event.get('error')) for event in self.events if event['kind']=='raw-repetition'),6)

    def test_guard_and_missing_signal(self) -> None:
        """Post-run interference persists; unavailable load yields no timings."""
        self.quiet={}
        self.assertEqual(self.run_mock({name:lambda:1 for name in ('old','cython','rust')}),[])
        self.assertTrue(any(event['kind']=='wait-timeout' for event in self.events))

    def test_cycle_contamination_and_duplicate(self) -> None:
        """Incomplete/contaminated cycle rows stay retained and never qualify ratios."""
        result=self.run_mock({name:lambda:1 for name in ('old','cython','rust')})
        result[0]['eligible']=False
        self.assertEqual(summarize(result,'old')['qualified_measurements'],0)
        report=three_way_report({'int':result})
        self.assertEqual(report['by_type']['int']['times']['old']['qualified_means_seconds'],[])
        with self.assertRaises(ValueError): summarize(result+[result[0]],'old')

    def test_paired_ratio_not_ratio_of_sums(self) -> None:
        """An expensive final block cannot dominate an equal-pair estimand."""
        result=self.run_mock({name:lambda:1 for name in ('old','cython','rust')})
        for row in result:
            reference=100. if row['block']==5 else 1.
            row['value']=reference if row['candidate']=='old' else reference*(1. if row['block']==5 else 2.)
        self.assertEqual(summarize(result,'old')['paired_ratios']['cython']['median'],2.)
        report=three_way_report({'int':result},self.events)
        self.assertIn('p95_seconds',report['by_type']['int']['times']['old'])
        self.assertGreater(report['diagnostics']['wait_seconds'],0)

    def test_warmup_only_failures_in_report(self) -> None:
        """Attempted cycles and errors survive even when warmups produce no timing rows."""
        def failure() -> None:
            """Fail a synthetic warmup before any timed work."""
            raise ValueError('warmup')
        result=self.run_mock({'old':lambda:1,'cython':failure,'rust':lambda:1})
        self.assertEqual(result,[])
        report=three_way_report({'fixed':result},self.events)['by_type']['fixed']
        self.assertEqual(report['diagnostics']['warmup_failure_count'],6)
        self.assertEqual(report['diagnostics']['failure_count'],6)
        self.assertEqual(report['diagnostics']['unqualified_or_incomplete_cycles'],1)

    def test_nonfinite_guard(self) -> None:
        """Reject unbounded guard durations before acquiring any lease."""
        for guard in (float('nan'),float('inf')):
            with self.assertRaises(ValueError):
                RunConfig(6,1,0,guard,'lock','fixed',{}).validate(3)

    def test_heavy_gates_and_complete_cycle(self) -> None:
        """Reject a heavy request lacking joint protocol/window and partial balance."""
        with self.assertRaises(ValueError):
            RunConfig(6,1,0,1.,'lock','fixed',{},heavy=True).validate(3)
        with self.assertRaises(ValueError):
            RunConfig(2,1,0,1.,'lock','fixed',{}).validate(3)

    def test_exclusive_sink_and_lease(self) -> None:
        """Evidence cannot be overwritten and a participating lease cannot overlap."""
        path=Path(self.directory.name)/'raw.jsonl'
        sink=JsonlEvidence(path)
        sink.emit({'protocol':PROTOCOL_VERSION,'kind':'retained'})
        sink.close()
        with self.assertRaises(FileExistsError): JsonlEvidence(path)
        self.assertEqual(json.loads(path.read_text())['kind'],'retained')
        with exclusive_lock(self.config.shared_lock_path,.02):
            with self.assertRaises(TimeoutError):
                with exclusive_lock(self.config.shared_lock_path,.02): pass

    @unittest.skipUnless(os.environ.get('SQLAERO_THREE_WAY_INTEGRATION')=='1','Explicit read-only three-way integration switch')
    def test_actual_public_entries(self) -> None:
        """Call every tiny fixture once on actual legacy and both new modules."""
        backends=load_backends()
        rows=validate_cases(backends,cases())
        self.assertEqual(len(rows),419)
        self.assertFalse(next(row for row in rows if row['case']=='decode-bit-9')['equivalent'])
        baseline=receipt(backends,Path('/Users/jef/ResilioSync/code/SqlCyCli'))
        self.assertTrue(baseline['old_orjson_retained'])
        dispatch=rust_dispatch_candidates(backends['rust'],cases()[2])
        expected=backends['old'].invoke(cases()[2])
        for candidate in dispatch.values():
            candidate.prepare()
            self.assertEqual(candidate(),expected)
        backends['rust'].module._set_dispatch_strategy('cascade')

if __name__=='__main__': unittest.main()
