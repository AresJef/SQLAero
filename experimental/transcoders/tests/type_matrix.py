"""Small frozen-baseline type matrix, never a benchmark or database test."""
import datetime as dt
import enum
import importlib
import json
import math
import sys
import time
import uuid
from dataclasses import dataclass
from decimal import Decimal
from pathlib import Path
import numpy as np
import pandas as pd

@dataclass
class Record:
    """JSON dataclass with a private field omitted by the frozen encoder."""
    value:int = 1
    _private:str = 'hidden'

class Choice(enum.Enum):
    """JSON Enum whose value belongs to the supported scalar domain."""
    ONE = 'one'


def cases() -> list[tuple[str,object]]:
    """Construct all baseline-safe scalar/container/array/pandas representatives."""
    values:list[tuple[str,object]] = [('none',None),('bool',True),('int',2**100),
        ('float',1.25),('float-exp',1e-7),('float-zero',-0.0),('float-nan',math.nan),
        ('text',"雪\0a'b\\c\n"),('bytes',b'\0\xff\'\\'),('bytearray',bytearray(b'\xff')),
        ('memoryview',memoryview(b'abc')[::2]),('decimal',Decimal('1.2300')),
        ('datetime',dt.datetime(2024,2,29,1,2,3,4)),('date',dt.date(2024,2,29)),
        ('time',dt.time(1,2,3,4)),('timedelta',dt.timedelta(days=-2,microseconds=1)),
        ('struct-time',time.struct_time((2024,2,29,1,2,3,3,60,0))),
        ('list',[1,'x']),('tuple',(1,'x')),('nested',[[1,2],[3,4]]),('dict',{'a':1,'b':'x'}),
        ('dict-keys',{'a':1}.keys()),('dict-values',{'a':1}.values()),('dict-items',{'a':1}.items()),
        ('set',{1,2}),('frozenset',frozenset([1,2])),('range',range(3)),
        ('npstr-nul',np.str_('\0')),('npbytes-nul',np.bytes_(b'\0')),
        ('pdNaT',pd.NaT),('timestamp',pd.Timestamp('2024-02-29 01:02:03.123456789')),
        ('pdtimedelta',pd.Timedelta(-1,'ns')),('series',pd.Series([1,2])),
        ('index',pd.Index([1,2])),('datetimeindex',pd.date_range('2024-01-01',periods=2)),
        ('timedeltaindex',pd.to_timedelta([0,-1],unit='us')),
        ('dataframe',pd.DataFrame({'i':[1,2],'f':np.array([1.1,2.2],dtype='f4')}))]
    for dtype in ['i1','i2','i4','i8','u1','u2','u4','u8','f2','f4','f8','bool','U3','S3','object']:
        data = ['a','b'] if dtype in ('U3','S3') else [1,2]
        for ndim in (1,2):
            values.append((f'array-{dtype}-{ndim}',np.array(data if ndim==1 else [data,data],dtype=dtype)))
        if dtype!='object': values.append((f'scalar-{dtype}',np.array(data,dtype=dtype)[0]))
    for unit in ['ns','us','ms','s','m','h','D','W','M','Y']:
        values.append(('datetime64-'+unit,np.datetime64(1,unit)))
        values.append(('timedelta64-'+unit,np.timedelta64(-1,unit)))
    return values


def outcome(module:object,data:object,many:bool,itemize:bool) -> dict[str,str]:
    """Capture precise Python result type/value or public exception class."""
    try:
        result=module.escape(data,many,itemize)
        return {'type':type(result).__name__,'value':repr(result)}
    except Exception as exc:
        return {'error':type(exc).__module__+'.'+type(exc).__name__}


def main() -> int:
    """Freeze baseline results and compare selected compiled experiment modules."""
    baseline=importlib.import_module('sqlaero.transcode')
    modules=[importlib.import_module(name) for name in sys.argv[1:]]
    rows:list[dict[str,object]]=[]
    for name,data in cases():
        for many in (False,True):
            for itemize in (False,True):
                expected=outcome(baseline,data,many,itemize)
                actual={module.__name__:outcome(module,data,many,itemize) for module in modules}
                rows.append({'case':name,'many':many,'itemize':itemize,'baseline':expected,
                             'actual':actual,'match':{key:value==expected for key,value in actual.items()}})
    report={'rows':len(rows),'matches':{module.__name__:sum(row['match'][module.__name__] for row in rows) for module in modules},'cases':rows}
    print(json.dumps(report,indent=2))
    return 0 if all(all(row['match'].values()) for row in rows) else 1

if __name__=='__main__': raise SystemExit(main())
