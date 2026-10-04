"""One authorized, bounded real three-way cycle; never execute on import."""
from __future__ import annotations

import argparse
import hashlib
import json
from dataclasses import asdict
from pathlib import Path

from benchmark_environment import BenchmarkPolicy, protocol_identity, shared_lock_path
from common_host import CommonHostProbe
from protocol import LoadPolicy, PROTOCOL_VERSION
from public_controller import run_public_cases
from runner import JsonlEvidence, RunConfig


def configuration(receipt: Path, reconciliation: Path) -> tuple[RunConfig,LoadPolicy]:
    """Attest common policy and already verified old-waiter/lease evidence."""
    evidence=json.loads(reconciliation.read_text())
    if not evidence.get('same_lock_verified') or not evidence.get('evidence'):
        raise ValueError('Verified old-waiter and literal-lock receipts required')
    lock=str(shared_lock_path().resolve())
    if lock!=evidence['resolved_literal_lock_path']:
        raise ValueError('Shared literal lock changed')
    policy=LoadPolicy(45.,.45,1.,30.,10,90.,2.,0.)
    identity={'common':protocol_identity(BenchmarkPolicy()),'strict':asdict(policy)}
    digest=hashlib.sha256(json.dumps(identity,sort_keys=True).encode()).hexdigest()
    proof=hashlib.sha256(reconciliation.read_bytes()).hexdigest()
    config=RunConfig(6,32,128,2.,lock,'str',{},digest,proof,True,
                     calls_per_repetition=16384,minimum_batch_seconds=4.,maximum_repetitions=8192)
    return config,policy


def main() -> None:
    """Prepare or run only the declared one-case six-order cycle without weakening gates."""
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action',choices=('prepare','run'))
    parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--receipt',type=Path,required=True)
    parser.add_argument('--old-root',type=Path,required=True)
    args=parser.parse_args()
    reconciliation=Path(__file__).with_name('shared-protocol-reconciliation.json')
    config,policy=configuration(args.receipt,reconciliation)
    args.output.mkdir(parents=True,exist_ok=False)
    declared={'protocol':PROTOCOL_VERSION,'config':asdict(config),'policy':asdict(policy),
              'selected_cases':['str'],'independent_cycles':1,'statistical_claim':'descriptive only; no interval from one cycle',
              'fairness':'Exactly one SQLAero cycle, then release; busy lock waits within90seconds and postpones',
              'common_policy_optional_signals':'Unavailable temperature/throttle/available-memory remain explicit; v2 does not exclude all interference'}
    (args.output/'declared.json').write_text(json.dumps(declared,indent=2)+'\n')
    if args.action=='prepare':
        print(json.dumps({'output':str(args.output),'status':'prepared','measurements':0}))
        return
    sink=JsonlEvidence(args.output/'raw.jsonl')
    probe=CommonHostProbe(sink.emit)
    try:
        result=run_public_cases(args.receipt.resolve(),args.old_root.resolve(),config,policy,probe,
                                sink.emit,60.,case_names=('str',))
        state={'status':'cycle-attempt-complete' if result['complete'] else 'incomplete-retained',
               'result':result,'heavy_workload_scope':'one scalar str workload only',
               'no_speed_winner_selected':True}
    except Exception as exc:
        sink.emit({'protocol':PROTOCOL_VERSION,'kind':'run-exception','error':type(exc).__name__+': '+str(exc)})
        state={'status':'postponed-or-failed-retained','error':type(exc).__name__+': '+str(exc),
               'no_speed_winner_selected':True}
    finally:
        sink.close()
    (args.output/'result.json').write_text(json.dumps(state,indent=2,allow_nan=False)+'\n')
    print(json.dumps({'output':str(args.output),'status':state['status']}))

if __name__=='__main__':main()
