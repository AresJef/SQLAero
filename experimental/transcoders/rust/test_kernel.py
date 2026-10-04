"""Independent compact validation, never benchmark or algorithm implementation."""
import unittest, datetime as dt, dataclasses, enum, uuid, random
import numpy as np
import pandas as pd
import cytimes
import sqlaero_rust_kernel as k
from sqlaero import errors
from sqlaero.sqlfunc import SQLFunction,RawText
from sqlaero.sqlintvl import SQLInterval
class KernelDeep(unittest.TestCase):
    """Exercise safety boundaries and the complete declared type families."""
    def test_layouts(self) -> None:
        """Owned native loops preserve all supported memory layouts and dtype widths."""
        for dtype in ('int8','int16','int32','int64','uint8','uint16','uint32','uint64','float16','float32','float64','bool'):
            a=np.arange(12).reshape(3,4).astype(dtype)
            variants=(a,a.T,np.asfortranarray(a),a[::-1,::-1],a[:,::2],np.broadcast_to(a[:1],(3,4)),a.astype(a.dtype.newbyteorder('>')))
            for v in variants:
                result=k.escape(v)
                self.assertEqual(len(result),v.shape[0])
                expected=[tuple(k.escape(v[i,j]) for j in range(v.shape[1])) for i in range(v.shape[0])]
                if dtype=='float16': expected=[tuple(k.escape(float(v[i,j])) for j in range(v.shape[1])) for i in range(v.shape[0])]
                self.assertEqual(result,expected)
            raw=np.zeros(a.nbytes+1,dtype='uint8'); unaligned=np.ndarray(a.shape,dtype=a.dtype,buffer=raw,offset=1);unaligned[:]=a
            self.assertEqual(k.escape(unaligned),k.escape(a))
            a.flags.writeable=False;self.assertEqual(k.escape(a),k.escape(a.copy()))
    def test_shapes_strings(self) -> None:
        """Object and string items, empty shapes, rejected dimensions and byte transport."""
        for dtype in ('object','S10','U10'):
            a=np.array([['a',"b'"],['雪','']],dtype=dtype) if dtype!='S10' else np.array([[b'a',b"b'"],[b'\xff',b'']],dtype=dtype)
            self.assertEqual(k.escape(a[::-1]),[tuple(k.escape(x)for x in row)for row in a[::-1]])
        for shape in ((0,), (0,2),(2,0),(0,0)):
            a=np.empty(shape);self.assertEqual(k.escape(a,False,False),'()');self.assertEqual(k.escape(a),() if len(shape)==1 else [])
        for a in (np.array(1),np.zeros((2,2,2)),np.array([1j]),np.zeros(2,dtype=[('a','i4')])):
            with self.assertRaises(Exception): k.escape(a)
        self.assertEqual(k.escape('\udcff\ud800'),"'\udcff\ud800'")
        self.assertEqual(k.escape(b'\xff'),"_binary'\udcff'")
        self.assertEqual(k.escape(memoryview(b"x'")),"_binary'x\\''")
    def test_temporal(self) -> None:
        """NumPy units, pre-epoch flooring, NaT, timezone local fields and cytimes."""
        for unit in ('ns','us','ms','s','m','h','D','W','M','Y'):
            for n in (-1,0,1):
                a=np.array([n],dtype='datetime64['+unit+']');self.assertEqual(k.escape(a),(k.escape(a[0]),))
        for unit in ('ns','us','ms','s','m','h','D','W','M','Y'):
            a=np.array([-1,0,1],dtype='timedelta64['+unit+']');self.assertEqual(k.escape(a),tuple(k.escape(x) for x in a))
        self.assertEqual(k.escape(np.timedelta64(-1,'ns')), "'-00:00:00.000001'")
        self.assertEqual(k.escape(np.array([1],dtype='datetime64[3D]')[0]),"'1970-01-02 00:00:00'")
        self.assertEqual(k.escape(np.array([1],dtype='timedelta64[3D]')[0]),"'24:00:00'")
        for v in (np.datetime64('NaT'),np.timedelta64('NaT'),pd.NaT):self.assertEqual(k.escape(v),'NULL')
        self.assertEqual(k.escape(dt.datetime(2024,1,2,3,tzinfo=dt.timezone(dt.timedelta(hours=2)))),"'2024-01-02 03:00:00'")
        self.assertEqual(k.escape(cytimes.Pydt(2024,1,2)),"'2024-01-02 00:00:00'")
        self.assertEqual(k.escape(cytimes.Pddt(['2024-01-02'])),("'2024-01-02 00:00:00'",))
        for value in (np.timedelta64(1,'ps'),np.datetime64(1,'ps')):
            with self.assertRaises(Exception):k.escape(value)
    def test_pandas(self) -> None:
        """Pandas preserves column order and duplicate column names with local date fields."""
        df=pd.DataFrame([[1,'x',dt.datetime(2024,1,2)],[2,'y',pd.NaT]],columns=['x','x','d'])
        self.assertEqual(k.escape(df),[('1',"'x'","'2024-01-02 00:00:00'"),('2',"'y'",'NULL')])
        for data in (pd.Series([1,2]),pd.Index([1,2]),pd.DatetimeIndex(['2024-01-02']),pd.TimedeltaIndex(['1 days'])):
            self.assertEqual(k.escape(data),k.escape(data.values))
        with self.assertRaises(Exception): k.escape(pd.Series([1,pd.NA],dtype='Int64'))
    def test_json_rich(self) -> None:
        """Rich JSON records retain documented semantics and reject implicit SQL/JSON hooks."""
        @dataclasses.dataclass
        class D:
            x:int=1
            _private:str='hidden'
        class E(enum.Enum): v='value'
        v={'d':D(),'e':E.v,'u':uuid.UUID(int=1),'date':dt.date(2024,1,2),'dt':dt.datetime(2024,1,2,tzinfo=dt.timezone.utc),'a':np.arange(8,dtype='f4').reshape(2,2,2)}
        tree=k.loads(k.dumps(v));self.assertEqual(tree['d'],{'x':1});self.assertEqual(tree['a'],[[[0.0,1.0],[2.0,3.0]],[[4.0,5.0],[6.0,7.0]]]);self.assertEqual(tree['dt'],'2024-01-02T00:00:00+00:00')
        for value in (dt.time(1,tzinfo=dt.timezone.utc),b'x',set([1]),pd.Series([1]),np.array(['x']),np.timedelta64(1,'s'),np.datetime64('NaT'),{'x':object()},{1:'x'},2**64,-2**63-1):
            with self.assertRaises(Exception):k.dumps(value)
        self.assertEqual(k.dumps([float('nan'),float('inf'),-0.0]),b'[null,null,-0.0]')
        self.assertEqual(k.dumps(np.empty((2,0))),b'[[],[]]')
        for value in (np.array([1],dtype='datetime64[3D]'),np.array([1],dtype='datetime64[3D]')[0]):
            with self.assertRaises(TypeError):k.dumps(value)
        for a in (np.asfortranarray(np.arange(6).reshape(2,3)),np.arange(6)[::-1],np.arange(6).astype('>i4')):
            with self.assertRaises(TypeError):k.dumps(a)
    def test_grammar(self) -> None:
        """Parser fully consumes numbers and escapes; bounded depth and malformed Unicode reject."""
        invalid=[b'+1',b'1.',b'1e',b'1e+',b'-01',b'--1',b'1e999',b'"\\udc00"',b'"\\ud800\\u0000"',b'"\xc0\x80"',b'"\xed\xa0\x80"',b'"\0"',b'{}[]',b'[[[[']
        for data in invalid:
            with self.assertRaises(Exception):k.loads(data)
        with self.assertRaises(Exception):k.loads(b'['*129+b'0'+b']'*129)
        self.assertEqual(k.loads(b'['*128+b'0'+b']'*128),self._nested(128))
        r=random.Random(42)
        for _ in range(200):
            v=[r.randint(-2**63,2**64-1),r.random(),''.join(chr(r.choice([0,1,8,9,10,13,34,92,65,0x2603,0x1f600])) for _ in range(12))]
            self.assertEqual(k.loads(k.dumps(v)),v)
    @staticmethod
    def _nested(depth: int) -> object:
        """Build an independent expected tree for the exact depth boundary."""
        value: object = 0
        for _ in range(depth): value = [value]
        return value
    def test_errors_hooks(self) -> None:
        """Trusted hooks preserve overrides and causes while ordinary subtypes stay quoted."""
        class Trusted:
            def __str__(self) -> str:return 'NOW()'
        class SubStr(str):
            def __str__(self) -> str:return 'unsafe'
        class Bad(Trusted):
            def __str__(self) -> str:raise ValueError('hook failure')
        k.configure_types(objstr=(Trusted,),rawtext=(RawText,),sqlfunction=(SQLFunction,),sqlinterval=(SQLInterval,))
        self.assertEqual(k.escape(Trusted()),'NOW()');self.assertEqual(k.escape(SubStr('payload')),"'unsafe'")
        self.assertEqual(k.escape(RawText('NOW()')),'NOW()')
        self.assertEqual(k.escape(SQLFunction('F',2,1,"x'")),"F(1,'x\\'')")
        self.assertEqual(k.escape(SQLInterval('DAY', 1)), 'INTERVAL 1 DAY')
        with self.assertRaises(errors.EscapeValueError)as cm:k.escape(Bad())
        self.assertIsInstance(cm.exception.__cause__,ValueError)
        with self.assertRaises(errors.DecodeValueError):k.decode(b'-',8,b'utf8',False)
        k.configure_types()
    def test_dispatch_candidates(self) -> None:
        """All candidate classifiers feed identical handlers for exact and subtype values."""
        self.assertEqual(k._dispatch_strategies(), ('cascade', 'type_id', 'hot_registry'))
        with self.assertRaises(ValueError): k._set_dispatch_strategy('function_table')
        values=[None,True,1,-1,1.25,"a'",b'\xff',[1,2],np.int32(2),np.float32(1.2),dt.date(2024,1,2)]
        expected=[k.escape(v) for v in values]
        for strategy in k._dispatch_strategies():
            k._set_dispatch_strategy(strategy)
            self.assertEqual([k.escape(v) for v in values],expected)
        k._set_dispatch_strategy('hot_registry')
    def test_subclass_boundaries(self) -> None:
        """Exact JSON families and base-storage reads match the fixed serializer boundary."""
        class Date(dt.date): pass
        class Float(float): pass
        from decimal import Decimal
        class DecimalSub(Decimal): pass
        class DataFrameSub(pd.DataFrame): pass
        class ArraySub(np.ndarray): pass
        class Tuple(tuple): pass
        class List(list):
            def __iter__(self): raise RuntimeError('must not invoke list iterator override')
        class Encoded(str):
            def encode(self, *args, **kwargs): raise RuntimeError('must not invoke encode override')
        for value in (Date(2024,1,2),Float(1.2),Tuple((1,2)),pd.Timestamp('2024-01-02'),cytimes.Pydt(2024,1,2)):
            with self.assertRaises(TypeError):k.dumps(value)
        for value in (DecimalSub('1'),DataFrameSub([[1]]),np.array([1]).view(ArraySub),np.void(b'x')):
            with self.assertRaises(errors.EscapeTypeError):k.escape(value)
        self.assertEqual(k.dumps(List([1,2])),b'[1,2]')
        with self.assertRaises(errors.EscapeError):k.escape(List([1,2]))
        class SQLList(list):
            def __iter__(self): return iter([7])
        class SQLTuple(tuple):
            def __iter__(self): return iter([7])
        self.assertEqual(k.escape(SQLList([1])),('7',))
        self.assertEqual(k.escape(SQLTuple([1])),('7',))
        self.assertEqual(k.escape(Encoded("a'")),"'a\\''")
        self.assertEqual(k.escape(np.str_('\0')),"'\\0'")
        self.assertEqual(k.escape(np.rec.array([(1,2)],dtype=[('a','i4'),('b','i4')])[0]),('1','2'))
    def test_scalar_conversion_hooks(self) -> None:
        """Normalize numeric subclass conversion hooks and reject a spoofed missing-date name."""
        class Int(int):
            def __int__(self) -> int: return 5
            def __str__(self) -> str: return 'SQL_INJECTION'
        class Float(float):
            def __float__(self) -> float: return 2.5
            def __str__(self) -> str: return 'SQL_INJECTION'
        class NaTType: pass
        class dict_items:
            def __iter__(self): return iter([('x',1)])
        self.assertEqual(k.escape(Int(1)),'5')
        self.assertEqual(k.escape(Float(1)),'2.5')
        with self.assertRaises(errors.EscapeTypeError):k.escape(NaTType())
        with self.assertRaises(errors.EscapeTypeError):k.escape(dict_items())
        with self.assertRaises(errors.DecodeTypeError) as cm:k.decode(b'x',999,b'invalid-codec-name',False)
        self.assertIsInstance(cm.exception.__cause__,LookupError)
    def test_decimal_range(self) -> None:
        """Exact Decimal decoding validates syntax without narrowing to binary float range."""
        from decimal import Decimal
        self.assertEqual(k.decode(b'1e1000',246,b'utf8',False,True),Decimal('1e1000'))
        for raw,expected in ((b'',0),(b'\xff'*8,2**64-1)):
            self.assertEqual(k.decode(raw,16,b'utf8',False,False,True),expected)
            self.assertEqual(k.decode(raw,16,b'utf8',False),raw)
        self.assertEqual(k.decode(b'\xff'*9,16,b'utf8',False),b'\xff'*9)
        with self.assertRaises(errors.DecodeValueError): k.decode(b'\xff'*9,16,b'utf8',False,False,True)
        for field in (17,18,19):self.assertEqual(k.decode(b'2024-01-02',field,b'utf8',False),'2024-01-02')
if __name__=='__main__':unittest.main()
