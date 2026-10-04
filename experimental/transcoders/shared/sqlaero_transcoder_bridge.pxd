# cython: language_level=3
cdef class ObjStr:
    pass
cdef class CustomEscapeType(ObjStr):
    cdef object _obj
cdef class BIT(CustomEscapeType):
    pass
cdef class JSON(CustomEscapeType):
    pass
cpdef object escape(object data, bint many=?, bint itemize=?)
cpdef object decode(bytes value, unsigned int field_type, const char* encoding, bint is_binary, bint use_decimal=?, bint decode_bit=?, bint decode_json=?)
