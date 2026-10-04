"""Run pure legacy transcoder methods independently, without DB or benchmarking."""
import contextlib
import importlib
import importlib.util
import io
import json
import sys
import traceback
from pathlib import Path

def main() -> int:
    """Record every existing pure test method instead of stopping at first error."""
    name = sys.argv[1]
    source = Path(__file__).resolve().parents[3] / 'src' / 'test_transcode.py'
    spec = importlib.util.spec_from_file_location('legacy_transcoder_cases',source)
    if spec is None or spec.loader is None: raise RuntimeError('Cannot load test module')
    cases = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(cases)
    kernel = importlib.import_module(name)
    from sqlaero.transcode import ObjStr
    from sqlaero.sqlfunc import RawText, SQLFunction
    from sqlaero.sqlintvl import SQLInterval
    if hasattr(kernel, "configure_types"):
        kernel.configure_types(objstr=(ObjStr,),rawtext=(RawText,),sqlfunction=(SQLFunction,),sqlinterval=(SQLInterval,))
    cases.escape, cases.decode = kernel.escape,kernel.decode
    results:list[dict[str,object]] = []
    for cls in (cases.TestEscape,cases.TestDecode):
        instance = cls()
        for method in cls.__dict__:
            if method=='test_all' or not(method.startswith('test_escape_') or method.startswith('decode_')): continue
            record:dict[str,object] = {'case':cls.__name__+'.'+method}
            try:
                with contextlib.redirect_stdout(io.StringIO()): getattr(instance,method)()
                record['passed'] = True
            except Exception:
                record['passed'] = False
                record['error'] = traceback.format_exc()
            results.append(record)
    print(json.dumps({'kernel':name,'passed':sum(x['passed'] for x in results),'total':len(results),'cases':results},indent=2))
    return 0 if all(x['passed'] for x in results) else 1

if __name__=='__main__': raise SystemExit(main())
