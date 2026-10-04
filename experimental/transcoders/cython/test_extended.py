"""Bounded domain, hook reentrancy and malformed grammar coverage beyond basic smoke."""
import unittest
import dataclasses
import datetime as dt
import numpy as np
import sqlaero_cython_kernel as k

class Extended(unittest.TestCase):
    """Test strict budgets and complete temporal/unit/flag domains without database access."""
    def test_datetime_units(self) -> None:
        """Check epoch floors, large safe ticks, month/year calendar units and NaT."""
        for unit in ('ns','us','ms','s','m','h','D','W','M','Y'):
            self.assertEqual(k.escape(np.datetime64(0,unit)),"'1970-01-01 00:00:00'")
            self.assertEqual(k.escape(np.datetime64('NaT',unit)),'NULL')
            self.assertEqual(k.escape(np.timedelta64('NaT',unit)),'NULL')
        self.assertEqual(k.escape(np.datetime64(-1,'ns')),"'1969-12-31 23:59:59.999999'")
        self.assertEqual(k.escape(np.timedelta64(-1,'ns')),"'-00:00:00.000001'")
        self.assertEqual(k.escape(np.timedelta64(1,'M')),"'730:29:06'")
        self.assertEqual(k.escape(np.timedelta64(1,'Y')),"'8765:49:12'")
        self.assertEqual(k.escape(np.datetime64(1,'3D')),"'1970-01-02 00:00:00'")
        self.assertEqual(k.escape(np.timedelta64(1,'3D')),"'24:00:00'")
        with self.assertRaises(k.EscapeValueError): k.escape(np.datetime64((1<<63)-1,'D'))
    def test_decode_flags(self) -> None:
        """Check all eight flag combinations for representative field categories."""
        for dec in (False,True):
            for bit in (False,True):
                for js in (False,True):
                    self.assertEqual(k.decode(b'12',3,b'utf8',False,dec,bit,js),12)
                    self.assertEqual(k.decode(b'\x02',16,b'utf8',True,dec,bit,js),2 if bit else b'\x02')
                    self.assertEqual(k.decode(b'{"x":1}',245,b'utf8',True,dec,bit,js),{'x':1} if js else '{"x":1}')
        with self.assertRaises(k.DecodeValueError): k.decode(b'123456789',16,b'utf8',True,decode_bit=True)
        self.assertEqual(k.decode(b'1-1-1T1:1:1.1',12,b'utf8',False),dt.datetime(1,1,1,1,1,1,100000))
    def test_json_domains(self) -> None:
        """Cover native float widths, private dataclass fields, cycles and forbidden special types."""
        self.assertEqual(k.dumps(np.float32(1.1)),b'1.1')
        self.assertEqual(k.dumps(np.float16(1.1)),b'1.0996094')
        self.assertEqual(k.dumps(np.array([1.1],dtype='f2')),b'[1.0996094]')
        self.assertEqual(k.dumps(1e-7),b'1e-7')
        self.assertIs(type(k.loads('18446744073709551616')),float)
        @dataclasses.dataclass
        class Node:
            """Dataclass graph fixture with skipped private field."""
            x:object
            _hidden:int=2
        self.assertEqual(k.dumps(Node(1)),b'{"x":1}')
        n=Node(None);n.x=n
        with self.assertRaises(ValueError): k.dumps(n)
        for value in (np.timedelta64(1,'D'),dt.time(1,tzinfo=dt.timezone.utc),np.datetime64(1,'3D')):
            with self.assertRaises(TypeError): k.dumps(value)
        for text in ('['*130+'0'+']'*130,'1e999999999999999999999','"\\u12xz"','{"x" 1}','[1 2]'):
            with self.assertRaises(ValueError): k.loads(text)
    def test_depth_and_exact_json(self) -> None:
        """Allow depth128, reject129 and reject frozen unsupported JSON subclass families."""
        self.assertIsInstance(k.loads('['*128+'0'+']'*128),list)
        with self.assertRaises(ValueError): k.loads('['*129+'0'+']'*129)
        class F(float):
            """Unsupported JSON float subclass fixture."""
        class DT(dt.datetime):
            """Unsupported JSON calendar subclass fixture."""
        class A(np.ndarray):
            """Unsupported JSON ndarray subclass fixture."""
        for value in (F(1),DT(2020,1,1),np.arange(2).view(A)):
            with self.assertRaises(TypeError): k.dumps(value)
    def test_output_budget(self) -> None:
        """Exercise pre-accumulation UTF-8 budgets cheaply with a temporary small limit."""
        saved=k.MAX_JSON_BYTES
        k.MAX_JSON_BYTES=64
        try:
            self.assertEqual(k.dumps('x'*62),b'"'+b'x'*62+b'"')
            for value in ('x'*63,'😀'*16,['x'*30,'y'*30],{'x':'z'*58}):
                with self.assertRaises(ValueError): k.dumps(value)
            for value in (b' '*65,'😀'*17):
                with self.assertRaises(ValueError): k.loads(value)
        finally: k.MAX_JSON_BYTES=saved
    def test_record_and_subclasses(self) -> None:
        """Preserve record scalar item shape and frozen tuple-subclass JSON rejection."""
        record=np.array([(1,2.5,'x')],dtype=[('a','i4'),('b','f8'),('c','U1')]).view(np.recarray)[0]
        self.assertEqual(k.escape(record),('1','2.5',"'x'"))
        self.assertEqual(k.escape(record,False,False),"(1,2.5,'x')")
        class T(tuple):
            """Unsupported JSON tuple-subclass fixture."""
        with self.assertRaises(TypeError): k.dumps(T((1,2)))
        class L(list):
            """List subclass with an override JSON must ignore."""
            def __iter__(self) -> object:
                """Produce an alternate sequence for ordinary Python iteration."""
                return iter([99])
        class D(dict):
            """Dict subclass with an override JSON must ignore."""
            def items(self) -> object:
                """Produce alternate pairs for ordinary Python mapping calls."""
                return [('bad',99)]
        self.assertEqual(k.dumps(L([1,2])),b'[1,2]')
        self.assertEqual(k.dumps(D(x=1)),b'{"x":1}')
        class I(int):
            """Integer subclass with conversion and comparison overrides JSON must ignore."""
            def __int__(self) -> int:
                """Return a distinct value for SQL subclass conversion."""
                return 7
            def __lt__(self, other: object) -> bool:
                """Deliberately misleading overridden comparison."""
                return True
        self.assertEqual(k.dumps(I(3)),b'3')
        self.assertEqual(k.escape(I(3)),'7')


    def test_hook_assertion(self) -> None:
        """Preserve the original trusted-hook AssertionError propagation contract."""
        class H:
            """Trusted SQL callback fixture failing an application assertion."""
            def __str__(self) -> str:
                """Signal the application assertion without conversion wrapping."""
                raise AssertionError('application invariant')
        k.configure_types(objstr=(H,))
        try:
            with self.assertRaises(AssertionError): k.escape(H())
        finally: k.configure_types()
    def test_callbacks(self) -> None:
        """Keep snapshots stable under controlled reentrant hook/container mutation."""
        source=[]
        class Hook:
            """Trusted callback mutating its surrounding sequence."""
            def __str__(self) -> str:
                """Reenter scalar serialization and change the source after its snapshot."""
                source.clear()
                return k.escape(2)
        source.extend([Hook(),1]);k.configure_types(objstr=(Hook,))
        self.assertEqual(k.escape(source),('2','1'))
        k.configure_types()

if __name__=='__main__': unittest.main()
