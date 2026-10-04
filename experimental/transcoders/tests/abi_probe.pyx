# cython: language_level=3
"""Newly compiled consumer probe; no claim about old binary compatibility."""
from sqlaero_transcoder_bridge cimport CustomEscapeType, escape, decode

def payload(CustomEscapeType item) -> object:
    """Read the documented Cython payload field in a newly compiled consumer."""
    return item._obj

def escape_through_capi(object data) -> object:
    """Exercise the bridge's exported cpdef escape capsule."""
    return escape(data, False, True)

def decode_through_capi(bytes data) -> object:
    """Decode an integer using the exported typed Cython decode capsule."""
    cdef bytes encoding = b'utf8'
    return decode(data, 3, encoding, False, False, False, False)
