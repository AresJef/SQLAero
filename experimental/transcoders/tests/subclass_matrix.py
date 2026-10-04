"""Cold-path subclass contracts, including overridden conversions and spoofing."""
import importlib
import json
import sys
import datetime as dt
from decimal import Decimal
import numpy as np
import pandas as pd
import orjson
import sqlaero.transcode as original
from type_matrix import Record

class Text(str):
    """Ordinary string with conversion and encode overrides."""
    def __str__(self) -> str:
        """Supply the ordinary SQL subclass conversion text."""
        return "replacement'"
    def encode(self,*args:object,**kwargs:object) -> bytes:
        """Expose an override that must not replace underlying JSON storage."""
        return b'hijacked'

class Integer(int):
    """Integer subtype whose conversion differs from stored digits."""
    def __int__(self) -> int:
        """Supply the SQL numeric subtype conversion value."""
        return 7
    def __str__(self) -> str:
        """Provide untrusted text that numeric writers must never emit raw."""
        return 'UNTRUSTED()'

class Floating(float):
    """Float subtype with an explicit numeric conversion override."""
    def __float__(self) -> float:
        """Supply the expected ordinary SQL float-subtype conversion."""
        return 7.5

class Dec(Decimal):
    """Decimal subtype outside the original SQL accepted subtype domain."""
class Array(np.ndarray):
    """Array subtype outside the original SQL and JSON exact-array domain."""
class Frame(pd.DataFrame):
    """DataFrame subtype outside the original SQL exact-frame domain."""
class Tuple(tuple):
    """Tuple subtype accepted by SQL but rejected by original JSON encoder."""
class Date(dt.date):
    """Date subtype accepted by SQL but rejected by original JSON encoder."""
class NaTType:
    """Spoofed class name that must not be mistaken for pandas missing time."""


def outcome(function:object,value:object) -> dict[str,str]:
    """Capture actual result semantics and whether unsupported domains reject."""
    try:
        result=function(value)
        return {'status':'ok','type':type(result).__name__,'value':repr(result)}
    except Exception:
        return {'status':'error'}


def main() -> int:
    """Compare SQL subclass conversions and JSON exact/base-storage policies."""
    modules=[importlib.import_module(name) for name in sys.argv[1:]]
    values=[Text('stored'),Integer(3),Floating(1.25),Dec('1.25'),np.array([1,2]).view(Array),
            Frame({'x':[1]}),Tuple([1,2]),Date(2024,1,1),NaTType(),Record]
    rows:list[dict[str,object]]=[]
    for value in values:
        for mode in ('sql','json'):
            baseline=outcome(lambda x:original.escape(x,False,False),value) if mode=='sql' else outcome(lambda x:orjson.dumps(x,option=orjson.OPT_SERIALIZE_NUMPY),value)
            actual={module.__name__:outcome(lambda x:module.escape(x,False,False),value) if mode=='sql' else outcome(module.dumps,value) for module in modules}
            rows.append({'case':type(value).__name__,'mode':mode,'baseline':baseline,'actual':actual,'match':{key:result==baseline for key,result in actual.items()}})
    print(json.dumps({'rows':len(rows),'matches':{module.__name__:sum(row['match'][module.__name__] for row in rows) for module in modules},'cases':rows},indent=2))
    return 0 if all(all(row['match'].values()) for row in rows) else 1

if __name__=='__main__': raise SystemExit(main())
