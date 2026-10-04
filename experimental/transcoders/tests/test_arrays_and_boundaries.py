"""Safety-oriented layout, BIT and depth checks without invoking unsafe old paths."""
import importlib
import os
import unittest
import numpy as np
from sqlaero import errors

class LayoutAndBoundaryContract(unittest.TestCase):
    """Validate checked layout handling and shared resource policies."""
    @classmethod
    def setUpClass(cls) -> None:
        """Load the explicit kernel selected by the coordinator runner."""
        cls.kernel = importlib.import_module(os.environ['SQLAERO_KERNEL'])

    def test_strided_layouts(self) -> None:
        """Compare safe exotic layouts to equivalent owned C-order values."""
        for dtype in ['i8','u4','f4','f8','bool']:
            base=np.arange(12).astype(dtype).reshape(3,4)
            layouts=[base,base.T,base[:,::2],base[::-1,::-1],np.asfortranarray(base),
                     np.broadcast_to(base[:1,:],(3,4)),base.astype(np.dtype(dtype).newbyteorder('>'))]
            size=base.size*base.dtype.itemsize
            buffer=bytearray(size+1)
            unaligned=np.ndarray(base.shape,dtype=base.dtype,buffer=buffer,offset=1)
            unaligned[:]=base
            layouts.append(unaligned)
            for value in layouts:
                value.flags.writeable=False
                expected=np.array(value,order='C',copy=True)
                for many in (False,True):
                    for itemize in (False,True):
                        with self.subTest(dtype=dtype,strides=value.strides,many=many,itemize=itemize):
                            self.assertEqual(self.kernel.escape(value,many,itemize),self.kernel.escape(expected,many,itemize))

    def test_bit_decode_boundary(self) -> None:
        """Preserve bytes unless decoding requested; reject values wider than 64 bits."""
        for value in [b'',b'\xff'*8,b'\xff'*9]:
            self.assertEqual(self.kernel.decode(value,16,b'utf8',False,False,False),value)
            if len(value)<=8:
                self.assertEqual(self.kernel.decode(value,16,b'utf8',False,False,True),int.from_bytes(value,'big'))
            else:
                with self.assertRaises(errors.DecodeValueError):
                    self.kernel.decode(value,16,b'utf8',False,False,True)

    def test_json_depth_boundary(self) -> None:
        """Check the same provisional maximum nesting definition in both kernels."""
        allowed=b'['*128+b'0'+b']'*128
        self.kernel.loads(allowed)
        with self.assertRaises(Exception): self.kernel.loads(b'['*129+b'0'+b']'*129)

    def test_unsupported_array_shapes(self) -> None:
        """Reject unsupported dimensions and complex dtypes through public errors."""
        for value in [np.array(1),np.zeros((1,1,1)),np.array([1+2j])]:
            expected=errors.EscapeValueError if value.ndim not in (1,2) else errors.EscapeTypeError
            with self.subTest(shape=value.shape,dtype=value.dtype),self.assertRaises(expected):
                self.kernel.escape(value)

if __name__=='__main__': unittest.main()
