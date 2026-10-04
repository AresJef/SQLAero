"""Isolated public-entry worker; only a controller may request timed repetitions."""
import argparse
import json
import statistics
import time
from pathlib import Path
from benchmark_environment import verify_managed_receipt
from prepare import verify_receipt
from backends import cases, load_backends, validate_cases


def main() -> None:
    """Time exact public adapters in a child so parent probes survive native GIL holds."""
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--candidate',choices=('old','cython','rust'),required=True)
    parser.add_argument('--case',required=True)
    parser.add_argument('--repetitions',type=int,required=True)
    parser.add_argument('--correctness-only',action='store_true')
    parser.add_argument('--receipt',type=Path)
    parser.add_argument('--managed-receipt',type=Path)
    parser.add_argument('--warmup-repetitions',type=int,default=0)
    parser.add_argument('--old-root',type=Path)
    args=parser.parse_args()
    if args.repetitions<1 or args.warmup_repetitions<0: parser.error('positive repetitions and nonnegative warmups required')
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
    selected=next(case for case in cases() if case.name==args.case)
    if not selected.equivalent: parser.error('non-equivalent case excluded from timing')
    semantics=validate_cases(backends,[selected])
    raw=[]
    timed_interval=None
    if args.correctness_only:
        backends[args.candidate].invoke(selected)
    else:
        for _ in range(args.warmup_repetitions):
            backends[args.candidate].invoke(selected)
        # Recheck ownership after setup/warmups and immediately before timed work.
        verify_managed_receipt(args.managed_receipt)
        batch_started=time.monotonic()
        for repetition in range(args.repetitions):
            started=time.perf_counter()
            error=None
            try: backends[args.candidate].invoke(selected)
            except Exception as exc: error=type(exc).__name__+': '+str(exc)
            elapsed=time.perf_counter()-started
            raw.append({'repetition':repetition,'seconds':elapsed,'error':error})
        timed_interval={'started':batch_started,'ended':time.monotonic()}
    print(json.dumps({'candidate':args.candidate,'case':args.case,'semantics':semantics,
                      'raw':raw,'timed_interval':timed_interval,'warmup_repetitions':args.warmup_repetitions,'mean_seconds':statistics.mean(row['seconds'] for row in raw) if raw else None,
                      'startup_import_and_fixture_construction_timed':False}))

if __name__=='__main__': main()
