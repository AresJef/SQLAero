"""Prepare immutable three-way receipts and smoke correctness without timing.

No heavy-run CLI is exposed until the joint cyTimes protocol/window is verified.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path

from backends import cases, load_backends, receipt, validate_cases
from benchmark_environment import shared_lock_path, PROTOCOL as HOST_PROTOCOL
from protocol import PROTOCOL_VERSION


def implementation_hashes() -> dict[str,str]:
    """Freeze every coordinator/probe/worker source alongside the protocol label."""
    root=Path(__file__).parent
    files={path.name:hashlib.sha256(path.read_bytes()).hexdigest() for path in sorted(root.glob('*.py'))}
    matrix=root.parent/'tests/type_matrix.py'
    files['../tests/type_matrix.py']=hashlib.sha256(matrix.read_bytes()).hexdigest()
    return files


def prepare(output: Path, old_root: Path) -> dict[str, object]:
    """Save actual baseline/new artifacts, input semantics and the cooperative lock."""
    backends=load_backends()
    rows=validate_cases(backends,cases())
    module=Path(__file__).with_name('benchmark_environment.py')
    result={'protocol':PROTOCOL_VERSION,'host_protocol':HOST_PROTOCOL,
            'host_module_sha256':hashlib.sha256(module.read_bytes()).hexdigest(),
            'implementation_sha256':implementation_hashes(),
            'shared_lock_path':str(shared_lock_path().resolve()),'artifact_receipt':receipt(backends,old_root),
            'semantics':rows,'measurements':0,'heavy_timing_admitted':False,
            'pending':['joint cyTimes policy reconciliation','verified mutual exclusion window',
                       'independent run-time sampler for long native calls','expanded type workload catalogue'],
            'dispatch_coverage':{name:list(backends[name].module._dispatch_strategies()) for name in ('cython','rust')}}
    with output.open('x',encoding='utf-8') as stream:
        json.dump(result,stream,indent=2,sort_keys=True,allow_nan=False)
        stream.write('\n')
    return result


def verify_receipt(path: Path, old_root: Path) -> dict[str, object]:
    """Reject changed source, binaries, interpreter or dependency versions before timing."""
    expected=json.loads(path.read_text())
    actual=receipt(load_backends(),old_root)
    if expected['implementation_sha256']!=implementation_hashes():
        raise ValueError('Measurement implementation changed')
    if actual!=expected['artifact_receipt']:
        raise ValueError('Frozen backend receipt changed; create a separately versioned experiment')
    host_module=Path(__file__).with_name('benchmark_environment.py')
    if expected['host_protocol']!=HOST_PROTOCOL or expected['host_module_sha256']!=hashlib.sha256(host_module.read_bytes()).hexdigest():
        raise ValueError('Shared host protocol/module changed')
    if expected['protocol']!=PROTOCOL_VERSION:
        raise ValueError('Protocol revision changed')
    return expected


def main() -> None:
    """Expose receipt preparation and verification only, never a timing command."""
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action',choices=('prepare','verify'))
    parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--old-root',type=Path,required=True)
    args=parser.parse_args()
    if args.action=='prepare': result=prepare(args.output,args.old_root)
    else: result=verify_receipt(args.output,args.old_root)
    print(json.dumps({'protocol':result['protocol'],'measurements':0,'receipt':str(args.output)}))

if __name__=='__main__': main()
