# SQLAero's jiter 0.17.0 patch

Source: the published `jiter` 0.17.0 crate from
https://github.com/pydantic/jiter/ . Its original MIT license is preserved in
`LICENSE`, and the original sources, platform-specific scanners and tests remain
in this directory. The project's run evidence records the upstream file hashes.
Cargo's root patch section selects this directory without upgrading the package.

The added `progress` module and strict checked string/key methods preserve the
streaming parser. They bound scanner, UTF-8-validation and controlled-copy work,
and return caller cancellation separately from syntax or allocation failure.
Only the Python binding supplies Python signal checks. The shared SQLAero Rust
core does not receive a Python callback or own Python values. Existing unmodified
entry points keep their original partial-input behavior; the new checked entry
points accept complete strict JSON only.

Object-start, object-separator and key-colon grammar are shared with the original
methods. The platform SIMD implementations and number parser are unchanged.
The byte budget counts down between callbacks, and ASCII escaped scalars are
written directly after fallible reservation. These shortcuts preserve the
cumulative checkpoint positions and the decoded string's proven ASCII status.
Allocator operations and CPython's internal dictionary hashing remain indivisible
calls; the callback is cooperative and does not promise a wall-clock deadline.

When updating this dependency, compare these local changes to the pinned upstream
version, preserve its license, and rerun the original parser regressions plus
`tests/progress.rs` and SQLAero's real signal/fetch tests. Do not replace this patch
with an outer check after a complete large string. Include this directory in the
source distribution and verify a build from that distribution.
