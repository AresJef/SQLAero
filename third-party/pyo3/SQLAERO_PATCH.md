# SQLAero local PyO3 patch

Upstream: PyO3 0.29.2, published crates.io archive.
Archive SHA-256: `4688ddedf473e32662b9b067670129a8afb8c18e351482c70d62ba4a88171e8b`. This matches the original Cargo.lock checksum.
Original MIT and Apache-2.0 licenses and upstream sources are retained.

## Scope and reason

Only `src/impl_/extract_argument.rs`, `DictVarkeywords::handle_varkeyword`,
is changed. Its existing Result boundary now checks PyDict_New and returns the
original Python error before collecting keyword arguments. Previously
PyDict::new panicked on NULL before SQLAeroError's constructor was entered.
The public constructor and argument-copying semantics remain unchanged.
There is no dependency version upgrade or platform-specific branch.

The repository's existing LF checkout policy also normalizes CRLF in four
upstream files (`src/call.rs`, `src/impl_/deprecated.rs`,
`src/pyclass/guard.rs`, `tests/test_class_init.rs`). This is line-ending-only;
the archive hash and per-file originals are retained in the task evidence.
Local final builds use that same canonical LF source as Actions and the sdist.

`SQLAERO_PATCH.diff` is the complete implementation difference from the fixed
archive. SQLAero's isolated public-constructor and driver-error regression tests
exercise this path in GIL and real free-threaded installations.

## Maintenance

For an intentional future PyO3 update, compare this exact upstream function,
check whether its allocation is already fallible, then rerun those regressions.
Remove the local patch only after the replacement has equivalent evidence.
Do not silently update the upstream snapshot or widen this patch.
