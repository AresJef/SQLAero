# cython: language_level=3, boundscheck=True, wraparound=True
"""Independent experimental SQL literal writer and strict JSON codec.

Object-level ndarray indexing intentionally respects strides, endian and alignment;
no raw pointers or borrowed buffers survive Python callbacks.
"""
from cpython.unicode cimport PyUnicode_GET_LENGTH, PyUnicode_READ_CHAR
from cpython.object cimport PyObject, Py_TYPE
from cpython.dict cimport PyDict_GetItemWithError
from cpython.long cimport PyLong_AsLong
import math
import re
import datetime as dt
import time
import decimal
import enum
import uuid
import dataclasses
try:
    import numpy as np
except ImportError:
    np = None
try:
    import pandas as pd
except ImportError:
    pd = None
try:
    from cytimes import Pddt
except ImportError:
    Pddt = None
_index_types = () if pd is None else (pd.Series,pd.Index,pd.DatetimeIndex,pd.TimedeltaIndex)
if Pddt is not None: _index_types += (Pddt,)


class EscapeError(Exception):
    """Base failure while converting an object to SQL."""
class EscapeTypeError(EscapeError, TypeError):
    """Unsupported SQL input type."""
class EscapeValueError(EscapeError, ValueError):
    """Invalid value or cyclic SQL input."""
class DecodeTypeError(ValueError):
    """Malformed or overflowing field representation."""

class DecodeValueError(DecodeTypeError):
    """Standalone malformed field failure when the public package is unavailable."""
try:
    from sqlaero.errors import EscapeError, EscapeTypeError, EscapeValueError, DecodeTypeError, DecodeValueError
except ImportError:
    pass

_JSON_NUMBER = re.compile(r'-?(?:0|[1-9][0-9]*)(?:\.[0-9]+)?(?:[eE][+-]?[0-9]+)?')
MAX_JSON_BYTES = 16 * 1024 * 1024

_trusted = dict(objstr=(), rawtext=(), sqlfunction=(), sqlinterval=())

def configure_types(*, objstr: tuple=(), rawtext: tuple=(), sqlfunction: tuple=(), sqlinterval: tuple=()) -> None:
    """Register explicit application-controlled SQL classes; arbitrary hooks stay untrusted."""
    global _trusted
    values = dict(objstr=objstr, rawtext=rawtext, sqlfunction=sqlfunction, sqlinterval=sqlinterval)
    for value in values.values():
        if not isinstance(value, tuple) or any(not isinstance(x, type) for x in value):
            raise TypeError('trusted registrations must be tuples of classes')
    _trusted = values

cdef int _dispatch_exact(object value) except -1:
    """Exact Python type identity chain; zero delegates to the subclass cold path."""
    cdef object typ = type(value)
    if typ is bool: return 1
    if typ is int: return 2
    if typ is float: return 3
    if typ is str: return 4
    if typ is bytes: return 5
    if typ is bytearray: return 5
    if typ is memoryview: return 5
    if value is None: return 6
    if typ is decimal.Decimal: return 7
    if typ is dt.datetime: return 8
    if typ is dt.date: return 9
    if typ is dt.time: return 10
    if typ is dt.timedelta: return 11
    if typ is time.struct_time: return 12
    return 0

# One internal declaration binds supported exact types to the existing handler IDs.
# Strong type references prevent identity-address reuse; users cannot extend this table.
cdef tuple _exact_type_declarations = (
    (bool,1),(int,2),(float,3),(str,4),(bytes,5),(bytearray,5),(memoryview,5),
    (type(None),6),(decimal.Decimal,7),(dt.datetime,8),(dt.date,9),
    (dt.time,10),(dt.timedelta,11),(time.struct_time,12),
)
cdef dict _type_id_codes = {<size_t><PyObject*>typ: code for typ,code in _exact_type_declarations}
cdef int _dispatch_strategy_id = 0
cdef tuple _strategy_names = ('exactif','hotcold','typeid_dict')

cdef int _dispatch_identity_dict(object value) except -1:
    """Classify by boxed CPython type address, with no metaclass hash/equality callbacks."""
    cdef object identity = <size_t>Py_TYPE(value)
    cdef PyObject* borrowed = PyDict_GetItemWithError(_type_id_codes,identity)
    if borrowed == NULL: return 0
    return <int>PyLong_AsLong(<object>borrowed)

cdef int _dispatch_hotcold(object value) except -1:
    """Keep common exact query scalars in direct branches and use the fixed cold identity registry."""
    cdef object typ = type(value)
    if typ is str: return 4
    if typ is int: return 2
    if typ is float: return 3
    if typ is bool: return 1
    if value is None: return 6
    return _dispatch_identity_dict(value)

cdef int _dispatch_selected(object value) except -1:
    """Select the research classifier while retaining identical downstream handlers and cold policy."""
    if _dispatch_strategy_id == 0: return _dispatch_exact(value)
    if _dispatch_strategy_id == 1: return _dispatch_hotcold(value)
    return _dispatch_identity_dict(value)

def _dispatch_strategies() -> tuple[str, ...]:
    """List internal research candidates; registration and serialization semantics are unchanged."""
    return _strategy_names

def _set_dispatch_strategy(name: str) -> None:
    """Select a process-local research strategy outside timed calls; exactif is the default."""
    global _dispatch_strategy_id
    if not isinstance(name,str) or name not in _strategy_names:
        raise ValueError('unknown internal dispatch strategy')
    _dispatch_strategy_id = _strategy_names.index(name)

def _get_dispatch_strategy() -> str:
    """Report the internal research setting for evidence without changing it."""
    return _strategy_names[_dispatch_strategy_id]

cdef int _dispatch_subclass(object value) except -1:
    """Known historical subclass categories only, ordered bool/int and datetime/date safely."""
    if isinstance(value,bool): return 1
    if isinstance(value,int): return 2
    if isinstance(value,float): return 3
    if isinstance(value,str): return 4
    if isinstance(value,(bytes,bytearray,memoryview)): return 5
    if isinstance(value,dt.datetime): return 8
    if isinstance(value,dt.date): return 9
    if isinstance(value,dt.time): return 10
    if isinstance(value,dt.timedelta): return 11
    return 0

def dispatch_code(value: object) -> int:
    """Expose classification only for future bounded dispatch experiments, without serialization."""
    cdef int code = _dispatch_selected(value)
    return code if code else _dispatch_subclass(value)

cdef str _quote(object value):
    """Quote Unicode with the text protocol's MySQL backslash escape table."""
    cdef list result = ["'"]
    cdef str ch
    cdef int code
    cdef Py_ssize_t i, length = PyUnicode_GET_LENGTH(value)
    for i in range(length):
        code = <unsigned int>PyUnicode_READ_CHAR(value,i)
        ch = chr(code)
        result.append({'\0':'\\0','\\':'\\\\','\n':'\\n','\r':'\\r','\x1a':'\\Z','"':'\\"',"'":"\\'"}.get(ch, ch))
    result.append("'")
    return ''.join(result)

cdef object _numpy_datetime(object value, bint apply_multiplier=True):
    """Convert epoch ticks with arbitrary precision before checked Python calendar construction."""
    unit, multiplier = np.datetime_data(value.dtype)
    ticks = int(value.view('i8')) * (multiplier if apply_multiplier else 1)
    if unit == 'Y': return dt.datetime(1970+ticks,1,1)
    if unit == 'M':
        year, month = divmod(ticks,12)
        return dt.datetime(1970+year,month+1,1)
    factors = dict(ns=1,us=1000,ms=1000000,s=1000000000,m=60000000000,h=3600000000000,D=86400000000000,W=604800000000000)
    if unit not in factors: raise ValueError('unsupported datetime64 unit')
    try: return dt.datetime(1970,1,1) + dt.timedelta(microseconds=ticks*factors[unit]//1000)
    except OverflowError as exc: raise ValueError('datetime64 outside supported calendar') from exc

cdef str _duration(object micros):
    """Format arbitrary-precision signed microseconds without C overflow."""
    sign = '-' if micros < 0 else ''
    micros = abs(micros)
    seconds, fraction = divmod(micros, 1000000)
    hours, seconds = divmod(seconds, 3600)
    minutes, seconds = divmod(seconds, 60)
    return "'%s%02d:%02d:%02d%s'" % (sign,hours,minutes,seconds,'.%06d' % fraction if fraction else '')

cdef object _sequence(object value):
    """Snapshot supported containers before recursively invoking user callbacks."""
    if isinstance(value,time.struct_time): return None
    if np is not None and isinstance(value,np.record): return list(value)
    if isinstance(value, dict): return list(value.values())
    if isinstance(value, type({}.items())): return list(dict(value).values())
    if isinstance(value, (list,tuple,set,frozenset,range,type({}.keys()),type({}.values()))): return list(value)
    return None

cdef str _array_literal(object value, set active, int depth):
    """Format native array float widths independently with float16-to-float32 compatibility."""
    if isinstance(value,np.floating):
        if not np.isfinite(value): return 'NULL'
        return _json_float(str(np.float32(value) if value.dtype.itemsize == 2 else value),max(4,value.dtype.itemsize))
    return _literal(value,active,depth)

cdef str _literal(object value, set active, int depth):
    """Dispatch trusted expressions before ordinary subclasses and write one SQL literal."""
    cdef int kind
    if depth > 128: raise ValueError('maximum SQL nesting depth exceeded')
    if isinstance(value, _trusted['objstr']): return str(value)
    if isinstance(value, _trusted['rawtext']): return value.value if hasattr(value,'value') else value._value
    if isinstance(value, _trusted['sqlfunction']):
        args = value.args if hasattr(value,'args') else value._args
        if not args: return value.syntax()
        return value.syntax() % tuple(_literal(x,active,depth+1) for x in args)
    if isinstance(value, _trusted['sqlinterval']):
        expr = value.expr if hasattr(value,'expr') else value._expr
        return value.syntax() % _literal(expr,active,depth+1)
    kind = _dispatch_selected(value)
    if kind == 0: kind = _dispatch_subclass(value)
    if kind == 6 or (pd is not None and value is pd.NaT): return 'NULL'
    if kind == 1: return '1' if value else '0'
    if kind == 2: return str(int(value))
    if kind == 3:
        if not math.isfinite(value): return 'NULL'
        text = repr(float(value))
        return _json_float(text) if type(value) is not (np.float64 if np is not None else None) and abs(value)>=2.2250738585072014e-308 else str(value)
    if kind == 4: return _quote(value if type(value) is str or (np is not None and type(value) is np.str_) else str(value))
    if kind == 5: return '_binary' + _quote(bytes(value).decode('ascii','surrogateescape'))
    if kind == 7:
        if not value.is_finite(): raise ValueError('nonfinite Decimal')
        return str(value)
    if kind == 8: return _quote('%04d-%02d-%02d %02d:%02d:%02d%s' % (value.year,value.month,value.day,value.hour,value.minute,value.second,'.%06d' % value.microsecond if value.microsecond else ''))
    if kind == 9: return _quote('%04d-%02d-%02d' % (value.year,value.month,value.day))
    if kind == 10: return _quote('%02d:%02d:%02d%s' % (value.hour,value.minute,value.second,'.%06d' % value.microsecond if value.microsecond else ''))
    if kind == 11: return _duration((value.days*86400+value.seconds)*1000000+value.microseconds)
    if kind == 12: return _literal(dt.datetime(*value[:6]),active,depth+1)
    if np is not None:
        if isinstance(value,np.datetime64):
            unit = np.datetime_data(value.dtype)[0]
            if unit not in ('ns','us','ms','s','m','h','D','W','M','Y'): raise ValueError('unsupported datetime64 unit')
            if np.isnat(value): return 'NULL'
            return _literal(_numpy_datetime(value,False),active,depth+1)
        if isinstance(value,np.timedelta64):
            unit, multiplier = np.datetime_data(value.dtype)
            if np.isnat(value): return 'NULL'
            factors = dict(ns=1,us=1000,ms=1000000,s=1000000000,m=60000000000,h=3600000000000,D=86400000000000,W=604800000000000,M=2629746000000000,Y=31556952000000000)
            if unit not in factors: raise ValueError('unsupported timedelta64 unit')
            nanos = int(value.view('i8'))*factors[unit]
            return _duration(nanos//1000)
        if isinstance(value,np.record): return _literal(tuple(value),active,depth+1)
        if isinstance(value,np.generic):
            if value.dtype.kind == 'f':
                if value.dtype.itemsize not in (2,4,8): raise TypeError('unsupported numpy float width')
                return str(value) if np.isfinite(value) else 'NULL'
            if value.dtype.kind not in 'biufSU': raise TypeError('unsupported numpy scalar')
            return _literal(value.item(),active,depth+1)
        if type(value) is np.ndarray:
            if value.ndim not in (1,2): raise ValueError('unsupported ndarray dimension')
            if (value.dtype.kind not in 'biufOSUmM' or (value.dtype.kind in 'iu' and value.dtype.itemsize not in (1,2,4,8)) or (value.dtype.kind == 'f' and value.dtype.itemsize not in (2,4,8))): raise TypeError('unsupported ndarray shape or dtype')
            if id(value) in active: raise ValueError('cyclic array')
            active.add(id(value))
            try:
                if value.size == 0: return '()'
                if value.ndim == 1: return '('+','.join(_array_literal(value[i],active,depth+1) for i in range(value.shape[0]))+')'
                return ','.join('('+','.join(_array_literal(value[i,j],active,depth+1) for j in range(value.shape[1]))+')' for i in range(value.shape[0]))
            finally: active.remove(id(value))
    if pd is not None and (isinstance(value,(pd.Series,pd.Index)) or type(value) is pd.DataFrame): return _literal(value.values,active,depth+1)
    seq = _sequence(value)
    if seq is not None:
        if id(value) in active: raise ValueError('cyclic container')
        active.add(id(value))
        try:
            parts = [_literal(x,active,depth+1) for x in seq]
            nested = bool(seq) and all(_sequence(x) is not None for x in seq)
            return ','.join(parts) if nested else '('+','.join(parts)+')'
        finally: active.remove(id(value))
    raise TypeError('unsupported SQL type: '+type(value).__name__)

def escape(data: object, many: bool=False, itemize: bool=True) -> str | tuple | list:
    """Convert scalars and supported container shapes to SQL literals or parameter items."""
    try:
        if pd is not None and type(data) is pd.DataFrame:
            if data.empty: return [] if many or itemize else '()'
            columns = [series.values for _,series in data.items()]
            rows = [tuple(_array_literal(column[i],set(),0) for column in columns) for i in range(len(data))]
            if many or itemize: return rows
            return ','.join('('+','.join(row)+')' for row in rows)
        value = data.values if pd is not None and isinstance(data,(pd.Series,pd.Index)) else data
        if np is not None and type(value) is np.ndarray:
            if value.ndim not in (1,2): raise ValueError('unsupported ndarray dimension')
            if (value.dtype.kind not in 'biufOSUmM' or (value.dtype.kind in 'iu' and value.dtype.itemsize not in (1,2,4,8)) or (value.dtype.kind == 'f' and value.dtype.itemsize not in (2,4,8))): raise TypeError('unsupported ndarray shape or dtype')
            if not (many or itemize): return _literal(value,set(),0)
            if value.ndim == 2 and value.size == 0: return []
            if value.ndim == 2: return [tuple(_array_literal(value[i,j],{id(value)},1) for j in range(value.shape[1])) for i in range(value.shape[0])]
            items = [_array_literal(value[i],{id(value)},1) for i in range(value.shape[0])]
            return items if many else tuple(items)
        else: seq = _sequence(value)
        if seq is None or not (many or itemize): return _literal(value,set(),0)
        if many:
            return [tuple(_literal(y,{id(value),id(x)},1) for y in _sequence(x)) if _sequence(x) is not None else _literal(x,{id(value)},1) for x in seq]
        return tuple(_literal(x,{id(value)},1) for x in seq)
    except ValueError as exc: raise EscapeValueError(str(exc)) from exc
    except TypeError as exc: raise EscapeTypeError(str(exc)) from exc
    except EscapeError: raise
    except AssertionError: raise
    except Exception as exc: raise EscapeError(str(exc)) from exc

cdef str _json_string(object value):
    """Encode a JSON string with strict surrogate rejection and minimal escapes."""
    cdef list out = ['"']
    cdef str ch
    cdef int code
    cdef Py_ssize_t i, length = PyUnicode_GET_LENGTH(value)
    for i in range(length):
        code = PyUnicode_READ_CHAR(value,i)
        ch = chr(code)
        if 0xD800 <= code <= 0xDFFF: raise ValueError('surrogate in JSON string')
        if ch in ('"','\\'): out.append('\\'+ch)
        elif ch in '\b\f\n\r\t': out.append({'\b':'\\b','\f':'\\f','\n':'\\n','\r':'\\r','\t':'\\t'}[ch])
        elif code < 32: out.append('\\u%04x' % code)
        else: out.append(ch)
    out.append('"')
    return ''.join(out)

cdef str _json_float(str text, int width=8):
    """Apply explicit shortest-digit decimal/scientific cutoffs without serializer forwarding."""
    if 'e' not in text: return text
    mantissa, exponent_text = text.split('e')
    exponent = int(exponent_text)
    if -(6 if width <= 4 else 5) <= exponent < (13 if width <= 4 else 16):
        sign = '-' if mantissa.startswith('-') else ''
        digits = mantissa.lstrip('-').replace('.','')
        digits = digits.rstrip('0') or '0'
        point = exponent + 1
        if point <= 0: return sign+'0.'+'0'*(-point)+digits
        if point >= len(digits): return sign+digits+'0'*(point-len(digits))+'.0'
        return sign+digits[:point]+'.'+digits[point:]
    if mantissa.endswith('.0'): mantissa=mantissa[:-2]
    if mantissa.endswith('.'): mantissa=mantissa[:-1]
    return mantissa+'e'+('+' if exponent>=0 else '-')+str(abs(exponent))

cdef void _emit(list chunks, list remaining, str text):
    """Reserve UTF-8 output bytes before accumulating each bounded fragment."""
    cdef Py_ssize_t size
    if len(text) > remaining[0]: raise ValueError('JSON output exceeds 16 MiB budget')
    size = len(text.encode('utf-8'))
    if size > remaining[0]: raise ValueError('JSON output exceeds 16 MiB budget')
    remaining[0] -= size
    chunks.append(text)

cdef void _emit_string(list chunks, list remaining, object value):
    """Compute escaped UTF-8 size before creating a potentially large string fragment."""
    cdef Py_ssize_t size = 2, i, length = PyUnicode_GET_LENGTH(value)
    cdef unsigned int code
    if length > remaining[0]: raise ValueError('JSON output exceeds 16 MiB budget')
    for i in range(length):
        code = <unsigned int>PyUnicode_READ_CHAR(value,i)
        if 0xD800 <= code <= 0xDFFF: raise ValueError('surrogate in JSON string')
        if code in (34,92,8,12,10,13,9): size += 2
        elif code < 32: size += 6
        elif code < 128: size += 1
        elif code < 2048: size += 2
        elif code < 65536: size += 3
        else: size += 4
        if size > remaining[0]: raise ValueError('JSON output exceeds 16 MiB budget')
    _emit(chunks,remaining,_json_string(value))

cdef void _json_write(object value, set active, int depth, list chunks, list remaining):
    """Write JSON fragments with recursion, cycle and pre-allocation UTF-8 output budgets."""
    cdef object field, key, item
    cdef bint first
    if depth > 128: raise ValueError('maximum JSON depth exceeded')
    if value is None: _emit(chunks,remaining,'null'); return
    if isinstance(value,enum.Enum): _json_write(value.value,active,depth+1,chunks,remaining); return
    if isinstance(value,bool): _emit(chunks,remaining,'true' if value else 'false'); return
    if isinstance(value,int):
        text = int.__repr__(value)
        numeric = int(text)
        if numeric < -(1<<63) or numeric > (1<<64)-1: raise ValueError('JSON integer outside 64-bit domain')
        _emit(chunks,remaining,text); return
    if isinstance(value,float) and type(value) is not float and not (np is not None and type(value) is np.float64): raise TypeError('JSON float subclasses unsupported')
    if isinstance(value,float): _emit(chunks,remaining,_json_float(repr(float(value))) if math.isfinite(value) else 'null'); return
    if isinstance(value,str): _emit_string(chunks,remaining,value); return
    if isinstance(value,uuid.UUID): _emit_string(chunks,remaining,str(value)); return
    if isinstance(value,(dt.datetime,dt.date,dt.time)) and type(value) not in (dt.datetime,dt.date,dt.time): raise TypeError('JSON calendar subclasses unsupported')
    if isinstance(value,dt.time) and value.tzinfo is not None: raise TypeError('JSON time cannot have tzinfo')
    if isinstance(value,(dt.datetime,dt.date,dt.time)): _emit_string(chunks,remaining,value.isoformat()); return
    if np is not None and isinstance(value,np.generic):
        if isinstance(value,np.floating):
            if value.dtype.itemsize not in (2,4,8): raise TypeError('unsupported JSON float width')
            _emit(chunks,remaining,_json_float(str(np.float32(value) if value.dtype.itemsize == 2 else value),max(4,value.dtype.itemsize)) if np.isfinite(value) else 'null'); return
        if isinstance(value,np.timedelta64): raise TypeError('JSON timedelta64 is unsupported')
        if isinstance(value,np.datetime64):
            if np.datetime_data(value.dtype)[1] != 1: raise TypeError('JSON datetime64 multipliers are unverified and unsupported')
            if np.isnat(value): raise ValueError('JSON NaT')
            _emit_string(chunks,remaining,_numpy_datetime(value).isoformat()); return
        _json_write(value.item(),active,depth+1,chunks,remaining); return
    if dataclasses.is_dataclass(value) and not isinstance(value,type):
        if id(value) in active: raise ValueError('cyclic JSON dataclass')
        active.add(id(value))
        try:
            _emit(chunks,remaining,'{'); first=True
            for field in dataclasses.fields(value):
                if field.name.startswith('_'): continue
                if not first: _emit(chunks,remaining,',')
                first=False; _emit_string(chunks,remaining,field.name); _emit(chunks,remaining,':')
                _json_write(getattr(value,field.name),active,depth+1,chunks,remaining)
            _emit(chunks,remaining,'}'); return
        finally: active.remove(id(value))
    if np is not None and isinstance(value,np.ndarray) and type(value) is not np.ndarray: raise TypeError('JSON ndarray subclasses unsupported')
    if isinstance(value,(dict,list,tuple)) or (np is not None and isinstance(value,np.ndarray)):
        if id(value) in active: raise ValueError('cyclic JSON value')
        if np is not None and isinstance(value,np.ndarray) and (value.dtype.kind not in 'biufM' or not value.flags.c_contiguous or not value.dtype.isnative): raise TypeError('unsupported JSON ndarray layout or dtype')
        if isinstance(value,tuple) and type(value) is not tuple: raise TypeError('JSON tuple subclasses unsupported')
        active.add(id(value))
        try:
            if isinstance(value,dict):
                snapshot = list(dict.items(value))
                if any(not isinstance(key,str) for key,item in snapshot): raise TypeError('JSON keys must be strings')
                _emit(chunks,remaining,'{'); first=True
                for key,item in snapshot:
                    if not first: _emit(chunks,remaining,',')
                    first=False; _emit_string(chunks,remaining,key); _emit(chunks,remaining,':')
                    _json_write(item,active,depth+1,chunks,remaining)
                _emit(chunks,remaining,'}'); return
            _emit(chunks,remaining,'['); first=True
            for item in list(list.__iter__(value)) if isinstance(value,list) else value:
                if not first: _emit(chunks,remaining,',')
                first=False; _json_write(item,active,depth+1,chunks,remaining)
            _emit(chunks,remaining,']'); return
        finally: active.remove(id(value))
    raise TypeError('unsupported JSON type: '+type(value).__name__)

def dumps(value: object) -> bytes:
    """Serialize compact UTF-8 JSON with pre-accumulation byte and recursive graph budgets."""
    cdef list chunks = []
    cdef list remaining = [MAX_JSON_BYTES]
    _json_write(value,set(),0,chunks,remaining)
    return ''.join(chunks).encode('utf-8')

cdef class _Parser:
    """Strict recursive-descent JSON parser with bounded recursion and complete consumption."""
    cdef str text
    cdef Py_ssize_t pos, size
    def __init__(self, text: str) -> None:
        """Initialize the owned Unicode input and cursor."""
        self.text=text; self.pos=0; self.size=len(text)
    cdef void whitespace(self):
        """Consume only the four whitespace characters allowed by JSON."""
        while self.pos < self.size and self.text[self.pos] in ' \t\r\n': self.pos+=1
    cdef str string(self):
        """Read one quoted string, checking controls, escapes and surrogate pairs."""
        cdef list out=[]
        cdef str ch, token
        cdef int code, low
        self.pos+=1
        while self.pos < self.size:
            ch=self.text[self.pos]; self.pos+=1
            if ch=='"': return ''.join(out)
            if ord(ch)<32 or 0xD800<=ord(ch)<=0xDFFF: raise ValueError('invalid JSON string character')
            if ch!='\\': out.append(ch); continue
            if self.pos>=self.size: raise ValueError('truncated escape')
            ch=self.text[self.pos]; self.pos+=1
            if ch in '"\\/': out.append(ch)
            elif ch in 'bfnrt': out.append(dict(b='\b',f='\f',n='\n',r='\r',t='\t')[ch])
            elif ch=='u':
                token=self.text[self.pos:self.pos+4]
                if len(token)!=4 or any(c not in '0123456789abcdefABCDEF' for c in token): raise ValueError('invalid Unicode escape')
                code=int(token,16); self.pos+=4
                if 0xD800<=code<=0xDBFF:
                    if self.text[self.pos:self.pos+2]!='\\u': raise ValueError('missing low surrogate')
                    self.pos+=2; token=self.text[self.pos:self.pos+4]
                    if len(token)!=4 or any(c not in '0123456789abcdefABCDEF' for c in token): raise ValueError('invalid low surrogate')
                    low=int(token,16); self.pos+=4
                    if not 0xDC00<=low<=0xDFFF: raise ValueError('invalid low surrogate')
                    code=0x10000+((code-0xD800)<<10)+low-0xDC00
                elif 0xDC00<=code<=0xDFFF: raise ValueError('unpaired low surrogate')
                out.append(chr(code))
            else: raise ValueError('invalid JSON escape')
        raise ValueError('unterminated JSON string')
    cdef object value(self, int depth):
        """Parse one JSON value and reject malformed punctuation or nonfinite numbers."""
        if depth>128: raise ValueError('maximum JSON depth exceeded')
        self.whitespace()
        if self.pos>=self.size: raise ValueError('missing JSON value')
        ch=self.text[self.pos]
        if ch=='"': return self.string()
        if ch in '[{':
            self.pos+=1; mapping=ch=='{'; out={} if mapping else []; end='}' if mapping else ']'
            self.whitespace()
            if self.pos<self.size and self.text[self.pos]==end: self.pos+=1; return out
            while True:
                self.whitespace()
                if mapping:
                    if self.pos>=self.size or self.text[self.pos]!='"': raise ValueError('object key must be quoted')
                    key=self.string(); self.whitespace()
                    if self.pos>=self.size or self.text[self.pos]!=':': raise ValueError('missing colon')
                    self.pos+=1; out[key]=self.value(depth+1)
                else: out.append(self.value(depth+1))
                self.whitespace()
                if self.pos>=self.size: raise ValueError('unterminated container')
                ch=self.text[self.pos]; self.pos+=1
                if ch==end: return out
                if ch!=',': raise ValueError('missing comma')
        for token, result in [('true',True),('false',False),('null',None)]:
            if self.text.startswith(token,self.pos): self.pos+=len(token); return result
        match=_JSON_NUMBER.match(self.text,self.pos)
        if match is None: raise ValueError('invalid JSON value')
        token=match.group(); self.pos+=len(token)
        if any(c in token for c in '.eE'): result=float(token)
        else:
            result=int(token)
            if result < -(1<<63) or result > (1<<64)-1: result=float(token)
        if isinstance(result,float) and not math.isfinite(result): raise ValueError('JSON numeric overflow')
        return result
    def parse(self) -> object:
        """Return the complete tree and reject trailing non-whitespace content."""
        result=self.value(0); self.whitespace()
        if self.pos!=self.size: raise ValueError('trailing JSON content')
        return result

def loads(value: bytes | str) -> object:
    """Parse strict UTF-8 JSON independently, including escaped surrogate pairs."""
    if not isinstance(value,(bytes,str)): raise TypeError('JSON input must be bytes or str')
    if len(value)>MAX_JSON_BYTES: raise ValueError('JSON input exceeds 16 MiB budget')
    if isinstance(value,str) and len(value.encode('utf-8','strict'))>MAX_JSON_BYTES: raise ValueError('JSON input exceeds 16 MiB budget')
    if isinstance(value,bytes): value=value.decode('utf-8','strict')
    elif not isinstance(value,str): raise TypeError('JSON input must be bytes or str')
    return _Parser(value).parse()

def decode(value: bytes, field_type: int, encoding: bytes, is_binary: bool, use_decimal: bool=False, decode_bit: bool=False, decode_json: bool=False) -> object:
    """Decode MySQL text-protocol fields with strict numbers and date fallback semantics."""
    if not isinstance(value,bytes): raise TypeError('field value must be bytes')
    try:
        codec=encoding.decode('ascii') if isinstance(encoding,bytes) else encoding
        if field_type==16:
            if decode_bit and len(value)>8: raise ValueError('BIT field exceeds 64 bits')
            return int.from_bytes(value,'big') if decode_bit else value
        if field_type in (1,2,3,8,9,13):
            text=value.decode('ascii')
            if re.fullmatch(r'-?[0-9]+',text) is None: raise ValueError('invalid integer field')
            result=int(text)
            if result<-(1<<63) or result>(1<<64)-1: raise ValueError('integer field overflow')
            return result
        if field_type in (0,4,5,246):
            text=value.decode('ascii')
            if re.fullmatch(r'[+-]?(?:[0-9]+(?:\.[0-9]*)?|\.[0-9]+)(?:[eE][+-]?[0-9]+)?',text) is None: raise ValueError('invalid numeric field')
            result=decimal.Decimal(text) if use_decimal and field_type in (0,246) else float(text)
            if not (result.is_finite() if isinstance(result,decimal.Decimal) else math.isfinite(result)): raise ValueError('numeric field overflow')
            return result
        if field_type in (7,10,11,12,14):
            text=value.decode('ascii')
            try:
                if field_type in (10,14):
                    match=re.fullmatch(r'([0-9]{1,4})-([0-9]{1,2})-([0-9]{1,2})',text)
                    if match is None: return text
                    return dt.date(*map(int,match.groups()))
                if field_type in (7,12):
                    match=re.fullmatch(r'([0-9]{1,4})-([0-9]{1,2})-([0-9]{1,2})[ T]([0-9]{1,2}):([0-9]{1,2}):([0-9]{1,2})(?:\.([0-9]{1,6}))?',text)
                    if match is None: return text
                    fields=match.groups()
                    return dt.datetime(*map(int,fields[:6]),microsecond=int((fields[6] or '').ljust(6,'0') or '0'))
                match=re.fullmatch(r'(-?)([0-9]+):([0-9]{2}):([0-9]{2})(?:\.([0-9]{1,6}))?',text)
                if match is None: return text
                sign,h,m,s,f=match.groups()
                if int(m)>59 or int(s)>59: return text
                micros=((int(h)*60+int(m))*60+int(s))*1000000+int((f or '').ljust(6,'0') or '0')
                return dt.timedelta(microseconds=-micros if sign else micros)
            except (ValueError,OverflowError): return text
        if field_type==245:
            text=value.decode(codec,'surrogateescape')
            return loads(text) if decode_json else text
        if field_type in (247,248): return value.decode(codec,'surrogateescape')
        return value if is_binary else value.decode(codec,'surrogateescape')
    except (ValueError,OverflowError) as exc: raise DecodeValueError(str(exc)) from exc
    except Exception as exc: raise DecodeTypeError(str(exc)) from exc

class _JSONCodec:
    """Namespace exposing the independent JSON encoder and parser."""
    @staticmethod
    def dumps(value: object) -> bytes:
        """Serialize a supported object tree to compact UTF-8 bytes."""
        return dumps(value)
    @staticmethod
    def loads(value: bytes | str) -> object:
        """Parse one complete strict JSON document."""
        return loads(value)
json = _JSONCodec()
