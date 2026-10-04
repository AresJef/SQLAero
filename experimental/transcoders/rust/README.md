# Independent Rust SQLAero transcoder

Experimental Python-callable SQL escape/decode and own JSON encoder/parser. This implements the declared current transcoder type families, separately from production src, old SqlCyCli, cyTimes and the sibling Cython kernel. No legacy escape/decode, orjson, Python json, serde_json or yyjson forwarding occurs. PyO3 object/codec APIs and the MIT-licensed zmij numeric formatter are the runtime boundaries.

Public callable signatures and JSON namespace are documented in `sqlaero_rust_kernel.pyi`. `configure_types` is internal shared bridge integration. Trust precedence is ObjStr, RawText, SQLFunction, SQLInterval, then ordinary scalar/container conversion. Shared BIT/JSON class hooks belong to the coordinator's bridge and honor overrides. Their algorithms are not duplicated in this module.

## Build

Validated CPython 3.13.3/macOS arm64, Rust 1.99.0, PyO3 0.28.2, zmij 1.0.21 and maturin 1.15.0. Cargo.lock pins transitive dependencies. Build with a Python environment containing the desired runtime dependencies:

```sh
maturin build --release --locked -i /path/to/python3.13 --manifest-path Cargo.toml
python3.13 -m pip install target/wheels/sqlaero_rust_kernel-0.1.0-cp313-cp313-macosx_11_0_arm64.whl
```

This task installed development tools only under `/tmp/sqlaero-cargo`, `/tmp/sqlaero-rustup` and the existing temporary Python build environment. The release wheel is in `target/wheels/`; `sqlaero_rust_kernel.so` is an identical release-library copy for direct local test imports. Maturin-generated wheel loaders export the internal module state needed by the native functions and package typed stubs. `from sqlaero_rust_kernel.json import dumps, loads` works after wheel installation.

## Validation

`test_kernel.py` is independent compact correctness/safety validation, including 200 deterministic JSON round trips. Shared tests and comparison matrices are owned by the coordinator in `../tests`. See `SUPPORT_MATRIX.md` and the saved evidence for exact results. No database access or heavy benchmarking occurred. The three internal dispatch strategies returned by `_dispatch_strategies()` share handlers; `test_dispatch.py` validates each across the complete owned suite. See `DISPATCH_DESIGN.md` for the removed redundant classifier table and comparison limits. Speed comparison remains deferred.

## Safety and policy

SQL output assumes MySQL backslash escapes are enabled; it has no connection sql_mode/charset state parameter. Python codecs preserve SQL surrogateescape/surrogatepass transport. Numeric array input is copied in logical C order before native loops; SQL supports safe negative/broadcast strides, F order, endian swaps, unaligned and read-only arrays. JSON preserves the fixed reference's C-contiguous/native-endian acceptance domain. Object/string arrays use safe NumPy indexing. No raw array pointers, own unsafe blocks, callbacks over borrowed ndarray storage, or GIL release occur.

Depth 128 is permitted, 129 rejected. JSON document/output and array snapshots have a provisional 16 MiB budget; string writer length is checked before growth. These are finite resource policies, not a peak-RSS guarantee. Rust widened temporal arithmetic prevents i64 scaling overflow; datetime years are checked. Invalid numeric grammar, oversized decode integers and >8-byte BIT integer decoding reject. Valid zero dates retain text fallback. Nullable/Arrow pandas extensions, unsupported NumPy kinds/ranks/temporal units reject rather than acquire implicit semantics.

This is an experimental implementation with functional type coverage, not production release approval. Cross-platform wheels, extensive fuzzing/sanitizers, free-threaded/subinterpreter/abi3 guarantees, live database charset/sql_mode integration and performance selection remain separate validation work.

Frozen compatibility decision: NumPy temporal dtype multipliers (e.g. datetime64[3D]) are ignored, matching the original base-unit-only interpretation. Applying the multiplier would change visible normal output and is a separate documented correction proposal. Checked wide arithmetic and overflow rejection remain enforced.

JSON numeric ndarray encoding streams an owned raw snapshot directly through bounded dtype and shape writers; it does not build a per-cell SQL-row intermediate. SQL list/tuple subclasses normalize through their base constructors to honor explicit iterator overrides, while JSON uses base list storage and rejects tuple subclasses, matching the distinct frozen semantics.
