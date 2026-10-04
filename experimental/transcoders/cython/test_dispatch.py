"""Bounded dispatch candidate equivalence with identical handlers and trusted/cold policies."""
import datetime as dt
from decimal import Decimal
import unittest
import numpy as np
import pandas as pd
import sqlaero_cython_kernel as k

class Obj:
    """Controlled shared-ObjStr surrogate preserving overridden SQL hook semantics."""
    def __str__(self) -> str:
        """Return deliberately trusted SQL syntax."""
        return 'OBJ()'
class Custom(Obj):
    """Controlled CustomEscapeType surrogate with overridden SQL hook."""
    def __str__(self) -> str:
        """Preserve a distinct subclass hook result."""
        return 'CUSTOM()'
class Bit(Custom):
    """Controlled BIT surrogate retaining its subclass SQL hook."""
    def __str__(self) -> str:
        """Return a representative validated bit literal."""
        return '7'
class Json(Custom):
    """Controlled JSON surrogate calling the same independent codec and quote handlers."""
    def __str__(self) -> str:
        """Produce encoded JSON through public same-handler paths."""
        return k.escape(k.dumps({'x':1}).decode(),False,False)
class Raw:
    """Controlled trusted RawText surrogate."""
    value = 'RAW(100%)'
class Function:
    """Controlled SQLFunction surrogate with recursively escaped arguments."""
    args = ("a'b",1)
    def syntax(self) -> str:
        """Supply an explicitly trusted percent-format SQL skeleton."""
        return 'F(%s,%s)'
class ZeroFunction(Function):
    """Controlled no-argument SQL syntax containing a literal percent sign."""
    args = ()
    def syntax(self) -> str:
        """Return raw zero-argument syntax without percent interpolation."""
        return 'F(100%)'
class Interval:
    """Controlled SQLInterval surrogate."""
    expr = 3
    def syntax(self) -> str:
        """Supply the trusted interval skeleton."""
        return 'INTERVAL %s DAY'
class Text(str):
    """Ordinary string subclass with explicit original conversion behavior."""
    def __str__(self) -> str:
        """Return substituted text which remains quoted."""
        return "ordinary'"
class Integer(int):
    """Ordinary integer subclass with original SQL conversion behavior."""
    def __int__(self) -> int:
        """Return a converted SQL value without affecting JSON base storage."""
        return 9
class BadHash(type):
    """Metaclass whose hooks classifiers must never invoke."""
    def __hash__(self) -> int:
        """Fail if an implementation hashes the user-controlled type instead of its identity."""
        raise AssertionError('metaclass hash must not run')
    def __eq__(self, other: object) -> bool:
        """Fail if an implementation compares user-controlled types for equality."""
        raise AssertionError('metaclass equality must not run')
class Unknown(metaclass=BadHash):
    """Unsupported class testing identity-registry callback safety."""
class Failing(Obj):
    """Registered callback whose application assertion must propagate raw."""
    def __str__(self) -> str:
        """Signal the original application assertion contract."""
        raise AssertionError('invariant')

class DispatchTests(unittest.TestCase):
    """Compare precise output/error type for all candidate settings and restore baseline after each test."""
    def setUp(self) -> None:
        """Register only explicit trusted fixture classes."""
        k.configure_types(objstr=(Obj,),rawtext=(Raw,),sqlfunction=(Function,),sqlinterval=(Interval,))
        k._set_dispatch_strategy('exactif')
    def tearDown(self) -> None:
        """Restore the unchanged default and clear test trust registrations."""
        k._set_dispatch_strategy('exactif');k.configure_types()
    def outcome(self, value: object, many: bool, itemize: bool) -> tuple[object, ...]:
        """Capture result type/value or precise exception and cause without running any timing."""
        try:
            result=k.escape(value,many,itemize)
            return type(result),result
        except Exception as exc:
            return type(exc),str(exc),type(exc.__cause__) if exc.__cause__ is not None else None
    def test_all_strategies_same_public_calls(self) -> None:
        """Exercise scalar/container/array/pandas/date and every trust class with all four flags."""
        values=[None,True,False,1,2**100,-1.25,1e-7,-0.0,float('nan'),"x'\\\0",b'\xff',bytearray(b'x'),memoryview(b'xy')[::-1],
                Decimal('1.2300'),dt.datetime(2024,2,29,1,2,3,4),dt.date(2024,2,29),dt.time(1,2,3),dt.timedelta(microseconds=-1),
                Text('stored'),Integer(3),Obj(),Custom(),Bit(),Json(),Raw(),Function(),ZeroFunction(),Interval(),Failing(),Unknown(),
                [],[1,'x'],[[1,2],[3,4]],{'a':1,'b':2},np.arange(6).reshape(2,3)[:,::-1],np.array([1.1],dtype='f4'),
                np.datetime64(-1,'ns'),np.timedelta64(1,'M'),np.str_('\0'),pd.Series([1,2]),pd.DataFrame({'a':[1],'b':['x']})]
        for value in values:
            for many in (False,True):
                for itemize in (False,True):
                    k._set_dispatch_strategy('exactif');expected=self.outcome(value,many,itemize)
                    for strategy in k._dispatch_strategies():
                        k._set_dispatch_strategy(strategy)
                        self.assertEqual(self.outcome(value,many,itemize),expected,(strategy,type(value),many,itemize))
    def test_errors_cycles_and_cold_rejections(self) -> None:
        """Reject unsupported types/cycles identically without invoking metaclass dictionary hooks."""
        class D(Decimal):
            """Rejected Decimal subclass fixture."""
        class A(np.ndarray):
            """Rejected ndarray subclass fixture."""
        values=[Unknown(),D('1'),np.array(1),np.zeros((1,1,1)),np.array([1j]),np.arange(2).view(A)]
        cycle=[];cycle.append(cycle);values.append(cycle)
        for value in values:
            k._set_dispatch_strategy('exactif');expected=self.outcome(value,False,True)
            for strategy in k._dispatch_strategies():
                k._set_dispatch_strategy(strategy);self.assertEqual(self.outcome(value,False,True),expected)
    def test_catalog_and_selected_classifier(self) -> None:
        """Keep baseline default and validate exact classification acquisition in every internal candidate."""
        self.assertEqual(k._get_dispatch_strategy(),'exactif')
        self.assertEqual(k._dispatch_strategies(),('exactif','hotcold','typeid_dict'))
        for strategy in k._dispatch_strategies():
            k._set_dispatch_strategy(strategy)
            self.assertEqual(k.dispatch_code(True),1);self.assertEqual(k.dispatch_code(Integer(3)),2)
            self.assertEqual(k.dispatch_code(Decimal('1')),7);self.assertEqual(k.dispatch_code(Unknown()),0)
        with self.assertRaises(ValueError):k._set_dispatch_strategy('not-a-strategy')

if __name__=='__main__':unittest.main()
