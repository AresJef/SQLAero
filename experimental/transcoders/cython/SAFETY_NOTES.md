# Safety scope and final validation

The normal extension builds with clang -O3 through setuptools; the separate ASan+UBSan extension builds in `/tmp/sqlaero-cython-sanitized` with clang21, explicit existing macOS SDK, -O1, -g and frame pointers. `run_sanitizers.py` is reproducible using `/tmp/sqlaero-pypi-release-verification/bin/python -B run_sanitizers.py`. Its process-local ASAN_OPTIONS enables abort_on_error and disables leak detection; UBSAN_OPTIONS enables halt_on_error and stack traces. It verifies the imported module path before tests. Unsanitized CPython/dependency internals are outside the instrumentation claim; this is bounded extension validation, not exhaustive fuzz or whole-process leak proof.

Ownership review: Unicode C API reads use the true Unicode length and indices within it; caller objects are held as owned function arguments and the GIL remains held. Every entry to the Unicode writer follows type validation or a checked Python string-producing API. No raw ndarray pointer is dereferenced; indexing delegates to checked NumPy object APIs, respecting strides/endian/alignment. Bytes/memoryview are copied; containers are snapshotted before callbacks. No buffer or borrowed pointer persists over a callback, and there is no GIL release. Shape changes during object callbacks can raise safe Python indexing errors. Arbitrary-precision epoch/duration arithmetic precedes checked calendar construction, with explicit NaT handling. Integer decode and JSON numeric parsers consume complete grammar with bounds/finite checks. JSON strings calculate escaped UTF8 size before construction and fragments reserve output bytes before accumulation. Depth128 is allowed,129 rejected.

Normal subclass boundaries: SQL builtin/date/Series/Index subclasses use historical conversions; Decimal/ndarray/DataFrame subclasses reject. JSON list/dict/integer/string subclasses use base storage; tuple/float/calendar/ndarray subclasses reject. Trusted classes are explicitly registered and their intended hooks honored, including raw AssertionError propagation. Generic arbitrary __str__ is never an SQL trust hook.

Frozen semantic frontier: SQL datetime/timedelta dtype multipliers intentionally remain ignored to preserve normal original output. Correct metadata scaling is a proposed visible change. JSON nonunit datetime dtype multipliers controlled-reject until a safe expected specification is agreed.

A tiny old JSON multiplier probe was run once in an isolated process, using `/tmp/sqlaero-pypi-release-verification/bin/python -B` and this body:

```python
import numpy as np, orjson
for v in (np.datetime64(1, '3D'), np.array([1], dtype='datetime64[3D]')):
    try:
        print(type(v).__name__, orjson.dumps(v, option=orjson.OPT_SERIALIZE_NUMPY))
    except Exception as e:
        print(type(e).__name__, str(e))
```

Tool result: process exit133, no probe stdout (only shell startup messages), no pre-call checkpoint. Therefore the failing operation and cause are unlocalized; no orjson vulnerability is asserted. There is no separate historical crash log beyond that tool result, and the probe is not part of routine tests and will not be repeated.

Remaining gates: broader sanitizer corpora/fuzz, cross-platform builds and end-to-end charset/SQL-mode database verification (not authorized/run here), allocation accounting, independent dispatch-candidate benchmarks in the exclusive window. No speed or production-readiness claim.
