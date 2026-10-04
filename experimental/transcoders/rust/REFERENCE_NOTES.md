# Independent implementation reference review

Reference: orjson 3.12.0, official commit 6737895a1a4e3e26df0569a40147893a786f9a58, retrieved 2026-10-04 from raw.githubusercontent.com/ijl/orjson. No reference code is incorporated.

Reviewed Cargo.toml, src/serialize/obtype.rs, src/ffi/pystrref/mod.rs, src/serialize/writer/byteswriter.rs, src/serialize/writer/num.rs, src/serialize/numpy/array.rs, src/serialize/numpy/item.rs, src/deserialize/input.rs. obtype.rs and NumPy array/item files declare Apache-2.0 OR MIT. pystrref/mod.rs, byteswriter.rs and writer/num.rs declare MPL-2.0. Package license is MPL-2.0 AND (Apache-2.0 OR MIT); the root MIT license does not override per-file licenses. MPL implementation was studied, not copied, translated or relicensed.

Dispatch: exact built-in type pointers cover the hot set, uncommon types/subclass flag checks occupy the slow path. Here trusted registered types precede built-ins (required SQL contract); PyO3 type checks and cached module state avoid arbitrary duck trust.

Unicode: reference PyStrRef borrows CPython representation; this implementation borrows PyString UTF-8 for strict JSON, but uses Python's UTF-8 surrogatepass codec as an owned snapshot for SQL, preserving lone surrogates safely. No CPython layout copying.

Writer: reference contiguous Python bytes buffer grows geometrically and uses unchecked pointer writes with capacity invariants. Here Rust Vec/String owns initialized storage; no writer unsafe, no borrowed buffer across Python callbacks, no GIL release. SQL and JSON use separate punctuation and escape writers.

Numbers: reference itoap writes integer digits and zmij shortest finite float spelling. Here Python arbitrary-precision int representation is accessed only as a numeric conversion API, Rust int format is used for native loops, and separately licensed zmij is used through its safe Buffer interface. JSON nonfinite floats become null, SQL becomes NULL. No serde_json, Python json, or old transcoder calls.

NumPy: reference checks C contiguous/native-endian flags, dispatches dtype/itemsize and uses typed loops. Here numeric data is copied via ndarray.tobytes(order='C') before owned dtype loops, supporting F order, negative/broadcast strides, swapped endian and unaligned input without naked pointers. Object/string items use NumPy indexing. The copy cost is explicit and performance optimization remains future work; SIMD is deferred.

Decode: reference delegates grammar to vendored yyjson; this implementation uses its own bounded recursive descent grammar with checked indexing, strict UTF-8, control/escape validation and surrogate pair assembly. No yyjson code/dependency.

Reviewed numeric dependency: zmij 1.0.21 crate Cargo.toml declares MIT, with its own LICENSE retained in the dependency source; public safe Buffer::format_finite is used without copying orjson's MPL writer glue. Python/NumPy scalar numeric conversion APIs preserve dtype-specific SQL scalar spelling; native numeric arrays and JSON use the safe formatter directly. The final implementation permits depth128 and rejects129, caps JSON at16MiB, and preserves the reference JSON C-contiguous/native-endian input domain while SQL arrays use safe owned snapshots for broader layouts. All four internal dispatch classifiers feed identical SQL handlers; no performance winner is claimed.

Additional deep review of src/ffi/pystrref/object.rs (MPL-2.0) confirmed exact Unicode identity checks, non-null borrowed object references, UTF-8 access through CPython, and optional inline representation/SIMD paths. This implementation deliberately uses PyO3's lifetime-bound Unicode borrow for JSON and base str.encode with owned surrogatepass bytes for SQL. No static lifetime transmutation, Send/Sync assertion, internal CPython layout or MPL source is reused. Runtime PyO3/ffi/once_cell/libc/zmij license texts are included as dependency attributions.
