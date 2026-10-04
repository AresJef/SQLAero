"""Deterministic numeric-format correctness vectors; no performance measurement."""
import importlib
import json
import math
import random
import struct
import sys
import numpy as np
import orjson
import sqlaero.transcode as original


def main() -> int:
    """Compare precision and spelling at thresholds and seeded IEEE bit patterns."""
    modules=[importlib.import_module(name) for name in sys.argv[1:]]
    values:list[tuple[str,object]]=[]
    seed=random.Random(20261004)
    known=[0.,-0.,1.1,-2.2,1e-4,1e-5,1e-6,1e-7,1e6,1e12,1e13,1e15,1e16,1e20,1e21]
    for name,typ,fmt,bits in [('f4',np.float32,'f',32),('f8',np.float64,'d',64)]:
        native=[typ(x) for x in known]+[np.nextafter(typ(0),typ(1)),np.finfo(typ).max,np.finfo(typ).tiny]
        for _ in range(64):
            raw=seed.getrandbits(bits).to_bytes(bits//8,'little')
            number=struct.unpack('<'+fmt,raw)[0]
            if math.isfinite(number):native.append(typ(number))
        for i,value in enumerate(native): values.append((f'{name}-{i}',value))
    rows:list[dict[str,object]]=[]
    for name,value in values:
        for mode in ('sql-scalar','json-scalar','json-array'):
            arg=value if mode!='json-array' else np.array([value],dtype=value.dtype)
            expected=original.escape(arg,False,False) if mode=='sql-scalar' else orjson.dumps(arg,option=orjson.OPT_SERIALIZE_NUMPY)
            actual={module.__name__:module.escape(arg,False,False) if mode=='sql-scalar' else module.dumps(arg) for module in modules}
            rows.append({'case':name,'mode':mode,'input':repr(value),'expected':repr(expected),'actual':{key:repr(v) for key,v in actual.items()},'match':{key:v==expected for key,v in actual.items()}})
    print(json.dumps({'rows':len(rows),'matches':{module.__name__:sum(row['match'][module.__name__] for row in rows) for module in modules},'cases':rows},indent=2))
    return 0 if all(all(row['match'].values()) for row in rows) else 1

if __name__=='__main__': raise SystemExit(main())
