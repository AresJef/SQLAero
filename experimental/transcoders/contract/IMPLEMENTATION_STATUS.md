# Implementation acceptance checkpoint

Both independent Executor implementations are frozen locally. Cython owns `cython/`; Rust owns `rust/`; the coordinator owns shared integration, contracts and test matrices. Public production source remains unchanged. HEAD remains ecb3cf09e5abed9b2f231e1fd2e6126bd182c95b; experimental work is uncommitted. No push, release, database access or heavy performance run was performed.

Each worker reports type matrix404/404, JSON58/58, float492/492, subclass20/20 and legacy37/38. The sole legacy group difference is deliberate rejection of BIT input beyond8bytes. Rust own tests11/11. Cython isolated ASan+UBSan run30/30; leak checking was disabled for the unsanitized Python host. This is bounded validation, not an exhaustive safety guarantee.

Coordinator acceptance after both freezes and the latest bridge rebuild:21 shared tests pass for each kernel (including6 load-protocol mock tests), and decode matrix464/464 matches for each. The bridge preserves actual Cython extension-class inheritance and a newly compiled C API consumer; precompiled old binary ABI compatibility is not promised.

Frozen artifact hashes and worker evidence: `cython/validation/final-hashes.json`, `rust/artifact-metadata.json`. Known boundary decisions remain in DECISIONS.md, including checked BIT ranges, resource limits, temporal metadata and SQL-mode assumptions. Benchmark protocol is `../benchmarking/PROTOCOL.md`; adapter/orchestrator integration and supplied cyTimes protocol reconciliation remain pending. No performance winner has been selected.

Historical `*-initial` and `*-latest` evidence filenames may precede the frozen handoff. Final worker artifact manifests and coordinator command below identify this checkpoint; a stale filename does not supersede the command result.

Reproduce coordinator tests with `/tmp/sqlaero-pypi-release-verification/bin/python -B -m unittest discover -s experimental/transcoders/tests -p "test_*.py" -q`, cwd SQLAero, `PYTHONPATH=experimental/transcoders/cython:/tmp/sqlaero-rust-installed:/tmp/sqlaero-transcoder-bridge-lib:/tmp/sqlaero-transcoder-probe-lib`, and `SQLAERO_KERNEL` set independently to `sqlaero_cython_kernel` or `sqlaero_rust_kernel`. This isolated interpreter includes the frozen compiled production SQLAero and old orjson dependency; temporary build paths must be recreated on other hosts. Validation does not represent full-project final acceptance.
