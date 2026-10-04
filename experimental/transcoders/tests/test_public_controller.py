"""Synthetic admission and incomplete-run coverage; no real benchmark workloads."""
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch
sys.path.insert(0,str(Path(__file__).resolve().parents[1]/'benchmarking'))
from backends import Case
from benchmark_environment import BenchmarkLease, PROTOCOL as HOST_PROTOCOL
from protocol import LoadPolicy, PROTOCOL_VERSION
from runner import RunConfig
from public_controller import PublicInvocation, run_public_cases, run_dispatch_cases


class PublicControllerTests(unittest.TestCase):
    """Exercise controller boundaries with fake cases and zero real timings."""
    def setUp(self) -> None:
        """Create isolated admission files and an explicit synthetic heavy configuration."""
        self.directory=tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.path=Path(self.directory.name)
        self.config=RunConfig(6,5,3,1.,str(self.path/'lease'),'fixed',{},'digest','window',True)
        self.policy=LoadPolicy(20.,1.,1.,1.,2,4.,1.)
        self.events=[]
        self.case=Case('fake','escape',1,())
        self.receipt={'protocol':PROTOCOL_VERSION,'shared_lock_path':self.config.shared_lock_path,
                      'artifact_receipt':{},'semantics':[{'case':'fake','outputs':{'old':{'value':'1'}}}]}

    def test_empty_and_partial_run(self) -> None:
        """Timeout returns explicit incomplete status and never reports an empty type."""
        for rows in ([],[{'block':0,'candidate':name} for name in ('old','cython','rust')]):
            with patch('public_controller.verify_receipt',return_value=self.receipt), \
                 patch('public_controller.cases',return_value=[self.case]), \
                 patch('public_controller.run_comparison',return_value=rows), \
                 patch('public_controller.three_way_report',return_value={'retained':True}) as report:
                output=run_public_cases(self.path/'receipt',self.path,self.config,self.policy,lambda:{},self.events.append,1.)
                self.assertFalse(output['complete'])
                self.assertEqual(output['incomplete_case'],'fake')
                self.assertEqual(report.call_count,bool(rows))

    def test_live_lease_admission_and_in_worker_warmups(self) -> None:
        """Admission is written under a lease; child arguments preserve inner warmups."""
        invocation=PublicInvocation('old','fake',self.path/'receipt',self.path,self.config,self.policy,
                                    lambda:{},self.events.append,1.,self.path)
        with self.assertRaises(ValueError):invocation()
        lease=BenchmarkLease(self.config.shared_lock_path)
        import time
        metadata=lease.acquire(time.monotonic()+1.,owner='synthetic-controller')
        try:
            invocation.admit(metadata)
            admitted=json.loads(invocation.managed_path.read_text())
            self.assertEqual(admitted['controller_pid'],os.getpid())
            with patch('public_controller.monitor_child') as monitor, patch('public_controller.parse_timing'):
                invocation()
                command=monitor.call_args.args[0]
                self.assertEqual(command[command.index('--warmup-repetitions')+1],'3')
                self.assertEqual(command[command.index('--repetitions')+1],'5')
            # Synthetic child only verifies receipt, without loading backend modules.
            code='from benchmark_environment import verify_managed_receipt;import sys;verify_managed_receipt(sys.argv[1])'
            run=subprocess.run([sys.executable,'-B','-c',code,str(invocation.managed_path)],
                               cwd=Path(__file__).resolve().parents[1]/'benchmarking',capture_output=True,text=True)
            self.assertEqual(run.returncode,0,run.stderr)
        finally:lease.release()
        run=subprocess.run([sys.executable,'-B','-c',code,str(invocation.managed_path)],
                           cwd=Path(__file__).resolve().parents[1]/'benchmarking',capture_output=True,text=True)
        self.assertNotEqual(run.returncode,0)
        self.assertIn('lease is not held',run.stderr)

    def test_dispatch_worker_adapters_and_partial_status(self) -> None:
        """Reviewed strategy is selected before timed calls and incomplete runs stay partial."""
        self.receipt['dispatch_coverage']={'cython':['exactif','hotcold','typeid_dict']}
        with patch('public_controller.verify_receipt',return_value=self.receipt), \
             patch('public_controller.cases',return_value=[self.case]), \
             patch('public_controller.run_comparison',return_value=[]) as comparison:
            result=run_dispatch_cases('cython',self.path/'receipt',self.path,self.config,
                                      self.policy,lambda:{},self.events.append,1.)
            self.assertFalse(result['complete'])
            self.assertEqual(tuple(comparison.call_args.args[0]),('exactif','hotcold','typeid_dict'))
            invocation=comparison.call_args.args[0]['hotcold']
            invocation.admitted=True
            with patch('public_controller.monitor_child') as monitor,patch('public_controller.parse_timing'):
                invocation()
                command=monitor.call_args.args[0]
                self.assertEqual(command[command.index('--candidate')+1],'cython')
                self.assertEqual(command[command.index('--dispatch-strategy')+1],'hotcold')
                self.assertEqual(command[command.index('--warmup-repetitions')+1],'3')

    def test_worker_without_managed_admission_refuses_before_imports(self) -> None:
        """Standalone timing cannot use just a preparation receipt."""
        worker=Path(__file__).resolve().parents[1]/'benchmarking'/'child_worker.py'
        run=subprocess.run([sys.executable,'-B',str(worker),'--candidate','old','--case','fake',
                            '--repetitions','1','--receipt',str(self.path/'absent'),'--old-root',str(self.path)],
                           capture_output=True,text=True)
        self.assertNotEqual(run.returncode,0)
        self.assertIn('timed workers require',run.stderr)

if __name__=='__main__':unittest.main()
