"""Freeze rich JSON bytes/trees against the actual baseline dependency, test only."""
import datetime as dt
import importlib
import json
import sys
import uuid
from decimal import Decimal
from pathlib import Path
import numpy as np
import orjson
from type_matrix import Choice,Record


def outcome(function:object,value:object) -> dict[str,str]:
    """Compare accepted domains and precise outputs; internal error classes may differ."""
    try:
        result=function(value)
        return {'status':'ok','type':type(result).__name__,'value':repr(result)}
    except Exception:
        return {'status':'error'}


def main() -> int:
    """Measure no timing; record encoding and decoding semantic comparisons."""
    modules=[importlib.import_module(name) for name in sys.argv[1:]]
    values:list[tuple[str,object]]=[('null',None),('bool',True),('int',2**64-1),('bigint',2**64),
        ('float',1.25),('exp',1e-7),('negzero',-0.0),('nan',float('nan')),('inf',float('inf')),
        ('text','雪😀\0\n\\"'),('surrogate','\ud800'),('dict',{'x':[True,None,1]}),
        ('nonstringkey',{1:'x'}),('uuid',uuid.UUID(int=0)),('enum',Choice.ONE),('dataclass',Record()),
        ('date',dt.date(2024,2,29)),('time',dt.time(1,2,3,4)),('awaretime',dt.time(1,2,tzinfo=dt.timezone.utc)),
        ('datetime',dt.datetime(2024,2,29,1,2,3,4)),('awaredatetime',dt.datetime(2024,1,1,tzinfo=dt.timezone.utc)),
        ('decimal',Decimal('1.25')),('bytes',b'a'),('set',{1}),('npstrNUL',np.str_('\0')),
        ('npfloat16',np.float16(1.1)),('npfloat32',np.float32(1.1)),('npfloat64',np.float64(1.1)),
        ('npint',np.int64(-1)),('npdatetime',np.datetime64('2024-01-01','D')),
        ('nptimedelta',np.timedelta64(1,'D')),('npNaTgeneric',np.datetime64('NaT')),
        ('arrayf2',np.array([1.1],dtype='f2')),('arrayf4',np.array([1.1],dtype='f4')),
        ('arrayf8',np.array([1.1,float('nan')],dtype='f8')),('arrayint',np.array([[1,2],[3,4]],dtype='i8')),
        ('arrayreverse',np.array([1,2])[::-1]),('arrayswapped',np.array([1,2],dtype='>i4')),
        ('arrayF',np.array([[1,2],[3,4]],order='F'))]
    tokens=[b'null',b'true',b'1',b'-0.0',b'1e-7',b'18446744073709551616',b'-9223372036854775809',
        b'1234567890123456789012345678901234567890',b'{"x":1,"x":2}',b'"\\ud83d\\ude00"',
        b'"\\ud800"',b'"\xff"',b'NaN',b'Infinity',b'1e309',b'01',b'[1,]',b'{"x":}',b'null x']
    rows:list[dict[str,object]]=[]
    for name,value in values:
        baseline=outcome(lambda data:orjson.dumps(data,option=orjson.OPT_SERIALIZE_NUMPY),value)
        actual={module.__name__:outcome(module.dumps,value) for module in modules}
        rows.append({'kind':'dumps','case':name,'baseline':baseline,'actual':actual,'match':{key:res==baseline for key,res in actual.items()}})
    for token in tokens:
        baseline=outcome(orjson.loads,token)
        actual={module.__name__:outcome(module.loads,token) for module in modules}
        rows.append({'kind':'loads','case':repr(token),'baseline':baseline,'actual':actual,'match':{key:res==baseline for key,res in actual.items()}})
    print(json.dumps({'baseline':'orjson 3.12.0 OPT_SERIALIZE_NUMPY','rows':len(rows),'matches':{module.__name__:sum(row['match'][module.__name__] for row in rows) for module in modules},'cases':rows},indent=2))
    return 0 if all(all(row['match'].values()) for row in rows) else 1

if __name__=='__main__': raise SystemExit(main())
