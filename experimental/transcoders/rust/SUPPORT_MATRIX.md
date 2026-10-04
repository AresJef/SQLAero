# Rust implementation support and evidence

Functional declared families are implemented independently. Counts below distinguish normal comparison coverage from deliberately corrected unsafe behavior. Latest source includes additional subtype, base-storage, Unicode codec-hook and float64 scalar fixes; final post-fix evidence is recorded separately when its run completes.

| Family | Implemented behavior | Validation evidence |
| --- | --- | --- |
| None/bool/int/float/str/bytes/bytearray/memoryview | Own SQL writers; arbitrary Python int; nonfinite float NULL; lossless surrogate transport | Shared scalar vectors, 404-vector normal matrix, own transport/hook tests |
| Decimal | Exact SQL type; finite precision retained; decode Decimal never narrows through f64 | Legacy Decimal group, own 1e1000 decode and subclass rejection |
| datetime/date/time/timedelta/struct_time | Local fields; microseconds; signed multi-day duration; scalar flag precedence | Legacy date/time groups, own timezone and duration vectors |
| NumPy scalar numeric/string/bytes | Signed/unsigned widths; native scalar spelling; dtype float handling; NUL preservation | Normal matrix; deterministic 492-float matrix; own NUL vectors |
| NumPy temporal | ns/us/ms/s/m/h/D/W/M/Y; floor negative ns; wide checked calendar/duration calculations; average Gregorian month/year durations; NaT NULL | Legacy temporal groups; all-unit own vectors; 404-vector matrix |
| Python containers/dict views/np.record | Ordered snapshots; dict values; items->dict; all many/itemize shapes; cycle/depth checks | Legacy container groups; shared shapes; own np.record/base-storage tests |
| ndarray SQL | 1D/2D O/S/U and b/i/u/f/M/m; safe owned native dtype loops and item indexing | All numeric widths on C/F/transposed/reversed/step/broadcast/endian-swapped/unaligned/read-only layouts; empty ranks |
| pandas/cytimes | Timestamp/Timedelta/NaT; Index/Series values; exact DataFrame with ordered duplicate columns; Pydt/Pddt | Legacy groups; normal matrix; own mixed DataFrame, temporal Index and cytimes vectors |
| ObjStr/CustomEscapeType/BIT/JSON | Trusted shared bridge class registration and inherited overrides; own JSON core feeds shared JSON wrapper | Shared bridge class/hook/inheritance/error tests; own trusted expression vectors |
| RawText/SQLFunction/SQLInterval | Public value/args/expr/syntax; recursive own escaping; zero-argument syntax bypasses formatting | Shared bridge and own expression tests |
| decode | All current MySQL field codes; strict numbers; exact Decimal; date fallback; time; codec/binary; BIT big endian; own JSON | Legacy decode groups; shared checked numeric/field vectors; own BIT 0/8/9 and unknown 17/18/19 fallback |
| JSON ordinary tree | Strict UTF-8/escapes/surrogates/numbers; repeated keys last wins; complete consumption; own bounded recursive grammar; int64/uint64 dumps; finite loads; nonfinite dumps null | Shared grammar vectors; 58-vector fixed orjson matrix; 200 deterministic own round trips; malformed Unicode/number/depth tests |
| JSON rich types | Exact stdlib dates/time/datetime and UUID; Enum values; dataclass public fields; native C-contiguous NumPy numeric/datetime arrays; float32 shortest spelling | Rich own tests; fixed 58-vector JSON matrix; subtype boundary vectors |

Known rejected domains: arbitrary iterable/mapping/custom __str__ objects; SQL Decimal/ndarray/DataFrame subclasses; NumPy 0D/3D SQL arrays, complex/structured array kinds, plain void scalars; unsupported temporal units; pandas nullable/Arrow/NA semantics. JSON additionally rejects Decimal/bytes/sets/pandas containers, aware time, date/time/float/tuple/ndarray subclasses, NumPy string arrays/timedelta/NaT, non-string keys, large out-of-domain integers, cycles and invalid Unicode. This avoids silently expanding the original accepted domain.

Verified milestones: first six shared tests passed; then legacy 37/38 groups passed (the sole differing group expects >8-byte BIT truncation); wheel-installed shared bridge+contract 11/11 passed; type matrix 404/404; JSON matrix 58/58; installed own 11/11 safety/rich/dispatch groups passed. Final shared suite grew to 15 tests; the final float matrix, post-fix shared results, build hashes and wheel results are supplied in final evidence files. An earlier 488/492 float result identified four native NumPy float64 scalar spelling differences, which were corrected in the final source.

Remaining validation limits: no extensive fuzz campaign, sanitizer/Miri claim, production connection integration, platform matrix, abi3/free-threaded/subinterpreter guarantees, live database work or dispatch/performance winner. No declared family remains intentionally delegated to an old or third-party JSON/transcoder implementation.

Frozen compatibility decision: NumPy temporal dtype multipliers (e.g. datetime64[3D]) are ignored, matching the original base-unit-only interpretation. Applying the multiplier would change visible normal output and is a separate documented correction proposal. Checked wide arithmetic and overflow rejection remain enforced.

JSON temporal multiplier frontier: datetime64 dtype multipliers other than1 are rejected before conversion. The frozen baseline JSON behavior is unverified; no claim of parity is made and no repeat baseline probe is performed. SQL keeps the separately verified multiplier-ignoring normal behavior.

Final saved evidence: installed own11/11, shared15/15, normal type404/404, JSON58/58, float492/492, subclass20/20. Legacy37/38 differs only at deliberately rejected >8-byte BIT integer decoding. See evidence/ and artifact-metadata.json for stable hashes and timestamps.

JSON numeric ndarray encoding streams an owned raw snapshot directly through bounded dtype and shape writers; it does not build a per-cell SQL-row intermediate. SQL list/tuple subclasses normalize through their base constructors to honor explicit iterator overrides, while JSON uses base list storage and rejects tuple subclasses, matching the distinct frozen semantics.
