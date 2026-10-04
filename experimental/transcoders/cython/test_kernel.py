"""Bounded independent safety and supported-domain regression tests."""
import unittest
import datetime as dt
import decimal
import dataclasses
import enum
import uuid
import numpy as np
import pandas as pd
import sqlaero_cython_kernel as k

class Tests(unittest.TestCase):
    """Exercise owned algorithms, recursive graphs and unusual array layouts."""
    def test_scalars(self):
        """Check escape tables, precise decimals, negative durations and zero dates."""
        self.assertEqual(k.escape('\0\n\r\x1a\\\'"'),"'\\0\\n\\r\\Z\\\\\\'\\\"'")
        self.assertEqual(k.escape(decimal.Decimal('1.2300')),'1.2300')
        self.assertEqual(k.escape(dt.timedelta(microseconds=-1)),"'-00:00:00.000001'")
        self.assertEqual(k.decode(b'0000-00-00',10,b'utf8',False),'0000-00-00')
        self.assertEqual(k.decode(b'1e3',5,b'utf8',False),1000.)
    def test_numbers(self):
        """Reject partial input and overflowing integer/float decode."""
        for text in (b'-',b'1x',b'18446744073709551616',b'-9223372036854775809'):
            with self.assertRaises(k.DecodeValueError): k.decode(text,8,b'utf8',False)
        for text in (b'.',b'1..2',b'NaN',b'1e9999'):
            with self.assertRaises(k.DecodeValueError): k.decode(text,5,b'utf8',False)
    def test_layouts(self):
        """Index all storage layouts safely with explicit rows and dtype checks."""
        base=np.arange(12).reshape(3,4)
        for array in (base,base.T,base[:,::-1],base[::2,::2],np.asfortranarray(base),base.astype('>i8'),np.broadcast_to(np.array([1,2]),(3,2))):
            self.assertEqual(k.escape(array),[tuple(str(int(v)) for v in row) for row in array])
        raw=bytearray(25); unaligned=np.ndarray((3,),dtype='i8',buffer=raw,offset=1)
        self.assertEqual(k.escape(unaligned),('0','0','0'))
        for array in (np.array(1),np.zeros((1,1,1))):
            with self.assertRaises(k.EscapeValueError): k.escape(array)
        with self.assertRaises(k.EscapeTypeError): k.escape(np.array([1j]))
        self.assertEqual(k.escape(np.array(['NaT'],dtype='timedelta64[ns]')),('NULL',))
    def test_shapes(self):
        """Preserve scalar, flat, row, empty and dataframe return types."""
        self.assertEqual(k.escape([1,2],False,False),'(1,2)')
        self.assertEqual(k.escape([[1,2],[3,4]],False,True),('(1,2)','(3,4)'))
        self.assertEqual(k.escape([[1,2],[3,4]],True,False),[('1','2'),('3','4')])
        self.assertEqual(k.escape(pd.DataFrame({'a':[1,2],'b':['x','y']})),[('1',"'x'"),('2',"'y'")])
        self.assertEqual(k.escape(np.empty((0,2),dtype='i8')),[])
    def test_json(self):
        """Check escapes, nested grammar, Unicode and representative special domains."""
        tree={'x':['😀','\0\n',True,None,1,-0.0],'y':{}}
        self.assertEqual(k.loads(k.dumps(tree)),tree)
        self.assertEqual(k.loads('"\\ud83d\\ude00"'),'😀')
        self.assertEqual(k.loads('{"x":1,"x":2}'),{'x':2})
        for bad in ('[1,]','{"x":}','01','true false','"\\ud800"','"\\udc00"','"\n"','NaN','1e9999'):
            with self.assertRaises(ValueError): k.loads(bad)
        with self.assertRaises(UnicodeDecodeError): k.loads(b'"\xff"')
        @dataclasses.dataclass
        class Record:
            """Minimal dataclass JSON fixture."""
            x:int
        self.assertEqual(k.dumps(Record(2)),b'{"x":2}')
        self.assertEqual(k.loads(k.dumps(uuid.UUID(int=0))),str(uuid.UUID(int=0)))
        self.assertEqual(k.loads(k.dumps(np.arange(6).reshape(2,3))),[[0,1,2],[3,4,5]])
    def test_cycles(self):
        """Reject cycles in SQL, JSON and object arrays without recursion crashes."""
        a=[]; a.append(a)
        with self.assertRaises(k.EscapeValueError): k.escape(a)
        with self.assertRaises(ValueError): k.dumps(a)
        a=np.empty(1,dtype=object); a[0]=a
        with self.assertRaises(k.EscapeValueError): k.escape(a)
    def test_hooks(self):
        """Registered hooks keep subclass overrides and ordinary string subclasses are quoted."""
        class Trusted:
            """Controlled SQL hook fixture."""
            def __str__(self):
                """Return deliberately trusted SQL text."""
                return 'NOW()'
        class Plain(str):
            """Ordinary untrusted text subclass."""
        k.configure_types(objstr=(Trusted,))
        self.assertEqual(k.escape(Trusted()),'NOW()')
        self.assertEqual(k.escape(Plain('NOW()')),"'NOW()'")
        k.configure_types()

if __name__=='__main__': unittest.main()
