"""All option/binary combinations for text-protocol field families, without DB."""
import importlib
import itertools
import json
import sys
import sqlaero.transcode as original


def outcome(module:object,value:bytes,field:int,binary:bool,options:tuple[bool,bool,bool]) -> dict[str,str]:
    """Capture the exact returned Python type/value and public failure class."""
    try:
        result=module.decode(value,field,b'utf8',binary,*options)
        return {'type':type(result).__name__,'value':repr(result)}
    except Exception as exc:
        return {'error':type(exc).__module__+'.'+type(exc).__name__}


def main() -> int:
    """Compare every field family across all three options and binary metadata."""
    modules=[importlib.import_module(name) for name in sys.argv[1:]]
    fields=[(field,b'a\xff') for field in (254,253,15,249,252,250,251,255)]
    fields += [(field,b'123') for field in (1,2,3,8,9,13)]
    fields += [(0,b'1.2300'),(246,b'1.2300'),(4,b'1.25'),(5,b'1.25'),
               (7,b'2024-02-29 01:02:03.123456'),(12,b'2024-02-29 01:02:03.123456'),
               (10,b'2024-02-29'),(14,b'0000-00-00'),(11,b'-25:06:07.123456'),
               (16,b'\x01\x02'),(247,b'a'),(248,b'a,b'),(245,b'{"x":1}'),(17,b'opaque'),(19,b'opaque')]
    rows:list[dict[str,object]]=[]
    for field,value in fields:
        for binary in (False,True):
            for options in itertools.product((False,True),repeat=3):
                expected=outcome(original,value,field,binary,options)
                actual={module.__name__:outcome(module,value,field,binary,options) for module in modules}
                rows.append({'field':field,'binary':binary,'options':options,'baseline':expected,'actual':actual,'match':{key:result==expected for key,result in actual.items()}})
    print(json.dumps({'rows':len(rows),'matches':{module.__name__:sum(row['match'][module.__name__] for row in rows) for module in modules},'cases':rows},indent=2))
    return 0 if all(all(row['match'].values()) for row in rows) else 1

if __name__=='__main__': raise SystemExit(main())
