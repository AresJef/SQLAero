"""Isolated public-entry worker; only a controller may request timed repetitions."""
import argparse
import json
import statistics
import time
import math
import sys
from typing import Callable
from pathlib import Path
from benchmark_environment import verify_managed_receipt
from prepare import verify_receipt
from backends import cases, load_backends, validate_cases


def collect_timings(callback:Callable[[],object], minimum_repetitions:int, calls:int,
                    minimum_seconds:float, maximum_repetitions:int|None,
                    emit:Callable[[dict[str,object]],None], timer:Callable[[],float],
                    clock:Callable[[],float]) -> tuple[list[dict[str,object]],dict[str,float],bool]:
    """Run declared fixed-size batches, retain every result and cap duration calibration.

    Raw per-call normalization includes the Python adapter and actual public
    entry. Inner iterations do not become independent statistical observations.
    """
    if minimum_repetitions<1 or calls<1 or not math.isfinite(minimum_seconds) or minimum_seconds<0:
        raise ValueError('Invalid declared batching policy')
    if maximum_repetitions is not None and maximum_repetitions<minimum_repetitions:
        raise ValueError('Insufficient repetition cap')
    if minimum_seconds and maximum_repetitions is None:
        raise ValueError('Duration-based batches require an explicit finite cap')
    started=clock();ended=started;raw=[]
    while len(raw)<minimum_repetitions or ended-started<minimum_seconds:
        if maximum_repetitions is not None and len(raw)>=maximum_repetitions:break
        repetition_started=timer();error=None;attempted=0
        try:
            for _ in range(calls):
                attempted+=1
                callback()
        except Exception as exc:error=type(exc).__name__+': '+str(exc)
        elapsed=timer()-repetition_started
        row={'repetition':len(raw),'seconds':elapsed/attempted,'total_seconds':elapsed,
             'calls':attempted,'error':error}
        raw.append(row);emit(row);ended=clock()
        if error:break
    return raw,{'started':started,'ended':ended},ended-started>=minimum_seconds


def main() -> None:
    """Time exact public adapters in a child so parent probes survive native GIL holds."""
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--candidate',choices=('old','cython','rust'),required=True)
    parser.add_argument('--case',required=True)
    parser.add_argument('--dispatch-strategy')
    parser.add_argument('--calls-per-repetition',type=int,default=1)
    parser.add_argument('--minimum-batch-seconds',type=float,default=0.)
    parser.add_argument('--maximum-repetitions',type=int)
    parser.add_argument('--repetitions',type=int,required=True)
    parser.add_argument('--correctness-only',action='store_true')
    parser.add_argument('--receipt',type=Path)
    parser.add_argument('--managed-receipt',type=Path)
    parser.add_argument('--warmup-repetitions',type=int,default=0)
    parser.add_argument('--old-root',type=Path)
    args=parser.parse_args()
    if args.repetitions<1 or args.warmup_repetitions<0: parser.error('positive repetitions and nonnegative warmups required')
    if args.calls_per_repetition<1 or not math.isfinite(args.minimum_batch_seconds) or args.minimum_batch_seconds<0:
        parser.error('invalid normalization or minimum duration')
    if args.maximum_repetitions is not None and args.maximum_repetitions<args.repetitions:
        parser.error('maximum repetitions below minimum')
    if args.minimum_batch_seconds and args.maximum_repetitions is None:
        parser.error('duration-based batches require an explicit finite repetition cap')
    if not args.correctness_only:
        if args.receipt is None or args.old_root is None or args.managed_receipt is None:
            parser.error('timed workers require frozen and live managed receipts')
        managed=verify_managed_receipt(args.managed_receipt)
        if not managed.get('shared_protocol_digest') or not managed.get('window_evidence'):
            parser.error('timed workers require parent heavy-run admission')
        frozen=verify_receipt(args.receipt,args.old_root)
        if frozen['shared_lock_path'] != managed['lease']['path']:
            parser.error('frozen receipt and live parent lease differ')
    backends=load_backends()
    if args.dispatch_strategy is not None:
        module=backends[args.candidate].module
        if args.candidate=='old' or args.dispatch_strategy not in module._dispatch_strategies():
            parser.error('unknown reviewed classifier strategy')
        module._set_dispatch_strategy(args.dispatch_strategy)
    selected=next(case for case in cases() if case.name==args.case)
    if not selected.equivalent: parser.error('non-equivalent case excluded from timing')
    semantics=validate_cases(backends,[selected])
    raw=[]
    timed_interval=None
    minimum_met=True
    if args.correctness_only:
        backends[args.candidate].invoke(selected)
    else:
        for _ in range(args.warmup_repetitions):
            backends[args.candidate].invoke(selected)
        # Recheck ownership after setup/warmups and immediately before timed work.
        verify_managed_receipt(args.managed_receipt)
        def record(row:dict[str,object]) -> None:
            """Flush every raw batch to stderr so an owned-child timeout retains progress."""
            print(json.dumps({'kind':'worker-raw','candidate':args.candidate,'case':args.case,**row}),file=sys.stderr,flush=True)
        raw,timed_interval,minimum_met=collect_timings(lambda:backends[args.candidate].invoke(selected),
            args.repetitions,args.calls_per_repetition,args.minimum_batch_seconds,args.maximum_repetitions,
            record,time.perf_counter,time.monotonic)
    print(json.dumps({'candidate':args.candidate,'case':args.case,'dispatch_strategy':args.dispatch_strategy,'semantics':semantics,
                      'raw':raw,'timed_interval':timed_interval,'minimum_duration_met':minimum_met,
                      'calls_per_repetition':args.calls_per_repetition,'normalization':'seconds per actual public call','warmup_repetitions':args.warmup_repetitions,'mean_seconds':statistics.mean(row['seconds'] for row in raw) if raw else None,
                      'startup_import_and_fixture_construction_timed':False}))

if __name__=='__main__': main()
