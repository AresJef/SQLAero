"""Verify public class identity, hook dispatch and error semantics for each kernel."""
import os
import unittest
import sqlaero_transcoder_bridge as bridge
from sqlaero import errors
from sqlaero.sqlfunc import RawText, SQLFunction
from sqlaero.sqlintvl import SQLInterval

class BridgeContract(unittest.TestCase):
    """Exercise the stable shared Cython class layer without database access."""
    @classmethod
    def setUpClass(cls) -> None:
        """Configure the kernel explicitly selected for this independent process."""
        bridge.configure_backend(os.environ['SQLAERO_KERNEL'])

    def test_inheritance(self) -> None:
        """Preserve the public extension-type inheritance and read-only payload."""
        self.assertTrue(issubclass(bridge.CustomEscapeType,bridge.ObjStr))
        self.assertTrue(issubclass(bridge.BIT,bridge.CustomEscapeType))
        self.assertTrue(issubclass(bridge.JSON,bridge.CustomEscapeType))
        payload:dict[str,int] = {'x':1}
        obj = bridge.JSON(payload)
        self.assertIs(obj.obj,payload)
        with self.assertRaises(AttributeError): obj.obj = 2

    def test_hooks(self) -> None:
        """Honor custom and overridden JSON hooks while escaping ordinary subclasses."""
        class Hook(bridge.CustomEscapeType):
            """Trusted expression for the hook dispatch test."""
            def __str__(self) -> str:
                """Return the declared trusted SQL expression."""
                return 'NOW()'
        class Override(bridge.JSON):
            """JSON subclass explicitly overriding its trusted SQL hook."""
            def __str__(self) -> str:
                """Return the override instead of invoking the JSON payload codec."""
                return 'OVERRIDE()'
        class Text(str):
            """Ordinary string subclass retaining the untrusted text domain."""
        self.assertEqual(bridge.escape(Hook('x')),'NOW()')
        self.assertEqual(bridge.escape(Override({})),'OVERRIDE()')
        self.assertEqual(bridge.escape(Text("a'b")),"'a\\'b'")

    def test_json_layers(self) -> None:
        """Keep JSON serialization separate from SQL quote escaping and byte transport."""
        self.assertEqual(bridge.escape(bridge.JSON({'x':1})),"'{\\\"x\\\":1}'")
        self.assertEqual(bridge.escape(bridge.JSON('{}')),"'\\\"{}\\\"'")
        encoded = bridge.escape(bridge.JSON({'x':'雪'}))
        self.assertIn('雪'.encode().decode('ascii','surrogateescape'),encoded)

    def test_bit_domain(self) -> None:
        """Preserve valid BIT values and reject previously truncated or invalid domains."""
        self.assertEqual(bridge.escape(bridge.BIT(b'\x01\x02')),'258')
        self.assertEqual(bridge.escape(bridge.BIT(True)),'True')
        self.assertEqual(bridge.escape(bridge.BIT(False)),'False')
        self.assertEqual(bridge.escape(bridge.BIT(2**64-1)),str(2**64-1))
        for value in (-1,2**64,b'123456789'):
            with self.subTest(value=value),self.assertRaises(errors.EscapeValueError):
                bridge.escape(bridge.BIT(value))
        with self.assertRaises(errors.EscapeTypeError): bridge.escape(bridge.BIT('abc'))

    def test_expressions_and_decode(self) -> None:
        """Use actual SQL expression properties and public typed decode entry points."""
        self.assertEqual(bridge.escape(RawText('NOW()')),'NOW()')
        self.assertEqual(bridge.escape(SQLFunction('F',2,1,'x')),"F(1,'x')")
        self.assertEqual(bridge.escape(SQLInterval('DAY',1)),'INTERVAL 1 DAY')
        self.assertEqual(bridge.decode(b'42',3,b'utf8',False),42)
        with self.assertRaises(errors.DecodeValueError): bridge.decode(b'-',3,b'utf8',False)

if __name__=='__main__': unittest.main()
