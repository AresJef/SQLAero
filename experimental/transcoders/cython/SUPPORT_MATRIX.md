# Implemented domain and validation status

Status meanings: **tested** means owned regression and/or frozen legacy group exercised the category, not exhaustive fuzz coverage; **implemented** means code exists with explicit dispatch, but focused vectors remain incomplete. Kernel is experimental and security/dispatch performance gates remain open.

## SQL escape

| Category | Implementation/shape | Evidence and remaining gaps |
|---|---|---|
| bool/int/float/None | Exact identity chain; arbitrary Python integer precision; finite float numeric APIs, nonfinite NULL | Legacy groups tested; edge decimal rounding exhaustive vectors pending |
| str | Underlying Unicode for exact str/np.str_; ordinary subclass str override is honored then quoted | Escapes, Unicode NUL and subclass override tested; codec/SQL mode integration pending |
| bytes/bytearray/memoryview | Owned bytes snapshot, ASCII surrogateescape, binary prefix | Legacy bytes + memoryview covered; no buffer persists across callbacks |
| Decimal | Exact type only; finite decimal string preserving scale | Legacy tested; Decimal subclass rejects |
| datetime/date/time/timedelta | Exact fast chain and known subclass cold chain; local fields and checked arbitrary precision duration | Legacy tested; timezone fields preserved; no normalization |
| struct_time | Scalar in all flags, datetime fields | Legacy tested after explicit sequence exclusion |
| list/tuple/set/frozenset/range/dict/views | Snapshots, recursive literals; dict values in order, item views converted to dict | Legacy tested; cycle/reentrant mutation tests; structural mixed-nesting policy differs from old first-character heuristic |
| np signed/unsigned integers | 8/16/32/64-bit accepted; dtype validation | Legacy array/scalar tested; >64-bit dtype rejects |
| np float16/32/64 | Scalar native digits; array float16 widened to float32; native shortest digits and exponent normalization | Legacy tested; focused float16/32 JSON parity; exhaustive IEEE bit patterns pending |
| np bool/str/bytes/record | Scalar native handlers; record as sequence | Legacy normal and focused record item-shape tested; NUL payload explicit tested |
| np datetime64 | ns/us/ms/s/m/h/D/W/M/Y; arbitrary precision epoch conversion, checked calendar; NaT NULL | All unit zero/NaT, negative ns and overflow tested |
| np timedelta64 | Same unit set; M/Y exact average-calendar constants; negative ns floor; NaT NULL | All unit NaT and M/Y/negative ns tested |
| ndarray 1D/2D | object/basic numeric/bool/S/U/calendar; exact ndarray only; safe indexed iteration | C/F/negative stride/transposed/zero stride/endian/unaligned covered; no pointer dereference |
| ndarray 0D/3D/complex/structured | Explicit reject | Rejection tested for 0D/3D/complex; structured branch implemented |
| pandas Timestamp/Timedelta | Known datetime/timedelta subclass path | Legacy calendar tested; nanos truncated to local microsecond fields |
| pandas Series/Index/DatetimeIndex/TimedeltaIndex | Known Series/Index subclasses -> values | Legacy tested; nullable/Arrow ExtensionArray unsupported |
| pandas DataFrame | Exact type, per-column values -> rows preserving dtype and column order | Legacy tested; duplicate columns preserved through items; extension dtypes may reject |
| cytimes Pydt | Known datetime subclass | Legacy tested through original fixtures |
| cytimes Pddt | Supported Index subclass -> owned ndarray handlers | Legacy ndarray datetime group exercises Pddt; explicit cytimes import is optional |
| ObjStr/CustomEscapeType/BIT/JSON | Explicit registered class tuple; invokes overriding str before ordinary subclass paths | Shared smoke and legacy custom tested; coordinator bridge owns stable classes |
| RawText/SQLFunction/SQLInterval | Explicit registered tuple; public value/args/expr/syntax properties; recursive parameters | Shared coordinator bridge suite pending final latest build run |

Shapes: scalar always str; flat items tuple or many list; nested many list of tuples; non-many nested tuple of row literals; 2D ndarray/DataFrame item output list of tuple. Empty containers have ()/tuple()/[] according to flags. Empty ndarray dimensions return literal () or [] for 2D items. Cyclic graphs reject; SQL depth above 128 rejects.

## Decode

Every boolean combination of use_decimal/decode_bit/decode_json has been covered with representative integer/BIT/JSON types; all full field combinations remain a larger shared-matrix gate. Production sqlaero error classes are used when importable; explicit standalone fallback exists otherwise.

| Field codes | Return and validation | Status |
|---|---|---|
| 1/2/3/8/9/13 | Fully consumed signed integer text; signed-min to unsigned-max 64-bit | Tested; illegal '-' and overflow reject DecodeValueError |
| 0/246 | Decimal if requested, float otherwise; complete decimal/exponent grammar and finite checks | Legacy tested; Decimal no float precision loss |
| 4/5 | Complete floating grammar, numeric APIs checked finite | Legacy and malformed/overflow tested |
| 7/12 | datetime; flexible 1-4 year/1-2 components, space/T, fraction<=6 | Legacy tested; invalid date fallback string |
| 10/14 | date; flexible components; invalid zero calendar fallback | Legacy tested |
| 11 | signed cross-day timedelta; minute/second bounds, fraction<=6; invalid fallback | Legacy tested; Python timedelta range overflow fallback |
| 16 | bytes default; checked <=8-byte big-endian int if requested | Flag tests; nine-byte decode intentionally rejects old silent truncation |
| 245 | connection-codec text, own loads if requested | Legacy/flag tests; binary flag ignored per contract |
| 247/248 | codec string regardless binary | Legacy tested |
| 15/249/250/251/252/253/254/255 and unknown | codec+surrogateescape string or bytes according binary | Legacy strings/fallback tested; geometry unchanged bytes/text |

Wrong input bytes type is TypeError at boundary. Malformed/overflow values map to public DecodeValueError with cause; unsupported operation/codec failure maps DecodeTypeError. Unknown field code normally falls back rather than rejects.

## JSON

| Category | Support | Evidence/gap |
|---|---|---|
| native null/bool/int/float/str/list/tuple/dict | Compact own writer; str keys only; signed-min/unsigned-max integer encoding; nonfinite floats null | Owned grammar/roundtrip/cycle tests; exponent exact focused parity |
| parsed integer beyond 64-bit | Convert to finite float, matching frozen baseline | Focused actual-orjson vectors pass |
| np float16/32/64 and integer/bool scalars | Native shortest digits; float16 cast float32; supported widths | Focused exact bytes parity pass |
| np datetime64 | Own epoch-calendar conversion then ISO local fields | Focused frozen parity pass |
| np ndarray | Contiguous native-endian numeric/calendar kinds; recursive native indexing | Ordinary 1D/2D tested; exotic JSON layout rejects; SQL layout broader |
| UUID/Enum/dataclass | UUID string, Enum value, dataclass public fields; recursion checked | UUID and public/private/cyclic dataclass tested; Enum implemented focused coverage pending |
| datetime/date/time | ISO strings; tz-aware datetime supported, tz-aware time rejects | Frozen focused parity; timezone subclass details pending |
| bytes/Decimal/set/custom/pandas/timedelta/np.timedelta64 | Reject | Explicit domain restrictions; focused special rejects |
| Unicode/UTF8 grammar | Controls escaped; strict UTF8 loads; escaped surrogate pairs supported, isolated surrogate rejects | Owned malformed and embedded NUL tests |
| duplicate keys/trailing/number grammar | Last key wins; full consumption; no NaN/Infinity tokens; overflow rejects | Owned tests |

Limits: depth 128; input UTF8 <=16MiB; UTF8 output <=16MiB checked before fragment accumulation and escaped string construction. Huge exponent tokens parse checked finite. Regex number parsing is positional, avoiding repeated suffix copies. Whole-document memory accounting and parser fuzz/sanitizer coverage remain open.

## Remaining acceptance gates

Latest shared bridge validation passed; larger JSON malformed/corpus fuzz and broader ASan/UBSan coverage remain, complete explicit subclass acceptance matrix, all supported ndarray storage combinations/dtype extremes, fair dispatch candidates and exclusive-window performance measurement. No production replacement, release, database access, GIL-release/thread-safety claim or speed winner.

## Frozen visible decisions

SQL datetime64/timedelta64 dtype metadata multipliers such as [3D] are intentionally ignored, matching old normal output. Thus tick1 datetime[3D] emits 1970-01-02 and duration[3D] emits24hours. This is a known semantic bug, not an overflow compatibility mode: arithmetic remains checked/arbitrary precision. Correctly multiplying metadata is a separate proposal for user decision. Own regression fixes the frozen behavior. JSON datetime multiplier behavior has no successful frozen result in the tiny isolated probe (process exited133 without pre-call checkpoint); this kernel controlled-rejects nonunit multipliers as an explicitly unverified frontier. No original multiplier JSON crash is re-executed in routine suites.

JSON subclasses follow frozen storage semantics: list/dict subclasses use base storage, integer subclasses use base PyLong representation, string subclasses use base Unicode contents; tuple/float/calendar/ndarray subclasses reject. SQL ordinary builtin subclass conversion remains distinct and preserves the old conversion hooks. Shared subclass matrix is independently maintained by coordinator.

Final evidence: normal shared15/15, type404/404, JSON58/58, float492/492, subclass20/20, legacy37/38 (only known oversizedBIT rejection); isolated ASan+UBSan30/30 small tests. Logs/matrix results and final hashes are retained in validation/. No performance measurement ran.
