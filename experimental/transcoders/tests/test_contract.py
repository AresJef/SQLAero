"""Shared small correctness and safety vectors for independent full kernels.

Run with SQLAERO_KERNEL=module python -m unittest discover -s tests -v.
This suite performs no database operations and no timing/benchmark loops.
"""
import datetime as dt
import importlib
import os
import unittest
from decimal import Decimal


class KernelContract(unittest.TestCase):
    """Check scalar, output shape, JSON grammar and early safety requirements."""
    @classmethod
    def setUpClass(cls) -> None:
        """Load exactly the experimental backend requested by the runner."""
        cls.kernel = importlib.import_module(os.environ["SQLAERO_KERNEL"])

    def test_scalar_literals(self) -> None:
        """Preserve normal scalar SQL literals without external serialization."""
        values = [(None,"NULL"),(True,"1"),(False,"0"),(0,"0"),(-123,"-123"),
                  (Decimal("1.2300"),"1.2300"),("a'b\\c\0\n", "'a\\'b\\\\c\\0\\n'"),
                  (b"a'b", "_binary'a\\'b'"),
                  (dt.date(2024,2,29),"'2024-02-29'"),
                  (dt.datetime(2024,2,29,1,2,3,4),"'2024-02-29 01:02:03.000004'")]
        for value, expected in values:
            for many in (False,True):
                for itemize in (False,True):
                    with self.subTest(value=value,many=many,itemize=itemize):
                        self.assertEqual(self.kernel.escape(value,many,itemize),expected)

    def test_shapes(self) -> None:
        """Preserve the many flag precedence and nested row return types."""
        self.assertEqual(self.kernel.escape([1,2],False,False),"(1,2)")
        self.assertEqual(self.kernel.escape([1,2],False,True),("1","2"))
        self.assertEqual(self.kernel.escape([1,2],True,False),["1","2"])
        self.assertEqual(self.kernel.escape([[1,2],[3,4]],True,True),[("1","2"),("3","4")])
        self.assertEqual(self.kernel.escape([],False,False),"()")
        self.assertEqual(self.kernel.escape([],False,True),())
        self.assertEqual(self.kernel.escape([],True,False),[])

    def test_decode_valid_fields(self) -> None:
        """Decode text-protocol values and preserve return types and options."""
        k = self.kernel
        self.assertEqual(k.decode(b"18446744073709551615",8,b"utf8",False),18446744073709551615)
        self.assertEqual(k.decode(b"-9223372036854775808",8,b"utf8",False),-9223372036854775808)
        self.assertEqual(k.decode(b"1.2300",246,b"utf8",False,True),Decimal("1.2300"))
        self.assertEqual(k.decode(b"abc",252,b"utf8",True),b"abc")
        self.assertEqual(k.decode(b"a,b",248,b"utf8",True),"a,b")
        self.assertEqual(k.decode(b"0000-00-00",10,b"utf8",False),"0000-00-00")
        self.assertEqual(k.decode(b"\x01\x02",16,b"utf8",False,False,True),258)
        self.assertEqual(k.decode(b'{"x":1}',245,b"utf8",False,False,False,True),{"x":1})

    def test_checked_decode_changes(self) -> None:
        """Reject overflow and malformed numeric tokens rather than preserve defects."""
        for value in (b"-", b"18446744073709551616",b"-9223372036854775809"):
            with self.subTest(value=value),self.assertRaises(Exception):
                self.kernel.decode(value,8,b"utf8",False)
        for value in (b".",b"-",b"1.2.3"):
            with self.subTest(value=value),self.assertRaises(Exception):
                self.kernel.decode(value,5,b"utf8",False)
        self.assertEqual(self.kernel.decode(b"1.25e2",5,b"utf8",False),125.0)

    def test_json_own_grammar(self) -> None:
        """Check encoding and parsing independent of a third-party JSON oracle."""
        value = {"text":"a\n雪😀", "list":[None,True,False,-1,1.25]}
        encoded = self.kernel.dumps(value)
        self.assertIsInstance(encoded,bytes)
        self.assertEqual(self.kernel.loads(encoded),value)
        self.assertEqual(self.kernel.loads(b'"\\ud83d\\ude00"'),"😀")
        self.assertEqual(self.kernel.loads(b'{"x":1,"x":2}'),{"x":2})
        for invalid in (b"",b"[1,]",b'{"x":}',b"01",b"NaN",b'"\\ud800"',b'"\xff"',b"null x"):
            with self.subTest(value=invalid),self.assertRaises(Exception):
                self.kernel.loads(invalid)

    def test_cycle_rejection(self) -> None:
        """Reject cyclic SQL and JSON containers under a bounded recursion policy."""
        value:list[object] = []
        value.append(value)
        with self.assertRaises(Exception):
            self.kernel.dumps(value)
        with self.assertRaises(Exception):
            self.kernel.escape(value)


if __name__ == "__main__":
    unittest.main()
