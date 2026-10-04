"""Full-entry equivalence vectors for every reviewed classifier, never a benchmark."""
from __future__ import annotations

import importlib
import itertools
import os
import sys
import unittest
from typing import Callable, Iterator
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[1]/'benchmarking'))
from backends import Backend, cases, dispatch_candidates, outcome
import sqlaero_transcoder_bridge as bridge
from sqlaero.sqlfunc import RawText, SQLFunction
from sqlaero.sqlintvl import SQLInterval


class HostileMeta(type):
    """Expose class hashing/equality that must not enter identity classification."""
    def __hash__(cls) -> int:
        """Raise if a classifier hashes an arbitrary Python type object."""
        raise AssertionError('arbitrary class hash invoked')
    def __eq__(cls, other: object) -> bool:
        """Raise if a classifier compares arbitrary class equality."""
        raise AssertionError('arbitrary class equality invoked')


class Text(str, metaclass=HostileMeta):
    """Ordinary text subclass remains escaped despite hostile metaclass behavior."""
    def __str__(self) -> str:
        """Provide ordinary conversion text containing a SQL quote."""
        return "replacement'"


class Integer(int):
    """Numeric conversion overrides belong to the reviewed subclass slow path."""
    def __int__(self) -> int:
        """Return a value distinguishable from stored integer digits."""
        return 7
    def __str__(self) -> str:
        """Supply text that must never become trusted SQL."""
        return 'UNTRUSTED()'


class Sequence(list):
    """Override SQL iteration without changing JSON base-storage semantics."""
    def __iter__(self) -> Iterator[object]:
        """Iterate alternative ordinary SQL values."""
        return iter([7,"a'b"])


class Hook(bridge.CustomEscapeType):
    """Explicit trusted SQL subclass carrying its original payload."""
    def __str__(self) -> str:
        """Return the registered hook's SQL expression."""
        return 'NOW()'


class JSONOverride(bridge.JSON):
    """Explicit JSON subclass overrides the SQL hook before JSON writing."""
    def __str__(self) -> str:
        """Return the trusted override rather than serializing its payload."""
        return 'OVERRIDE()'


class AssertionHook(bridge.CustomEscapeType):
    """Verify that assertion failures retain the public error contract."""
    def __str__(self) -> str:
        """Raise the deliberate hook assertion without changing its error type."""
        raise AssertionError('hook assertion')


def capture(function: Callable[...,object], *args: object) -> dict[str,object]:
    """Retain return type/shape or precise failure and cause for candidate parity."""
    try:
        value=function(*args)
        return {'type':type(value).__name__,'value':repr(value)}
    except Exception as exc:
        return {'error':type(exc).__name__,'cause':type(exc.__cause__).__name__ if exc.__cause__ else None}


class DispatchCandidateTests(unittest.TestCase):
    """Compare all public shapes, trust boundaries and classes across each strategy."""
    @classmethod
    def setUpClass(cls) -> None:
        """Bind one independently compiled kernel selected by the lightweight runner."""
        cls.kernel=importlib.import_module(os.environ['SQLAERO_KERNEL'])
        cls.strategies=cls.kernel._dispatch_strategies()
        cls.baseline=cls.strategies[0]
        bridge.configure_backend(cls.kernel.__name__)

    def tearDown(self) -> None:
        """Restore the fixed identity cascade after every research test."""
        self.kernel._set_dispatch_strategy(self.baseline)

    def test_full_public_catalogue(self) -> None:
        """All419 type/flag/field representatives match the same baseline handlers."""
        backend=Backend('experimental',self.kernel)
        self.kernel._set_dispatch_strategy(self.baseline)
        catalogue=cases()
        expected=[outcome(backend,case) for case in catalogue]
        for strategy in self.strategies:
            self.kernel._set_dispatch_strategy(strategy)
            for case,reference in zip(catalogue,expected):
                with self.subTest(strategy=strategy,case=case.name):
                    self.assertEqual(outcome(backend,case),reference)

    def test_trusted_subclasses_errors_and_shapes(self) -> None:
        """Trust priority, overrides, invalid domains and four shape flags stay identical."""
        cycle=[];cycle.append(cycle)
        values=[Text('stored'),Integer(3),Sequence([1]),Hook('x'),JSONOverride({}),
                AssertionHook('x'),bridge.BIT(b'123456789'),bridge.BIT(True),
                bridge.JSON({'x':'雪'}),bridge.JSON(object()),RawText('NOW()'),
                SQLFunction('F',0),SQLFunction('F',2,1,"a'b"),SQLInterval('DAY',1),
                cycle,[[1,'x'],[None,True]],[],[[],[]],object()]
        self.kernel._set_dispatch_strategy(self.baseline)
        expected={(index,many,itemize):capture(self.kernel.escape,value,many,itemize)
                  for index,value in enumerate(values) for many,itemize in itertools.product((False,True),repeat=2)}
        for strategy in self.strategies:
            self.kernel._set_dispatch_strategy(strategy)
            for index,value in enumerate(values):
                for many,itemize in itertools.product((False,True),repeat=2):
                    with self.subTest(strategy=strategy,index=index,many=many,itemize=itemize):
                        self.assertEqual(capture(self.kernel.escape,value,many,itemize),expected[index,many,itemize])
        self.assertEqual(expected[0,False,False]['value'],repr("'replacement\\''"))
        self.assertEqual(expected[3,False,False]['value'],repr('NOW()'))
        self.assertEqual(expected[4,False,False]['value'],repr('OVERRIDE()'))
        self.assertEqual(expected[5,False,False]['error'],'AssertionError')

    def test_control_adapters_and_invalid_strategy(self) -> None:
        """Control setup remains outside full public calls and unknown selectors reject."""
        backend=Backend('experimental',self.kernel)
        candidates=dispatch_candidates(backend,cases()[2])
        self.assertEqual(tuple(candidates),self.strategies)
        for candidate in candidates.values():
            candidate.prepare()
            self.assertEqual(candidate(),str(123456789))
        with self.assertRaises(ValueError):self.kernel._set_dispatch_strategy('unknown')

if __name__=='__main__':unittest.main()
