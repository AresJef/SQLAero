# cython: language_level=3
"""Experimental public classes and Cython call adapter for independent kernels.

This module retains a Cython interface for newly compiled consumers. It does not
promise compatibility with previously compiled sqlaero.transcode consumers.
"""
import importlib
import operator
import numpy as np

_BIT_INTEGER_TYPES = (np.int8, np.int16, np.int32, np.int64, np.uint8, np.uint16, np.uint32, np.uint64)

_backend: object = None


def configure_backend(name: str) -> None:
    """Select and register one complete experimental kernel before using classes."""
    global _backend
    if name not in ("sqlaero_cython_kernel", "sqlaero_rust_kernel"):
        raise ValueError("Unknown experimental backend")
    candidate = importlib.import_module(name)
    from sqlaero.sqlfunc import RawText, SQLFunction
    from sqlaero.sqlintvl import SQLInterval
    candidate.configure_types(objstr=(ObjStr,), rawtext=(RawText,),
                              sqlfunction=(SQLFunction,), sqlinterval=(SQLInterval,))
    _backend = candidate


def _require_backend() -> object:
    """Return the selected kernel or fail before any conversion starts."""
    if _backend is None:
        raise RuntimeError("Call configure_backend before using experimental classes")
    return _backend


cdef class ObjStr:
    """Trusted SQL expression whose subclasses implement their SQL text hook."""
    def __str__(self) -> str:
        """Require subclasses to supply already trusted SQL text."""
        raise NotImplementedError("ObjStr subclass must implement __str__")


cdef class CustomEscapeType(ObjStr):
    """Trusted SQL hook carrying an underlying Python object."""
    def __init__(self, obj: object) -> None:
        """Store the payload without implicit conversion."""
        self._obj = obj

    @property
    def obj(self) -> object:
        """Return the read-only payload reference."""
        return self._obj

    def __repr__(self) -> str:
        """Represent the public type name and payload."""
        return "<'%s' (obj=%r)>" % (self.__class__.__name__, self._obj)

    def __str__(self) -> str:
        """Require subclasses to implement their trusted SQL text hook."""
        raise NotImplementedError("CustomEscapeType subclass must implement __str__")


cdef class BIT(CustomEscapeType):
    """MySQL BIT value represented by a checked unsigned 64-bit integer."""
    def __str__(self) -> str:
        """Encode a BIT payload, rejecting truncation and invalid integer ranges."""
        data = self._obj
        if isinstance(data, (bytes, bytearray, memoryview)):
            raw = bytes(data)
            if len(raw) > 8:
                raise ValueError("BIT byte payload exceeds 64 bits")
            value = int.from_bytes(raw, "big")
        elif type(data) is bool:
            return str(data)
        elif isinstance(data, int) or type(data) in _BIT_INTEGER_TYPES:
            try:
                value = operator.index(data)
            except TypeError as exc:
                raise TypeError("BIT requires bytes or an integer") from exc
        else:
            raise TypeError("BIT requires bytes or a supported integer type")
        if value < 0 or value > 18446744073709551615:
            raise ValueError("BIT integer is outside unsigned 64-bit range")
        return str(value)


cdef class JSON(CustomEscapeType):
    """Python value serialized by the selected self-implemented JSON kernel."""
    def __str__(self) -> str:
        """Serialize JSON first, then apply the separate SQL string escaping layer."""
        backend = _require_backend()
        try:
            raw = backend.dumps(self._obj)
        except Exception as exc:
            raise ValueError("Cannot serialize JSON payload") from exc
        if not isinstance(raw, bytes):
            raise TypeError("Kernel dumps must return bytes")
        # Preserve the original bytes-to-SQL surrogateescape transport contract.
        return backend.escape(raw.decode("ascii", "surrogateescape"), False, False)


cpdef object escape(object data, bint many=False, bint itemize=True):
    """Escape through the selected full kernel, preserving output shape flags."""
    return _require_backend().escape(data, many, itemize)


cpdef object decode(bytes value, unsigned int field_type, const char* encoding,
                    bint is_binary, bint use_decimal=False, bint decode_bit=False,
                    bint decode_json=False):
    """Decode a text-protocol field through the selected full kernel."""
    if encoding == NULL:
        raise ValueError("encoding must not be NULL")
    return _require_backend().decode(value, field_type, <bytes>encoding, is_binary,
                                     use_decimal, decode_bit, decode_json)
