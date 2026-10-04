"""Injected multi-backend timing coordinator with durable raw evidence.

Imports do not measure, probe or acquire locks. The shared cyTimes handshake
must be supplied by the integrator before a heavy run can be admitted.
"""
from __future__ import annotations

import hashlib
import json
import math
import random
import statistics
import time
import threading
from contextlib import contextmanager
from itertools import permutations
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Callable, Iterator, Mapping, TextIO

from process_sampler import NativeTiming
from benchmark_environment import BenchmarkLease
from protocol import Event, LoadPolicy, PROTOCOL_VERSION, Signal, balanced_orders, record_measurement, wait_for_stability


@dataclass(frozen=True)
class RunConfig:
    """Declare repetitions and gates before observing any candidate timings."""
    blocks: int
    repetitions: int
    warmup_repetitions: int
    post_guard_seconds: float
    shared_lock_path: str
    workload_id: str
    artifact_metadata: Mapping[str, object]
    shared_protocol_digest: str | None = None
    window_evidence: str | None = None
    heavy: bool = False

    def validate(self, candidate_count: int) -> None:
        """Require complete order cycles and explicit heavy-run integration evidence."""
        if candidate_count not in (2, 3, 4):
            raise ValueError('Two to four distinct candidates are required')
        if self.blocks < 1 or self.blocks % math.factorial(candidate_count):
            raise ValueError('Blocks must contain complete permutation cycles')
        if self.repetitions < 1 or self.warmup_repetitions < 0 or not math.isfinite(self.post_guard_seconds) or self.post_guard_seconds <= 0:
            raise ValueError('Invalid repetition or guard policy')
        if not self.shared_lock_path or not self.workload_id:
            raise ValueError('An explicit shared lock and workload ID are required')
        if self.heavy and (not self.shared_protocol_digest or not self.window_evidence):
            raise ValueError('Heavy timing requires reconciled protocol and verified exclusive-window evidence')


class JsonlEvidence:
    """Append and flush every event; never overwrite an earlier run artifact."""
    def __init__(self, path: Path) -> None:
        """Create a new evidence file exclusively, rejecting existing paths."""
        self.stream: TextIO = path.open('x', encoding='utf-8')
        self.lock=threading.Lock()

    def emit(self, event: Event) -> None:
        """Write strict JSON and flush each event so interruptions retain progress."""
        with self.lock:
            self.stream.write(json.dumps(event, sort_keys=True, allow_nan=False) + '\n')
            self.stream.flush()

    def close(self) -> None:
        """Close the sink after all available evidence has been flushed."""
        self.stream.close()


@contextmanager
def exclusive_lock(path: str, wait_seconds: float) -> Iterator[dict[str, object]]:
    """Reuse the cyTimes kernel-held lease and its bounded waiting semantics."""
    lease = BenchmarkLease(path)
    metadata = lease.acquire(time.monotonic() + wait_seconds, owner='sqlaero-comparison')
    try:
        yield metadata
    finally:
        lease.release()


def run_comparison(candidates: Mapping[str, Callable[[], object]], config: RunConfig,
                   policy: LoadPolicy, probe: Callable[[], Signal],
                   emit: Callable[[Event], None], clock: Callable[[], float] = time.monotonic,
                   sleep: Callable[[float], None] = time.sleep,
                   timer: Callable[[], float] = time.perf_counter) -> list[Event]:
    """Run balanced paired blocks, retaining warmup, wait and interference evidence.

    Candidate callbacks represent the same fixed workload and handler work.
    Correctness validation precedes this function; it is not inferred from timing.
    A cooperative during-run sample is taken after every callback. An independent
    sampler remains required for calls longer than the allowed sampling interval.
    """
    destination=emit
    def attributed(event:Event) -> None:
        """Tag warmups, waits and failed attempts with their workload provenance."""
        destination({**event,'workload_id':config.workload_id})
    emit=attributed
    names = tuple(candidates)
    config.validate(len(names))
    effective = {'protocol': PROTOCOL_VERSION, 'config': asdict(config), 'policy': asdict(policy), 'candidates': names}
    digest = hashlib.sha256(json.dumps(effective, sort_keys=True, allow_nan=False).encode()).hexdigest()
    results: list[Event] = []
    emit({'protocol': PROTOCOL_VERSION, 'kind': 'configuration', 'digest': digest, 'effective': effective})
    with exclusive_lock(config.shared_lock_path, policy.max_wait_seconds) as lease:
        emit({'protocol': PROTOCOL_VERSION, 'kind': 'lock-acquired', 'path': config.shared_lock_path, 'lease': lease})
        try:
            for callback in candidates.values():
                admit=getattr(callback,'admit',None)
                if admit is not None:
                    admit(lease)
            for block, order in enumerate(balanced_orders(names, config.blocks)):
                emit({'protocol': PROTOCOL_VERSION, 'kind': 'block-start', 'block': block, 'order': order})
                if not wait_for_stability(policy, probe, clock, sleep, emit):
                    emit({'protocol': PROTOCOL_VERSION, 'kind': 'run-stopped', 'block': block, 'reason': 'stability-timeout'})
                    break
                warmup_failed = False
                for name in order:
                    prepare = getattr(candidates[name], 'prepare', None)
                    if prepare is not None:
                        prepare()
                    for repetition in range(config.warmup_repetitions):
                        error = None
                        try:
                            candidates[name]()
                        except Exception as exc:
                            error = type(exc).__name__ + ': ' + str(exc)
                            warmup_failed = True
                        emit({'protocol': PROTOCOL_VERSION, 'kind': 'warmup', 'block': block,
                              'candidate': name, 'repetition': repetition, 'error': error})
                if warmup_failed:
                    emit({'protocol': PROTOCOL_VERSION, 'kind': 'block-rejected', 'block': block, 'reason': 'warmup-error'})
                    continue
                if not wait_for_stability(policy, probe, clock, sleep, emit):
                    emit({'protocol': PROTOCOL_VERSION, 'kind': 'run-stopped', 'block': block, 'reason': 'post-warmup-timeout'})
                    break
                block_results: list[Event] = []
                for name in order:
                    prepare = getattr(candidates[name], 'prepare', None)
                    if prepare is not None:
                        prepare()
                    def measure(candidate: str, sample: Callable[[], None]) -> float:
                        """Retain individual durations while excluding probe cost from totals."""
                        durations: list[float] = []
                        child_flags: list[bool] = []
                        for repetition in range(config.repetitions):
                            started = timer()
                            try:
                                returned = candidates[candidate]()
                            except Exception as exc:
                                ended = timer()
                                emit({'protocol': PROTOCOL_VERSION, 'kind': 'raw-repetition', 'block': block,
                                      'candidate': candidate, 'repetition': repetition, 'seconds': ended-started,
                                      'error': type(exc).__name__ + ': ' + str(exc)})
                                sample()
                                raise
                            elapsed = timer() - started
                            if isinstance(returned, NativeTiming):
                                elapsed = returned.seconds
                                child_flags.append(returned.child_qualified)
                                emit({'protocol':PROTOCOL_VERSION,'kind':'child-native-raw',
                                      'candidate':candidate,'block':block,'raw':returned.raw,
                                      'child_qualified':returned.child_qualified})
                            if not math.isfinite(elapsed) or elapsed <= 0:
                                raise ValueError('Timer returned a nonpositive or nonfinite duration')
                            durations.append(elapsed)
                            emit({'protocol': PROTOCOL_VERSION, 'kind': 'raw-repetition', 'block': block,
                                  'candidate': candidate, 'repetition': repetition, 'seconds': elapsed, 'error': None})
                            sample()
                        if child_flags and not all(child_flags):
                            raise ValueError('Independent child monitoring missing or interfered; raw timings retained')
                        return statistics.mean(durations)
                    result = record_measurement(name, block, policy, probe, clock, measure, emit)
                    result.update(configuration_digest=digest, workload_id=config.workload_id,
                                  balance_cycle=block//math.factorial(len(names)), order_index=block, order=order)
                    # A guard window continues observing interference after measured work.
                    guard_started = clock()
                    while True:
                        signals = dict(probe())
                        ok, reasons = policy.classify(signals)
                        emit({'protocol': PROTOCOL_VERSION, 'kind': 'post-guard', 'candidate': name,
                              'block': block, 'time': clock(), 'signals': signals, 'stable': ok, 'reasons': reasons})
                        if not ok:
                            result['eligible'] = False
                            result['interference'].extend('guard:' + reason for reason in reasons)
                        if clock() - guard_started >= config.post_guard_seconds:
                            break
                        sleep(min(policy.poll_seconds, config.post_guard_seconds - (clock()-guard_started)))
                    block_results.append(result)
                    emit({**result, 'kind': 'measurement-qualified'})
                eligible = all(result['eligible'] for result in block_results)
                emit({'protocol': PROTOCOL_VERSION, 'kind': 'block-finished', 'block': block, 'eligible': eligible})
                for result in block_results:
                    result['block_eligible'] = eligible
                    emit({**result, 'kind': 'paired-result'})
                results.extend(block_results)
        finally:
            emit({'protocol': PROTOCOL_VERSION, 'kind': 'lock-releasing', 'path': config.shared_lock_path})
    return results


def summarize(results: list[Event], reference: str, seed: int = 0,
              bootstrap_repeats: int = 1000) -> dict[str, object]:
    """Summarize qualified paired block ratios with block-level bootstrap uncertainty."""
    if bootstrap_repeats < 1:
        raise ValueError('Bootstrap repeats must be positive')
    if len({(row.get('configuration_digest'),row.get('workload_id')) for row in results}) > 1:
        raise ValueError('Cannot pool incompatible configurations or workloads')
    if any(row.get('protocol') != PROTOCOL_VERSION for row in results):
        raise ValueError('Cannot summarize mixed protocol revisions')
    accepted=qualified_cycles(results)
    blocks={cycle:{name:statistics.mean(float(row['value']) for row in rows if row['candidate']==name)
                   for name in {str(row['candidate']) for row in rows}}
            for cycle,rows in accepted.items()}
    names = sorted({str(row['candidate']) for row in results} - {reference})
    output: dict[str, object] = {'protocol': PROTOCOL_VERSION, 'total_measurements': len(results),
                               'qualified_measurements': sum(len(rows) for rows in accepted.values()),
                               'paired_ratios': {}}
    generator = random.Random(seed)
    for name in names:
        ratios=[]
        for rows in accepted.values():
            paired:dict[int,dict[str,float]]={}
            for row in rows:
                paired.setdefault(int(row['block']),{})[str(row['candidate'])]=float(row['value'])
            within=[values[name]/values[reference] for values in paired.values() if name in values and reference in values]
            if within:
                ratios.append(statistics.median(within))
        if not ratios:
            output['paired_ratios'][name] = {'blocks': 0, 'interval': None}
            continue
        samples = sorted(statistics.median(generator.choices(ratios, k=len(ratios))) for _ in range(bootstrap_repeats))
        output['paired_ratios'][name] = {'blocks': len(ratios), 'median': statistics.median(ratios),
                                        'minimum': min(ratios), 'maximum': max(ratios),
                                        'interval': [samples[int(.025*(len(samples)-1))], samples[int(.975*(len(samples)-1))]] if len(ratios)>1 else None,
                                        'independent_unit':'complete balanced permutation cycle',
                                        'estimand':'median of within-order paired ratios, aggregated equally by cycle'}
    return output


def three_way_report(by_type: Mapping[str, list[Event]], events: list[Event] | None = None) -> dict[str, object]:
    """Report each type's real times and all three paired comparisons separately."""
    report: dict[str, object] = {'protocol': PROTOCOL_VERSION, 'by_type': {}, 'diagnostics':{
        'wait_seconds':sum(float(event.get('waited_seconds',0)) for event in events or [] if event['kind'] in ('window-ready','wait-timeout')),
        'wait_timeouts':sum(event['kind']=='wait-timeout' for event in events or []),
        'events_supplied':events is not None}}
    for type_name, results in by_type.items():
        if results and {row['candidate'] for row in results} != {'old','cython','rust'}:
            raise ValueError('Every formal report must include the three public entries')
        qualified_rows=[row for rows in qualified_cycles(results).values() for row in rows]
        workload=str(results[0].get('workload_id',type_name)) if results else type_name
        relevant=[event for event in events or [] if event.get('workload_id')==workload or event.get('case')==type_name]
        warmup_failures=sum(event['kind']=='warmup' and event.get('error') is not None for event in relevant)
        attempted_cycles={int(event['block'])//6 for event in relevant if event['kind']=='block-start'}
        attempted_cycles.update(row['balance_cycle'] for row in results)
        distributions: dict[str, object] = {}
        for name in ('old','cython','rust'):
            values = [float(row['value']) for row in results if row['candidate']==name and row.get('value') is not None]
            qualified = [float(row['value']) for row in qualified_rows if row['candidate']==name]
            distributions[name] = {'all_retained_means_seconds': values, 'qualified_means_seconds': qualified,
                'median_seconds': statistics.median(qualified) if qualified else None,
                'stdev_seconds': statistics.stdev(qualified) if len(qualified)>1 else None,
                'coefficient_of_variation': statistics.stdev(qualified)/statistics.mean(qualified) if len(qualified)>1 else None,
                'p05_seconds':percentile(qualified,.05) if qualified else None,
                'p95_seconds':percentile(qualified,.95) if qualified else None}
        old = summarize(results,'old')['paired_ratios']
        cross = summarize(results,'cython')['paired_ratios']
        absent={'blocks':0,'interval':None}
        old={name:old.get(name,absent) for name in ('cython','rust')}
        cross={'rust':cross.get('rust',absent)}
        report['by_type'][type_name] = {'times': distributions,
            'ratios': {'cython_over_old':old['cython'], 'rust_over_old':old['rust'], 'rust_over_cython':cross['rust']},
            'ratio_definition':'candidate seconds / reference seconds; below one is faster',
            'semantic_differences_excluded':True,
            'diagnostics':{'failure_count':sum(row.get('error') is not None for row in results)+warmup_failures,
                           'warmup_failure_count':warmup_failures,
                           'interfered_count':sum(bool(row.get('interference')) for row in results),
                           'retained_measurements':len(results),
                           'qualified_cycles':len(qualified_cycles(results)),
                           'unqualified_or_incomplete_cycles':len(attempted_cycles)-len(qualified_cycles(results))}, 'uncertainty':'Whole balanced-cycle bootstrap median interval; small samples are descriptive'}
    return report


def qualified_cycles(results: list[Event]) -> dict[int,list[Event]]:
    """Select complete uncontaminated permutation cycles with unique row identities."""
    candidates={str(row['candidate']) for row in results}
    expected_orders=set(permutations(sorted(candidates)))
    cycles: dict[int,list[Event]]={}
    identities:set[tuple[int,str]]=set()
    for row in results:
        identity=(int(row['block']),str(row['candidate']))
        if identity in identities:
            raise ValueError('Duplicate block/candidate measurement')
        identities.add(identity)
        cycles.setdefault(int(row['balance_cycle']),[]).append(row)
    accepted={}
    for cycle,rows in cycles.items():
        if len(rows)!=len(expected_orders)*len(candidates):
            continue
        by_block:dict[int,list[Event]]={}
        for row in rows: by_block.setdefault(int(row['block']),[]).append(row)
        orders={tuple(rows_in_block[0]['order']) for rows_in_block in by_block.values()}
        if orders!=expected_orders or len(by_block)!=len(expected_orders):
            raise ValueError('Incorrect balanced permutation coverage')
        for rows_in_block in by_block.values():
            if {str(row['candidate']) for row in rows_in_block}!=candidates or len({tuple(row['order']) for row in rows_in_block})!=1:
                raise ValueError('Incomplete candidate/order metadata')
        if not all(row.get('eligible') and row.get('block_eligible') for row in rows):
            continue
        if any(row.get('value') is None or not math.isfinite(float(row['value'])) or float(row['value'])<=0 for row in rows):
            raise ValueError('Qualified timings must be finite and positive')
        accepted[cycle]=rows
    return accepted


def percentile(values: list[float], fraction: float) -> float:
    """Interpolate an explicitly declared quantile of retained qualified observations."""
    ordered=sorted(values)
    if not ordered or not 0<=fraction<=1:
        raise ValueError('A nonempty sample and unit-range fraction are required')
    position=(len(ordered)-1)*fraction
    left=int(position);right=min(left+1,len(ordered)-1)
    return ordered[left]+(ordered[right]-ordered[left])*(position-left)
