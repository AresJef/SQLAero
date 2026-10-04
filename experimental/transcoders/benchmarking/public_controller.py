"""Three-party public-entry orchestration using separately monitored child workers."""
from __future__ import annotations

import json
import math
import os
import sys
import tempfile
from dataclasses import asdict, replace
from pathlib import Path
from typing import Callable

from backends import Case, cases
from benchmark_environment import BenchmarkPolicy, PROTOCOL as HOST_PROTOCOL, protocol_identity
from prepare import verify_receipt
from process_sampler import NativeTiming, monitor_child, parse_timing
from protocol import Event, LoadPolicy, Signal
from runner import RunConfig, run_comparison, three_way_report, summarize


class PublicInvocation:
    """Create child admission only when the runner supplies its acquired parent lease."""
    def __init__(self, name: str, case_name: str, receipt_path: Path, old_root: Path,
                 config: RunConfig, policy: LoadPolicy, probe: Callable[[], Signal],
                 emit: Callable[[Event], None], timeout: float, directory: Path,
                 dispatch_strategy: str | None = None) -> None:
        """Bind one public candidate to an eventual live-parent admission receipt."""
        self.name, self.case_name = name, case_name
        self.receipt_path, self.old_root = receipt_path, old_root
        self.config, self.policy, self.probe, self.emit = config, policy, probe, emit
        self.timeout = timeout
        self.managed_path = directory / ((dispatch_strategy or name) + '.json')
        self.admitted = False
        self.dispatch_strategy=dispatch_strategy

    def admit(self, lease: dict[str, object]) -> None:
        """Write an exclusive managed receipt inside the kernel-held lease scope."""
        self.config.validate(3)
        if not self.config.heavy:
            raise ValueError('Managed worker requires explicit heavy-run admission')
        if lease.get('pid') != os.getpid() or lease.get('protocol') != HOST_PROTOCOL:
            raise ValueError('Managed receipt requires this controller lease')
        if Path(str(lease['path'])).resolve() != Path(self.config.shared_lock_path).resolve():
            raise ValueError('Managed receipt lease path mismatch')
        receipt = {'protocol': HOST_PROTOCOL, 'controller_pid': os.getpid(), 'lease': lease,
                   'policy_identity': protocol_identity(BenchmarkPolicy()),
                   'sqlaero_policy': asdict(self.policy),
                   'shared_protocol_digest': self.config.shared_protocol_digest,
                   'window_evidence': self.config.window_evidence}
        with self.managed_path.open('x', encoding='utf-8') as stream:
            json.dump(receipt, stream, sort_keys=True, allow_nan=False)
        self.admitted = True

    def __call__(self) -> NativeTiming:
        """Launch one worker with warmups in its own eventual measured process."""
        if not self.admitted:
            raise ValueError('Worker invocation requires acquired parent lease admission')
        worker = Path(__file__).with_name('child_worker.py')
        command = [sys.executable, '-B', str(worker), '--candidate', self.name, '--case', self.case_name,
                   '--repetitions', str(self.config.repetitions),
                   '--warmup-repetitions', str(self.config.warmup_repetitions),
                   '--calls-per-repetition',str(self.config.calls_per_repetition),
                   '--minimum-batch-seconds',str(self.config.minimum_batch_seconds),
                   '--receipt', str(self.receipt_path), '--old-root', str(self.old_root),
                   '--managed-receipt', str(self.managed_path)]
        if self.config.maximum_repetitions is not None:
            command.extend(['--maximum-repetitions',str(self.config.maximum_repetitions)])
        if self.dispatch_strategy is not None:
            command.extend(['--dispatch-strategy',self.dispatch_strategy])
        def attributed(event: Event) -> None:
            """Tag every child observation and raw receipt with its public candidate."""
            self.emit({**event, 'candidate': self.dispatch_strategy or self.name, 'case': self.case_name})
        return parse_timing(monitor_child(command, self.policy, self.probe, attributed, self.timeout))


def run_public_cases(receipt_path: Path, old_root: Path, config: RunConfig,
                     policy: LoadPolicy, probe: Callable[[], Signal],
                     emit: Callable[[Event],None], child_timeout_seconds: float,
                     case_names: tuple[str,...] | None = None) -> dict[str,object]:
    """Run attested equivalent public types and retain an explicit partial-run status."""
    if not config.heavy:
        raise ValueError('Formal public timing requires explicit heavy-run admission')
    if not math.isfinite(child_timeout_seconds) or child_timeout_seconds <= 0:
        raise ValueError('Child timeout must be finite and positive')
    config.validate(3)
    expected = verify_receipt(receipt_path, old_root)
    if expected['shared_lock_path'] != config.shared_lock_path:
        raise ValueError('Receipt and controller must use the same cyTimes shared lock')
    retained_events:list[Event]=[]
    destination=emit
    def retain(event:Event) -> None:
        """Keep report diagnostics alongside the caller's durable raw sink."""
        retained_events.append(event)
        destination(event)
    emit=retain
    catalogue = select_cases(case_names)
    semantics = {row['case']: row for row in expected['semantics']}
    eligible_cases = []
    for case in catalogue:
        baseline_error = 'error' in semantics[case.name]['outputs']['old']
        if not case.equivalent or baseline_error:
            emit({'protocol': expected['protocol'], 'kind': 'semantic-difference-excluded',
                  'case': case.name, 'reason': case.difference if not case.equivalent else 'baseline-error'})
        else:
            eligible_cases.append(case)
    by_type: dict[str, list[Event]] = {}
    completed_types = []
    incomplete_case = None
    for case in eligible_cases:
        with tempfile.TemporaryDirectory(prefix='sqlaero-managed-') as directory:
            candidates = {name: PublicInvocation(name, case.name, receipt_path, old_root,
                          config, policy, probe, emit, child_timeout_seconds, Path(directory))
                          for name in ('old', 'cython', 'rust')}
            # Outer warmups would discard workers; each measured worker warms itself.
            child_config = replace(config, repetitions=1, warmup_repetitions=0, workload_id=case.name,
                                   artifact_metadata={'artifact_receipt': expected['artifact_receipt'],
                                                      'worker_repetitions': config.repetitions,
                                                      'worker_warmup_repetitions': config.warmup_repetitions,
                                                      'worker_calls_per_repetition':config.calls_per_repetition,
                                                      'worker_minimum_batch_seconds':config.minimum_batch_seconds,
                                                      'worker_maximum_repetitions':config.maximum_repetitions})
            results = run_comparison(candidates, child_config, policy, probe, emit)
        if results:
            by_type[case.name] = results
        planned = {(block, name) for block in range(config.blocks) for name in candidates}
        actual = {(row['block'], row['candidate']) for row in results}
        if actual != planned or len(results) != len(planned):
            incomplete_case = case.name
            emit({'protocol': expected['protocol'], 'kind': 'formal-run-incomplete', 'case': case.name,
                  'planned_measurements': len(planned), 'retained_measurements': len(results)})
            break
        completed_types.append(case.name)
    return {'receipt': expected, 'statistics': three_way_report(by_type,retained_events) if by_type else None,
            'complete': len(completed_types) == len(eligible_cases),
            'completed_types': completed_types, 'incomplete_case': incomplete_case,
            'planned_types': len(eligible_cases), 'requested_case_names':case_names}


def run_dispatch_cases(backend_name: str, receipt_path: Path, old_root: Path,
                       config: RunConfig, policy: LoadPolicy, probe: Callable[[],Signal],
                       emit: Callable[[Event],None], child_timeout_seconds: float,
                       case_names: tuple[str,...] | None = None) -> dict[str,object]:
    """Compare reviewed classifiers using the same public handlers and isolated workers.

    This supplements the mandatory three-party public comparison. Strategy setup
    happens before in-worker warmup; type-ID acquisition, lookup, adapter and
    public handler work all remain inside the measured callback.
    """
    if backend_name not in ('cython','rust') or not config.heavy:
        raise ValueError('Reviewed backend and explicit formal-run admission required')
    if not math.isfinite(child_timeout_seconds) or child_timeout_seconds<=0:
        raise ValueError('Child timeout must be finite and positive')
    expected=verify_receipt(receipt_path,old_root)
    strategies=tuple(expected['dispatch_coverage'][backend_name])
    config.validate(len(strategies))
    if expected['shared_lock_path']!=config.shared_lock_path:
        raise ValueError('Receipt and controller shared lock differ')
    semantics={row['case']:row for row in expected['semantics']}
    by_type={};complete=True
    for case in select_cases(case_names):
        if not case.equivalent or 'error' in semantics[case.name]['outputs']['old']:
            continue
        with tempfile.TemporaryDirectory(prefix='sqlaero-dispatch-') as directory:
            candidates={strategy:PublicInvocation(backend_name,case.name,receipt_path,old_root,
                         config,policy,probe,emit,child_timeout_seconds,Path(directory),strategy)
                         for strategy in strategies}
            child_config=replace(config,repetitions=1,warmup_repetitions=0,workload_id=case.name,
                                 artifact_metadata={'artifact_receipt':expected['artifact_receipt'],
                                                    'backend':backend_name,'strategies':strategies,
                                                    'worker_repetitions':config.repetitions,
                                                    'worker_warmup_repetitions':config.warmup_repetitions,
                                                    'worker_calls_per_repetition':config.calls_per_repetition,
                                                    'worker_minimum_batch_seconds':config.minimum_batch_seconds,
                                                    'worker_maximum_repetitions':config.maximum_repetitions})
            results=run_comparison(candidates,child_config,policy,probe,emit)
        planned={(block,strategy) for block in range(config.blocks) for strategy in strategies}
        actual={(row['block'],row['candidate']) for row in results}
        by_type[case.name]={'retained':results,'statistics':summarize(results,strategies[0])}
        if len(results)!=len(planned) or actual!=planned:
            complete=False
            break
    return {'backend':backend_name,'strategies':strategies,'reference':strategies[0],
            'by_type':by_type,'complete':complete,'requested_case_names':case_names,
            'supplements_three_way_public_comparison':True}


def select_cases(names: tuple[str,...] | None) -> list[Case]:
    """Select a declared bounded subset without introducing unreceipted inputs."""
    catalogue=cases()
    if names is None: return catalogue
    if not names or len(set(names))!=len(names):
        raise ValueError('Declare distinct nonempty case names')
    known={case.name:case for case in catalogue}
    if not set(names)<=set(known): raise ValueError('Unknown frozen input case')
    return [known[name] for name in names]
