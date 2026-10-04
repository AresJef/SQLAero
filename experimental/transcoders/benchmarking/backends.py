"""Real three-way public-entry workloads and immutable baseline receipts."""
from __future__ import annotations

import hashlib
import importlib
import importlib.metadata
import importlib.util
import itertools
import json
import platform
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Callable, Mapping


@dataclass(frozen=True)
class Case:
    """One fixed input/flag contract shared by all Python public-entry adapters."""
    name: str
    operation: str
    value: object
    options: tuple[object, ...]
    equivalent: bool = True
    difference: str | None = None


@dataclass(frozen=True)
class Backend:
    """A module-backed public escape/decode adapter including all Python call cost."""
    name: str
    module: object

    def invoke(self, case: Case) -> object:
        """Invoke the actual public function with unchanged input and flags."""
        return getattr(self.module, case.operation)(case.value, *case.options)


def sha256(path: Path) -> str:
    """Hash an actual source or native artifact without modifying it."""
    return hashlib.sha256(path.read_bytes()).hexdigest()


def load_backends() -> dict[str, Backend]:
    """Import the frozen legacy sqlcycli and both independent experimental kernels.

    The caller must supply explicit PYTHONPATH entries. A renamed SQLAero module
    is not substituted for the original sqlcycli public entry.
    """
    return {name: Backend(name, importlib.import_module(module)) for name, module in
            (('old', 'sqlcycli.transcode'), ('cython', 'sqlaero_cython_kernel'), ('rust', 'sqlaero_rust_kernel'))}


def cases() -> list[Case]:
    """Declare tiny representative fixtures; production sweeps extend this catalogue."""
    import datetime as dt
    from decimal import Decimal
    import numpy as np
    representatives = [Case('none', 'escape', None, (False, True)),
            Case('bool', 'escape', True, (False, True)),
            Case('int', 'escape', 123456789, (False, True)),
            Case('float', 'escape', 1.23456789, (False, True)),
            Case('str', 'escape', "雪'a\\b\n", (False, True)),
            Case('bytes', 'escape', b"a'b\\c", (False, True)),
            Case('decimal', 'escape', Decimal('1.2300'), (False, True)),
            Case('date', 'escape', dt.date(2024,2,29), (False, True)),
            Case('datetime', 'escape', dt.datetime(2024,2,29,12,34,56,123456), (False, True)),
            Case('list', 'escape', [1,'x',None], (False, True)),
            Case('numpy-int32', 'escape', np.int32(12), (False, True)),
            Case('array-int64', 'escape', np.array([1,2,3],dtype=np.int64), (False, True)),
            Case('decode-int', 'decode', b'12345', (3,b'utf8',False,False,False,False)),
            Case('decode-json', 'decode', b'{"x":[1,true,null]}', (245,b'utf8',False,False,False,True)),
            Case('decode-bit-9', 'decode', b'123456789', (16,b'utf8',False,False,True,False),
                 False, 'Legacy truncates oversized BIT; new kernels reject. Never an equivalent speedup.')]
    matrix_path=Path(__file__).resolve().parents[1]/'tests/type_matrix.py'
    specification=importlib.util.spec_from_file_location('sqlaero_benchmark_type_matrix',matrix_path)
    module=importlib.util.module_from_spec(specification)
    sys.modules[specification.name]=module
    specification.loader.exec_module(module)
    for name,value in module.cases():
        for many,itemize in itertools.product((False,True),repeat=2):
            representatives.append(Case(f'type-{name}-many{int(many)}-itemize{int(itemize)}',
                                        'escape',value,(many,itemize)))
    return representatives



def outcome(backend: Backend, case: Case) -> dict[str, str]:
    """Capture exact output type/value or failure class for parity eligibility."""
    try:
        value = backend.invoke(case)
        return {'type': type(value).__name__, 'value': repr(value)}
    except Exception as exc:
        return {'error': type(exc).__name__}


def validate_cases(backends: Mapping[str, Backend], inputs: list[Case]) -> list[dict[str, object]]:
    """Reject any undeclared three-way semantic difference before timing."""
    if set(backends) != {'old','cython','rust'}:
        raise ValueError('Formal public comparison requires old, cython and rust')
    rows: list[dict[str, object]] = []
    for case in inputs:
        outputs = {name: outcome(backend, case) for name, backend in backends.items()}
        parity = all(value == outputs['old'] for value in outputs.values())
        row = {'case': case.name, 'outputs': outputs, 'equivalent': case.equivalent,
               'parity': parity, 'difference': case.difference}
        rows.append(row)
        if case.equivalent and not parity:
            raise ValueError('Undeclared semantic difference: ' + json.dumps(row))
    return rows


def receipt(backends: Mapping[str, Backend], old_root: Path) -> dict[str, object]:
    """Attest interpreter, dependencies, frozen old source and loaded native binaries."""
    dependencies = {}
    for name in ('orjson','numpy','pandas','cytimes'):
        dependencies[name] = importlib.metadata.version(name)
    old_source = old_root/'src/sqlcycli/transcode.pyx'
    modules = {}
    for name, backend in backends.items():
        path = Path(backend.module.__file__).resolve()
        files = {str(path): sha256(path)}
        if name == 'rust':
            for native in path.parent.glob('*.so'):
                files[str(native)] = sha256(native)
        modules[name] = files
    git = subprocess.run(['git','rev-parse','HEAD'],cwd=old_root,capture_output=True,text=True,check=True)
    return {'python': sys.version, 'executable': sys.executable, 'platform': platform.platform(),
            'dependencies': dependencies, 'old_root': str(old_root.resolve()),
            'old_git_head': git.stdout.strip(), 'old_source_sha256': sha256(old_source),
            'old_source_files': {str(path.relative_to(old_root)):sha256(path) for path in
                [old_source,old_source.with_suffix('.pxd'),old_source.with_suffix('.pyi'),old_root/'pyproject.toml',old_root/'setup.py']},
            'old_source_native_consistency': 'Existing native artifact frozen by hash; not rebuilt or independently attested against source',
            'modules': modules, 'old_orjson_retained': True}


@dataclass(frozen=True)
class DispatchInvocation:
    """Research strategy with control setup outside timed public invocations."""
    backend: Backend
    case: Case
    strategy: str

    def prepare(self) -> None:
        """Select the classifier once before warmup or one timed candidate batch."""
        self.backend.module._set_dispatch_strategy(self.strategy)

    def __call__(self) -> object:
        """Include public entry, type acquisition and handler costs in timing."""
        return self.backend.invoke(self.case)


def rust_dispatch_candidates(backend: Backend, case: Case) -> dict[str, Callable[[], object]]:
    """Return four classifier candidates with identical handlers and public inputs."""
    return {strategy: DispatchInvocation(backend,case,strategy)
            for strategy in ('cascade','type_id','function_table','hot_registry')}
